// Features on and off (Settings): whole parts of HotSwap that can be switched off so they don't
// load or run at all. feat() in app.js says whether one is on; each part checks it.

// key, name, what it does, what turning it off saves. Keys starting with "set" are existing
// switches (their own state and click handler in events.js); the rest live in state.off.
const FEATURES = [
  ['feed', 'Home feed', 'News from your games, deals, store charts and your own sources on Home.', 'No downloads from Steam, CheapShark or your sources. Home keeps Jump back in.'],
  ['maps', 'Maps', 'Interactive maps for supported games.', 'The Maps page and its code never load.'],
  ['mods', 'Mods page', 'Installing mods from Thunderstore, Nexus and SpaceDock.', 'The Mods page and its mod lists never load.'],
  ['discord', 'Discord', 'Who’s talking, Discord popups and the Discord tab in the overlay.', 'No connection to the Discord app for voice. Your Discord status has its own switch below.'],
  ['stats', 'System stats', 'Graphs of your PC in the overlay, the stats widget and stats on the HUD.', 'No hardware readings at all.'],
  ['setOverlay', 'In-game overlay', 'Popups, the HUD, the cheat sheet and the overlay menu over your game.', 'The overlay window never opens.'],
  ['setSounds', 'Sounds', 'A short chime when a mod turns on or off.', ''],
  ['setAutoUpdate', 'Update checks', 'Looks for a new HotSwap on GitHub at start and every 30 minutes. Updates are signed, and nothing installs without your click.', 'You can still check by hand under About.'],
];
const FEATURE_STATE = { setOverlay: 'overlay', setSounds: 'sounds', setAutoUpdate: 'autoUpdate' };
const featOn = k => FEATURE_STATE[k] ? !!state[FEATURE_STATE[k]] : feat(k);

function featuresSettings() {
  const row = ([k, name, what, saves]) => {
    const on = featOn(k), id = FEATURE_STATE[k] ? ` id="${k}"` : ` data-feat="${k}"`;
    if (FEATURE_STATE[k] && !invoke && k !== 'setSounds') return '';
    return `<div class="set-row"><div class="txt"><b>${name}</b><span>${what}${saves ? ` <i>Off: ${saves}</i>` : ''}</span></div><button class="switch" role="switch"${id} aria-checked="${on}"></button></div>`;
  };
  const n = FEATURES.filter(f => !featOn(f[0])).length;
  return `<div class="set-group"><h2>Features</h2>
    <div class="set-row"><div class="txt"><b>Turn off what you don’t use</b><span>Switched off features don’t load or run at all, so HotSwap uses less memory, network and CPU. ${n ? `${n} off right now.` : 'Everything is on.'}</span></div>
      <div class="btnrow"><button class="btn" data-featall="0" title="Only mods, hotkeys, the overlay and update checks">Minimal</button><button class="btn" data-featall="1">All on</button></div></div>
    ${FEATURES.map(row).join('')}
  </div>`;
}

// Starts or stops what a feature runs in the background when it's switched.
function applyFeature(k) {
  const on = feat(k);
  if (k === 'feed') { if (on) refreshHome(false); }
  if ((k === 'maps' || k === 'mods') && !on && state.view === k) { go('home'); return; }
  if (k === 'discord') {
    VOICE = null; lastTalk = '';
    if (invoke && localStorage.getItem('hs.dc.voice') === '1') invoke(on ? 'voice_start' : 'voice_stop').catch(() => {});
    if (!on) { talkOn = false; overlaySend({ kind: 'talk', members: [] }); }
    lastMenu = ''; pushMenu();
  }
  if (k === 'stats') { pushStatsCfg(true); lastMenu = ''; pushMenu(); }
}

document.addEventListener('click', e => {
  const t = e.target.closest('[data-feat],[data-featall]');
  if (!t) return;
  const set = (k, on) => { const off = (state.off ||= {}); if (on) delete off[k]; else off[k] = true; };
  if (t.dataset.feat) {
    const k = t.dataset.feat;
    set(k, !feat(k));
    save(); applyFeature(k);
  } else {
    const on = t.dataset.featall === '1';
    for (const [k] of FEATURES) {
      if (FEATURE_STATE[k]) { if (k !== 'setOverlay' && (on || k !== 'setAutoUpdate')) state[FEATURE_STATE[k]] = on; continue; }
      if (feat(k) === on) continue;
      set(k, on); save(); applyFeature(k);
    }
    // Minimal keeps the overlay (it's how mods are used in game) and update checks (fixes and
    // safety). All on turns everything back on.
    if (on && !state.overlay) state.overlay = true;
    save();
  }
  render();
});

// Shown on the Overlay page in place of a switched off feature's settings.
const featOffNote = name => `<div class="set-group"><h2>${name}</h2><div class="set-tip">${name} is turned off in Settings → Features.</div></div>`;
