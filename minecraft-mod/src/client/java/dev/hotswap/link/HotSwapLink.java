package dev.hotswap.link;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientLifecycleEvents;
import net.fabricmc.fabric.api.client.event.lifecycle.v1.ClientTickEvents;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ServerData;
import net.minecraft.client.server.IntegratedServer;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.entity.monster.Enemy;
import net.minecraft.world.entity.npc.villager.AbstractVillager;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.storage.LevelResource;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/**
 * HotSwap Link: lets HotSwap draw your Minecraft world on its map.
 *
 * Shares, on 127.0.0.1 only and behind a per-session token: your own position, direction and
 * dimension; the colours of the chunks your game has loaded around you; and in single-player a
 * terrain preview from the world's own generator. Never other players, entities or chat.
 */
public final class HotSwapLink implements ClientModInitializer {
	public static final String VERSION = "1.4.1";
	public static final Logger LOG = LoggerFactory.getLogger("HotSwap Link");

	/** What HotSwap reads with GET /status, refreshed every client tick. */
	public record Snapshot(boolean inWorld, String world, String worldName, String dim, double x, double y, double z, float yaw, boolean canGenerate) {
		static final Snapshot MENU = new Snapshot(false, "", "", "", 0, 0, 0, 0, false);

		JsonObject json() {
			JsonObject o = new JsonObject();
			o.addProperty("protocol", Server.PROTOCOL);
			o.addProperty("version", VERSION);
			o.addProperty("inWorld", inWorld);
			if (inWorld) {
				o.addProperty("world", world);
				o.addProperty("worldName", worldName);
				o.addProperty("dim", dim);
				o.addProperty("x", x);
				o.addProperty("y", y);
				o.addProperty("z", z);
				o.addProperty("yaw", yaw);
				o.addProperty("canGenerate", canGenerate);
			}
			return o;
		}
	}

	private static volatile Snapshot snapshot = Snapshot.MENU;
	/** The rules where you play now (see ClientNet.rules); null in the menu. */
	private static volatile Net.Config rules = null;

	public static Net.Config rules() {
		return rules;
	}
	/** The slower-changing reads: your health, the clock, the weather, your biome, where you died. */
	private static volatile JsonObject extras = null;
	/** What's around you: mobs, animals, villagers, dropped items, nearby players. */
	private static volatile JsonArray entities = null;
	private final Scanner scanner = new Scanner();
	private Server server;

	public static Snapshot snapshot() {
		return snapshot;
	}

	public static JsonObject extras() {
		return extras;
	}

	public static JsonArray entities() {
		return entities;
	}

	/** Bumped every client tick; the live stream waits on it. */
	private static final Object TICK = new Object();
	private static long tickSeq;
	/** Bumped when the mobs list is read again. */
	private static volatile long entitySeq;

	public static long entitySeq() {
		return entitySeq;
	}

	/** Waits (up to timeoutMs) for the tick after {@code seen}; returns the new tick number. */
	public static long awaitTick(long seen, long timeoutMs) throws InterruptedException {
		synchronized (TICK) {
			long until = System.currentTimeMillis() + timeoutMs;
			while (tickSeq == seen) {
				long left = until - System.currentTimeMillis();
				if (left <= 0) break;
				TICK.wait(left);
			}
			return tickSeq;
		}
	}

	private static void ticked() {
		synchronized (TICK) {
			tickSeq++;
			TICK.notifyAll();
		}
	}

	@Override
	public void onInitializeClient() {
		try {
			server = new Server(scanner);
			server.start();
		} catch (Exception e) {
			LOG.error("HotSwap Link could not start its local connection", e);
			return;
		}
		ClientNet.init(scanner);
		ClientTickEvents.END_CLIENT_TICK.register(this::tick);
		ClientLifecycleEvents.CLIENT_STOPPING.register(mc -> {
			server.stop();
			Generator.shutdown();
		});
	}

	private int ticks;

	private void tick(Minecraft mc) {
		if (mc.level == null || mc.player == null) {
			if (snapshot.inWorld()) {
				snapshot = Snapshot.MENU;
				extras = null;
				entities = null;
				scanner.reset("");
			}
			rules = null;
			return;
		}
		String[] id = worldId(mc);
		String dim = mc.level.dimension().identifier().toString();
		rules = ClientNet.rules(mc);
		// A server that says no: nothing at all goes to HotSwap, and what was scanned is dropped.
		if (!rules.mapAllowed()) {
			if (snapshot.inWorld()) {
				snapshot = Snapshot.MENU;
				extras = null;
				entities = null;
				scanner.reset("");
			}
			return;
		}
		if (!id[0].equals(snapshot.world())) scanner.reset(id[0]);
		IntegratedServer sp = mc.getSingleplayerServer();
		snapshot = new Snapshot(true, id[0], id[1], dim, mc.player.getX(), mc.player.getY(), mc.player.getZ(), mc.player.getYRot(),
			sp != null ? Generator.supports(sp, mc.level.dimension()) : ClientNet.serverSeed());
		scanner.tick(mc.level, mc.player, dim);
		ClientNet.tick(mc);
		// Mobs 10 times a second (each has an id, so HotSwap can slide them smoothly); the rest twice.
		if (ticks % 2 == 0) {
			entities = readEntities(mc);
			entitySeq++;
		}
		if (ticks % 10 == 0) extras = readExtras(mc);
		ticks++;
		ticked();
	}

	/** Your own state and the world's, straight from what the client already knows. */
	private static JsonObject readExtras(Minecraft mc) {
		JsonObject o = new JsonObject();
		o.addProperty("health", Math.round(mc.player.getHealth() * 10) / 10.0);
		o.addProperty("maxHealth", mc.player.getMaxHealth());
		o.addProperty("food", mc.player.getFoodData().getFoodLevel());
		o.addProperty("armor", mc.player.getArmorValue());
		o.addProperty("xp", mc.player.experienceLevel);
		long clock = mc.level.getOverworldClockTime();
		o.addProperty("day", clock / 24000);
		o.addProperty("time", clock % 24000);
		o.addProperty("raining", mc.level.isRaining());
		o.addProperty("thundering", mc.level.isThundering());
		o.addProperty("biome", mc.level.getBiome(mc.player.blockPosition())
			.unwrapKey().map(k -> k.identifier().getPath().replace('_', ' ')).orElse(""));
		mc.player.getLastDeathLocation().ifPresent(d -> {
			JsonObject death = new JsonObject();
			death.addProperty("dim", d.dimension().identifier().toString());
			death.addProperty("x", d.pos().getX());
			death.addProperty("y", d.pos().getY());
			death.addProperty("z", d.pos().getZ());
			o.add("death", death);
		});
		return o;
	}

	/** Everything living (and dropped items) the client has loaded around you. */
	private static JsonArray readEntities(Minecraft mc) {
		JsonArray arr = new JsonArray();
		Net.Config r = rules;
		if (r == null) return arr;
		for (Entity e : mc.level.entitiesForRendering()) {
			if (e == mc.player) continue;
			if (arr.size() >= 400) break;
			String kind;
			String name = null;
			if (e instanceof Player p) {
				kind = "player";
				name = p.getGameProfile().name();
			} else if (e instanceof Enemy) kind = "hostile";
			else if (e instanceof AbstractVillager) kind = "villager";
			else if (e instanceof net.minecraft.world.entity.animal.Animal) kind = "passive";
			else if (e instanceof ItemEntity) kind = "item";
			else if (e instanceof LivingEntity) kind = "other";
			else continue;
			// Only what this server allows (everything in single-player).
			boolean allowed = switch (kind) {
				case "player" -> r.players();
				case "hostile" -> r.hostiles();
				case "villager" -> r.villagers();
				case "passive" -> r.animals();
				case "item" -> r.items();
				default -> r.hostiles() && r.animals();
			};
			if (!allowed) continue;
			JsonObject o = new JsonObject();
			o.addProperty("i", e.getId());
			o.addProperty("k", kind);
			o.addProperty("n", name != null ? name : e.getType().getDescription().getString());
			o.addProperty("x", Math.round(e.getX() * 10) / 10.0);
			o.addProperty("y", Math.round(e.getY() * 10) / 10.0);
			o.addProperty("z", Math.round(e.getZ() * 10) / 10.0);
			arr.add(o);
		}
		return arr;
	}

	/** A stable id for the world (save folder or server address) and a name to show. */
	private static String[] worldId(Minecraft mc) {
		IntegratedServer sp = mc.getSingleplayerServer();
		if (sp != null) {
			String folder = String.valueOf(sp.getWorldPath(LevelResource.ROOT).toAbsolutePath().normalize().getFileName());
			return new String[] {"sp:" + folder, sp.getWorldData().getLevelName()};
		}
		ServerData server = mc.getCurrentServer();
		if (server != null) return new String[] {"mp:" + server.ip.toLowerCase(java.util.Locale.ROOT), server.name};
		return new String[] {"mp:unknown", "Multiplayer"};
	}
}
