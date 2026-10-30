# 🗺️ How HotSwap is built

A map of every file and what it does. HotSwap has two halves:

- **The engine** (`src-tauri/src`, Rust) does the real work: it finds your games, reads and writes game memory, installs mods, and talks to Discord.
- **The interface** (`ui`, plain HTML, CSS and JavaScript) is what you see. It runs in Windows' built-in WebView2 and asks the engine to do things through commands.

## ⚙️ Engine: `src-tauri/src`

### App and windows

| File | What it does |
|---|---|
| `main.rs` | Starts the app: settings, the main window, tray icon, hotkeys, library scanning, launching games, updates, and the list of commands the interface can call. |
| `overlay.rs` | The in-game overlay window (notes, cheat sheet, HUD and menu) and the radar window; hands focus back to the game and closes itself when idle. |
| `watch.rs` | Notices which installed game is running, and tells the interface when that changes. |
| `trainer.rs` | Runs the mods: attaches to the running game and keeps applying the values you've set. |
| `logs.rs` | HotSwap's log file (rotates at 5 MB), crash logging, and Export logs (full, or under 5 MB for Discord). |

### Commands for the pages

| File | What it does |
|---|---|
| `cmd_maps.rs` | Commands for the Maps page: the web map, the HotSwap map's world and background, your position, teleport and pins. |
| `cmd_mods.rs` | Commands for the Mods page: browsing, installing, switching off and removing mods from every mod site. |
| `cmd_discord.rs` | Commands for Discord: your profile status and Discord voice. |

### Your library

| File | What it does |
|---|---|
| `steam.rs` | Reads your Steam install: libraries, installed games, playtime and cached artwork. |
| `appinfo.rs` | Reads Steam's app info file to tell games apart from tools and software. |
| `vdf.rs` | Reads Valve's text config files (`.vdf`, `.acf`). |
| `epic.rs` | Finds installed Epic Games Store games. |
| `gog.rs` | Finds installed GOG games. |
| `home.rs` | Fetches the Home page's news, deals, top sellers and new releases. |
| `sources.rs` | Custom Home panel sources: recognises feeds, sites with feeds, JSON lists and websites. |

### Game support

| File | What it does |
|---|---|
| `mono.rs` | Reads and changes Unity (Mono) games by asking the game's own runtime where things are. |
| `ksp.rs` | Kerbal Space Program mods: the game's cheat switches, resources, funds, science and reputation. |
| `sn1.rs` | Subnautica and Below Zero mods: the game's cheats and game-mode options. |
| `sn2.rs` | Subnautica 2 mods: player stats in an Unreal game without debug symbols. |
| `repo.rs` | R.E.P.O. mods: health, stamina, extraction skipping and the enemy radar. |
| `extra.rs` | Oxygen Not Included, Mon Bazou and Occupy Mars mods. |
| `scrap.rs` | Scrap Mechanic mods through a Lua bridge (switched off for now). |
| `world.rs` | Reads Satisfactory's world for the HotSwap map: resources, collectibles and your factory. |

### Unreal Engine games: `ue/`

| File | What it does |
|---|---|
| `ue/mod.rs` | Ties the Unreal readers together. |
| `ue/mem.rs` | Reads and writes another program's memory. |
| `ue/objects.rs` | Walks a running Unreal game's object list and names. |
| `ue/symbols.rs` | Finds Unreal's internals from a game's debug symbol files. |
| `ue/scan.rs` | Finds the same internals in games that ship without symbols. |
| `ue/call.rs` | Calls the game's own script functions from outside. |

### Maps

| File | What it does |
|---|---|
| `iostore.rs` | Reads files out of Unreal Engine 5 game archives. |
| `mapimage.rs` | Builds Satisfactory's map picture from the game's own map texture. |
| `sn1map.rs` | Builds Subnautica's map picture from the game's own terrain files. |

### Mods from mod sites

| File | What it does |
|---|---|
| `modloader.rs` | Satisfactory mods from ficsit.app, with their dependencies and the mod loader. |
| `modfiles.rs` | Installs downloaded mod files: UE4SS, Lua and pak mods for Subnautica 2. |
| `thunderstore.rs` | Thunderstore mods (BepInEx games), and installing Nexus Mods files into BepInEx. |
| `spacedock.rs` | KSP mods from SpaceDock, installed into GameData. |

### Discord

| File | What it does |
|---|---|
| `discord.rs` | The "Playing HotSwap" status on your Discord profile. |
| `voice.rs` | Discord voice (beta): your call, who's talking, mute, deafen, volumes, devices and notifications. |

## 🎨 Interface: `ui`

| File | What it does |
|---|---|
| `index.html` | The main window's layout: sidebar, top bar and the page containers. |
| `styles.css` | How the main window looks. |
| `data.js` | Every game's mod list (names, descriptions, hotkeys) and which mods work. |
| `app.js` | The core: saved settings, the library, profiles, key bindings, changing mods, and sending errors to the log. |
| `home.js` | The Home page: its panels, customise mode (move, resize, hide, add) and custom sources. |
| `pages.js` | The Maps, Settings and game pages, and the function that draws whichever page is open. |
| `ingame.js` | Sends mod changes, the menu and the HUD to the overlay. |
| `discord.js` | The Overlay page, Discord voice popups, who's talking, and your Discord status. |
| `session.js` | Sounds, the Play button, Inject and the running game's session. |
| `hotkeys.js` | Reacts to hotkey presses and records new key bindings. |
| `updates.js` | Checks for and installs HotSwap updates. |
| `extras.js` | Status dots, the cheat sheet, Export logs, share codes, What's new and mini mode. |
| `palette.js` | The Ctrl+K command palette. |
| `events.js` | Every click, key and form input in the main window. |
| `icons.js` | Small icons used across the pages. |
| `boot.js` | Starts the interface; loaded last. |
| `map.js` | The HotSwap map for Satisfactory; only loaded when you open Maps. |
| `mods.js` | The Mods page; only loaded when you open Mods. |
| `map-overlay.js` | Adds your position and pins on top of the Satisfactory Calculator web map. |
| `overlay.html` | The in-game overlay window: notes, cheat sheet, HUD, menu and the Discord panel. |
| `radar.html` | The R.E.P.O. enemy radar window. |

## 📦 Other files

| File | What it does |
|---|---|
| `src-tauri/build.rs` | Lists the commands the interface is allowed to call. |
| `src-tauri/capabilities/` | Which windows may use which commands. |
| `src-tauri/assets/sm_bridge.lua` | The Scrap Mechanic Lua bridge (unused while Scrap Mechanic is off). |
| `.github/workflows/release.yml` | A signed release build on GitHub (waiting for code signing). |
