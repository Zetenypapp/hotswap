// Sounds, launching games, Inject and the running game's session.

/* ---------- Feedback: sound and the in-game overlay ---------- */
let audio;
// A short two-note chime, rising for on and falling for off. Made in code, no sound files.
function chime(on) {
  try {
    audio ||= new AudioContext();
    if (audio.state === 'suspended') audio.resume();
    const t = audio.currentTime;
    (on ? [660, 990] : [660, 440]).forEach((f, i) => {
      const o = audio.createOscillator(), gain = audio.createGain();
      o.type = 'sine'; o.frequency.value = f;
      gain.gain.setValueAtTime(0, t + i * 0.07);
      gain.gain.linearRampToValueAtTime(0.18, t + i * 0.07 + 0.01);
      gain.gain.exponentialRampToValueAtTime(0.001, t + i * 0.07 + 0.16);
      o.connect(gain).connect(audio.destination);
      o.start(t + i * 0.07); o.stop(t + i * 0.07 + 0.18);
    });
  } catch {}
}
// `hotkey` is true for changes made from the game (hotkeys, panic key): those show on the overlay.
function feedback(g, title, value, on, hotkey, pct) {
  if (state.sounds) chime(on);
  if (state.overlay && hotkey && invoke && isLive(g)) overlaySend({ title, value, on, game: g.name, pct });
}

function play(g) {
  if (!invoke) { state.playing = state.playing === g.id ? null : g.id; save(); render(); return; }
  clearTimeout(launching?.timer);
  launching = {
    id: g.id,
    timer: setTimeout(() => {
      launching = null; render();
      toast(`${g.name} didn't start`, 'is Steam running?');
    }, 60000),
  };
  invoke('launch_game', { id: g.id }).catch(err => { clearTimeout(launching?.timer); launching = null; render(); toast("Couldn't launch", String(err)); });
  toast('Launching', g.name);
  render();
}

// Starts the session in the running game: mods, hotkeys, the overlay menu and the Discord timer.
function inject(g, quiet) {
  if (!g || RUNNING !== g.id) return;
  INJECTED = g.id;
  sessionStart = Date.now();
  logLine(g, 'Session started');
  if (!quiet) toast(g.name, 'injected');
  else toast(g.name, 'is running');
  save();
  render();
}

function onRunning(id) {
  const prev = RUNNING;
  if (id === prev) return;
  const now = Date.now();
  if (prev && sessionStart) {
    state.sessions.push({ g: prev, start: sessionStart, end: now });
    state.sessions = state.sessions.slice(-300);
    const g = findGame(prev);
    if (g) logLine(g, `Session ended (${Math.max(1, Math.round((now - sessionStart) / 60000))} min)`);
  }
  RUNNING = id;
  sessionStart = null;
  INJECTED = null;
  if (id) {
    const g = findGame(id);
    // Started with HotSwap's Play button: injected right away.
    if (launching?.id === id) { clearTimeout(launching.timer); launching = null; if (g) { inject(g, true); return; } }
    if (g) toast(g.name, 'is running · press Inject on its page to use HotSwap');
  }
  save();
  render();
}
