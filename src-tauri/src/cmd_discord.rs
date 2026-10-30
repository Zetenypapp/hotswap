//! Commands for Discord: the status on your profile and Discord voice (see discord.rs, voice.rs).

use super::*;

/// Discord voice (beta, see voice.rs): connect, disconnect, the current call, and mute or deafen.
#[tauri::command]
pub(crate) fn voice_start(app: AppHandle, voice: State<voice::Voice>) {
    voice.start(app);
}
#[tauri::command]
pub(crate) fn voice_options(notes: bool, privacy: u8, joins: bool) {
    voice::NOTES.store(notes, Ordering::Relaxed);
    voice::PRIVACY.store(privacy.min(2), Ordering::Relaxed);
    voice::JOINS.store(joins, Ordering::Relaxed);
}
#[tauri::command]
pub(crate) fn voice_stop(voice: State<voice::Voice>) {
    voice.stop();
}
#[tauri::command]
pub(crate) fn voice_state(voice: State<voice::Voice>) -> voice::State {
    voice.state()
}
#[tauri::command]
pub(crate) fn voice_send(voice: State<voice::Voice>, cmd: String, args: serde_json::Value) -> Result<(), String> {
    voice.send(cmd, args)
}

/// Sets (or with None clears) what Discord shows you playing.
#[tauri::command]
pub(crate) fn discord_presence(discord: State<discord::Discord>, activity: Option<serde_json::Value>) -> Result<(), String> {
    discord.set(activity)
}
