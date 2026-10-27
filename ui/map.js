/* Hotswap's own Satisfactory map, drawn from the running game's memory: resource nodes, collectibles,
   your factory, your live position and trail, pins, and right-click teleport. No bundled map art. */
const LocalMap = (() => {
  // Per game: world bounds in game units (map x = east, map y = south), game units per meter,
  // and which features it has (a live world scan with resources and buildings, teleporting).
  const GAMES = {
    '526870': { name: 'Satisfactory', world: { x0: -324698.832031, x1: 425301.832031, y0: -375000, y1: 375000 }, unit: 100, scan: true, teleport: true },
    '387990': { name: 'Scrap Mechanic', world: { x0: -4096, x1: 4096, y0: -4096, y1: 4096 }, unit: 1, scan: false, teleport: true },
    '264710': { name: 'Subnautica', world: { x0: -2048, x1: 2048, y0: -2048, y1: 2048 }, unit: 1, scan: false, teleport: false, styles: [['satellite', 'Satellite'], ['depth', 'Depth']] },
  };
  let G = GAMES['526870'], GAME = G.id = '526870', WORLD = G.world, U = G.unit;

  const RES = {
    OreIron: ['Iron ore', '#a9b6c8'], OreCopper: ['Copper ore', '#e8833a'], Stone: ['Limestone', '#e6dcc0'],
    Coal: ['Coal', '#8a8f9a'], OreGold: ['Caterium ore', '#f5c542'], RawQuartz: ['Raw quartz', '#ff8fd6'],
    Sulfur: ['Sulfur', '#c8e64a'], OreBauxite: ['Bauxite', '#d0583c'], OreUranium: ['Uranium', '#4cff7a'],
    SAM: ['SAM', '#9b6bff'], LiquidOil: ['Crude oil', '#5b6cff'], Water: ['Water', '#3fc7ff'],
    NitrogenGas: ['Nitrogen gas', '#7fe7e0'], Geyser: ['Geyser', '#ff6b6b'],
  };
  const PURITY = ['Impure', 'Normal', 'Pure'];
  const THINGS = {
    slug1: ['Blue power slug', '#5bc8ff', 'slugs'], slug2: ['Yellow power slug', '#ffd84d', 'slugs'], slug3: ['Purple power slug', '#c76bff', 'slugs'],
    somersloop: ['Somersloop', '#ff5a5a', 'somersloops'], sphere: ['Mercer sphere', '#7affd4', 'spheres'], pod: ['Crash site', '#ffffff', 'pods'],
  };
  const LINES = { belt: ['Belts', '#f3b73f', 1], pipe: ['Pipes', '#3fc7ff', 1], rail: ['Railways', '#d9dde3', 1.6], power: ['Power lines', '#ffe066', .5], hyper: ['Hypertubes', '#ff9de0', 1] };
  // Building footprints in meters (width, length), by class name prefix.
  const SIZES = [
    ['SmelterMk1', 6, 9], ['ConstructorMk1', 8, 10], ['AssemblerMk1', 10, 15], ['ManufacturerMk1', 18, 20], ['FoundryMk1', 10, 9],
    ['OilRefinery', 10, 20], ['Blender', 18, 16], ['Packager', 8, 8], ['HadronCollider', 24, 38], ['QuantumEncoder', 22, 48], ['Converter', 16, 16],
    ['MinerMk', 6, 14], ['OilPump', 8, 14], ['WaterPump', 20, 19.5], ['FrackingSmasher', 20, 20], ['FrackingExtractor', 4, 4],
    ['GeneratorCoal', 10, 26], ['GeneratorFuel', 20, 20], ['GeneratorNuclear', 36, 43], ['GeneratorBiomass', 8, 8], ['GeneratorGeoThermal', 20, 20],
    ['PowerStorage', 6, 6], ['StorageContainerMk1', 5, 10], ['StorageContainerMk2', 10, 10], ['IndustrialTank', 16, 16], ['PipeStorageTank', 8, 8],
    ['TrainStation', 16, 32], ['TrainDockingStation', 16, 32], ['SpaceElevator', 54, 54], ['TradingPost', 16, 24], ['ResourceSink', 16, 26],
    ['TruckStation', 16, 20], ['DroneStation', 12, 12], ['RadarTower', 10, 10], ['Workbench', 4, 4], ['Mam', 6, 6],
  ];
  const CATS = [
    [/Smelter|Constructor|Assembler|Manufacturer|Foundry|Refinery|Blender|Packager|HadronCollider|QuantumEncoder|Converter/, '#f3b73f', 'Production'],
    [/Miner|OilPump|WaterPump|Fracking/, '#ff8a3d', 'Extraction'],
    [/Generator|PowerStorage|PowerSwitch|PowerPole|PowerTower|Battery/, '#ffe066', 'Power'],
    [/Storage|Tank/, '#6fd3a8', 'Storage'],
    [/Train|Station|Truck|Drone|Hyper|Rail/, '#7aa7ff', 'Transport'],
    [/Splitter|Merger|Lift|Junction|Valve|Pump/, '#9aa6b8', 'Logistics'],
  ];

  let host, cv, ctx, ui = {}, timers = [], world = null, worldAt = 0, me = null, trail = [], pins = [];
  let view = null, dpr = 1, dirty = true, follow = false, hover = null, menu = null, drag = null, ro;
  let worldError = '', loop = 0;
  // Map background from the game's own map texture: an overview image, and full-size tiles for close up.
  let bg = null, bgError = '';
  const prefs = loadPrefs();

  function loadPrefs() {
    const d = { res: {}, purity: [true, true, true], things: { slugs: true, somersloops: true, spheres: true, pods: true }, hideOpened: true,
      terrain: true, mapStyle: 'satellite', buildings: true, lines: { belt: true, pipe: true, rail: true, power: false, hyper: true }, trail: true, panel: true };
    try { return Object.assign(d, JSON.parse(localStorage.getItem('hotswap.map') || '{}')); } catch { return d; }
  }
  function savePrefs() { try { localStorage.setItem('hotswap.map', JSON.stringify(prefs)); } catch {} }
  const resOn = r => prefs.res[r] !== false;

  /* ---------- coordinates ---------- */
  const sx = x => (x - view.cx) * view.s + cv.clientWidth / 2;
  const sy = y => (y - view.cy) * view.s + cv.clientHeight / 2;
  const gx = px => (px - cv.clientWidth / 2) / view.s + view.cx;
  const gy = py => (py - cv.clientHeight / 2) / view.s + view.cy;
  const meters = u => u / U;
  // Whole world in view, beside the layers panel when it's open.
  function fit() {
    const left = prefs.panel && cv.clientWidth > 900 ? 320 : 0;
    const w = (cv.clientWidth || 800) - left, h = cv.clientHeight || 600;
    const s = Math.min(w / (WORLD.x1 - WORLD.x0), h / (WORLD.y1 - WORLD.y0)) * 0.94;
    view = { cx: (WORLD.x0 + WORLD.x1) / 2 - left / 2 / s, cy: (WORLD.y0 + WORLD.y1) / 2, s };
    dirty = true;
  }
  const MIN_S = () => Math.min(cv.clientWidth / (WORLD.x1 - WORLD.x0), cv.clientHeight / (WORLD.y1 - WORLD.y0)) * 0.5;
  const MAX_S = () => 50 / U; // 1 m = 50 px
  function zoomAt(px, py, f) {
    const wx = gx(px), wy = gy(py);
    view.s = Math.min(MAX_S(), Math.max(MIN_S(), view.s * f));
    view.cx = wx - (px - cv.clientWidth / 2) / view.s;
    view.cy = wy - (py - cv.clientHeight / 2) / view.s;
    dirty = true;
  }
  function flyTo(x, y, s) {
    const from = { ...view }, to = { cx: x, cy: y, s: s || Math.max(view.s, 0.4 / U) }, t0 = performance.now();
    const step = () => {
      const k = Math.min(1, Math.max(0, (performance.now() - t0) / 450)), e = 1 - Math.pow(1 - k, 3);
      view.cx = from.cx + (to.cx - from.cx) * e; view.cy = from.cy + (to.cy - from.cy) * e;
      view.s = Math.exp(Math.log(from.s) + (Math.log(to.s) - Math.log(from.s)) * e);
      dirty = true;
      if (k < 1) requestAnimationFrame(step);
    };
    requestAnimationFrame(step);
  }

  /* ---------- data ---------- */
  async function loadWorld() {
    if (!G.scan) { worldError = ''; buildLegend(); return; }
    try {
      world = await invoke('map_world', { game: GAME });
      worldAt = Date.now(); worldError = '';
    } catch (e) { worldError = String(e); if (!world) world = null; }
    buildLegend(); dirty = true;
  }
  let bgTried = 0;
  async function loadBackground() {
    if (bg) return;
    try {
      const r = await invoke('map_background', { game: GAME, style: G.styles ? prefs.mapStyle : null });
      const img = src => { const i = new Image(); i.onload = () => { dirty = true; }; i.src = fileSrc(src); return i; };
      bg = { overview: img(r.overview), tiles: r.tiles.map(([x, y, src]) => ({ x, y, src, img: null })), img };
    } catch (e) { bgError = String(e); buildLegend(); }
    dirty = true;
  }
  function drawBackground() {
    if (!bg || !prefs.terrain) return false;
    const W = WORLD.x1 - WORLD.x0, H = WORLD.y1 - WORLD.y0;
    ctx.imageSmoothingEnabled = true; ctx.imageSmoothingQuality = 'high';
    const o = bg.overview;
    if (o.complete && o.naturalWidth) ctx.drawImage(o, sx(WORLD.x0), sy(WORLD.y0), W * view.s, H * view.s);
    // Close up, draw the full-size tiles that are on screen (loaded the first time they're needed).
    if (view.s * W > o.naturalWidth * 1.2) {
      for (const t of bg.tiles) {
        const x0 = WORLD.x0 + t.x * W / 2, y0 = WORLD.y0 + t.y * H / 2;
        const px = sx(x0), py = sy(y0), pw = W / 2 * view.s, ph = H / 2 * view.s;
        if (px > cv.clientWidth || py > cv.clientHeight || px + pw < 0 || py + ph < 0) continue;
        if (!t.img) t.img = bg.img(t.src);
        if (t.img.complete && t.img.naturalWidth) ctx.drawImage(t.img, px, py, pw, ph);
      }
    }
    // A little darker, so markers stand out on the light terrain.
    ctx.fillStyle = 'rgba(8,10,14,.22)';
    ctx.fillRect(sx(WORLD.x0), sy(WORLD.y0), W * view.s, H * view.s);
    return true;
  }

  async function loadMe() {
    let p = null;
    try { p = await invoke('map_player'); } catch {}
    me = p;
    if (p) {
      const last = trail[trail.length - 1];
      if (!last || Math.hypot(p[0] - last[0], p[1] - last[1]) > 500) { trail.push([p[0], p[1]]); trail = trail.slice(-2000); }
      if (follow) { view.cx = p[0]; view.cy = p[1]; }
      if (G.scan && !world && !worldError.includes('Start')) loadWorld();
      // Some maps (Scrap Mechanic) are only saved once a world loads: try again now and then.
      if (!bg && bgError && Date.now() - (bgTried || 0) > 10000) { bgTried = Date.now(); bgError = ''; loadBackground(); }
    }
    status(); dirty = true;
  }
  async function loadPins() {
    try { pins = JSON.parse((await invoke('map_pins_load')) || '{}')[GAME] || []; } catch { pins = []; }
    renderPins(); dirty = true;
  }
  async function savePins() {
    let all = {};
    try { all = JSON.parse((await invoke('map_pins_load')) || '{}'); } catch {}
    all[GAME] = pins;
    try { await invoke('map_pins_save', { data: JSON.stringify(all) }); } catch (e) { toast('Could not save pins', String(e)); }
    renderPins(); dirty = true;
  }

  /* ---------- drawing ---------- */
  function sizeOf(cls) {
    const m = SIZES.find(([k]) => cls.startsWith(k));
    return m ? [m[1] * 100, m[2] * 100] : [400, 400];
  }
  function catOf(cls) { return CATS.find(([re]) => re.test(cls)) || [null, '#c9ced6', 'Other']; }

  function frame(id) {
    if (id !== loop || !cv) return;
    requestAnimationFrame(() => frame(id));
    if (dirty) draw();
  }
  function draw() {
    dirty = false;
    const w = cv.clientWidth, h = cv.clientHeight;
    if (cv.width !== Math.round(w * dpr) || cv.height !== Math.round(h * dpr)) { cv.width = Math.round(w * dpr); cv.height = Math.round(h * dpr); }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    ctx.clearRect(0, 0, w, h);
    const accent = getComputedStyle(document.documentElement).getPropertyValue('--accent').trim() || '#3ddc84';

    // The game's map, or without it the world area and a soft land hint around everything we know about.
    const terrain = drawBackground();
    if (!terrain) {
      ctx.fillStyle = 'rgba(255,255,255,.025)';
      roundRect(sx(WORLD.x0), sy(WORLD.y0), (WORLD.x1 - WORLD.x0) * view.s, (WORLD.y1 - WORLD.y0) * view.s, 18);
      ctx.fill();
    }
    // Fades out as you zoom in, where it would just be big blobs.
    const hint = Math.max(0, Math.min(1, (0.4 / U - view.s) / (0.3 / U)));
    if (world && hint > 0 && !terrain) {
      ctx.fillStyle = `rgba(120,140,170,${(0.05 * hint).toFixed(3)})`;
      const r = Math.max(6, 120 * U * view.s);
      for (const n of world.nodes) { ctx.beginPath(); ctx.arc(sx(n.x), sy(n.y), r, 0, 7); ctx.fill(); }
      for (const t of world.things) { ctx.beginPath(); ctx.arc(sx(t.x), sy(t.y), r, 0, 7); ctx.fill(); }
    }
    if (!terrain) grid(w, h);

    if (world) {
      // Factory: lines under buildings.
      for (const l of world.lines) {
        if (!prefs.lines[l.kind]) continue;
        const [, col, wdt] = LINES[l.kind] || ['', '#888', 1];
        ctx.strokeStyle = col; ctx.globalAlpha = l.kind === 'power' ? .45 : .85;
        ctx.lineWidth = Math.max(l.kind === 'power' ? .6 : 1.2, wdt * 100 * view.s);
        ctx.beginPath();
        l.pts.forEach((p, i) => i ? ctx.lineTo(sx(p[0]), sy(p[1])) : ctx.moveTo(sx(p[0]), sy(p[1])));
        ctx.stroke();
      }
      ctx.globalAlpha = 1;
      if (prefs.buildings) for (const b of world.buildings) {
        const [bw, bl] = sizeOf(b.class), col = catOf(b.class)[1];
        const x = sx(b.x), y = sy(b.y);
        if (x < -60 || y < -60 || x > w + 60 || y > h + 60) continue;
        const pw = Math.max(3, bw * view.s), pl = Math.max(3, bl * view.s);
        ctx.save(); ctx.translate(x, y); ctx.rotate(b.yaw * Math.PI / 180);
        ctx.fillStyle = col + '55'; ctx.strokeStyle = col; ctx.lineWidth = 1;
        ctx.fillRect(-pl / 2, -pw / 2, pl, pw); ctx.strokeRect(-pl / 2, -pw / 2, pl, pw);
        ctx.restore();
      }
      // Resources.
      const r = Math.min(11, Math.max(3.2, 9 * U * view.s));
      for (const n of world.nodes) {
        if (!resOn(n.res) || !prefs.purity[n.purity]) continue;
        const x = sx(n.x), y = sy(n.y);
        if (x < -20 || y < -20 || x > w + 20 || y > h + 20) continue;
        nodeIcon(x, y, n, r);
      }
      // Collectibles.
      for (const t of world.things) {
        const def = THINGS[t.kind];
        if (!def || !prefs.things[def[2]] || (t.done && prefs.hideOpened)) continue;
        thingIcon(sx(t.x), sy(t.y), t, def[1], r);
      }
    }

    // Trail, pins, you.
    if (prefs.trail && trail.length > 1) {
      ctx.strokeStyle = accent; ctx.globalAlpha = .5; ctx.lineWidth = 2; ctx.setLineDash([4, 5]);
      ctx.beginPath(); trail.forEach((p, i) => i ? ctx.lineTo(sx(p[0]), sy(p[1])) : ctx.moveTo(sx(p[0]), sy(p[1]))); ctx.stroke();
      ctx.setLineDash([]); ctx.globalAlpha = 1;
    }
    for (const p of pins) pinIcon(sx(p.x), sy(p.y), p);
    if (me) {
      const x = sx(me[0]), y = sy(me[1]), pulse = 10 + 4 * Math.sin(performance.now() / 300);
      ctx.fillStyle = accent; ctx.globalAlpha = .18; ctx.beginPath(); ctx.arc(x, y, pulse + 8, 0, 7); ctx.fill(); ctx.globalAlpha = 1;
      ctx.save(); ctx.translate(x, y); ctx.rotate(me[3] * Math.PI / 180);
      ctx.fillStyle = accent; ctx.strokeStyle = '#0b0d10'; ctx.lineWidth = 2;
      ctx.beginPath(); ctx.moveTo(13, 0); ctx.lineTo(-8, -8); ctx.lineTo(-4, 0); ctx.lineTo(-8, 8); ctx.closePath(); ctx.fill(); ctx.stroke();
      ctx.restore();
      dirty = true; // keep the pulse going
    }
    scaleBar(w, h);
  }

  function roundRect(x, y, w, h, r) {
    ctx.beginPath(); ctx.moveTo(x + r, y); ctx.arcTo(x + w, y, x + w, y + h, r); ctx.arcTo(x + w, y + h, x, y + h, r);
    ctx.arcTo(x, y + h, x, y, r); ctx.arcTo(x, y, x + w, y, r); ctx.closePath();
  }
  function niceStep(px) {
    const target = px / view.s;
    const steps = [1, 5, 10, 25, 50, 100, 250, 500, 1000].map(m => m * U);
    return steps.find(s => s >= target) || 1000 * U;
  }
  function grid(w, h) {
    const step = niceStep(70);
    ctx.lineWidth = 1;
    const x0 = Math.ceil(gx(0) / step) * step, y0 = Math.ceil(gy(0) / step) * step;
    for (let x = x0; x < gx(w); x += step) {
      ctx.strokeStyle = Math.round(x / step) % 5 === 0 ? 'rgba(255,255,255,.07)' : 'rgba(255,255,255,.03)';
      ctx.beginPath(); ctx.moveTo(Math.round(sx(x)) + .5, 0); ctx.lineTo(Math.round(sx(x)) + .5, h); ctx.stroke();
    }
    for (let y = y0; y < gy(h); y += step) {
      ctx.strokeStyle = Math.round(y / step) % 5 === 0 ? 'rgba(255,255,255,.07)' : 'rgba(255,255,255,.03)';
      ctx.beginPath(); ctx.moveTo(0, Math.round(sy(y)) + .5); ctx.lineTo(w, Math.round(sy(y)) + .5); ctx.stroke();
    }
  }
  function scaleBar(w, h) {
    const step = niceStep(110), len = step * view.s;
    const x = 18, y = h - 18;
    ctx.fillStyle = 'rgba(11,13,16,.75)'; roundRect(x - 8, y - 24, len + 16, 30, 9); ctx.fill();
    ctx.strokeStyle = 'rgba(255,255,255,.7)'; ctx.lineWidth = 2;
    ctx.beginPath(); ctx.moveTo(x, y - 5); ctx.lineTo(x, y); ctx.lineTo(x + len, y); ctx.lineTo(x + len, y - 5); ctx.stroke();
    ctx.fillStyle = 'rgba(255,255,255,.8)'; ctx.font = '600 11px Figtree, system-ui';
    ctx.fillText(step >= 1000 * U ? `${step / 1000 / U} km` : `${meters(step)} m`, x + 4, y - 7);
  }
  function nodeIcon(x, y, n, r) {
    const col = (RES[n.res] || ['', '#ccc'])[1];
    const rr = n.kind === 'well' ? r * .7 : n.kind === 'deposit' ? r * .75 : r;
    ctx.lineWidth = 2;
    if (n.kind === 'core') {
      ctx.strokeStyle = col; ctx.beginPath(); ctx.moveTo(x, y - rr * 1.3); ctx.lineTo(x + rr * 1.3, y); ctx.lineTo(x, y + rr * 1.3); ctx.lineTo(x - rr * 1.3, y); ctx.closePath(); ctx.stroke();
      return;
    }
    if (n.kind === 'deposit') {
      ctx.fillStyle = col; ctx.strokeStyle = '#0b0d10'; ctx.fillRect(x - rr, y - rr, rr * 2, rr * 2); ctx.strokeRect(x - rr, y - rr, rr * 2, rr * 2);
      return;
    }
    if (n.purity === 2) { ctx.fillStyle = col; ctx.globalAlpha = .28; ctx.beginPath(); ctx.arc(x, y, rr + 4, 0, 7); ctx.fill(); ctx.globalAlpha = 1; }
    ctx.beginPath();
    if (n.kind === 'geyser') { ctx.moveTo(x, y - rr * 1.2); ctx.lineTo(x + rr * 1.1, y + rr * .8); ctx.lineTo(x - rr * 1.1, y + rr * .8); ctx.closePath(); }
    else ctx.arc(x, y, rr, 0, 7);
    if (n.purity === 0) { ctx.strokeStyle = col; ctx.fillStyle = '#0b0d10'; ctx.fill(); ctx.stroke(); }
    else { ctx.fillStyle = col; ctx.strokeStyle = n.purity === 2 ? '#fff' : '#0b0d10'; ctx.fill(); ctx.stroke(); }
  }
  function thingIcon(x, y, t, col, r) {
    const rr = r * 1.05;
    ctx.globalAlpha = t.done ? .35 : 1;
    ctx.fillStyle = col; ctx.strokeStyle = '#0b0d10'; ctx.lineWidth = 2;
    ctx.beginPath();
    if (t.kind === 'pod') { ctx.rect(x - rr, y - rr, rr * 2, rr * 2); }
    else if (t.kind === 'somersloop' || t.kind === 'sphere') {
      for (let i = 0; i < 10; i++) { const a = i * Math.PI / 5 - Math.PI / 2, k = i % 2 ? rr * .55 : rr * 1.35; ctx.lineTo(x + Math.cos(a) * k, y + Math.sin(a) * k); }
      ctx.closePath();
    } else {
      for (let i = 0; i < 6; i++) { const a = i * Math.PI / 3; ctx.lineTo(x + Math.cos(a) * rr, y + Math.sin(a) * rr); }
      ctx.closePath();
    }
    ctx.fill(); ctx.stroke();
    if (t.kind === 'pod' && !t.done) { ctx.fillStyle = '#0b0d10'; ctx.font = `800 ${Math.round(rr * 1.6)}px system-ui`; ctx.textAlign = 'center'; ctx.textBaseline = 'middle'; ctx.fillText('!', x, y + 1); ctx.textAlign = 'start'; ctx.textBaseline = 'alphabetic'; }
    ctx.globalAlpha = 1;
  }
  function pinIcon(x, y, p) {
    ctx.save(); ctx.translate(x, y);
    ctx.fillStyle = p.color; ctx.strokeStyle = '#0b0d10'; ctx.lineWidth = 2;
    ctx.beginPath(); ctx.moveTo(0, 0); ctx.bezierCurveTo(-3, -8, -10, -11, -10, -19); ctx.arc(0, -19, 10, Math.PI, 0); ctx.bezierCurveTo(10, -11, 3, -8, 0, 0); ctx.fill(); ctx.stroke();
    ctx.fillStyle = '#0b0d10'; ctx.beginPath(); ctx.arc(0, -19, 3.5, 0, 7); ctx.fill();
    if (view.s > 0.0015) {
      ctx.font = '700 12px Figtree, system-ui'; const tw = ctx.measureText(p.name).width;
      ctx.fillStyle = 'rgba(11,13,16,.8)'; roundRect(14, -28, tw + 12, 20, 8); ctx.fill();
      ctx.fillStyle = '#fff'; ctx.fillText(p.name, 20, -14);
    }
    ctx.restore();
  }

  /* ---------- picking ---------- */
  function pick(px, py) {
    let best = null, bd = 14;
    const consider = (d, item) => { if (d < bd) { bd = d; best = item; } };
    for (const p of pins) consider(Math.hypot(sx(p.x) - px, sy(p.y) - 19 - py), { type: 'pin', p, x: p.x, y: p.y, z: p.z });
    if (world) {
      for (const t of world.things) {
        const def = THINGS[t.kind];
        if (def && prefs.things[def[2]] && !(t.done && prefs.hideOpened)) consider(Math.hypot(sx(t.x) - px, sy(t.y) - py), { type: 'thing', t, x: t.x, y: t.y, z: t.z });
      }
      for (const n of world.nodes) if (resOn(n.res) && prefs.purity[n.purity]) consider(Math.hypot(sx(n.x) - px, sy(n.y) - py), { type: 'node', n, x: n.x, y: n.y, z: n.z });
      if (!best && prefs.buildings) for (const b of world.buildings) {
        const [bw, bl] = sizeOf(b.class);
        consider(Math.max(0, Math.hypot(sx(b.x) - px, sy(b.y) - py) - Math.min(bw, bl) / 2 * view.s), { type: 'building', b, x: b.x, y: b.y, z: b.z });
      }
    }
    return best;
  }
  function describe(it) {
    const away = me ? ` · ${fmtDist(Math.hypot(it.x - me[0], it.y - me[1]))} away` : '';
    if (it.type === 'pin') return [it.p.name, `Pin${it.p.z != null ? ' · exact spot' : ''}${away}`];
    if (it.type === 'thing') { const d = THINGS[it.t.kind]; return [d[0], `${it.t.kind === 'pod' ? (it.t.done ? 'Opened' : 'Not opened yet') : 'Collectible'}${away}`]; }
    if (it.type === 'node') {
      const n = it.n, name = (RES[n.res] || [n.res])[0];
      const what = { node: 'node', well: 'well (needs a pressurizer)', core: 'pressurizer spot', geyser: 'geothermal geyser', deposit: 'deposit (mine by hand)' }[n.kind];
      return [n.kind === 'geyser' ? 'Geyser' : name, `${n.kind === 'core' ? '' : PURITY[n.purity] + ' '}${what}${away}`];
    }
    const b = it.b; return [b.class.replace(/_/g, ' ').replace(/([a-z])([A-Z0-9])/g, '$1 $2').replace(/Mk (\d)/, 'Mk$1'), `${catOf(b.class)[2]}${away}`];
  }
  const fmtDist = u => u >= 1000 * U ? `${(u / 1000 / U).toFixed(1)} km` : `${Math.round(u / U)} m`;

  /* ---------- teleport ---------- */
  // Height for a map spot: just above the nearest thing we know the height of, else a sky drop.
  function heightNear(x, y) {
    let best = null, bd = 15000;
    const all = world ? [...world.nodes, ...world.things, ...world.buildings] : [];
    for (const o of all) { const d = Math.hypot(o.x - x, o.y - y); if (d < bd) { bd = d; best = o; } }
    for (const p of pins) if (p.z != null) { const d = Math.hypot(p.x - x, p.y - y); if (d < bd) { bd = d; best = p; } }
    return best ? best.z + 400 + bd * 0.15 : null;
  }
  async function teleport(x, y, z) {
    try {
      await invoke('map_teleport', { x, y, z: z ?? null });
      toast('Teleporting', z == null ? 'dropping in from the sky' : '');
    } catch (e) { toast('Teleport failed', String(e)); }
  }

  /* ---------- UI ---------- */
  function mount(el, game = '526870') {
    unmount();
    if (game !== GAME) {
      G = GAMES[game]; GAME = G.id = game; WORLD = G.world; U = G.unit;
      world = null; worldAt = 0; worldError = ''; bg = null; bgError = ''; me = null; trail = []; pins = []; view = null; follow = false;
    }
    host = el;
    dpr = window.devicePixelRatio || 1;
    host.classList.add('lm');
    host.innerHTML = `
      <canvas class="lm-canvas"></canvas>
      <div class="lm-card lm-layers${prefs.panel ? '' : ' min'}">
        <button class="lm-head" data-lm="panel"><b>Layers</b><span class="lm-caret">▾</span></button>
        <div class="lm-body" id="lmLegend"></div>
      </div>
      <div class="lm-tools">
        <button class="lm-btn" data-lm="in" title="Zoom in">+</button>
        <button class="lm-btn" data-lm="out" title="Zoom out">−</button>
        <button class="lm-btn" data-lm="fit" title="Whole map">⤢</button>
        <button class="lm-btn wide" data-lm="find" title="Center on you">Find me</button>
        <button class="lm-btn wide${follow ? ' on' : ''}" data-lm="follow" title="Keep you centered (F)">Follow</button>
        <button class="lm-btn wide" data-lm="refresh" title="Read the world again">Refresh</button>
      </div>
      <div class="lm-card lm-pins">
        <div class="lm-head"><b>Pins</b><button class="lm-mini" data-lm="pinhere">+ Pin my spot</button></div>
        <div class="lm-body" id="lmPins"></div>
      </div>
      <div class="lm-status" id="lmStatus"></div>
      <div class="lm-tip" id="lmTip" hidden></div>
      <div class="lm-empty" id="lmEmpty" hidden></div>`;
    cv = host.querySelector('canvas'); ctx = cv.getContext('2d');
    ui = { tip: host.querySelector('#lmTip'), status: host.querySelector('#lmStatus'), legend: host.querySelector('#lmLegend'), pins: host.querySelector('#lmPins'), empty: host.querySelector('#lmEmpty') };
    if (!view) fit();
    ro = new ResizeObserver(() => { dirty = true; }); ro.observe(host);
    bind();
    buildLegend(); renderPins(); status();
    loadPins(); loadMe(); loadBackground();
    if (!world || Date.now() - worldAt > 20000) loadWorld();
    timers.push(setInterval(loadMe, 400), setInterval(loadWorld, 20000), setInterval(status, 1000));
    const id = ++loop;
    requestAnimationFrame(() => frame(id));
  }
  function unmount() {
    loop++;
    timers.forEach(clearInterval); timers = [];
    if (ro) ro.disconnect();
    hideMenu();
    if (host) host.innerHTML = '';
    host = cv = ctx = null;
  }

  function bind() {
    cv.addEventListener('wheel', e => { e.preventDefault(); const r = cv.getBoundingClientRect(); zoomAt(e.clientX - r.left, e.clientY - r.top, Math.exp(-e.deltaY * 0.0015)); }, { passive: false });
    cv.addEventListener('mousedown', e => { if (e.button !== 0) return; hideMenu(); drag = { x: e.clientX, y: e.clientY, cx: view.cx, cy: view.cy, moved: false }; });
    addEventListener('mousemove', onMove);
    addEventListener('mouseup', onUp);
    cv.addEventListener('mouseleave', () => { if (!drag) { hover = null; ui.tip.hidden = true; } });
    cv.addEventListener('dblclick', e => { const r = cv.getBoundingClientRect(); zoomAt(e.clientX - r.left, e.clientY - r.top, 2.2); });
    cv.addEventListener('contextmenu', e => { e.preventDefault(); showMenu(e); });
    host.addEventListener('click', onClick);
    host.addEventListener('keydown', e => e.key === 'Escape' && hideMenu());
  }
  function onMove(e) {
    if (!cv) return;
    const r = cv.getBoundingClientRect(), px = e.clientX - r.left, py = e.clientY - r.top;
    if (drag) {
      if (Math.abs(e.clientX - drag.x) + Math.abs(e.clientY - drag.y) > 3) { drag.moved = true; follow = false; host.querySelector('[data-lm="follow"]').classList.remove('on'); }
      view.cx = drag.cx - (e.clientX - drag.x) / view.s; view.cy = drag.cy - (e.clientY - drag.y) / view.s; dirty = true;
      return;
    }
    if (e.target !== cv) return;
    hover = pick(px, py);
    cv.style.cursor = hover ? 'pointer' : 'grab';
    const [t, s] = hover ? describe(hover) : [null, null];
    ui.tip.hidden = !hover;
    if (hover) { ui.tip.innerHTML = `<b>${esc(t)}</b><span>${esc(s)}</span>`; ui.tip.style.left = px + 16 + 'px'; ui.tip.style.top = py + 12 + 'px'; }
    ui.status.dataset.cursor = `${Math.round(meters(gx(px)))}, ${Math.round(meters(gy(py)))} m`;
    status();
  }
  function onUp(e) {
    if (!drag || !cv) return;
    const wasDrag = drag.moved; drag = null;
    if (wasDrag || e.target !== cv) return;
    // A plain click on something: open its actions.
    const r = cv.getBoundingClientRect(), it = pick(e.clientX - r.left, e.clientY - r.top);
    if (it) showMenu(e, it);
  }
  function onClick(e) {
    const b = e.target.closest('[data-lm]');
    if (!b) return;
    const a = b.dataset.lm, c = cv;
    if (a === 'in') zoomAt(c.clientWidth / 2, c.clientHeight / 2, 1.6);
    if (a === 'out') zoomAt(c.clientWidth / 2, c.clientHeight / 2, 1 / 1.6);
    if (a === 'fit') fit();
    if (a === 'find') me ? flyTo(me[0], me[1], Math.max(view.s, 1 / U)) : toast('Not connected', `start ${G.name} first`);
    if (a === 'follow') { follow = !follow; b.classList.toggle('on', follow); if (follow && me) flyTo(me[0], me[1], Math.max(view.s, 1 / U)); }
    if (a === 'refresh') { loadWorld(); toast('Map', 'reading the world again'); }
    if (a === 'panel') { prefs.panel = !prefs.panel; savePrefs(); host.querySelector('.lm-layers').classList.toggle('min', !prefs.panel); }
    if (a === 'pinhere') me ? addPin(me[0], me[1], me[2] + 0.5 * U) : toast('Not connected', `start ${G.name} first`);
    if (a === 'res') { prefs.res[b.dataset.k] = !resOn(b.dataset.k); savePrefs(); buildLegend(); dirty = true; }
    if (a === 'resall') { const on = b.dataset.k === 'all'; Object.keys(RES).forEach(k => prefs.res[k] = on); savePrefs(); buildLegend(); dirty = true; }
    if (a === 'nearest') nearest(b.dataset.k);
    if (a === 'purity') { prefs.purity[+b.dataset.k] = !prefs.purity[+b.dataset.k]; savePrefs(); buildLegend(); dirty = true; }
    if (a === 'thing') { prefs.things[b.dataset.k] = !prefs.things[b.dataset.k]; savePrefs(); buildLegend(); dirty = true; }
    if (a === 'opened') { prefs.hideOpened = !prefs.hideOpened; savePrefs(); buildLegend(); dirty = true; }
    if (a === 'line') { prefs.lines[b.dataset.k] = !prefs.lines[b.dataset.k]; savePrefs(); buildLegend(); dirty = true; }
    if (a === 'buildings') { prefs.buildings = !prefs.buildings; savePrefs(); buildLegend(); dirty = true; }
    if (a === 'style' && prefs.mapStyle !== b.dataset.k) { prefs.mapStyle = b.dataset.k; savePrefs(); bg = null; bgError = ''; buildLegend(); loadBackground(); }
    if (a === 'terrain') { prefs.terrain = !prefs.terrain; savePrefs(); buildLegend(); dirty = true; }
    if (a === 'trail') { prefs.trail = !prefs.trail; savePrefs(); buildLegend(); dirty = true; }
    if (a === 'cleartrail') { trail = me ? [[me[0], me[1]]] : []; dirty = true; }
    if (a === 'pinfly') { const p = pins.find(q => q.id == b.dataset.k); if (p) flyTo(p.x, p.y, Math.max(view.s, 1 / U)); }
    if (a === 'pintp') { const p = pins.find(q => q.id == b.dataset.k); if (p) teleport(p.x, p.y, p.z ?? heightNear(p.x, p.y)); }
    if (a === 'pindel') { pins = pins.filter(q => q.id != b.dataset.k); savePins(); }
  }
  function nearest(res) {
    if (!world) return;
    const from = me || [view.cx, view.cy];
    const list = world.nodes.filter(n => n.res === res && prefs.purity[n.purity]);
    if (!list.length) return toast((RES[res] || [res])[0], 'none with the selected purities');
    const n = list.reduce((a, b) => Math.hypot(a.x - from[0], a.y - from[1]) < Math.hypot(b.x - from[0], b.y - from[1]) ? a : b);
    prefs.res[res] = true; savePrefs(); buildLegend();
    flyTo(n.x, n.y, Math.max(view.s, 1.2 / U));
  }

  function chip(on, a, k, label, col, extra = '') {
    return `<button class="lm-chip${on ? ' on' : ''}" data-lm="${a}" data-k="${k}"${extra}>${col ? `<i style="background:${col}"></i>` : ''}${label}</button>`;
  }
  function buildLegend() {
    if (!ui.legend) return;
    const count = {}, things = {}, lines = {};
    if (world) {
      for (const n of world.nodes) count[n.res] = (count[n.res] || 0) + 1;
      for (const t of world.things) if (!(t.done && prefs.hideOpened)) { const g = THINGS[t.kind]?.[2]; if (g) things[g] = (things[g] || 0) + 1; }
      for (const l of world.lines) lines[l.kind] = (lines[l.kind] || 0) + 1;
    }
    const resKeys = Object.keys(RES).filter(k => !world || count[k]);
    ui.legend.innerHTML = (G.scan ? `
      <div class="lm-sec"><span>Resources</span><button class="lm-link" data-lm="resall" data-k="all">All</button><button class="lm-link" data-lm="resall" data-k="none">None</button></div>
      <div class="lm-chips">${resKeys.map(k => chip(resOn(k), 'res', k, `${RES[k][0]}${count[k] ? ` <em>${count[k]}</em>` : ''}`, RES[k][1], ' title="Click to show or hide. Right-click: fly to the nearest one."')).join('')}</div>
      <div class="lm-chips">${PURITY.map((p, i) => chip(prefs.purity[i], 'purity', i, p)).join('')}</div>
      <div class="lm-sec"><span>Collectibles</span></div>
      <div class="lm-chips">
        ${chip(prefs.things.slugs, 'thing', 'slugs', `Power slugs${things.slugs ? ` <em>${things.slugs}</em>` : ''}`, '#5bc8ff')}
        ${chip(prefs.things.somersloops, 'thing', 'somersloops', `Somersloops${things.somersloops ? ` <em>${things.somersloops}</em>` : ''}`, '#ff5a5a')}
        ${chip(prefs.things.spheres, 'thing', 'spheres', `Mercer spheres${things.spheres ? ` <em>${things.spheres}</em>` : ''}`, '#7affd4')}
        ${chip(prefs.things.pods, 'thing', 'pods', `Crash sites${things.pods ? ` <em>${things.pods}</em>` : ''}`, '#ffffff')}
        ${chip(prefs.hideOpened, 'opened', '', 'Hide opened')}
      </div>
      <div class="lm-sec"><span>Your factory</span></div>
      <div class="lm-chips">
        ${chip(prefs.buildings, 'buildings', '', `Buildings${world?.buildings.length ? ` <em>${world.buildings.length}</em>` : ''}`, '#f3b73f')}
        ${Object.entries(LINES).map(([k, [l, c]]) => chip(prefs.lines[k], 'line', k, `${l}${lines[k] ? ` <em>${lines[k]}</em>` : ''}`, c)).join('')}
      </div>` : '') + `
      <div class="lm-sec"><span>Map</span><button class="lm-link" data-lm="cleartrail">Clear trail</button></div>
      ${G.styles ? `<div class="lm-chips">${G.styles.map(([k, l]) => chip(prefs.mapStyle === k, 'style', k, l)).join('')}</div>` : ''}
      <div class="lm-chips">${chip(prefs.terrain, 'terrain', '', 'Terrain')}${chip(prefs.trail, 'trail', '', 'Your trail')}</div>
      ${bgError ? `<p class="lm-note">Terrain unavailable: ${esc(bgError)}</p>` : ''}`;
    ui.legend.querySelectorAll('[data-lm="res"]').forEach(b => b.addEventListener('contextmenu', e => { e.preventDefault(); nearest(b.dataset.k); }));
  }
  function renderPins() {
    if (!ui.pins) return;
    ui.pins.innerHTML = pins.length ? pins.map(p => `
      <div class="lm-pin"><button class="lm-pinname" data-lm="pinfly" data-k="${p.id}" title="Show on the map"><i style="background:${p.color}"></i>${esc(p.name)}</button>
        ${G.teleport ? `<button class="lm-mini" data-lm="pintp" data-k="${p.id}" title="Teleport">Go</button>` : ''}<button class="lm-mini" data-lm="pindel" data-k="${p.id}" title="Delete">✕</button></div>`).join('')
      : '<p class="lm-note">Right-click the map to drop a pin, or pin where you stand.</p>';
  }
  function status() {
    if (!ui.status) return;
    const age = worldAt ? Math.round((Date.now() - worldAt) / 1000) : null;
    const where = me ? `You: ${Math.round(meters(me[0]))}, ${Math.round(meters(me[1]))} · ${me[2] < 0 ? `${Math.round(meters(-me[2]))} m deep` : `${Math.round(meters(me[2]))} m up`}` : `Not connected · start ${G.name}`;
    ui.status.innerHTML = `<span class="${me ? 'on' : ''}">${where}</span>${ui.status.dataset.cursor ? `<span>Cursor ${ui.status.dataset.cursor}</span>` : ''}${age != null ? `<span>World read ${age}s ago</span>` : ''}`;
    const showEmpty = G.scan ? !world : !bg && !!bgError;
    ui.empty.hidden = !showEmpty;
    if (showEmpty && !G.scan) ui.empty.innerHTML = `<b>Couldn’t build the map</b><span>${esc(bgError)}</span>`;
    else if (showEmpty) ui.empty.innerHTML = `<b>${worldError.includes('Start') || !worldError ? `Start ${G.name} to see your world` : 'Couldn’t read the world'}</b><span>${worldError && !worldError.includes('Start') ? esc(worldError) : 'Resources, collectibles and your factory appear once a save is loaded.'}</span>`;
  }

  const PIN_COLORS = ['#3ddc84', '#4da3ff', '#ffb02e', '#ff5c7a', '#b98cff', '#e8e8e8'];
  function addPin(x, y, z, name) {
    const n = prompt('Pin name', name || `Pin ${pins.length + 1}`);
    if (n === null) return;
    pins.push({ id: Date.now(), name: n.trim() || `Pin ${pins.length + 1}`, color: PIN_COLORS[pins.length % PIN_COLORS.length], x, y, z: z ?? null });
    savePins();
  }

  function showMenu(e, it) {
    hideMenu();
    const r = cv.getBoundingClientRect(), px = e.clientX - r.left, py = e.clientY - r.top;
    it = it || pick(px, py);
    const x = it ? it.x : gx(px), y = it ? it.y : gy(py);
    const z = it ? (it.type === 'pin' ? it.p.z ?? heightNear(x, y) : it.z + 400) : heightNear(x, y);
    const title = it ? describe(it) : ['Map spot', `${Math.round(meters(x))}, ${Math.round(meters(y))} m${me ? ` · ${fmtDist(Math.hypot(x - me[0], y - me[1]))} away` : ''}`];
    menu = document.createElement('div');
    menu.className = 'lm-menu';
    menu.innerHTML = `<div class="lm-mt"><b>${esc(title[0])}</b><span>${esc(title[1])}</span></div>
      ${G.teleport ? `<button data-m="tp">Teleport here${z == null ? ' <em>sky drop</em>' : ''}</button>` : ''}
      ${it?.type === 'pin' ? '<button data-m="ren">Rename pin</button><button data-m="del">Delete pin</button>' : '<button data-m="pin">Drop a pin here</button>'}
      <button data-m="center">Center here</button>`;
    menu.style.left = Math.min(px, cv.clientWidth - 220) + 'px';
    menu.style.top = Math.min(py, cv.clientHeight - 200) + 'px';
    menu.onclick = ev => {
      const m = ev.target.closest('[data-m]')?.dataset.m;
      if (m === 'tp') teleport(x, y, z);
      if (m === 'pin') addPin(x, y, it ? it.z + 300 : null, it ? describe(it)[0] : undefined);
      if (m === 'center') flyTo(x, y);
      if (m === 'ren') { const n = prompt('Pin name', it.p.name); if (n) { it.p.name = n.trim() || it.p.name; savePins(); } }
      if (m === 'del') { pins = pins.filter(q => q !== it.p); savePins(); }
      hideMenu();
    };
    host.appendChild(menu);
    ui.tip.hidden = true;
  }
  function hideMenu() { if (menu) { menu.remove(); menu = null; } }

  addEventListener('keydown', e => {
    if (!host || e.target.closest('input, textarea')) return;
    if (e.key === 'Escape') hideMenu();
    if (e.key.toLowerCase() === 'f' && !e.ctrlKey) host.querySelector('[data-lm="follow"]')?.click();
  });

  return { mount, unmount };
})();
