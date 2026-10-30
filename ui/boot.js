// Starts the app: loads the saved state and the library, then listens to the backend. Loaded
// last, after every other script has defined what it needs.

/* ---------- Boot ---------- */
(async () => {
  if (!appWin) $('winctl').hidden = true;
  await loadState();
  await scanLibrary();
  await loadHome();
  if (!findGame(state.game)) state.game = LIB[0]?.id ?? null;
  if (invoke) {
    state.playing = null;
    invoke('set_close_to_tray', { enabled: state.closeToTray }).catch(console.error);
    AUTOSTART = await invoke('get_autostart').catch(() => false);
    VERSION = await invoke('app_version').catch(() => '');
    if (state.lowMemory && state.view === 'home') { state.view = 'library'; render(); }
    if (state.autoUpdate) setTimeout(() => checkUpdate(false), 4000);
    // And every 30 minutes after that.
    setInterval(() => { if (state.autoUpdate) checkUpdate(false); }, 30 * 60 * 1000);
    // First start after an update: show what changed.
    if (VERSION && state.lastVersion && state.lastVersion !== VERSION) setTimeout(() => showWhatsNew(false), 800);
    if (VERSION) { state.lastVersion = VERSION; save(); }
    listen?.('game-running', e => onRunning(e.payload ?? null));
    listen?.('hotkey', e => onCombo(e.payload));
    listen?.('mod-progress', e => { if (typeof Mods !== 'undefined') Mods.onProgress(e.payload); });
    listen?.('trainer-status', e => { TRAINER = e.payload; if (state.view === 'game') { renderMods(); renderPanel(); } if (state.mini) renderMini(); pushMenu(); });
    listen?.('hotkey-up', e => { if (e.payload === state.sheetKey) showSheet(false); });
    listen?.('overlay-input', e => onOverlayInput(e.payload || {}));
    onRunning(await invoke('running_game').catch(() => null));
  } else if (state.playing && !findGame(state.playing)) state.playing = null;
  nav.stack = [{ view: state.view, game: state.game }]; nav.i = 0;
  render();
  if (state.mini) setMini(true);
})().catch(err => {
  console.error(err);
  document.querySelectorAll('.view').forEach(v => v.hidden = v.id !== 'mapsView');
  $('mapsBody').innerHTML = `<div class="empty-page"><h1>HotSwap couldn't start</h1>
    <p>${esc(err?.message || err)}</p><button class="btn" onclick="location.reload()">Try again</button></div>`;
});
