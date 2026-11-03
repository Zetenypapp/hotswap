// The Boost page: Game Boost's gearbox, what it does right now, Quick clean, every gear's settings
// and the app lists. The backend (boost.rs) does the work; app.js holds the state helpers.

const GEAR_COLORS = ['#8a919a', '#3fd49a', '#5cb8ff', '#ffb340', '#e0409f', null]; // Custom uses the accent
const GEAR_TAGS = [
  'Your PC as usual. Nothing changes when a game starts.',
  'A light touch: the game goes first, nothing else is touched.',
  'High performance power and Windows Game Mode.',
  'Background apps closed, frozen or slowed. The game gets the PC.',
  'Everything at once. Ultimate power, nothing in the way.',
  'Your own gear, set up exactly how you like it.',
];
const gearColor = n => GEAR_COLORS[n] || 'var(--accent)';
const gearMark = n => n === 5 ? 'C' : n ? String(n) : 'N';

// What a gear changes, as short lines, and how many settings it has on.
function boostLines(g) {
  if (!g) return [];
  const out = [`${{ normal: 'Normal', above: 'Above normal', high: 'High' }[g.priority] || 'Above normal'} game priority`];
  if (g.plan !== 'none') out.push(g.plan === 'ultimate' ? 'Ultimate Performance plan' : 'High performance plan');
  BOOST_ADV.forEach(it => { if (it.k && !it.opts && g[it.k]) out.push(it.name); });
  return out;
}
const boostCount = g => boostLines(g).length - (g.priority === 'normal' ? 1 : 0);

// The big rev counter. Gears 0-4 are the arc; Custom puts the needle straight up.
let dialAngle = -90;
function boostDial(level) {
  const C = 160, pt = (r, deg) => [C + r * Math.cos(deg * Math.PI / 180), C - r * Math.sin(deg * Math.PI / 180)].map(n => n.toFixed(1));
  const arc = (r1, r2, a1, a2) => { const [o1, o2, i2, i1] = [pt(r1, a1), pt(r1, a2), pt(r2, a2), pt(r2, a1)]; return `M${o1} A${r1} ${r1} 0 0 1 ${o2} L${i2} A${r2} ${r2} 0 0 0 ${i1} Z`; };
  const custom = level === 5;
  const segs = [0, 1, 2, 3, 4].map(i => {
    const on = !custom && i <= level;
    return `<path data-gear="${i}" d="${arc(132, 104, 180 - 36 * i - 1.2, 180 - 36 * (i + 1) + 1.2)}" fill="${gearColor(i)}" opacity="${on ? 1 : .13}"${on && i ? ' filter="url(#bglow)"' : ''}><title>${BOOST_GEARS[i].name}</title></path>`;
  }).join('');
  const ticks = Array.from({ length: 41 }, (_, k) => {
    const deg = 180 - 4.5 * k, big = k % 5 === 0, [a, b] = [pt(142, deg), pt(big ? 154 : 148, deg)];
    return `<line x1="${a[0]}" y1="${a[1]}" x2="${b[0]}" y2="${b[1]}" stroke="${k >= 32 ? '#e0409f' : 'rgba(255,255,255,.45)'}" stroke-width="${big ? 2.4 : 1.2}" stroke-linecap="round"/>`;
  }).join('');
  const nums = Array.from({ length: 9 }, (_, k) => { const [x, y] = pt(90, 180 - 22.5 * k); return `<text x="${x}" y="${(+y + 4).toFixed(1)}" text-anchor="middle" class="bd-num${k >= 7 ? ' red' : ''}">${k}</text>`; }).join('');
  const to = custom ? 0 : -90 + 36 * level + 18;
  return `<svg class="bd" viewBox="0 0 320 196" role="group" aria-label="Boost gear">
    <defs><filter id="bglow" x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur stdDeviation="5" result="b"/><feMerge><feMergeNode in="b"/><feMergeNode in="SourceGraphic"/></feMerge></filter>
      <radialGradient id="bhub"><stop offset="0" stop-color="#fff"/><stop offset="1" stop-color="#b9c0c8"/></radialGradient></defs>
    <path d="${arc(134, 102, 180, 0)}" fill="rgba(255,255,255,.04)"/>
    ${ticks}${segs}${nums}
    <text x="${C}" y="${C - 34}" text-anchor="middle" class="bd-x">×1000 rpm</text>
    <g class="bd-needle" data-to="${to}" style="transform: rotate(${dialAngle}deg)">
      <path d="M${C - 4} ${C} L${C} ${C - 122} L${C + 4} ${C} Z" fill="${custom ? 'var(--accent)' : '#fff'}" filter="url(#bglow)"/>
    </g>
    <circle cx="${C}" cy="${C}" r="15" fill="url(#bhub)"/><circle cx="${C}" cy="${C}" r="5" fill="#16181c"/>
    <text x="${C}" y="${C + 34}" text-anchor="middle" class="bd-gear" fill="${gearColor(level)}">${gearMark(level)}</text>
  </svg>`;
}
// Swings the needle from where it was to the new gear.
function swingDial() {
  const n = document.querySelector('#boostBody .bd-needle');
  if (!n) return;
  const to = +n.dataset.to;
  requestAnimationFrame(() => requestAnimationFrame(() => { n.style.transform = `rotate(${to}deg)`; dialAngle = to; }));
}

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
