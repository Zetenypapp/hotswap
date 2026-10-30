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
