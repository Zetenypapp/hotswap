// Hotkey presses and capturing new key bindings.

// Applies a hotkey press, from the system-wide listener or (in a browser) the page.
function onCombo(combo) {
  if (capture) return;
  if (combo === state.panicKey) { panic(comboLabel(combo)); return; }
  if (combo === state.sheetKey) { showSheet(true); return; }
  if (combo === state.menuKey) { toggleMenu(); return; }
  const g = findGame(runningId());
  if (!g || !state.hotkeys) return;
  for (const m of modsOf(g)) {
    const k = keysOf(g, m);
    if (combo !== k.up && combo !== k.down) continue;
    const how = comboLabel(combo);
    if (m.type === 'toggle') setMod(g, m, !valueOf(g, m), how);
    else if (m.type === 'slider') setMod(g, m, valueOf(g, m) + (combo === k.down ? -1 : 1) * m.step * (m.max - m.min > 50 ? 10 : 1), how);
    else setMod(g, m, null, how);
    document.querySelectorAll(`[data-key="${CSS.escape(combo)}"]`).forEach(el => { el.classList.add('flash'); setTimeout(() => el.classList.remove('flash'), 250); });
    return;
  }
}

function startCapture(gid, mid, dir) {
  capture = { gid, mid, dir };
  syncHotkeys(); // release global keys so the new key reaches us
  render();
}
function finishCapture(combo) {
  const c = capture;
  capture = null;
  if (combo !== undefined) {
    if (c.mid === MENU) {
      const g = findGame(runningId()) || game();
      const used = combo && g && whoUses(g, combo, 'menu');
      state.menuKey = combo;
      toast('Overlay menu key', combo ? comboLabel(combo) : 'unbound');
      if (used) setTimeout(() => toast(`${comboLabel(combo)} is also used by`, used), 900);
    } else if (c.mid === SHEET) {
      const g = findGame(runningId()) || game();
      const used = combo && g && whoUses(g, combo, 'sheet');
      state.sheetKey = combo;
      toast('Cheat sheet key', combo ? comboLabel(combo) : 'unbound');
      if (used) setTimeout(() => toast(`${comboLabel(combo)} is also used by`, used), 900);
    } else if (c.mid === null) {
      if (!combo) { toast('The panic key needs a key', ''); }
      else {
        const g = findGame(runningId()) || game();
        const used = g && whoUses(g, combo, 'panic');
        state.panicKey = combo;
        toast('Panic key', comboLabel(combo));
        if (used) setTimeout(() => toast(`${comboLabel(combo)} is also used by`, used), 900);
      }
    } else {
      const g = findGame(c.gid), m = modsOf(g).find(x => x.id === c.mid);
      const used = combo && whoUses(g, combo, `${m.id}/${c.dir}`);
      ((state.binds[g.id] ||= {})[m.id] ||= {})[c.dir] = combo;
      toast(modName(g, m), combo ? comboLabel(combo) : 'unbound');
      if (used) setTimeout(() => toast(`${comboLabel(combo)} is also used by`, used), 900);
    }
    save();
  }
  lastHotkeys = '';
  render();
}

function switchProfile(g, name) {
  state.activeProfile[g.id] = name;
  profileMenu = false;
  logLine(g, `Profile → ${name}`);
  save();
  toast('Profile', name);
  render();
}
