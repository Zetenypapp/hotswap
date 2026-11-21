package dev.hotswap.link;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.TimeUnit;
import java.util.function.Predicate;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.StructureManager;
import net.minecraft.world.level.chunk.ChunkGeneratorStructureState;
import net.minecraft.world.level.levelgen.structure.StructureCheckResult;
import net.minecraft.world.level.levelgen.structure.StructureSet;
import net.minecraft.world.level.levelgen.structure.placement.ConcentricRingsStructurePlacement;
import net.minecraft.world.level.levelgen.structure.placement.RandomSpreadStructurePlacement;
import net.minecraft.world.level.levelgen.structure.placement.StructurePlacement;

/**
 * Where structures are in a 512 x 512 block region, the way /locate finds them: the world's own
 * placement grid from the seed, then the same terrain and biome check the game runs before it
 * places one. Nothing is generated or saved. Runs on the server thread (the game's structure
 * checks aren't thread-safe), one region at a time.
 */
final class Structures {
	private Structures() {}

	/**
	 * One structure: its kind ("end_city"), its set ("end_cities", for permissions) and where.
	 * pieces/height: how big its planned layout is (0 when not worked out). tag: what's special
	 * about it ("ship" on an End city, "treasure" on a bastion, a shipwreck's variant...), or "".
	 */
	record Found(String type, String set, int x, int z, int pieces, int height, String tag) {}

	/** Sets whose layout is worth planning (size, ship, kind). Others only get a position. */
	private static final java.util.Set<String> DETAILED = java.util.Set.of("end_cities", "nether_complexes", "shipwrecks", "ruined_portals", "villages",
		"ancient_cities", "trial_chambers", "woodland_mansions", "pillager_outposts");

	static final int REGION = 512;

	/**
	 * Changes when the answers could: a different seed (world reset) or different switches.
	 * HotSwap keeps saved structures only while this matches. One-way: the seed can't be read back.
	 */
	static String fingerprint(MinecraftServer server, String switches) {
		try {
			byte[] h = java.security.MessageDigest.getInstance("SHA-256")
				.digest(("hotswap-link|" + server.overworld().getSeed() + "|" + switches).getBytes(java.nio.charset.StandardCharsets.UTF_8));
			return java.util.HexFormat.of().formatHex(h, 0, 8);
		} catch (java.security.NoSuchAlgorithmException e) {
			return "";
		}
	}

	/**
	 * The structures in region (rx, rz) of dimension {@code dim} whose set {@code allowed} accepts.
	 * Returns null when the dimension doesn't exist. Call from any thread.
	 */
	static List<Found> region(MinecraftServer server, String dim, int rx, int rz, Predicate<String> allowed) throws Exception {
		Identifier id = Identifier.tryParse(dim);
		if (id == null) return null;
		ResourceKey<Level> key = ResourceKey.create(Registries.DIMENSION, id);
		if (server.isSameThread()) return find(server.getLevel(key), rx, rz, allowed);
		return server.submit(() -> find(server.getLevel(key), rx, rz, allowed)).get(30, TimeUnit.SECONDS);
	}

	private static List<Found> find(ServerLevel level, int rx, int rz, Predicate<String> allowed) {
		if (level == null) return null;
		ChunkGeneratorStructureState state = level.getChunkSource().getGeneratorState();
		StructureManager structures = level.structureManager();
		int c0x = rx * 32, c0z = rz * 32, c1x = c0x + 31, c1z = c0z + 31;
		List<Found> out = new ArrayList<>();
		for (Holder<StructureSet> set : state.possibleStructureSets()) {
			String setName = set.unwrapKey().map(k -> k.identifier().getPath()).orElse("");
			if (!allowed.test(setName)) continue;
			StructurePlacement placement = set.value().placement();
			List<ChunkPos> candidates = new ArrayList<>();
			if (placement instanceof RandomSpreadStructurePlacement spread) {
				// One possible spot per grid cell: only the cells overlapping this region.
				int s = spread.spacing();
				for (int gx = Math.floorDiv(c0x, s); gx <= Math.floorDiv(c1x, s); gx++) {
					for (int gz = Math.floorDiv(c0z, s); gz <= Math.floorDiv(c1z, s); gz++) {
						ChunkPos p = spread.getPotentialStructureChunk(state.getLevelSeed(), gx * s, gz * s);
						if (p.x() < c0x || p.x() > c1x || p.z() < c0z || p.z() > c1z) continue;
						if (spread.isStructureChunk(state, p.x(), p.z())) candidates.add(p);
					}
				}
			} else if (placement instanceof ConcentricRingsStructurePlacement rings) {
				// Strongholds: a fixed list of spots for the whole world.
				List<ChunkPos> all = state.getRingPositionsFor(rings);
				if (all != null) {
					for (ChunkPos p : all) {
						if (p.x() >= c0x && p.x() <= c1x && p.z() >= c0z && p.z() <= c1z) candidates.add(p);
					}
				}
			}
			for (ChunkPos p : candidates) {
				// Which of the set's structures fits there (a village set has one per biome).
				for (StructureSet.StructureSelectionEntry entry : set.value().structures()) {
					StructureCheckResult r = structures.checkStructurePresence(p, entry.structure().value(), placement, false);
					if (r == StructureCheckResult.START_NOT_PRESENT) continue;
					String type = entry.structure().unwrapKey().map(k -> k.identifier().getPath()).orElse(setName);
					out.add(DETAILED.contains(setName) ? plan(level, entry.structure(), p, type, setName)
						: new Found(type, setName, p.getMiddleBlockX(), p.getMiddleBlockZ(), 0, 0, ""));
					break;
				}
			}
		}
		return out;
	}

	/**
	 * Plans the structure the way world generation does before placing it (the layout only,
	 * nothing is placed or saved), and reads its size and what's special about it.
	 */
	private static Found plan(ServerLevel level, Holder<net.minecraft.world.level.levelgen.structure.Structure> structure, ChunkPos p, String type, String set) {
		Found plain = new Found(type, set, p.getMiddleBlockX(), p.getMiddleBlockZ(), 0, 0, "");
		try {
			var chunks = level.getChunkSource();
			var gen = chunks.getGenerator();
			var start = structure.value().generate(structure, level.dimension(), level.registryAccess(), gen, gen.getBiomeSource(),
				chunks.randomState().createClimateSampler(net.minecraft.world.level.levelgen.densityfunction.SamplerContext.EMPTY_UNCACHED),
				chunks.randomState(), level.getStructureTemplateManager(), chunks.getGeneratorState().getLevelSeed(), p, 0, level,
				structure.value().biomes()::contains);
			if (!start.isValid()) return plain;
			var ctx = net.minecraft.world.level.levelgen.structure.pieces.StructurePieceSerializationContext.fromLevel(level);
			// The piece names (templates and jigsaw elements) say what was built.
			StringBuilder names = new StringBuilder();
			for (var piece : start.getPieces()) names.append(piece.createTag(ctx)).append('\n');
			String all = names.toString();
			String tag = switch (set) {
				case "end_cities" -> all.contains("end_city/ship") ? "ship" : "";
				case "nether_complexes" -> all.contains("bastion/treasure") ? "treasure" : all.contains("hoglin_stable") ? "stables"
					: all.contains("bastion/bridge") ? "bridge" : all.contains("bastion/units") ? "housing" : "";
				case "shipwrecks" -> all.contains("upsidedown") ? "upside down" : all.contains("sideways") ? "on its side"
					: all.contains("_degraded") ? "broken" : "upright";
				case "ruined_portals" -> all.contains("giant_portal") ? "giant" : "";
				default -> "";
			};
			var box = start.getBoundingBox();
			int center = (box.minX() + box.maxX()) / 2, middle = (box.minZ() + box.maxZ()) / 2;
			return new Found(type, set, center, middle, start.getPieces().size(), box.maxY() - box.minY() + 1, tag);
		} catch (Exception e) {
			return plain;
		}
	}
}
