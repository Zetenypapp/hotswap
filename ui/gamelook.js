// Customising a game page: category order, hidden mods, short names for mods, and row size.
// The page itself is drawn by renderMods in pages.js; this file holds the editing controls.

// The game whose page is being customised, or null.
let gameEdit = null;
const gameEditing = () => !!gameEdit && gameEdit === state.game && state.view === 'game';

function setGameEdit(on) {
  gameEdit = on ? state.game : null;
  if (on) state.cat = 'all';
  render();
}

// The bar at the top of the mod list while customising.
function gameEditBar(g) {
  const size = (k, l) => `<button class="chip${state.rowSize === k ? ' on' : ''}" data-glsize="${k}">${l}</button>`;
  return `<div class="gl-bar"><div class="txt"><b>Customise ${esc(g.name)}</b><span>Move categories, hide mods you don’t use and give mods short names. Hidden mods keep their hotkeys.</span></div>
    <div class="gl-tools"><div class="chips">${size('comfortable', 'Comfortable')}${size('compact', 'Compact')}</div>
      <button class="btn" data-glfold="1">Collapse all</button><button class="btn" data-glfold="0">Expand all</button>
      <button class="btn" data-glreset>Reset</button><button class="btn accent" data-gldone>Done</button></div></div>`;
}

// A mod row while customising: its real name, a short name box and Hide or Show.
function gameEditRow(g, m) {
  const off = !!state.hiddenMods[g.id]?.[m.id];
  return `<div class="row gl-row${off ? ' gl-off' : ''}" data-mod="${m.id}">
    <div class="label"><svg class="bolt" width="13" height="13" viewBox="0 0 24 24" fill="currentColor"><path d="M13 2L4 14h7l-1 8 9-12h-7z"/></svg>${esc(m.name)}</div>
    <input class="gl-name" data-glname="${m.id}" value="${esc(state.modNames[g.id]?.[m.id] || '')}" placeholder="Short name" maxlength="28" spellcheck="false" autocomplete="off" aria-label="Short name for ${esc(m.name)}">
    <button class="btn" data-glhide="${m.id}">${off ? 'Show' : 'Hide'}</button></div>`;
}

// Up and down buttons next to a category's name.
function gameEditMoves(g, name) {
  const order = catsOf(g).map(c => c.name), i = order.indexOf(name);
  return `<span class="gl-move"><button class="ghost" data-glmove="${esc(name)}" data-dir="-1" title="Move up"${i ? '' : ' disabled'}>↑</button><button class="ghost" data-glmove="${esc(name)}" data-dir="1" title="Move down"${i < order.length - 1 ? '' : ' disabled'}>↓</button></span>`;
}

document.addEventListener('click', e => {
  const t = e.target.closest('[data-glsize],[data-glfold],[data-glreset],[data-gldone],[data-glhide],[data-glmove],[data-gledit],#gameEditBtn');
  if (!t) return;
  const g = game(), d = t.dataset;
  if (!g) return;
  if (t.id === 'gameEditBtn') return setGameEdit(!gameEditing());
  if ('gledit' in d) return setGameEdit(true);
  if ('gldone' in d) return setGameEdit(false);
  if (d.glsize) { state.rowSize = d.glsize; save(); return renderMods(); }
  if (d.glfold) { g.cats.forEach(c => state.closed[g.id + '/' + c.name] = d.glfold === '1'); save(); return renderMods(); }
  if (d.glhide) {
    const h = (state.hiddenMods[g.id] ||= {});
    if (h[d.glhide]) delete h[d.glhide]; else h[d.glhide] = true;
    save(); renderChips(); renderMods(); pushMenu();
    return;
  }
  if (d.glmove) {
    const order = catsOf(g).map(c => c.name), i = order.indexOf(d.glmove), j = i + +d.dir;
    if (j < 0 || j >= order.length) return;
    [order[i], order[j]] = [order[j], order[i]];
    state.catOrder[g.id] = order;
    save(); renderChips(); renderMods(); pushMenu();
    return;
  }
  if ('glreset' in d && confirm(`Reset the order, hidden mods and short names for ${g.name}?`)) {
    delete state.catOrder[g.id]; delete state.hiddenMods[g.id]; delete state.modNames[g.id];
    save(); render();
  }
});
document.addEventListener('change', e => {
  const id = e.target.dataset?.glname, g = game();
  if (!id || !g) return;
  const names = (state.modNames[g.id] ||= {}), v = e.target.value.trim();
  if (v) names[id] = v; else delete names[id];
  save(); renderPanel(); pushMenu();
  if (state.mini) renderMini();
});
