// Status dots and links, the cheat sheet, share codes, what's new and mini mode.

/* ---------- Status dots, corners and links ---------- */
// Green: on and live in the game. Amber: on, but HotSwap is still connecting.
function modDot(g, m) {
  if (!invoke || !m.live || !isActive(g, m) || !isLive(g) || state.paused[g.id]) return '';
  return TRAINER.connected
    ? '<span class="mdot ok" title="Live in the game"></span>'
    : `<span class="mdot wait" title="${esc(TRAINER.message || 'Connecting to the game…')}"></span>`;
}
const cornerPicker = (kind, cur) => `<div class="corners">${CORNERS.map(([k, l]) =>
  `<button class="corner c-${k}${cur === k ? ' on' : ''}" data-${kind}="${k}" aria-label="${l}" title="${l}"><span></span></button>`).join('')}</div>`;

const openUrl = url => invoke ? invoke('plugin:opener|open_url', { url }).catch(console.error) : window.open(url, '_blank');
const issueUrl = (title, body, label) => `${REPO_URL}/issues/new?title=${encodeURIComponent(title)}&body=${encodeURIComponent(body)}${label ? '&labels=' + encodeURIComponent(label) : ''}`;
function bugUrl() {
  const run = findGame(runningId());
  return issueUrl('Bug: ', [
    '**What happened?**', '', '', '**What did you expect?**', '', '', '**Steps to reproduce**', '1. ', '', '---',
    `HotSwap ${VERSION || '(browser preview)'}`, `Game: ${run ? `${run.name} (${run.id})` : 'none running'}`,
    `Trainer: ${run ? TRAINER.connected ? 'connected' : TRAINER.message || 'not connected' : '-'}`,
  ].join('\n'), 'bug');
}
function requestUrl(g) {
  return issueUrl(`Game request: ${g?.name || ''}`, [
    `**Game:** ${g?.name || ''}`, `**Store:** ${g ? SOURCES[g.source] : ''}${g?.appId ? ` (app ${g.appId})` : ''}`, '',
    '**Mods you would like:**', '- ', '', '---', `HotSwap ${VERSION || ''}`,
  ].join('\n'), 'game request');
}

/* ---------- Cheat sheet: every mod's hotkey on the overlay while the key is held ---------- */
function showSheet(on) {
  const g = findGame(runningId());
  if (!invoke || !state.overlay || !g || !modsOf(g).length) return;
  const pins = state.pinned[g.id] || {};
  const items = modsOf(g).filter(m => m.live).sort((a, b) => !!pins[b.id] - !!pins[a.id]).map(m => {
    const k = keysOf(g, m);
    return {
      name: m.name, on: isActive(g, m),
      value: m.type === 'toggle' ? (valueOf(g, m) ? 'On' : 'Off') : m.type === 'slider' ? fmt(m, valueOf(g, m)) : '',
      keys: [k.up && (m.type === 'slider' ? '↑ ' : '') + comboLabel(k.up), k.down && '↓ ' + comboLabel(k.down)].filter(Boolean),
    };
  });
  overlaySend({ kind: 'sheet', show: on, items, game: g.name });
}

/* ---------- Export logs ---------- */
// A summary of this setup for bug reports: version, Windows, the settings that change behaviour.
function logSummary() {
  const r = RUNNING && findGame(RUNNING);
  return [
    `HotSwap ${VERSION || '?'}`,
    `Windows: ${navigator.userAgent}`,
    `Games: ${LIB.length} in the library, running: ${r ? `${r.name} (${r.id})` : 'none'}, injected: ${INJECTED || 'no'}`,
    `Trainer: ${TRAINER.connected ? 'connected' : 'not connected'} ${TRAINER.message || ''}`,
    `Low memory mode: ${!!state.lowMemory}, overlay: ${!!state.overlay}, Discord overlay: ${!!state.dcOverlay}, Discord voice: ${typeof dcVoiceOn === 'function' && dcVoiceOn()}`,
    `Hotkeys: ${!!state.hotkeys}, menu key: ${state.menuKey || 'none'}, auto updates: ${!!state.autoUpdate}`,
  ].join('\n');
}
document.addEventListener('click', async e => {
  const b = e.target.closest('[data-exportlogs]');
  if (!b || !invoke) return;
  b.disabled = true;
  try {
    const path = await invoke('export_logs', { small: b.dataset.exportlogs === 'small', summary: logSummary() });
    toast('Logs exported', path.split(/[\\/]/).pop());
  } catch (err) { toast("Couldn't export logs", String(err)); }
  b.disabled = false;
});

/* ---------- Share codes: a profile as a short string ---------- */
function shareCode(g) {
  const p = activeProf(g);
  const json = JSON.stringify({ g: g.id, n: p.name, v: p.values || {} });
  return 'HS1-' + btoa(String.fromCharCode(...new TextEncoder().encode(json)));
}
async function copyText(s) {
  try { await navigator.clipboard.writeText(s); return true; } catch { }
  const ta = Object.assign(document.createElement('textarea'), { value: s });
  document.body.append(ta); ta.select();
  const ok = document.execCommand('copy'); ta.remove(); return ok;
}
function importCode(g, code) {
  let data;
  try {
    const b = atob(code.trim().replace(/^HS1-/, ''));
    data = JSON.parse(new TextDecoder().decode(Uint8Array.from(b, c => c.charCodeAt(0))));
  } catch { toast("That isn't a HotSwap share code", ''); return; }
  if (String(data.g) !== g.id) { toast('That code is for', findGame(data.g)?.name || 'another game'); return; }
  // Only mods this game has, with values they accept.
  const values = {};
  for (const m of modsOf(g)) {
    const v = data.v?.[m.id];
    if (m.type === 'toggle' && typeof v === 'boolean') values[m.id] = v;
    if (m.type === 'slider' && typeof v === 'number' && isFinite(v)) values[m.id] = Math.min(m.max, Math.max(m.min, v));
  }
  const list = profilesOf(g), base = String(data.n || 'Shared').slice(0, 20);
  let name = base;
  for (let i = 2; list.some(p => p.name.toLowerCase() === name.toLowerCase()); i++) name = `${base} ${i}`;
  list.push({ name, values });
  importProfile = false;
  switchProfile(g, name);
}

/* ---------- What's new: release notes after an update ---------- */
function mdLite(s) {
  const inline = t => esc(t).replace(/\*\*(.+?)\*\*/g, '<b>$1</b>').replace(/\*(.+?)\*/g, '<i>$1</i>').replace(/`(.+?)`/g, '<code>$1</code>');
  let html = '', list = false;
  for (const raw of s.replace(/\r/g, '').split('\n')) {
    const line = raw.trimEnd(), li = line.match(/^(\s*)[-*] (.*)/);
    if (li) { if (!list) { html += '<ul>'; list = true; } html += `<li${li[1].length ? ' class="sub"' : ''}>${inline(li[2])}</li>`; continue; }
    if (list) { html += '</ul>'; list = false; }
    if (!line.trim()) continue;
    const h = line.match(/^#{1,4} (.*)/);
    if (h) { if (!/^#{1,2} /.test(line)) html += `<h3>${inline(h[1])}</h3>`; continue; }
    html += `<p>${inline(line.replace(/^> ?/, ''))}</p>`;
  }
  return html + (list ? '</ul>' : '');
}
async function showWhatsNew(manual) {
  let notes = '';
  try {
    const r = await fetch(`https://api.github.com/repos/Zetenypapp/hotswap/releases/tags/v${VERSION}`);
    if (r.ok) notes = (await r.json()).body || '';
  } catch { }
  if (!notes && state.pendingNotes?.version === VERSION) notes = state.pendingNotes.notes;
  state.pendingNotes = null; save();
  if (!notes) { if (manual) toast("Couldn't load the release notes", 'check your connection'); return; }
  $('wnTitle').textContent = `What's new in HotSwap ${VERSION}`;
  $('wnBody').innerHTML = mdLite(notes.split(/\n### 📦 Install/)[0]);
  $('wnLink').href = `${REPO_URL}/releases/tag/v${VERSION}`;
  $('whatsNew').hidden = false;
}

/* ---------- Mini mode: a small always-on-top window with the running game's mods ---------- */
function setMini(on) {
  state.mini = on;
  if (on) { const run = runningId(); if (run && findGame(run) && modsOf(findGame(run)).length) state.game = run; closePalette?.(); }
  document.body.classList.toggle('mini', on);
  invoke?.('set_mini', { on }).catch(err => toast("Couldn't switch modes", String(err)));
  save();
  render();
}
function renderMini() {
  const g = game();
  if (!g) return;
  const live = isLive(g), paused = live && state.paused[g.id], locked = !live || paused;
  const pins = state.pinned[g.id] || {};
  const mods = modsOf(g).filter(m => m.live).sort((a, b) => !!pins[b.id] - !!pins[a.id]);
  const row = m => {
    const v = valueOf(g, m), dis = locked ? ' disabled' : '', k = keysOf(g, m);
    const ctl = m.type === 'toggle'
      ? `<button class="seg sm" role="switch" aria-checked="${!!v}" aria-label="${esc(m.name)}" data-flip="${m.id}"${dis}><span class="${!v ? 'sel' : ''}">Off</span><span class="${v ? 'sel on' : ''}">On</span></button>`
      : m.type === 'slider'
        ? `<div class="slider sm"><input type="number" data-num="${m.id}" min="${m.min}" max="${m.max}" step="${m.step}" value="${fmt(m, v)}" aria-label="${esc(m.name)}"${dis}></div>`
        : `<button class="apply sm" data-act="${m.id}"${dis}>Apply</button>`;
    return `<div class="mrow${isActive(g, m) ? ' active' : ''}"><div class="mlabel"><span>${pins[m.id] ? '<i class="mpin"></i>' : ''}${esc(m.name)}${modDot(g, m)}</span><small>${esc([k.up && comboLabel(k.up), k.down && comboLabel(k.down)].filter(Boolean).join(' / ') || 'No hotkey')}</small></div>${ctl}</div>`;
  };
  $('mini').innerHTML = `<div class="mhead" data-tauri-drag-region>${tile(g, 'md')}<div class="mtitle" data-tauri-drag-region><b data-tauri-drag-region>${esc(g.name)}</b>
      <span class="${live && TRAINER.connected ? 'ok' : ''}" data-tauri-drag-region>${!live ? 'Not running' : paused ? 'Mods paused' : invoke && !TRAINER.connected ? esc(TRAINER.message || 'Connecting…') : `${activeCount(g)} on`}</span></div>
      <button class="ghost" id="miniExit" aria-label="Back to the full window" title="Back to the full window"><svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.4" stroke-linecap="round" stroke-linejoin="round"><path d="M15 3h6v6M9 21H3v-6M21 3l-7 7M3 21l7-7"/></svg></button>
      <button class="ghost" id="winClose" aria-label="Close" title="Close"><svg width="12" height="12" viewBox="0 0 10 10"><path d="M0 0l10 10M10 0L0 10" stroke="currentColor" stroke-width="1.4"/></svg></button></div>
    <div class="mlist">${mods.length ? mods.map(row).join('') : `<p class="empty">${esc(g.name)} has no mods yet.</p>`}</div>`;
}
