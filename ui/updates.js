// Checking for and installing HotSwap updates.

/* ---------- Updates ---------- */
async function checkUpdate(manual) {
  if (!invoke) return;
  try {
    UPDATE = await invoke('check_update');
    UPDATE_MSG = UPDATE ? '' : 'You have the latest version.';
    if (manual) toast(UPDATE ? `Version ${UPDATE.version}` : 'HotSwap is up to date', UPDATE ? 'available' : '');
  } catch (e) {
    UPDATE_MSG = "Couldn't check for updates.";
    if (manual) toast("Couldn't check for updates", String(e));
  }
  const pill = $('updatePill');
  pill.hidden = !UPDATE;
  if (UPDATE) pill.textContent = `Update to ${UPDATE.version}`;
  if (state.view === 'settings') renderSettings();
}
async function installUpdate() {
  if (!UPDATE) return;
  toast('Downloading update', UPDATE.version);
  $('updatePill').textContent = 'Updating…';
  state.pendingNotes = { version: UPDATE.version, notes: UPDATE.notes || '' };
  await saveNow();
  // HotSwap closes and the installer reopens it on the new version.
  invoke('install_update').catch(e => { toast("Update failed", String(e)); $('updatePill').textContent = `Update to ${UPDATE.version}`; });
}
