# 🔁 Hotswap

**Swap the way you play, in real time.**

Hotswap is a Windows desktop app for single-player game mods: switch mods on and off while the game runs, with hotkeys that work in-game. No ads, no paywalls.

> 💡 **Inspired by ArtCraft.**
> 🤖 **Made by AI.** This app was designed and written by Claude (Anthropic's AI) in Claude Code, directed by [@Zetenypapp](https://github.com/Zetenypapp).

> ⚠️ **Early preview:** the interface and game detection work, but **mods don't change games yet**. The mod lists are placeholders.

## ✨ Features

- 🎮 **Your real library:** finds installed Steam, Epic and GOG games, with artwork read from Steam's local cache
- 🧰 **Games vs software:** tools like OBS or 3DMark are sorted out using Steam's own app labels
- 🛡️ **Anti-cheat aware:** games with VAC, Easy Anti-Cheat, BattlEye or EA Javelin get no mods
- ▶️ **Real Play button:** launches the game and detects when it's running, however you started it
- ⌨️ **In-game hotkeys:** rebindable, with clash warnings and a panic key that turns everything off
- ⏻ **Power and stop:** pause all mods, or close the game, from the sidebar
- 🗂️ **Profiles:** save mod setups per game and switch in one click
- 📰 **Live Home:** developer news for your games, Steam deals (via CheapShark), top sellers and new releases
- ⏱️ **Playtime and sessions:** Steam hours plus time played with Hotswap
- 🔍 **Ctrl+K palette:** jump to any game, page, mod or action
- 🎨 **Accent colors, tray icon, start with Windows**

## 🛠️ Built with

- 🦀 **Rust + [Tauri 2](https://tauri.app)** for the app itself: library scanning, game detection, hotkeys, launching
- 🌐 **Plain HTML, CSS and JavaScript** for the interface (no framework, no build step)

## 🚀 Run it

Needs Windows, [Rust](https://rustup.rs), and the Visual Studio C++ build tools.

```sh
cd src-tauri
cargo run
```

Dev helpers: `hotswap.exe --scan` prints your detected library, `--home` prints the live Home data.

## 📜 Credits

- Font: [Figtree](https://github.com/erikdkennedy/figtree), SIL Open Font License
- Game news and store data: Steam Web API and storefront; deals: [CheapShark](https://www.cheapshark.com)
- Game artwork is read from your own Steam install and is never bundled or redistributed
