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

// Boost's gears and its rev-counter dial (the Boost page and the overlay menu). Every gear is a set
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
const GEAR_COLORS = ['#8a919a', '#3fd49a', '#5cb8ff', '#ffb340', '#e0409f', null]; // Custom uses the accent
const GEAR_TAGS = [
  'Your PC as usual. Nothing changes when a game starts.',
  'A light touch: the game goes first, nothing else is touched.',
  'High performance power and Windows Game Mode.',
  'Background apps closed, frozen or slowed. The game gets the PC.',
  'Everything at once. Ultimate power, nothing in the way.',
  'Your own gear, set up exactly how you like it.',
];
const gearColor = n => GEAR_COLORS[n] || 'var(--accent, var(--c))';
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
      <path d="M${C - 4} ${C} L${C} ${C - 122} L${C + 4} ${C} Z" fill="${custom ? 'var(--accent, var(--c))' : '#fff'}" filter="url(#bglow)"/>
    </g>
    <circle cx="${C}" cy="${C}" r="15" fill="url(#bhub)"/><circle cx="${C}" cy="${C}" r="5" fill="#16181c"/>
    <text x="${C}" y="${C + 34}" text-anchor="middle" class="bd-gear" fill="${gearColor(level)}">${gearMark(level)}</text>
  </svg>`;
}
// Swings the needle from where it was to the new gear.
function swingDial() {
  const n = document.querySelector('.bd-needle');
  if (!n) return;
  const to = +n.dataset.to;
  requestAnimationFrame(() => requestAnimationFrame(() => { n.style.transform = `rotate(${to}deg)`; dialAngle = to; }));
}

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
