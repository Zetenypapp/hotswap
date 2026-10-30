// Clicks, typing and keys anywhere in the window.

const findMod = id => modsOf(game()).find(m => m.id === id);
let wipeArmed = false;

document.addEventListener('click', e => {
  const a = e.target.closest('a[target="_blank"]');
  if (a && invoke) { e.preventDefault(); invoke('plugin:opener|open_url', { url: a.href }).catch(console.error); return; }
  if (e.target.closest('#whatsNew') && (e.target.id === 'whatsNew' || e.target.closest('#wnClose'))) { $('whatsNew').hidden = true; return; }
  if (profileMenu && !e.target.closest('.profile')) { profileMenu = newProfile = importProfile = false; renderGame(); }
  if (pal) {
    const item = e.target.closest('[data-pal]');
    if (item) runPalette(+item.dataset.pal);
    else if (!e.target.closest('.pal-box')) closePalette();
    return;
  }
  const t = e.target.closest('button');
  if (!t) return;
  const d = t.dataset;
  if (capture && !d.bind && t.id !== 'panicBind' && t.id !== 'sheetBind' && t.id !== 'menuBind') { capture = null; lastHotkeys = ''; render(); }
  if (d.game) go('game', d.game);
  else if (d.view) go(d.view);
  else if (d.flip) { const g = game(), m = findMod(d.flip); setMod(g, m, !valueOf(g, m)); }
  else if (d.act) setMod(game(), findMod(d.act));
  else if (d.bind) startCapture(game().id, d.bind, d.dir);
  else if (d.chip) { state.cat = d.chip; save(); renderChips(); renderMods(); }
  else if (d.cat) { state.closed[d.cat] = !state.closed[d.cat]; save(); renderMods(); }
  else if (d.libfilter) { state.libFilter = d.libfilter; save(); renderLibrary(); }
  else if (d.accent) { state.accent = d.accent; save(); render(); }
  else if (d.profile) switchProfile(game(), d.profile);
  else if (d.pin) {
    const g = game(), p = (state.pinned[g.id] ||= {});
    if (p[d.pin]) delete p[d.pin]; else p[d.pin] = true;
    save(); renderMods(); if (state.mini) renderMini();
    toast(findMod(d.pin).name, p[d.pin] ? 'pinned' : 'unpinned');
  }
  else if (d.ocorner) { state.overlayCorner = d.ocorner; save(); renderSettings(); overlaySend({ title: 'Overlay', value: 'Here', on: true, game: 'Mod changes show up here' }); }
  else if (d.hcorner) { state.hudCorner = d.hcorner; save(); renderSettings(); syncHud(true); }
  else if (d.rcorner) { state.radarCorner = d.rcorner; save(); renderSettings(); if (radarOpen) invoke?.('radar_show', { show: true, corner: state.radarCorner }); else toast('Radar position', CORNERS.find(c => c[0] === d.rcorner)[1]); }
  else if (d.delprofile) {
    const g = game(), list = profilesOf(g);
    state.profiles[g.id] = list.filter(p => p.name !== d.delprofile);
    if (state.activeProfile[g.id] === d.delprofile) state.activeProfile[g.id] = state.profiles[g.id][0].name;
    save(); toast('Deleted profile', d.delprofile); renderGame();
  }
  else if (d.jump) {
    const run = runningId();
    if (run && (state.view !== 'game' || state.game !== run)) go('game', run);
    if (d.jump !== 'mods' && !state.panel) { state.panel = true; save(); renderGame(); }
    $(d.jump).scrollIntoView({ behavior: 'smooth', block: 'start' });
  }
  else switch (t.id) {
    case 'favBtn': state.fav[state.game] = !state.fav[state.game]; save(); renderGame(); break;
    case 'playBtn': isLive(game()) ? panic() : play(game()); break;
    case 'injectBtn': inject(game()); break;
    case 'stopBtn':
      if (!stopArmed) { stopArmed = true; renderSide(); setTimeout(() => { stopArmed = false; renderSide(); }, 3000); }
      else {
        stopArmed = false;
        const run = findGame(runningId());
        if (!invoke) setPlaying(null);
        else if (run) invoke('close_game', { id: run.id }).then(n => toast(run.name, n ? 'closed' : "couldn't be closed"));
        renderSide();
      }
      break;
    case 'powerBtn': {
      const run = findGame(runningId());
      if (!run) break;
      state.paused[run.id] = !state.paused[run.id];
      logLine(run, state.paused[run.id] ? 'Mods paused' : 'Mods resumed');
      save(); render(); toast('Mods', state.paused[run.id] ? 'paused' : 'resumed');
      break;
    }
    case 'panelBtn': state.panel = !state.panel; save(); renderGame(); break;
    case 'resetAll': case 'resetBtn': {
      const g = game();
      activeProf(g).values = {}; logLine(g, 'All mods → Off'); save(); render(); toast('All mods', 'off'); break;
    }
    case 'profileBtn': profileMenu = !profileMenu; newProfile = importProfile = false; renderGame(); break;
    case 'addProfile': newProfile = true; importProfile = false; renderGame(); $('pnewName')?.focus(); break;
    case 'importProfile': importProfile = true; newProfile = false; renderGame(); $('pimportCode')?.focus(); break;
    case 'copyProfile': copyText(shareCode(game())).then(ok => toast(ok ? 'Share code copied' : "Couldn't copy", activeProf(game()).name)); profileMenu = false; renderGame(); break;
    case 'sheetBind': startCapture(null, SHEET, 'up'); break;
    case 'menuBind': startCapture(null, MENU, 'up'); break;
    case 'setHud': state.hud = !state.hud; save(); render(); syncHud(true); break;
    case 'setHudStats': state.hudStats = !state.hudStats; save(); render(); syncHud(true); break;
    case 'setDiscord': state.discord = !state.discord; save(); render(); break;
    case 'setFgFriendly': state.fgFriendly = !state.fgFriendly; save(); syncHud(true); pushStatsCfg(true); render(); break;
    case 'setDcOverlay': state.dcOverlay = !state.dcOverlay; save(); render(); break;
    case 'setDcTalk': state.dcTalk = !state.dcTalk; save(); render(); break;
    case 'setDcNotes': state.dcNotes = !state.dcNotes; save(); dcOptions(); render(); break;
    case 'setDcJoins': state.dcJoins = !state.dcJoins; save(); dcOptions(); render(); break;
    case 'dcVoiceBtn': {
      const on = !dcVoiceOn();
      try { localStorage.setItem('hs.dc.voice', on ? '1' : '0'); } catch {}
      VOICE = null; invoke?.(on ? 'voice_start' : 'voice_stop').catch(() => {}); render(); break;
    }
    case 'setLowMem': state.lowMemory = !state.lowMemory; save(); render(); toast('Low memory mode ' + (state.lowMemory ? 'on' : 'off'), 'restart HotSwap to apply'); break;
    case 'testMenu': toggleMenu(true, true); break;
    case 'whatsNewBtn': showWhatsNew(true); break;
    case 'winMini': setMini(true); break;
    case 'miniExit': setMini(false); break;
    case 'panicBind': startCapture(null, null, 'up'); break;
    case 'resetBinds': state.binds = {}; save(); lastHotkeys = ''; render(); toast('Key bindings', 'reset'); break;
    case 'sortBtn': state.sortAZ = !state.sortAZ; save(); renderSide(); toast('Games sorted', state.sortAZ ? 'A–Z' : 'recently played'); break;
    case 'backBtn': step(-1); break;
    case 'fwdBtn': step(1); break;
    case 'setCounts': state.showCounts = !state.showCounts; save(); render(); break;
    case 'setHotkeys': state.hotkeys = !state.hotkeys; save(); render(); break;
    case 'setSoftware': state.showSoftware = !state.showSoftware; save(); render(); break;
    case 'setTray': state.closeToTray = !state.closeToTray; save(); invoke?.('set_close_to_tray', { enabled: state.closeToTray }); render(); break;
    case 'setAutostart':
      invoke('set_autostart', { enabled: !AUTOSTART })
        .then(() => { AUTOSTART = !AUTOSTART; toast('Start with Windows', AUTOSTART ? 'on' : 'off'); })
        .catch(err => toast("Couldn't change startup", String(err)))
        .finally(render);
      break;
    case 'homeRefresh': refreshHome(true); break;
    case 'setAutoUpdate': state.autoUpdate = !state.autoUpdate; save(); render(); break;
    case 'setSounds': state.sounds = !state.sounds; save(); render(); if (state.sounds) chime(true); break;
    case 'setOverlay': state.overlay = !state.overlay; if (!state.overlay) { if (menuOpen) toggleMenu(false); hudOn = false; invoke?.('overlay_close'); } save(); render(); break;
    case 'testOverlay': overlaySend({ title: 'HotSwap overlay', value: 'On', on: true, game: 'This is how mod changes look in game' }); chime(true); break;
    case 'checkUpdateBtn': checkUpdate(true); break;
    case 'updateBtn': case 'updatePill': installUpdate(); break;
    case 'softToggle': state.softOpen = !state.softOpen; save(); renderSide(); break;
    case 'rescanBtn': scanLibrary().then(() => { render(); toast('Found', `${LIB.length} games`); }); break;
    case 'wipeBtn':
      if (!wipeArmed) { wipeArmed = true; t.textContent = 'Click again to reset'; setTimeout(() => { wipeArmed = false; if (t.isConnected) t.textContent = 'Reset'; }, 3000); }
      else {
        wipeArmed = false;
        Object.values(state.profiles).forEach(list => list.forEach(p => { p.values = {}; }));
        save(); render(); toast('All mod settings', 'reset');
      }
      break;
    case 'winMin': appWin?.minimize(); break;
    case 'winMax': appWin?.toggleMaximize(); break;
    case 'winClose': appWin?.close(); break;
  }
});

document.addEventListener('submit', e => {
  if (e.target.id === 'pimportForm') { e.preventDefault(); importCode(game(), $('pimportCode').value); return; }
  if (e.target.id !== 'pnewForm') return;
  e.preventDefault();
  const g = game(), name = $('pnewName').value.trim();
  if (!name) return;
  if (profilesOf(g).some(p => p.name.toLowerCase() === name.toLowerCase())) { toast('There is already a profile called', name); return; }
  profilesOf(g).push({ name, values: { ...vals(g) } });
  newProfile = false;
  switchProfile(g, name);
});

document.addEventListener('change', e => {
  const id = e.target.dataset.num || e.target.dataset.range;
  if (id) setMod(game(), findMod(id), parseFloat(e.target.value));
});
document.addEventListener('dblclick', e => {
  const id = e.target.dataset?.range;
  if (id) { const m = findMod(id); setMod(game(), m, m.def); }
});
document.addEventListener('input', e => {
  if (e.target.dataset.range) {
    const m = findMod(e.target.dataset.range), v = parseFloat(e.target.value);
    e.target.style.setProperty('--p', pct(m, v));
    e.target.closest('.slider').querySelector('[data-num]').value = fmt(m, v);
  }
  if (e.target.id === 'search') render();
  if (e.target.id === 'palInput') { pal.q = e.target.value.trim().toLowerCase(); pal.sel = 0; renderPalette(); }
  if (e.target.id === 'notes') { state.notes[state.game] = e.target.value; save(); }
});

document.addEventListener('keydown', e => {
  if (capture) {
    e.preventDefault();
    if (e.key === 'Escape') finishCapture(undefined);
    else if (e.key === 'Backspace' || e.key === 'Delete') finishCapture(capture.mid === null ? undefined : null);
    else { const combo = comboFromEvent(e); if (combo) finishCapture(combo); }
    return;
  }
  if (pal) {
    if (e.key === 'Escape') { e.preventDefault(); closePalette(); }
    else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault();
      pal.sel = (pal.sel + (e.key === 'ArrowDown' ? 1 : -1) + pal.items.length) % Math.max(1, pal.items.length);
      renderPalette();
    } else if (e.key === 'Enter') { e.preventDefault(); runPalette(pal.sel); }
    return;
  }
  if (e.ctrlKey && e.key.toLowerCase() === 'k') { e.preventDefault(); openPalette(); return; }
  const typing = /INPUT|TEXTAREA/.test(document.activeElement.tagName);
  if (e.key === '/' && !typing) { e.preventDefault(); $('search').focus(); $('search').select(); return; }
  if (e.key === 'Escape' && !$('whatsNew').hidden) { $('whatsNew').hidden = true; return; }
  if (e.key === 'Escape' && profileMenu) { profileMenu = newProfile = importProfile = false; renderGame(); return; }
  if (e.key === 'Escape' && document.activeElement.id === 'search') { e.target.value = ''; render(); e.target.blur(); return; }
  if (e.altKey && e.key === 'ArrowLeft') { step(-1); return; }
  if (e.altKey && e.key === 'ArrowRight') { step(1); return; }
  // In the desktop app the system-wide listener handles hotkeys, even while HotSwap has focus.
  if (invoke || typing) return;
  const combo = comboFromEvent(e);
  if (combo && state.playing) { e.preventDefault(); onCombo(combo); }
});
// Mouse back/forward buttons
window.addEventListener('mouseup', e => { if (e.button === 3) step(-1); if (e.button === 4) step(1); });
