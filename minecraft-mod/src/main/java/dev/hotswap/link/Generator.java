package dev.hotswap.link;

import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;
import java.util.concurrent.TimeUnit;
import net.minecraft.core.Holder;
import net.minecraft.core.QuartPos;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.biome.Biome;
import net.minecraft.world.level.biome.BiomeResolver;
import net.minecraft.world.level.chunk.ChunkGenerator;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.level.levelgen.NoiseBasedChunkGenerator;
import net.minecraft.world.level.levelgen.RandomState;

/**
 * A terrain preview of a 512 x 512 block region, one pixel per 4 x 4 blocks, worked out with the
 * single-player world's own generator and seed (land height and biome only: nothing is generated
 * or saved in the world). Runs on one low-priority thread so the game keeps its speed.
 */
final class Generator {
	static final int REGION = 512, STEP = 4, SIZE = REGION / STEP;

	private static final ExecutorService POOL = Executors.newSingleThreadExecutor(r -> {
		Thread t = new Thread(r, "HotSwap Link preview");
		t.setDaemon(true);
		t.setPriority(Thread.MIN_PRIORITY);
		return t;
	});

	static void shutdown() {
		POOL.shutdownNow();
	}

	static boolean supports(MinecraftServer server, ResourceKey<Level> dim) {
		ServerLevel level = server.getLevel(dim);
		// Noise-based worlds without a roof: the Overworld and the End (and similar modded ones).
		return level != null && level.getChunkSource().getGenerator() instanceof NoiseBasedChunkGenerator && !level.dimensionType().hasCeiling();
	}

	/** RGB, SIZE x SIZE, row by row from the region's north-west corner; null if not possible here. */
	static byte[] region(MinecraftServer server, String dim, int rx, int rz) throws Exception {
		Identifier id = Identifier.tryParse(dim);
		if (id == null) return null;
		ResourceKey<Level> key = ResourceKey.create(Registries.DIMENSION, id);
		if (!supports(server, key)) return null;
		ServerLevel level = server.getLevel(key);
		Future<byte[]> job = POOL.submit(() -> draw(level, rx, rz));
		return job.get(120, TimeUnit.SECONDS);
	}

	private static byte[] draw(ServerLevel level, int rx, int rz) {
		ChunkGenerator gen = level.getChunkSource().getGenerator();
		RandomState rs = level.getChunkSource().randomState();
		BiomeResolver biomes = gen.getBiomeSource().createCachingResolver(rs);
		int sea = gen.getSeaLevel();
		byte[] out = new byte[SIZE * SIZE * 3];
		int[] prev = new int[SIZE];
		java.util.Arrays.fill(prev, Integer.MIN_VALUE);
		for (int pz = 0; pz < SIZE; pz++) {
			for (int px = 0; px < SIZE; px++) {
				int bx = rx * REGION + px * STEP + STEP / 2, bz = rz * REGION + pz * STEP + STEP / 2;
				int h = gen.getBaseHeight(bx, bz, Heightmap.Types.OCEAN_FLOOR_WG, level, rs);
				Holder<Biome> biome = biomes.getNoiseBiome(QuartPos.fromBlock(bx), QuartPos.fromBlock(Math.max(h, sea)), QuartPos.fromBlock(bz));
				int rgb;
				double f;
				if (h < sea && !name(biome).contains("end")) {
					rgb = mix(biome.value().getWaterColor(), 0x1a2a40, 0.35);
					f = Math.max(0.5, 1.0 - (sea - h) * 0.02);
				} else {
					rgb = land(biome, bx, bz, h);
					int d = prev[px] == Integer.MIN_VALUE ? 0 : h - prev[px];
					f = Math.max(0.7, Math.min(1.15, 1.0 + d * 0.03));
				}
				prev[px] = h;
				int i = (pz * SIZE + px) * 3;
				out[i] = (byte) Math.min(255, (int) (((rgb >> 16) & 255) * f));
				out[i + 1] = (byte) Math.min(255, (int) (((rgb >> 8) & 255) * f));
				out[i + 2] = (byte) Math.min(255, (int) ((rgb & 255) * f));
			}
		}
		return out;
	}

	static int mix(int a, int b, double t) {
		int r = (int) (((a >> 16) & 255) * (1 - t) + ((b >> 16) & 255) * t);
		int g = (int) (((a >> 8) & 255) * (1 - t) + ((b >> 8) & 255) * t);
		int bl = (int) ((a & 255) * (1 - t) + (b & 255) * t);
		return (r << 16) | (g << 8) | bl;
	}

	private static String name(Holder<Biome> biome) {
		return biome.unwrapKey().map(k -> k.identifier().getPath()).orElse("");
	}

	/** Roughly what the ground looks like from above in this biome. */
	private static int land(Holder<Biome> biome, int x, int z, int h) {
		String n = name(biome);
		if (n.contains("end")) return 0xdbdca0;
		if (n.contains("snowy") || n.contains("frozen") || n.contains("ice") || n.contains("grove") || n.equals("jagged_peaks")) return 0xeef4f8;
		if (n.contains("desert") || n.contains("beach")) return 0xdbd3a0;
		if (n.contains("badlands")) return 0xbf6a3a;
		if (n.contains("mushroom")) return 0x8a7086;
		if (n.contains("stony") || n.contains("peaks") || n.contains("gravelly")) return 0x8f8f8f;
		if (n.contains("cherry")) return 0xe6a8c8;
		if (n.contains("pale_garden")) return 0x9aa08f;
		if (n.contains("forest") || n.contains("jungle") || n.contains("taiga") || n.contains("swamp") || n.contains("dark")) {
			return mix(biome.value().getSpecialEffects().foliageColorOverride().orElse(0x2e6b2e), 0x000000, 0.22);
		}
		int grass = biome.value().getSpecialEffects().grassColorOverride().orElse(0x79ab51);
		return h > 190 ? mix(grass, 0x8f8f8f, 0.5) : grass;
	}
}
