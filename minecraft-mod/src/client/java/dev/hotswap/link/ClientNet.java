package dev.hotswap.link;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import java.awt.image.BufferedImage;
import java.io.ByteArrayInputStream;
import java.util.ArrayDeque;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicInteger;
import javax.imageio.ImageIO;
import net.fabricmc.fabric.api.client.networking.v1.ClientPlayConnectionEvents;
import net.fabricmc.fabric.api.client.networking.v1.ClientPlayNetworking;
import net.minecraft.client.Minecraft;

/**
 * The client's side of talking to a server that also has HotSwap Link. Everything here is
 * optional: on a vanilla server (or one with the features off) none of it runs and the mod
 * behaves exactly as before.
 */
final class ClientNet {
	private ClientNet() {}

	private static Scanner scanner;
	private static volatile Net.Config cfg = null;
	private static volatile List<Net.PlayerPos> players = null;
	private static final AtomicInteger genIds = new AtomicInteger();
	private static final Map<Integer, CompletableFuture<byte[]>> gens = new ConcurrentHashMap<>();
	// Explored regions the server offered and we haven't fetched yet (one a second).
	private static final ArrayDeque<Net.RegionKey> queue = new ArrayDeque<>();
	private static final Set<String> fetched = new HashSet<>();
	private static int ticks;

	private static final ExecutorService DECODE = Executors.newSingleThreadExecutor(r -> {
		Thread t = new Thread(r, "HotSwap Link region decode");
		t.setDaemon(true);
		return t;
	});

	static void init(Scanner s) {
		scanner = s;
		ClientPlayConnectionEvents.JOIN.register((handler, sender, mc) -> {
			reset();
			if (ClientPlayNetworking.canSend(Net.Hello.TYPE)) ClientPlayNetworking.send(new Net.Hello());
		});
		ClientPlayConnectionEvents.DISCONNECT.register((handler, mc) -> reset());

		ClientPlayNetworking.registerGlobalReceiver(Net.Config.TYPE, (p, ctx) -> {
			cfg = p;
			HotSwapLink.LOG.info("Server has HotSwap Link: {}", p);
			if (p.mapAllowed() && p.sharedMap() && ClientPlayNetworking.canSend(Net.IndexReq.TYPE)) ClientPlayNetworking.send(new Net.IndexReq());
		});
		ClientPlayNetworking.registerGlobalReceiver(Net.Tiles.TYPE, (p, ctx) -> {
			for (Net.Tile t : p.tiles()) {
				if (t.rgb().length == 16 * 16 * 3) scanner.inject(t.dim(), t.x(), t.z(), t.rgb());
			}
		});
		ClientPlayNetworking.registerGlobalReceiver(Net.Index.TYPE, (p, ctx) -> {
			synchronized (queue) {
				for (Net.RegionKey k : p.regions()) {
					if (fetched.add(k.dim() + "|" + k.x() + "|" + k.z())) queue.add(k);
				}
			}
		});
		// PNG decoding is off the game thread; the tiles drop into the same store as our own scan.
		ClientPlayNetworking.registerGlobalReceiver(Net.RegionData.TYPE, (p, ctx) -> {
			if (p.png().length == 0 || p.png().length > 4 << 20) return;
			DECODE.execute(() -> inject(p));
		});
		ClientPlayNetworking.registerGlobalReceiver(Net.GenData.TYPE, (p, ctx) -> {
			CompletableFuture<byte[]> f = gens.remove(p.id());
			if (f == null) return;
			if (!p.error().isEmpty()) f.completeExceptionally(new RuntimeException(p.error()));
			else f.complete(p.rgb());
		});
		ClientPlayNetworking.registerGlobalReceiver(Net.Players.TYPE, (p, ctx) -> {
			String me = ctx.client().player != null ? ctx.client().player.getGameProfile().name() : "";
			players = p.players().stream().filter(x -> !x.name().equals(me)).toList();
		});
	}

	private static void reset() {
		cfg = null;
		players = null;
		synchronized (queue) {
			queue.clear();
			fetched.clear();
		}
		gens.values().forEach(f -> f.completeExceptionally(new RuntimeException("Left the server")));
		gens.clear();
	}

	/** Called every client tick (from the game thread). */
	static void tick(Minecraft mc) {
		Net.Config c = cfg;
		if (c == null || !c.mapAllowed() || !c.sharedMap()) return;
		if (++ticks % 20 != 0) return;
		Net.RegionKey k;
		synchronized (queue) {
			k = queue.poll();
		}
		if (k != null && ClientPlayNetworking.canSend(Net.RegionReq.TYPE)) ClientPlayNetworking.send(new Net.RegionReq(k.dim(), k.x(), k.z()));
	}

	private static void inject(Net.RegionData p) {
		try {
			BufferedImage img = ImageIO.read(new ByteArrayInputStream(p.png()));
			if (img == null || img.getWidth() != 512 || img.getHeight() != 512) return;
			int[] px = new int[512 * 512];
			img.getRGB(0, 0, 512, 512, px, 0, 512);
			// Only fully explored chunks: a half-seen chunk would stamp black over the real map.
			for (int cz = 0; cz < 32; cz++) {
				chunk:
				for (int cx = 0; cx < 32; cx++) {
					byte[] rgb = new byte[16 * 16 * 3];
					for (int z = 0; z < 16; z++) {
						for (int x = 0; x < 16; x++) {
							int v = px[(cz * 16 + z) * 512 + cx * 16 + x];
							if ((v >>> 24) != 255) continue chunk;
							int i = (z * 16 + x) * 3;
							rgb[i] = (byte) (v >> 16);
							rgb[i + 1] = (byte) (v >> 8);
							rgb[i + 2] = (byte) v;
						}
					}
					scanner.inject(p.dim(), p.x() * 32 + cx, p.z() * 32 + cz, rgb);
				}
			}
		} catch (Exception e) {
			HotSwapLink.LOG.warn("Could not read a shared map region", e);
		}
	}

	/** Whether the server will answer seed previews. */
	static boolean serverSeed() {
		Net.Config c = cfg;
		return c != null && c.mapAllowed() && c.seedPreview();
	}

	/** What the map may show here: all in single-player, the server's rules on a server with the mod, little otherwise. */
	static Net.Config rules(Minecraft mc) {
		if (mc.getSingleplayerServer() != null) return Net.Config.ALL;
		Net.Config c = cfg;
		return c != null ? c : Net.Config.UNKNOWN;
	}

	/** Asks the server for a seed preview (called from HotSwap's connection thread). */
	static byte[] generate(String dim, int rx, int rz) throws Exception {
		if (!serverSeed()) throw new RuntimeException("Seed preview is off on this server");
		int id = genIds.incrementAndGet();
		CompletableFuture<byte[]> f = new CompletableFuture<>();
		gens.put(id, f);
		Minecraft.getInstance().execute(() -> {
			if (ClientPlayNetworking.canSend(Net.GenReq.TYPE)) ClientPlayNetworking.send(new Net.GenReq(id, dim, rx, rz));
			else f.completeExceptionally(new RuntimeException("Not connected"));
		});
		try {
			byte[] rgb = f.get(120, TimeUnit.SECONDS);
			if (rgb.length == 0) throw new RuntimeException("Preview failed");
			return rgb;
		} finally {
			gens.remove(id);
		}
	}

	/** Everyone else online, when the server shares that; otherwise null. */
	static JsonArray playersJson() {
		List<Net.PlayerPos> list = players;
		Net.Config c = cfg;
		if (list == null || c == null || !c.mapAllowed() || !c.players()) return null;
		JsonArray arr = new JsonArray();
		for (Net.PlayerPos p : list) {
			JsonObject o = new JsonObject();
			o.addProperty("name", p.name());
			o.addProperty("dim", p.dim());
			o.addProperty("x", p.x());
			o.addProperty("y", p.y());
			o.addProperty("z", p.z());
			o.addProperty("yaw", p.yaw());
			arr.add(o);
		}
		return arr;
	}
}
