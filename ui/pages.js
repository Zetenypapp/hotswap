// The Maps, Settings and game pages, and render(), which draws whichever page is open.

/* ---------- Maps ---------- */
// Steam app IDs with an interactive map (opened in its own window by the backend).
const MAP_GAMES = ['526870', '264710'];
// Shown on the Maps page but not ready yet.
const SOON_MAPS = ['387990'];
const MAP_NAMES = { 526870: 'Satisfactory', 264710: 'Subnautica', 387990: 'Scrap Mechanic' };
// Games that also have Satisfactory Calculator's web map.
const WEB_MAPS = ['526870'];

let mapOpen = null, mapShown = false; // app ID of the map being shown on the Maps page
let mapMode = 'local'; // 'local' = HotSwap's own map, 'web' = Satisfactory Calculator

function renderMaps() {
  if (typeof LocalMap === 'undefined') return lazyPage('mapsBody', 'map.js');
  setAmbient(null);
  const list = MAP_GAMES.map(id => LIB.find(g => String(g.appId) === id) || buildGame({ appId: +id, name: MAP_NAMES[id] }));
  const open = list.find(g => String(g.appId) === mapOpen);
  // Re-renders happen on every app event; leave a live map alone instead of rebuilding it.
  const key = open ? `${open.appId}:${mapMode}` : '';
  if (key && $('mapsBody').dataset.key === key) return;
  $('mapsBody').dataset.key = key;
  if (!open || mapMode !== 'local') LocalMap.unmount();
  $('mapsBody').innerHTML = open ? `
    <div class="lib-head map-head"><button class="chip" data-mapback>‹ All maps</button><h1>${esc(open.name)}</h1>
      ${WEB_MAPS.includes(String(open.appId)) ? `<div class="seg-tabs"><button class="chip${mapMode === 'local' ? ' on' : ''}" data-mapmode="local">HotSwap map</button><button class="chip${mapMode === 'web' ? ' on' : ''}" data-mapmode="web">Detailed map</button></div>` : ''}
      <span class="meta">${mapMode === 'web' ? 'By Satisfactory Calculator (their site, their ads).' : WEB_MAPS.includes(String(open.appId)) ? 'Live from your game. Right-click to teleport or pin.' : 'Made from your game\'s terrain files. Right-click to drop a pin.'}</span></div>
    <div id="mapHost" class="map-host">${mapMode === 'web' ? '<span>Loading map…</span>' : ''}</div>` : `
    <div class="lib-head"><h1>Maps</h1><span class="meta">Resources, collectibles and your factory, live. Pin spots and teleport.</span></div>
    <div class="lib-grid">${list.map(g => `<button class="lib-item" data-map="${g.appId}"><span class="art">${capsule(g)}</span>
      <span class="nm">${esc(g.name)}</span><span class="sub">Interactive map${RUNNING === g.id ? ' · live' : ''}</span></button>`).join('')}
      ${SOON_MAPS.map(id => LIB.find(g => String(g.appId) === id) || buildGame({ appId: +id, name: MAP_NAMES[id] })).map(g => `<button class="lib-item soon-item" disabled><span class="art">${capsule(g)}</span>
      <span class="nm">${esc(g.name)}</span><span class="sub">Coming soon</span></button>`).join('')}</div>
    <p class="meta" style="margin-top:22px">The HotSwap map is drawn from your running game. The detailed map is <a href="https://satisfactory-calculator.com" ${ext}>Satisfactory Calculator</a>, shown as-is.</p>`;
  if (open && mapMode === 'local') {
    const host = $('mapHost');
    host.style.height = Math.max(300, innerHeight - host.getBoundingClientRect().top - 16) + 'px';
    LocalMap.mount(host, String(open.appId));
  }
}

// The map is a separate web view laid over #mapHost, so keep it matched to that box and hide it
// whenever the page (or the palette, which it would cover) says so.
function syncMap() {
  if (state.view !== 'maps' && $('mapsBody').dataset.key && typeof LocalMap !== 'undefined') { LocalMap.unmount(); $('mapsBody').dataset.key = ''; }
  if (!invoke) return;
  const host = $('mapHost');
  if (state.view !== 'maps' || !mapOpen || mapMode !== 'web' || !host || pal) {
    if (mapShown) { mapShown = false; invoke('map_hide'); }
    return;
  }
  host.style.height = Math.max(200, innerHeight - host.getBoundingClientRect().top - 16) + 'px';
  const r = host.getBoundingClientRect();
  mapShown = true;
  invoke('map_show', { game: mapOpen, x: r.left, y: r.top, w: r.width, h: r.height }).catch(e => { mapShown = false; toast(String(e)); });
}
addEventListener('resize', () => {
  const host = $('mapHost');
  if (host && mapMode === 'local') host.style.height = Math.max(300, innerHeight - host.getBoundingClientRect().top - 16) + 'px';
  syncMap();
});

document.addEventListener('click', e => {
  const m = e.target.closest('[data-map]');
  if (m) { mapOpen = m.dataset.map; if (!WEB_MAPS.includes(mapOpen)) mapMode = 'local'; renderMaps(); syncMap(); }
  if (e.target.closest('[data-mapback]')) { mapOpen = null; renderMaps(); syncMap(); }
  const mm = e.target.closest('[data-mapmode]');
  if (mm) { mapMode = mm.dataset.mapmode; renderMaps(); syncMap(); }
});

function renderSettings() {
  setAmbient(null);
  const sw = (id, on) => `<button class="switch" role="switch" id="${id}" aria-checked="${on}"></button>`;
  const listeningPanic = capture && capture.mid === null;
  $('setBody').innerHTML = `
    <h1>Settings</h1>
    <div class="set-group"><h2>Appearance</h2>
      <div class="set-row"><div class="txt"><b>Accent color</b><span>Used for active mods, buttons and highlights.</span></div>
        <div class="swatches">${ACCENTS.map(a => `<button class="swatch${state.accent === a.c ? ' on' : ''}" style="--c:${a.c}" data-accent="${a.c}" aria-label="${a.name}" title="${a.name}"></button>`).join('')}</div></div>
      <div class="set-row"><div class="txt"><b>Show active mod counts</b><span>Badges next to games in the sidebar.</span></div>${sw('setCounts', state.showCounts)}</div>
      <div class="set-row"><div class="txt"><b>Show software in the sidebar</b><span>Tools and apps like OBS or 3DMark go in their own Software section under your games. They're always in Library under Software.</span></div>${sw('setSoftware', state.showSoftware)}</div>
    </div>
    <div class="set-group"><h2>Hotkeys</h2>
      <div class="set-row"><div class="txt"><b>Enable hotkeys</b><span>While a game with mods is running, its hotkeys work everywhere, including inside the game. They're released again when the game closes.</span></div>${sw('setHotkeys', state.hotkeys)}</div>
      <div class="set-row"><div class="txt"><b>Panic key</b><span>Turns off every mod in the running game at once. Click to change it.</span></div>
        <button class="key bindable${listeningPanic ? ' listening' : ''}" id="panicBind">${listeningPanic ? 'Press a key…' : esc(comboLabel(state.panicKey))}</button></div>
      ${invoke ? `<div class="set-tip"><b>Overlay tip:</b> HotSwap’s overlay is its own window on top of the game, so it never touches the game itself. Set the game’s display mode to <b>Borderless</b> or <b>Windowed</b>; in <b>exclusive fullscreen</b> the game covers the overlay and it won’t show.</div>` : ''}
      ${invoke ? `<div class="set-row"><div class="txt"><b>Cheat sheet key</b><span>Hold it in game to see every mod and its hotkey on the overlay. Click to change.</span></div>
        <button class="key bindable${capture?.mid === SHEET ? ' listening' : ''}${state.sheetKey ? '' : ' unbound'}" id="sheetBind">${capture?.mid === SHEET ? 'Press a key…' : esc(comboLabel(state.sheetKey))}</button></div>` : ''}
      ${invoke ? `<div class="set-row"><div class="txt"><b>Overlay menu key</b><span>Opens a menu over the game where you can flip mods with the mouse or arrow keys. Press it again or Esc to close.</span></div>
        <button class="btn" id="testMenu">Try it</button><button class="key bindable${capture?.mid === MENU ? ' listening' : ''}${state.menuKey ? '' : ' unbound'}" id="menuBind">${capture?.mid === MENU ? 'Press a key…' : esc(comboLabel(state.menuKey))}</button></div>` : ''}
      <div class="set-row"><div class="txt"><b>Sounds</b><span>A short chime when a mod turns on or off.</span></div>${sw('setSounds', state.sounds)}</div>
      ${invoke ? `<div class="set-row"><div class="txt"><b>In-game overlay</b><span>Shows mod changes from hotkeys at the top of the screen, over the game. Works with borderless and windowed games, not exclusive fullscreen.</span></div><button class="btn" id="testOverlay">Test</button>${sw('setOverlay', state.overlay)}</div>` : ''}
      ${invoke ? `<div class="set-row"><div class="txt"><b>Overlay position</b><span>Where mod changes and the cheat sheet show up.</span></div>${cornerPicker('ocorner', state.overlayCorner)}</div>
      <div class="set-row"><div class="txt"><b>HUD</b><span>A small bar over the game with your active mods, session time and clock. It keeps the overlay on screen, which can cost a little smoothness in some games.</span></div>${sw('setHud', state.hud)}</div>
      ${state.hud ? `<div class="set-row"><div class="txt"><b>HUD position</b><span>Where the HUD sits.</span></div>${cornerPicker('hcorner', state.hudCorner)}</div>
      <div class="set-row"><div class="txt"><b>CPU and RAM on the HUD</b><span>Your PC's total use, updated every second.</span></div>${sw('setHudStats', state.hudStats)}</div>` : ''}
      <div class="set-row"><div class="txt"><b>Radar position</b><span>Where the enemy radar sits, in games that have one.</span></div>${cornerPicker('rcorner', state.radarCorner)}</div>` : ''}
      <div class="set-row"><div class="txt"><b>Reset all key bindings</b><span>Puts every mod back on its default hotkey.</span></div><button class="btn" id="resetBinds">Reset</button></div>
    </div>
    ${invoke ? `<div class="set-group"><h2>Discord</h2>
      <div class="set-row"><div class="txt"><b>Show in Discord</b><span>Your Discord status shows the game you're playing with HotSwap and how many mods are on. Needs the Discord app running on this PC.${DISCORD_ERR ? ` <i>${esc(DISCORD_ERR)}.</i>` : ''}</span></div>${sw('setDiscord', state.discord)}</div>
    </div>` : ''}
    ${invoke ? `<div class="set-group"><h2>Startup and tray</h2>
      ${invoke ? `<div class="set-row"><div class="txt"><b>Low memory mode</b><span>Runs HotSwap as lean as it can: no graphics acceleration, fewer background processes, and the overlay closes soon after use. About 300 MB less memory. Animations cost a little more CPU. Takes effect after restarting HotSwap.</span></div>${sw('setLowMem', state.lowMemory)}</div>` : ''}
      <div class="set-row"><div class="txt"><b>Start with Windows</b><span>HotSwap opens quietly in the tray when you sign in, so hotkeys are ready when you start a game.</span></div>${sw('setAutostart', AUTOSTART)}</div>
      <div class="set-row"><div class="txt"><b>Close to tray</b><span>The close button hides HotSwap in the tray instead of quitting. Quit from the tray icon's menu.</span></div>${sw('setTray', state.closeToTray)}</div>
    </div>` : ''}
    <div class="set-group"><h2>Library</h2>
      <div class="set-row"><div class="txt"><b>Steam install</b><span>${esc(STEAM.root || 'Not found.')}</span></div><button class="btn" id="rescanBtn">Rescan</button></div>
      <div class="set-row"><div class="txt"><b>Other stores</b><span>${(() => {
        const n = s => LIB.filter(g => g.source === s).length;
        return `Epic Games: ${n('epic') ? `${n('epic')} found` : 'none installed'}. GOG: ${n('gog') ? `${n('gog')} found` : 'none installed'}.`;
      })()}</span></div></div>
      ${STEAM.root ? `<div class="set-row"><div class="txt"><b>Games and software</b><span>${gamesOnly().length} games and ${softwareOnly().length} software, sorted using Steam's own app labels${LIB.some(g => !g.steamType) ? ', with a list of common tools as a backup' : ''}.</span></div></div>` : ''}
      ${STEAM.libraries.length ? `<div class="set-row"><div class="txt"><b>Library folders</b><span>${STEAM.libraries.map(esc).join('<br>')}</span></div></div>` : ''}
    </div>
    <div class="set-group"><h2>Data</h2>
      <div class="set-row"><div class="txt"><b>Reset all mod settings</b><span>Turns every mod off in every profile of every game. Profiles, notes and favorites stay.</span></div><button class="btn danger" id="wipeBtn">Reset</button></div>
    </div>
    <div class="set-group"><h2>About and updates</h2>
      <div class="set-row"><div class="txt"><b>HotSwap ${esc(VERSION || '')}</b><span>${UPDATE ? `Version ${esc(UPDATE.version)} is available.` : esc(UPDATE_MSG || 'Free, open source, no ads.')} Game art is read from your local Steam cache. Figtree font under the SIL Open Font License.</span></div>
        ${invoke ? (UPDATE ? '<button class="btn" id="updateBtn">Update now</button>' : '<button class="btn" id="checkUpdateBtn">Check now</button>') : ''}</div>
      <div class="set-row"><div class="txt"><b>What's new</b><span>The release notes for this version.</span></div><button class="btn" id="whatsNewBtn">Show</button></div>
      <div class="set-row"><div class="txt"><b>Found a bug or want a game added?</b><span>Opens a GitHub issue with your HotSwap version already filled in.</span></div>
        <div class="btnrow"><a class="btn" href="${esc(bugUrl())}" ${ext}>Report a bug</a><a class="btn" href="${esc(requestUrl())}" ${ext}>Request a game</a></div></div>
      ${invoke ? `<div class="set-row"><div class="txt"><b>Check for updates automatically</b><span>Looks for a new release on GitHub when HotSwap starts. Updates are signed, and nothing installs without your click.</span></div>${sw('setAutoUpdate', state.autoUpdate)}</div>` : ''}
    </div>`;
}

function profileControl(g) {
  const list = profilesOf(g), cur = activeProf(g);
  const menu = !profileMenu ? '' : `<div class="pmenu" id="pmenu">
      ${list.map(p => `<div class="pitem${p === cur ? ' on' : ''}"><button data-profile="${esc(p.name)}">${p === cur ? '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12l5 5 9-10"/></svg>' : '<span class="sp"></span>'}${esc(p.name)}</button>
        ${list.length > 1 ? `<button class="pdel" data-delprofile="${esc(p.name)}" aria-label="Delete ${esc(p.name)}" title="Delete"><svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18"/></svg></button>` : ''}</div>`).join('')}
      <div class="psep"></div>
      <button class="padd" id="copyProfile"><svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 8V5a1 1 0 0 0-1-1H5a1 1 0 0 0-1 1v10a1 1 0 0 0 1 1h3"/></svg>Copy share code</button>
      ${importProfile
        ? `<form class="pnew" id="pimportForm"><input id="pimportCode" placeholder="Paste a share code" autocomplete="off" spellcheck="false"><button class="btn" type="submit">Add</button></form>`
        : `<button class="padd" id="importProfile"><svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M12 4v11M7 10l5 5 5-5M5 20h14"/></svg>Add from share code</button>`}
      ${newProfile
        ? `<form class="pnew" id="pnewForm"><input id="pnewName" maxlength="24" placeholder="Profile name" autocomplete="off"><button class="btn" type="submit">Save</button></form>`
        : `<button class="padd" id="addProfile"><svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round"><path d="M12 5v14M5 12h14"/></svg>Save current as new profile</button>`}
    </div>`;
  return `<div class="profile"><button class="pbtn" id="profileBtn" aria-expanded="${profileMenu}" title="Mod profile">
      <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><path d="M4 5h16v3H4zM4 10.5h16v3H4zM4 16h10v3H4z"/></svg>${esc(cur.name)}
      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"><path d="M6 9l6 6 6-6"/></svg></button>${menu}</div>`;
}

function renderGame() {
  const g = game();
  if (!g) { go('home'); return; }
  setAmbient(g.art.heroBlur || g.art.hero);
  const live = isLive(g), fav = state.fav[g.id], total = modsOf(g).length;
  const busy = launching?.id === g.id;
  const cover = g.art.capsule ? `<span class="cover">${img(g.art.capsule)}</span>` : `<span class="cover" style="background:${g.color}">${initials(g.name)}</span>`;
  $('gamehead').innerHTML = `
    ${cover}
    <div><div class="gh-title"><h1>${esc(g.name)}</h1>
      <button class="ghost${fav ? ' on' : ''}" id="favBtn" aria-pressed="${!!fav}" aria-label="Favorite" title="Favorite"><svg width="20" height="20" viewBox="0 0 24 24" fill="${fav ? 'currentColor' : 'none'}" stroke="currentColor" stroke-width="2.2" stroke-linejoin="round"><path d="M12 3l2.8 5.7 6.2.9-4.5 4.4 1 6.2L12 17.3 6.5 20.2l1-6.2L3 9.6l6.2-.9z"/></svg></button></div>
      <div class="gh-sub">${[
        g.software ? 'Software' : g.genre,
        g.software ? '' : g.anticheat.length ? 'Mods off' : total ? `${total} mods` : 'No mods yet',
        g.playtime ? `${hours(g.playtime)} played on Steam` : '',
        (() => { const ms = state.sessions.filter(s => s.g === g.id).reduce((t, s) => t + s.end - s.start, 0) + (live && sessionStart ? Date.now() - sessionStart : 0); return ms ? `${duration(ms)} played with HotSwap` : ''; })(),
        g.source !== 'steam' ? SOURCES[g.source] : '',
        g.solo ? 'Solo or private sessions only' : '',
      ].filter(Boolean).join(' · ')}</div>
      ${g.anticheat.length ? `<div class="ac-note"><svg width="13" height="13" viewBox="0 0 24 24" fill="currentColor"><path d="M12 2l8 3v6c0 5-3.4 9.4-8 11-4.6-1.6-8-6-8-11V5z"/></svg>Uses ${esc(g.anticheat.join(' and '))}. Modding it can get your account banned.</div>` : ''}</div>
    <div class="gh-actions">
      ${total ? `${profileControl(g)}
      <button class="roundbtn" id="resetBtn" aria-label="Turn off all mods" title="Turn off all mods"><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M3 12a9 9 0 1 0 3-6.7L3 8"/><path d="M3 3v5h5"/></svg></button>` : ''}
      <button class="roundbtn${state.panel ? ' on' : ''}" id="panelBtn" aria-pressed="${state.panel}" aria-label="Side panel" title="Side panel"><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4"><rect x="3" y="4" width="18" height="16" rx="3"/><path d="M15 4v16"/></svg></button>
      ${RUNNING === g.id && !live && invoke
        ? `<button class="play" id="injectBtn" title="Start HotSwap in the running game: mods, hotkeys and the session timer"><svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor"><path d="M13 2L4 14h7l-1 8 9-12h-7z"/></svg>Inject</button>`
        : live
        ? `<button class="play live" id="playBtn" title="${total ? 'Turn off all mods' : 'Running'}">Playing ${total ? '<span class="x" aria-hidden="true">✕</span>' : ''}</button>`
        : busy
          ? `<button class="play busy" disabled><span class="spin"></span>Launching</button>`
          : `<button class="play" id="playBtn"><svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor"><path d="M7 4.5v15a1 1 0 0 0 1.5.9l12-7.5a1 1 0 0 0 0-1.8l-12-7.5A1 1 0 0 0 7 4.5z"/></svg>Play</button>`}
    </div>`;
  $('content').classList.toggle('no-panel', !state.panel);
  $('activeCard').hidden = !total;
  renderChips(); renderMods(); renderPanel();
}

function renderChips() {
  const g = game();
  if (state.cat !== 'all' && !g.cats.some(c => c.name === state.cat)) state.cat = 'all';
  const chip = (k, label, n) => `<button class="chip${state.cat === k ? ' on' : ''}" data-chip="${esc(k)}">${icon(k === 'all' ? 'all' : k.toLowerCase(), 14)}${esc(label)}${n != null ? ` <span class="n">${n}</span>` : ''}</button>`;
  $('chips').innerHTML = g.cats.length ? chip('all', 'All') + g.cats.map(c => chip(c.name, c.name, c.mods.length)).join('') : '';
}

function keyChip(g, m, dir, clash) {
  const combo = keysOf(g, m)[dir];
  const listening = capture && capture.gid === g.id && capture.mid === m.id && capture.dir === dir;
  const arrow = m.type === 'slider' ? (dir === 'up' ? '↑ ' : '↓ ') : '';
  const cls = ['key', 'bindable', listening && 'listening', combo && clash.has(combo) && 'clash', !combo && 'unbound'].filter(Boolean).join(' ');
  const title = listening ? 'Press a key. Esc cancels, Backspace clears.' : combo && clash.has(combo) ? `${comboLabel(combo)} is used more than once` : 'Click to change';
  return `<button class="${cls}" data-bind="${m.id}" data-dir="${dir}" data-key="${esc(combo || '')}" title="${esc(title)}">${listening ? 'Press a key…' : arrow + esc(comboLabel(combo))}</button>`;
}

function modRow(g, m, clash, locked) {
  if (!m.live) locked = true;
  const v = valueOf(g, m), dis = locked ? ' disabled' : '';
  const info = m.info ? `<button class="info" data-tip="${esc(m.info)}" aria-label="${esc(m.info)}">i</button>` : '';
  let ctl;
  if (m.type === 'toggle') {
    ctl = `<button class="seg" role="switch" aria-checked="${!!v}" aria-label="${esc(m.name)}" data-flip="${m.id}"${dis}>
      <span class="${!v ? 'sel' : ''}">Off</span><span class="${v ? 'sel on' : ''}">On</span></button>`;
  } else if (m.type === 'slider') {
    ctl = `<div class="slider">
      <input type="number" id="n-${g.id}-${m.id}" data-num="${m.id}" min="${m.min}" max="${m.max}" step="${m.step}" value="${fmt(m, v)}" aria-label="${esc(m.name)} value"${dis}>
      <input type="range" id="r-${g.id}-${m.id}" data-range="${m.id}" title="Double-click to reset" min="${m.min}" max="${m.max}" step="${m.step}" value="${v}" style="--p:${pct(m, v)}" aria-label="${esc(m.name)}"${dis}></div>`;
  } else {
    ctl = `<button class="apply" data-act="${m.id}"${dis}>Apply</button>`;
  }
  const keys = keyChip(g, m, 'up', clash) + (m.type === 'slider' ? keyChip(g, m, 'down', clash) : '');
  const pinned = !!state.pinned[g.id]?.[m.id];
  return `<div class="row${isActive(g, m) ? ' active' : ''}${locked ? ' locked' : ''}" data-mod="${m.id}">
    <div class="label"><svg class="bolt" width="13" height="13" viewBox="0 0 24 24" fill="currentColor"><path d="M13 2L4 14h7l-1 8 9-12h-7z"/></svg>${esc(m.name)}${info}${m.live ? '' : '<span class="soon">Soon</span>'}${modDot(g, m)}
      <button class="pin${pinned ? ' on' : ''}" data-pin="${m.id}" aria-pressed="${pinned}" aria-label="${pinned ? 'Unpin' : 'Pin to top'}" title="${pinned ? 'Unpin' : 'Pin to top'}"><svg width="13" height="13" viewBox="0 0 24 24" fill="currentColor"><path d="M15 3l6 6-3 1-4 4 1 5-2 2-4-4-5 5H3v-1l5-5-4-4 2-2 5 1 4-4z"/></svg></button></div>
    ${ctl}<div class="keys">${keys}</div></div>`;
}

function renderMods() {
  const g = game(), q = query();
  if (g.anticheat.length && !g.software) {
    $('mods').innerHTML = `<div class="nomods">${img(g.art.logo, 'logo')}<h2>Mods are off for ${esc(g.name)}</h2>
      <p>This game uses ${esc(g.anticheat.join(' and '))}. Changing it while anti-cheat is running can get your account banned, so HotSwap doesn't offer mods here.</p></div>`;
    return;
  }
  if (!g.cats.length) {
    $('mods').innerHTML = g.software
      ? `<div class="nomods">${img(g.art.logo, 'logo')}<h2>${esc(g.name)} is software</h2>
        <p>Steam lists this as ${g.steamType ? `a${/^[aeiou]/.test(g.steamType) ? 'n' : ''} ${esc(g.steamType)}` : 'software'}, not a game, so HotSwap doesn't add mods to it.</p></div>`
      : `<div class="nomods">${img(g.art.logo, 'logo')}<h2>No mods for ${esc(g.name)} yet</h2>
        <p>Mods are added game by game. This one isn't in the catalog yet, but your notes and favorites still work.</p>
        <a class="btn accent" href="${esc(requestUrl(g))}" ${ext}>Request mods for ${esc(g.name)}</a></div>`;
    return;
  }
  const paused = isLive(g) && state.paused[g.id];
  const locked = !isLive(g) || paused, clash = clashes(g);
  const lockbar = locked ? `<div class="lockbar"><svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><path d="M7 10V7a5 5 0 0 1 10 0v3h1a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1v-9a1 1 0 0 1 1-1zm2 0h6V7a3 3 0 0 0-6 0z"/></svg>
      <span>${paused ? 'Mods are paused. Press the power button in the sidebar to turn them back on.' : `Start ${esc(g.name)} to change mods. You can set up hotkeys and profiles any time.`}</span></div>` : '';
  // Pinned mods get their own section at the top of All (and leave their category there).
  const pins = state.pinned[g.id] || {};
  const pinCat = state.cat === 'all' ? [{ name: 'Pinned', mods: modsOf(g).filter(m => pins[m.id]) }] : [];
  const cats = [...pinCat, ...g.cats.filter(c => state.cat === 'all' || c.name === state.cat)].map(c => {
    const mods = c.mods.filter(m => (!q || m.name.toLowerCase().includes(q) || c.name.toLowerCase().includes(q))
      && (c.name === 'Pinned' || state.cat !== 'all' || !pins[m.id]));
    if (!mods.length) return '';
    const key = g.id + '/' + c.name, closed = state.closed[key] && !q;
    return `<section class="cat${closed ? ' closed' : ''}">
      <button class="cat-head" data-cat="${esc(key)}" aria-expanded="${!closed}">${icon(c.name.toLowerCase(), 16)}${esc(c.name)}
        <svg class="chev" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M6 9l6 6 6-6"/></svg></button>
      <div class="rows">${mods.map(m => modRow(g, m, clash, locked)).join('')}</div></section>`;
  }).join('');
  $('mods').innerHTML = lockbar + (cats || `<p class="noresults">No mods match “${esc(q)}”.</p>`);
}

function renderPanel() {
  const g = game(), live = isLive(g), act = modsOf(g).filter(m => isActive(g, m));
  $('status').className = 'status' + (live ? ' live' : '');
  $('status').innerHTML = `<span class="d"></span>${live
    ? (modsOf(g).some(m => m.live) && invoke
        ? (TRAINER.connected ? 'Connected. Mods are live in the game.' : esc(TRAINER.message || 'Connecting to the game…'))
        : state.hotkeys ? 'Running. Hotkeys work inside the game.' : 'Running. Hotkeys are off in Settings.')
    : invoke ? 'Not running. Press Play to launch it through Steam.' : 'Press Play to start a session.'}`;
  $('activeList').innerHTML = act.length
    ? act.map(m => `<li><span>${esc(m.name)}</span><span>${m.type === 'toggle' ? 'On' : fmt(m, valueOf(g, m))}</span></li>`).join('')
    : `<p class="empty">Nothing on in the ${esc(activeProf(g).name)} profile.</p>`;
  const log = state.log.filter(l => l.g === g.id).slice(-6).reverse();
  $('log').innerHTML = log.length ? log.map(l => `<li><time>${l.t}</time>${esc(l.msg)}</li>`).join('') : '<p class="empty">No activity yet.</p>';

  const mine = state.sessions.filter(s => s.g === g.id);
  const total = mine.reduce((sum, s) => sum + (s.end - s.start), 0) + (live && sessionStart ? Date.now() - sessionStart : 0);
  const rows = mine.slice(-5).reverse().map(s => `<li><span>${new Date(s.start).toLocaleDateString('en-US', { month: 'short', day: 'numeric' })}, ${new Date(s.start).toTimeString().slice(0, 5)}</span><span>${duration(s.end - s.start)}</span></li>`);
  if (live && sessionStart) rows.unshift(`<li class="now-row"><span>Now playing</span><span>${duration(Date.now() - sessionStart)}</span></li>`);
  $('sessionsMeta').textContent = [g.playtime ? `${hours(g.playtime)} on Steam` : '', total ? `${duration(total)} with HotSwap` : ''].filter(Boolean).join(' · ');
  $('sessions').innerHTML = rows.length ? rows.join('') : '<p class="empty">Sessions show up here after you play with HotSwap open.</p>';
  const ta = $('notes');
  if (document.activeElement !== ta) ta.value = state.notes[g.id] || '';
}

function applyAccent() {
  const a = ACCENTS.find(x => x.c === state.accent) || ACCENTS[0];
  document.documentElement.style.setProperty('--accent', a.c);
  document.documentElement.style.setProperty('--accent-ink', a.ink);
}

// The Maps and Mods pages' code (map.js, mods.js) loads the first time the page opens, not at
// startup, so it takes no memory until it's used.
const lazyScripts = {};
function needScript(src) {
  return lazyScripts[src] ||= new Promise((ok, fail) => {
    const s = document.createElement('script');
    s.src = src; s.onload = ok; s.onerror = () => { delete lazyScripts[src]; fail(new Error(`Couldn't load ${src}`)); };
    document.head.append(s);
  });
}
function lazyPage(body, src) {
  $(body).innerHTML = '<p class="empty">Loading…</p>';
  $(body).dataset.view = $(body).dataset.key = '';
  needScript(src).then(render).catch(e => toast(String(e), ''));
}
var lastView = null;

function render() {
  applyAccent();
  // Leaving the Mods page frees its mod lists, here and in the app (Thunderstore's is big).
  if (lastView === 'mods' && state.view !== 'mods' && typeof Mods !== 'undefined') { Mods.release(); invoke?.('mods_release').catch(() => {}); }
  lastView = state.view;
  const views = { home: 'homeView', library: 'libView', maps: 'mapsView', mods: 'modsView', overlay: 'ovView', settings: 'setView', game: 'gameView' };
  if (state.view === 'game' && !game()) state.view = 'home';
  Object.entries(views).forEach(([v, id]) => $(id).hidden = v !== state.view);
  renderSide();
  ({ home: renderHome, library: renderLibrary, maps: renderMaps, mods: () => typeof Mods === 'undefined' ? lazyPage('modsBody', 'mods.js') : Mods.render(), overlay: renderOverlayPage, settings: renderSettings, game: renderGame })[state.view]();
  if (state.mini) renderMini();
  syncHotkeys();
  syncTrainer();
  syncMap();
  pushMenu();
  syncDiscord();
  dcOptions();
}
