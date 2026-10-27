/* Mod loader page: browse ficsit.app, install mods with their dependencies, and manage installed
   ones. Satisfactory only for now. The backend refuses changes while the game is running. */
const Mods = (() => {
  const GAME = '526870';
  const S = { tab: 'installed', query: '', sort: 'popular', results: [], count: 0, loading: false, error: '',
    installed: [], build: null, updates: {}, checking: false, busy: {}, loaded: false };
  let searchTimer;

  const running = () => RUNNING === GAME;
  const fmtN = n => n >= 1e6 ? (n / 1e6).toFixed(1) + 'M' : n >= 1e3 ? Math.round(n / 1e3) + 'k' : String(n);
  const mb = b => b >= 1 << 20 ? `${(b / (1 << 20)).toFixed(0)} MB` : `${Math.max(1, Math.round(b / 1024))} KB`;

  async function refreshInstalled() {
    try {
      const r = await invoke('mods_state', { game: GAME });
      S.installed = r.installed; S.build = r.build; S.error = '';
    } catch (e) { S.error = String(e); }
    S.loaded = true;
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
    try { S.updates = await invoke('mods_updates', { game: GAME }); toast('Mods', Object.keys(S.updates).length ? `${Object.keys(S.updates).length} updates` : 'all up to date'); }
    catch (e) { toast("Couldn't check for mod updates", String(e)); }
    S.checking = false; list();
  }
  async function install(ref, name) {
    if (running()) return toast('Close Satisfactory first', 'then change mods');
    S.busy[ref] = { stage: 'resolving' }; list();
    try {
      const done = await invoke('mods_install', { game: GAME, reference: ref });
      delete S.updates[ref];
      toast(name || ref, done.length > 1 ? `installed with ${done.length - 1} ${done.length === 2 ? 'dependency' : 'dependencies'}` : 'installed');
    } catch (e) { toast(`Couldn't install ${name || ref}`, String(e)); }
    delete S.busy[ref];
    refreshInstalled();
  }
  async function act(cmd, args, msg) {
    if (running()) return toast('Close Satisfactory first', 'then change mods');
    try { await invoke(cmd, { game: GAME, ...args }); if (msg) toast(...msg); } catch (e) { toast("Couldn't change the mod", String(e)); }
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
  function render() {
    setAmbient(null);
    const body = $('modsBody');
    if (!invoke) { body.innerHTML = '<div class="empty-page"><h1>Mods</h1><p>The mod loader works in the desktop app.</p></div>'; return; }
    if (body.dataset.tab !== S.tab) {
      body.dataset.tab = S.tab;
      body.innerHTML = `
        <div class="lib-head"><h1>Mods</h1><span class="meta">Satisfactory${S.build ? ` · game build ${S.build}` : ''} · mods from <a href="https://ficsit.app" ${ext}>ficsit.app</a></span>
          <div class="chips"><button class="chip${S.tab === 'installed' ? ' on' : ''}" data-modtab="installed">Installed</button><button class="chip${S.tab === 'browse' ? ' on' : ''}" data-modtab="browse">Browse</button></div></div>
        <div id="modsWarn"></div>
        ${S.tab === 'browse' ? `<div class="mods-bar"><label class="search mods-search"><svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round"><circle cx="11" cy="11" r="7"/><path d="M20 20l-4-4"/></svg>
            <input id="modSearch" type="search" placeholder="Search mods" autocomplete="off" spellcheck="false" value="${esc(S.query)}"></label>
            <div class="chips">${[['popular', 'Popular'], ['updated', 'Updated'], ['new', 'New']].map(([k, l]) => `<button class="chip${S.sort === k ? ' on' : ''}" data-modsort="${k}">${l}</button>`).join('')}</div></div>` : ''}
        <div id="modsList"></div>`;
      const input = $('modSearch');
      if (input) input.addEventListener('input', () => { S.query = input.value; clearTimeout(searchTimer); searchTimer = setTimeout(() => search(false), 350); });
      if (!S.loaded) refreshInstalled();
      if (S.tab === 'browse' && !S.results.length && !S.loading) search(false);
    }
    list();
  }

  function list() {
    const el = $('modsList');
    if (!el || state.view !== 'mods') return;
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
    const t = e.target.closest('[data-modtab],[data-modsort],[data-modinstall],[data-modtoggle],[data-modremove],[data-modcheck],[data-modmore],[data-modupall]');
    if (!t) return;
    const d = t.dataset;
    if (d.modtab) { S.tab = d.modtab; render(); }
    else if (d.modsort) { S.sort = d.modsort; document.querySelectorAll('[data-modsort]').forEach(b => b.classList.toggle('on', b === t)); search(false); }
    else if (d.modinstall) install(d.modinstall, d.name);
    else if (d.modtoggle) { const on = t.getAttribute('aria-checked') !== 'true'; act('mods_set_enabled', { reference: d.modtoggle, enabled: on }, [d.modtoggle, on ? 'on' : 'off']); }
    else if (d.modremove) { if (confirm(`Uninstall ${d.name}?`)) act('mods_uninstall', { reference: d.modremove }, [d.name, 'removed']); }
    else if ('modcheck' in d) checkUpdates();
    else if ('modmore' in d) search(true);
    else if ('modupall' in d) Object.keys(S.updates).reduce((p, r) => p.then(() => install(r)), Promise.resolve());
  });

  return { render, onProgress, list };
})();
