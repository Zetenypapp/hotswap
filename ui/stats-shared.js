// Shared by the main window and the overlay: which system stats exist, how they're shown, the
// default settings, and the graph drawing (line, area, bars, steps or number only).

const statGb = b => (b / 1073741824).toFixed(1);
const statRate = b => b >= 1048576 ? `${(b / 1048576).toFixed(1)} MB/s` : `${Math.round(b / 1024)} KB/s`;
const statTemp = (c, units) => units === 'f' ? `${Math.round(c * 9 / 5 + 32)}°F` : `${Math.round(c)}°C`;

// id, name, group, read(snapshot), top of the graph (fixed, from the snapshot, or following the
// data), format, and what to say when this PC doesn't report it.
const NO_CPU = 'Run LibreHardwareMonitor with its Remote Web Server on to see this.';
const NO_GPU = 'Your graphics driver doesn’t report this.';
const NO_GAME = 'Shows while a game is running.';
const STAT_METRICS = [
  { id: 'cpu', name: 'CPU', group: 'CPU', get: s => s.cpu, max: 100, fmt: v => `${Math.round(v)}%` },
  { id: 'cpu_temp', name: 'CPU temperature', group: 'CPU', get: s => s.cpu_temp, temp: true, na: NO_CPU },
  { id: 'cpu_power', name: 'CPU power', group: 'CPU', get: s => s.cpu_power, fmt: v => `${Math.round(v)} W`, na: NO_CPU },
  { id: 'ram', name: 'Memory', group: 'Memory', get: s => s.ram_total ? s.ram_used / s.ram_total * 100 : null, max: 100, fmt: v => `${Math.round(v)}%`, sub: s => `${statGb(s.ram_used)} / ${statGb(s.ram_total)} GB` },
  { id: 'gpu', name: 'GPU', group: 'GPU', get: s => s.gpu, max: 100, fmt: v => `${Math.round(v)}%`, sub: s => s.gpu_name || '' },
  { id: 'gpu_temp', name: 'GPU temperature', group: 'GPU', get: s => s.gpu_temp, temp: true, na: NO_GPU },
  { id: 'gpu_hotspot', name: 'GPU hotspot', group: 'GPU', get: s => s.gpu_hotspot, temp: true, na: NO_GPU },
  { id: 'gpu_power', name: 'GPU power', group: 'GPU', get: s => s.gpu_power, fmt: v => `${Math.round(v)} W`, na: NO_GPU },
  { id: 'gpu_clock', name: 'GPU clock', group: 'GPU', get: s => s.gpu_clock, fmt: v => `${Math.round(v)} MHz`, na: NO_GPU },
  { id: 'gpu_fan', name: 'GPU fan', group: 'GPU', get: s => s.gpu_fan, max: 100, fmt: v => `${Math.round(v)}%`, na: NO_GPU },
  { id: 'vram', name: 'Video memory', group: 'GPU', get: s => s.vram_used, maxOf: s => s.vram_total, fmt: v => `${statGb(v)} GB`, na: NO_GPU },
  { id: 'disk_read', name: 'Disk read', group: 'Disk and network', get: s => s.disk_read, fmt: statRate },
  { id: 'disk_write', name: 'Disk write', group: 'Disk and network', get: s => s.disk_write, fmt: statRate },
  { id: 'net_down', name: 'Download', group: 'Disk and network', get: s => s.net_down, fmt: statRate },
  { id: 'net_up', name: 'Upload', group: 'Disk and network', get: s => s.net_up, fmt: statRate },
  { id: 'game_cpu', name: 'Game CPU', group: 'Running game', get: s => s.game?.cpu, max: 100, fmt: v => `${Math.round(v)}%`, na: NO_GAME },
  { id: 'game_ram', name: 'Game memory', group: 'Running game', get: s => s.game?.ram, fmt: v => `${statGb(v)} GB`, na: NO_GAME },
  { id: 'game_gpu', name: 'Game GPU', group: 'Running game', get: s => s.game?.gpu, max: 100, fmt: v => `${Math.round(v)}%`, na: NO_GAME },
];
const STAT_BY_ID = Object.fromEntries(STAT_METRICS.map(m => [m.id, m]));
const STAT_COLORS = ['', '#3ddc84', '#4da3ff', '#b57bff', '#ff6b9a', '#ff9f43', '#ffd84d', '#2ee6d6', '#f2f4f7'];
const STAT_COLOR_NAMES = ['Accent', 'Green', 'Blue', 'Purple', 'Pink', 'Orange', 'Yellow', 'Teal', 'White'];
const STAT_STYLES = [['line', 'Line'], ['area', 'Area'], ['bars', 'Bars'], ['step', 'Steps'], ['number', 'Number']];

const STAT_DEFAULT = {
  order: STAT_METRICS.map(m => m.id),
  tab: ['cpu', 'ram', 'gpu', 'gpu_temp', 'gpu_power', 'vram', 'net_down', 'game_cpu', 'game_ram'],
  hud: [],
  widget: ['cpu', 'gpu', 'gpu_temp', 'game_ram'],
  colors: {}, styles: {}, sizes: {},
  style: 'area', window: 60, rate: 1000, units: 'c',
  widgetOn: false, widgetWhen: 'games', widgetCorner: 'top-right', widgetSize: 'm', widgetOpacity: 90,
};
// The user's settings over the defaults (new stats in an update join the end of their order).
function statConfig(saved) {
  const c = { ...STAT_DEFAULT, ...(saved || {}) };
  c.order = [...c.order.filter(id => STAT_BY_ID[id]), ...STAT_DEFAULT.order.filter(id => !c.order.includes(id))];
  return c;
}

function statValue(m, s) {
  const v = s ? m.get(s) : null;
  return v == null || !Number.isFinite(v) ? null : v;
}
function statText(m, v, units) {
  if (v == null) return '–';
  return m.temp ? statTemp(v, units) : m.fmt(v);
}

// A graph of the readings in `vals` (oldest first, null for gaps) for `slots` time slots.
function statGraph(vals, slots, top, style, color) {
  const c = color || 'var(--c)';
  const pts = vals.filter(v => v != null);
  if (pts.length < 2 || style === 'number') return '';
  const max = top || Math.max(...pts, 1) * 1.15;
  const off = slots - vals.length;
  const x = i => (120 * (i + off) / (slots - 1)).toFixed(2), y = v => (38 - 36 * Math.min(Math.max(v, 0), max) / max).toFixed(2);
  if (style === 'bars') {
    const w = Math.max(0.6, 120 / slots - 0.6);
    return `<svg viewBox="0 0 120 40" preserveAspectRatio="none">${vals.map((v, i) => v == null ? '' : `<rect x="${(120 * (i + off) / slots).toFixed(2)}" y="${y(v)}" width="${w.toFixed(2)}" height="${(40 - y(v)).toFixed(2)}" fill="${c}" opacity=".8"/>`).join('')}</svg>`;
  }
  let line = '';
  vals.forEach((v, i) => {
    if (v == null) return;
    if (style === 'step' && line) line += ` ${x(i)},${line.split(' ').pop().split(',')[1]}`;
    line += ` ${x(i)},${y(v)}`;
  });
  line = line.trim();
  const first = vals.findIndex(v => v != null);
  const fill = style === 'area' || style === 'step' ? `<polygon points="${x(first)},40 ${line} ${x(vals.length - 1)},40" fill="${c}" opacity=".18"/>` : '';
  return `<svg viewBox="0 0 120 40" preserveAspectRatio="none">${fill}<polyline points="${line}" fill="none" stroke="${c}" stroke-width="1.6" vector-effect="non-scaling-stroke" stroke-linejoin="round"/></svg>`;
}

// Recent readings per stat, kept for the longest time window (5 minutes at 2 a second).
const STAT_HISTORY = {};
function statRecord(s) {
  for (const m of STAT_METRICS) {
    const h = STAT_HISTORY[m.id] ||= [];
    h.push(statValue(m, s));
    if (h.length > 600) h.shift();
  }
}
// The last `seconds` of a stat's readings, as many as fit at `rate` ms each.
function statSlice(id, seconds, rate) {
  const n = Math.max(2, Math.round(seconds * 1000 / rate));
  return { vals: (STAT_HISTORY[id] || []).slice(-n), slots: n };
}

// One stat as a card: name, value, extra line, graph.
function statCard(m, s, cfg, size, extraClass = '') {
  const v = statValue(m, s);
  const style = cfg.styles[m.id] || cfg.style;
  const top = m.max || (m.maxOf && s && m.maxOf(s)) || null;
  const { vals, slots } = statSlice(m.id, cfg.window, cfg.rate);
  const body = v == null && m.na && !vals.some(x => x != null) ? `<div class="na">${m.na}</div>` : statGraph(vals, slots, top, style, cfg.colors[m.id]);
  const sub = m.sub && s ? m.sub(s) : '';
  return `<div class="sys-card s-${size || 'm'}${extraClass}" style="${cfg.colors[m.id] ? `--c:${cfg.colors[m.id]}` : ''}"><div class="top"><b>${m.name}</b><span>${statText(m, v, cfg.units)}</span></div>${sub ? `<small class="sub">${sub}</small>` : ''}${body}</div>`;
}

// Boost's gears and its rev-counter dial (the overlay menu; the Boost page draws its own). Every gear is a set
// of real changes (boost.rs) the user can edit in Advanced; these are the starting points.
const BOOST_GEARS = [
  { name: 'Off' }, { name: 'Eco' }, { name: 'Sport' }, { name: 'Turbo' }, { name: 'Redline' }, { name: 'Custom' },
];
const BOOST_BASE = { priority: 'above', plan: 'none', game_mode: false, no_capture: false, no_mouse_accel: false, no_transparency: false, eco_steam: false, eco_browsers: false, eco_discord: false, eco_mine: false, eco_apps: [], max_refresh: true, block_win: false, no_sticky: false, pause_wallpaper: true, hide_hotswap: false, no_throttle: true, fast_gpu: false, no_gamebar: false, close_apps: false, freeze_apps: false };
const BOOST_PRESETS = {
  1: {},
  2: { plan: 'high', game_mode: true },
  3: { plan: 'high', game_mode: true, no_capture: true, close_apps: true, freeze_apps: true, eco_steam: true, eco_browsers: true },
  4: { plan: 'ultimate', game_mode: true, no_capture: true, close_apps: true, freeze_apps: true, eco_steam: true, eco_browsers: true, eco_mine: true, no_transparency: true, no_gamebar: true },
  5: { plan: 'high', game_mode: true },
};
// One line on what a gear does, from its settings.
function boostSummary(g) {
  if (!g) return 'Boost does nothing.';
  const parts = [`${{ normal: 'normal', above: 'above normal', high: 'high' }[g.priority] || 'above normal'} priority`];
  if (g.plan === 'high') parts.push('High performance plan');
  if (g.plan === 'ultimate') parts.push('Ultimate Performance plan');
  if (g.close_apps) parts.push('closes your apps');
  if (g.freeze_apps) parts.push('freezes your apps');
  const extras = BOOST_ADV.filter(it => it.k && !it.opts && !['close_apps', 'freeze_apps'].includes(it.k) && g[it.k]).length;
  if (extras) parts.push(`${extras} more setting${extras > 1 ? 's' : ''}`);
  return parts.join(' · ') + '.';
}
function boostGauge(level, accent = '#f3b73f') {
  const custom = level === 5;
  const pt = (r, deg) => [100 + r * Math.cos(deg * Math.PI / 180), 100 - r * Math.sin(deg * Math.PI / 180)].map(n => n.toFixed(1));
  const seg = i => {
    const [a1, a2] = [180 - 36 * i - 1.5, 180 - 36 * (i + 1) + 1.5];
    const [o1, o2, n2, n1] = [pt(88, a1), pt(88, a2), pt(66, a2), pt(66, a1)];
    const color = i === 0 ? 'rgba(255,255,255,.35)' : i === 4 ? '#e0409f' : accent;
    return `<path data-gear="${i}" d="M${o1} A88 88 0 0 1 ${o2} L${n2} A66 66 0 0 0 ${n1} Z" fill="${color}" opacity="${!custom && i <= level ? 1 : .18}"><title>${BOOST_GEARS[i].name}</title></path>`;
  };
  const ticks = Array.from({ length: 21 }, (_, k) => { const [a, b] = [pt(94, 180 - 9 * k), pt(k % 4 ? 97 : 101, 180 - 9 * k)]; return `<line x1="${a[0]}" y1="${a[1]}" x2="${b[0]}" y2="${b[1]}" stroke="${k >= 16 ? '#e0409f' : 'rgba(255,255,255,.5)'}" stroke-width="${k % 4 ? 1 : 2}"/>`; }).join('');
  const [nx, ny] = pt(78, custom ? 90 : 180 - 36 * level - 18);
  return `<svg class="boost-gauge" viewBox="0 0 200 118" role="group" aria-label="Game Boost gear">
    ${ticks}${[0, 1, 2, 3, 4].map(seg).join('')}
    <line x1="100" y1="100" x2="${nx}" y2="${ny}" stroke="${custom ? accent : '#fff'}" stroke-width="3.5" stroke-linecap="round" style="transition: all .3s"/>
    <circle cx="100" cy="100" r="7" fill="#fff"/>
    <text x="100" y="117" text-anchor="middle" font-size="9" fill="rgba(255,255,255,.6)" letter-spacing="2">${custom ? 'CUSTOM' : `GEAR ${level}`}</text>
  </svg>`;
}
// The Custom gear sits next to the dial.
const boostCustomBtn = level => `<button class="btn${level === 5 ? ' accent' : ''}" data-gear="5" title="Your own gear: set it up in Advanced">Custom</button>`;

// Each gear's settings in Advanced (boost.rs), all real changes, applied live and put back after.
const BOOST_ADV = [
  { group: 'Game' },
  { k: 'priority', name: 'Game priority', info: 'High can make sound or input stutter in some games.', opts: [['normal', 'Normal'], ['above', 'Above normal'], ['high', 'High']] },
  { k: 'plan', name: 'Power plan', info: 'Ultimate Performance is Windows’ hidden top plan: HotSwap adds it for the game and removes it after.', opts: [['none', 'Don’t change'], ['high', 'High'], ['ultimate', 'Ultimate']] },
  { k: 'no_throttle', name: 'Never throttle the game', info: 'Windows never runs the game in its power-saving mode, which it sometimes does by mistake.' },
  { k: 'fast_gpu', name: 'Use the fast graphics card', info: 'For laptops with two: the game runs on the dedicated one, from its next start. Stays set for that game; turn this off to remove it.' },
  { group: 'Display and keys' },
  { k: 'max_refresh', name: 'Highest refresh rate', info: 'The monitor the game is on runs at the fastest refresh rate it has (say 144 Hz instead of 60). Back to normal after.' },
  { k: 'block_win', name: 'Block the Windows key', info: 'Only while the game is in front, so you don’t land on the desktop mid-game.' },
  { k: 'no_sticky', name: 'No Sticky Keys popup', info: 'Pressing Shift five times (or holding it) won’t pop up the Sticky or Filter Keys question.' },
  { group: 'Windows' },
  { k: 'game_mode', name: 'Game Mode', info: 'Windows puts the game first and holds back updates and notifications.' },
  { k: 'no_capture', name: 'No background recording', info: 'Stops Xbox Game Bar from recording the last minutes of play in the background.' },
  { k: 'no_mouse_accel', name: 'No mouse acceleration', info: 'Turns off “Enhance pointer precision”, so aim follows your hand exactly.' },
  { k: 'no_transparency', name: 'No transparency effects', info: 'Windows draws plain backgrounds instead of see-through ones.' },
  { k: 'no_gamebar', name: 'No Xbox Game Bar', info: 'The controller’s Xbox button and Win+G don’t open Game Bar while the game is in front.' },
  { group: 'Your apps', info: 'The close and freeze lists are on HotSwap’s Boost page, under Your apps.' },
  { k: 'close_apps', name: 'Close the apps on your list', info: 'They get a normal close, so they can save first. Reopened after, if you like.' },
  { k: 'freeze_apps', name: 'Freeze the apps on your list', info: 'Paused completely, keeping everything they had open. Resumed after.' },
  { group: 'Efficiency mode for background apps', info: 'What Task Manager’s leaf does: less CPU, run slower and cooler, and their memory goes first when the game needs more. They keep working.' },
  { k: 'eco_steam', name: 'Steam’s web helper', info: 'The Steam store and library pages. Steam itself stays normal, your game needs it.' },
  { k: 'eco_browsers', name: 'Browsers', info: 'Chrome, Edge, Firefox, Opera, Brave and Vivaldi. Like their own memory and energy savers, but it works while they run.' },
  { k: 'eco_discord', name: 'Discord', info: 'In a voice call, it can make your voice crackle.' },
  { k: 'eco_mine', name: 'Your own apps', info: 'The apps you picked for Efficiency mode on the Boost page.' },
  { group: 'Other apps' },
  { k: 'pause_wallpaper', name: 'Pause Wallpaper Engine', info: 'Your animated wallpaper stops while you play and starts again after.' },
  { k: 'hide_hotswap', name: 'Tuck HotSwap away', info: 'HotSwap’s window goes to the tray and frees its memory. Mods, hotkeys and the overlay keep working.' },
];
// The 5-way switch picking which gear Advanced edits, then that gear's settings.
function boostEditor(edit, cfg, attr = 'data-badv') {
  return `<div class="badv-edit"><span>Settings for</span><div class="badv-seg big">${[1, 2, 3, 4, 5].map(n => `<button data-bedit="${n}" class="${edit === n ? 'on' : ''}">${BOOST_GEARS[n].name}</button>`).join('')}</div>
    <button class="badv-reset" data-breset="${edit}">Reset ${BOOST_GEARS[edit].name}</button></div>${boostAdvRows(cfg, attr)}`;
}
function boostAdvRows(adv, attr = 'data-badv') {
  adv = { ...BOOST_BASE, ...(adv || {}) };
  const e = s => String(s).replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
  return BOOST_ADV.map(it => it.group
    ? `<div class="badv-group"><b>${e(it.group)}</b>${it.info ? `<span>${e(it.info)}</span>` : ''}</div>`
    : `<div class="badv-row"><div class="badv-txt"><b>${e(it.name)}</b><span>${e(it.info)}</span></div>${it.opts
      ? `<div class="badv-seg">${it.opts.map(([v, l]) => `<button ${attr}="${it.k}" data-val="${v}" class="${adv[it.k] === v ? 'on' : ''}">${e(l)}</button>`).join('')}</div>`
      : `<button class="badv-sw${adv[it.k] ? ' on' : ''}" ${attr}="${it.k}" role="switch" aria-checked="${!!adv[it.k]}"></button>`}</div>`).join('');
}

// What Game Boost is doing right now, as a list (from the backend's status lines).
function boostStatusHtml(lines) {
  const e = s => String(s).replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
  return lines?.length ? `<ul class="boost-status">${lines.map(l => `<li>${e(l)}</li>`).join('')}</ul>` : '';
}
