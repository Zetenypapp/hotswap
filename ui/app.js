// Hotswap UI. Runs inside the Tauri window; also opens in a plain browser with demo data for quick styling work.

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
  pinned: {}, sheetKey: 'Ctrl+F1', overlayCorner: 'top', radarCorner: 'top-right', mini: false, lastVersion: '', pendingNotes: null,
};
const CORNERS = [['top-left', 'Top left'], ['top', 'Top'], ['top-right', 'Top right'], ['bottom-left', 'Bottom left'], ['bottom-right', 'Bottom right']];
const SHEET = '__sheet'; // capture.mid while rebinding the cheat sheet key
const REPO_URL = 'https://github.com/Zetenypapp/hotswap';
let state = { ...DEFAULTS };
let LIB = [];
let STEAM = { root: null, libraries: [] };
let RUNNING = null;          // game ID reported by the watcher (desktop app only)
let sessionStart = null;     // when the current RUNNING session began
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
const gamesOnly = () => LIB.filter(g => !g.software);
const softwareOnly = () => LIB.filter(g => g.software);
const findGame = id => LIB.find(g => g.id === String(id));
const game = () => findGame(state.game) || LIB[0];
// In the desktop app "running" comes from the watcher; in a browser preview Play simulates it.
const runningId = () => invoke ? RUNNING : state.playing;
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
  for (const m of modsOf(g)) for (const k of Object.values(keysOf(g, m))) if (k) seen.set(k, (seen.get(k) || 0) + 1);
  return new Set([...seen].filter(([, n]) => n > 1).map(([k]) => k));
}
function whoUses(g, combo, except) {
  if (combo === state.panicKey && except !== 'panic') return 'the panic key';
  if (combo === state.sheetKey && except !== 'sheet') return 'the cheat sheet key';
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
    ? state.paused[g.id] ? [state.panicKey]
      : [...new Set([state.panicKey, state.overlay && state.sheetKey, ...modsOf(g).filter(m => m.live).flatMap(m => Object.values(keysOf(g, m)))].filter(Boolean))]
    : [];
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
  const g = findGame(RUNNING);
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

/* ---------- Maps ---------- */
// Steam app IDs with an interactive map (opened in its own window by the backend).
const MAP_GAMES = ['526870', '264710'];
// Shown on the Maps page but not ready yet.
const SOON_MAPS = ['387990'];
const MAP_NAMES = { 526870: 'Satisfactory', 264710: 'Subnautica', 387990: 'Scrap Mechanic' };
// Games that also have Satisfactory Calculator's web map.
const WEB_MAPS = ['526870'];

let mapOpen = null, mapShown = false; // app ID of the map being shown on the Maps page
let mapMode = 'local'; // 'local' = Hotswap's own map, 'web' = Satisfactory Calculator

function renderMaps() {
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
      ${WEB_MAPS.includes(String(open.appId)) ? `<div class="seg-tabs"><button class="chip${mapMode === 'local' ? ' on' : ''}" data-mapmode="local">Hotswap map</button><button class="chip${mapMode === 'web' ? ' on' : ''}" data-mapmode="web">Detailed map</button></div>` : ''}
      <span class="meta">${mapMode === 'web' ? 'By Satisfactory Calculator (their site, their ads).' : WEB_MAPS.includes(String(open.appId)) ? 'Live from your game. Right-click to teleport or pin.' : 'Made from your game\'s terrain files. Right-click to drop a pin.'}</span></div>
    <div id="mapHost" class="map-host">${mapMode === 'web' ? '<span>Loading map…</span>' : ''}</div>` : `
    <div class="lib-head"><h1>Maps</h1><span class="meta">Resources, collectibles and your factory, live. Pin spots and teleport.</span></div>
    <div class="lib-grid">${list.map(g => `<button class="lib-item" data-map="${g.appId}"><span class="art">${capsule(g)}</span>
      <span class="nm">${esc(g.name)}</span><span class="sub">Interactive map${RUNNING === g.id ? ' · live' : ''}</span></button>`).join('')}
      ${SOON_MAPS.map(id => LIB.find(g => String(g.appId) === id) || buildGame({ appId: +id, name: MAP_NAMES[id] })).map(g => `<button class="lib-item soon-item" disabled><span class="art">${capsule(g)}</span>
      <span class="nm">${esc(g.name)}</span><span class="sub">Coming soon</span></button>`).join('')}</div>
    <p class="meta" style="margin-top:22px">The Hotswap map is drawn from your running game. The detailed map is <a href="https://satisfactory-calculator.com" ${ext}>Satisfactory Calculator</a>, shown as-is.</p>`;
  if (open && mapMode === 'local') {
    const host = $('mapHost');
    host.style.height = Math.max(300, innerHeight - host.getBoundingClientRect().top - 16) + 'px';
    LocalMap.mount(host, String(open.appId));
  }
}

// The map is a separate web view laid over #mapHost, so keep it matched to that box and hide it
// whenever the page (or the palette, which it would cover) says so.
function syncMap() {
  if (state.view !== 'maps' && $('mapsBody').dataset.key) { LocalMap.unmount(); $('mapsBody').dataset.key = ''; }
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
      ${invoke ? `<div class="set-row"><div class="txt"><b>Cheat sheet key</b><span>Hold it in game to see every mod and its hotkey on the overlay. Click to change.</span></div>
        <button class="key bindable${capture?.mid === SHEET ? ' listening' : ''}${state.sheetKey ? '' : ' unbound'}" id="sheetBind">${capture?.mid === SHEET ? 'Press a key…' : esc(comboLabel(state.sheetKey))}</button></div>` : ''}
      <div class="set-row"><div class="txt"><b>Sounds</b><span>A short chime when a mod turns on or off.</span></div>${sw('setSounds', state.sounds)}</div>
      ${invoke ? `<div class="set-row"><div class="txt"><b>In-game overlay</b><span>Shows mod changes from hotkeys at the top of the screen, over the game. Works with borderless and windowed games, not exclusive fullscreen.</span></div><button class="btn" id="testOverlay">Test</button>${sw('setOverlay', state.overlay)}</div>` : ''}
      ${invoke ? `<div class="set-row"><div class="txt"><b>Overlay position</b><span>Where mod changes and the cheat sheet show up.</span></div>${cornerPicker('ocorner', state.overlayCorner)}</div>
      <div class="set-row"><div class="txt"><b>Radar position</b><span>Where the enemy radar sits, in games that have one.</span></div>${cornerPicker('rcorner', state.radarCorner)}</div>` : ''}
      <div class="set-row"><div class="txt"><b>Reset all key bindings</b><span>Puts every mod back on its default hotkey.</span></div><button class="btn" id="resetBinds">Reset</button></div>
    </div>
    ${invoke ? `<div class="set-group"><h2>Startup and tray</h2>
      <div class="set-row"><div class="txt"><b>Start with Windows</b><span>Hotswap opens quietly in the tray when you sign in, so hotkeys are ready when you start a game.</span></div>${sw('setAutostart', AUTOSTART)}</div>
      <div class="set-row"><div class="txt"><b>Close to tray</b><span>The close button hides Hotswap in the tray instead of quitting. Quit from the tray icon's menu.</span></div>${sw('setTray', state.closeToTray)}</div>
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
      <div class="set-row"><div class="txt"><b>Hotswap ${esc(VERSION || '')}</b><span>${UPDATE ? `Version ${esc(UPDATE.version)} is available.` : esc(UPDATE_MSG || 'Free, open source, no ads.')} Game art is read from your local Steam cache. Figtree font under the SIL Open Font License.</span></div>
        ${invoke ? (UPDATE ? '<button class="btn" id="updateBtn">Update now</button>' : '<button class="btn" id="checkUpdateBtn">Check now</button>') : ''}</div>
      <div class="set-row"><div class="txt"><b>What's new</b><span>The release notes for this version.</span></div><button class="btn" id="whatsNewBtn">Show</button></div>
      <div class="set-row"><div class="txt"><b>Found a bug or want a game added?</b><span>Opens a GitHub issue with your Hotswap version already filled in.</span></div>
        <div class="btnrow"><a class="btn" href="${esc(bugUrl())}" ${ext}>Report a bug</a><a class="btn" href="${esc(requestUrl())}" ${ext}>Request a game</a></div></div>
      ${invoke ? `<div class="set-row"><div class="txt"><b>Check for updates automatically</b><span>Looks for a new release on GitHub when Hotswap starts. Updates are signed, and nothing installs without your click.</span></div>${sw('setAutoUpdate', state.autoUpdate)}</div>` : ''}
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
        (() => { const ms = state.sessions.filter(s => s.g === g.id).reduce((t, s) => t + s.end - s.start, 0) + (live && sessionStart ? Date.now() - sessionStart : 0); return ms ? `${duration(ms)} played with Hotswap` : ''; })(),
        g.source !== 'steam' ? SOURCES[g.source] : '',
        g.solo ? 'Solo or private sessions only' : '',
      ].filter(Boolean).join(' · ')}</div>
      ${g.anticheat.length ? `<div class="ac-note"><svg width="13" height="13" viewBox="0 0 24 24" fill="currentColor"><path d="M12 2l8 3v6c0 5-3.4 9.4-8 11-4.6-1.6-8-6-8-11V5z"/></svg>Uses ${esc(g.anticheat.join(' and '))}. Modding it can get your account banned.</div>` : ''}</div>
    <div class="gh-actions">
      ${total ? `${profileControl(g)}
      <button class="roundbtn" id="resetBtn" aria-label="Turn off all mods" title="Turn off all mods"><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M3 12a9 9 0 1 0 3-6.7L3 8"/><path d="M3 3v5h5"/></svg></button>` : ''}
      <button class="roundbtn${state.panel ? ' on' : ''}" id="panelBtn" aria-pressed="${state.panel}" aria-label="Side panel" title="Side panel"><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4"><rect x="3" y="4" width="18" height="16" rx="3"/><path d="M15 4v16"/></svg></button>
      ${live
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
      <p>This game uses ${esc(g.anticheat.join(' and '))}. Changing it while anti-cheat is running can get your account banned, so Hotswap doesn't offer mods here.</p></div>`;
    return;
  }
  if (!g.cats.length) {
    $('mods').innerHTML = g.software
      ? `<div class="nomods">${img(g.art.logo, 'logo')}<h2>${esc(g.name)} is software</h2>
        <p>Steam lists this as ${g.steamType ? `a${/^[aeiou]/.test(g.steamType) ? 'n' : ''} ${esc(g.steamType)}` : 'software'}, not a game, so Hotswap doesn't add mods to it.</p></div>`
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
  $('sessionsMeta').textContent = [g.playtime ? `${hours(g.playtime)} on Steam` : '', total ? `${duration(total)} with Hotswap` : ''].filter(Boolean).join(' · ');
  $('sessions').innerHTML = rows.length ? rows.join('') : '<p class="empty">Sessions show up here after you play with Hotswap open.</p>';
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
  const views = { home: 'homeView', library: 'libView', maps: 'mapsView', mods: 'modsView', settings: 'setView', game: 'gameView' };
  if (state.view === 'game' && !game()) state.view = 'home';
  Object.entries(views).forEach(([v, id]) => $(id).hidden = v !== state.view);
  renderSide();
  ({ home: renderHome, library: renderLibrary, maps: renderMaps, mods: () => Mods.render(), settings: renderSettings, game: renderGame })[state.view]();
  if (state.mini) renderMini();
  syncHotkeys();
  syncTrainer();
  syncMap();
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
  feedback(g, m.name, shown, m.type === 'toggle' ? !!v : true, !!how);
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

/* ---------- Feedback: sound and the in-game overlay ---------- */
let audio;
// A short two-note chime, rising for on and falling for off. Made in code, no sound files.
function chime(on) {
  try {
    audio ||= new AudioContext();
    if (audio.state === 'suspended') audio.resume();
    const t = audio.currentTime;
    (on ? [660, 990] : [660, 440]).forEach((f, i) => {
      const o = audio.createOscillator(), gain = audio.createGain();
      o.type = 'sine'; o.frequency.value = f;
      gain.gain.setValueAtTime(0, t + i * 0.07);
      gain.gain.linearRampToValueAtTime(0.18, t + i * 0.07 + 0.01);
      gain.gain.exponentialRampToValueAtTime(0.001, t + i * 0.07 + 0.16);
      o.connect(gain).connect(audio.destination);
      o.start(t + i * 0.07); o.stop(t + i * 0.07 + 0.18);
    });
  } catch {}
}
// `hotkey` is true for changes made from the game (hotkeys, panic key): those show on the overlay.
function feedback(g, title, value, on, hotkey) {
  if (state.sounds) chime(on);
  if (state.overlay && hotkey && invoke && isLive(g)) {
    invoke('overlay_notify', { note: { title, value, on, game: g.name, accent: state.accent, corner: state.overlayCorner } }).catch(console.error);
  }
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

/* ---------- Updates ---------- */
async function checkUpdate(manual) {
  if (!invoke) return;
  try {
    UPDATE = await invoke('check_update');
    UPDATE_MSG = UPDATE ? '' : 'You have the latest version.';
    if (manual) toast(UPDATE ? `Version ${UPDATE.version}` : 'Hotswap is up to date', UPDATE ? 'available' : '');
  } catch (e) {
    UPDATE_MSG = "Couldn't check for updates.";
    if (manual) toast("Couldn't check for updates", String(e));
  }
  const pill = $('updatePill');
  pill.hidden = !UPDATE;
  if (UPDATE) pill.textContent = `Update to ${UPDATE.version}`;
  if (state.view === 'settings') renderSettings();
}
async function installUpdate() {
  if (!UPDATE) return;
  toast('Downloading update', UPDATE.version);
  $('updatePill').textContent = 'Updating…';
  state.pendingNotes = { version: UPDATE.version, notes: UPDATE.notes || '' };
  await saveNow();
  // Hotswap closes and the installer reopens it on the new version.
  invoke('install_update').catch(e => { toast("Update failed", String(e)); $('updatePill').textContent = `Update to ${UPDATE.version}`; });
}

// Applies a hotkey press, from the system-wide listener or (in a browser) the page.
function onCombo(combo) {
  if (capture) return;
  if (combo === state.panicKey) { panic(comboLabel(combo)); return; }
  if (combo === state.sheetKey) { showSheet(true); return; }
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
    if (c.mid === SHEET) {
      const g = findGame(runningId()) || game();
      const used = combo && g && whoUses(g, combo, 'sheet');
      state.sheetKey = combo;
      toast('Cheat sheet key', combo ? comboLabel(combo) : 'unbound');
      if (used) setTimeout(() => toast(`${comboLabel(combo)} is also used by`, used), 900);
    } else if (c.mid === null) {
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

/* ---------- Status dots, corners and links ---------- */
// Green: on and live in the game. Amber: on, but Hotswap is still connecting.
function modDot(g, m) {
  if (!invoke || !m.live || !isActive(g, m) || !isLive(g) || state.paused[g.id]) return '';
  return TRAINER.connected
    ? '<span class="mdot ok" title="Live in the game"></span>'
    : `<span class="mdot wait" title="${esc(TRAINER.message || 'Connecting to the game…')}"></span>`;
}
const cornerPicker = (kind, cur) => `<div class="corners">${CORNERS.map(([k, l]) =>
  `<button class="corner c-${k}${cur === k ? ' on' : ''}" data-${kind}="${k}" aria-label="${l}" title="${l}"><span></span></button>`).join('')}</div>`;

const openUrl = url => invoke ? invoke('plugin:opener|open_url', { url }).catch(console.error) : window.open(url, '_blank');
const issueUrl = (title, body, label) => `${REPO_URL}/issues/new?title=${encodeURIComponent(title)}&body=${encodeURIComponent(body)}${label ? '&labels=' + encodeURIComponent(label) : ''}`;
function bugUrl() {
  const run = findGame(runningId());
  return issueUrl('Bug: ', [
    '**What happened?**', '', '', '**What did you expect?**', '', '', '**Steps to reproduce**', '1. ', '', '---',
    `Hotswap ${VERSION || '(browser preview)'}`, `Game: ${run ? `${run.name} (${run.id})` : 'none running'}`,
    `Trainer: ${run ? TRAINER.connected ? 'connected' : TRAINER.message || 'not connected' : '-'}`,
  ].join('\n'), 'bug');
}
function requestUrl(g) {
  return issueUrl(`Game request: ${g?.name || ''}`, [
    `**Game:** ${g?.name || ''}`, `**Store:** ${g ? SOURCES[g.source] : ''}${g?.appId ? ` (app ${g.appId})` : ''}`, '',
    '**Mods you would like:**', '- ', '', '---', `Hotswap ${VERSION || ''}`,
  ].join('\n'), 'game request');
}

/* ---------- Cheat sheet: every mod's hotkey on the overlay while the key is held ---------- */
function showSheet(on) {
  const g = findGame(runningId());
  if (!invoke || !state.overlay || !g || !modsOf(g).length) return;
  const pins = state.pinned[g.id] || {};
  const items = modsOf(g).filter(m => m.live).sort((a, b) => !!pins[b.id] - !!pins[a.id]).map(m => {
    const k = keysOf(g, m);
    return {
      name: m.name, on: isActive(g, m),
      value: m.type === 'toggle' ? (valueOf(g, m) ? 'On' : 'Off') : m.type === 'slider' ? fmt(m, valueOf(g, m)) : '',
      keys: [k.up && (m.type === 'slider' ? '↑ ' : '') + comboLabel(k.up), k.down && '↓ ' + comboLabel(k.down)].filter(Boolean),
    };
  });
  invoke('overlay_notify', { note: { kind: 'sheet', show: on, items, game: g.name, accent: state.accent, corner: state.overlayCorner } }).catch(console.error);
}

/* ---------- Share codes: a profile as a short string ---------- */
function shareCode(g) {
  const p = activeProf(g);
  const json = JSON.stringify({ g: g.id, n: p.name, v: p.values || {} });
  return 'HS1-' + btoa(String.fromCharCode(...new TextEncoder().encode(json)));
}
async function copyText(s) {
  try { await navigator.clipboard.writeText(s); return true; } catch { }
  const ta = Object.assign(document.createElement('textarea'), { value: s });
  document.body.append(ta); ta.select();
  const ok = document.execCommand('copy'); ta.remove(); return ok;
}
function importCode(g, code) {
  let data;
  try {
    const b = atob(code.trim().replace(/^HS1-/, ''));
    data = JSON.parse(new TextDecoder().decode(Uint8Array.from(b, c => c.charCodeAt(0))));
  } catch { toast("That isn't a Hotswap share code", ''); return; }
  if (String(data.g) !== g.id) { toast('That code is for', findGame(data.g)?.name || 'another game'); return; }
  // Only mods this game has, with values they accept.
  const values = {};
  for (const m of modsOf(g)) {
    const v = data.v?.[m.id];
    if (m.type === 'toggle' && typeof v === 'boolean') values[m.id] = v;
    if (m.type === 'slider' && typeof v === 'number' && isFinite(v)) values[m.id] = Math.min(m.max, Math.max(m.min, v));
  }
  const list = profilesOf(g), base = String(data.n || 'Shared').slice(0, 20);
  let name = base;
  for (let i = 2; list.some(p => p.name.toLowerCase() === name.toLowerCase()); i++) name = `${base} ${i}`;
  list.push({ name, values });
  importProfile = false;
  switchProfile(g, name);
}

/* ---------- What's new: release notes after an update ---------- */
function mdLite(s) {
  const inline = t => esc(t).replace(/\*\*(.+?)\*\*/g, '<b>$1</b>').replace(/\*(.+?)\*/g, '<i>$1</i>').replace(/`(.+?)`/g, '<code>$1</code>');
  let html = '', list = false;
  for (const raw of s.replace(/\r/g, '').split('\n')) {
    const line = raw.trimEnd(), li = line.match(/^(\s*)[-*] (.*)/);
    if (li) { if (!list) { html += '<ul>'; list = true; } html += `<li${li[1].length ? ' class="sub"' : ''}>${inline(li[2])}</li>`; continue; }
    if (list) { html += '</ul>'; list = false; }
    if (!line.trim()) continue;
    const h = line.match(/^#{1,4} (.*)/);
    if (h) { if (!/^#{1,2} /.test(line)) html += `<h3>${inline(h[1])}</h3>`; continue; }
    html += `<p>${inline(line.replace(/^> ?/, ''))}</p>`;
  }
  return html + (list ? '</ul>' : '');
}
async function showWhatsNew(manual) {
  let notes = '';
  try {
    const r = await fetch(`https://api.github.com/repos/Zetenypapp/hotswap/releases/tags/v${VERSION}`);
    if (r.ok) notes = (await r.json()).body || '';
  } catch { }
  if (!notes && state.pendingNotes?.version === VERSION) notes = state.pendingNotes.notes;
  state.pendingNotes = null; save();
  if (!notes) { if (manual) toast("Couldn't load the release notes", 'check your connection'); return; }
  $('wnTitle').textContent = `What's new in Hotswap ${VERSION}`;
  $('wnBody').innerHTML = mdLite(notes.split(/\n### 📦 Install/)[0]);
  $('wnLink').href = `${REPO_URL}/releases/tag/v${VERSION}`;
  $('whatsNew').hidden = false;
}

/* ---------- Mini mode: a small always-on-top window with the running game's mods ---------- */
function setMini(on) {
  state.mini = on;
  if (on) { const run = runningId(); if (run && findGame(run) && modsOf(findGame(run)).length) state.game = run; closePalette?.(); }
  document.body.classList.toggle('mini', on);
  invoke?.('set_mini', { on }).catch(err => toast("Couldn't switch modes", String(err)));
  save();
  render();
}
function renderMini() {
  const g = game();
  if (!g) return;
  const live = isLive(g), paused = live && state.paused[g.id], locked = !live || paused;
  const pins = state.pinned[g.id] || {};
  const mods = modsOf(g).filter(m => m.live).sort((a, b) => !!pins[b.id] - !!pins[a.id]);
  const row = m => {
    const v = valueOf(g, m), dis = locked ? ' disabled' : '', k = keysOf(g, m);
    const ctl = m.type === 'toggle'
      ? `<button class="seg sm" role="switch" aria-checked="${!!v}" aria-label="${esc(m.name)}" data-flip="${m.id}"${dis}><span class="${!v ? 'sel' : ''}">Off</span><span class="${v ? 'sel on' : ''}">On</span></button>`
      : m.type === 'slider'
        ? `<div class="slider sm"><input type="number" data-num="${m.id}" min="${m.min}" max="${m.max}" step="${m.step}" value="${fmt(m, v)}" aria-label="${esc(m.name)}"${dis}></div>`
        : `<button class="apply sm" data-act="${m.id}"${dis}>Apply</button>`;
    return `<div class="mrow${isActive(g, m) ? ' active' : ''}"><div class="mlabel"><span>${pins[m.id] ? '<i class="mpin"></i>' : ''}${esc(m.name)}${modDot(g, m)}</span><small>${esc([k.up && comboLabel(k.up), k.down && comboLabel(k.down)].filter(Boolean).join(' / ') || 'No hotkey')}</small></div>${ctl}</div>`;
  };
  $('mini').innerHTML = `<div class="mhead" data-tauri-drag-region>${tile(g, 'md')}<div class="mtitle" data-tauri-drag-region><b data-tauri-drag-region>${esc(g.name)}</b>
      <span class="${live && TRAINER.connected ? 'ok' : ''}" data-tauri-drag-region>${!live ? 'Not running' : paused ? 'Mods paused' : invoke && !TRAINER.connected ? esc(TRAINER.message || 'Connecting…') : `${activeCount(g)} on`}</span></div>
      <button class="ghost" id="miniExit" aria-label="Back to the full window" title="Back to the full window"><svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M15 3h6v6M9 21H3v-6M21 3l-7 7M3 21l7-7"/></svg></button>
      <button class="ghost" id="winClose" aria-label="Close" title="Close"><svg width="12" height="12" viewBox="0 0 10 10"><path d="M0 0l10 10M10 0L0 10" stroke="currentColor" stroke-width="1.4"/></svg></button></div>
    <div class="mlist">${mods.length ? mods.map(row).join('') : `<p class="empty">${esc(g.name)} has no mods yet.</p>`}</div>`;
}

/* ---------- Command palette (Ctrl+K) ---------- */
let pal = null; // { q, sel, items }
const PAGE_ICONS = {
  home: '<path d="M3 10.5L12 3l9 7.5V21h-6v-6H9v6H3z"/>',
  library: ICONS_GRID(),
  maps: '<path d="M3 6l6-2 6 2 6-2v14l-6 2-6-2-6 2z" opacity=".35"/><path d="M9 4v14l6 2V6z"/>',
  mods: '<path d="M10 3h4v3a2 2 0 1 0 4 0V3h3v7h-3a2 2 0 1 0 0 4h3v7h-7v-3a2 2 0 1 0-4 0v3H3v-7h3a2 2 0 1 0 0-4H3V3z"/>',
  settings: '<circle cx="12" cy="12" r="4"/><path d="M11 2h2v4h-2zM11 18h2v4h-2zM2 11h4v2H2zM18 11h4v2h-4z"/>',
  action: '<path d="M13 2L4 14h7l-1 8 9-12h-7z"/>',
};
function ICONS_GRID() { return '<rect x="3" y="3" width="8" height="8" rx="2"/><rect x="13" y="3" width="8" height="8" rx="2"/><rect x="3" y="13" width="8" height="8" rx="2"/><rect x="13" y="13" width="8" height="8" rx="2"/>'; }
const palIcon = k => `<span class="pal-ic"><svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor">${PAGE_ICONS[k]}</svg></span>`;

function paletteItems(q) {
  const items = [];
  const add = (kind, label, sub, run, ic) => items.push({ kind, label, sub, run, ic });
  [['home', 'Home'], ['library', 'Library'], ['maps', 'Maps'], ['mods', 'Mods'], ['settings', 'Settings']]
    .forEach(([v, l]) => add('Page', l, '', () => go(v), palIcon(v)));
  const games = q ? LIB : gamesOnly().slice().sort((a, b) => b.lastPlayed - a.lastPlayed).slice(0, 6);
  games.forEach(g => add(g.software ? 'Software' : 'Game', g.name, [g.playtime ? hours(g.playtime) : '', modsOf(g).length ? `${modsOf(g).length} mods` : ''].filter(Boolean).join(' · '),
    () => go('game', g.id), tile(g, 'pal-tile', false)));
  if (q) LIB.forEach(g => modsOf(g).forEach(m => add('Mod', m.name, g.name, () => { go('game', g.id); flashMod(m.id); }, palIcon('action'))));
  const run = findGame(runningId());
  if (run && modsOf(run).length) add('Action', 'Turn off all mods', run.name, () => panic(), palIcon('action'));
  if (invoke) add('Action', 'Refresh Home', 'News and deals', () => { go('home'); refreshHome(true); }, palIcon('action'));
  add('Action', 'Rescan library', 'Steam, Epic and GOG', () => scanLibrary().then(() => { render(); toast('Found', `${LIB.length} games`); }), palIcon('action'));
  add('Action', state.panel ? 'Hide side panel' : 'Show side panel', '', () => { state.panel = !state.panel; save(); render(); }, palIcon('action'));
  if (invoke) add('Action', state.mini ? 'Leave mini mode' : 'Mini mode', 'Small window that stays on top', () => setMini(!state.mini), palIcon('action'));
  add('Action', "What's new", `Hotswap ${VERSION}`, () => showWhatsNew(true), palIcon('action'));
  add('Action', 'Report a bug', 'Opens GitHub', () => openUrl(bugUrl()), palIcon('action'));
  add('Action', 'Request a game', 'Opens GitHub', () => openUrl(requestUrl()), palIcon('action'));
  if (!q) return items;
  // Name starts with the query, then contains it, then only the subtitle matches.
  return items
    .map(it => ({ it, s: it.label.toLowerCase().startsWith(q) ? 0 : it.label.toLowerCase().includes(q) ? 1 : it.sub.toLowerCase().includes(q) ? 2 : 9 }))
    .filter(x => x.s < 9).sort((a, b) => a.s - b.s).map(x => x.it).slice(0, 40);
}
function openPalette() {
  pal = { q: '', sel: 0 };
  $('palette').hidden = false;
  $('palInput').value = '';
  renderPalette();
  $('palInput').focus();
  syncMap();
}
function closePalette() { pal = null; $('palette').hidden = true; syncMap(); }
function renderPalette() {
  pal.items = paletteItems(pal.q);
  pal.sel = Math.max(0, Math.min(pal.sel, pal.items.length - 1));
  $('palList').innerHTML = pal.items.map((it, i) => `<button class="pal-item${i === pal.sel ? ' sel' : ''}" data-pal="${i}" role="option" aria-selected="${i === pal.sel}">
      ${it.ic}<span class="pal-text"><b>${esc(it.label)}</b>${it.sub ? `<small>${esc(it.sub)}</small>` : ''}</span><span class="pal-kind">${it.kind}</span></button>`).join('')
    || `<p class="empty" style="padding:18px">Nothing matches “${esc(pal.q)}”.</p>`;
  $('palList').querySelector('.sel')?.scrollIntoView({ block: 'nearest' });
}
function runPalette(i) {
  const it = pal?.items[i];
  closePalette();
  it?.run();
}
function flashMod(id) {
  requestAnimationFrame(() => {
    const row = document.querySelector(`.row[data-mod="${CSS.escape(id)}"]`);
    if (!row) return;
    row.scrollIntoView({ block: 'center', behavior: 'smooth' });
    row.classList.add('flashrow');
    setTimeout(() => row.classList.remove('flashrow'), 1400);
  });
}

const findMod = id => modsOf(game()).find(m => m.id === id);
let wipeArmed = false;

document.addEventListener('click', e => {
  const a = e.target.closest('a[target="_blank"]');
  if (a && invoke) { e.preventDefault(); invoke('plugin:opener|open_url', { url: a.href }).catch(console.error); return; }
  if (e.target.closest('#whatsNew') && (e.target.id === 'whatsNew' || e.target.closest('#wnClose'))) { $('whatsNew').hidden = true; return; }
  if (profileMenu && !e.target.closest('.profile')) { profileMenu = newProfile = importProfile = false; renderGame(); }
  if (pal) {
    const item = e.target.closest('[data-pal]');
    if (item) runPalette(+item.dataset.pal);
    else if (!e.target.closest('.pal-box')) closePalette();
    return;
  }
  const t = e.target.closest('button');
  if (!t) return;
  const d = t.dataset;
  if (capture && !d.bind && t.id !== 'panicBind' && t.id !== 'sheetBind') { capture = null; lastHotkeys = ''; render(); }
  if (d.game) go('game', d.game);
  else if (d.view) go(d.view);
  else if (d.flip) { const g = game(), m = findMod(d.flip); setMod(g, m, !valueOf(g, m)); }
  else if (d.act) setMod(game(), findMod(d.act));
  else if (d.bind) startCapture(game().id, d.bind, d.dir);
  else if (d.chip) { state.cat = d.chip; save(); renderChips(); renderMods(); }
  else if (d.cat) { state.closed[d.cat] = !state.closed[d.cat]; save(); renderMods(); }
  else if (d.libfilter) { state.libFilter = d.libfilter; save(); renderLibrary(); }
  else if (d.accent) { state.accent = d.accent; save(); render(); }
  else if (d.profile) switchProfile(game(), d.profile);
  else if (d.pin) {
    const g = game(), p = (state.pinned[g.id] ||= {});
    if (p[d.pin]) delete p[d.pin]; else p[d.pin] = true;
    save(); renderMods(); if (state.mini) renderMini();
    toast(findMod(d.pin).name, p[d.pin] ? 'pinned' : 'unpinned');
  }
  else if (d.ocorner) { state.overlayCorner = d.ocorner; save(); renderSettings(); invoke?.('overlay_notify', { note: { title: 'Overlay', value: 'Here', on: true, game: 'Mod changes show up here', accent: state.accent, corner: state.overlayCorner } }); }
  else if (d.rcorner) { state.radarCorner = d.rcorner; save(); renderSettings(); if (radarOpen) invoke?.('radar_show', { show: true, corner: state.radarCorner }); else toast('Radar position', CORNERS.find(c => c[0] === d.rcorner)[1]); }
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
    case 'stopBtn':
      if (!stopArmed) { stopArmed = true; renderSide(); setTimeout(() => { stopArmed = false; renderSide(); }, 3000); }
      else {
        stopArmed = false;
        const run = findGame(runningId());
        if (!invoke) setPlaying(null);
        else if (run) invoke('close_game', { id: run.id }).then(n => toast(run.name, n ? 'closed' : "couldn't be closed"));
        renderSide();
      }
      break;
    case 'powerBtn': {
      const run = findGame(runningId());
      if (!run) break;
      state.paused[run.id] = !state.paused[run.id];
      logLine(run, state.paused[run.id] ? 'Mods paused' : 'Mods resumed');
      save(); render(); toast('Mods', state.paused[run.id] ? 'paused' : 'resumed');
      break;
    }
    case 'panelBtn': state.panel = !state.panel; save(); renderGame(); break;
    case 'resetAll': case 'resetBtn': {
      const g = game();
      activeProf(g).values = {}; logLine(g, 'All mods → Off'); save(); render(); toast('All mods', 'off'); break;
    }
    case 'profileBtn': profileMenu = !profileMenu; newProfile = importProfile = false; renderGame(); break;
    case 'addProfile': newProfile = true; importProfile = false; renderGame(); $('pnewName')?.focus(); break;
    case 'importProfile': importProfile = true; newProfile = false; renderGame(); $('pimportCode')?.focus(); break;
    case 'copyProfile': copyText(shareCode(game())).then(ok => toast(ok ? 'Share code copied' : "Couldn't copy", activeProf(game()).name)); profileMenu = false; renderGame(); break;
    case 'sheetBind': startCapture(null, SHEET, 'up'); break;
    case 'whatsNewBtn': showWhatsNew(true); break;
    case 'winMini': setMini(true); break;
    case 'miniExit': setMini(false); break;
    case 'panicBind': startCapture(null, null, 'up'); break;
    case 'resetBinds': state.binds = {}; save(); lastHotkeys = ''; render(); toast('Key bindings', 'reset'); break;
    case 'sortBtn': state.sortAZ = !state.sortAZ; save(); renderSide(); toast('Games sorted', state.sortAZ ? 'A–Z' : 'recently played'); break;
    case 'backBtn': step(-1); break;
    case 'fwdBtn': step(1); break;
    case 'setCounts': state.showCounts = !state.showCounts; save(); render(); break;
    case 'setHotkeys': state.hotkeys = !state.hotkeys; save(); render(); break;
    case 'setSoftware': state.showSoftware = !state.showSoftware; save(); render(); break;
    case 'setTray': state.closeToTray = !state.closeToTray; save(); invoke?.('set_close_to_tray', { enabled: state.closeToTray }); render(); break;
    case 'setAutostart':
      invoke('set_autostart', { enabled: !AUTOSTART })
        .then(() => { AUTOSTART = !AUTOSTART; toast('Start with Windows', AUTOSTART ? 'on' : 'off'); })
        .catch(err => toast("Couldn't change startup", String(err)))
        .finally(render);
      break;
    case 'homeRefresh': refreshHome(true); break;
    case 'setAutoUpdate': state.autoUpdate = !state.autoUpdate; save(); render(); break;
    case 'setSounds': state.sounds = !state.sounds; save(); render(); if (state.sounds) chime(true); break;
    case 'setOverlay': state.overlay = !state.overlay; save(); render(); if (!state.overlay) invoke?.('overlay_close'); break;
    case 'testOverlay': invoke?.('overlay_notify', { note: { title: 'Hotswap overlay', value: 'On', on: true, game: 'This is how mod changes look in game', accent: state.accent, corner: state.overlayCorner } }); chime(true); break;
    case 'checkUpdateBtn': checkUpdate(true); break;
    case 'updateBtn': case 'updatePill': installUpdate(); break;
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
  if (e.target.id === 'pimportForm') { e.preventDefault(); importCode(game(), $('pimportCode').value); return; }
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
document.addEventListener('dblclick', e => {
  const id = e.target.dataset?.range;
  if (id) { const m = findMod(id); setMod(game(), m, m.def); }
});
document.addEventListener('input', e => {
  if (e.target.dataset.range) {
    const m = findMod(e.target.dataset.range), v = parseFloat(e.target.value);
    e.target.style.setProperty('--p', pct(m, v));
    e.target.closest('.slider').querySelector('[data-num]').value = fmt(m, v);
  }
  if (e.target.id === 'search') render();
  if (e.target.id === 'palInput') { pal.q = e.target.value.trim().toLowerCase(); pal.sel = 0; renderPalette(); }
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
  if (pal) {
    if (e.key === 'Escape') { e.preventDefault(); closePalette(); }
    else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      pal.sel = (pal.sel + (e.key === 'ArrowDown' ? 1 : -1) + pal.items.length) % Math.max(1, pal.items.length);
      renderPalette();
    } else if (e.key === 'Enter') { e.preventDefault(); runPalette(pal.sel); }
    return;
  }
  if (e.ctrlKey && e.key.toLowerCase() === 'k') { e.preventDefault(); openPalette(); return; }
  const typing = /INPUT|TEXTAREA/.test(document.activeElement.tagName);
  if (e.key === '/' && !typing) { e.preventDefault(); $('search').focus(); $('search').select(); return; }
  if (e.key === 'Escape' && !$('whatsNew').hidden) { $('whatsNew').hidden = true; return; }
  if (e.key === 'Escape' && profileMenu) { profileMenu = newProfile = importProfile = false; renderGame(); return; }
  if (e.key === 'Escape' && document.activeElement.id === 'search') { e.target.value = ''; render(); e.target.blur(); return; }
  if (e.altKey && e.key === 'ArrowLeft') { step(-1); return; }
  if (e.altKey && e.key === 'ArrowRight') { step(1); return; }
  // In the desktop app the system-wide listener handles hotkeys, even while Hotswap has focus.
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
ICONS.pinned = '<path d="M15 3l6 6-3 1-4 4 1 5-2 2-4-4-5 5H3v-1l5-5-4-4 2-2 5 1 4-4z"/>';
const icon = (k, s = 16) => `<svg width="${s}" height="${s}" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">${ICONS[k] || ICONS.dot}</svg>`;

/* ---------- Boot ---------- */
(async () => {
  if (!appWin) $('winctl').hidden = true;
  await loadState();
  await scanLibrary();
  await loadHome();
  if (!findGame(state.game)) state.game = LIB[0]?.id ?? null;
  if (invoke) {
    state.playing = null;
    invoke('set_close_to_tray', { enabled: state.closeToTray }).catch(console.error);
    AUTOSTART = await invoke('get_autostart').catch(() => false);
    VERSION = await invoke('app_version').catch(() => '');
    if (state.autoUpdate) setTimeout(() => checkUpdate(false), 4000);
    // First start after an update: show what changed.
    if (VERSION && state.lastVersion && state.lastVersion !== VERSION) setTimeout(() => showWhatsNew(false), 800);
    if (VERSION) { state.lastVersion = VERSION; save(); }
    listen?.('game-running', e => onRunning(e.payload ?? null));
    listen?.('hotkey', e => onCombo(e.payload));
    listen?.('mod-progress', e => Mods.onProgress(e.payload));
    listen?.('trainer-status', e => { TRAINER = e.payload; if (state.view === 'game') { renderMods(); renderPanel(); } if (state.mini) renderMini(); });
    listen?.('hotkey-up', e => { if (e.payload === state.sheetKey) showSheet(false); });
    onRunning(await invoke('running_game').catch(() => null));
  } else if (state.playing && !findGame(state.playing)) state.playing = null;
  nav.stack = [{ view: state.view, game: state.game }]; nav.i = 0;
  render();
  if (state.mini) setMini(true);
})().catch(err => {
  console.error(err);
  document.querySelectorAll('.view').forEach(v => v.hidden = v.id !== 'mapsView');
  $('mapsBody').innerHTML = `<div class="empty-page"><h1>Hotswap couldn't start</h1>
    <p>${esc(err?.message || err)}</p><button class="btn" onclick="location.reload()">Try again</button></div>`;
});
