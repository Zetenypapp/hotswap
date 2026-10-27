/* Mod loader page: browse ficsit.app, install mods with their dependencies, and manage installed
   ones. Satisfactory only for now. The backend refuses changes while the game is running. */
const Mods = (() => {
  // Games with a mod loader page. `source` is where their mods come from; others show as coming soon.
  const GAMES = [
    { id: '526870', source: 'ficsit.app', url: 'https://ficsit.app' },
    { id: '1962700', source: 'Nexus Mods', url: 'https://www.nexusmods.com/subnautica2', nexus: 'subnautica2' },
  ];
  let GAME = null; // the game whose mod page is open, or null for the game grid
  // Installed mods are only read for Satisfactory, the one game with a mod source so far.
  const gid = () => GAME || GAMES[0].id;
  const S = { tab: 'installed', query: '', sort: 'popular', results: [], count: 0, loading: false, error: '',
    installed: [], build: null, updates: {}, checking: false, busy: {}, loaded: false };
  let searchTimer;
  // Nexus Mods games (mods are files: UE4SS, Lua and pak mods).
  const N = { mods: [], downloads: [], results: [], count: 0, loading: false, error: '', builtIn: false, busy: '', clicked: null };
  const isNexus = () => !!GAMES.find(x => x.id === GAME)?.nexus;

  const running = () => RUNNING === gid();
  const fmtN = n => n >= 1e6 ? (n / 1e6).toFixed(1) + 'M' : n >= 1e3 ? Math.round(n / 1e3) + 'k' : String(n);
  const mb = b => b >= 1 << 20 ? `${(b / (1 << 20)).toFixed(0)} MB` : `${Math.max(1, Math.round(b / 1024))} KB`;

  async function refreshInstalled() {
    try {
      const r = await invoke('mods_state', { game: gid() });
      S.installed = r.installed; S.build = r.build; S.error = '';
    } catch (e) { S.error = String(e); }
    S.loaded = true;
    if (!GAME && state.view === 'mods') { const b = $('modsBody'); if (b.dataset.view === 'grid') grid(b); }
    list();
  }
  async function search(more) {
    S.loading = true; list();
    try {
      const r = await invoke('mods_search', { query: S.query, sort: S.sort, offset: more ? S.results.length : 0 });
      S.results = more ? [...S.results, ...r.mods] : r.mods; S.count = r.count; S.error = '';
    } catch (e) { S.error = String(e); }
    S.loading = false; list();
  }
  async function checkUpdates() {
    S.checking = true; list();
    try { S.updates = await invoke('mods_updates', { game: gid() }); toast('Mods', Object.keys(S.updates).length ? `${Object.keys(S.updates).length} updates` : 'all up to date'); }
    catch (e) { toast("Couldn't check for mod updates", String(e)); }
    S.checking = false; list();
  }
  async function install(ref, name) {
    if (running()) return toast('Close Satisfactory first', 'then change mods');
    S.busy[ref] = { stage: 'resolving' }; list();
    try {
      const done = await invoke('mods_install', { game: gid(), reference: ref });
      delete S.updates[ref];
      toast(name || ref, done.length > 1 ? `installed with ${done.length - 1} ${done.length === 2 ? 'dependency' : 'dependencies'}` : 'installed');
    } catch (e) { toast(`Couldn't install ${name || ref}`, String(e)); }
    delete S.busy[ref];
    refreshInstalled();
  }
  async function act(cmd, args, msg) {
    if (running()) return toast('Close Satisfactory first', 'then change mods');
    try { await invoke(cmd, { game: gid(), ...args }); if (msg) toast(...msg); } catch (e) { toast("Couldn't change the mod", String(e)); }
    refreshInstalled();
  }

  function onProgress(p) {
    // Progress comes per file (a dependency can be downloading); show it on the mod that was clicked.
    const key = S.busy[p.reference] ? p.reference : Object.keys(S.busy)[0];
    if (!key) return;
    S.busy[key] = p;
    const el = document.querySelector(`[data-modprog="${CSS.escape(key)}"]`);
    if (el) el.textContent = progressText(p); else list();
  }
  function progressText(p) {
    if (!p || p.stage === 'resolving') return 'Checking…';
    if (p.stage === 'downloading') return `${p.reference} ${p.total ? Math.round(p.done / p.total * 100) + '%' : mb(p.done)}`;
    if (p.stage === 'installing') return `Installing ${p.reference}…`;
    return 'Working…';
  }

  /* ---------- page ---------- */
  const gameOf = id => LIB.find(g => g.id === id || String(g.appId) === id) || buildGame({ appId: +id, name: DEMO_LIBRARY.find(d => String(d.appId) === id)?.name || 'Game' });

  // The grid of games, like the Library.
  function grid(body) {
    const list = GAMES.filter(x => x.source || LIB.some(g => g.id === x.id));
    body.innerHTML = `
      <div class="lib-head"><h1>Mods</h1><span class="meta">Install and manage mods for each of your games.</span></div>
      <div class="lib-grid">${list.map(x => {
        const g = gameOf(x.id);
        const n = x.id === '526870' && S.loaded ? S.installed.length : 0;
        return `<button class="lib-item${x.source ? '' : ' soon-item'}" data-modgame="${x.id}"${x.source ? '' : ' disabled'}><span class="art">${capsule(g)}${n ? `<span class="modded">${n} installed</span>` : ''}</span>
          <span class="nm">${esc(g.name)}</span><span class="sub">${x.source ? `Mods from ${esc(x.source)}` : 'Coming soon'}</span></button>`;
      }).join('')}</div>`;
    if (!S.loaded) refreshInstalled();
  }

  function render() {
    setAmbient(null);
    const body = $('modsBody');
    if (!invoke) { body.innerHTML = '<div class="empty-page"><h1>Mods</h1><p>The mod loader works in the desktop app.</p></div>'; return; }
    if (!GAME) { if (body.dataset.view !== 'grid') { body.dataset.view = 'grid'; grid(body); } return; }
    const info = GAMES.find(x => x.id === GAME);
    if (body.dataset.view !== GAME + S.tab) {
      body.dataset.view = GAME + S.tab;
      body.innerHTML = `
        <div class="lib-head"><button class="chip" data-modgame="">‹ All games</button><h1>${esc(gameOf(GAME).name)}</h1><span class="meta">${S.build && !isNexus() ? `Game build ${S.build} · ` : ''}mods from <a href="${info.url}" ${ext}>${esc(info.source)}</a></span>
          <div class="chips"><button class="chip${S.tab === 'installed' ? ' on' : ''}" data-modtab="installed">Installed</button><button class="chip${S.tab === 'browse' ? ' on' : ''}" data-modtab="browse">Browse</button></div></div>
        <div id="modsWarn"></div>
        ${S.tab === 'browse' ? `<div class="mods-bar"><label class="search mods-search"><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><circle cx="11" cy="11" r="7"/><path d="M20 20l-4-4"/></svg>
            <input id="modSearch" type="search" placeholder="Search mods" autocomplete="off" spellcheck="false" value="${esc(S.query)}"></label>
            <div class="chips">${[['popular', 'Popular'], ['updated', 'Updated'], ['new', 'New']].map(([k, l]) => `<button class="chip${S.sort === k ? ' on' : ''}" data-modsort="${k}">${l}</button>`).join('')}</div></div>` : ''}
        <div id="modsList"></div>`;
      const input = $('modSearch');
      if (input) input.addEventListener('input', () => { S.query = input.value; clearTimeout(searchTimer); searchTimer = setTimeout(() => isNexus() ? nSearch(false) : search(false), 350); });
      if (isNexus()) { nRefresh(); if (S.tab === 'browse' && !N.results.length) nSearch(false); }
      else {
        if (!S.loaded) refreshInstalled();
        if (S.tab === 'browse' && !S.results.length && !S.loading) search(false);
      }
    }
    list();
  }

  function list() {
    const el = $('modsList');
    if (!el || state.view !== 'mods' || !GAME) return;
    if (isNexus()) return nexusList(el);
    $('modsWarn').innerHTML = running() ? '<div class="mods-warn">Satisfactory is running. Close it to install, update or remove mods.</div>' : '';
    if (S.error && S.tab === 'installed') { el.innerHTML = `<p class="empty">${esc(S.error)}</p>`; return; }
    S.tab === 'installed' ? installedList(el) : browseList(el);
  }

  function installedList(el) {
    const ups = Object.keys(S.updates).length;
    el.innerHTML = `
      <div class="mods-tools"><span class="meta">${S.installed.length} installed${ups ? ` · ${ups} update${ups > 1 ? 's' : ''}` : ''}</span>
        ${ups > 1 ? '<button class="btn" data-modupall>Update all</button>' : ''}
        <button class="btn" data-modcheck${S.checking ? ' disabled' : ''}>${S.checking ? 'Checking…' : 'Check for updates'}</button></div>
      ${S.installed.length ? `<div class="mods-rows">${S.installed.map(m => {
        const busy = S.busy[m.reference], up = S.updates[m.reference];
        return `<div class="mod-row${m.enabled ? '' : ' off'}">
          <span class="mod-ico">${esc((m.name || m.reference)[0])}</span>
          <div class="mod-txt"><b>${esc(m.name)}</b><span>${esc(m.reference)} · ${esc(m.version || '?')}${m.reference === 'SML' ? ' · the mod loader itself, needed by other mods' : ''}</span></div>
          ${busy ? `<span class="mod-prog" data-modprog="${esc(m.reference)}">${esc(progressText(busy))}</span>`
            : up ? `<button class="btn accent" data-modinstall="${esc(m.reference)}" data-name="${esc(m.name)}">Update to ${esc(up)}</button>` : ''}
          <button class="switch" role="switch" aria-checked="${m.enabled}" data-modtoggle="${esc(m.reference)}" title="${m.enabled ? 'Turn off (keeps it installed)' : 'Turn on'}"></button>
          <button class="btn outline" data-modremove="${esc(m.reference)}" data-name="${esc(m.name)}" title="Uninstall">Remove</button>
        </div>`;
      }).join('')}</div>` : `<div class="empty-page small"><h1>No mods yet</h1><p>Find mods in Browse. Hotswap installs the Satisfactory Mod Loader and any other mods they need automatically.</p><button class="btn accent" data-modtab="browse">Browse mods</button></div>`}
      <p class="meta mods-foot">Mods live in the game's FactoryGame\\Mods folder, the same place Satisfactory Mod Manager uses. Turned-off mods move to a folder next to it. Mods can break saves: keep backups.</p>`;
  }

  function compat(m) {
    const st = m.compatibility?.EA?.state;
    if (!st) return '';
    const cls = st === 'Works' ? 'ok' : st === 'Damaged' ? 'warn' : 'bad';
    return `<span class="mod-compat ${cls}">${st === 'Works' ? 'Works' : st === 'Damaged' ? 'Partly works' : 'Broken'}</span>`;
  }
  function browseList(el) {
    const have = new Map(S.installed.map(m => [m.reference, m]));
    if (S.error) { el.innerHTML = `<p class="empty">${esc(S.error)}</p>`; return; }
    el.innerHTML = `<div class="mod-grid">${S.results.map(m => {
      const h = have.get(m.mod_reference), busy = S.busy[m.mod_reference];
      const author = m.authors?.[0]?.user?.username;
      return `<div class="mod-card">
        <div class="mod-top">${m.logo ? `<img src="${esc(m.logo)}" alt="" loading="lazy">` : `<span class="mod-ico big">${esc(m.name[0])}</span>`}
          <div class="mod-txt"><b>${esc(m.name)}</b><span>${author ? `by ${esc(author)} · ` : ''}${fmtN(m.downloads)} downloads</span></div></div>
        <p>${esc(m.short_description || '')}</p>
        <div class="mod-foot">${compat(m)}<span class="meta">${esc(m.latestVersions?.release?.version || '')}</span>
          <a class="btn outline" href="https://ficsit.app/mod/${esc(m.mod_reference)}" ${ext}>Details</a>
          ${busy ? `<span class="mod-prog" data-modprog="${esc(m.mod_reference)}">${esc(progressText(busy))}</span>`
            : h ? `<span class="mod-have">Installed ${esc(h.version)}</span>`
            : `<button class="btn accent" data-modinstall="${esc(m.mod_reference)}" data-name="${esc(m.name)}">Install</button>`}</div>
      </div>`;
    }).join('')}</div>
    ${S.loading ? '<p class="empty">Loading…</p>' : S.results.length < S.count ? '<div class="mods-more"><button class="btn" data-modmore>Load more</button></div>' : S.results.length ? '' : '<p class="empty">No mods found.</p>'}`;
  }

  document.addEventListener('click', e => {
    const t = e.target.closest('[data-modgame],[data-modtab],[data-modsort],[data-modinstall],[data-modtoggle],[data-modremove],[data-modcheck],[data-modmore],[data-modupall]');
    if (!t) return;
    const d = t.dataset;
    if ('modgame' in d) { GAME = d.modgame || null; render(); }
    else if (d.modtab) { S.tab = d.modtab; render(); }
    else if (d.modsort) { S.sort = d.modsort; document.querySelectorAll('[data-modsort]').forEach(b => b.classList.toggle('on', b === t)); isNexus() ? nSearch(false) : search(false); }
    else if (d.modinstall) install(d.modinstall, d.name);
    else if (d.modtoggle) { const on = t.getAttribute('aria-checked') !== 'true'; act('mods_set_enabled', { reference: d.modtoggle, enabled: on }, [d.modtoggle, on ? 'on' : 'off']); }
    else if (d.modremove) { if (confirm(`Uninstall ${d.name}?`)) act('mods_uninstall', { reference: d.modremove }, [d.name, 'removed']); }
    else if ('modcheck' in d) checkUpdates();
    else if ('modmore' in d) isNexus() ? nSearch(true) : search(true);
    else if ('modupall' in d) Object.keys(S.updates).reduce((p, r) => p.then(() => install(r)), Promise.resolve());
  });

  /* ---------- Nexus Mods games ---------- */
  async function nRefresh() {
    try { N.mods = await invoke('fmods_list', { game: GAME }); N.error = ''; } catch (e) { N.error = String(e); }
    try { N.downloads = await invoke('fmods_downloads'); } catch {}
    list();
  }
  async function nSearch(more) {
    N.loading = true; list();
    try {
      const r = await invoke('nexus_search', { game: GAME, query: S.query, sort: S.sort, offset: more ? N.results.length : 0 });
      N.results = more ? [...N.results, ...r.nodes] : r.nodes; N.count = r.totalCount; N.error = '';
    } catch (e) { N.error = String(e); }
    N.loading = false; list();
  }
  async function nInstall(path, name) {
    if (running()) return toast('Close the game first', 'then change mods');
    N.busy = path; list();
    try { const what = await invoke('fmods_install', { game: GAME, path }); toast(what, 'installed'); }
    catch (e) { toast(`Couldn't install ${name}`, String(e)); }
    N.busy = ''; nRefresh();
  }
  async function nAct(cmd, args, msg) {
    if (running()) return toast('Close the game first', 'then change mods');
    try { await invoke(cmd, { game: GAME, ...args }); if (msg) toast(...msg); } catch (e) { toast("Couldn't change the mod", String(e)); }
    nRefresh();
  }

  const KIND = { ue4ss: 'Mod loader', lua: 'UE4SS Lua mod', pak: 'Pak mod', logic: 'Blueprint mod' };
  function downloadsBox() {
    return `<div class="mods-dl"><div class="mods-dl-head"><b>Install a downloaded mod</b><span class="meta">Mod files from your Downloads folder in the last 3 days.</span>
        <button class="btn" data-nrefresh>Refresh</button><button class="btn" data-npick>Choose file…</button></div>
      ${N.downloads.length ? N.downloads.map(f => `<div class="mods-dl-row"><span class="mod-txt"><b>${esc(f.name)}</b><span>${mb(f.size)}</span></span>
        ${N.busy === f.path ? '<span class="mod-prog">Installing…</span>' : `<button class="btn accent" data-ninstall="${esc(f.path)}" data-name="${esc(f.name)}">Install</button>`}</div>`).join('')
        : '<p class="meta">Nothing yet. Download a mod from its Nexus page, then press Refresh.</p>'}</div>`;
  }
  function nexusList(el) {
    $('modsWarn').innerHTML = running() ? '<div class="mods-warn">The game is running. Close it to install, change or remove mods.</div>' : '';
    const hasLoader = N.mods.some(m => m.kind === 'ue4ss');
    const loaderTip = hasLoader ? '' : `<div class="mods-warn">Most mods need <b>UE4SS</b>. <a href="https://www.nexusmods.com/subnautica2/mods/36?tab=files" ${ext}>Download UE4SS</a>, then install it below.</div>`;
    if (S.tab === 'installed') {
      const shown = N.mods.filter(m => N.builtIn || !m.built_in);
      const hidden = N.mods.length - shown.length;
      el.innerHTML = `${loaderTip}${downloadsBox()}
        <div class="mods-tools"><span class="meta">${N.mods.filter(m => !m.built_in).length} installed</span>
          ${hidden || N.builtIn ? `<button class="btn" data-nbuiltin>${N.builtIn ? 'Hide' : 'Show'} UE4SS's own mods</button>` : ''}</div>
        ${N.error ? `<p class="empty">${esc(N.error)}</p>` : shown.length ? `<div class="mods-rows">${shown.map(m => `<div class="mod-row${m.enabled ? '' : ' off'}">
          <span class="mod-ico">${esc(m.name[0])}</span>
          <div class="mod-txt"><b>${esc(m.name)}</b><span>${KIND[m.kind]}${m.built_in ? ' · comes with UE4SS' : ''}</span></div>
          ${m.kind === 'ue4ss' ? '' : `<button class="switch" role="switch" aria-checked="${m.enabled}" data-ntoggle="${esc(m.id)}"></button>`}
          <button class="btn outline" data-nremove="${esc(m.id)}" data-name="${esc(m.name)}">Remove</button></div>`).join('')}</div>`
          : '<p class="empty">No mods installed yet.</p>'}
        <p class="meta mods-foot">Lua mods are switched with UE4SS's enabled.txt; turned-off pak mods move to a folder next to ~mods. Mods can break saves: keep backups.</p>`;
      return;
    }
    el.innerHTML = `${loaderTip}${N.clicked ? downloadsBox() : ''}
      ${N.error ? `<p class="empty">${esc(N.error)}</p>` : ''}
      <div class="mod-grid">${N.results.map(m => `<div class="mod-card">
        <div class="mod-top">${m.thumbnailUrl ? `<img src="${esc(m.thumbnailUrl)}" alt="" loading="lazy">` : `<span class="mod-ico big">${esc(m.name[0])}</span>`}
          <div class="mod-txt"><b>${esc(m.name)}</b><span>by ${esc(m.author)} · ${fmtN(m.downloads)} downloads</span></div></div>
        <p>${esc(m.summary || '')}</p>
        <div class="mod-foot"><span class="meta">${esc(m.version || '')}</span>
          <a class="btn outline" href="https://www.nexusmods.com/subnautica2/mods/${m.modId}" ${ext}>Details</a>
          <a class="btn accent" href="https://www.nexusmods.com/subnautica2/mods/${m.modId}?tab=files" ${ext} data-ndownload="${m.modId}">Download</a></div>
      </div>`).join('')}</div>
      ${N.loading ? '<p class="empty">Loading…</p>' : N.results.length < N.count ? '<div class="mods-more"><button class="btn" data-modmore>Load more</button></div>' : N.results.length ? '' : '<p class="empty">No mods found.</p>'}
      <p class="meta mods-foot">Nexus Mods only lets apps download for premium members, so Download opens the mod's files on Nexus. Once it's downloaded, install it from the box at the top or under Installed.</p>`;
  }

  document.addEventListener('click', e => {
    const t = e.target.closest('[data-ninstall],[data-npick],[data-nrefresh],[data-ntoggle],[data-nremove],[data-nbuiltin],[data-ndownload]');
    if (!t) return;
    const d = t.dataset;
    if (d.ninstall) nInstall(d.ninstall, d.name);
    else if ('npick' in d) invoke('fmods_pick').then(p => p && nInstall(p, p.split(/[\\/]/).pop()));
    else if ('nrefresh' in d) nRefresh();
    else if (d.ntoggle) { const on = t.getAttribute('aria-checked') !== 'true'; nAct('fmods_set_enabled', { id: d.ntoggle, enabled: on }, [d.ntoggle.split(':')[1], on ? 'on' : 'off']); }
    else if (d.nremove) { if (confirm(`Remove ${d.name}?`)) nAct('fmods_remove', { id: d.nremove }, [d.name, 'removed']); }
    else if ('nbuiltin' in d) { N.builtIn = !N.builtIn; list(); }
    // The link itself opens the browser; keep the downloads box in view and look for the file shortly.
    else if (d.ndownload) { N.clicked = d.ndownload; setTimeout(nRefresh, 15000); setTimeout(() => list(), 50); }
  });

  return { render, onProgress, list };
})();
