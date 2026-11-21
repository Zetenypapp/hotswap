package dev.hotswap.link;

import java.awt.image.BufferedImage;
import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Properties;
import java.util.Set;
import java.util.UUID;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicLong;
import javax.imageio.ImageIO;
import net.fabricmc.api.EnvType;
import net.fabricmc.fabric.api.event.lifecycle.v1.ServerLifecycleEvents;
import net.fabricmc.fabric.api.event.lifecycle.v1.ServerTickEvents;
import net.fabricmc.fabric.api.networking.v1.ServerPlayConnectionEvents;
import net.fabricmc.fabric.api.networking.v1.ServerPlayNetworking;
import net.fabricmc.loader.api.FabricLoader;
import net.minecraft.core.BlockPos;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.biome.Biome;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.level.material.MapColor;
import net.minecraft.world.level.storage.LevelResource;

/**
 * The server half, for dedicated Fabric servers. Everything is OFF until the admin turns it on in
 * config/hotswap-link.properties, and nothing is sent to players without the mod (vanilla players
 * join as always). With it on, players who have the mod can get:
 * - shared-map: the explored map, scanned around online players and kept in the world folder
 * - seed-preview: terrain previews from the server's own generator (the seed never leaves it)
 * - share-players: everyone's position
 */
final class ServerSide {
	private ServerSide() {}


	private static final int PER_TICK = 8;
	private static final int RESCAN = 200;
	private static final int RADIUS = 8;
	private static final int REGION = 512;
	private static final int MAX_TILES = 16384;

	private static volatile Net.Config cfg = Net.Config.UNKNOWN;
	private static MinecraftServer server;
	private static final Set<UUID> subs = ConcurrentHashMap.newKeySet();
	private static final Map<UUID, Long> sentRev = new ConcurrentHashMap<>();
	private static final Set<UUID> genBusy = ConcurrentHashMap.newKeySet();
	private static final Set<UUID> structBusy = ConcurrentHashMap.newKeySet();
	/** The whole config file, for the per-structure *_location switches. */
	private static volatile Properties props = new Properties();

	private static final AtomicLong rev = new AtomicLong();
	private record Stored(long rev, Net.Tile tile) {}
	private static final Map<String, Stored> tiles = new ConcurrentHashMap<>();
	// Explored pixels composited per region (ARGB, main thread only), most recently used last.
	private static final List<Map.Entry<String, int[]>> regions = new ArrayList<>();
	private static final Set<String> dirty = new HashSet<>();
	private static final Map<String, Integer> lastScan = new HashMap<>();
	private static int ticks;

	static void init() {
		ServerLifecycleEvents.SERVER_STARTING.register(s -> {
			if (FabricLoader.getInstance().getEnvironmentType() != EnvType.SERVER) return;
			server = s;
			cfg = loadConfig();
			HotSwapLinkCommon.LOG.info("HotSwap Link: {}", cfg);
		});
		ServerLifecycleEvents.SERVER_STOPPING.register(s -> saveDirty(true));
		ServerPlayConnectionEvents.DISCONNECT.register((handler, s) -> {
			subs.remove(handler.player.getUUID());
			sentRev.remove(handler.player.getUUID());
			genBusy.remove(handler.player.getUUID());
			structBusy.remove(handler.player.getUUID());
		});

		ServerPlayNetworking.registerGlobalReceiver(Net.Hello.TYPE, (p, ctx) -> {
			subs.add(ctx.player().getUUID());
			sentRev.put(ctx.player().getUUID(), rev.get());
			ServerPlayNetworking.send(ctx.player(), cfg);
		});
		ServerPlayNetworking.registerGlobalReceiver(Net.IndexReq.TYPE, (p, ctx) -> {
			if (shares() && server != null) ServerPlayNetworking.send(ctx.player(), new Net.Index(index()));
		});
		ServerPlayNetworking.registerGlobalReceiver(Net.RegionReq.TYPE, (p, ctx) -> {
			if (!shares() || server == null || Math.abs(p.x()) > 1 << 16 || Math.abs(p.z()) > 1 << 16) return;
			byte[] png = regionPng(p.dim(), p.x(), p.z());
			ServerPlayNetworking.send(ctx.player(), new Net.RegionData(p.dim(), p.x(), p.z(), png == null ? new byte[0] : png));
		});
		ServerPlayNetworking.registerGlobalReceiver(Net.GenReq.TYPE, (p, ctx) -> {
			ServerPlayer player = ctx.player();
			if (!cfg.mapAllowed() || !cfg.seedPreview() || server == null) {
				ServerPlayNetworking.send(player, new Net.GenData(p.id(), new byte[0], "Seed preview is off on this server"));
				return;
			}
			if (!genBusy.add(player.getUUID())) {
				ServerPlayNetworking.send(player, new Net.GenData(p.id(), new byte[0], "busy"));
				return;
			}
			// Generator runs on its own low-priority thread; the answer hops back to the server thread.
			Thread t = new Thread(() -> {
				byte[] rgb = null;
				String error = "";
				try {
					rgb = Generator.region(server, p.dim(), p.rx(), p.rz());
					if (rgb == null) error = "No preview for this dimension";
				} catch (Exception e) {
					error = "Preview failed";
					HotSwapLinkCommon.LOG.warn("Seed preview failed", e);
				}
				byte[] data = rgb == null ? new byte[0] : rgb;
				String err = error;
				server.execute(() -> {
					genBusy.remove(player.getUUID());
					if (!player.hasDisconnected()) ServerPlayNetworking.send(player, new Net.GenData(p.id(), data, err));
				});
			}, "HotSwap Link gen");
			t.setDaemon(true);
			t.start();
		});

		// Structures run on the server thread (that's where this handler is), one region per player at a time.
		ServerPlayNetworking.registerGlobalReceiver(Net.StructReq.TYPE, (p, ctx) -> {
			ServerPlayer player = ctx.player();
			if (server == null || !cfg.mapAllowed() || !located("overall")) {
				ServerPlayNetworking.send(player, new Net.StructData(p.id(), List.of(), "Locating structures is off on this server", ""));
				return;
			}
			if (Math.abs(p.rx()) > 1 << 16 || Math.abs(p.rz()) > 1 << 16 || !structBusy.add(player.getUUID())) {
				ServerPlayNetworking.send(player, new Net.StructData(p.id(), List.of(), "busy", ""));
				return;
			}
			try {
				List<Structures.Found> found = Structures.region(server, p.dim(), p.rx(), p.rz(), ServerSide::located);
				ServerPlayNetworking.send(player, new Net.StructData(p.id(), found == null ? List.of() : found, found == null ? "No such dimension" : "", Structures.fingerprint(server, switches())));
			} catch (Exception e) {
				HotSwapLinkCommon.LOG.warn("Locating structures failed", e);
				ServerPlayNetworking.send(player, new Net.StructData(p.id(), List.of(), "Locating failed", ""));
			} finally {
				structBusy.remove(player.getUUID());
			}
		});

		ServerTickEvents.END_SERVER_TICK.register(s -> {
			if (server == null || !shares()) return;
			ticks++;
			scan();
			if (ticks % 20 == 0) flush();
			if (ticks % 6000 == 0) saveDirty(false);
		});
	}

	private static boolean shares() {
		return cfg.mapAllowed() && cfg.sharedMap();
	}

	/** The *_location switches that are off, in a stable order (for the structures fingerprint). */
	private static String switches() {
		Properties p = props;
		return p.stringPropertyNames().stream().filter(k -> k.endsWith("_location") && "false".equalsIgnoreCase(p.getProperty(k).trim()))
			.sorted().collect(java.util.stream.Collectors.joining(","));
	}

	/** Whether a structure set (or "overall") may be located: on unless the config says false. */
	static boolean located(String set) {
		Properties p = props;
		java.util.function.Predicate<String> on = k -> !"false".equalsIgnoreCase(p.getProperty(k, "true").trim());
		return on.test("overall_location") && (set.equals("overall") || on.test(set + "_location"));
	}

	private static final String LOCATIONS = """
		# --- locating structures ---
		# overall_location: false hides every structure. Each other line hides one kind.
		# Modded structures follow overall_location.
		overall_location=true
		villages_location=true
		pillager_outposts_location=true
		woodland_mansions_location=true
		desert_pyramids_location=true
		jungle_temples_location=true
		swamp_huts_location=true
		igloos_location=true
		ocean_monuments_location=true
		ocean_ruins_location=true
		shipwrecks_location=true
		buried_treasures_location=true
		ruined_portals_location=true
		mineshafts_location=true
		trail_ruins_location=true
		trial_chambers_location=true
		ancient_cities_location=true
		abandoned_camp_location=true
		strongholds_location=true
		nether_complexes_location=true
		nether_fossils_location=true
		end_cities_location=true
		""";

	private static final String DEFAULTS = """
		# HotSwap Link: what players with the mod may see on HotSwap's map on this server.
		# Everything is on by default. Set any of these to false and restart the server.
		# Players without the mod aren't affected either way.

		# --- client permissions ---
		# is_map_allowed_at_all: false turns HotSwap's map off completely on this server.
		is_map_allowed_at_all=true
		# Which things the map may show around a player, when the player's game has them loaded:
		hostiles=true
		animals=true
		villagers=true
		players=true
		items=true

		# --- server features ---
		# shared_map: keep one explored map for everyone (in the world folder) and send it to players.
		shared_map=true
		# seed_preview: answer terrain previews from this world's generator. The seed itself is never sent.
		seed_preview=true
		""" + LOCATIONS;

	private static Net.Config loadConfig() {
		Path file = FabricLoader.getInstance().getConfigDir().resolve("hotswap.properties");
		Properties p = new Properties();
		try {
			if (!Files.exists(file)) {
				Files.createDirectories(file.getParent());
				Files.writeString(file, DEFAULTS);
			}
			try (var in = Files.newInputStream(file)) {
				p.load(in);
			}
			// A config from before structures existed: add their switches (on), keep everything else.
			if (!p.containsKey("overall_location")) {
				Files.writeString(file, LOCATIONS, java.nio.file.StandardOpenOption.APPEND);
				try (var in = Files.newInputStream(file)) {
					p.load(in);
				}
			}
		} catch (IOException e) {
			HotSwapLinkCommon.LOG.warn("Could not read hotswap.properties, using the defaults", e);
		}
		props = p;
		// Anything missing or misspelled counts as on, the default.
		java.util.function.Predicate<String> on = k -> !"false".equalsIgnoreCase(p.getProperty(k, "true").trim());
		return new Net.Config(on.test("is_map_allowed_at_all"), on.test("hostiles"), on.test("animals"), on.test("villagers"),
			on.test("players"), on.test("items"), on.test("shared_map"), on.test("seed_preview"));
	}

	/* ---------- scanning (same walk as the client, with map colours that work on a server) ---------- */

	private static void scan() {
		int done = 0;
		for (ServerLevel level : server.getAllLevels()) {
			if (level.players().isEmpty() || done >= PER_TICK) continue;
			String dim = level.dimension().identifier().toString();
			for (Player player : level.players()) {
				int pcx = player.getBlockX() >> 4, pcz = player.getBlockZ() >> 4;
				for (int r = 0; r <= RADIUS && done < PER_TICK; r++) {
					for (int dx = -r; dx <= r && done < PER_TICK; dx++) {
						for (int dz = -r; dz <= r && done < PER_TICK; dz++) {
							if (Math.max(Math.abs(dx), Math.abs(dz)) != r) continue;
							int cx = pcx + dx, cz = pcz + dz;
							String key = dim + "|" + cx + "|" + cz;
							Integer at = lastScan.get(key);
							if (at != null && ticks - at < RESCAN) continue;
							LevelChunk chunk = level.getChunkSource().getChunkNow(cx, cz);
							if (chunk == null || chunk.isEmpty()) continue;
							lastScan.put(key, ticks);
							done++;
							byte[] rgb = colours(level, chunk, cx, cz);
							Stored old = tiles.get(key);
							if (old != null && java.util.Arrays.equals(old.tile().rgb(), rgb)) continue;
							tiles.put(key, new Stored(rev.incrementAndGet(), new Net.Tile(dim, cx, cz, rgb)));
							composite(dim, cx, cz, rgb);
						}
					}
				}
			}
		}
		if (tiles.size() > MAX_TILES) prune();
		if (lastScan.size() > 40000) lastScan.clear();
	}

	private static void prune() {
		List<Long> revs = new ArrayList<>(tiles.size());
		for (Stored s : tiles.values()) revs.add(s.rev());
		revs.sort(null);
		long cut = revs.get(revs.size() / 4);
		tiles.values().removeIf(s -> s.rev() <= cut);
	}

	private static byte[] colours(ServerLevel level, LevelChunk chunk, int cx, int cz) {
		byte[] out = new byte[16 * 16 * 3];
		boolean ceiling = level.dimensionType().hasCeiling();
		int minY = level.getMinY();
		int x0 = cx << 4, z0 = cz << 4;
		BlockPos.MutableBlockPos pos = new BlockPos.MutableBlockPos();
		int[] prev = new int[16];
		LevelChunk north = level.getChunkSource().getChunkNow(cx, cz - 1);
		for (int x = 0; x < 16; x++) {
			prev[x] = north != null && !north.isEmpty() ? surface(level, north, x0 + x, z0 - 1, ceiling, minY, pos)[0] : Integer.MIN_VALUE;
		}
		for (int z = 0; z < 16; z++) {
			for (int x = 0; x < 16; x++) {
				int[] s = surface(level, chunk, x0 + x, z0 + z, ceiling, minY, pos);
				int y = s[0], rgb = s[1], depth = s[2];
				double f = depth > 0 ? Math.max(0.55, 1.0 - depth * 0.035)
					: Math.max(0.68, Math.min(1.14, 1.0 + (prev[x] == Integer.MIN_VALUE ? 0 : y - prev[x]) * 0.07));
				prev[x] = y;
				int i = (z * 16 + x) * 3;
				out[i] = (byte) Math.min(255, (int) (((rgb >> 16) & 255) * f));
				out[i + 1] = (byte) Math.min(255, (int) (((rgb >> 8) & 255) * f));
				out[i + 2] = (byte) Math.min(255, (int) ((rgb & 255) * f));
			}
		}
		return out;
	}

	private static int[] surface(ServerLevel level, LevelChunk chunk, int x, int z, boolean ceiling, int minY, BlockPos.MutableBlockPos pos) {
		int y;
		if (ceiling) {
			y = Math.min(level.getMaxY(), minY + 120);
			boolean air = false;
			for (; y > minY; y--) {
				pos.set(x, y, z);
				if (chunk.getBlockState(pos).isAir()) air = true;
				else if (air) break;
			}
		} else {
			y = chunk.getHeight(Heightmap.Types.WORLD_SURFACE, x, z);
		}
		BlockState state = null;
		for (; y > minY; y--) {
			pos.set(x, y, z);
			state = chunk.getBlockState(pos);
			if (state.getMapColor(level, pos) != MapColor.NONE) break;
		}
		if (state == null || y <= minY) return new int[] {minY, 0x0a0a0a, 0};
		MapColor mc = state.getMapColor(level, pos);
		int depth = 0;
		if (!state.getFluidState().isEmpty()) {
			BlockPos.MutableBlockPos below = new BlockPos.MutableBlockPos();
			for (int by = y - 1; by > minY && depth < 30; by--) {
				below.set(x, by, z);
				if (chunk.getBlockState(below).getFluidState().isEmpty()) break;
				depth++;
			}
			depth = Math.max(1, depth);
		}
		// A dedicated server has no colour textures loaded, so grass and leaves use the biome's
		// override when it has one and the vanilla map colours otherwise.
		Biome biome = level.getBiome(pos).value();
		int rgb;
		if (mc == MapColor.WATER) rgb = mix(biome.getSpecialEffects().waterColor(), 0x1a2a40, 0.35);
		else if (mc == MapColor.GRASS) rgb = biome.getSpecialEffects().grassColorOverride().orElse(0x7fb238);
		else if (mc == MapColor.PLANT) rgb = mix(biome.getSpecialEffects().foliageColorOverride().orElse(0x007c00), 0x000000, 0.1);
		else rgb = mc.col;
		return new int[] {y, rgb & 0xffffff, mc == MapColor.WATER ? depth : 0};
	}

	static int mix(int a, int b, double t) {
		int r = (int) (((a >> 16) & 255) * (1 - t) + ((b >> 16) & 255) * t);
		int g = (int) (((a >> 8) & 255) * (1 - t) + ((b >> 8) & 255) * t);
		int bl = (int) ((a & 255) * (1 - t) + (b & 255) * t);
		return (r << 16) | (g << 8) | bl;
	}

	/* ---------- the kept map: region PNGs in the world folder ---------- */

	private static Path mapDir() {
		return server.getWorldPath(LevelResource.ROOT).resolve("data").resolve("hotswap-link");
	}

	private static String safe(String s) {
		StringBuilder b = new StringBuilder();
		for (char c : s.toCharArray()) b.append(Character.isLetterOrDigit(c) || c == '.' || c == '-' || c == '_' ? c : '_');
		return b.isEmpty() ? "_" : b.substring(0, Math.min(b.length(), 80));
	}

	private static Path dimDir(String dim) {
		Path dir = mapDir().resolve(safe(dim));
		try {
			if (!Files.exists(dir)) {
				Files.createDirectories(dir);
				Files.writeString(dir.resolve("dim.txt"), dim);
			}
		} catch (IOException ignored) {
		}
		return dir;
	}

	private static int[] region(String dim, int rx, int rz) {
		String key = safe(dim) + "|" + rx + "|" + rz;
		for (int i = 0; i < regions.size(); i++) {
			if (regions.get(i).getKey().equals(key)) {
				var e = regions.remove(i);
				regions.add(e);
				return e.getValue();
			}
		}
		int[] px = new int[REGION * REGION];
		Path file = dimDir(dim).resolve("r." + rx + "." + rz + ".png");
		if (Files.exists(file)) {
			try {
				BufferedImage img = ImageIO.read(file.toFile());
				if (img != null && img.getWidth() == REGION && img.getHeight() == REGION) img.getRGB(0, 0, REGION, REGION, px, 0, REGION);
			} catch (IOException ignored) {
			}
		}
		regions.add(Map.entry(key, px));
		if (regions.size() > 16) {
			var old = regions.remove(0);
			if (dirty.remove(old.getKey())) savePng(old.getKey(), old.getValue());
		}
		return px;
	}

	private static void composite(String dim, int cx, int cz, byte[] rgb) {
		int rx = cx >> 5, rz = cz >> 5;
		int[] px = region(dim, rx, rz);
		int ox = (cx & 31) * 16, oz = (cz & 31) * 16;
		for (int z = 0; z < 16; z++) {
			for (int x = 0; x < 16; x++) {
				int j = (z * 16 + x) * 3;
				px[(oz + z) * REGION + ox + x] = 0xff000000 | ((rgb[j] & 255) << 16) | ((rgb[j + 1] & 255) << 8) | (rgb[j + 2] & 255);
			}
		}
		dirty.add(safe(dim) + "|" + rx + "|" + rz);
	}

	private static void savePng(String key, int[] px) {
		String[] parts = key.split("\\|");
		BufferedImage img = new BufferedImage(REGION, REGION, BufferedImage.TYPE_INT_ARGB);
		img.setRGB(0, 0, REGION, REGION, px, 0, REGION);
		try {
			Path dir = mapDir().resolve(parts[0]);
			Files.createDirectories(dir);
			ImageIO.write(img, "png", dir.resolve("r." + parts[1] + "." + parts[2] + ".png").toFile());
		} catch (IOException e) {
			HotSwapLinkCommon.LOG.warn("Could not save map region {}", key, e);
		}
	}

	private static void saveDirty(boolean all) {
		for (var e : regions) {
			if (dirty.remove(e.getKey())) savePng(e.getKey(), e.getValue());
		}
		if (all) dirty.clear();
	}

	private static List<Net.RegionKey> index() {
		saveDirty(false);
		List<Net.RegionKey> out = new ArrayList<>();
		try (var dims = Files.list(mapDir())) {
			for (Path d : dims.toList()) {
				if (!Files.isDirectory(d)) continue;
				String dim;
				try {
					dim = Files.readString(d.resolve("dim.txt")).trim();
				} catch (IOException e) {
					continue;
				}
				try (var files = Files.list(d)) {
					for (Path f : files.toList()) {
						String[] parts = f.getFileName().toString().split("\\.");
						if (parts.length == 4 && parts[0].equals("r") && parts[3].equals("png")) {
							try {
								out.add(new Net.RegionKey(dim, Integer.parseInt(parts[1]), Integer.parseInt(parts[2])));
							} catch (NumberFormatException ignored) {
							}
						}
						if (out.size() >= 8192) return out;
					}
				}
			}
		} catch (IOException ignored) {
		}
		return out;
	}

	private static byte[] regionPng(String dim, int rx, int rz) {
		String key = safe(dim) + "|" + rx + "|" + rz;
		if (dirty.contains(key)) {
			saveDirty(false);
		}
		Path file = dimDir(dim).resolve("r." + rx + "." + rz + ".png");
		try {
			return Files.exists(file) ? Files.readAllBytes(file) : null;
		} catch (IOException e) {
			return null;
		}
	}

	/* ---------- streaming to players with the mod ---------- */

	private static void flush() {
		List<Stored> fresh = null;
		for (ServerPlayer player : server.getPlayerList().getPlayers()) {
			Long since = subs.contains(player.getUUID()) ? sentRev.get(player.getUUID()) : null;
			if (since == null) continue;
			if (fresh == null) {
				fresh = new ArrayList<>(tiles.values());
				fresh.sort((a, b) -> Long.compare(a.rev(), b.rev()));
			}
			List<Net.Tile> batch = new ArrayList<>();
			long last = since;
			for (Stored s : fresh) {
				if (s.rev() > since && batch.size() < 48) {
					batch.add(s.tile());
					last = Math.max(last, s.rev());
				}
			}
			if (!batch.isEmpty()) {
				sentRev.put(player.getUUID(), last);
				ServerPlayNetworking.send(player, new Net.Tiles(batch));
			}
			if (cfg.mapAllowed() && cfg.players()) {
				List<Net.PlayerPos> all = new ArrayList<>();
				for (ServerPlayer other : server.getPlayerList().getPlayers()) {
					all.add(new Net.PlayerPos(other.getGameProfile().name(), other.level().dimension().identifier().toString(),
						other.getX(), other.getY(), other.getZ(), other.getYRot()));
				}
				ServerPlayNetworking.send(player, new Net.Players(all));
			}
		}
	}
}
