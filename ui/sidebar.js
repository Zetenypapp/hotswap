// Customising the sidebar: the order of the pages and which show, games pinned to the top, and
// what game rows show next to the name. renderSide in app.js draws the games; this file draws
// the page buttons and holds the editing controls.

let sideEdit = false;
// The page buttons as written in index.html, read once so they can be reordered and hidden.
const NAV = [...document.querySelectorAll('.mainnav [data-view]')].map(b => ({ view: b.dataset.view, html: b.innerHTML }));
const NAV_BY = Object.fromEntries(NAV.map(n => [n.view, n]));
// The user's page order (new pages from an update go last).
const navOrder = () => [...(state.navOrder || []).filter(v => NAV_BY[v]), ...NAV.map(n => n.view).filter(v => !(state.navOrder || []).includes(v))];

// What game rows show: active mods, mods available, playtime or nothing. Older settings only had
// the active mods switch.
const sideInfo = () => state.sideInfo || (state.showCounts ? 'active' : 'none');
const SIDE_INFO = [['active', 'Mods on'], ['mods', 'Mods'], ['playtime', 'Playtime'], ['none', 'Nothing']];
function sideBadge(g) {
  const k = sideInfo();
  if (k === 'active') { const n = activeCount(g); return n ? `<span class="count">${n}</span>` : ''; }
  if (k === 'mods') { const n = modsOf(g).length; return n ? `<span class="count dim">${n}</span>` : ''; }
  if (k === 'playtime') return g.playtime ? `<span class="count dim">${hours(g.playtime)}</span>` : '';
  return '';
}

function renderNav() {
  const order = navOrder(), hid = state.navHidden || {};
  $('mainnav').innerHTML = sideEdit
    ? order.map((v, i) => `<div class="nav-edit${hid[v] ? ' off' : ''}"><span class="nav-lbl">${NAV_BY[v].html}</span>
        <button class="ghost" data-navmove="${v}" data-dir="-1" title="Move up"${i ? '' : ' disabled'}>↑</button><button class="ghost" data-navmove="${v}" data-dir="1" title="Move down"${i < order.length - 1 ? '' : ' disabled'}>↓</button>
        <button class="ghost" data-navhide="${v}" title="${hid[v] ? 'Show' : 'Hide'}">${eyeIcon(!hid[v])}</button></div>`).join('')
    : order.filter(v => !hid[v]).map(v => `<button data-view="${v}">${NAV_BY[v].html}</button>`).join('');
  $('sideEditBtn').classList.toggle('on', sideEdit);
}
const eyeIcon = open => `<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"><path d="M2 12s3.6-7 10-7 10 7 10 7-3.6 7-10 7S2 12 2 12z"/>${open ? '<circle cx="12" cy="12" r="3"/>' : '<path d="M4 4l16 16"/>'}</svg>`;

// The top of the games list while customising.
function sideEditBar() {
  return `<div class="side-edit"><b>Customise sidebar</b><span>Move or hide pages above. Pin games with the pin, so they stay at the top.</span>
    <div class="side-edit-lbl">Next to games</div>
    <div class="chips">${SIDE_INFO.map(([k, l]) => `<button class="chip sm${sideInfo() === k ? ' on' : ''}" data-sideinfo="${k}">${l}</button>`).join('')}</div>
    <div class="side-edit-btns"><button class="btn" data-sidereset>Reset</button><button class="btn accent" data-sidedone>Done</button></div></div>`;
}
// A game row while customising: no link, a pin button instead of the badge.
function sideEditRow(g, plain) {
  const on = !!state.sidePins?.[g.id];
  return `<div class="gamebtn${plain ? ' plain' : ''} edit">${tile(g)}<span class="name">${esc(g.name)}</span>
    <button class="ghost side-pin${on ? ' on' : ''}" data-sidepin="${g.id}" aria-pressed="${on}" title="${on ? 'Unpin' : 'Pin to the top'}"><svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor"><path d="M15 3l6 6-3 1-4 4 1 5-2 2-4-4-5 5H3v-1l5-5-4-4 2-2 5 1 4-4z"/></svg></button></div>`;
}

document.addEventListener('click', e => {
  const t = e.target.closest('[data-navmove],[data-navhide],[data-sideinfo],[data-sidepin],[data-sidereset],[data-sidedone],#sideEditBtn');
  if (!t) return;
  const d = t.dataset;
  if (t.id === 'sideEditBtn' || 'sidedone' in d) { sideEdit = t.id === 'sideEditBtn' ? !sideEdit : false; return renderSide(); }
  if (d.navmove) {
    const order = navOrder(), i = order.indexOf(d.navmove), j = i + +d.dir;
    if (j < 0 || j >= order.length) return;
    [order[i], order[j]] = [order[j], order[i]];
    state.navOrder = order;
  } else if (d.navhide) {
    const h = (state.navHidden ||= {});
    if (h[d.navhide]) delete h[d.navhide]; else h[d.navhide] = true;
  } else if (d.sideinfo) {
    state.sideInfo = d.sideinfo;
    if (state.view === 'settings') renderSettings();
  } else if (d.sidepin) {
    const p = (state.sidePins ||= {});
    if (p[d.sidepin]) delete p[d.sidepin]; else p[d.sidepin] = true;
  } else if ('sidereset' in d) {
    if (!confirm('Reset the sidebar: page order, hidden pages and pinned games?')) return;
    state.navOrder = null; state.navHidden = {}; state.sidePins = {};
  }
  save();
  renderSide();
});
