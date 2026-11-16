package dev.hotswap.link;

import java.util.ArrayList;
import java.util.List;
import net.fabricmc.fabric.api.networking.v1.PayloadTypeRegistry;
import net.minecraft.network.FriendlyByteBuf;
import net.minecraft.network.codec.StreamCodec;
import net.minecraft.network.protocol.common.custom.CustomPacketPayload;
import net.minecraft.resources.Identifier;

/**
 * What the client and server halves of HotSwap Link say to each other, over Minecraft's own
 * connection (so it works through any port forwarding, and vanilla players are unaffected).
 * Everything the server sends is gated by its config; everything starts with the client's Hello.
 */
public final class Net {
	private Net() {}

	private static <T extends CustomPacketPayload> CustomPacketPayload.Type<T> channel(String path) {
		return new CustomPacketPayload.Type<>(Identifier.fromNamespaceAndPath("hotswap-link", path));
	}

	/** One chunk's top-down colours: 16 x 16 RGB, row by row from the north-west corner. */
	public record Tile(String dim, int x, int z, byte[] rgb) {}

	/** Client -> server: "I have the mod, send me what the config allows." */
	public record Hello() implements CustomPacketPayload {
		public static final Type<Hello> TYPE = channel("hello");
		public static final StreamCodec<FriendlyByteBuf, Hello> CODEC = CustomPacketPayload.codec((v, b) -> {}, b -> new Hello());
		@Override public Type<Hello> type() { return TYPE; }
	}

	/**
	 * Server -> client: what this server allows (config/hotswap.properties).
	 * mapAllowed: the HotSwap map at all. hostiles, animals, villagers, players, items: which of
	 * those the client may show. sharedMap, seedPreview: what the server itself provides.
	 */
	public record Config(boolean mapAllowed, boolean hostiles, boolean animals, boolean villagers, boolean players, boolean items,
		boolean sharedMap, boolean seedPreview) implements CustomPacketPayload {
		public static final Type<Config> TYPE = channel("rules");
		public static final StreamCodec<FriendlyByteBuf, Config> CODEC = CustomPacketPayload.codec(
			(v, b) -> {
				b.writeBoolean(v.mapAllowed); b.writeBoolean(v.hostiles); b.writeBoolean(v.animals); b.writeBoolean(v.villagers);
				b.writeBoolean(v.players); b.writeBoolean(v.items); b.writeBoolean(v.sharedMap); b.writeBoolean(v.seedPreview);
			},
			b -> new Config(b.readBoolean(), b.readBoolean(), b.readBoolean(), b.readBoolean(), b.readBoolean(), b.readBoolean(), b.readBoolean(), b.readBoolean()));
		@Override public Type<Config> type() { return TYPE; }

		/** Single-player: your own world, everything allowed. */
		public static final Config ALL = new Config(true, true, true, true, true, true, true, true);
		/** A server without HotSwap Link: your own map, but nothing it couldn't say yes to. */
		public static final Config UNKNOWN = new Config(true, false, false, false, false, false, false, false);
	}

	/** Server -> client: freshly scanned chunk colours. */
	public record Tiles(List<Tile> tiles) implements CustomPacketPayload {
		public static final Type<Tiles> TYPE = channel("tiles");
		public static final StreamCodec<FriendlyByteBuf, Tiles> CODEC = CustomPacketPayload.codec(
			(v, b) -> {
				b.writeVarInt(v.tiles.size());
				for (Tile t : v.tiles) { b.writeUtf(t.dim(), 128); b.writeVarInt(t.x()); b.writeVarInt(t.z()); b.writeByteArray(t.rgb()); }
			},
			b -> {
				int n = Math.min(b.readVarInt(), 256);
				List<Tile> out = new ArrayList<>(n);
				for (int i = 0; i < n; i++) out.add(new Tile(b.readUtf(128), b.readVarInt(), b.readVarInt(), b.readByteArray()));
				return new Tiles(out);
			});
		@Override public Type<Tiles> type() { return TYPE; }
	}

	/** Client -> server: which explored regions does the server's map have? */
	public record IndexReq() implements CustomPacketPayload {
		public static final Type<IndexReq> TYPE = channel("index_req");
		public static final StreamCodec<FriendlyByteBuf, IndexReq> CODEC = CustomPacketPayload.codec((v, b) -> {}, b -> new IndexReq());
		@Override public Type<IndexReq> type() { return TYPE; }
	}

	public record RegionKey(String dim, int x, int z) {}

	/** Server -> client: the answer to IndexReq. */
	public record Index(List<RegionKey> regions) implements CustomPacketPayload {
		public static final Type<Index> TYPE = channel("index");
		public static final StreamCodec<FriendlyByteBuf, Index> CODEC = CustomPacketPayload.codec(
			(v, b) -> {
				b.writeVarInt(v.regions.size());
				for (RegionKey k : v.regions) { b.writeUtf(k.dim(), 128); b.writeVarInt(k.x()); b.writeVarInt(k.z()); }
			},
			b -> {
				int n = Math.min(b.readVarInt(), 8192);
				List<RegionKey> out = new ArrayList<>(n);
				for (int i = 0; i < n; i++) out.add(new RegionKey(b.readUtf(128), b.readVarInt(), b.readVarInt()));
				return new Index(out);
			});
		@Override public Type<Index> type() { return TYPE; }
	}

	/** Client -> server: send me that explored region. */
	public record RegionReq(String dim, int x, int z) implements CustomPacketPayload {
		public static final Type<RegionReq> TYPE = channel("region_req");
		public static final StreamCodec<FriendlyByteBuf, RegionReq> CODEC = CustomPacketPayload.codec(
			(v, b) -> { b.writeUtf(v.dim, 128); b.writeVarInt(v.x); b.writeVarInt(v.z); },
			b -> new RegionReq(b.readUtf(128), b.readVarInt(), b.readVarInt()));
		@Override public Type<RegionReq> type() { return TYPE; }
	}

	/** Server -> client: one explored region as a PNG (512 x 512 RGBA, transparent where unseen). */
	public record RegionData(String dim, int x, int z, byte[] png) implements CustomPacketPayload {
		public static final Type<RegionData> TYPE = channel("region");
		public static final StreamCodec<FriendlyByteBuf, RegionData> CODEC = CustomPacketPayload.codec(
			(v, b) -> { b.writeUtf(v.dim, 128); b.writeVarInt(v.x); b.writeVarInt(v.z); b.writeByteArray(v.png); },
			b -> new RegionData(b.readUtf(128), b.readVarInt(), b.readVarInt(), b.readByteArray()));
		@Override public Type<RegionData> type() { return TYPE; }
	}

	/** Client -> server: work out a seed preview of this region for me. */
	public record GenReq(int id, String dim, int rx, int rz) implements CustomPacketPayload {
		public static final Type<GenReq> TYPE = channel("gen_req");
		public static final StreamCodec<FriendlyByteBuf, GenReq> CODEC = CustomPacketPayload.codec(
			(v, b) -> { b.writeVarInt(v.id); b.writeUtf(v.dim, 128); b.writeVarInt(v.rx); b.writeVarInt(v.rz); },
			b -> new GenReq(b.readVarInt(), b.readUtf(128), b.readVarInt(), b.readVarInt()));
		@Override public Type<GenReq> type() { return TYPE; }
	}

	/** Server -> client: the preview (RGB, 128 x 128), or why not. The seed itself never leaves the server. */
	public record GenData(int id, byte[] rgb, String error) implements CustomPacketPayload {
		public static final Type<GenData> TYPE = channel("gen");
		public static final StreamCodec<FriendlyByteBuf, GenData> CODEC = CustomPacketPayload.codec(
			(v, b) -> { b.writeVarInt(v.id); b.writeByteArray(v.rgb); b.writeUtf(v.error, 256); },
			b -> new GenData(b.readVarInt(), b.readByteArray(), b.readUtf(256)));
		@Override public Type<GenData> type() { return TYPE; }
	}

	public record PlayerPos(String name, String dim, double x, double y, double z, float yaw) {}

	/** Server -> client: where everyone online is (only when the admin turned that on). */
	public record Players(List<PlayerPos> players) implements CustomPacketPayload {
		public static final Type<Players> TYPE = channel("players");
		public static final StreamCodec<FriendlyByteBuf, Players> CODEC = CustomPacketPayload.codec(
			(v, b) -> {
				b.writeVarInt(v.players.size());
				for (PlayerPos p : v.players) { b.writeUtf(p.name(), 48); b.writeUtf(p.dim(), 128); b.writeDouble(p.x()); b.writeDouble(p.y()); b.writeDouble(p.z()); b.writeFloat(p.yaw()); }
			},
			b -> {
				int n = Math.min(b.readVarInt(), 256);
				List<PlayerPos> out = new ArrayList<>(n);
				for (int i = 0; i < n; i++) out.add(new PlayerPos(b.readUtf(48), b.readUtf(128), b.readDouble(), b.readDouble(), b.readDouble(), b.readFloat()));
				return new Players(out);
			});
		@Override public Type<Players> type() { return TYPE; }
	}

	static void register() {
		PayloadTypeRegistry.serverboundPlay().register(Hello.TYPE, Hello.CODEC);
		PayloadTypeRegistry.serverboundPlay().register(IndexReq.TYPE, IndexReq.CODEC);
		PayloadTypeRegistry.serverboundPlay().register(RegionReq.TYPE, RegionReq.CODEC);
		PayloadTypeRegistry.serverboundPlay().register(GenReq.TYPE, GenReq.CODEC);
		PayloadTypeRegistry.clientboundPlay().register(Config.TYPE, Config.CODEC);
		PayloadTypeRegistry.clientboundPlay().registerLarge(Tiles.TYPE, Tiles.CODEC, 256 << 10);
		PayloadTypeRegistry.clientboundPlay().registerLarge(Index.TYPE, Index.CODEC, 256 << 10);
		PayloadTypeRegistry.clientboundPlay().registerLarge(RegionData.TYPE, RegionData.CODEC, 4 << 20);
		PayloadTypeRegistry.clientboundPlay().registerLarge(GenData.TYPE, GenData.CODEC, 256 << 10);
		PayloadTypeRegistry.clientboundPlay().register(Players.TYPE, Players.CODEC);
	}
}
