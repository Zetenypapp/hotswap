// Session insights: the running game's frame rate per Boost gear (the Boost scorecard), the recap
// when a game closes, a note when a game updated, save backups, and feedback on experimental mods.

const fmtMB = b => b >= 1 << 30 ? `${(b / (1 << 30)).toFixed(1)} GB` : `${Math.max(1, Math.round(b / (1 << 20)))} MB`;
const SCORE_MIN = 1; // samples before a gear's numbers show: right away
const TICK = 2000;

/* ---------- Sampling the running game, every 2 seconds ---------- */
// { g, start, gear, gears: { [gear]: { n, fps, low } }, mods: Set, snap (latest stats), backedUp }
let RUN = null;

(function sampleLoop() {
  const id = invoke ? RUNNING : null;
  if (RUN && RUN.g !== id) runEnd();
  if (!id) { setTimeout(sampleLoop, TICK); return; }
  const g = findGame(id);
  if (!RUN) { RUN = { g: id, start: Date.now(), gear: 0, gears: {}, mods: new Set(), snap: null }; if (g) updateToast(g); }
  const gear = RUN.gear = boostGear();
  if (g && isLive(g)) {
    for (const m of modsOf(g).filter(m => isActive(g, m))) {
      RUN.mods.add(modName(g, m));
      if (g.experimental && !state.tried?.[g.id]?.[m.id]) { state.tried = { ...state.tried, [g.id]: { ...state.tried?.[g.id], [m.id]: 1 } }; save(); }
    }
    if (!RUN.backedUp && INJECTED === id) { RUN.backedUp = true; backupNow(g, true); }
  }
  if (!feat('boost') && !feat('stats')) { liveRefresh(); setTimeout(sampleLoop, TICK); return; }
  invoke('system_stats_full', { game: id }).then(snap => {
    if (!RUN) return;
    RUN.snap = snap;
    const f = snap?.game?.fps;
    // Skips loading and pause screens, and the overlay menu (most games slow down behind it).
    if (f?.fps >= 5 && !menuOpen) {
      const s = RUN.gears[gear] ||= { n: 0, fps: 0, low: 0 };
      s.n++; s.fps += f.fps; s.low += f.low;
    }
  }).catch(() => {}).finally(() => { liveRefresh(); setTimeout(sampleLoop, TICK); });
})();

// Redraws only the insight cards, so the rest of the page stays put.
function liveRefresh() {
  document.querySelectorAll('[data-insight]').forEach(el => { el.outerHTML = insightCard(el.dataset.insight === 'boost'); });
}

function runEnd() {
  const r = RUN, g = findGame(r.g), ms = Date.now() - r.start;
  RUN = null;
  if (g?.build) state.builds = { ...state.builds, [g.id]: g.build }; // played on this build
  // Scorecard sums per game and gear, halved past an hour of samples so they follow the PC as it is now.
  const score = state.boostScore ||= {};
  for (const [gear, s] of Object.entries(r.gears)) {
    const t = (score[r.g] ||= {})[gear] ||= { n: 0, fps: 0, low: 0 };
    t.n += s.n; t.fps += s.fps; t.low += s.low;
    if (t.n > 1800) { t.n /= 2; t.fps /= 2; t.low /= 2; }
  }
  if (g) {
    const all = Object.values(r.gears).reduce((a, s) => ({ n: a.n + s.n, fps: a.fps + s.fps, low: a.low + s.low }), { n: 0, fps: 0, low: 0 });
    const gear = +(Object.entries(r.gears).sort((a, b) => b[1].n - a[1].n)[0]?.[0] ?? r.gear);
    state.recap = { g: r.g, start: r.start, ms, gear, fps: all.n ? all.fps / all.n : null, low: all.n ? all.low / all.n : null, mods: [...r.mods].slice(0, 12) };
  }
  save(); render();
}

/* ---------- Live session and recap: on Home and the Boost page ---------- */
const avgOf = s => s && s.n >= SCORE_MIN ? { fps: s.fps / s.n, low: s.low / s.n } : null;
const sumOf = list => list.reduce((a, s) => s ? { n: a.n + s.n, fps: a.fps + s.fps, low: a.low + s.low } : a, { n: 0, fps: 0, low: 0 });
const clockOf = ms => { const s = Math.floor(ms / 1000), h = Math.floor(s / 3600), m = Math.floor(s / 60) % 60; return `${h ? `${h}:${String(m).padStart(2, '0')}` : m}:${String(s % 60).padStart(2, '0')}`; };
const statTile = (k, v, cls = '') => `<div class="bstat${cls}"><span>${k}</span><b>${v}</b></div>`;

// Each card carries data-insight, so the sampler can redraw just the card (an empty one waits for a game).
function insightCard(onBoost) {
  const tag = `data-insight="${onBoost ? 'boost' : 'home'}"`;
  const g = RUN && findGame(RUN.g);
  if (g) return liveCard(g, tag);
  const r = !onBoost && state.recap, rg = r && findGame(r.g);
  if (!rg) return `<div ${tag} hidden></div>`;
  const when = new Date(r.start).toLocaleDateString('en-US', { weekday: 'short', month: 'short', day: 'numeric' });
  return `<section class="bcard recap" ${tag} style="--g:${gearColor(r.gear)}">
    <div class="recap-head">${tile(rg, '', false)}<div class="txt"><span class="recap-k">Last session · ${when}</span><b>${esc(rg.name)}</b></div>
      <button class="btn sm" data-game="${esc(rg.id)}">Open</button><button class="bsug-x" data-recapx title="Hide" aria-label="Hide">×</button></div>
    <div class="bstats">${statTile('Played', duration(r.ms))}${statTile('Average FPS', r.fps ? Math.round(r.fps) : '–')}${statTile('1% low', r.low ? Math.round(r.low) : '–')}${statTile('Boost gear', BOOST_GEARS[r.gear]?.name || 'Off')}</div>
    ${r.mods.length ? `<div class="bapps-chips">${r.mods.map(n => `<span class="bchip">${esc(n)}</span>`).join('')}</div>` : ''}
    ${r.fps ? '' : '<p class="dim">No frame rate this time. HotSwap needs Windows’ one-time OK for it: Settings → Hotkeys and overlay → FPS counter.</p>'}
  </section>`;
}

function liveCard(g, tag) {
  const s = RUN.snap, f = s?.game?.fps, gear = RUN.gear, all = avgOf(sumOf(Object.values(RUN.gears)));
  const pct = v => v == null ? '–' : `${Math.round(v)}%`;
  const ram = s?.ram_total ? s.ram_used / s.ram_total * 100 : null;
  const tips = liveTips(g, s, gear);
  return `<section class="bcard recap live" ${tag} style="--g:${gearColor(gear)}">
    <div class="recap-head">${tile(g, '', false)}<div class="txt"><span class="recap-k"><i class="bdot live"></i>Live session</span><b>${esc(g.name)}</b></div>
      <button class="btn sm" data-game="${esc(g.id)}">Open</button></div>
    <div class="bstats">
      ${statTile('Playing', clockOf(Date.now() - RUN.start))}${statTile('FPS now', f ? Math.round(f.fps) : '–')}${statTile('1% low now', f ? Math.round(f.low) : '–')}${statTile('Session average', all ? `${Math.round(all.fps)} · ${Math.round(all.low)}` : '–')}
      ${statTile('CPU', pct(s?.cpu))}${statTile('GPU', pct(s?.game?.gpu ?? s?.gpu))}${statTile('Memory', pct(ram))}${statTile('Boost gear', BOOST_GEARS[gear]?.name || 'Off')}
    </div>
    ${RUN.mods.size ? `<div class="bapps-chips">${[...RUN.mods].map(n => `<span class="bchip">${esc(n)}</span>`).join('')}</div>` : ''}
    ${tips.length ? `<div class="bsug">${tips.map(t => `<div class="bsug-row"><span class="bsug-i"></span><div class="txt"><b>${esc(t.text)}</b><span>${esc(t.why)}</span></div>
      ${t.gear != null ? `<button class="btn sm accent" data-livegear="${t.gear}">Shift to ${BOOST_GEARS[t.gear].name}</button>` : t.clean ? '<button class="btn sm accent" data-liveclean>Quick clean</button>' : ''}</div>`).join('')}</div>` : ''}
  </section>`;
}

// What could run better, from this game's numbers in each gear (saved plus this session) and the PC right now.
function liveTips(g, s, gear) {
  const out = [], f = s?.game?.fps, r = Math.round, name = n => BOOST_GEARS[n].name;
  const score = state.boostScore?.[g.id] || {};
  const inGear = n => avgOf(sumOf([score[n], RUN.gears[n]]));
  const cur = inGear(gear);
  const best = cur && [0, 1, 2, 3, 4, 5].filter(n => n !== gear).map(n => [n, inGear(n)])
    .filter(([, a]) => a && a.low > cur.low * 1.05).sort((a, b) => b[1].low - a[1].low)[0];
  if (best) out.push({ text: `${name(best[0])} ran ${r((best[1].low / cur.low - 1) * 100)}% smoother in this game`, why: `1% lows averaged ${r(best[1].low)} in ${name(best[0])}, against ${r(cur.low)} in ${name(gear)}.`, gear: best[0] });
  else if (!gear) out.push({ text: 'Boost is off', why: 'Try Sport for a while: the scorecard then shows what it does for this game.', gear: 2 });
  if (!s) return out;
  if (!f) { out.push({ text: 'No frame rate yet', why: noFps(s) }); return out.slice(0, 3); }
  const gpu = s.game?.gpu ?? s.gpu, ram = s.ram_total ? s.ram_used / s.ram_total * 100 : 0;
  if (gpu >= 95) out.push({ text: 'Your graphics card is the limit', why: `GPU at ${r(gpu)}%. Boost can’t add much here: a lower resolution, lower graphics settings or DLSS/FSR will.` });
  else if (s.cpu >= 85 && !(gpu >= 80)) out.push({ text: 'Your processor is the limit', why: `CPU at ${r(s.cpu)}%${gpu != null ? `, GPU only at ${r(gpu)}%` : ''}. Higher gears close, freeze and slow background apps to free the CPU.`, gear: gear < 3 || gear === 5 ? 3 : undefined });
  if (f.fps >= 20 && f.low < f.fps * .5) out.push({ text: 'It stutters', why: `1% lows (${r(f.low)}) are under half the frame rate (${r(f.fps)}). ${gear >= 2 && gear < 5 ? 'A frame cap a little under your average, in the game’s settings, usually smooths it.' : 'Sport adds the High performance plan and Game Mode, which often smooth this.'}`, gear: gear < 2 ? 2 : undefined });
  if (ram >= 88) out.push({ text: 'Memory is almost full', why: `${r(ram)}% in use. Quick clean asks background apps to hand back memory they aren’t using.`, clean: true });
  if (s.gpu_temp >= 85 || s.cpu_temp >= 90) out.push({ text: 'Running hot', why: `${s.gpu_temp >= 85 ? `GPU at ${r(s.gpu_temp)} °C` : `CPU at ${r(s.cpu_temp)} °C`}. Hot chips slow themselves down: check fans and airflow.` });
  if (!out.length) out.push({ text: 'Nothing to change', why: `Smooth at ${r(f.fps)} FPS, with lows at ${r(f.low)}, and nothing maxed out.` });
  return out.slice(0, 3);
}

/* ---------- Boost scorecard ---------- */
function scorecard() {
  const score = state.boostScore || {};
  const avg = avgOf;
  const run = runningId();
  const games = Object.keys(score).map(findGame).filter(g => g && Object.values(score[g.id]).some(avg))
    .sort((a, b) => (b.id === run) - (a.id === run) || b.lastPlayed - a.lastPlayed).slice(0, 6);
  const body = games.length ? games.map(g => {
    const base = avg(score[g.id][0]);
    const cells = [0, 1, 2, 3, 4, 5].map(n => [n, avg(score[g.id][n])]).filter(([, a]) => a).map(([n, a]) => {
      const d = n && base ? Math.round((a.low / base.low - 1) * 100) : null;
      return `<div class="sc-gear" style="--g:${gearColor(n)}"><span>${BOOST_GEARS[n].name}</span><b>${Math.round(a.fps)} <small>fps</small></b><em>1% low ${Math.round(a.low)}${d != null ? ` · <i class="${d >= 0 ? 'up' : 'down'}">${d >= 0 ? '+' : ''}${d}%</i>` : ''}</em></div>`;
    }).join('');
    return `<div class="sc-row">${tile(g, '', false)}<b class="sc-name">${esc(g.name)}</b><div class="sc-gears">${cells}</div></div>`;
  }).join('') : '<p class="dim">Play with a gear on and HotSwap measures the game’s frame rate in each gear, so you can see what each one really does for you. Play a session on <b>Off</b> too, to compare against.</p>';
  const note = games.length && !games.some(g => avg(score[g.id][0])) ? '<p class="dim">Play a session on <b>Off</b> to see how much each gear adds.</p>' : '';
  return `<section class="bcard"><h2>Scorecard</h2>${body}${note}
    <p class="dim sc-foot">Averages from your own sessions, measured every 5 seconds while the game shows frames. Needs the FPS counter (Settings → Hotkeys and overlay).</p></section>`;
}

/* ---------- Game updates ---------- */
const buildChanged = g => !!(g.build && state.builds?.[g.id] && state.builds[g.id] !== g.build && modsOf(g).length);

function buildNote(g) {
  if (g.build && !state.builds?.[g.id]) { state.builds = { ...state.builds, [g.id]: g.build }; save(); }
  if (!buildChanged(g)) return '';
  return `<div class="ac-note exp upd"><svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.6" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12a9 9 0 1 1-3-6.7L21 8"/><path d="M21 3v5h-5"/></svg>Updated since you last played with HotSwap. Most mods follow updates; if one stopped working, tell us.<button class="ac-btn" data-buildok>Got it</button></div>`;
}

function updateToast(g) {
  if (!buildChanged(g)) return;
  toast(`${g.name} updated`, 'mods may need a check');
  if (state.overlay) overlaySend({ title: `${g.name} updated`, value: 'Mods may need a check', on: false, game: g.name });
}

/* ---------- Feedback on experimental mods ---------- */
function voteBtns(g, m) {
  if (!g.experimental || !m.live || !state.tried?.[g.id]?.[m.id]) return '';
  const v = state.votes?.[g.id]?.[m.id];
  const b = (ok, path) => `<button class="vote${v === (ok ? 'y' : 'n') ? ' on' : ''}${ok ? '' : ' no'}" data-vote="${m.id}" data-ok="${ok ? 1 : 0}" title="${ok ? 'It works: tell us' : 'It doesn’t work: tell us'}" aria-label="${ok ? 'It works' : 'It doesn’t work'}"><svg width="13" height="13" viewBox="0 0 24 24" fill="currentColor"${ok ? '' : ' style="transform:scaleY(-1)"'}>${path}</svg></button>`;
  const thumb = '<path d="M2 10h4v11H2zM8 21h9.3a2 2 0 0 0 2-1.6l1.6-7A2 2 0 0 0 19 10h-5l.9-4.3A1.6 1.6 0 0 0 13.3 4L8 10z"/>';
  return `<span class="votes">${b(true, thumb)}${b(false, thumb)}</span>`;
}

function vote(g, m, ok) {
  state.votes = { ...state.votes, [g.id]: { ...state.votes?.[g.id], [m.id]: ok ? 'y' : 'n' } };
  save(); renderMods();
  const title = `${ok ? 'Works' : 'Doesn’t work'}: ${g.name} / ${m.name}`;
  const body = [
    `**Game:** ${g.name}${g.build ? ` (build ${g.build})` : ''}`, `**Mod:** ${m.name}`, `**HotSwap:** ${VERSION}`,
    `**Result:** ${ok ? 'works' : 'doesn’t work'}`, '',
    ok ? 'Anything else to add?' : 'What happened? Logs help a lot: Settings → Export logs, then attach the file.',
  ].join('\n');
  openUrl(`${REPO_URL}/issues/new?title=${encodeURIComponent(title)}&body=${encodeURIComponent(body)}`);
}

/* ---------- Save backups ---------- */
// Where games keep their saves. Others: the user picks the folder once.
const SAVES = {
  526870: '%LOCALAPPDATA%\\FactoryGame\\Saved\\SaveGames',
  264710: '<install>\\SNAppData\\SavedGames',
  848450: '<install>\\SNAppData\\SavedGames',
  220200: '<install>\\saves',
  457140: '%DOCUMENTS%\\Klei\\OxygenNotIncluded\\save_files',
  3241660: '%USERPROFILE%\\AppData\\LocalLow\\semiwork\\Repo\\saves',
  227300: '%DOCUMENTS%\\Euro Truck Simulator 2\\profiles',
  270880: '%DOCUMENTS%\\American Truck Simulator\\profiles',
  573090: '%APPDATA%\\Stormworks\\saves',
  271590: '%DOCUMENTS%\\Rockstar Games\\GTA V\\Profiles',
};
const savePath = g => state.saveDirs?.[g.id] || SAVES[g.appId] || null;
let BK = { gid: null, data: null, busy: false, armed: null };

function loadBackups(g) {
  BK.gid = g.id;
  invoke('save_backups', { game: g.id, path: savePath(g) })
    .then(d => { if (BK.gid === g.id) { BK.data = d; renderBackups(g); } }).catch(() => {});
}

function backupNow(g, auto) {
  const path = savePath(g);
  if (!path || (auto && state.backupOff?.[g.id])) return;
  BK.busy = true; if (game()?.id === g.id) renderBackups(g);
  invoke('save_backup', { game: g.id, path })
    .then(r => { logLine(g, `Saves backed up (${fmtMB(r.bytes)})`); if (!auto) toast(g.name, 'saves backed up'); })
    .catch(err => { logLine(g, `Save backup skipped: ${err}`); if (!auto) toast('Backup', String(err)); })
    .finally(() => { BK.busy = false; save(); if (game()?.id === g.id) { loadBackups(g); renderPanel(); } });
}

const bkWhen = at => `${new Date(at * 1000).toLocaleDateString('en-US', { month: 'short', day: 'numeric' })}, ${new Date(at * 1000).toTimeString().slice(0, 5)}`;

function restoreBackup(g, at) {
  if (RUNNING === g.id) { toast(g.name, 'close the game first'); return; }
  BK.busy = true; BK.armed = null; renderBackups(g);
  invoke('save_backup_restore', { game: g.id, path: savePath(g), at })
    .then(() => { logLine(g, `Saves restored from ${bkWhen(at)} (the ones there before were backed up)`); toast(g.name, 'saves restored'); })
    .catch(err => toast('Restore', String(err)))
    .finally(() => { BK.busy = false; save(); if (game()?.id === g.id) { loadBackups(g); renderPanel(); } });
}

function renderBackups(g) {
  const card = $('backupCard');
  card.hidden = !invoke || !modsOf(g).length;
  if (card.hidden) return;
  if (BK.gid !== g.id) { BK.data = null; loadBackups(g); }
  const d = BK.data, path = savePath(g);
  let html;
  if (!d) html = '<p class="empty">…</p>';
  else if (!d.folder) html = `<p class="empty">${path ? 'The save folder isn’t there yet. Play once, or pick it yourself.' : `HotSwap doesn’t know where ${esc(g.name)} keeps its saves yet.`}</p>
    <div class="bk-btns"><button class="btn sm" data-bk="pick">Choose save folder</button></div>`;
  else html = `<div class="bk-row"><span>Back up when a session starts</span><button class="switch" role="switch" data-bk="auto" aria-checked="${!state.backupOff?.[g.id]}"></button></div>
    <ul class="active-list bk-list">${d.list.slice(0, 5).map(b => `<li><span>${bkWhen(b.at)}</span><span>${fmtMB(b.bytes)}<button class="btn sm${BK.armed === b.at ? ' danger on' : ''}" data-bkrestore="${b.at}"${BK.busy ? ' disabled' : ''} title="Puts these saves back. The saves there now are backed up first.">${BK.armed === b.at ? 'Sure?' : 'Restore'}</button></span></li>`).join('') || '<p class="empty">No backups yet.</p>'}</ul>
    <div class="bk-btns"><button class="btn sm accent" data-bk="now"${BK.busy ? ' disabled' : ''}>${BK.busy ? 'Backing up…' : 'Back up now'}</button><button class="btn sm" data-bk="open">Open folder</button><button class="btn sm outline" data-bk="pick" title="${esc(d.folder)}">Change folder</button></div>`;
  $('backupMeta').textContent = d?.folder ? `Newest 5 kept, in HotSwap’s own folder. Your saves are only read.` : '';
  $('backupBody').innerHTML = html;
}

/* ---------- Clicks ---------- */
document.addEventListener('click', e => {
  const t = e.target.closest('[data-recapx],[data-livegear],[data-liveclean],[data-buildok],[data-vote],[data-bk],[data-bkrestore]');
  if (!t) return;
  const g = game(), d = t.dataset;
  if ('recapx' in d) { state.recap = null; save(); render(); }
  else if (d.livegear) setBoostLevel(+d.livegear, true);
  else if ('liveclean' in d) boostClean();
  else if ('buildok' in d && g) { state.builds = { ...state.builds, [g.id]: g.build }; save(); renderGame(); }
  else if (d.bkrestore && g) {
    const at = +d.bkrestore;
    if (BK.armed === at) restoreBackup(g, at);
    else { BK.armed = at; renderBackups(g); setTimeout(() => { if (BK.armed === at) { BK.armed = null; if (game()?.id === g.id) renderBackups(g); } }, 4000); }
  }
  else if (d.vote && g) { const m = modsOf(g).find(m => m.id === d.vote); if (m) vote(g, m, d.ok === '1'); }
  else if (d.bk && g) {
    if (d.bk === 'now') backupNow(g, false);
    else if (d.bk === 'open') invoke('save_backup_open', { game: g.id }).catch(err => toast('Backup', String(err)));
    else if (d.bk === 'auto') { state.backupOff = { ...state.backupOff, [g.id]: !state.backupOff?.[g.id] }; save(); renderBackups(g); }
    else if (d.bk === 'pick') invoke('save_backup_pick').then(p => { if (p) { state.saveDirs = { ...state.saveDirs, [g.id]: p }; save(); loadBackups(g); } });
  }
});
