// Discord: the Overlay page, Discord voice (who's talking, popups) and the status on your profile.

/* ---------- Discord overlay ---------- */
// Discord voice runs in the app (voice.rs) once the user connected it in the overlay; the choice
// is kept in localStorage (shared with the overlay page).
const dcVoiceOn = () => { try { return localStorage.getItem('hs.dc.voice') === '1'; } catch { return false; } };
if (invoke && dcVoiceOn()) invoke('voice_start').catch(() => {});
// Whether Discord popups may show now: always, only while a game runs, or never.
function dcPopupsNow() { return state.dcPopups === 'never' ? false : state.dcPopups === 'games' ? !!RUNNING : true; }
var lastDcOptions = '';
function dcOptions() {
  const o = { notes: state.dcNotes !== false && dcPopupsNow(), privacy: state.dcPrivacy || 0, joins: state.dcJoins !== false && dcPopupsNow() };
  const sig = JSON.stringify(o);
  if (sig === lastDcOptions) return;
  lastDcOptions = sig;
  invoke?.('voice_options', o).catch(() => {});
};
dcOptions();
// Who's talking, shown over any game (unless the HUD already shows it).
var talkOn = false, lastTalk = '', VOICE = null;
setInterval(async () => {
  if (!invoke || !dcVoiceOn()) { VOICE = null; return; }
  VOICE = await invoke('voice_state').catch(() => null);
  if (state.view === 'overlay') renderOverlayPage();
  const talking = state.overlay && state.dcTalk && dcPopupsNow() && !hudOn && VOICE?.status === 'ready' ? VOICE.members.filter(m => m.speaking).map(m => ({ name: m.name, avatar: m.avatar })) : [];
  const sig = JSON.stringify(talking);
  if (sig === lastTalk) return;
  lastTalk = sig;
  talkOn = talking.length > 0;
  overlaySend({ kind: 'talk', members: talking });
}, 350);

document.addEventListener('click', e => {
  const c = e.target.closest('[data-dcprivacy],[data-dcpopups]');
  if (!c) return;
  if (c.dataset.dcpopups) state.dcPopups = c.dataset.dcpopups;
  else state.dcPrivacy = +c.dataset.dcprivacy;
  save(); dcOptions(); render();
});
function renderOverlayPage() {
  const sw = (id, on) => `<button class="switch" role="switch" id="${id}" aria-checked="${!!on}"></button>`;
  const v = VOICE, on = dcVoiceOn();
  const status = !on ? 'Not connected' : !v || v.status === 'connecting' ? 'Connecting…' : v.status === 'authorizing' ? 'Click Authorize in Discord' : v.status === 'error' ? (v.message || 'Not connected') : v.channel ? `In ${v.channel} · ${v.members.length} ${v.members.length === 1 ? 'person' : 'people'}` : 'Connected · not in a call';
  const html = `<h1>Overlay</h1>
    <div class="set-group"><h2>Discord overlay</h2>
      <div class="set-tip">A Discord menu over any game, even ones HotSwap has no mods for, like CS2. It’s its own window on top of the game and never touches the game itself, so anti-cheat has nothing to see. Run games in <b>borderless or windowed</b> mode.</div>
      <div class="set-row"><div class="txt"><b>Discord overlay in every game</b><span>The overlay key opens the Discord menu in games without HotSwap mods. Games with mods still get the full menu, with Discord as a tab.</span></div>${sw('setDcOverlay', state.dcOverlay)}</div>
      ${invoke ? `<div class="set-row"><div class="txt"><b>Overlay key</b><span>Works everywhere while the Discord overlay is on, so pick a key you don’t need for anything else.</span></div>
        <button class="btn" id="testMenu">Try it</button><button class="key bindable${capture?.mid === MENU ? ' listening' : ''}${state.menuKey ? '' : ' unbound'}" id="menuBind">${capture?.mid === MENU ? 'Press a key…' : esc(comboLabel(state.menuKey) || 'Not set')}</button></div>` : ''}
    </div>
    <div class="set-group"><h2>Discord voice (beta)</h2>
      <div class="set-row"><div class="txt"><b>${esc(status)}</b><span>Who’s in your call, who’s talking, real mute and deafen, volumes and notifications. Connects as Discord’s own StreamKit overlay, so Discord asks once to authorize “StreamKit”. Unofficial: it may stop working if Discord changes it.</span></div>
        <button class="btn${on ? '' : ' accent'}" id="dcVoiceBtn">${on ? 'Turn off' : 'Connect'}</button></div>
      <div class="set-row"><div class="txt"><b>Show who’s talking</b><span>Avatars of the people speaking, in the HUD corner over any game.</span></div>${sw('setDcTalk', state.dcTalk)}</div>
      <div class="set-row"><div class="txt"><b>Discord popups</b><span>When notifications, join and leave popups and who’s talking may show over your screen.</span></div>
        <div class="chips">${[['always', 'Always'], ['games', 'Only in games'], ['never', 'Never']].map(([k, l]) => `<button class="chip${(state.dcPopups || 'always') === k ? ' on' : ''}" data-dcpopups="${k}">${l}</button>`).join('')}</div></div>
      <div class="set-row"><div class="txt"><b>Discord notifications</b><span>Messages and pings pop up over the game.</span></div>${sw('setDcNotes', state.dcNotes)}</div>
      <div class="set-row"><div class="txt"><b>Notification privacy</b><span>For streaming or screen sharing. <b>Kind only</b> shows “Private message”, “Group message” or “Server message”; <b>Hidden</b> only says a message came.</span></div>
        <div class="chips">${[['0', 'Off'], ['1', 'Kind only'], ['2', 'Hidden']].map(([k, l]) => `<button class="chip${String(state.dcPrivacy || 0) === k ? ' on' : ''}" data-dcprivacy="${k}">${l}</button>`).join('')}</div></div>
      <div class="set-row"><div class="txt"><b>Join and leave popups</b><span>A popup when someone joins or leaves your call.</span></div>${sw('setDcJoins', state.dcJoins)}</div>
    </div>`;
  if ($('ovBody').innerHTML !== html) $('ovBody').innerHTML = html;
}

// Discord: what you're playing with HotSwap, while a game runs.
let lastDiscord = '';
function syncDiscord() {
  if (!invoke) return;
  const g = findGame(invoke ? RUNNING : state.playing);
  const on = state.discord && g;
  const injected = g && runningId() === g.id;
  const n = g ? activeCount(g) : 0;
  const activity = on ? {
    details: `Playing ${g.name}`,
    state: injected && modsOf(g).length ? (state.paused[g.id] ? 'Mods paused' : `${n} mod${n === 1 ? '' : 's'} on`) : 'With HotSwap',
    timestamps: injected && sessionStart ? { start: Math.floor(sessionStart / 1000) } : undefined,
    assets: { large_image: 'hotswap_avatar', large_text: 'HotSwap: free, open-source game mods' },
    buttons: [{ label: 'Get HotSwap', url: REPO_URL }],
  } : null;
  const sig = JSON.stringify(activity);
  if (sig === lastDiscord) return;
  lastDiscord = sig;
  invoke('discord_presence', { activity }).then(() => { DISCORD_ERR = ''; }).catch(e => { DISCORD_ERR = String(e); });
}
