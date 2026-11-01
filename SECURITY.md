# Security Policy

HotSwap reads and changes other programs' memory, so we take its own security seriously. Thank you for helping keep it and its users safe.

## Supported versions

Only the **latest release** gets security fixes. HotSwap updates itself (Settings → About and updates → Check now), so please make sure you're on the newest version before reporting.

| Version | Supported |
|---|---|
| Latest release | ✅ |
| Anything older | ❌ Update first |

## Reporting a vulnerability

**Please don't open a public issue for security problems.** Report them privately instead:

1. Go to the repository's **[Security tab → Report a vulnerability](https://github.com/Zetenypapp/hotswap/security/advisories/new)**.
2. Describe the problem, how to reproduce it, and what an attacker could do with it.
3. If you can, include the HotSwap version, your Windows version, and logs (Settings → Export logs; check them for anything personal first).

### What to expect

- **Within 7 days:** a first reply saying we got your report.
- **Within 30 days:** a fix or a plan, depending on how serious and complex it is. We'll keep you updated along the way.
- **When it's fixed:** a release with the fix, and credit for you in the release notes and the advisory (unless you'd rather stay anonymous).

Please give us a reasonable chance to fix the problem before you talk about it publicly.

## What counts

Examples of things we want to hear about:

- Code running in HotSwap's window that shouldn't (for example getting past the Content Security Policy, or a window using commands it isn't allowed to)
- Ways to make HotSwap write files outside where it should (mod installs, zip extraction, settings)
- Ways for a website, mod, feed or another computer on the network to control HotSwap or the games it connects to
- Problems with how updates are downloaded or verified
- Anything that lets HotSwap's mods reach online or multiplayer games, or get past the singleplayer-only checks

Not security vulnerabilities (please use a normal issue instead):

- A mod not working, or a game crashing with a mod on
- Antivirus false positives (HotSwap edits game memory, which some scanners flag; tell us in an issue)
- Something that needs an attacker who already controls your Windows account

## How HotSwap protects itself

The [README](README.md)'s Security section explains the protections in place: a strict Content Security Policy, least-privilege permissions for every window, signed updates, safe archive handling, local-only connections and anti-cheat awareness.

## Out of scope: cheating in online games

HotSwap is for **single-player** games. Reports asking for help to get around anti-cheat systems, or to use HotSwap in online games, will be closed. See the README's Fair play section.
