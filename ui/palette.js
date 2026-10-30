// The command palette (Ctrl+K).

/* ---------- Command palette (Ctrl+K) ---------- */
let pal = null; // { q, sel, items }
const PAGE_ICONS = {
  home: '<path d="M3 10.5L12 3l9 7.5V21h-6v-6H9v6H3z"/>',
  library: ICONS_GRID(),
  maps: '<path d="M3 6l6-2 6 2 6-2v14l-6 2-6-2-6 2z" opacity=".35"/><path d="M9 4v14l6 2V6z"/>',
  mods: '<path d="M10 3h4v3a2 2 0 1 0 4 0V3h3v7h-3a2 2 0 1 0 0 4h3v7h-7v-3a2 2 0 1 0-4 0v3H3v-7h3a2 2 0 1 0 0-4H3V3z"/>',
  settings: '<circle cx="12" cy="12" r="4"/><path d="M11 2h2v4h-2zM11 18h2v4h-2zM2 11h4v2H2zM18 11h4v2h-4z"/>',
  action: '<path d="M13 2L4 14h7l-1 8 9-12h-7z"/>',
};
function ICONS_GRID() { return '<rect x="3" y="3" width="8" height="8" rx="2"/><rect x="13" y="3" width="8" height="8" rx="2"/><rect x="3" y="13" width="8" height="8" rx="2"/><rect x="13" y="13" width="8" height="8" rx="2"/>'; }
const palIcon = k => `<span class="pal-ic"><svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor">${PAGE_ICONS[k]}</svg></span>`;

function paletteItems(q) {
  const items = [];
  const add = (kind, label, sub, run, ic) => items.push({ kind, label, sub, run, ic });
  [['home', 'Home'], ['library', 'Library'], ['maps', 'Maps'], ['mods', 'Mods'], ['settings', 'Settings']]
    .filter(([v]) => feat(v)).forEach(([v, l]) => add('Page', l, '', () => go(v), palIcon(v)));
  const games = q ? LIB : gamesOnly().slice().sort((a, b) => b.lastPlayed - a.lastPlayed).slice(0, 6);
  games.forEach(g => add(g.software ? 'Software' : 'Game', g.name, [g.playtime ? hours(g.playtime) : '', modsOf(g).length ? `${modsOf(g).length} mods` : ''].filter(Boolean).join(' · '),
    () => go('game', g.id), tile(g, 'pal-tile', false)));
  if (q) LIB.forEach(g => modsOf(g).forEach(m => add('Mod', m.name, g.name, () => { go('game', g.id); flashMod(m.id); }, palIcon('action'))));
  const run = findGame(runningId());
  if (run && modsOf(run).length) add('Action', 'Turn off all mods', run.name, () => panic(), palIcon('action'));
  if (invoke) add('Action', 'Refresh Home', 'News and deals', () => { go('home'); refreshHome(true); }, palIcon('action'));
  add('Action', 'Rescan library', 'Steam, Epic and GOG', () => scanLibrary().then(() => { render(); toast('Found', `${LIB.length} games`); }), palIcon('action'));
  add('Action', state.panel ? 'Hide side panel' : 'Show side panel', '', () => { state.panel = !state.panel; save(); render(); }, palIcon('action'));
  if (invoke) add('Action', state.mini ? 'Leave mini mode' : 'Mini mode', 'Small window that stays on top', () => setMini(!state.mini), palIcon('action'));
  add('Action', "What's new", `HotSwap ${VERSION}`, () => showWhatsNew(true), palIcon('action'));
  add('Action', 'Report a bug', 'Opens GitHub', () => openUrl(bugUrl()), palIcon('action'));
  add('Action', 'Request a game', 'Opens GitHub', () => openUrl(requestUrl()), palIcon('action'));
  if (!q) return items;
  // Name starts with the query, then contains it, then only the subtitle matches.
  return items
    .map(it => ({ it, s: it.label.toLowerCase().startsWith(q) ? 0 : it.label.toLowerCase().includes(q) ? 1 : it.sub.toLowerCase().includes(q) ? 2 : 9 }))
    .filter(x => x.s < 9).sort((a, b) => a.s - b.s).map(x => x.it).slice(0, 40);
}
function openPalette() {
  pal = { q: '', sel: 0 };
  $('palette').hidden = false;
  $('palInput').value = '';
  renderPalette();
  $('palInput').focus();
  syncMap();
}
function closePalette() { pal = null; $('palette').hidden = true; syncMap(); }
function renderPalette() {
  pal.items = paletteItems(pal.q);
  pal.sel = Math.max(0, Math.min(pal.sel, pal.items.length - 1));
  $('palList').innerHTML = pal.items.map((it, i) => `<button class="pal-item${i === pal.sel ? ' sel' : ''}" data-pal="${i}" role="option" aria-selected="${i === pal.sel}">
      ${it.ic}<span class="pal-text"><b>${esc(it.label)}</b>${it.sub ? `<small>${esc(it.sub)}</small>` : ''}</span><span class="pal-kind">${it.kind}</span></button>`).join('')
    || `<p class="empty" style="padding:18px">Nothing matches “${esc(pal.q)}”.</p>`;
  $('palList').querySelector('.sel')?.scrollIntoView({ block: 'nearest' });
}
function runPalette(i) {
  const it = pal?.items[i];
  closePalette();
  it?.run();
}
function flashMod(id) {
  requestAnimationFrame(() => {
    const row = document.querySelector(`.row[data-mod="${CSS.escape(id)}"]`);
    if (!row) return;
    row.scrollIntoView({ block: 'center', behavior: 'smooth' });
    row.classList.add('flashrow');
    setTimeout(() => row.classList.remove('flashrow'), 1400);
  });
}
