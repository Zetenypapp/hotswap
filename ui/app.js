// HotSwap UI. Runs inside the Tauri window; also opens in a plain browser with demo data for quick styling work.

const tauri = window.__TAURI__;
const invoke = tauri?.core?.invoke;
const listen = tauri?.event?.listen;
const fileSrc = p => (p && tauri?.core?.convertFileSrc ? tauri.core.convertFileSrc(p) : null);
const appWin = tauri?.window?.getCurrentWindow?.();

const $ = id => document.getElementById(id);
const esc = s => String(s ?? '').replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
const initials = n => n.replace(/[^A-Za-z0-9 ]/g, '').split(' ').filter(Boolean).slice(0, 2).map(w => w[0]).join('').toUpperCase();
const hue = id => `hsl(${([...String(id)].reduce((h, c) => h * 31 + c.charCodeAt(0), 7) * 47 >>> 0) % 360} 55% 55%)`;
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
  showSoftware: true, softOpen: false, closeToTray: true, autoUpdate: true, sounds: true, overlay: true, paused: {},
  binds: {}, panicKey: 'Ctrl+Numpad0', profiles: {}, activeProfile: {}, sessions: [],
  pinned: {}, sheetKey: 'Ctrl+F1', menuKey: 'Insert', dcOverlay: true, dcTalk: true, dcNotes: true, dcPrivacy: 0, dcJoins: true, dcPopups: 'always', hud: false, hudCorner: 'top-left', hudStats: true, discord: true, overlayCorner: 'top', radarCorner: 'top-right', mini: false, lastVersion: '', pendingNotes: null,
};
const CORNERS = [['top-left', 'Top left'], ['top', 'Top'], ['top-right', 'Top right'], ['bottom-left', 'Bottom left'], ['bottom-right', 'Bottom right']];
const SHEET = '__sheet'; // capture.mid while rebinding the cheat sheet key
const MENU = '__menu'; // ... and the overlay menu key
const REPO_URL = 'https://github.com/Zetenypapp/hotswap';
let state = { ...DEFAULTS };
let LIB = [];
let STEAM = { root: null, libraries: [] };
let RUNNING = null;          // game ID reported by the watcher (desktop app only)
let sessionStart = null;     // when HotSwap was injected into the running game (the session)
// The running game HotSwap is injected into: right away when started with HotSwap's Play button,
// otherwise once the user presses Inject on its page. Until then the game only shows on Discord,
// with no mods and no timer.
let INJECTED = null;
let launching = null;        // { id, timer } while waiting for a launched game to appear
let capture = null;          // { gid, mid, dir } while rebinding a key; mid is null for the panic key
let stopArmed = false;       // stop button clicked once; a second click closes the game
let VERSION = '', UPDATE = null, UPDATE_MSG = ''; // app version and a newer release, if any
let AUTOSTART = false;      // read from Windows at boot; the setting lives there, not in state
let profileMenu = false;     // profile popover open
let newProfile = false;      // profile popover showing the name field
let importProfile = false;   // profile popover showing the share code field

/* ---------- Persistence ---------- */
async function loadState() {
  try {
    const raw = invoke ? await invoke('load_data', { name: 'state' }) : localStorage.getItem('hotswap');
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
function saveNow() {
  clearTimeout(saveTimer);
  const data = JSON.stringify(state);
  return invoke ? invoke('save_data', { name: 'state', data }).catch(console.error) : Promise.resolve();
}
function save() {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(() => {
    const data = JSON.stringify(state);
    if (invoke) invoke('save_data', { name: 'state', data }).catch(err => console.error('save failed', err));
    else try { localStorage.setItem('hotswap', data); } catch { }
  }, 250);
}

/* ---------- Library ---------- */
const SOURCES = { steam: 'Steam', epic: 'Epic Games', gog: 'GOG' };

function buildGame(raw) {
  const appId = raw.appId || 0;
  const c = appId ? CATALOG[appId] : null;
  const art = raw.art || {};
  const g = {
    id: raw.id || String(appId), appId, source: raw.source || 'steam', name: raw.name,
    lastPlayed: raw.lastPlayed || 0, size: raw.sizeOnDisk || 0, playtime: raw.playtime || 0,
    software: raw.kind === 'software', steamType: raw.steamType || null, anticheat: raw.anticheat || [],
    genre: c?.genre || '', solo: !!c?.solo, color: hue(raw.id || appId),
    // Anti-cheat games never get mods, even if the catalog has some.
    cats: raw.anticheat?.length ? [] : c?.cats || [],
    art: {
      icon: fileSrc(art.icon), logo: fileSrc(art.logo), hero: fileSrc(art.hero), heroBlur: fileSrc(art.heroBlur),
      capsule: fileSrc(art.capsule) || (!invoke && appId ? steamCdn(appId, 'library_600x900.jpg') : null),
      header: fileSrc(art.header) || (appId ? steamCdn(appId, 'header.jpg') : null),
    },
  };
  if (!invoke) { g.art.hero = steamCdn(appId, 'library_hero.jpg'); g.art.logo = steamCdn(appId, 'logo.png'); }
  let i = 0;
  g.cats.forEach(cat => cat.mods.forEach(m => {
    m.id = m.name.toLowerCase().replace(/[^a-z0-9]+/g, '-');
    m.key = KEY_POOL[i++ % KEY_POOL.length];
    m.live = !!LIVE[appId]?.includes(m.id);
  }));
  return g;
}

async function scanLibrary() {
  if (invoke) {
    try {
      const scan = await invoke('scan_library');
      STEAM = { root: scan.root, libraries: scan.libraries };
      LIB = scan.games.map(buildGame);
      return;
    } catch (err) { console.error('Steam scan failed', err); }
  }
  LIB = DEMO_LIBRARY.map(buildGame);
}

const modsOf = g => g.cats.flatMap(c => c.mods);
// Has mods that work now (not only ones marked Soon), and no anti-cheat.
const hasLiveMods = g => !g.anticheat.length && modsOf(g).some(m => m.live);
const gamesOnly = () => LIB.filter(g => !g.software);
const softwareOnly = () => LIB.filter(g => g.software);
const findGame = id => LIB.find(g => g.id === String(id));
const game = () => findGame(state.game) || LIB[0];
// In the desktop app "running" comes from the watcher; in a browser preview Play simulates it.
const runningId = () => invoke ? (RUNNING && INJECTED === RUNNING ? RUNNING : null) : state.playing;
const hours = min => min >= 60 ? `${Math.round(min / 60).toLocaleString()} h` : `${min} min`;
const duration = ms => { const m = Math.max(1, Math.round(ms / 60000)); return m >= 60 ? `${Math.floor(m / 60)} h ${m % 60} min` : `${m} min`; };
const ago = sec => {
  const m = Math.round((Date.now() / 1000 - sec) / 60);
  return m < 1 ? 'just now' : m < 60 ? `${m} min ago` : m < 1440 ? `${Math.round(m / 60)} h ago` : `${Math.round(m / 1440)} d ago`;
};
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
  if (state.sheetKey) seen.set(state.sheetKey, (seen.get(state.sheetKey) || 0) + 1);
  if (state.menuKey) seen.set(state.menuKey, (seen.get(state.menuKey) || 0) + 1);
  for (const m of modsOf(g)) for (const k of Object.values(keysOf(g, m))) if (k) seen.set(k, (seen.get(k) || 0) + 1);
  return new Set([...seen].filter(([, n]) => n > 1).map(([k]) => k));
}
function whoUses(g, combo, except) {
  if (combo === state.panicKey && except !== 'panic') return 'the panic key';
  if (combo === state.sheetKey && except !== 'sheet') return 'the cheat sheet key';
  if (combo === state.menuKey && except !== 'menu') return 'the overlay menu key';
  for (const m of modsOf(g)) {
    for (const [dir, k] of Object.entries(keysOf(g, m))) if (k === combo && `${m.id}/${dir}` !== except) return m.name;
  }
  return null;
}

let lastHotkeys = '';
function syncHotkeys() {
  if (!invoke) return;
  const g = findGame(runningId());
  let keys = !capture && state.hotkeys && g && modsOf(g).length
    ? state.paused[g.id] ? [state.panicKey]
      : [...new Set([state.panicKey, state.overlay && state.sheetKey, state.overlay && state.menuKey, ...modsOf(g).filter(m => m.live).flatMap(m => Object.values(keysOf(g, m)))].filter(Boolean))]
    : [];
  // The Discord overlay opens with the same key in any game (or none).
  if (!capture && state.overlay && state.dcOverlay && state.menuKey && !keys.includes(state.menuKey)) keys = [...keys, state.menuKey];
  const sig = keys.join('|');
  if (sig === lastHotkeys) return;
  lastHotkeys = sig;
  invoke('set_hotkeys', { keys }).then(failed => {
    if (failed.length) toast(`Couldn't use ${failed.map(comboLabel).join(', ')}`, 'another app has it');
  }).catch(console.error);
}

// Tells the trainer which values the running game should have.
let radarOpen = false;
let lastTrainer = '';
let TRAINER = { connected: false, message: '' };
function syncTrainer() {
  if (!invoke) return;
  const g = findGame(runningId());
  const values = {};
  if (g) for (const m of modsOf(g)) if (m.live && m.type !== 'action') values[m.id] = m.type === 'toggle' ? (valueOf(g, m) ? 1 : 0) : valueOf(g, m);
  const args = { game: g?.id ?? null, paused: !!(g && state.paused[g.id]), values };
  const sig = JSON.stringify(args);
  if (sig === lastTrainer) return;
  lastTrainer = sig;
  invoke('trainer_set', args).catch(console.error);
  // The radar window lives while a game with the radar mod has it on.
  const radar = !!values['enemy-radar'] && !args.paused;
  if (radar !== radarOpen) { radarOpen = radar; invoke('radar_show', { show: radar, corner: state.radarCorner }).catch(console.error); }
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
  profileMenu = newProfile = importProfile = false;
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
  const paused = run && state.paused[run.id];
  $('nowCard').innerHTML = run
    ? `<div class="now${stopArmed ? ' armed' : ''}"><button class="now-open" data-game="${run.id}" title="Open ${esc(run.name)}">${tile(run, 'md', false)}<span class="name">${stopArmed ? 'Click ■ again to close' : esc(run.name)}</span></button>
        ${modsOf(run).length ? `<button class="circle power${paused ? ' off' : ''}" id="powerBtn" aria-pressed="${!paused}" title="${paused ? 'Mods are paused. Click to turn them back on' : 'Pause all mods'}"><svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round"><path d="M12 3v8M6.3 6.3a8 8 0 1 0 11.4 0"/></svg></button>` : ''}
        <button class="circle stop" id="stopBtn" title="Close ${esc(run.name)}"><svg width="12" height="12" viewBox="0 0 24 24" fill="currentColor"><rect x="3" y="3" width="18" height="18" rx="4"/></svg></button></div>`
    : `<div class="now idle">No game running</div>`;

  const q = query();
  const pick = src => {
    let list = src.filter(x => !q || x.name.toLowerCase().includes(q) || modsOf(x).some(m => m.name.toLowerCase().includes(q)));
    return state.sortAZ ? [...list].sort((a, b) => a.name.localeCompare(b.name)) : list;
  };
  const row = (x, plain) => {
    const n = state.showCounts ? activeCount(x) : 0;
    return `<button class="gamebtn${plain ? ' plain' : ''}${x.id === state.game && state.view === 'game' ? ' on' : ''}" data-game="${x.id}">${tile(x)}<span class="name">${esc(x.name)}</span>${n ? `<span class="count">${n}</span>` : ''}</button>`;
  };
  const games = pick(gamesOnly()), soft = pick(softwareOnly());
  // Games HotSwap can mod come first, in their own section; everything else under a divider.
  const modded = games.filter(hasLiveMods), other = games.filter(g => !hasLiveMods(g));
  $('gameCount').textContent = gamesOnly().length;
  let html = !games.length ? '<p class="empty" style="padding:6px 10px">No games match.</p>'
    : (modded.length ? `<div class="side-sec">With mods <span class="n">${modded.length}</span></div>${modded.map(g => row(g)).join('')}` : '')
      + (other.length ? `<div class="side-sec other">Other games <span class="n">${other.length}</span></div>${other.map(g => row(g, true)).join('')}` : '');
  if (state.showSoftware && soft.length) {
    const open = state.softOpen || !!q;
    html += `<button class="soft-head" id="softToggle" aria-expanded="${open}">Software <span class="n">${soft.length}</span>
      <svg class="chev" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"><path d="M6 9l6 6 6-6"/></svg></button>
      ${open ? soft.map(g => row(g, true)).join('') : ''}`;
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

  const H = HOME || snapshotHome();
  const news = H.news.filter(n => {
    const g = findGame(n.app);
    return match(n.title) || (g && match(g.name));
  }).map((n, i) => {
    const g = findGame(n.app);
    const name = g?.name || DEMO_LIBRARY.find(d => d.appId === n.app)?.name || '';
    const feature = i === 0 && !q;
    const bg = g?.art.hero || g?.art.header;
    const who = feature && g?.art.logo ? img(g.art.logo, 'logo') : `<span class="who">${g ? tile(g, '', false) : ''}${esc(name)}</span>`;
    const when = new Date(n.date * 1000).toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
    return `<a class="news${feature ? ' feature' : ''}" style="--c:${g?.color || hue(n.app)}" href="${esc(n.url)}" ${ext}>
      ${bg ? `<span class="bg" style="background-image:url('${esc(bg)}')"></span><span class="shade"></span>` : ''}
      ${who}<h3>${esc(n.title)}</h3>${n.text && feature ? `<p>${esc(n.text)}</p>` : ''}
      <span class="src">${when} · ${esc(n.src || 'Steam announcement')}</span></a>`;
  }).join('');

  const deals = H.deals.filter(d => match(d.t)).map(d => `<a class="deal" style="--c:${hue(d.app)}" href="https://store.steampowered.com/app/${d.app}/" ${ext}>
      <span class="art">${img(steamCdn(d.app, 'header.jpg'))}<span class="off">−${d.off}%</span></span>
      <span class="body"><span class="t">${esc(d.t)}</span>
      <span class="price"><b>${money(d.sale)}</b><s>${money(d.was)}</s></span>
      ${d.rating ? `<span class="rating">${esc(d.rating)}</span>` : ''}</span></a>`).join('');

  const rankList = list => `<ol class="rank">${list.filter(x => match(x.t)).map((x, i) => `<li><a href="https://store.steampowered.com/app/${x.app}/" ${ext}>
      <span class="i">${i + 1}</span><span class="thumb">${img(steamCdn(x.app, 'header.jpg'), 'thumb')}</span>
      <span class="nm">${esc(x.t)}${x.note ? `<small>${x.note}</small>` : ''}</span>
      <span class="pr">${x.off ? `<em>−${x.off}%</em>` : ''}${x.price ? money(x.price) : 'Free'}</span></a></li>`).join('') || '<li class="empty" style="padding:10px">No matches.</li>'}</ol>`;

  $('homeBody').innerHTML = `
    <div class="hello"><h1>${hi}</h1><p>Pick up where you left off, or see what's new with your games.</p></div>
    ${jump ? `<section><div class="sec-head"><h2>Jump back in</h2><span>Recently played</span></div><div class="jump">${jump}</div></section>` : ''}
    <section><div class="sec-head"><h2>From your games</h2><span>Announcements from the developers of your recent games</span></div>
      <div class="news-grid">${news || '<p class="empty">No news matches your search.</p>'}</div></section>
    <div class="cols">
      <section><div class="sec-head"><h2>Hot deals</h2><span>Top-rated Steam discounts</span><a href="https://www.cheapshark.com/" ${ext}>More on CheapShark →</a></div>
        <div class="deal-grid">${deals || '<p class="empty">No deals match your search.</p>'}</div></section>
      <section>
        <div class="sec-head"><h2>Top sellers</h2><a href="https://steamdb.info/charts/" ${ext}>SteamDB charts →</a></div>
        <div class="listcard">${rankList(H.top)}</div>
        <div class="sec-head"><h2>New on Steam</h2></div>
        <div class="listcard">${rankList(H.newReleases)}</div>
      </section>
    </div>
    <p class="stamp">${H.fetchedAt
      ? `Updated ${ago(H.fetchedAt)} from Steam and CheapShark. Prices in USD.`
      : `Showing a saved snapshot from ${SNAPSHOT}.`}
      ${invoke ? `<button class="linkbtn" id="homeRefresh"${homeLoading ? ' disabled' : ''}>${homeLoading ? 'Updating…' : 'Refresh'}</button>` : ''}</p>`;
}

/* ---------- Home data: live from Steam and CheapShark, cached on disk ---------- */
let HOME = null;
let homeLoading = false;
function snapshotHome() {
  return {
    fetchedAt: 0,
    news: NEWS.map(n => ({ app: n.app, title: n.title, text: n.text || '', src: n.src, url: `https://store.steampowered.com/news/app/${n.app}`, date: Date.parse(n.date + 'T12:00:00') / 1000 })),
    deals: DEALS, top: TOP, newReleases: NEW_RELEASES,
  };
}
async function loadHome() {
  try {
    const raw = invoke && await invoke('load_data', { name: 'home' });
    HOME = raw ? JSON.parse(raw) : snapshotHome();
  } catch { HOME = snapshotHome(); }
  if (invoke && Date.now() / 1000 - (HOME.fetchedAt || 0) > 3600) refreshHome(false);
}
async function refreshHome(manual) {
  if (!invoke || homeLoading) return;
  homeLoading = true;
  if (state.view === 'home') renderHome();
  const appIds = gamesOnly().filter(g => g.appId).sort((a, b) => b.lastPlayed - a.lastPlayed).slice(0, 8).map(g => g.appId);
  try {
    const fresh = await invoke('fetch_home', { appIds });
    if (fresh.failed.length === 3) {
      if (manual) toast("Couldn't reach Steam or CheapShark", 'check your connection');
    } else {
      // Keep the previous data for any source that failed this time.
      const next = { ...fresh };
      if (fresh.failed.includes('news')) next.news = HOME.news;
      if (fresh.failed.includes('deals')) next.deals = HOME.deals;
      if (fresh.failed.includes('store')) { next.top = HOME.top; next.newReleases = HOME.newReleases; }
      HOME = next;
      invoke('save_data', { name: 'home', data: JSON.stringify(HOME) }).catch(console.error);
      if (manual) toast('Home', 'updated');
    }
  } catch (err) { console.error('Home refresh failed', err); }
  homeLoading = false;
  if (state.view === 'home') renderHome();
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
        <span class="nm">${esc(g.name)}</span><span class="sub">${[
          g.software ? 'Software' : g.anticheat.length ? 'Anti-cheat' : total ? `${total} mods` : 'No mods yet',
          g.playtime ? hours(g.playtime) : '', g.source !== 'steam' ? SOURCES[g.source] : '',
        ].filter(Boolean).join(' · ')}</span></button>`;
    }).join('') || '<p class="empty">Nothing here yet.</p>'}</div>`;
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
  // A rejected change must also snap the control back, or it shows a value that was never saved.
  const reject = (a, b) => { toast(a, b); if (state.view === 'game') renderGame(); };
  if (!m.live) return reject(m.name, 'coming soon');
  if (!isLive(g)) return reject(`Start ${g.name} first`, '');
  if (state.paused[g.id]) return reject('Mods are paused', 'press ⏻ to resume');
  let shown;
  if (m.type === 'slider') {
    v = Math.min(m.max, Math.max(m.min, Math.round(v / m.step) * m.step));
    if (v === valueOf(g, m)) { renderGame(); return; }
    vals(g)[m.id] = v; shown = fmt(m, v);
  } else if (m.type === 'toggle') {
    if (v === valueOf(g, m)) return;
    vals(g)[m.id] = v; shown = v ? 'On' : 'Off';
  } else { shown = 'applied'; invoke?.('trainer_action', { id: m.id }); }
  logLine(g, `${m.name} → ${shown}${how ? ' (' + how + ')' : ''}`);
  save();
  syncTrainer();
  toast(m.name, shown);
  feedback(g, m.name, shown, m.type === 'toggle' ? !!v : true, !!how, m.type === 'slider' ? (v - m.min) / (m.max - m.min) * 100 : null);
  renderSide();
  if (state.view === 'game' && state.game === g.id) renderGame();
  if (state.mini) renderMini();
}

// Panic key, the Playing pill's ✕ and the sidebar stop button: every mod off in the running game.
function panic(how) {
  const g = findGame(runningId()) || game();
  if (!g || !modsOf(g).length) return;
  activeProf(g).values = {};
  logLine(g, `All mods → Off${how ? ' (' + how + ')' : ''}`);
  save();
  toast('All mods', 'off');
  feedback(g, 'All mods', 'Off', false, true);
  render();
}
