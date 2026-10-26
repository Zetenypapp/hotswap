// Tinker UI. Runs inside the Tauri window; also opens in a plain browser with demo data for quick styling work.

const tauri = window.__TAURI__;
const invoke = tauri?.core?.invoke;
const listen = tauri?.event?.listen;
const fileSrc = p => (p && tauri?.core?.convertFileSrc ? tauri.core.convertFileSrc(p) : null);
const appWin = tauri?.window?.getCurrentWindow?.();

const $ = id => document.getElementById(id);
const esc = s => String(s ?? '').replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
const initials = n => n.replace(/[^A-Za-z0-9 ]/g, '').split(' ').filter(Boolean).slice(0, 2).map(w => w[0]).join('').toUpperCase();
const hue = id => `hsl(${(id * 47) % 360} 55% 55%)`;
const steamCdn = (app, file) => `https://cdn.akamai.steamstatic.com/steam/apps/${app}/${file}`;
const img = (src, cls = '') => src ? `<img class="${cls}" src="${esc(src)}" alt="" loading="lazy" onerror="this.remove()">` : '';
const ext = 'target="_blank" rel="noopener"';
const money = n => '$' + n.toLocaleString('en-US', { minimumFractionDigits: n >= 1000 ? 0 : 2, maximumFractionDigits: 2 });
const shortDate = d => new Date(d + 'T12:00:00').toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
const gb = bytes => bytes ? (bytes / 1073741824).toFixed(bytes > 10737418240 ? 0 : 1) + ' GB' : '';
const clock = () => new Date().toTimeString().slice(0, 5);

const ACCENTS = [
  { name: 'Amber', c: '#f3b73f', ink: '#1d1404' },
  { name: 'Mint', c: '#3fd49a', ink: '#05170f' },
  { name: 'Sky', c: '#5cb8ff', ink: '#04121d' },
  { name: 'Rose', c: '#ff7a93', ink: '#1f060b' },
  { name: 'Violet', c: '#a493ff', ink: '#0f0a24' },
];

/* ---------- Hotkey combos ----------
   A combo is modifiers then a KeyboardEvent.code, joined by "+": "F1", "Shift+F9", "Ctrl+Alt+KeyK".
   Toggles and actions have one key ("up"); sliders also have a "down" key. */
const KEY_POOL = ['F1', 'F2', 'F3', 'F4', 'F6', 'F7', 'F8', 'F9', 'F10', 'Numpad1', 'Numpad2', 'Numpad3', 'Numpad4', 'Numpad5', 'Numpad6', 'Numpad7', 'Numpad8', 'Numpad9'];
const MODIFIER_CODES = /^(Control|Shift|Alt|Meta|OS)(Left|Right)?$/;
const keyName = code => code
  .replace(/^Key/, '').replace(/^Digit/, '').replace(/^Numpad/, 'Num ')
  .replace(/^Arrow(\w+)/, '$1 arrow').replace('Backquote', '`').replace('Minus', '-').replace('Equal', '=');
const comboLabel = c => c ? c.split('+').map(keyName).join(' ') : 'Unbound';
function comboFromEvent(e) {
  if (MODIFIER_CODES.test(e.code)) return null;
  return [e.ctrlKey && 'Ctrl', e.altKey && 'Alt', e.shiftKey && 'Shift', e.code].filter(Boolean).join('+');
}

const DEFAULTS = {
  view: 'home', game: null, playing: null, fav: {}, closed: {}, notes: {}, log: [],
  cat: 'all', libFilter: 'all', sortAZ: false, panel: true, accent: '#f3b73f', showCounts: true, hotkeys: true,
  showSoftware: true, softOpen: false,
  binds: {}, panicKey: 'Ctrl+Numpad0', profiles: {}, activeProfile: {}, sessions: [],
};
let state = { ...DEFAULTS };
let LIB = [];
let STEAM = { root: null, libraries: [] };
let RUNNING = null;          // game ID reported by the watcher (desktop app only)
let sessionStart = null;     // when the current RUNNING session began
let launching = null;        // { id, timer } while waiting for a launched game to appear
let capture = null;          // { gid, mid, dir } while rebinding a key; mid is null for the panic key
let profileMenu = false;     // profile popover open
let newProfile = false;      // profile popover showing the name field

/* ---------- Persistence ---------- */
async function loadState() {
  try {
    const raw = invoke ? await invoke('load_state') : localStorage.getItem('tinker');
    if (raw) state = { ...DEFAULTS, ...JSON.parse(raw.replace(/^﻿/, '')) };
  } catch { /* first launch or unreadable file: keep defaults */ }
  // Log lines saved while app.js was briefly mis-encoded have a mangled arrow.
  state.log.forEach(l => { l.msg = l.msg.replace(/â†’/g, '→'); });
  // Older saves kept mod values per game; they now live in the game's Default profile.
  if (state.values) {
    for (const [gid, v] of Object.entries(state.values)) state.profiles[gid] ||= [{ name: 'Default', values: v }];
    delete state.values;
  }
}
let saveTimer;
function save() {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(() => {
    const data = JSON.stringify(state);
    if (invoke) invoke('save_state', { data }).catch(err => console.error('save failed', err));
    else try { localStorage.setItem('tinker', data); } catch { }
  }, 250);
}

/* ---------- Library ---------- */
function buildGame(raw) {
  const c = CATALOG[raw.appId];
  const art = raw.art || {};
  const g = {
    id: String(raw.appId), appId: raw.appId, name: raw.name, lastPlayed: raw.lastPlayed || 0, size: raw.sizeOnDisk || 0,
    software: raw.kind === 'software', steamType: raw.steamType || null, anticheat: raw.anticheat || [],
    genre: c?.genre || '', solo: !!c?.solo, color: hue(raw.appId),
    // Anti-cheat games never get mods, even if the catalog has some.
    cats: raw.anticheat?.length ? [] : c?.cats || [],
    art: {
      icon: fileSrc(art.icon), logo: fileSrc(art.logo), hero: fileSrc(art.hero), heroBlur: fileSrc(art.heroBlur),
      capsule: fileSrc(art.capsule) || (invoke ? null : steamCdn(raw.appId, 'library_600x900.jpg')),
      header: fileSrc(art.header) || steamCdn(raw.appId, 'header.jpg'),
    },
  };
  if (!invoke) { g.art.hero = steamCdn(raw.appId, 'library_hero.jpg'); g.art.logo = steamCdn(raw.appId, 'logo.png'); }
  let i = 0;
  g.cats.forEach(cat => cat.mods.forEach(m => {
    m.id = m.name.toLowerCase().replace(/[^a-z0-9]+/g, '-');
    m.key = KEY_POOL[i++ % KEY_POOL.length];
  }));
  return g;
}

async function scanLibrary() {
  if (invoke) {
    try {
      const scan = await invoke('scan_steam');
      STEAM = { root: scan.root, libraries: scan.libraries };
      LIB = scan.games.map(buildGame);
      return;
    } catch (err) { console.error('Steam scan failed', err); }
  }
  LIB = DEMO_LIBRARY.map(buildGame);
}

const modsOf = g => g.cats.flatMap(c => c.mods);
const gamesOnly = () => LIB.filter(g => !g.software);
const softwareOnly = () => LIB.filter(g => g.software);
const findGame = id => LIB.find(g => g.id === String(id));
const game = () => findGame(state.game) || LIB[0];
// In the desktop app "running" comes from the watcher; in a browser preview Play simulates it.
const runningId = () => invoke ? RUNNING : state.playing;
const isLive = g => !!g && runningId() === g.id;

/* ---------- Profiles: each game has named sets of mod values ---------- */
function profilesOf(g) {
  return (state.profiles[g.id] ||= [{ name: 'Default', values: {} }]);
}
function activeProf(g) {
  const list = profilesOf(g);
  return list.find(p => p.name === state.activeProfile[g.id]) || list[0];
}
const vals = g => (activeProf(g).values ||= {});
const valueOf = (g, m) => vals(g)[m.id] ?? (m.type === 'slider' ? m.def : false);
const isActive = (g, m) => m.type === 'toggle' ? valueOf(g, m) === true : m.type === 'slider' ? valueOf(g, m) !== m.def : false;
const fmt = (m, v) => Number.isInteger(m.step) ? String(v) : v.toFixed(1);
const pct = (m, v) => ((v - m.min) / (m.max - m.min) * 100).toFixed(1) + '%';
const activeCount = g => modsOf(g).filter(m => isActive(g, m)).length;
const query = () => $('search').value.trim().toLowerCase();

/* ---------- Key bindings ---------- */
function keysOf(g, m) {
  const custom = state.binds[g.id]?.[m.id] || {};
  return {
    up: 'up' in custom ? custom.up : m.key,
    down: m.type !== 'slider' ? null : 'down' in custom ? custom.down : 'Shift+' + m.key,
  };
}
// Combos used more than once in a game (the panic key counts too).
function clashes(g) {
  const seen = new Map([[state.panicKey, 1]]);
  for (const m of modsOf(g)) for (const k of Object.values(keysOf(g, m))) if (k) seen.set(k, (seen.get(k) || 0) + 1);
  return new Set([...seen].filter(([, n]) => n > 1).map(([k]) => k));
}
function whoUses(g, combo, except) {
  if (combo === state.panicKey && except !== 'panic') return 'the panic key';
  for (const m of modsOf(g)) {
    for (const [dir, k] of Object.entries(keysOf(g, m))) if (k === combo && `${m.id}/${dir}` !== except) return m.name;
  }
  return null;
}

let lastHotkeys = '';
function syncHotkeys() {
  if (!invoke) return;
  const g = findGame(RUNNING);
  const keys = !capture && state.hotkeys && g && modsOf(g).length
    ? [...new Set([state.panicKey, ...modsOf(g).flatMap(m => Object.values(keysOf(g, m)))].filter(Boolean))]
    : [];
  const sig = keys.join('|');
  if (sig === lastHotkeys) return;
  lastHotkeys = sig;
  invoke('set_hotkeys', { keys }).then(failed => {
    if (failed.length) toast(`Couldn't use ${failed.map(comboLabel).join(', ')}`, 'another app has it');
  }).catch(console.error);
}

/* ---------- Tiles ---------- */
function tile(g, cls = '', withRun = true) {
  const run = withRun && isLive(g) ? ' run' : '';
  return g.art.icon
    ? `<span class="tile ${cls}${run}">${img(g.art.icon)}</span>`
    : `<span class="tile ${cls}${run}" style="background:${g.color}">${initials(g.name)}</span>`;
}
const capsule = g => g.art.capsule
  ? `<span class="fallback">${esc(g.name)}</span>${img(g.art.capsule)}`
  : `<span class="fallback" style="background:linear-gradient(160deg, ${g.color}, #1a1d22)">${esc(g.name)}</span>`;

/* ---------- Navigation ---------- */
const nav = { stack: [], i: -1 };
function go(view, gameId, push = true) {
  state.view = view;
  if (gameId != null) state.game = String(gameId);
  profileMenu = newProfile = false;
  if (push) {
    nav.stack = nav.stack.slice(0, nav.i + 1);
    nav.stack.push({ view, game: state.game });
    nav.i = nav.stack.length - 1;
  }
  save();
  render();
  document.querySelectorAll('.scroll, .mods').forEach(el => el.scrollTop = 0);
}
function step(d) {
  const t = nav.stack[nav.i + d];
  if (!t) return;
  nav.i += d;
  go(t.view, t.game, false);
}

/* ---------- Render ---------- */
function setAmbient(art) {
  $('ambArt').classList.toggle('show', !!art);
  if (art) $('ambArt').style.backgroundImage = `url("${art}")`;
  $('ambGlow').classList.toggle('show', !art);
}

function renderSide() {
  // The sidebar glow shows the running game's art; with nothing running it follows the accent color.
  const run = findGame(runningId());
  const artSrc = run && (run.art.heroBlur || run.art.hero);
  $('sideArt').classList.toggle('accent', !artSrc);
  $('sideArt').style.backgroundImage = artSrc ? `url("${artSrc}")` : '';
  $('nowCard').innerHTML = run
    ? `<div class="now">${tile(run, 'md', false)}<span class="name">${esc(run.name)}</span>
        <button class="circle" data-game="${run.id}" aria-label="Open ${esc(run.name)}" title="Open"><svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><circle cx="5" cy="12" r="2"/><circle cx="12" cy="12" r="2"/><circle cx="19" cy="12" r="2"/></svg></button>
        ${modsOf(run).length ? `<button class="circle stop" id="stopBtn" aria-label="Turn off all mods" title="Turn off all mods (${esc(comboLabel(state.panicKey))})"><svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor"><rect x="3" y="3" width="18" height="18" rx="4"/></svg></button>` : ''}</div>`
    : `<div class="now idle">No game running</div>`;
  $('subnav').hidden = !run;

  const q = query();
  const pick = src => {
    let list = src.filter(x => !q || x.name.toLowerCase().includes(q) || modsOf(x).some(m => m.name.toLowerCase().includes(q)));
    return state.sortAZ ? [...list].sort((a, b) => a.name.localeCompare(b.name)) : list;
  };
  const row = x => {
    const n = state.showCounts ? activeCount(x) : 0;
    return `<button class="gamebtn${x.id === state.game && state.view === 'game' ? ' on' : ''}" data-game="${x.id}">${tile(x)}<span class="name">${esc(x.name)}</span>${n ? `<span class="count">${n}</span>` : ''}</button>`;
  };
  const games = pick(gamesOnly()), soft = pick(softwareOnly());
  $('gameCount').textContent = gamesOnly().length;
  let html = games.map(row).join('') || '<p class="empty" style="padding:6px 10px">No games match.</p>';
  if (state.showSoftware && soft.length) {
    const open = state.softOpen || !!q;
    html += `<button class="soft-head" id="softToggle" aria-expanded="${open}">Software <span class="n">${soft.length}</span>
      <svg class="chev" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"><path d="M6 9l6 6 6-6"/></svg></button>
      ${open ? soft.map(row).join('') : ''}`;
  }
  $('games').innerHTML = html;

  document.querySelectorAll('[data-view]').forEach(b => b.classList.toggle('on', b.dataset.view === state.view));
  $('backBtn').disabled = nav.i <= 0;
  $('fwdBtn').disabled = nav.i >= nav.stack.length - 1;
}

function renderHome() {
  setAmbient(null);
  const h = new Date().getHours();
  const hi = h < 5 ? 'Late night session?' : h < 12 ? 'Good morning' : h < 18 ? 'Good afternoon' : 'Good evening';
  const q = query();
  const match = t => !q || t.toLowerCase().includes(q);

  // Anti-cheat games can't be modded, so they never show up as a place to jump back in.
  const recent = gamesOnly().filter(g => !g.anticheat.length && match(g.name)).sort((a, b) => b.lastPlayed - a.lastPlayed).slice(0, 6);
  const jump = recent.map(g => {
    const n = activeCount(g), total = modsOf(g).length;
    return `<button data-game="${g.id}" title="${esc(g.name)}">${capsule(g)}
      ${total ? `<span class="badge">${n ? `<b>${n}</b> active` : `${total} mods`}</span>` : ''}</button>`;
  }).join('');

  const news = NEWS.filter(n => {
    const g = findGame(n.app);
    return match(n.title) || (g && match(g.name));
  }).map(n => {
    const g = findGame(n.app);
    const name = g?.name || DEMO_LIBRARY.find(d => d.appId === n.app)?.name || '';
    const feature = n.feature && !q;
    const bg = g?.art.hero || g?.art.header;
    const who = feature && g?.art.logo ? img(g.art.logo, 'logo') : `<span class="who">${g ? tile(g, '', false) : ''}${esc(name)}</span>`;
    return `<a class="news${feature ? ' feature' : ''}" style="--c:${g?.color || hue(n.app)}" href="https://store.steampowered.com/news/app/${n.app}" ${ext}>
      ${bg ? `<span class="bg" style="background-image:url('${esc(bg)}')"></span><span class="shade"></span>` : ''}
      ${who}<h3>${esc(n.title)}</h3>${n.text && feature ? `<p>${esc(n.text)}</p>` : ''}
      <span class="src">${shortDate(n.date)} · ${esc(n.src)}</span></a>`;
  }).join('');

  const deals = DEALS.filter(d => match(d.t)).map(d => `<a class="deal" style="--c:${hue(d.app)}" href="https://store.steampowered.com/app/${d.app}/" ${ext}>
      <span class="art">${img(steamCdn(d.app, 'header.jpg'))}<span class="off">−${d.off}%</span></span>
      <span class="body"><span class="t">${esc(d.t)}</span>
      <span class="price"><b>${money(d.sale)}</b><s>${money(d.was)}</s></span>
      <span class="rating">${esc(d.rating)}</span></span></a>`).join('');

  const rankList = list => `<ol class="rank">${list.filter(x => match(x.t)).map((x, i) => `<li><a href="https://store.steampowered.com/app/${x.app}/" ${ext}>
      <span class="i">${i + 1}</span><span class="thumb">${img(steamCdn(x.app, 'header.jpg'), 'thumb')}</span>
      <span class="nm">${esc(x.t)}${x.note ? `<small>${x.note}</small>` : ''}</span>
      <span class="pr">${x.off ? `<em>−${x.off}%</em>` : ''}${money(x.price)}</span></a></li>`).join('') || '<li class="empty" style="padding:10px">No matches.</li>'}</ol>`;

  $('homeBody').innerHTML = `
    <div class="hello"><h1>${hi}</h1><p>Pick up where you left off, or see what's new with your games.</p></div>
    ${jump ? `<section><div class="sec-head"><h2>Jump back in</h2><span>Recently played</span></div><div class="jump">${jump}</div></section>` : ''}
    <section><div class="sec-head"><h2>From your games</h2><span>Steam news for games in your library</span></div>
      <div class="news-grid">${news || '<p class="empty">No news matches your search.</p>'}</div></section>
    <div class="cols">
      <section><div class="sec-head"><h2>Hot deals</h2><span>Top-rated Steam discounts</span><a href="https://www.cheapshark.com/" ${ext}>More on CheapShark →</a></div>
        <div class="deal-grid">${deals || '<p class="empty">No deals match your search.</p>'}</div></section>
      <section>
        <div class="sec-head"><h2>Top sellers</h2><a href="https://steamdb.info/charts/" ${ext}>SteamDB charts →</a></div>
        <div class="listcard">${rankList(TOP)}</div>
        <div class="sec-head"><h2>New on Steam</h2></div>
        <div class="listcard">${rankList(NEW_RELEASES)}</div>
      </section>
    </div>
    <p class="stamp">News and prices are a snapshot from ${SNAPSHOT} (Steam, CheapShark). Live updates are coming in a later version.</p>`;
}

function renderLibrary() {
  setAmbient(null);
  const q = query();
  const soft = softwareOnly();
  const filters = [['all', 'All games'], ['modded', 'Has mods'], ['fav', 'Favorites'], ...(soft.length ? [['software', 'Software']] : [])];
  if (state.libFilter === 'software' && !soft.length) state.libFilter = 'all';
  const list = (state.libFilter === 'software' ? soft : gamesOnly()).filter(g => (!q || g.name.toLowerCase().includes(q))
    && (state.libFilter !== 'modded' || modsOf(g).length)
    && (state.libFilter !== 'fav' || state.fav[g.id]));
  const nGames = gamesOnly().length;
  $('libBody').innerHTML = `
    <div class="lib-head"><h1>Library</h1><span class="meta">${nGames} ${nGames === 1 ? 'game' : 'games'}${soft.length ? ` + ${soft.length} software` : ''}${STEAM.libraries.length ? ` across ${STEAM.libraries.length} Steam ${STEAM.libraries.length === 1 ? 'folder' : 'folders'}` : ''}</span>
      <div class="chips">${filters.map(([k, l]) => `<button class="chip${state.libFilter === k ? ' on' : ''}" data-libfilter="${k}">${l}</button>`).join('')}</div></div>
    <div class="lib-grid">${list.map(g => {
      const n = activeCount(g), total = modsOf(g).length;
      return `<button class="lib-item" data-game="${g.id}"><span class="art">${capsule(g)}${n ? `<span class="modded">${n} active</span>` : ''}</span>
        <span class="nm">${esc(g.name)}</span><span class="sub">${g.software ? 'Software' : g.anticheat.length ? 'Anti-cheat' : total ? `${total} mods` : 'No mods yet'}${g.size ? ` · ${gb(g.size)}` : ''}</span></button>`;
    }).join('') || '<p class="empty">Nothing here yet.</p>'}</div>`;
}

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
      <div class="set-row"><div class="txt"><b>Reset all key bindings</b><span>Puts every mod back on its default hotkey.</span></div><button class="btn" id="resetBinds">Reset</button></div>
    </div>
    <div class="set-group"><h2>Steam</h2>
      <div class="set-row"><div class="txt"><b>Steam install</b><span>${esc(STEAM.root || 'Not found. Showing demo games.')}</span></div><button class="btn" id="rescanBtn">Rescan</button></div>
      ${STEAM.root ? `<div class="set-row"><div class="txt"><b>Games and software</b><span>${gamesOnly().length} games and ${softwareOnly().length} software, sorted using Steam's own app labels${LIB.some(g => !g.steamType) ? ', with a list of common tools as a backup' : ''}.</span></div></div>` : ''}
      ${STEAM.libraries.length ? `<div class="set-row"><div class="txt"><b>Library folders</b><span>${STEAM.libraries.map(esc).join('<br>')}</span></div></div>` : ''}
    </div>
    <div class="set-group"><h2>Data</h2>
      <div class="set-row"><div class="txt"><b>Reset all mod settings</b><span>Turns every mod off in every profile of every game. Profiles, notes and favorites stay.</span></div><button class="btn danger" id="wipeBtn">Reset</button></div>
    </div>
    <div class="set-group"><h2>About</h2>
      <div class="set-row"><div class="txt"><b>Tinker 0.1.0</b><span>Interface preview. Mods don't change games yet. Game art is read from your local Steam cache. Figtree font under the SIL Open Font License.</span></div></div>
    </div>`;
}

function profileControl(g) {
  const list = profilesOf(g), cur = activeProf(g);
  const menu = !profileMenu ? '' : `<div class="pmenu" id="pmenu">
      ${list.map(p => `<div class="pitem${p === cur ? ' on' : ''}"><button data-profile="${esc(p.name)}">${p === cur ? '<svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><path d="M5 12l5 5 9-10"/></svg>' : '<span class="sp"></span>'}${esc(p.name)}</button>
        ${list.length > 1 ? `<button class="pdel" data-delprofile="${esc(p.name)}" aria-label="Delete ${esc(p.name)}" title="Delete"><svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><path d="M6 6l12 12M18 6L6 18"/></svg></button>` : ''}</div>`).join('')}
      <div class="psep"></div>
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
      <div class="gh-sub">${[g.software ? 'Software' : g.genre, g.software ? '' : g.anticheat.length ? 'Mods off' : total ? `${total} mods` : 'No mods yet', g.solo ? 'Solo or private sessions only' : ''].filter(Boolean).join(' · ')}</div>
      ${g.anticheat.length ? `<div class="ac-note"><svg width="13" height="13" viewBox="0 0 24 24" fill="currentColor"><path d="M12 2l8 3v6c0 5-3.4 9.4-8 11-4.6-1.6-8-6-8-11V5z"/></svg>Uses ${esc(g.anticheat.join(' and '))}. Modding it can get your account banned.</div>` : ''}</div>
    <div class="gh-actions">
      ${total ? `${profileControl(g)}
      <button class="roundbtn" id="resetBtn" aria-label="Turn off all mods" title="Turn off all mods"><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M3 12a9 9 0 1 0 3-6.7L3 8"/><path d="M3 3v5h5"/></svg></button>
      <button class="roundbtn${state.panel ? ' on' : ''}" id="panelBtn" aria-pressed="${state.panel}" aria-label="Side panel" title="Side panel"><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4"><rect x="3" y="4" width="18" height="16" rx="3"/><path d="M15 4v16"/></svg></button>` : ''}
      ${live
        ? `<button class="play live" id="playBtn" title="${total ? 'Turn off all mods' : 'Running'}">Playing ${total ? '<span class="x" aria-hidden="true">✕</span>' : ''}</button>`
        : busy
          ? `<button class="play busy" disabled><span class="spin"></span>Launching</button>`
          : `<button class="play" id="playBtn"><svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor"><path d="M7 4.5v15a1 1 0 0 0 1.5.9l12-7.5a1 1 0 0 0 0-1.8l-12-7.5A1 1 0 0 0 7 4.5z"/></svg>Play</button>`}
    </div>`;
  $('content').classList.toggle('no-panel', !state.panel || !total);
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
  const v = valueOf(g, m), dis = locked ? ' disabled' : '';
  const info = m.info ? `<button class="info" data-tip="${esc(m.info)}" aria-label="${esc(m.info)}">i</button>` : '';
  let ctl;
  if (m.type === 'toggle') {
    ctl = `<div class="seg" role="group" aria-label="${esc(m.name)}">
      <button class="${!v ? 'sel' : ''}" data-set="${m.id}" data-v="0"${dis}>Off</button>
      <button class="${v ? 'sel on' : ''}" data-set="${m.id}" data-v="1"${dis}>On</button></div>`;
  } else if (m.type === 'slider') {
    ctl = `<div class="slider">
      <input type="number" id="n-${g.id}-${m.id}" data-num="${m.id}" min="${m.min}" max="${m.max}" step="${m.step}" value="${fmt(m, v)}" aria-label="${esc(m.name)} value"${dis}>
      <input type="range" id="r-${g.id}-${m.id}" data-range="${m.id}" min="${m.min}" max="${m.max}" step="${m.step}" value="${v}" style="--p:${pct(m, v)}" aria-label="${esc(m.name)}"${dis}></div>`;
  } else {
    ctl = `<button class="apply" data-act="${m.id}"${dis}>Apply</button>`;
  }
  const keys = keyChip(g, m, 'up', clash) + (m.type === 'slider' ? keyChip(g, m, 'down', clash) : '');
  return `<div class="row${isActive(g, m) ? ' active' : ''}${locked ? ' locked' : ''}">
    <div class="label"><svg class="bolt" width="13" height="13" viewBox="0 0 24 24" fill="currentColor"><path d="M13 2L4 14h7l-1 8 9-12h-7z"/></svg>${esc(m.name)}${info}</div>
    ${ctl}<div class="keys">${keys}</div></div>`;
}

function renderMods() {
  const g = game(), q = query();
  if (g.anticheat.length && !g.software) {
    $('mods').innerHTML = `<div class="nomods">${img(g.art.logo, 'logo')}<h2>Mods are off for ${esc(g.name)}</h2>
      <p>This game uses ${esc(g.anticheat.join(' and '))}. Changing it while anti-cheat is running can get your account banned, so Tinker doesn't offer mods here.</p></div>`;
    return;
  }
  if (!g.cats.length) {
    $('mods').innerHTML = g.software
      ? `<div class="nomods">${img(g.art.logo, 'logo')}<h2>${esc(g.name)} is software</h2>
        <p>Steam lists this as ${g.steamType ? `a${/^[aeiou]/.test(g.steamType) ? 'n' : ''} ${esc(g.steamType)}` : 'software'}, not a game, so Tinker doesn't add mods to it.</p></div>`
      : `<div class="nomods">${img(g.art.logo, 'logo')}<h2>No mods for ${esc(g.name)} yet</h2>
        <p>Mods are added game by game. This one isn't in the catalog yet, but your notes and favorites still work.</p></div>`;
    return;
  }
  const locked = !isLive(g), clash = clashes(g);
  const lockbar = locked ? `<div class="lockbar"><svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><path d="M7 10V7a5 5 0 0 1 10 0v3h1a1 1 0 0 1 1 1v9a1 1 0 0 1-1 1H6a1 1 0 0 1-1-1v-9a1 1 0 0 1 1-1zm2 0h6V7a3 3 0 0 0-6 0z"/></svg>
      <span>Start ${esc(g.name)} to change mods. You can set up hotkeys and profiles any time.</span></div>` : '';
  const cats = g.cats.filter(c => state.cat === 'all' || c.name === state.cat).map(c => {
    const mods = c.mods.filter(m => !q || m.name.toLowerCase().includes(q) || c.name.toLowerCase().includes(q));
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
    ? (state.hotkeys ? 'Running. Hotkeys work inside the game.' : 'Running. Hotkeys are off in Settings.')
    : invoke ? 'Not running. Press Play to launch it through Steam.' : 'Press Play to start a session.'}`;
  $('activeList').innerHTML = act.length
    ? act.map(m => `<li><span>${esc(m.name)}</span><span>${m.type === 'toggle' ? 'On' : fmt(m, valueOf(g, m))}</span></li>`).join('')
    : `<p class="empty">Nothing on in the ${esc(activeProf(g).name)} profile.</p>`;
  const log = state.log.filter(l => l.g === g.id).slice(-6).reverse();
  $('log').innerHTML = log.length ? log.map(l => `<li><time>${l.t}</time>${esc(l.msg)}</li>`).join('') : '<p class="empty">No activity yet.</p>';
  const ta = $('notes');
  if (document.activeElement !== ta) ta.value = state.notes[g.id] || '';
}

function applyAccent() {
  const a = ACCENTS.find(x => x.c === state.accent) || ACCENTS[0];
  document.documentElement.style.setProperty('--accent', a.c);
  document.documentElement.style.setProperty('--accent-ink', a.ink);
}

function render() {
  applyAccent();
  const views = { home: 'homeView', library: 'libView', maps: 'mapsView', settings: 'setView', game: 'gameView' };
  if (state.view === 'game' && !game()) state.view = 'home';
  Object.entries(views).forEach(([v, id]) => $(id).hidden = v !== state.view);
  renderSide();
  ({ home: renderHome, library: renderLibrary, maps: () => setAmbient(null), settings: renderSettings, game: renderGame })[state.view]();
  syncHotkeys();
}

/* ---------- Actions ---------- */
let toastTimer;
function toast(name, val) {
  $('toast').innerHTML = `${esc(name)}${val ? ` <b>${esc(val)}</b>` : ''}`;
  $('toast').classList.add('show');
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => $('toast').classList.remove('show'), 1800);
}
function logLine(g, msg) {
  state.log.push({ g: g.id, t: clock(), msg });
  state.log = state.log.slice(-150);
}

function setMod(g, m, v, how) {
  if (!isLive(g)) { toast(`Start ${g.name} first`, ''); return; }
  let shown;
  if (m.type === 'slider') {
    v = Math.min(m.max, Math.max(m.min, Math.round(v / m.step) * m.step));
    vals(g)[m.id] = v; shown = fmt(m, v);
  } else if (m.type === 'toggle') {
    vals(g)[m.id] = v; shown = v ? 'On' : 'Off';
  } else shown = 'applied';
  logLine(g, `${m.name} → ${shown}${how ? ' (' + how + ')' : ''}`);
  save();
  toast(m.name, shown);
  renderSide();
  if (state.view === 'game' && state.game === g.id) renderGame();
}

// Panic key, the Playing pill's ✕ and the sidebar stop button: every mod off in the running game.
function panic(how) {
  const g = findGame(runningId()) || game();
  if (!g || !modsOf(g).length) return;
  activeProf(g).values = {};
  logLine(g, `All mods → Off${how ? ' (' + how + ')' : ''}`);
  save();
  toast('All mods', 'off');
  render();
}

function play(g) {
  if (!invoke) { state.playing = state.playing === g.id ? null : g.id; save(); render(); return; }
  clearTimeout(launching?.timer);
  launching = {
    id: g.id,
    timer: setTimeout(() => {
      launching = null; render();
      toast(`${g.name} didn't start`, 'is Steam running?');
    }, 60000),
  };
  invoke('launch_game', { id: g.id }).catch(err => { clearTimeout(launching?.timer); launching = null; render(); toast("Couldn't launch", String(err)); });
  toast('Launching', g.name);
  render();
}

function onRunning(id) {
  const prev = RUNNING;
  if (id === prev) return;
  const now = Date.now();
  if (prev && sessionStart) {
    state.sessions.push({ g: prev, start: sessionStart, end: now });
    state.sessions = state.sessions.slice(-300);
    const g = findGame(prev);
    if (g) logLine(g, `Session ended (${Math.max(1, Math.round((now - sessionStart) / 60000))} min)`);
  }
  RUNNING = id;
  sessionStart = id ? now : null;
  if (id) {
    const g = findGame(id);
    if (launching?.id === id) { clearTimeout(launching.timer); launching = null; }
    if (g) { logLine(g, 'Session started'); toast(g.name, 'is running'); }
  }
  save();
  render();
}

// Applies a hotkey press, from the system-wide listener or (in a browser) the page.
function onCombo(combo) {
  if (capture) return;
  if (combo === state.panicKey) { panic(comboLabel(combo)); return; }
  const g = findGame(runningId());
  if (!g || !state.hotkeys) return;
  for (const m of modsOf(g)) {
    const k = keysOf(g, m);
    if (combo !== k.up && combo !== k.down) continue;
    const how = comboLabel(combo);
    if (m.type === 'toggle') setMod(g, m, !valueOf(g, m), how);
    else if (m.type === 'slider') setMod(g, m, valueOf(g, m) + (combo === k.down ? -1 : 1) * m.step * (m.max - m.min > 50 ? 10 : 1), how);
    else setMod(g, m, null, how);
    document.querySelectorAll(`[data-key="${CSS.escape(combo)}"]`).forEach(el => { el.classList.add('flash'); setTimeout(() => el.classList.remove('flash'), 250); });
    return;
  }
}

function startCapture(gid, mid, dir) {
  capture = { gid, mid, dir };
  syncHotkeys(); // release global keys so the new key reaches us
  render();
}
function finishCapture(combo) {
  const c = capture;
  capture = null;
  if (combo !== undefined) {
    if (c.mid === null) {
      if (!combo) { toast('The panic key needs a key', ''); }
      else {
        const g = findGame(runningId()) || game();
        const used = g && whoUses(g, combo, 'panic');
        state.panicKey = combo;
        toast('Panic key', comboLabel(combo));
        if (used) setTimeout(() => toast(`${comboLabel(combo)} is also used by`, used), 900);
      }
    } else {
      const g = findGame(c.gid), m = modsOf(g).find(x => x.id === c.mid);
      const used = combo && whoUses(g, combo, `${m.id}/${c.dir}`);
      ((state.binds[g.id] ||= {})[m.id] ||= {})[c.dir] = combo;
      toast(m.name, combo ? comboLabel(combo) : 'unbound');
      if (used) setTimeout(() => toast(`${comboLabel(combo)} is also used by`, used), 900);
    }
    save();
  }
  lastHotkeys = '';
  render();
}

function switchProfile(g, name) {
  state.activeProfile[g.id] = name;
  profileMenu = false;
  logLine(g, `Profile → ${name}`);
  save();
  toast('Profile', name);
  render();
}

const findMod = id => modsOf(game()).find(m => m.id === id);
let wipeArmed = false;

document.addEventListener('click', e => {
  const a = e.target.closest('a[target="_blank"]');
  if (a && invoke) { e.preventDefault(); invoke('plugin:opener|open_url', { url: a.href }).catch(console.error); return; }
  if (profileMenu && !e.target.closest('.profile')) { profileMenu = newProfile = false; renderGame(); }
  const t = e.target.closest('button');
  if (!t) return;
  const d = t.dataset;
  if (capture && !d.bind && t.id !== 'panicBind') { capture = null; lastHotkeys = ''; render(); }
  if (d.game) go('game', d.game);
  else if (d.view) go(d.view);
  else if (d.set) setMod(game(), findMod(d.set), d.v === '1');
  else if (d.act) setMod(game(), findMod(d.act));
  else if (d.bind) startCapture(game().id, d.bind, d.dir);
  else if (d.chip) { state.cat = d.chip; save(); renderChips(); renderMods(); }
  else if (d.cat) { state.closed[d.cat] = !state.closed[d.cat]; save(); renderMods(); }
  else if (d.libfilter) { state.libFilter = d.libfilter; save(); renderLibrary(); }
  else if (d.accent) { state.accent = d.accent; save(); render(); }
  else if (d.profile) switchProfile(game(), d.profile);
  else if (d.delprofile) {
    const g = game(), list = profilesOf(g);
    state.profiles[g.id] = list.filter(p => p.name !== d.delprofile);
    if (state.activeProfile[g.id] === d.delprofile) state.activeProfile[g.id] = state.profiles[g.id][0].name;
    save(); toast('Deleted profile', d.delprofile); renderGame();
  }
  else if (d.jump) {
    const run = runningId();
    if (run && (state.view !== 'game' || state.game !== run)) go('game', run);
    if (d.jump !== 'mods' && !state.panel) { state.panel = true; save(); renderGame(); }
    $(d.jump).scrollIntoView({ behavior: 'smooth', block: 'start' });
  }
  else switch (t.id) {
    case 'favBtn': state.fav[state.game] = !state.fav[state.game]; save(); renderGame(); break;
    case 'playBtn': isLive(game()) ? panic() : play(game()); break;
    case 'stopBtn': panic(); break;
    case 'panelBtn': state.panel = !state.panel; save(); renderGame(); break;
    case 'resetAll': case 'resetBtn': {
      const g = game();
      activeProf(g).values = {}; logLine(g, 'All mods → Off'); save(); render(); toast('All mods', 'off'); break;
    }
    case 'profileBtn': profileMenu = !profileMenu; newProfile = false; renderGame(); break;
    case 'addProfile': newProfile = true; renderGame(); $('pnewName')?.focus(); break;
    case 'panicBind': startCapture(null, null, 'up'); break;
    case 'resetBinds': state.binds = {}; save(); lastHotkeys = ''; render(); toast('Key bindings', 'reset'); break;
    case 'sortBtn': state.sortAZ = !state.sortAZ; save(); renderSide(); toast('Games sorted', state.sortAZ ? 'A–Z' : 'recently played'); break;
    case 'backBtn': step(-1); break;
    case 'fwdBtn': step(1); break;
    case 'setCounts': state.showCounts = !state.showCounts; save(); render(); break;
    case 'setHotkeys': state.hotkeys = !state.hotkeys; save(); render(); break;
    case 'setSoftware': state.showSoftware = !state.showSoftware; save(); render(); break;
    case 'softToggle': state.softOpen = !state.softOpen; save(); renderSide(); break;
    case 'rescanBtn': scanLibrary().then(() => { render(); toast('Found', `${LIB.length} games`); }); break;
    case 'wipeBtn':
      if (!wipeArmed) { wipeArmed = true; t.textContent = 'Click again to reset'; setTimeout(() => { wipeArmed = false; if (t.isConnected) t.textContent = 'Reset'; }, 3000); }
      else {
        wipeArmed = false;
        Object.values(state.profiles).forEach(list => list.forEach(p => { p.values = {}; }));
        save(); render(); toast('All mod settings', 'reset');
      }
      break;
    case 'winMin': appWin?.minimize(); break;
    case 'winMax': appWin?.toggleMaximize(); break;
    case 'winClose': appWin?.close(); break;
  }
});

document.addEventListener('submit', e => {
  if (e.target.id !== 'pnewForm') return;
  e.preventDefault();
  const g = game(), name = $('pnewName').value.trim();
  if (!name) return;
  if (profilesOf(g).some(p => p.name.toLowerCase() === name.toLowerCase())) { toast('There is already a profile called', name); return; }
  profilesOf(g).push({ name, values: { ...vals(g) } });
  newProfile = false;
  switchProfile(g, name);
});

document.addEventListener('change', e => {
  const id = e.target.dataset.num || e.target.dataset.range;
  if (id) setMod(game(), findMod(id), parseFloat(e.target.value));
});
document.addEventListener('input', e => {
  if (e.target.dataset.range) {
    const m = findMod(e.target.dataset.range), v = parseFloat(e.target.value);
    e.target.style.setProperty('--p', pct(m, v));
    e.target.closest('.slider').querySelector('[data-num]').value = fmt(m, v);
  }
  if (e.target.id === 'search') render();
  if (e.target.id === 'notes') { state.notes[state.game] = e.target.value; save(); }
});

document.addEventListener('keydown', e => {
  if (capture) {
    e.preventDefault();
    if (e.key === 'Escape') finishCapture(undefined);
    else if (e.key === 'Backspace' || e.key === 'Delete') finishCapture(capture.mid === null ? undefined : null);
    else { const combo = comboFromEvent(e); if (combo) finishCapture(combo); }
    return;
  }
  const typing = /INPUT|TEXTAREA/.test(document.activeElement.tagName);
  if ((e.key === '/' && !typing) || (e.ctrlKey && e.key.toLowerCase() === 'k')) { e.preventDefault(); $('search').focus(); $('search').select(); return; }
  if (e.key === 'Escape' && profileMenu) { profileMenu = newProfile = false; renderGame(); return; }
  if (e.key === 'Escape' && document.activeElement.id === 'search') { e.target.value = ''; render(); e.target.blur(); return; }
  if (e.altKey && e.key === 'ArrowLeft') { step(-1); return; }
  if (e.altKey && e.key === 'ArrowRight') { step(1); return; }
  // In the desktop app the system-wide listener handles hotkeys, even while Tinker has focus.
  if (invoke || typing) return;
  const combo = comboFromEvent(e);
  if (combo && state.playing) { e.preventDefault(); onCombo(combo); }
});
// Mouse back/forward buttons
window.addEventListener('mouseup', e => { if (e.button === 3) step(-1); if (e.button === 4) step(1); });

/* ---------- Icons ---------- */
const ICONS = {
  all: '<rect x="3" y="3" width="8" height="8" rx="2"/><rect x="13" y="3" width="8" height="8" rx="2"/><rect x="3" y="13" width="8" height="8" rx="2"/><rect x="13" y="13" width="8" height="8" rx="2"/>',
  player: '<circle cx="12" cy="7" r="4"/><path d="M4 21c0-4.4 3.6-7 8-7s8 2.6 8 7z"/>',
  physics: '<circle cx="14" cy="4" r="2.5"/><path d="M8 21l3-6-3-3 4-5 3 4h4v2h-5l-1.5-1.8L10 13l3 3-2.5 5z"/>',
  inventory: '<path d="M3 7l9-4 9 4v10l-9 4-9-4z" opacity=".45"/><path d="M3 7l9 4 9-4M12 11v10" fill="none" stroke="currentColor" stroke-width="2"/>',
  building: '<rect x="3" y="12" width="8" height="9" rx="1.5"/><rect x="13" y="12" width="8" height="9" rx="1.5"/><rect x="8" y="3" width="8" height="8" rx="1.5"/>',
  resources: '<path d="M12 2s7 7.5 7 12.5A7 7 0 0 1 5 14.5C5 9.5 12 2 12 2z"/>',
  weapons: '<circle cx="12" cy="12" r="8" fill="none" stroke="currentColor" stroke-width="2.4"/><circle cx="12" cy="12" r="3"/>',
  vessel: '<path d="M12 2c4 3 5 8 5 12l-2 3H9l-2-3c0-4 1-9 5-12z"/><path d="M9 18l-1 4h8l-1-4z" opacity=".5"/>',
  career: '<path d="M7 3h10v6a5 5 0 0 1-10 0z"/><path d="M10 15h4v3h3v3H7v-3h3z"/>',
  time: '<circle cx="12" cy="12" r="9" opacity=".4"/><path d="M11 6h2v6l4 2-1 2-5-3z"/>',
  crafting: '<path d="M14 3l7 7-3 3-2-2-8 8a2 2 0 0 1-3-3l8-8-2-2z"/>',
  truck: '<path d="M2 6h12v10H2zM14 9h4l4 4v3h-8z"/><circle cx="6" cy="18" r="2"/><circle cx="18" cy="18" r="2"/>',
  dot: '<circle cx="12" cy="12" r="5"/>',
};
ICONS.world = ICONS.resources;
const icon = (k, s = 16) => `<svg width="${s}" height="${s}" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">${ICONS[k] || ICONS.dot}</svg>`;

/* ---------- Boot ---------- */
(async () => {
  if (!appWin) $('winctl').hidden = true;
  await loadState();
  await scanLibrary();
  if (!findGame(state.game)) state.game = LIB[0]?.id ?? null;
  if (invoke) {
    state.playing = null;
    listen?.('game-running', e => onRunning(e.payload ?? null));
    listen?.('hotkey', e => onCombo(e.payload));
    onRunning(await invoke('running_game').catch(() => null));
  } else if (state.playing && !findGame(state.playing)) state.playing = null;
  nav.stack = [{ view: state.view, game: state.game }]; nav.i = 0;
  render();
})();
