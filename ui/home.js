// The Home page: panels in a grid that can be moved, resized, hidden, added and given their own
// source (a feed, a JSON list or a website; the app works out which, see sources.rs).

// Each panel takes `w` of 6 columns and `h` rows of 100 px. Built-in panels have fixed IDs; added
// ones are "c" + a number. `src` replaces a panel's built-in content with a custom source.
const HOME_DEFAULT = [
  { id: 'jump', w: 6, h: 2 },
  { id: 'news', w: 6, h: 4 },
  { id: 'deals', w: 4, h: 6 },
  { id: 'top', w: 2, h: 3 },
  { id: 'new', w: 2, h: 3 },
];
const HOME_NAMES = { jump: 'Jump back in', news: 'From your games', deals: 'Hot deals', top: 'Top sellers', new: 'New on Steam' };
const homeLayout = () => (Array.isArray(state.homeLayout) && state.homeLayout.length ? state.homeLayout : HOME_DEFAULT).filter(p => p && p.id);
let homeEdit = false;      // the customise mode
let homeSrcOpen = null;    // panel whose source field is open
const HP_SOURCES = {};        // panel id -> { url, loading, data, error }
let webAllowed = false;    // web panels shown this session despite low memory mode

function setHomeLayout(list) {
  state.homeLayout = list.map(p => ({ ...p }));
  save();
}

// A panel's custom source, fetched once per URL (Refresh fetches again).
function loadSource(p, force) {
  const s = HP_SOURCES[p.id];
  if (!invoke || (!force && s && s.url === p.src && (s.loading || s.data || s.error))) return;
  HP_SOURCES[p.id] = { url: p.src, loading: true };
  invoke('fetch_source', { url: p.src })
    .then(data => { HP_SOURCES[p.id] = { url: p.src, data }; })
    .catch(e => { HP_SOURCES[p.id] = { url: p.src, error: String(e) }; })
    .finally(() => { if (state.view === 'home') renderHome(); });
}

function sourceBody(p) {
  const s = HP_SOURCES[p.id];
  if (!s || s.loading) return '<p class="empty">Loading…</p>';
  if (s.error) return `<p class="empty">${esc(s.error)}</p>`;
  const d = s.data;
  if (d.kind === 'web') {
    if (!d.framable) return `<p class="empty">This site can’t be shown inside apps. <a href="${esc(d.url)}" ${ext}>Open it in your browser →</a></p>`;
    if (state.lowMemory && !webAllowed) return `<p class="empty">Web panels are off in Low memory mode (each one is a whole web page). <button class="linkbtn" data-hpweb>Show anyway</button></p>`;
    return `<iframe class="hp-web" src="${esc(d.url)}" referrerpolicy="no-referrer" loading="lazy"></iframe>`;
  }
  if (!d.items.length) return '<p class="empty">Nothing in this feed yet.</p>';
  return `<div class="hp-list">${d.items.map(i => `<a class="hp-item" href="${esc(i.url)}" ${ext}>
      ${i.image ? `<span class="hp-thumb" style="background-image:url('${esc(i.image)}')"></span>` : ''}
      <span class="hp-txt"><b>${esc(i.title)}</b>${i.text ? `<span>${esc(i.text)}</span>` : ''}${i.date ? `<small>${esc(String(i.date).slice(0, 16))}</small>` : ''}</span></a>`).join('')}</div>`;
}

function renderHome() {
  setAmbient(null);
  const h = new Date().getHours();
  const hi = h < 5 ? 'Late night session?' : h < 12 ? 'Good morning' : h < 18 ? 'Good afternoon' : 'Good evening';
  const q = query();
  const match = t => !q || t.toLowerCase().includes(q);
  const H = HOME || snapshotHome();

  // Built-in panels: [title, subtitle, link, body].
  const built = {
    jump: () => {
      // Anti-cheat games can't be modded, so they never show up as a place to jump back in.
      const recent = gamesOnly().filter(g => !g.anticheat.length && match(g.name)).sort((a, b) => b.lastPlayed - a.lastPlayed).slice(0, 6);
      const jump = recent.map(g => {
        const n = activeCount(g), total = modsOf(g).length;
        return `<button data-game="${g.id}" title="${esc(g.name)}">${capsule(g)}
          ${total ? `<span class="badge">${n ? `<b>${n}</b> active` : `${total} mods`}</span>` : ''}</button>`;
      }).join('');
      return ['Recently played', '', jump ? `<div class="jump">${jump}</div>` : '<p class="empty">Games you play show up here.</p>'];
    },
    news: () => {
      const news = H.news.filter(n => {
        const g = findGame(n.app);
        return match(n.title) || (g && match(g.name));
      }).map((n, i) => {
        const g = findGame(n.app);
        const name = g?.name || DEMO_LIBRARY.find(d => d.appId === n.app)?.name || '';
        const feature = i === 0 && !q;
        const bg = g?.art.hero || g?.art.header;
        const who = feature && g?.art.logo ? img(g.art.logo, 'logo') : `<span class="who">${g ? tile(g, '', false) : ''}${esc(name)}</span>`;
        const when = new Date(n.date * 1000).toLocaleDateString('en-US', { month: 'short', day: 'numeric' });
        return `<a class="news${feature ? ' feature' : ''}" style="--c:${g?.color || hue(n.app)}" href="${esc(n.url)}" ${ext}>
          ${bg ? `<span class="bg" style="background-image:url('${esc(bg)}')"></span><span class="shade"></span>` : ''}
          ${who}<h3>${esc(n.title)}</h3>${n.text && feature ? `<p>${esc(n.text)}</p>` : ''}
          <span class="src">${when} · ${esc(n.src || 'Steam announcement')}</span></a>`;
      }).join('');
      return ['Announcements from the developers of your recent games', '', `<div class="news-grid">${news || '<p class="empty">No news matches your search.</p>'}</div>`];
    },
    deals: () => {
      const deals = H.deals.filter(d => match(d.t)).map(d => `<a class="deal" style="--c:${hue(d.app)}" href="https://store.steampowered.com/app/${d.app}/" ${ext}>
          <span class="art">${img(steamCdn(d.app, 'header.jpg'))}<span class="off">−${d.off}%</span></span>
          <span class="body"><span class="t">${esc(d.t)}</span>
          <span class="price"><b>${money(d.sale)}</b><s>${money(d.was)}</s></span>
          ${d.rating ? `<span class="rating">${esc(d.rating)}</span>` : ''}</span></a>`).join('');
      return ['Top-rated Steam discounts', `<a href="https://www.cheapshark.com/" ${ext}>More on CheapShark →</a>`, `<div class="deal-grid">${deals || '<p class="empty">No deals match your search.</p>'}</div>`];
    },
    top: () => ['', `<a href="https://steamdb.info/charts/" ${ext}>SteamDB charts →</a>`, `<div class="listcard">${rankList(H.top)}</div>`],
    new: () => ['', '', `<div class="listcard">${rankList(H.newReleases)}</div>`],
  };
  const rankList = list => `<ol class="rank">${list.filter(x => match(x.t)).map((x, i) => `<li><a href="https://store.steampowered.com/app/${x.app}/" ${ext}>
      <span class="i">${i + 1}</span><span class="thumb">${img(steamCdn(x.app, 'header.jpg'), 'thumb')}</span>
      <span class="nm">${esc(x.t)}${x.note ? `<small>${x.note}</small>` : ''}</span>
      <span class="pr">${x.off ? `<em>−${x.off}%</em>` : ''}${x.price ? money(x.price) : 'Free'}</span></a></li>`).join('') || '<li class="empty" style="padding:10px">No matches.</li>'}</ol>`;

  const layout = homeLayout();
  const panels = layout.filter(p => !p.hidden).map(p => {
    let title = HOME_NAMES[p.id] || 'Custom panel', sub = '', link = '', body;
    if (p.src) {
      loadSource(p);
      const d = HP_SOURCES[p.id]?.data;
      title = p.title || d?.title || HOME_NAMES[p.id] || 'Custom panel';
      sub = d ? (d.kind === 'web' ? 'Website' : d.feed ? 'Feed found on the site' : 'Feed') : '';
      link = `<a href="${esc(d?.url || p.src)}" ${ext}>Open →</a>`;
      body = sourceBody(p);
    } else if (built[p.id]) {
      [sub, link, body] = built[p.id]();
    } else {
      body = '<p class="empty">Paste a link to a site, feed or list in this panel’s source.</p>';
    }
    const custom = !HOME_NAMES[p.id];
    const tools = homeEdit ? `<div class="hp-tools">
        <span class="hp-size">${p.w}×${p.h}</span>
        <button class="chip" data-hpsrc="${p.id}">Source</button>
        <button class="chip" data-hphide="${p.id}">${custom ? 'Remove' : 'Hide'}</button></div>
      <span class="hp-grip" data-hpgrip="${p.id}" title="Drag to resize"></span>` : '';
    const srcForm = homeEdit && homeSrcOpen === p.id ? `<form class="hp-srcform" data-hpsrcform="${p.id}">
        <input name="src" type="text" placeholder="A site, feed or list, like pcgamer.com" value="${esc(p.src || '')}" autocomplete="off" spellcheck="false">
        <input name="title" type="text" placeholder="Title (optional)" value="${esc(p.title || '')}" autocomplete="off">
        <button class="btn accent" type="submit">Use</button>${HOME_NAMES[p.id] ? `<button class="btn" type="button" data-hpdefault="${p.id}">Default</button>` : ''}</form>` : '';
    return `<section class="hp${homeEdit ? ' edit' : ''}" data-hp="${p.id}" style="grid-column:span ${p.w};grid-row:span ${p.h}"${homeEdit ? ' draggable="true"' : ''}>
      <div class="sec-head"><h2>${esc(title)}</h2>${sub ? `<span>${esc(sub)}</span>` : ''}${homeEdit ? '' : link}</div>
      ${srcForm}<div class="hp-body">${body}</div>${tools}</section>`;
  }).join('');
  const hidden = layout.filter(p => p.hidden);

  $('homeBody').innerHTML = `
    <div class="hello"><h1>${hi}</h1><p>Pick up where you left off, or see what's new with your games.</p></div>
    ${homeEdit ? `<div class="hp-bar"><span><b>Customising Home.</b> Drag panels to move them, drag a corner to resize, and give any panel its own source.</span>
      ${hidden.map(p => `<button class="chip" data-hpshow="${p.id}">+ ${esc(HOME_NAMES[p.id] || p.title || 'Panel')}</button>`).join('')}
      <button class="chip" data-hpadd>+ New panel</button><button class="btn" data-hpreset>Reset</button><button class="btn accent" data-hpdone>Done</button></div>` : ''}
    <div class="home-grid${homeEdit ? ' editing' : ''}">${panels}</div>
    <p class="stamp">${H.fetchedAt
      ? `Updated ${ago(H.fetchedAt)} from Steam and CheapShark. Prices in USD.`
      : `Showing a saved snapshot from ${SNAPSHOT}.`}
      ${invoke ? `<button class="linkbtn" id="homeRefresh"${homeLoading ? ' disabled' : ''}>${homeLoading ? 'Updating…' : 'Refresh'}</button>` : ''}</p>`;
}

// Customise mode: the button next to Settings, then the panel tools.
function setHomeEdit(on) {
  homeEdit = on; homeSrcOpen = null;
  render();
}
document.addEventListener('click', e => {
  const t = e.target.closest('[data-hpsrc],[data-hphide],[data-hpshow],[data-hpadd],[data-hpreset],[data-hpdone],[data-hpdefault],[data-hpweb],#customizeBtn');
  if (!t) return;
  const d = t.dataset, layout = homeLayout().map(p => ({ ...p }));
  const find = id => layout.find(p => p.id === id);
  if (t.id === 'customizeBtn') return setHomeEdit(!homeEdit);
  if ('hpdone' in d) return setHomeEdit(false);
  if ('hpweb' in d) { webAllowed = true; return renderHome(); }
  if ('hpreset' in d) { if (confirm('Put Home back to how it was?')) { setHomeLayout(HOME_DEFAULT); renderHome(); } return; }
  if ('hpadd' in d) { const id = 'c' + Date.now(); layout.push({ id, w: 3, h: 4 }); setHomeLayout(layout); homeSrcOpen = id; return renderHome(); }
  if (d.hpsrc) { homeSrcOpen = homeSrcOpen === d.hpsrc ? null : d.hpsrc; return renderHome(); }
  if (d.hpshow) { const p = find(d.hpshow); if (p) delete p.hidden; setHomeLayout(layout); return renderHome(); }
  if (d.hpdefault) { const p = find(d.hpdefault); if (p) { delete p.src; delete p.title; } homeSrcOpen = null; setHomeLayout(layout); return renderHome(); }
  if (d.hphide) {
    const p = find(d.hphide);
    if (!p) return;
    if (HOME_NAMES[p.id]) p.hidden = true; else layout.splice(layout.indexOf(p), 1);
    setHomeLayout(layout); renderHome();
  }
});
document.addEventListener('submit', e => {
  const f = e.target.closest('[data-hpsrcform]');
  if (!f) return;
  e.preventDefault();
  const layout = homeLayout().map(p => ({ ...p })), p = layout.find(x => x.id === f.dataset.hpsrcform);
  if (!p) return;
  const src = f.src.value.trim(), title = f.title.value.trim();
  if (src) p.src = src; else delete p.src;
  if (title) p.title = title; else delete p.title;
  delete HP_SOURCES[p.id];
  homeSrcOpen = null;
  setHomeLayout(layout); renderHome();
}, true);

// Moving: drag a panel onto another; it goes before or after it, by which half you drop on.
let hpDrag = null;
document.addEventListener('dragstart', e => {
  const p = homeEdit && e.target.closest?.('.hp');
  if (!p) return;
  hpDrag = p.dataset.hp;
  e.dataTransfer.effectAllowed = 'move';
  p.classList.add('dragging');
});
document.addEventListener('dragover', e => {
  if (!hpDrag || !e.target.closest?.('.hp')) return;
  e.preventDefault();
});
document.addEventListener('drop', e => {
  const over = hpDrag && e.target.closest?.('.hp');
  if (!over) return;
  e.preventDefault();
  const to = over.dataset.hp, r = over.getBoundingClientRect();
  const after = (e.clientX - r.left) / r.width > 0.5;
  if (to !== hpDrag) {
    const layout = homeLayout().map(p => ({ ...p }));
    const from = layout.findIndex(p => p.id === hpDrag);
    const [moved] = layout.splice(from, 1);
    const at = layout.findIndex(p => p.id === to);
    layout.splice(after ? at + 1 : at, 0, moved);
    setHomeLayout(layout);
  }
  hpDrag = null;
  renderHome();
});
document.addEventListener('dragend', () => { if (hpDrag) { hpDrag = null; renderHome(); } });

// Resizing: drag a panel's corner; the size snaps to whole columns (of 6) and 100 px rows.
document.addEventListener('pointerdown', e => {
  const grip = homeEdit && e.target.closest?.('[data-hpgrip]');
  if (!grip) return;
  e.preventDefault();
  const panel = grip.closest('.hp'), grid = panel.parentElement;
  const gap = parseFloat(getComputedStyle(grid).columnGap) || 18;
  const colW = (grid.clientWidth - gap * 5) / 6, rowH = 100;
  const r = panel.getBoundingClientRect();
  const id = grip.dataset.hpgrip;
  let w, h;
  const move = ev => {
    w = Math.max(2, Math.min(6, Math.round((ev.clientX - r.left + gap) / (colW + gap))));
    h = Math.max(2, Math.min(10, Math.round((ev.clientY - r.top + gap) / (rowH + gap))));
    panel.style.gridColumn = `span ${w}`; panel.style.gridRow = `span ${h}`;
    panel.querySelector('.hp-size').textContent = `${w}×${h}`;
  };
  const up = () => {
    removeEventListener('pointermove', move); removeEventListener('pointerup', up);
    if (w && h) {
      const layout = homeLayout().map(p => p.id === id ? { ...p, w, h } : { ...p });
      setHomeLayout(layout);
    }
    renderHome();
  };
  addEventListener('pointermove', move); addEventListener('pointerup', up);
});

// Home's Refresh also fetches the custom sources again.
document.addEventListener('click', e => { if (e.target.closest('#homeRefresh')) Object.keys(HP_SOURCES).forEach(k => delete HP_SOURCES[k]); }, true);
