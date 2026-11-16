package dev.hotswap.link;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicLong;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.client.renderer.BiomeColors;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.level.material.MapColor;

/**
 * Turns the chunks the game has loaded around you into 16 x 16 top-down colours, a few chunks per
 * tick, nearest first. Only what your own client already has; nothing is loaded or requested.
 */
final class Scanner {
	/** A chunk's colours (16 x 16 RGB, row by row from the north-west corner) and when they changed. */
	record Tile(long rev, String dim, int x, int z, byte[] rgb) {}

	private static final int PER_TICK = 6;
	/** Ticks before a loaded chunk is looked at again (blocks change). */
	private static final int RESCAN = 100;
	private static final int MAX_RADIUS = 16;

	private final AtomicLong rev = new AtomicLong();
	/** The latest colours of every chunk seen in this world, for HotSwap to fetch what it lacks. */
	private final Map<String, Tile> tiles = new ConcurrentHashMap<>();
	private final Map<Long, Integer> lastScan = new HashMap<>();
	private final Map<Long, Integer> lastHash = new HashMap<>();
	private volatile String world = "";
	private String dim = "";
	private int ticks;

	synchronized void reset(String world) {
		this.world = world;
		tiles.clear();
		lastScan.clear();
		lastHash.clear();
	}

	String world() {
		return world;
	}

	/** Tiles newer than {@code since}, oldest first, at most {@code max}. */
	List<Tile> since(long since, int max) {
		List<Tile> out = new ArrayList<>();
		for (Tile t : tiles.values()) if (t.rev() > since) out.add(t);
		out.sort((a, b) -> Long.compare(a.rev(), b.rev()));
		return out.size() > max ? out.subList(0, max) : out;
	}

	long rev() {
		return rev.get();
	}

	/** A tile from somewhere else (the server's shared map); same store as our own scan. */
	void inject(String dim, int x, int z, byte[] rgb) {
		if (world.isEmpty()) return;
		String key = dim + "|" + x + "|" + z;
		Tile old = tiles.get(key);
		if (old != null && java.util.Arrays.equals(old.rgb(), rgb)) return;
		tiles.put(key, new Tile(rev.incrementAndGet(), dim, x, z, rgb));
		if (tiles.size() > 24000) prune();
	}

	/** HotSwap drains tiles every second; when far behind a big shared map, drop the oldest. */
	private synchronized void prune() {
		if (tiles.size() <= 24000) return;
		java.util.List<Long> revs = new java.util.ArrayList<>(tiles.size());
		for (Tile t : tiles.values()) revs.add(t.rev());
		revs.sort(null);
		long cut = revs.get(revs.size() / 4);
		tiles.values().removeIf(t -> t.rev() <= cut);
	}

	synchronized void tick(ClientLevel level, LocalPlayer player, String dim) {
		ticks++;
		if (!dim.equals(this.dim)) {
			this.dim = dim;
			lastScan.clear();
			lastHash.clear();
		}
		int pcx = player.getBlockX() >> 4, pcz = player.getBlockZ() >> 4;
		int radius = Math.min(MAX_RADIUS, net.minecraft.client.Minecraft.getInstance().options.renderDistance().get());
		int done = 0;
		// Square rings outwards from the player's chunk.
		for (int r = 0; r <= radius && done < PER_TICK; r++) {
			for (int dx = -r; dx <= r && done < PER_TICK; dx++) {
				for (int dz = -r; dz <= r && done < PER_TICK; dz++) {
					if (Math.max(Math.abs(dx), Math.abs(dz)) != r) continue;
					int cx = pcx + dx, cz = pcz + dz;
					long key = ((long) cx << 32) ^ (cz & 0xffffffffL);
					Integer at = lastScan.get(key);
					if (at != null && ticks - at < RESCAN) continue;
					LevelChunk chunk = level.getChunkSource().getChunk(cx, cz, ChunkStatus.FULL, false);
					if (chunk == null || chunk.isEmpty()) continue;
					lastScan.put(key, ticks);
					done++;
					byte[] rgb = colours(level, chunk, cx, cz);
					int hash = java.util.Arrays.hashCode(rgb);
					if (Integer.valueOf(hash).equals(lastHash.put(key, hash))) continue;
					tiles.put(dim + "|" + cx + "|" + cz, new Tile(rev.incrementAndGet(), dim, cx, cz, rgb));
				}
			}
		}
		// Forget far-away bookkeeping now and then (the tiles stay for HotSwap).
		if (ticks % 1200 == 0 && lastScan.size() > 20000) {
			lastScan.clear();
			lastHash.clear();
		}
	}

	private static byte[] colours(ClientLevel level, LevelChunk chunk, int cx, int cz) {
		byte[] out = new byte[16 * 16 * 3];
		boolean ceiling = level.dimensionType().hasCeiling();
		int minY = level.getMinY();
		int x0 = cx << 4, z0 = cz << 4;
		BlockPos.MutableBlockPos pos = new BlockPos.MutableBlockPos();
		// The height of the row north of this chunk, for shading the first row.
		int[] prev = new int[16];
		LevelChunk north = level.getChunkSource().getChunk(cx, cz - 1, ChunkStatus.FULL, false);
		for (int x = 0; x < 16; x++) {
			prev[x] = north != null && !north.isEmpty() ? surface(level, north, x0 + x, z0 - 1, ceiling, minY, pos)[0] : Integer.MIN_VALUE;
		}
		for (int z = 0; z < 16; z++) {
			for (int x = 0; x < 16; x++) {
				int[] s = surface(level, chunk, x0 + x, z0 + z, ceiling, minY, pos);
				int y = s[0], rgb = s[1], depth = s[2];
				double f;
				if (depth > 0) {
					// Water: darker the deeper it is.
					f = Math.max(0.55, 1.0 - depth * 0.035);
				} else {
					int d = prev[x] == Integer.MIN_VALUE ? 0 : y - prev[x];
					f = Math.max(0.68, Math.min(1.14, 1.0 + d * 0.07));
				}
				prev[x] = y;
				int i = (z * 16 + x) * 3;
				out[i] = (byte) Math.min(255, (int) (((rgb >> 16) & 255) * f));
				out[i + 1] = (byte) Math.min(255, (int) (((rgb >> 8) & 255) * f));
				out[i + 2] = (byte) Math.min(255, (int) ((rgb & 255) * f));
			}
		}
		return out;
	}

	/** {y, rgb, water depth} of the top visible block of a column. */
	private static int[] surface(ClientLevel level, LevelChunk chunk, int x, int z, boolean ceiling, int minY, BlockPos.MutableBlockPos pos) {
		int y;
		if (ceiling) {
			// The Nether's roof is solid: start below it and look for the first floor under open air.
			y = Math.min(level.getMaxY(), minY + 120);
			boolean air = false;
			for (; y > minY; y--) {
				pos.set(x, y, z);
				boolean empty = chunk.getBlockState(pos).isAir();
				if (empty) air = true;
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
			int top = y;
			BlockPos.MutableBlockPos below = new BlockPos.MutableBlockPos();
			for (int by = y - 1; by > minY && depth < 30; by--) {
				below.set(x, by, z);
				if (chunk.getBlockState(below).getFluidState().isEmpty()) break;
				depth++;
			}
			depth = Math.max(1, depth);
			y = top;
		}
		int rgb;
		if (mc == MapColor.WATER) rgb = mix(BiomeColors.getAverageWaterColor(level, pos), 0x1a2a40, 0.35);
		else if (mc == MapColor.GRASS) rgb = BiomeColors.getAverageGrassColor(level, pos);
		else if (mc == MapColor.PLANT) rgb = mix(BiomeColors.getAverageFoliageColor(level, pos), 0x000000, 0.18);
		else rgb = mc.col;
		return new int[] {y, rgb & 0xffffff, mc == MapColor.WATER ? depth : 0};
	}

	static int mix(int a, int b, double t) {
		int r = (int) (((a >> 16) & 255) * (1 - t) + ((b >> 16) & 255) * t);
		int g = (int) (((a >> 8) & 255) * (1 - t) + ((b >> 8) & 255) * t);
		int bl = (int) ((a & 255) * (1 - t) + (b & 255) * t);
		return (r << 16) | (g << 8) | bl;
	}
}
