// The Boost page: Game Boost's gearbox, what it does right now, Quick clean, every gear's settings
// and the app lists. The backend (boost.rs) does the work; app.js holds the state helpers, and the
// dial and gear colors are in stats-shared.js (the overlay menu draws them too).

let BOOST_LAST_CLEAN = null; // { mb, apps, at } from the last Quick clean
const cleanAgo = t => { const s = (Date.now() - t) / 1000; return s < 60 ? 'just now' : s < 3600 ? `${Math.round(s / 60)} min ago` : `${Math.round(s / 3600)} h ago`; };

function boostAppList(kind, list, pickId, attr, dropAttr, empty) {
  const open = BOOST_APPS && BOOST_PICK === kind;
  return `<div class="bapps-col">
    <div class="bapps-head"><b>${{ close: 'Close', freeze: 'Freeze', eco: 'Efficiency mode' }[kind]}</b><span class="n">${list.length}</span><button class="btn sm" id="${pickId}">${open ? 'Done' : 'Choose'}</button></div>
    <p>${empty}</p>
    <div class="bapps-chips">${list.length ? list.map(n => `<button class="bchip" ${dropAttr}="${esc(n)}" title="Remove">${esc(n.replace(/\.exe$/, ''))} ×</button>`).join('') : '<span class="dim">None yet.</span>'}</div>
    ${open ? `<div class="boost-apps">${BOOST_APPS.length ? BOOST_APPS.map(a => `<label><input type="checkbox" ${attr}="${esc(a.name)}"${list.includes(a.name) ? ' checked' : ''}><span>${esc(a.name.replace(/\.exe$/, ''))}</span><small>${a.mb.toLocaleString()} MB</small></label>`).join('') : '<p class="empty">Looking at what’s running…</p>'}</div>` : ''}
  </div>`;
}

// Suggestions: a gear for this PC, and apps worth handling, from what's running now (refreshed at
// most every 30 seconds while the page shows). Each one does its thing in a click, or goes away.
let BOOST_SUG = null, BOOST_SUG_AT = 0;
const BOOST_SUG_ACTS = {};
const SUG_BROWSERS = ['chrome.exe', 'msedge.exe', 'firefox.exe', 'opera.exe', 'brave.exe', 'vivaldi.exe'];
const SUG_SYNC = { 'onedrive.exe': 'OneDrive', 'dropbox.exe': 'Dropbox', 'googledrivefs.exe': 'Google Drive', 'ms-teams.exe': 'Microsoft Teams', 'creative cloud.exe': 'Adobe Creative Cloud' };
const SUG_KEEP = ['discord.exe', 'steam.exe', 'steamwebhelper.exe', 'spotify.exe'];
function loadBoostSug() {
  if (!invoke || Date.now() - BOOST_SUG_AT < 30000) return;
  BOOST_SUG_AT = Date.now();
  Promise.all([invoke('boost_hw'), invoke('boost_apps', { protect: state.boostProtect !== false })])
    .then(([hw, apps]) => { BOOST_SUG = { hw, apps }; if (state.view === 'boost') render(); }).catch(console.error);
}
const sugGb = mb => mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${mb} MB`;
function boostSuggestions(level, cfg) {
  if (!BOOST_SUG) return [];
  const { hw, apps } = BOOST_SUG, out = [], hidden = state.boostSugHidden || {};
  const listed = n => [...state.boostClose, ...state.boostFreeze, ...state.boostEcoApps].includes(n);
  const add = (id, text, why, btn, act) => { if (!hidden[id]) { out.push({ id, text, why, btn, act }); BOOST_SUG_ACTS[id] = act; } };
  // A gear for this PC.
  if (level === 0) add('gear-sport', 'Try Sport', 'High performance power and Windows Game Mode, nothing closed. A safe start on any PC.', 'Shift to Sport', () => setBoostLevel(2));
  else if (hw.ram_gb <= 12.5 && level < 3) add('gear-turbo', 'Try Turbo', `This PC has ${Math.round(hw.ram_gb)} GB of memory, so closing, freezing and slowing background apps frees the most for the game.`, 'Shift to Turbo', () => setBoostLevel(3));
  if (hw.battery && state.boostBattery === false && level >= 2) add('battery', 'Turn on Easy on battery', 'This PC has a battery. On it, Boost stays at Eco instead of draining it faster.', 'Turn on', () => { state.boostBattery = true; save(); syncBoost(); render(); });
  // Browsers, in Efficiency mode while you play.
  const browsers = apps.filter(a => SUG_BROWSERS.includes(a.name));
  const bmb = browsers.reduce((t, a) => t + a.mb, 0);
  if (level && !cfg.eco_browsers && bmb >= 500) add('eco-browsers', `Slow down ${browsers.map(a => a.name.replace(/\.exe$/, '')).join(' and ')} while you play`, `Using ${sugGb(bmb)} right now. Efficiency mode in ${BOOST_GEARS[level].name} keeps them working, with less CPU and memory.`, 'Turn on', () => setBoostAdv('eco_browsers', true, level));
  // Sync apps, closed while you play.
  for (const a of apps.filter(a => SUG_SYNC[a.name] && !listed(a.name)).slice(0, 2)) {
    add(`close-${a.name}`, `Close ${SUG_SYNC[a.name]} while you play`, `Sync apps can upload and download in the middle of a game. Closed by gears that close apps${cfg.close_apps ? '' : ` (Turbo and Redline; ${BOOST_GEARS[level].name} doesn’t)`}, and reopened after.`, 'Add', () => { state.boostClose = [...new Set([...state.boostClose, a.name])]; save(); syncBoost(); render(); });
  }
  // The biggest other app.
  const big = apps.find(a => a.mb >= 800 && !SUG_BROWSERS.includes(a.name) && !SUG_SYNC[a.name] && !SUG_KEEP.includes(a.name) && !listed(a.name));
  if (big) add(`eco-${big.name}`, `Slow down ${big.name.replace(/\.exe$/, '')} while you play`, `It’s using ${sugGb(big.mb)}. Efficiency mode keeps it running with less CPU and memory, in gears with “Your own apps” on (Redline).`, 'Add', () => { state.boostEcoApps = [...new Set([...state.boostEcoApps, big.name])]; save(); syncBoost(); render(); });
  return out.slice(0, 4);
}

function renderBoost() {
  setAmbient(null);
  const sw = (id, on) => `<button class="switch" role="switch" id="${id}" aria-checked="${!!on}"></button>`;
  const run = findGame(runningId()), live = !!run && BOOST_STATUS.length > 0;
  const level = run ? boostGear() : state.boostLevel, cfg = boostGearCfg(level);
  const perGame = run && state.boostGames?.[run.id] != null;
  const lines = level ? boostLines(cfg) : [];
  const edit = state.boostEdit || 1;
  const kicker = live ? `<span class="bdot live"></span>Live in ${esc(run.name)}${perGame ? ' · its own gear' : ''}`
    : level ? `<span class="bdot"></span>Ready · kicks in when a game starts` : `<span class="bdot off"></span>Off`;
  const stat = (k, v) => `<div class="bstat"><span>${k}</span><b>${v}</b></div>`;
  loadBoostSug();
  const sugs = boostSuggestions(level, cfg);

  $('boostBody').innerHTML = `
  <section class="bhero${live ? ' live' : ''}" style="--g:${gearColor(level)}">
    <div class="bhero-l">
      <div class="bkick">${kicker}</div>
      <h1 class="bname">${BOOST_GEARS[level].name}</h1>
      <p class="btag">${GEAR_TAGS[level]}</p>
      <div class="bstats">
        ${stat('Priority', level ? { normal: 'Normal', above: 'Above normal', high: 'High' }[cfg.priority] : '–')}
        ${stat('Power plan', level ? { none: 'As is', high: 'High', ultimate: 'Ultimate' }[cfg.plan] : '–')}
        ${stat('Settings on', level ? boostCount(cfg) : 0)}
        ${stat('Apps handled', level ? (cfg.close_apps ? state.boostClose.length : 0) + (cfg.freeze_apps ? state.boostFreeze.length : 0) : 0)}
      </div>
      <div class="bhero-btns"><button class="btn accent big" id="boostCleanBtn"><svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M3 21l7-7M14 4l6 6-7 7-6-6zM9 9l6 6"/></svg>Quick clean</button>
        <span class="bclean">${BOOST_LAST_CLEAN ? `${BOOST_LAST_CLEAN.mb > 0 ? `<b>${BOOST_LAST_CLEAN.mb.toLocaleString()} MB</b> freed from ${BOOST_LAST_CLEAN.apps} apps` : 'Nothing to free'} · ${cleanAgo(BOOST_LAST_CLEAN.at)}` : 'Asks background apps to hand back memory they aren’t using.'}</span></div>
    </div>
    <div class="bhero-r">${boostDial(level)}</div>
  </section>

  <div class="bgears">${[0, 1, 2, 3, 4, 5].map(n => {
    const g = boostGearCfg(n);
    return `<button class="bgear${n === level ? ' on' : ''}" data-gear="${n}" style="--g:${gearColor(n)}">
      <span class="bgear-top"><span class="bgear-mark">${gearMark(n)}</span><span class="bgear-bars">${[1, 2, 3, 4].map(b => `<i class="${n && n !== 5 && b <= n ? 'on' : ''}"></i>`).join('')}</span></span>
      <b>${BOOST_GEARS[n].name}</b><span>${n ? `${boostCount(g)} settings` : 'Nothing'}${state.boostGears?.[n] ? ' · edited' : ''}</span></button>`;
  }).join('')}</div>
  ${sugs.length ? `<section class="bcard"><h2>Suggestions</h2><div class="bsug">${sugs.map(s => `<div class="bsug-row"><span class="bsug-i"></span><div class="txt"><b>${esc(s.text)}</b><span>${esc(s.why)}</span></div>
    ${s.id.startsWith('gear-') ? `<button class="btn sm accent" data-gear="${s.id === 'gear-sport' ? 2 : 3}">${s.btn}</button>` : `<button class="btn sm accent" data-sug="${esc(s.id)}">${s.btn}</button>`}
    <button class="bsug-x" data-sugx="${esc(s.id)}" title="Don’t suggest this again" aria-label="Don’t suggest this again">×</button></div>`).join('')}</div></section>` : ''}
  <p class="bnote">Click a gear or the dial to shift. ${run ? `Shifting now sets the gear for ${esc(run.name)} only.` : 'This is the gear for every game; shift in the overlay while you play to give a game its own.'} Everything goes back when the game closes.</p>

  <div class="bgrid">
    <section class="bcard">
      <h2>${live ? 'Doing right now' : level ? 'When a game starts' : 'Boost is off'}</h2>
      ${live ? `<ul class="bdo">${BOOST_STATUS.map(l => `<li><i></i>${esc(l)}</li>`).join('')}</ul>`
        : level ? `<ul class="bdo">${lines.map(l => `<li><i></i>${esc(l)}</li>`).join('')}</ul>`
        : '<p class="dim">Pick a gear and HotSwap tunes your PC each time a game starts, then puts everything back when it closes.</p>'}
    </section>
    <section class="bcard">
      <h2>Safety</h2>
      <div class="bsafe"><div class="txt"><b>Protect recording apps</b><span>OBS, Streamlabs, XSplit, Medal, ShadowPlay, ReLive, Bandicam, Twitch Studio and ShareX are never closed, frozen, slowed or cleaned.</span></div>${sw('setBoostProtect', state.boostProtect !== false)}</div>
      <div class="bsafe"><div class="txt"><b>Easy on battery</b><span>On a laptop’s battery, Boost stays at Eco.</span></div>${sw('setBoostBattery', state.boostBattery !== false)}</div>
      <div class="bsafe"><div class="txt"><b>Reopen closed apps</b><span>Starts them again when you quit the game or shift down.</span></div>${sw('setBoostReopen', state.boostReopen)}</div>
      ${Object.keys(state.boostGames || {}).length ? `<div class="bsafe col"><div class="txt"><b>Games with their own gear</b></div><div class="bapps-chips">${Object.entries(state.boostGames).map(([id, n]) => `<button class="bchip" data-gamegear="${esc(id)}" title="Use the gear for every game again" style="--g:${gearColor(n)}"><i class="gdot"></i>${esc(findGame(id)?.name || id)}: ${BOOST_GEARS[n].name} ×</button>`).join('')}</div></div>` : ''}
    </section>
  </div>

  <section class="bcard">
    <h2>Your apps</h2>
    <div class="bapps">
      ${boostAppList('close', state.boostClose, 'boostPick', 'data-boostapp', 'data-boostdrop', 'A normal close, so they can save first. Browsers, launchers, chat or RGB apps are good picks.')}
      ${boostAppList('freeze', state.boostFreeze, 'freezePick', 'data-freezeapp', 'data-freezedrop', 'Paused completely, keeping everything open. Good for launchers, not for apps you use in game.')}
      ${boostAppList('eco', state.boostEcoApps, 'ecoPick', 'data-ecoapp', 'data-ecodrop', 'Less CPU and memory, but they keep running. Used by gears with “Your own apps” on.')}
    </div>
  </section>

  <section class="bcard" style="--g:${gearColor(edit)}">
    <h2>Tune a gear</h2>
    ${boostEditor(edit, boostGearCfg(edit))}
  </section>`;
  swingDial();
}
