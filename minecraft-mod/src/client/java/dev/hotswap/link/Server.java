package dev.hotswap.link;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import com.sun.net.httpserver.HttpExchange;
import com.sun.net.httpserver.HttpServer;
import java.io.IOException;
import java.io.OutputStream;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.security.MessageDigest;
import java.security.SecureRandom;
import java.util.Base64;
import java.util.HashMap;
import java.util.HexFormat;
import java.util.Map;
import java.util.concurrent.Executors;
import net.minecraft.client.Minecraft;
import net.minecraft.client.server.IntegratedServer;

/**
 * The local connection HotSwap reads. Listens on 127.0.0.1 only, on a random port, and answers
 * only requests with this session's token (written to ~/.hotswap/minecraft-link.json, which only
 * your Windows account can read). Browser requests are refused, so web pages can't reach it.
 *
 * GET /status                         your position, dimension and world
 * GET /chunks?since=REV               chunk colours that changed after REV
 * GET /generate?dim=D&rx=X&rz=Z       single-player: a 512 x 512 block terrain preview
 */
final class Server {
	static final int PROTOCOL = 1;
	private static final int MAX_CHUNKS = 400;

	private final Scanner scanner;
	private final String token = HexFormat.of().formatHex(new SecureRandom().generateSeed(24));
	private final Path discovery = Path.of(System.getProperty("user.home"), ".hotswap", "minecraft-link.json");
	private HttpServer http;
	private int port;

	Server(Scanner scanner) {
		this.scanner = scanner;
	}

	void start() throws IOException {
		http = HttpServer.create(new InetSocketAddress(InetAddress.getLoopbackAddress(), 0), 8);
		http.setExecutor(Executors.newFixedThreadPool(2, r -> {
			Thread t = new Thread(r, "HotSwap Link");
			t.setDaemon(true);
			return t;
		}));
		http.createContext("/", this::handle);
		http.start();
		port = http.getAddress().getPort();
		Files.createDirectories(discovery.getParent());
		Path tmp = discovery.resolveSibling("minecraft-link.json.tmp");
		JsonObject o = new JsonObject();
		o.addProperty("protocol", PROTOCOL);
		o.addProperty("port", port);
		o.addProperty("token", token);
		o.addProperty("pid", ProcessHandle.current().pid());
		o.addProperty("version", HotSwapLink.VERSION);
		Files.writeString(tmp, o.toString(), StandardCharsets.UTF_8);
		Files.move(tmp, discovery, StandardCopyOption.REPLACE_EXISTING, StandardCopyOption.ATOMIC_MOVE);
		HotSwapLink.LOG.info("HotSwap Link {} listening on 127.0.0.1:{}", HotSwapLink.VERSION, port);
	}

	void stop() {
		if (http != null) http.stop(0);
		// Only remove the file if it is still ours (another Minecraft may have started since).
		try {
			if (Files.readString(discovery).contains(token)) Files.delete(discovery);
		} catch (IOException ignored) {
		}
	}

	private void handle(HttpExchange ex) throws IOException {
		try (ex) {
			String host = ex.getRequestHeaders().getFirst("Host");
			String auth = ex.getRequestHeaders().getFirst("Authorization");
			if (ex.getRequestHeaders().containsKey("Origin") || !("127.0.0.1:" + port).equals(host)) {
				reply(ex, 403, error("Refused"));
				return;
			}
			if (auth == null || !MessageDigest.isEqual(auth.getBytes(StandardCharsets.UTF_8), ("Bearer " + token).getBytes(StandardCharsets.UTF_8))) {
				reply(ex, 401, error("Bad token"));
				return;
			}
			if (!"GET".equals(ex.getRequestMethod())) {
				reply(ex, 405, error("GET only"));
				return;
			}
			Map<String, String> q = query(ex.getRequestURI().getRawQuery());
			switch (ex.getRequestURI().getPath()) {
				case "/status" -> reply(ex, 200, status());
				case "/chunks" -> reply(ex, 200, chunks(num(q.get("since"), 0)));
				case "/generate" -> generate(ex, q);
				default -> reply(ex, 404, error("Not found"));
			}
		} catch (Exception e) {
			HotSwapLink.LOG.warn("HotSwap Link request failed", e);
		}
	}

	/** The snapshot plus the slower reads: your stats, the world around you, who else is online. */
	private static JsonObject status() {
		JsonObject o = HotSwapLink.snapshot().json();
		Net.Config r = HotSwapLink.rules();
		if (r != null && !r.mapAllowed()) {
			o.addProperty("blocked", true);
			return o;
		}
		if (r != null) {
			JsonObject allow = new JsonObject();
			allow.addProperty("hostile", r.hostiles());
			allow.addProperty("passive", r.animals());
			allow.addProperty("villager", r.villagers());
			allow.addProperty("player", r.players());
			allow.addProperty("item", r.items());
			allow.addProperty("other", r.hostiles() && r.animals());
			o.add("allowed", allow);
		}
		if (o.has("inWorld") && o.get("inWorld").getAsBoolean()) {
			JsonObject extras = HotSwapLink.extras();
			if (extras != null) for (var e : extras.entrySet()) o.add(e.getKey(), e.getValue());
			JsonArray entities = HotSwapLink.entities();
			if (entities != null) o.add("entities", entities);
			JsonArray players = ClientNet.playersJson();
			if (players != null) o.add("players", players);
		}
		return o;
	}

	private JsonObject chunks(long since) {
		JsonObject o = new JsonObject();
		o.addProperty("world", scanner.world());
		// A different world (or a restarted game) starts the count again.
		long head = scanner.rev();
		if (since > head) since = 0;
		var list = scanner.since(since, MAX_CHUNKS);
		JsonArray arr = new JsonArray();
		long last = since;
		for (Scanner.Tile t : list) {
			JsonObject c = new JsonObject();
			c.addProperty("d", t.dim());
			c.addProperty("x", t.x());
			c.addProperty("z", t.z());
			c.addProperty("c", Base64.getEncoder().encodeToString(t.rgb()));
			arr.add(c);
			last = t.rev();
		}
		o.addProperty("rev", list.size() == MAX_CHUNKS ? last : Math.max(last, head));
		o.addProperty("more", list.size() == MAX_CHUNKS);
		o.add("chunks", arr);
		return o;
	}

	private void generate(HttpExchange ex, Map<String, String> q) throws Exception {
		IntegratedServer sp = Minecraft.getInstance().getSingleplayerServer();
		String dim = q.getOrDefault("dim", "");
		byte[] rgb;
		if (sp != null) {
			rgb = Generator.region(sp, dim, (int) num(q.get("rx"), 0), (int) num(q.get("rz"), 0));
		} else if (ClientNet.serverSeed()) {
			// The server works it out with its own seed and sends back just the picture.
			rgb = ClientNet.generate(dim, (int) num(q.get("rx"), 0), (int) num(q.get("rz"), 0));
		} else {
			reply(ex, 409, error("Only in single-player, or on a server that turned previews on"));
			return;
		}
		if (rgb == null) {
			reply(ex, 409, error("No preview for this dimension"));
			return;
		}
		JsonObject o = new JsonObject();
		o.addProperty("size", Generator.SIZE);
		o.addProperty("step", Generator.STEP);
		o.addProperty("c", Base64.getEncoder().encodeToString(rgb));
		reply(ex, 200, o);
	}

	private static JsonObject error(String message) {
		JsonObject o = new JsonObject();
		o.addProperty("error", message);
		return o;
	}

	private static void reply(HttpExchange ex, int status, JsonObject body) throws IOException {
		byte[] b = body.toString().getBytes(StandardCharsets.UTF_8);
		ex.getResponseHeaders().set("Content-Type", "application/json");
		ex.getResponseHeaders().set("Cache-Control", "no-store");
		ex.sendResponseHeaders(status, b.length);
		try (OutputStream os = ex.getResponseBody()) {
			os.write(b);
		}
	}

	private static Map<String, String> query(String raw) {
		Map<String, String> m = new HashMap<>();
		if (raw == null) return m;
		for (String part : raw.split("&")) {
			int i = part.indexOf('=');
			if (i > 0) m.put(java.net.URLDecoder.decode(part.substring(0, i), StandardCharsets.UTF_8), java.net.URLDecoder.decode(part.substring(i + 1), StandardCharsets.UTF_8));
		}
		return m;
	}

	private static long num(String s, long fallback) {
		try {
			return s == null ? fallback : Long.parseLong(s);
		} catch (NumberFormatException e) {
			return fallback;
		}
	}
}
