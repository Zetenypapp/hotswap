package dev.hotswap.link;

import net.fabricmc.api.ModInitializer;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/** Runs on both sides: registers the payload types, and the server features (off by default). */
public final class HotSwapLinkCommon implements ModInitializer {
	public static final Logger LOG = LoggerFactory.getLogger("HotSwap Link");

	@Override
	public void onInitialize() {
		Net.register();
		ServerSide.init();
	}
}
