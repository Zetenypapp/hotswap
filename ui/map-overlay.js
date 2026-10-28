// HotSwap layer on top of the Satisfactory Calculator interactive map (runs inside the map window).
// Adds a live "you are here" marker, right-click teleport, and saved pins. The site's own map
// (zoom, layers, resources) is untouched; we only draw on its Leaflet map.
(() => {
  if (window.top !== window || !location.hostname.endsWith('satisfactory-calculator.com')) return;
  const invoke = (cmd, args) => window.__TAURI_INTERNALS__.invoke(cmd, args);

  let map, toLatLng, toGame, me, follow = false, pins = [], pinLayer, menu;

  // Wait for the site to build its map, then hook in.
  const wait = setInterval(() => {
    const m = window.SCIM && window.SCIM.map;
    if (!m || !m.leafletMap || typeof m.unproject !== 'function' || !window.L) return;
    clearInterval(wait);
    start(m);
  }, 500);

  function start(scim) {
    map = scim.leafletMap;
    toLatLng = (x, y) => scim.unproject([x, y]);
    // The site converts game -> map only, so fit the inverse from three points (it's affine).
    const s = 100000, o = toLatLng(0, 0), a = toLatLng(s, 0), b = toLatLng(0, s);
    const m = [[(a.lng - o.lng) / s, (b.lng - o.lng) / s], [(a.lat - o.lat) / s, (b.lat - o.lat) / s]];
    const det = m[0][0] * m[1][1] - m[0][1] * m[1][0];
    toGame = ll => {
      const dx = ll.lng - o.lng, dy = ll.lat - o.lat;
      return { x: (m[1][1] * dx - m[0][1] * dy) / det, y: (m[0][0] * dy - m[1][0] * dx) / det };
    };

    css();
    pinLayer = L.layerGroup().addTo(map);
    panel();
    map.on('contextmenu', e => { L.DomEvent.preventDefault(e.originalEvent); showMenu(e); });
    map.on('click movestart', hideMenu);
    loadPins();
    tick();
    setInterval(tick, 500);
  }

  // ---- live player marker ----
  async function tick() {
    let p = null;
    try { p = await invoke('map_player'); } catch {}
    const status = document.getElementById('hs-status');
    if (!p) {
      if (me) { me.remove(); me = null; }
      status.textContent = 'Not connected · start Satisfactory with HotSwap open';
      status.className = 'hs-off';
      return;
    }
    const [x, y, z, yaw] = p, ll = toLatLng(x, y);
    status.textContent = `You · ${Math.round(x / 100)}, ${Math.round(y / 100)}, ${Math.round(z / 100)} m`;
    status.className = 'hs-on';
    if (!me) {
      me = L.marker(ll, { icon: L.divIcon({ className: '', html: '<div class="hs-me"><i></i></div>', iconSize: [28, 28], iconAnchor: [14, 14] }), zIndexOffset: 10000, interactive: false }).addTo(map);
    } else me.setLatLng(ll);
    const el = me.getElement() && me.getElement().querySelector('.hs-me');
    if (el) el.style.transform = `rotate(${yaw + 90}deg)`; // game yaw 0 = +X (east on this map)
    if (follow) map.panTo(ll, { animate: true });
  }

  // ---- teleport ----
  async function teleport(x, y, z) {
    try {
      await invoke('map_teleport', { x, y, z: z ?? null });
      toast(z == null ? 'Teleporting · dropping in from the sky (no fall damage for 30 s)' : 'Teleporting');
    } catch (e) { toast(String(e)); }
  }

  // ---- right-click menu ----
  function showMenu(e) {
    hideMenu();
    const g = toGame(e.latlng);
    menu = document.createElement('div');
    menu.className = 'hs-menu';
    menu.style.left = e.originalEvent.clientX + 'px';
    menu.style.top = e.originalEvent.clientY + 'px';
    menu.innerHTML = `<button data-a="tp">Teleport here</button><button data-a="pin">Drop a pin here</button><small>${Math.round(g.x / 100)}, ${Math.round(g.y / 100)}</small>`;
    menu.onclick = ev => {
      const a = ev.target.dataset.a;
      if (a === 'tp') teleport(g.x, g.y, null);
      if (a === 'pin') addPin({ x: g.x, y: g.y, z: null });
      hideMenu();
    };
    document.body.appendChild(menu);
  }
  function hideMenu() { if (menu) { menu.remove(); menu = null; } }
  document.addEventListener('keydown', e => e.key === 'Escape' && hideMenu());

  // ---- pins ----
  const COLORS = ['#3ddc84', '#4da3ff', '#ffb02e', '#ff5c7a', '#b98cff', '#e8e8e8'];
  async function loadPins() {
    try { pins = JSON.parse((await invoke('map_pins_load')) || '{}')['526870'] || []; } catch { pins = []; }
    drawPins();
  }
  async function savePins() {
    let all = {};
    try { all = JSON.parse((await invoke('map_pins_load')) || '{}'); } catch {}
    all['526870'] = pins;
    try { await invoke('map_pins_save', { data: JSON.stringify(all) }); } catch (e) { toast('Could not save pins: ' + e); }
  }
  function addPin(at) {
    const name = prompt('Pin name', `Pin ${pins.length + 1}`);
    if (name === null) return;
    pins.push({ id: Date.now(), name: name.trim() || `Pin ${pins.length + 1}`, color: COLORS[pins.length % COLORS.length], ...at });
    savePins(); drawPins();
  }
  function drawPins() {
    pinLayer.clearLayers();
    for (const p of pins) {
      const icon = L.divIcon({ className: '', html: `<div class="hs-pin" style="--c:${p.color}"></div>`, iconSize: [22, 30], iconAnchor: [11, 29] });
      const mk = L.marker(toLatLng(p.x, p.y), { icon, title: p.name, zIndexOffset: 9000 }).addTo(pinLayer);
      mk.bindPopup(() => {
        const d = document.createElement('div');
        d.className = 'hs-pop';
        d.innerHTML = `<b></b><div><button data-a="tp">Teleport</button><button data-a="ren">Rename</button><button data-a="del">Delete</button></div>`;
        d.querySelector('b').textContent = p.name;
        d.onclick = ev => pinAction(p, ev.target.dataset.a);
        return d;
      });
    }
    const list = document.getElementById('hs-pins');
    list.innerHTML = pins.length ? '' : '<p class="hs-empty">Right-click the map to drop a pin, or pin where you stand.</p>';
    for (const p of pins) {
      const row = document.createElement('div');
      row.className = 'hs-row';
      row.innerHTML = `<span class="hs-dot" style="background:${p.color}"></span><span class="hs-name"></span><button data-a="tp" title="Teleport">⤴</button><button data-a="del" title="Delete">✕</button>`;
      row.querySelector('.hs-name').textContent = p.name + (p.z != null ? '' : ' ·');
      row.querySelector('.hs-name').title = p.z != null ? 'Exact spot' : 'Map spot (drops in from the sky)';
      row.onclick = ev => {
        const a = ev.target.dataset.a;
        if (a) pinAction(p, a); else map.flyTo(toLatLng(p.x, p.y), Math.max(map.getZoom(), map.getMaxZoom() - 3));
      };
      list.appendChild(row);
    }
  }
  function pinAction(p, a) {
    if (a === 'tp') teleport(p.x, p.y, p.z);
    if (a === 'ren') { const n = prompt('Pin name', p.name); if (n) { p.name = n.trim() || p.name; savePins(); drawPins(); } }
    if (a === 'del') { pins = pins.filter(q => q !== p); savePins(); drawPins(); }
    map.closePopup();
  }

  // ---- panel ----
  function panel() {
    const d = document.createElement('div');
    d.id = 'hs-panel';
    d.innerHTML = `
      <header><span class="hs-logo">⇄</span><b>HotSwap</b><button id="hs-min" title="Collapse">–</button></header>
      <div id="hs-body">
        <div id="hs-status" class="hs-off">Connecting…</div>
        <div class="hs-btns">
          <button id="hs-find">Find me</button>
          <button id="hs-follow">Follow</button>
          <button id="hs-here">Pin my spot</button>
        </div>
        <div class="hs-title">Pins</div>
        <div id="hs-pins"></div>
        <p class="hs-tip">Right-click anywhere: teleport or pin</p>
      </div>`;
    document.body.appendChild(d);
    const pos = async () => { try { return await invoke('map_player'); } catch { return null; } };
    d.querySelector('#hs-min').onclick = () => d.classList.toggle('min');
    d.querySelector('#hs-find').onclick = async () => {
      const p = await pos();
      p ? map.flyTo(toLatLng(p[0], p[1]), Math.max(map.getZoom(), map.getMaxZoom() - 3)) : toast('Not connected to the game');
    };
    d.querySelector('#hs-follow').onclick = e => { follow = !follow; e.target.classList.toggle('on', follow); };
    d.querySelector('#hs-here').onclick = async () => {
      const p = await pos();
      p ? addPin({ x: p[0], y: p[1], z: p[2] + 50 }) : toast('Not connected to the game');
    };
    L.DomEvent.disableClickPropagation(d);
    L.DomEvent.disableScrollPropagation(d);
  }

  function toast(msg) {
    const t = document.createElement('div');
    t.className = 'hs-toast';
    t.textContent = msg;
    document.body.appendChild(t);
    setTimeout(() => t.remove(), 3500);
  }

  function css() {
    const s = document.createElement('style');
    s.textContent = `
      #hs-panel { position: fixed; left: 16px; bottom: 16px; z-index: 100000; width: 250px; font: 13px/1.4 system-ui, sans-serif; color: #e9ecef;
        background: rgba(22,24,28,.88); backdrop-filter: blur(14px); border: 1px solid rgba(255,255,255,.08); border-radius: 18px; box-shadow: 0 10px 30px rgba(0,0,0,.45); overflow: hidden; }
      #hs-panel header { display: flex; align-items: center; gap: 8px; padding: 10px 12px; }
      #hs-panel header b { flex: 1; }
      .hs-logo { display: grid; place-items: center; width: 24px; height: 24px; border-radius: 8px; background: #3ddc84; color: #10301e; font-weight: 800; }
      #hs-panel button { font: inherit; color: inherit; background: rgba(255,255,255,.08); border: 0; border-radius: 10px; padding: 6px 9px; cursor: pointer; }
      #hs-panel button:hover { background: rgba(255,255,255,.16); }
      #hs-panel button.on { background: #3ddc84; color: #10301e; }
      #hs-panel.min #hs-body { display: none; }
      #hs-body { padding: 0 12px 12px; }
      #hs-status { font-size: 12px; padding: 7px 10px; border-radius: 10px; margin-bottom: 8px; }
      #hs-status.hs-on { background: rgba(61,220,132,.14); color: #7ff0b0; }
      #hs-status.hs-off { background: rgba(255,255,255,.06); color: #9aa1a9; }
      .hs-btns { display: flex; gap: 6px; flex-wrap: wrap; }
      .hs-title { margin: 12px 0 4px; font-size: 11px; text-transform: uppercase; letter-spacing: .08em; color: #8b9198; }
      #hs-pins { max-height: 220px; overflow: auto; }
      .hs-row { display: flex; align-items: center; gap: 8px; padding: 5px 6px; border-radius: 10px; cursor: pointer; }
      .hs-row:hover { background: rgba(255,255,255,.06); }
      .hs-row button { padding: 2px 7px !important; }
      .hs-dot { width: 10px; height: 10px; border-radius: 50%; flex: none; }
      .hs-name { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
      .hs-empty, .hs-tip { margin: 6px 0 0; font-size: 12px; color: #8b9198; }
      .hs-me { width: 28px; height: 28px; border-radius: 50%; background: rgba(61,220,132,.25); border: 2px solid #3ddc84; box-shadow: 0 0 12px #3ddc84; position: relative; }
      .hs-me i { position: absolute; left: 50%; top: -9px; margin-left: -6px; border: 6px solid transparent; border-bottom: 9px solid #3ddc84; border-top: 0; }
      .hs-pin { width: 22px; height: 22px; border-radius: 50% 50% 50% 0; transform: rotate(-45deg); background: var(--c); border: 2px solid #111; box-shadow: 0 2px 6px rgba(0,0,0,.5); }
      .hs-menu { position: fixed; z-index: 100001; display: flex; flex-direction: column; min-width: 170px; padding: 6px; font: 13px system-ui, sans-serif;
        background: rgba(22,24,28,.95); border: 1px solid rgba(255,255,255,.1); border-radius: 14px; box-shadow: 0 10px 30px rgba(0,0,0,.5); }
      .hs-menu button { text-align: left; color: #e9ecef; background: none; border: 0; border-radius: 9px; padding: 8px 10px; cursor: pointer; font: inherit; }
      .hs-menu button:hover { background: rgba(61,220,132,.18); }
      .hs-menu small { color: #8b9198; padding: 4px 10px 2px; }
      .hs-pop div { display: flex; gap: 6px; margin-top: 6px; }
      .hs-pop button { border: 0; border-radius: 8px; padding: 4px 8px; cursor: pointer; background: #e9ecef; }
      .hs-toast { position: fixed; left: 50%; bottom: 24px; transform: translateX(-50%); z-index: 100002; padding: 10px 16px; border-radius: 12px;
        background: rgba(22,24,28,.95); color: #e9ecef; font: 13px system-ui, sans-serif; box-shadow: 0 10px 30px rgba(0,0,0,.5); }`;
    document.head.appendChild(s);
  }
})();
