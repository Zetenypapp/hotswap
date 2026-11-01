# Contributing to HotSwap

Thanks for wanting to help! Bug reports, game requests, mods for new games and code are all welcome. Please follow the [Code of Conduct](CODE_OF_CONDUCT.md) everywhere in the project.

## The ground rules

- **Single-player only.** HotSwap never mods online or multiplayer games, and never gets around anti-cheat. Contributions that do will be closed. Games whose anti-cheat only runs online can get singleplayer-only mods that switch off by themselves online (see Unturned in `ui/data.js`).
- **Security problems go through private reporting**, not issues. See [SECURITY.md](SECURITY.md).
- **Your contributions are licensed under GPL-3.0-or-later**, the same as HotSwap (see [LICENSE](LICENSE)). The HotSwap name and logo stay reserved.

## Reporting bugs and requesting games

- **Bugs:** in HotSwap, go to **Settings → Report a bug**. It opens a GitHub issue with your version and system details filled in. Say what you did, what happened and what you expected. Logs help a lot (Settings → Export logs; check them for anything personal first).
- **Game requests:** use **Request mods** on a game's page in HotSwap, or open an issue with the game's name and its Steam link.
- **Mods that don't work:** name the game, its version, and which mod.

## Setting up

You need Windows 10 or 11, [Rust](https://rustup.rs) (stable), the [Tauri CLI](https://tauri.app) (`cargo install tauri-cli`), and Visual Studio's C++ build tools.

```powershell
git clone https://github.com/Zetenypapp/hotswap
cd hotswap\src-tauri
cargo tauri dev
```

Tips:
- **Build from PowerShell**, not Git Bash. Git Bash is missing a tool the build needs, and switching between the two forces a full rebuild.
- **Close an installed HotSwap first** (including the tray icon). Only one HotSwap can run at a time, so the dev build would quit straight away.
- If a dev build fails to link with strange "unresolved external symbol anon..." errors, run `cargo clean -p hotswap` and build again.

[ARCHITECTURE.md](ARCHITECTURE.md) explains what every file does.

## Adding mods for a game

Most new games need **no Rust at all**. If the game runs on an engine HotSwap can read (Unity, Unreal Engine 4.25+, Electron and NW.js, RPG Maker MV/MZ, Ren'Py, Godot), its mods are a few lines in `ui/data.js`:

```js
1234560: { genre: 'Survival', experimental: true, cats: [
  { name: 'Player', mods: [
    T('Infinite Health', 'Health stays full.', { at: 'PlayerStats.instance.health', fill: 'maxHealth' }),
    T('Infinite Money', '', { at: 'GameManager.money', min: 999999 }),
    A('Add 1,000 Gold', '', { at: 'GameManager.gold', add: 1000 }),
  ]},
]},
```

- `at` is a path made of the game's own class and field names. The comment above `T`/`S`/`A` in `data.js` lists every operation (`set`, `min`, `max`, `freeze`, `best`, `fill`, `scale`, `value`, `add`, `put`, `js`).
- To find the names, run the game and use the dev dump:
  `hotswap --engine-dump unity-mono <exe name> [Namespace/Class]`
  (also `unity-il2cpp` and `unreal`). Without a class, it lists the game's classes; with one, it lists its fields, offsets and types.
- **Mark new games `experimental: true`** until someone has tested every mod in the game.
- Mods can also ship between releases through `catalog/mods.json` (same shape as the catalog, mods as plain objects). Only the maintainer can publish it: it has to be signed with HotSwap's update key, and the app drops anything that would run code.
- Games on their own engine need a Rust module like `ets2.rs` or `stormworks.rs`. Open an issue first so we can talk about the approach.

## Code style

- **Match the code around you:** naming, comment density and idioms.
- **Comments say why**, in plain English, the way the existing ones do.
- **Keep line endings as they are.** Most files use CRLF and a few use LF; don't convert whole files.
- **No hard-coded memory offsets** when the engine can tell us the name. That's what keeps HotSwap working after game updates.
- **Keep the security rules:** no inline scripts or `eval` in the UI (the Content Security Policy blocks them), and every new Tauri command needs a permission in `src-tauri/build.rs` and the right file in `src-tauri/capabilities/`, granted only to the window that uses it.
- **UI text:** short, plain sentences. Tell the user what happens, not how it's implemented.

## Pull requests

1. **Fork the repo** and create a branch from `main`.
2. **Keep it focused:** one game or one feature per pull request.
3. **Test it:** the app builds and starts, there are no errors in the logs, and for mods, each one does what it says in the game (say which game version you tested).
4. **Describe it:** what changed, why, and how you tested it. Screenshots help for UI changes.
5. **Wait for review:** the maintainer will look and may ask for changes. Releases are made by the maintainer.

Thank you for making HotSwap better!
