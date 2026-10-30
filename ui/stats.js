// System stats settings (Overlay page) and the feed for the overlay's stats widget and HUD. The
// stats themselves, their formats and the graphs are in stats-shared.js; the overlay draws them.

const statsCfg = () => statConfig(state.stats);
function saveStats(change) {
  state.stats = { ...statsCfg(), ...change };
  save();
  pushStatsCfg(true);
  render();
}
// Whether the stats widget shows now.
function statsWidgetShown() {
  const c = statsCfg();
  return !!(invoke && state.overlay && c.widgetOn && (c.widgetWhen === 'always' || RUNNING));
}
// The overlay window stays up while the widget is on (see overlaySend in ingame.js).
function statsKeep() { return statsWidgetShown(); }

// Sends the settings to the overlay when they or the running game change.
let lastStatsCfg = '';
function pushStatsCfg(force) {
  if (!invoke) return;
  const msg = { kind: 'statscfg', cfg: statsCfg(), game: runningId() || RUNNING || null, widget: statsWidgetShown() };
  const sig = JSON.stringify(msg);
  if (!force && sig === lastStatsCfg) return;
  lastStatsCfg = sig;
  overlaySend(msg);
}

// Readings for the widget and the HUD, at the chosen rate, only while one of them shows stats.
(function statsLoop() {
  const c = statsCfg();
  const need = statsWidgetShown() || (hudOn && c.hud.length);
  if (need) {
    pushStatsCfg(false);
    invoke('system_stats_full', { game: runningId() || RUNNING || null })
      .then(snap => overlaySend({ kind: 'stats', snap }))
      .catch(() => {})
      .finally(() => setTimeout(statsLoop, c.rate));
    return;
  }
  setTimeout(statsLoop, 1000);
})();

// The settings, on the Overlay page.
function statsSettings() {
  const c = statsCfg();
  const chips = (attr, list, cur) => `<div class="chips">${list.map(([k, l]) => `<button class="chip${String(cur) === String(k) ? ' on' : ''}" data-${attr}="${k}">${l}</button>`).join('')}</div>`;
  const place = (id, p, label) => `<button class="chip sm${c[p].includes(id) ? ' on' : ''}" data-stplace="${p}" data-id="${id}" title="Show in the ${label}">${label}</button>`;
  let group = '';
  const rows = c.order.map((id, i) => {
    const m = STAT_BY_ID[id];
    const head = m.group !== group ? `<div class="st-group">${(group = m.group)}</div>` : '';
    return `${head}<div class="st-row">
      <span class="st-move"><button class="ghost" data-stmove="${id}" data-dir="-1" title="Move up"${i ? '' : ' disabled'}>↑</button><button class="ghost" data-stmove="${id}" data-dir="1" title="Move down"${i < c.order.length - 1 ? '' : ' disabled'}>↓</button></span>
      <b class="st-name">${m.name}</b>
      <span class="st-places">${place(id, 'tab', 'Tab')}${place(id, 'widget', 'Widget')}${place(id, 'hud', 'HUD')}</span>
      <select data-stmsize="${id}" title="Card size in the System tab">${[['s', 'Small'], ['m', 'Medium'], ['l', 'Large']].map(([k, l]) => `<option value="${k}"${(c.sizes[id] || 'm') === k ? ' selected' : ''}>${l}</option>`).join('')}</select>
      <select data-stmstyle="${id}" title="Graph style">${[['', 'Default style'], ...STAT_STYLES].map(([k, l]) => `<option value="${k}"${(c.styles[id] || '') === k ? ' selected' : ''}>${l}</option>`).join('')}</select>
      <span class="st-color"><span class="st-dot" style="background:${c.colors[id] || 'var(--accent)'}"></span><select data-stmcolor="${id}" title="Colour">${STAT_COLORS.map((col, k) => `<option value="${col}"${(c.colors[id] || '') === col ? ' selected' : ''}>${STAT_COLOR_NAMES[k]}</option>`).join('')}</select></span>
    </div>`;
  }).join('');
  return `<div class="set-group"><h2>System stats</h2>
    <div class="set-tip">The overlay’s <b>System tab</b> (the graph button next to Discord), an always-on <b>stats widget</b> in a corner, and the <b>HUD</b> can each show the stats you pick. Temperatures and power come from your graphics driver (NVIDIA or AMD); CPU temperature and power need <a href="https://github.com/LibreHardwareMonitor/LibreHardwareMonitor" ${ext}>LibreHardwareMonitor</a> running with Options → Remote Web Server on.</div>
    <div class="set-row"><div class="txt"><b>Graph style</b><span>For every stat, unless a stat has its own below.</span></div>${chips('ststyle', STAT_STYLES, c.style)}</div>
    <div class="set-row"><div class="txt"><b>Time shown</b><span>How far back the graphs go.</span></div>${chips('stwindow', [[30, '30 s'], [60, '1 min'], [120, '2 min'], [300, '5 min']], c.window)}</div>
    <div class="set-row"><div class="txt"><b>Update every</b><span>Faster is smoother, slower uses less.</span></div>${chips('strate', [[500, '0.5 s'], [1000, '1 s'], [2000, '2 s']], c.rate)}</div>
    <div class="set-row"><div class="txt"><b>Temperature</b></div>${chips('stunits', [['c', '°C'], ['f', '°F']], c.units)}</div>
    <div class="set-row"><div class="txt"><b>Stats widget</b><span>Your widget stats in a corner over the game. It keeps the overlay on screen, which can cost a little smoothness in some games.</span></div><button class="switch" role="switch" id="stWidget" aria-checked="${!!c.widgetOn}"></button></div>
    ${c.widgetOn ? `<div class="set-row"><div class="txt"><b>Widget shows</b></div>${chips('stwhen', [['games', 'Only in games'], ['always', 'Always']], c.widgetWhen)}</div>
    <div class="set-row"><div class="txt"><b>Widget corner</b></div>${chips('stcorner', [['top-left', 'Top left'], ['top-right', 'Top right'], ['bottom-left', 'Bottom left'], ['bottom-right', 'Bottom right']], c.widgetCorner)}</div>
    <div class="set-row"><div class="txt"><b>Widget size</b><span>Small shows numbers only.</span></div>${chips('stwsize', [['s', 'Small'], ['m', 'Medium'], ['l', 'Large']], c.widgetSize)}</div>
    <div class="set-row"><div class="txt"><b>Widget opacity</b></div><input type="range" min="30" max="100" step="5" value="${c.widgetOpacity}" data-stopacity style="max-width:200px"><span class="meta">${c.widgetOpacity}%</span></div>` : ''}
    <div class="set-row st-head"><div class="txt"><b>Stats</b><span>Order, where each one shows, its card size in the System tab, graph style and colour.</span></div><button class="btn" data-streset>Reset</button></div>
    <div class="st-help">
      <div><b>TAB</b> The System tab in the overlay menu: press your overlay key, then the graph button next to Discord. Big graphs, while the menu is open.</div>
      <div><b>WIDGET</b> A small box in a corner that stays on screen while you play, like an FPS counter. Turn it on with <b>Stats widget</b> above.</div>
      <div><b>HUD</b> The slim info bar (game, mods, session time). Turn it on in Settings → HUD; ticked stats show there as short text.</div>
    </div>
    <div class="st-list">${rows}</div>
  </div>`;
}

document.addEventListener('click', e => {
  const t = e.target.closest('[data-ststyle],[data-stwindow],[data-strate],[data-stunits],[data-stwhen],[data-stcorner],[data-stwsize],[data-stplace],[data-stmove],[data-stcolor],[data-streset],#stWidget');
  if (!t) return;
  const d = t.dataset, c = statsCfg();
  if (t.id === 'stWidget') return saveStats({ widgetOn: !c.widgetOn });
  if (d.ststyle) return saveStats({ style: d.ststyle });
  if (d.stwindow) return saveStats({ window: +d.stwindow });
  if (d.strate) return saveStats({ rate: +d.strate });
  if (d.stunits) return saveStats({ units: d.stunits });
  if (d.stwhen) return saveStats({ widgetWhen: d.stwhen });
  if (d.stcorner) return saveStats({ widgetCorner: d.stcorner });
  if (d.stwsize) return saveStats({ widgetSize: d.stwsize });
  if (d.stplace) {
    const list = c[d.stplace].includes(d.id) ? c[d.stplace].filter(x => x !== d.id) : [...c[d.stplace], d.id];
    return saveStats({ [d.stplace]: list });
  }
  if (d.stmove) {
    const order = [...c.order], i = order.indexOf(d.stmove), j = i + +d.dir;
    if (j < 0 || j >= order.length) return;
    [order[i], order[j]] = [order[j], order[i]];
    return saveStats({ order });
  }
  if (d.stcolor) return saveStats({ colors: { ...c.colors, [d.stcolor]: d.col } });
  if ('streset' in d && confirm('Reset all system stats settings?')) { state.stats = null; save(); pushStatsCfg(true); render(); }
});
document.addEventListener('change', e => {
  const t = e.target, c = statsCfg();
  if (t.dataset.stmsize) saveStats({ sizes: { ...c.sizes, [t.dataset.stmsize]: t.value } });
  else if (t.dataset.stmstyle) saveStats({ styles: { ...c.styles, [t.dataset.stmstyle]: t.value } });
  else if (t.dataset.stmcolor) saveStats({ colors: { ...c.colors, [t.dataset.stmcolor]: t.value } });
  else if ('stopacity' in t.dataset) saveStats({ widgetOpacity: +t.value });
});
