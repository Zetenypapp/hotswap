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
