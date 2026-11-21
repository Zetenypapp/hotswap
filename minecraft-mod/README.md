# HotSwap Link

A small client-side Fabric mod that puts your Minecraft (Java Edition) world on HotSwap's Maps page.

- **Your position**, direction and dimension, live.
- **The map fills in as you explore**: the colours of the chunks your game already has loaded, a few per tick, nearest first. HotSwap keeps them per world and dimension, so the map stays for next time.
- **Seed preview (single-player)**: land height and biome of places you haven't been, worked out with the world's own generator on one low-priority thread. Nothing is generated or saved in your world. Servers don't share their seed, so there it's explored-only.

- **Structures**: villages, strongholds, End cities, ancient cities, monuments, mansions and every other kind, located from the seed in single-player (and on servers that allow it).
- **What's around you**: mobs, animals, villagers, dropped items and nearby players your game has loaded, plus your health, hunger, XP, biome, the clock, the weather and where you last died.

## What a server allows

- **Single-player:** everything.
- **A server without HotSwap Link:** your own position and the map you explore, but no mobs, items or other players. The server can't say what it allows, so the map doesn't assume.
- **A server with HotSwap Link** (the same jar in its `mods` folder): whatever its `config/hotswap.properties` allows. Everything is on by default:

```properties
# --- client permissions ---
is_map_allowed_at_all=true   # false turns HotSwap's map off completely on this server
hostiles=true
animals=true
villagers=true
players=true                 # also sends everyone's position, not just nearby players
items=true

# --- server features ---
shared_map=true              # one explored map for everyone, kept in the world folder
seed_preview=true            # terrain previews from the server's generator; the seed is never sent

# --- locating structures ---
overall_location=true        # false hides every structure; modded ones follow this
villages_location=true
strongholds_location=true
end_cities_location=true
# ...one line per kind: pillager_outposts, woodland_mansions, desert_pyramids, jungle_temples,
# swamp_huts, igloos, ocean_monuments, ocean_ruins, shipwrecks, buried_treasures, ruined_portals,
# mineshafts, trail_ruins, trial_chambers, ancient_cities, abandoned_camp, nether_complexes, nether_fossils
```

Structures are found the way `/locate` finds them: the world's placement grid from the seed, then the game's own biome and terrain check. Nothing is generated. A config from an older version gets the structure lines added (all on) the next time the server starts.

Players without the mod join exactly as before.

It never shares chat or anything you type, and it sends nothing to the game. It listens on `127.0.0.1` only, on a random port, behind a per-session token in `~/.hotswap/minecraft-link.json`, and refuses browser requests.

Needs Minecraft 26.3, Fabric Loader 0.20+ and Fabric API.

## Build

```
gradlew build
```

The jar lands in `build/libs/`.
