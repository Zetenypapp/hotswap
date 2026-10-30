// The in-game overlay: mod notes, the menu (full, or Discord only) and the HUD.

/* ---------- The in-game overlay: notes, cheat sheet, menu and HUD ---------- */
let menuOpen = false, hudOn = false, DISCORD_ERR = '';
function overlaySend(note) {
  if (!invoke) return;
  const a = ACCENTS.find(x => x.c === state.accent) || ACCENTS[0];
  invoke('overlay_notify', { note: { ...note, fast: !!state.fgFriendly, accent: a.c, ink: a.ink, corner: state.overlayCorner, hudCorner: state.hudCorner, keep: menuOpen || hudOn || talkOn || statsKeep() } }).catch(console.error);
}

// The menu: the running game's mods over the game, driven by mouse or keys (see overlay.html).
function menuData(g) {
  const pins = state.pinned[g.id] || {};
  return {
    game: g.name, connected: TRAINER.connected, message: TRAINER.message, paused: !!state.paused[g.id], count: activeCount(g),
    profile: activeProf(g).name, profiles: profilesOf(g).map(p => p.name), start: sessionStart, menuKey: comboLabel(state.menuKey),
    cats: g.cats.map(c => ({
      name: c.name,
      mods: c.mods.map(m => {
        const v = valueOf(g, m), k = keysOf(g, m);
        return {
          id: m.id, name: m.name, type: m.type, info: m.info || '', live: m.live, pinned: !!pins[m.id], on: isActive(g, m),
          value: m.type === 'slider' ? v : null, shown: m.type === 'slider' ? fmt(m, v) : '', min: m.min, max: m.max, step: m.step,
          keys: [k.up && (m.type === 'slider' ? '↑ ' : '') + comboLabel(k.up), k.down && '↓ ' + comboLabel(k.down)].filter(Boolean),
        };
      }),
    })),
  };
}
let lastMenu = '';
function pushMenu() {
  if (!menuOpen) return;
  const g = findGame(runningId());
  const mods = g && modsOf(g).length;
  if (!mods && !state.dcOverlay) { toggleMenu(false); return; }
  const data = mods ? menuData(g) : dcMenuData(), sig = JSON.stringify(data);
  if (sig === lastMenu) return;
  lastMenu = sig;
  overlaySend({ kind: 'menu', open: true, data });
}
// The Discord-only menu, for games without HotSwap mods (any game, even ones with anti-cheat:
// the overlay is a separate window and never touches the game).
function dcMenuData() {
  const r = RUNNING && findGame(RUNNING);
  return { discordOnly: true, game: r ? r.name : 'Discord', menuKey: comboLabel(state.menuKey), cats: [], profiles: [], count: 0 };
}
// `test` opens it from Settings without a game running (with the game HotSwap shows).
function toggleMenu(open = !menuOpen, test = false) {
  if (!invoke || !state.overlay) return;
  const g = test ? game() : findGame(runningId());
  if (open && (!g || !modsOf(g).length) && !state.dcOverlay) { if (test) toast('Open a game with mods first', ''); return; }
  menuOpen = open;
  lastMenu = '';
  invoke('overlay_menu', { open }).catch(console.error);
  if (open) pushStatsCfg(true);
  if (open) {
    if (test) overlaySend({ kind: 'menu', open: true, data: g && modsOf(g).length ? menuData(g) : dcMenuData() }); else pushMenu();
  } else overlaySend({ kind: 'menu', open: false });
}
function onOverlayInput(i) {
  const g = findGame(runningId());
  if (i.type === 'close' || !g || !modsOf(g).length) { if (i.type === 'close') toggleMenu(false); return; }
  const m = modsOf(g).find(x => x.id === i.id);
  if (i.type === 'set' && m) setMod(g, m, i.value, 'overlay menu');
  else if (i.type === 'act' && m) setMod(g, m, null, 'overlay menu');
  else if (i.type === 'profile') switchProfile(g, i.name);
  else if (i.type === 'panic') panic('overlay menu');
  else if (i.type === 'pause') {
    state.paused[g.id] = !state.paused[g.id];
    logLine(g, state.paused[g.id] ? 'Mods paused' : 'Mods resumed');
    save(); render();
  }
  pushMenu();
}

// The HUD: once a second while it's on and a game runs.
async function syncHud(force) {
  const g = findGame(runningId());
  const want = !!(invoke && state.overlay && state.hud && !state.fgFriendly && g);
  if (!want) {
    if (hudOn || force) { hudOn = false; overlaySend({ kind: 'hud', data: null }); }
    return;
  }
  hudOn = true;
  let cpu = null, ram = null;
  if (state.hudStats) [cpu, ram] = await invoke('system_stats').catch(() => [null, null]);
  overlaySend({
    kind: 'hud',
    data: { game: g.name, count: activeCount(g), mods: modsOf(g).filter(m => isActive(g, m)).map(m => m.type === 'slider' ? `${m.name} ${fmt(m, valueOf(g, m))}` : m.name),
      start: sessionStart, connected: TRAINER.connected && !state.paused[g.id], cpu, ram },
  });
}
setInterval(() => syncHud(false), 1000);
