/* HotSwap's Minecraft map, through the HotSwap Link mod: the world as you explore it (saved per world
   and dimension), a seed preview of the rest in single-player, your live position, trail and pins. */
const McMap = (() => {
  const DIMS = [['minecraft:overworld', 'Overworld'], ['minecraft:the_nether', 'Nether'], ['minecraft:the_end', 'End']];
  const REGION = 512;
  const dimName = d => (DIMS.find(x => x[0] === d) || [, d.replace(/^minecraft[:_]/, '').replace(/_/g, ' ')])[1];
  const safe = s => s.slice(0, 80).replace(/[^A-Za-z0-9.-]/g, '_');

  let host, cv, ctx, ui = {}, timers = [], loop = 0, ro, dpr = 1, dirty = true;
  let view = null, drag = null, menu = null, follow = true;
  let st = { state: 'loading' }, worlds = [];
  // What the map shows: a world and dimension (the one you're in, unless you pick another).
  let world = null, dim = 'minecraft:overworld', picked = false;
  let dir = null, explored = new Map(), generated = new Map(), genQueue = false, genFail = '';
  let trail = [], pins = [];
  const prefs = loadPrefs();

  function loadPrefs() {
    const d = { seed: true, grid: true, trail: true, panel: true, death: true,
      mobs: { hostile: true, passive: true, villager: true, item: false, player: true, other: false } };
    try { return Object.assign(d, JSON.parse(localStorage.getItem('hotswap.mcmap') || '{}')); } catch { return d; }
  }
  function savePrefs() { try { localStorage.setItem('hotswap.mcmap', JSON.stringify(prefs)); } catch {} }

  /* ---------- coordinates: world x to the right, z down; view.s = pixels per block ---------- */
  const sx = x => (x - view.cx) * view.s + cv.clientWidth / 2;
  const sy = z => (z - view.cz) * view.s + cv.clientHeight / 2;
  const gx = px => (px - cv.clientWidth / 2) / view.s + view.cx;
  const gz = py => (py - cv.clientHeight / 2) / view.s + view.cz;
  const MIN_S = 1 / 64, MAX_S = 16;
  function zoomAt(px, py, f) {
    const wx = gx(px), wz = gz(py);
    view.s = Math.min(MAX_S, Math.max(MIN_S, view.s * f));
    view.cx = wx - (px - cv.clientWidth / 2) / view.s;
    view.cz = wz - (py - cv.clientHeight / 2) / view.s;
    dirty = true;
  }
  const here = () => st.state === 'ingame' && st.world === world && st.dim === dim;
  const fmt = n => Math.round(n).toLocaleString('en-US').replace(/,/g, ' ');
  const MOBC = { hostile: '#ff5a5a', passive: '#7ddc84', villager: '#e8b84d', item: '#9aa6b8', player: '#4da3ff', other: '#c9ced6' };
  const MOBN = { hostile: 'Hostile', passive: 'Animals', villager: 'Villagers', item: 'Items', player: 'Players', other: 'Other' };
  const clock = t => { const h = Math.floor(t / 1000 + 6) % 24, m = Math.floor(t % 1000 * 60 / 1000); return `${h}:${String(m).padStart(2, '0')}`; };

  /* ---------- data ---------- */
  // Where the arrow is drawn: eases toward the last position read, so it glides between polls.
  let shown = null;
  let polling = false;
  async function poll() {
    if (polling) return;
    polling = true;
    let s;
    try { s = await invoke('mc_status'); } catch { s = null; } finally { polling = false; }
    if (!s) return;
    // Keep what only the chunk sync knows.
    if (s.state === 'ingame' && st.state === 'ingame' && s.world === st.world) s.changed = st.changed;
    const moved = s.state !== st.state || s.world !== st.world || s.dim !== st.dim;
    st = s;
    if (st.state === 'ingame' && st.world === world && st.dim === dim) {
      const last = trail[trail.length - 1];
      if (!last || Math.hypot(st.x - last[0], st.z - last[1]) > 4) { trail.push([st.x, st.z]); trail = trail.slice(-3000); }
    }
    if (moved) { panel(); status(); }
    else if (++pollCount % 5 === 0) { panel(); status(); }
    else status();
    dirty = true;
  }
  let pollCount = 0;
  // Smooth the arrow and the follow camera toward the latest position, every frame.
  function glide() {
    if (!here()) { shown = null; return; }
    if (!shown || Math.hypot(st.x - shown.x, st.z - shown.z) > 64) shown = { x: st.x, z: st.z, yaw: st.yaw };
    const k = 0.35;
    shown.x += (st.x - shown.x) * k;
    shown.z += (st.z - shown.z) * k;
    let dy = ((st.yaw - shown.yaw) % 360 + 540) % 360 - 180;
    shown.yaw += dy * k;
    if (follow) { view.cx = shown.x; view.cz = shown.z; }
    if (Math.abs(st.x - shown.x) + Math.abs(st.z - shown.z) > 0.01 || Math.abs(dy) > 0.1) dirty = true;
  }
  let syncing = false;
  async function sync() {
    // One at a time: a slow sync must not pile up behind the next.
    if (syncing) return;
    syncing = true;
    try { st = await invoke('mc_sync'); } catch (e) { st = { state: 'closed', error: String(e) }; } finally { syncing = false; }
    if (st.state === 'ingame') {
      if (!picked && (world !== st.world || dim !== st.dim)) await show(st.world, st.dim);
      if (st.world === world && st.dim === dim) {
        for (const [rx, rz] of st.changed || []) refresh(explored, rx, rz, 'r');
      }
    } else if (!world) {
      await loadWorlds();
      if (worlds[0]) await show(worlds[0].folder, worlds[0].dims.includes('minecraft_overworld') ? 'minecraft:overworld' : worlds[0].dims[0]);
    }
    if (prefs.seed && here() && st.canGenerate) generateNext();
    status(); panel(); dirty = true;
  }
  async function loadWorlds() { try { worlds = await invoke('mc_worlds'); } catch { worlds = []; } }

  async function show(w, d) {
    const first = world === null;
    world = w; dim = d; dir = null; explored = new Map(); generated = new Map(); trail = []; genFail = '';
    if (first || !view) view = { cx: 0, cz: 0, s: 1 };
    try {
      const r = await invoke('mc_regions', { world: w, dim: d });
      dir = r.dir;
      for (const [x, z] of r.explored) explored.set(`${x},${z}`, { x, z, img: null, v: 0 });
      for (const [x, z] of r.generated) generated.set(`${x},${z}`, { x, z, img: null, v: 0 });
    } catch (e) { toast('Couldn’t open the map', String(e)); }
    if (st.state === 'ingame' && st.world === w && st.dim === d) { view.cx = st.x; view.cz = st.z; }
    else if (explored.size) {
      // Center on what was explored.
      const xs = [...explored.values()];
      view.cx = (xs.reduce((a, r) => a + r.x, 0) / xs.length + 0.5) * REGION; view.cz = (xs.reduce((a, r) => a + r.z, 0) / xs.length + 0.5) * REGION;
    }
    await loadPins();
    panel(); status(); dirty = true;
  }

  const path = (kind, x, z) => `${dir}\\${kind}.${x}.${z}.png`;
  // Loads (or reloads after a change) a region picture; the old one stays drawn until the new one is ready.
  function refresh(map, x, z, kind) {
    if (!dir) return;
    const k = `${x},${z}`;
    const t = map.get(k) || { x, z, img: null, v: 0 };
    map.set(k, t);
    t.v++;
    if (t.img || kind === 'g') load(t, kind);
  }
  function load(t, kind) {
    const img = new Image();
    t.loading = true;
    img.onload = () => { t.img = img; t.loading = false; dirty = true; };
    img.onerror = () => { t.loading = false; };
    img.src = fileSrc(path(kind, t.x, t.z)) + (t.v ? `?v=${t.v}` : '');
  }

  // Seed previews of the regions on screen that don't have one yet, nearest the middle first.
  async function generateNext() {
    if (genQueue || !cv || genFail) return;
    const x0 = Math.floor(gx(0) / REGION), x1 = Math.floor(gx(cv.clientWidth) / REGION);
    const z0 = Math.floor(gz(0) / REGION), z1 = Math.floor(gz(cv.clientHeight) / REGION);
    const cx = view.cx / REGION, cz = view.cz / REGION;
    let best = null, bd = Infinity;
    for (let x = x0; x <= x1; x++) for (let z = z0; z <= z1; z++) {
      if (generated.has(`${x},${z}`)) continue;
      const d = Math.hypot(x + 0.5 - cx, z + 0.5 - cz);
      if (d < bd && d < 12) { bd = d; best = [x, z]; }
    }
    if (!best) return;
    genQueue = true; status();
    try {
      await invoke('mc_generate', { world, dim, rx: best[0], rz: best[1] });
      refresh(generated, best[0], best[1], 'g');
    } catch (e) { genFail = String(e); }
    genQueue = false; status();
    if (prefs.seed && here() && st.canGenerate) setTimeout(generateNext, 50);
  }

  async function loadPins() {
    try { pins = JSON.parse((await invoke('map_pins_load')) || '{}')[pinKey()] || []; } catch { pins = []; }
    renderPins();
  }
  const pinKey = () => `minecraft:${safe(world || '')}|${safe(dim)}`;
  async function savePins() {
    let all = {};
    try { all = JSON.parse((await invoke('map_pins_load')) || '{}'); } catch {}
    all[pinKey()] = pins;
    try { await invoke('map_pins_save', { data: JSON.stringify(all) }); } catch (e) { toast('Could not save pins', String(e)); }
    renderPins(); dirty = true;
  }

  /* ---------- drawing ---------- */
  function frame(id) {
    if (id !== loop || !cv) return;
    requestAnimationFrame(() => frame(id));
    glide();
    if (dirty) draw();
  }
  function draw() {
    dirty = false;
    const w = cv.clientWidth, h = cv.clientHeight;
    if (cv.width !== Math.round(w * dpr) || cv.height !== Math.round(h * dpr)) { cv.width = Math.round(w * dpr); cv.height = Math.round(h * dpr); }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    if (!view) return;
    const accent = getComputedStyle(document.documentElement).getPropertyValue('--accent').trim() || '#3ddc84';
    const x0 = Math.floor(gx(0) / REGION), x1 = Math.floor(gx(w) / REGION), z0 = Math.floor(gz(0) / REGION), z1 = Math.floor(gz(h) / REGION);
    const visible = (t) => t.x >= x0 && t.x <= x1 && t.z >= z0 && t.z <= z1;
    const size = REGION * view.s;
    // Seed preview underneath, a bit faded, so what you really explored stands out.
    if (prefs.seed) {
      ctx.imageSmoothingEnabled = view.s < 0.25;
      ctx.save(); ctx.globalAlpha = 0.62; ctx.filter = 'saturate(.75) brightness(.85)';
      for (const t of generated.values()) {
        if (!visible(t)) continue;
        if (!t.img && !t.loading) load(t, 'g');
        if (t.img) ctx.drawImage(t.img, sx(t.x * REGION), sy(t.z * REGION), size, size);
      }
      ctx.restore();
    }
    ctx.imageSmoothingEnabled = view.s < 1;
    for (const t of explored.values()) {
      if (!visible(t)) continue;
      if (!t.img && !t.loading) load(t, 'r');
      if (t.img) ctx.drawImage(t.img, sx(t.x * REGION), sy(t.z * REGION), size, size);
    }
    if (prefs.grid) grid(w, h);
    // Trail.
    if (prefs.trail && trail.length > 1 && here()) {
      ctx.strokeStyle = accent; ctx.globalAlpha = .55; ctx.lineWidth = 2; ctx.lineJoin = 'round';
      ctx.beginPath(); trail.forEach(([x, z], i) => (i ? ctx.lineTo(sx(x), sy(z)) : ctx.moveTo(sx(x), sy(z)))); ctx.stroke();
      ctx.globalAlpha = 1;
    }
    // Pins.
    for (const p of pins) {
      const px = sx(p.x), py = sy(p.z);
      if (px < -20 || py < -20 || px > w + 20 || py > h + 20) continue;
      ctx.fillStyle = p.color; ctx.strokeStyle = '#0b0d10'; ctx.lineWidth = 2;
      ctx.beginPath(); ctx.arc(px, py - 12, 7, 0, 7); ctx.fill(); ctx.stroke();
      ctx.beginPath(); ctx.moveTo(px - 4, py - 8); ctx.lineTo(px, py); ctx.lineTo(px + 4, py - 8); ctx.fill();
      ctx.font = '600 12px system-ui'; ctx.fillStyle = '#fff'; ctx.shadowColor = '#000'; ctx.shadowBlur = 4;
      ctx.fillText(p.name, px + 10, py - 8); ctx.shadowBlur = 0;
    }
    // Where you died, until you pick it up.
    const sameWorld = st.state === 'ingame' && st.world === world;
    if (prefs.death && sameWorld && st.death && st.death.dim === dim) {
      const px = sx(st.death.x), py = sy(st.death.z);
      if (px > -30 && py > -30 && px < w + 30 && py < h + 30) {
        ctx.strokeStyle = '#ff5a5a'; ctx.lineWidth = 2.5; ctx.beginPath();
        ctx.moveTo(px - 6, py - 6); ctx.lineTo(px + 6, py + 6); ctx.moveTo(px + 6, py - 6); ctx.lineTo(px - 6, py + 6); ctx.stroke();
        if (view.s > 0.4) { ctx.font = '600 11px system-ui'; ctx.fillStyle = '#ff8a8a'; ctx.shadowColor = '#000'; ctx.shadowBlur = 3; ctx.fillText('You died here', px + 10, py + 4); ctx.shadowBlur = 0; }
      }
    }
    // Everyone the server shares (drawn on their own dimension's map).
    const served = new Set();
    if (prefs.mobs.player && sameWorld && st.players) {
      for (const p of st.players) {
        if (p.dim !== dim) continue;
        served.add(p.name);
        dot(p.x, p.z, MOBC.player, 4, p.name, w, h);
      }
    }
    // What's loaded around you: mobs, animals, villagers, dropped items, nearby players.
    if (here() && st.entities) {
      for (const e of st.entities) {
        if (!prefs.mobs[e.k] || (e.k === 'player' && served.has(e.n))) continue;
        dot(e.x, e.z, MOBC[e.k], e.k === 'item' ? 2.5 : e.k === 'player' ? 4 : 3.2, e.k === 'player' || (view.s >= 4 && e.k !== 'item') ? e.n : null, w, h);
      }
    }
    // You: an arrow pointing where you look (yaw 0 = south, 90 = west).
    if (here()) {
      const me = shown || st;
      const px = sx(me.x), py = sy(me.z), a = me.yaw * Math.PI / 180;
      ctx.save(); ctx.translate(px, py); ctx.rotate(a);
      ctx.fillStyle = accent; ctx.strokeStyle = '#0b0d10'; ctx.lineWidth = 2.5;
      ctx.beginPath(); ctx.moveTo(0, 13); ctx.lineTo(9, -9); ctx.lineTo(0, -4); ctx.lineTo(-9, -9); ctx.closePath(); ctx.stroke(); ctx.fill();
      ctx.restore();
    }
  }
  function dot(x, z, color, r, label, w, h) {
    const px = sx(x), py = sy(z);
    if (px < -12 || py < -12 || px > w + 12 || py > h + 12) return;
    ctx.fillStyle = color; ctx.strokeStyle = 'rgba(0,0,0,.65)'; ctx.lineWidth = 1.2;
    ctx.beginPath(); ctx.arc(px, py, r, 0, 7); ctx.fill(); ctx.stroke();
    if (label) { ctx.font = '600 11px system-ui'; ctx.fillStyle = '#fff'; ctx.shadowColor = '#000'; ctx.shadowBlur = 3; ctx.fillText(label, px + r + 3, py - r); ctx.shadowBlur = 0; }
  }
  function grid(w, h) {
    // Chunk lines close up, region lines further out.
    const step = view.s >= 2 ? 16 : view.s >= 0.25 ? REGION : 0;
    if (!step) return;
    ctx.strokeStyle = view.s >= 2 ? 'rgba(255,255,255,.07)' : 'rgba(255,255,255,.06)'; ctx.lineWidth = 1;
    ctx.beginPath();
    for (let x = Math.floor(gx(0) / step) * step; x <= gx(w); x += step) { const p = Math.round(sx(x)) + .5; ctx.moveTo(p, 0); ctx.lineTo(p, h); }
    for (let z = Math.floor(gz(0) / step) * step; z <= gz(h); z += step) { const p = Math.round(sy(z)) + .5; ctx.moveTo(0, p); ctx.lineTo(w, p); }
    ctx.stroke();
  }

  /* ---------- panels ---------- */
  function chip(on, a, k, label, extra = '') {
    return `<button class="lm-chip${on ? ' on' : ''}" data-lm="${a}" data-k="${esc(k)}"${extra}>${label}</button>`;
  }
  function panel() {
    if (!ui.legend) return;
    const saved = worlds.find(x => x.folder === safe(world || ''));
    const dims = [...new Set([...DIMS.map(d => d[0]), ...(saved?.dims || []).map(d => d.replace(/^minecraft_/, 'minecraft:'))])];
    const others = worlds.filter(x => x.folder !== safe(world || ''));
    ui.legend.innerHTML = `
      <div class="lm-sec"><span>World</span></div>
      <p class="lm-note"><b style="color:var(--text)">${esc(st.state === 'ingame' && st.world === world ? st.worldName || world : saved?.name || world || 'None yet')}</b>${here() ? ' · you’re here' : ''}</p>
      ${others.length ? `<div class="lm-chips">${others.slice(0, 6).map(x => chip(false, 'world', x.folder, esc(x.name || x.folder.replace(/^(sp|mp)_/, '')))).join('')}</div>` : ''}
      ${picked && st.state === 'ingame' ? `<div class="lm-chips">${chip(false, 'back', '', 'Back to where you are')}</div>` : ''}
      <div class="lm-sec"><span>Dimension</span></div>
      <div class="lm-chips">${dims.map(d => chip(d === dim, 'dim', d, esc(dimName(d)))).join('')}</div>
      ${here() && st.health != null ? `
      <div class="lm-sec"><span>You</span></div>
      <p class="lm-note">❤ ${st.health}/${st.maxHealth ?? 20} · 🍗 ${st.food ?? '?'} · XP ${st.xp ?? 0}${st.armor ? ` · 🛡 ${st.armor}` : ''}<br>
      ${esc(st.biome || '')} · Day ${st.day ?? '?'} · ${clock(st.time ?? 0)}${st.thundering ? ' · ⛈ Storm' : st.raining ? ' · 🌧 Rain' : ''}</p>` : ''}
      ${here() && st.entities ? `
      <div class="lm-sec"><span>Around you</span></div>
      <div class="lm-chips">${Object.keys(MOBN).filter(k => !st.allowed || st.allowed[k]).map(k => {
        const n = st.entities.filter(e => e.k === k).length + (k === 'player' && st.players ? st.players.filter(p => p.dim === dim).length : 0);
        return chip(prefs.mobs[k], 'mob', k, `<i style="background:${MOBC[k]};width:8px;height:8px;border-radius:50%;display:inline-block"></i> ${MOBN[k]}${n ? ` <em>${n}</em>` : ''}`);
      }).join('')}${st.death ? chip(prefs.death, 'death', '', 'Death spot') : ''}</div>
      ${st.allowed && Object.keys(MOBN).some(k => !st.allowed[k]) ? `<p class="lm-note">This server doesn’t allow showing ${Object.keys(MOBN).filter(k => !st.allowed[k]).map(k => MOBN[k].toLowerCase()).join(', ')}.</p>` : ''}` : ''}
      <div class="lm-sec"><span>Map</span><button class="lm-link" data-lm="cleartrail">Clear trail</button></div>
      <div class="lm-chips">${chip(prefs.seed, 'seed', '', 'Seed preview')}${chip(prefs.grid, 'grid', '', 'Grid')}${chip(prefs.trail, 'trail', '', 'Your trail')}</div>
      <p class="lm-note">${here() && st.canGenerate ? 'The faded parts come from your world’s seed: where you haven’t been yet. They fill in for real as you explore.'
        : 'The map fills in as you explore. Seed preview works in single-player (Overworld and End); servers don’t share their seed.'}</p>
      ${genFail ? `<p class="lm-note">Preview stopped: ${esc(genFail)}</p>` : ''}`;
  }
  const PIN_COLORS = ['#3ddc84', '#4da3ff', '#ffb02e', '#ff5c7a', '#b98cff', '#e8e8e8'];
  function renderPins() {
    if (!ui.pins) return;
    ui.pins.innerHTML = pins.length ? pins.map(p => `
      <div class="lm-pin"><button class="lm-pinname" data-lm="pinfly" data-k="${p.id}" title="Show on the map"><i style="background:${p.color}"></i>${esc(p.name)} <em>${fmt(p.x)}, ${fmt(p.z)}</em></button>
        <button class="lm-mini" data-lm="pindel" data-k="${p.id}" title="Delete">✕</button></div>`).join('')
      : '<p class="lm-note">Right-click the map to drop a pin, or pin where you stand.</p>';
  }
  function addPin(x, z, y) {
    const n = prompt('Pin name', `Pin ${pins.length + 1}`);
    if (n === null) return;
    pins.push({ id: Date.now().toString(36), name: n.slice(0, 40) || 'Pin', x: Math.round(x), z: Math.round(z), y: y == null ? null : Math.round(y), color: PIN_COLORS[pins.length % PIN_COLORS.length] });
    savePins();
  }
  function status() {
    if (!ui.status) return;
    const parts = [];
    if (st.state === 'ingame') {
      parts.push(`<span class="on">You: ${fmt(st.x)}, ${fmt(st.y)}, ${fmt(st.z)} · ${esc(dimName(st.dim))}</span>`);
      if (st.dim === 'minecraft:the_nether') parts.push(`<span>Overworld ≈ ${fmt(st.x * 8)}, ${fmt(st.z * 8)}</span>`);
      else if (st.dim === 'minecraft:overworld') parts.push(`<span>Nether ≈ ${fmt(st.x / 8)}, ${fmt(st.z / 8)}</span>`);
    } else parts.push(`<span>${{ nomod: 'HotSwap Link isn’t installed', closed: 'Minecraft isn’t running', menu: 'In the menu', blocked: 'Map turned off by this server', loading: 'Connecting…' }[st.state] || ''}</span>`);
    if (ui.status.dataset.cursor) parts.push(`<span>Cursor ${ui.status.dataset.cursor}</span>`);
    if (genQueue) parts.push('<span>Previewing from seed…</span>');
    ui.status.innerHTML = parts.join('');
    const empty = !explored.size && !generated.size;
    ui.empty.hidden = !(st.state === 'nomod' || st.state === 'blocked' || (empty && st.state !== 'ingame'));
    if (st.state === 'nomod') {
      ui.empty.style.pointerEvents = 'auto';
      ui.empty.innerHTML = `<b>Add HotSwap Link to Minecraft</b>
        <span>A small client-side Fabric mod that shows your world here. It only shares your own position and what your game has loaded, never other players.</span>
        <ol class="cl-steps" style="text-align:left;margin:6px 0 0"><li>Needs Minecraft Java 26.3 with Fabric Loader and Fabric API</li><li>Put <b>hotswap-link.jar</b> in your instance’s <b>mods</b> folder</li><li>Start Minecraft and join a world</li></ol>`;
    } else if (st.state === 'blocked') {
      ui.empty.style.pointerEvents = 'none';
      ui.empty.innerHTML = '<b>This server turned the map off</b><span>Its admin set is_map_allowed_at_all=false. The map works again in single-player and on other servers.</span>';
    } else if (!ui.empty.hidden) {
      ui.empty.style.pointerEvents = 'none';
      ui.empty.innerHTML = `<b>${st.state === 'menu' ? 'Join a world' : 'Start Minecraft'}</b><span>Your map fills in as you explore, and stays here for next time.</span>`;
    }
  }

  /* ---------- input ---------- */
  function mount(el) {
    unmount();
    host = el; dpr = window.devicePixelRatio || 1;
    host.classList.add('lm');
    host.innerHTML = `
      <canvas class="lm-canvas"></canvas>
      <div class="lm-card lm-layers${prefs.panel ? '' : ' min'}">
        <button class="lm-head" data-lm="panel"><b>Minecraft</b><span class="lm-caret">▾</span></button>
        <div class="lm-body" id="mcLegend"></div>
      </div>
      <div class="lm-tools">
        <button class="lm-btn" data-lm="in" title="Zoom in">+</button>
        <button class="lm-btn" data-lm="out" title="Zoom out">−</button>
        <button class="lm-btn wide" data-lm="find" title="Center on you">Find me</button>
        <button class="lm-btn wide${follow ? ' on' : ''}" data-lm="follow" title="Keep you centered (F)">Follow</button>
      </div>
      <div class="lm-card lm-pins">
        <div class="lm-head"><b>Pins</b><button class="lm-mini" data-lm="pinhere">+ Pin my spot</button></div>
        <div class="lm-body" id="mcPins"></div>
      </div>
      <div class="lm-status" id="mcStatus"></div>
      <div class="lm-empty" id="mcEmpty" hidden></div>`;
    cv = host.querySelector('canvas'); ctx = cv.getContext('2d');
    ui = { status: host.querySelector('#mcStatus'), legend: host.querySelector('#mcLegend'), pins: host.querySelector('#mcPins'), empty: host.querySelector('#mcEmpty') };
    if (!view) view = { cx: 0, cz: 0, s: 1 };
    ro = new ResizeObserver(() => { dirty = true; }); ro.observe(host);
    cv.addEventListener('wheel', e => { e.preventDefault(); const r = cv.getBoundingClientRect(); zoomAt(e.clientX - r.left, e.clientY - r.top, Math.exp(-e.deltaY * 0.0015)); }, { passive: false });
    cv.addEventListener('mousedown', e => { if (e.button !== 0) return; hideMenu(); drag = { x: e.clientX, y: e.clientY, cx: view.cx, cz: view.cz, moved: false }; });
    cv.addEventListener('contextmenu', e => { e.preventDefault(); showMenu(e); });
    cv.addEventListener('dblclick', e => { const r = cv.getBoundingClientRect(); zoomAt(e.clientX - r.left, e.clientY - r.top, 2); });
    cv.addEventListener('mouseleave', () => { delete ui.status.dataset.cursor; status(); });
    host.addEventListener('click', onClick);
    loadWorlds().then(panel);
    renderPins(); status(); sync();
    timers.push(setInterval(sync, 1000), setInterval(poll, 140));
    const id = ++loop;
    requestAnimationFrame(() => frame(id));
  }
  function unmount() {
    loop++;
    timers.forEach(clearInterval); timers = [];
    if (ro) ro.disconnect();
    hideMenu();
    // Leaving frees the pictures; they load again from disk next time.
    for (const m of [explored, generated]) for (const t of m.values()) t.img = null;
    if (host) host.innerHTML = '';
    host = cv = ctx = null;
  }
  addEventListener('mousemove', e => {
    if (!cv) return;
    if (drag) {
      if (Math.abs(e.clientX - drag.x) + Math.abs(e.clientY - drag.y) > 3 && !drag.moved) { drag.moved = true; setFollow(false); }
      if (drag.moved) { view.cx = drag.cx - (e.clientX - drag.x) / view.s; view.cz = drag.cz - (e.clientY - drag.y) / view.s; dirty = true; }
      return;
    }
    if (e.target !== cv) return;
    const r = cv.getBoundingClientRect();
    ui.status.dataset.cursor = `${fmt(gx(e.clientX - r.left))}, ${fmt(gz(e.clientY - r.top))}`;
    status();
  });
  addEventListener('mouseup', () => { drag = null; if (cv && prefs.seed && here() && st.canGenerate) generateNext(); });
  addEventListener('keydown', e => {
    if (!host || e.target.closest('input, textarea')) return;
    if (e.key === 'Escape') hideMenu();
    if (e.key.toLowerCase() === 'f' && !e.ctrlKey) setFollow(!follow);
  });
  function setFollow(on) {
    follow = on;
    host?.querySelector('[data-lm="follow"]')?.classList.toggle('on', on);
    if (on && here()) { view.cx = st.x; view.cz = st.z; dirty = true; }
  }

  async function onClick(e) {
    const b = e.target.closest('[data-lm]');
    if (!b) return;
    const a = b.dataset.lm, k = b.dataset.k;
    if (a === 'in') zoomAt(cv.clientWidth / 2, cv.clientHeight / 2, 1.6);
    if (a === 'out') zoomAt(cv.clientWidth / 2, cv.clientHeight / 2, 1 / 1.6);
    if (a === 'find') {
      if (st.state !== 'ingame') return toast('Not in a world', 'Join a world in Minecraft first');
      if (!here()) { picked = false; await show(st.world, st.dim); }
      view.cx = st.x; view.cz = st.z; view.s = Math.max(view.s, 1); dirty = true;
    }
    if (a === 'follow') { if (!here() && st.state === 'ingame') { picked = false; await show(st.world, st.dim); } setFollow(!follow); }
    if (a === 'panel') { prefs.panel = !prefs.panel; savePrefs(); host.querySelector('.lm-layers').classList.toggle('min', !prefs.panel); }
    if (a === 'dim') { picked = !(st.state === 'ingame' && st.world === world && st.dim === k); setFollow(false); await show(world, k); }
    if (a === 'world') { picked = true; setFollow(false); const w = worlds.find(x => x.folder === k); await show(k, w?.dims.includes('minecraft_overworld') ? 'minecraft:overworld' : (w?.dims[0] || 'minecraft_overworld').replace(/^minecraft_/, 'minecraft:')); }
    if (a === 'back') { picked = false; await show(st.world, st.dim); setFollow(true); }
    if (a === 'seed') { prefs.seed = !prefs.seed; genFail = ''; savePrefs(); if (prefs.seed && here() && st.canGenerate) generateNext(); }
    if (a === 'grid') { prefs.grid = !prefs.grid; savePrefs(); }
    if (a === 'mob') { prefs.mobs[k] = !prefs.mobs[k]; savePrefs(); }
    if (a === 'death') { prefs.death = !prefs.death; savePrefs(); }
    if (a === 'trail') { prefs.trail = !prefs.trail; savePrefs(); }
    if (a === 'cleartrail') trail = [];
    if (a === 'pinhere') here() ? addPin(st.x, st.z, st.y) : toast('Not in this world', 'Join it in Minecraft to pin where you stand');
    if (a === 'pinfly') { const p = pins.find(p => p.id === k); if (p) { setFollow(false); view.cx = p.x; view.cz = p.z; view.s = Math.max(view.s, 1); } }
    if (a === 'pindel') { pins = pins.filter(p => p.id !== k); savePins(); }
    panel(); status(); dirty = true;
  }
  function showMenu(e) {
    hideMenu();
    const r = cv.getBoundingClientRect(), px = e.clientX - r.left, py = e.clientY - r.top;
    const x = Math.floor(gx(px)), z = Math.floor(gz(py));
    const near = pins.find(p => Math.hypot(sx(p.x) - px, sy(p.z) - 12 - py) < 12);
    menu = document.createElement('div');
    menu.className = 'lm-menu';
    menu.innerHTML = `<div class="lm-mt"><b>${near ? esc(near.name) : 'Map spot'}</b><span>${fmt(near ? near.x : x)}, ${fmt(near ? near.z : z)}${here() ? ` · ${fmt(Math.hypot((near ? near.x : x) - st.x, (near ? near.z : z) - st.z))} blocks away` : ''}</span></div>
      ${near ? '<button data-m="del">Delete pin</button>' : '<button data-m="pin">Drop a pin here</button>'}
      <button data-m="copy">Copy coordinates</button><button data-m="center">Center here</button>`;
    menu.style.left = Math.min(px, cv.clientWidth - 220) + 'px'; menu.style.top = Math.min(py, cv.clientHeight - 160) + 'px';
    host.append(menu);
    menu.addEventListener('click', ev => {
      const m = ev.target.closest('[data-m]')?.dataset.m;
      if (m === 'pin') addPin(x, z);
      if (m === 'del') { pins = pins.filter(p => p !== near); savePins(); }
      if (m === 'copy') navigator.clipboard?.writeText(`${near ? near.x : x} ${near?.y ?? '~'} ${near ? near.z : z}`).then(() => toast('Copied', 'Paste it in chat or a /tp command')).catch(() => {});
      if (m === 'center') { setFollow(false); view.cx = x; view.cz = z; dirty = true; }
      hideMenu();
    });
  }
  function hideMenu() { if (menu) { menu.remove(); menu = null; } }

  return { mount, unmount };
})();
