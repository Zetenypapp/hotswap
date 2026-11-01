## What changed

<!-- What does this pull request do? Link the issue it fixes, if there is one (e.g. "Fixes #12"). -->

## Why

<!-- The problem it solves, or what it makes better. -->

## How I tested it

<!-- The app builds and starts, no errors in the logs. For mods: which game and version, and what each mod did in the game. -->

- [ ] HotSwap builds and starts (`cargo tauri dev`)
- [ ] For mods: each one tested in the game (game version: )
- [ ] New games are marked `experimental: true` unless every mod was tested

## Checklist

- [ ] Single-player only: nothing here touches online games or gets around anti-cheat
- [ ] No inline scripts or `eval` in the UI, and new Tauri commands are only allowed for the window that uses them
- [ ] Matches the style of the code around it, line endings unchanged
- [ ] I agree my contribution is licensed under GPL-3.0-or-later

<!-- Screenshots help for UI changes. See CONTRIBUTING.md for details. -->
