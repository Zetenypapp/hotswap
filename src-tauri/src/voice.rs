//! Discord voice (beta): who's in your call, who's talking, and your real mute and deafen, for the
//! overlay. Discord only gives voice access to apps it approved, so this connects as Discord's own
//! StreamKit overlay (the same way open-source overlays like Discover do): the Discord app on this
//! PC asks once to authorize "StreamKit", and its token is kept in HotSwap's settings folder.
//! Unofficial, so it can stop working whenever Discord changes it; nothing else depends on it.

use serde::Serialize;
use serde_json::{json, Value};
use std::{
    net::TcpStream,
    sync::{mpsc, Arc, Mutex},
    time::{Duration, Instant},
};
use tungstenite::{client::IntoClientRequest, stream::MaybeTlsStream, Message, WebSocket};

const STREAMKIT: &str = "207646673902501888";

#[derive(Clone, Default, Serialize)]
pub struct Member {
    id: String,
    name: String,
    avatar: String,
    speaking: bool,
    mute: bool,
    deaf: bool,
    /// Your own settings for this person: muted for you, and volume (0..200, 100 is normal).
    local_mute: bool,
    volume: f64,
}

#[derive(Clone, Default, Serialize)]
pub struct Channel {
    id: String,
    name: String,
}

#[derive(Clone, Default, Serialize)]
pub struct State {
    /// off, connecting, authorizing, ready, or error.
    status: String,
    message: String,
    me: String,
    channel: String,
    members: Vec<Member>,
    mute: bool,
    deaf: bool,
    channel_id: String,
    /// Voice channels of the server you're in.
    channels: Vec<Channel>,
    /// Mic volume (0..100), headphone volume (0..200), noise suppression.
    input_volume: f64,
    output_volume: f64,
    noise: bool,
}

/// Commands the overlay may send to Discord, with their arguments.
const ALLOWED: [&str; 4] = ["SET_VOICE_SETTINGS", "SET_USER_VOICE_SETTINGS", "SELECT_VOICE_CHANNEL", "GET_CHANNELS"];

enum Cmd {
    Send(String, Value),
    Stop,
}

#[derive(Default)]
pub struct Voice {
    state: Arc<Mutex<State>>,
    tx: Mutex<Option<mpsc::Sender<Cmd>>>,
}

impl Voice {
    pub fn start(&self, app: tauri::AppHandle) {
        let mut tx = self.tx.lock().unwrap();
        if tx.is_some() {
            return;
        }
        let (t, rx) = mpsc::channel();
        *tx = Some(t);
        let state = self.state.clone();
        std::thread::spawn(move || run(app, state, rx));
    }

    pub fn stop(&self) {
        if let Some(t) = self.tx.lock().unwrap().take() {
            let _ = t.send(Cmd::Stop);
        }
        *self.state.lock().unwrap() = State { status: "off".into(), ..State::default() };
    }

    pub fn state(&self) -> State {
        let s = self.state.lock().unwrap().clone();
        if s.status.is_empty() { State { status: "off".into(), ..s } } else { s }
    }

    /// Sends one of the ALLOWED commands to Discord (mute, volumes, channels, ...).
    pub fn send(&self, cmd: String, args: Value) -> Result<(), String> {
        if !ALLOWED.contains(&cmd.as_str()) {
            return Err(format!("{cmd} isn't allowed"));
        }
        let tx = self.tx.lock().unwrap();
        tx.as_ref().ok_or("Discord voice isn't connected")?.send(Cmd::Send(cmd, args)).map_err(|_| "Discord voice stopped".to_string())
    }
}

fn token_path() -> Option<std::path::PathBuf> {
    Some(std::path::Path::new(&std::env::var("APPDATA").ok()?).join("app.hotswap.desktop").join("discord_voice_token"))
}

type Ws = WebSocket<MaybeTlsStream<TcpStream>>;

fn connect() -> Option<Ws> {
    for port in 6463..=6472 {
        let Ok(mut req) = format!("ws://127.0.0.1:{port}/?v=1&client_id={STREAMKIT}").into_client_request() else { continue };
        req.headers_mut().insert("Origin", "https://streamkit.discord.com".parse().unwrap());
        let Ok(tcp) = TcpStream::connect_timeout(&format!("127.0.0.1:{port}").parse().unwrap(), Duration::from_millis(300)) else { continue };
        if let Ok((ws, _)) = tungstenite::client(req, MaybeTlsStream::Plain(tcp)) {
            return Some(ws);
        }
    }
    None
}

fn set_status(state: &Mutex<State>, status: &str, message: &str) {
    crate::trainer::log(&format!("voice: {status} {message}"));
    let mut s = state.lock().unwrap();
    s.status = status.into();
    s.message = message.into();
}

fn run(app: tauri::AppHandle, state: Arc<Mutex<State>>, rx: mpsc::Receiver<Cmd>) {
    loop {
        set_status(&state, "connecting", "Looking for the Discord app…");
        if let Some(ws) = connect() {
            let mut s = Session { ws, app: app.clone(), state: state.clone(), nonce: 0, channel: None, guild: None, pending: Default::default() };
            match s.serve(&rx) {
                Ok(()) => return, // stopped
                Err(e) => set_status(&state, "error", &e),
            }
        } else {
            set_status(&state, "error", "Discord isn't running");
        }
        // Wait a bit before trying again, unless told to stop.
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Cmd::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => return,
            _ => {}
        }
    }
}

struct Session {
    ws: Ws,
    app: tauri::AppHandle,
    state: Arc<Mutex<State>>,
    nonce: u64,
    channel: Option<String>,
    guild: Option<String>,
    /// Nonce -> what the reply is for.
    pending: std::collections::HashMap<String, &'static str>,
}

impl Session {
    fn send(&mut self, cmd: &str, args: Value, evt: Option<&str>, purpose: &'static str) -> Result<(), String> {
        self.nonce += 1;
        let nonce = self.nonce.to_string();
        let mut msg = json!({ "cmd": cmd, "args": args, "nonce": nonce });
        if let Some(e) = evt {
            msg["evt"] = json!(e);
        }
        self.pending.insert(nonce, purpose);
        self.ws.send(Message::text(msg.to_string())).map_err(|e| format!("Lost Discord: {e}"))
    }

    fn serve(&mut self, rx: &mpsc::Receiver<Cmd>) -> Result<(), String> {
        if let MaybeTlsStream::Plain(t) = self.ws.get_ref() {
            let _ = t.set_read_timeout(Some(Duration::from_millis(100)));
        }
        let mut last_poll = Instant::now();
        loop {
            match rx.try_recv() {
                Ok(Cmd::Stop) | Err(mpsc::TryRecvError::Disconnected) => {
                    let _ = self.ws.close(None);
                    return Ok(());
                }
                Ok(Cmd::Send(cmd, args)) => self.send(&cmd, args, None, "")?,
                Err(mpsc::TryRecvError::Empty) => {}
            }
            // A slow safety net in case an event was missed.
            if self.channel.is_some() && last_poll.elapsed() > Duration::from_secs(15) {
                last_poll = Instant::now();
                self.send("GET_SELECTED_VOICE_CHANNEL", json!({}), None, "channel")?;
            }
            match self.ws.read() {
                Ok(Message::Text(t)) => {
                    if let Ok(v) = serde_json::from_str::<Value>(&t) {
                        self.handle(v)?;
                    }
                }
                Ok(Message::Close(_)) => return Err("Discord closed the connection".into()),
                Ok(_) => {}
                Err(tungstenite::Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
                Err(e) => return Err(format!("Lost Discord: {e}")),
            }
        }
    }

    fn handle(&mut self, v: Value) -> Result<(), String> {
        let cmd = v["cmd"].as_str().unwrap_or("");
        let evt = v["evt"].as_str().unwrap_or("");
        let data = &v["data"];
        let purpose = v["nonce"].as_str().and_then(|n| self.pending.remove(n)).unwrap_or("");
        if evt == "ERROR" {
            let msg = data["message"].as_str().unwrap_or("Discord said no").to_string();
            crate::trainer::log(&format!("voice: {cmd} refused: {msg}"));
            return match purpose {
                "auth" => {
                    // Saved token expired or was revoked: ask again.
                    if let Some(p) = token_path() {
                        let _ = std::fs::remove_file(p);
                    }
                    self.authorize()
                }
                "authorize" => Err(format!("Not authorized: {msg}")),
                _ => Ok(()),
            };
        }
        match (cmd, evt) {
            ("DISPATCH", "READY") => {
                self.state.lock().unwrap().me = data["user"]["id"].as_str().unwrap_or("").to_string();
                match token_path().and_then(|p| std::fs::read_to_string(p).ok()) {
                    Some(t) if !t.trim().is_empty() => self.send("AUTHENTICATE", json!({ "access_token": t.trim() }), None, "auth")?,
                    _ => self.authorize()?,
                }
            }
            ("AUTHORIZE", _) => {
                let code = data["code"].as_str().ok_or("Discord sent no code")?;
                let resp: Value = ureq::post("https://streamkit.discord.com/overlay/token")
                    .send_json(json!({ "code": code }))
                    .map_err(|e| format!("Token request failed: {e}"))?
                    .into_json()
                    .map_err(|e| e.to_string())?;
                let token = resp["access_token"].as_str().ok_or("No token from StreamKit")?.to_string();
                if let Some(p) = token_path() {
                    let _ = std::fs::write(p, &token);
                }
                self.send("AUTHENTICATE", json!({ "access_token": token }), None, "auth")?;
            }
            ("AUTHENTICATE", _) => {
                // You (this connection's READY carries no user).
                if let Some(id) = data["user"]["id"].as_str() {
                    self.state.lock().unwrap().me = id.to_string();
                }
                set_status(&self.state, "ready", "");
                self.send("SUBSCRIBE", json!({}), Some("VOICE_CHANNEL_SELECT"), "")?;
                self.send("SUBSCRIBE", json!({}), Some("VOICE_SETTINGS_UPDATE"), "")?;
                self.send("SUBSCRIBE", json!({}), Some("NOTIFICATION_CREATE"), "")?;
                self.send("GET_VOICE_SETTINGS", json!({}), None, "settings")?;
                self.send("GET_SELECTED_VOICE_CHANNEL", json!({}), None, "channel")?;
            }
            ("GET_VOICE_SETTINGS", _) | ("SET_VOICE_SETTINGS", _) | ("DISPATCH", "VOICE_SETTINGS_UPDATE") => {
                let mut s = self.state.lock().unwrap();
                if let Some(m) = data["mute"].as_bool() {
                    s.mute = m;
                }
                if let Some(d) = data["deaf"].as_bool() {
                    s.deaf = d;
                }
                if let Some(v) = data["input"]["volume"].as_f64() {
                    s.input_volume = v;
                }
                if let Some(v) = data["output"]["volume"].as_f64() {
                    s.output_volume = v;
                }
                if let Some(n) = data["noise_suppression"].as_bool() {
                    s.noise = n;
                }
            }
            ("SET_USER_VOICE_SETTINGS", _) => {
                let id = data["user_id"].as_str().unwrap_or("");
                let mut s = self.state.lock().unwrap();
                if let Some(x) = s.members.iter_mut().find(|x| x.id == id) {
                    if let Some(v) = data["volume"].as_f64() {
                        x.volume = v;
                    }
                    if let Some(m) = data["mute"].as_bool() {
                        x.local_mute = m;
                    }
                }
            }
            ("GET_CHANNELS", _) => {
                let list = data["channels"].as_array().map(|a| {
                    a.iter()
                        .filter(|c| matches!(c["type"].as_i64(), Some(2 | 13)))
                        .map(|c| Channel { id: c["id"].as_str().unwrap_or("").into(), name: c["name"].as_str().unwrap_or("").into() })
                        .collect()
                });
                self.state.lock().unwrap().channels = list.unwrap_or_default();
            }
            ("DISPATCH", "NOTIFICATION_CREATE") => {
                // A Discord notification, shown as an overlay toast.
                let note = json!({ "kind": "discord", "title": data["title"], "body": data["body"], "icon": data["icon_url"] });
                let app = self.app.clone();
                tauri::async_runtime::spawn(async move {
                    let _ = crate::overlay_notify(app, note).await;
                });
            }
            ("GET_SELECTED_VOICE_CHANNEL", _) => self.channel(data)?,
            ("DISPATCH", "VOICE_CHANNEL_SELECT") => {
                self.send("GET_SELECTED_VOICE_CHANNEL", json!({}), None, "channel")?;
            }
            ("DISPATCH", "VOICE_STATE_CREATE" | "VOICE_STATE_UPDATE") => {
                let m = member(data);
                let mut s = self.state.lock().unwrap();
                match s.members.iter_mut().find(|x| x.id == m.id) {
                    Some(x) => *x = Member { speaking: x.speaking, ..m },
                    None => s.members.push(m),
                }
            }
            ("DISPATCH", "VOICE_STATE_DELETE") => {
                let id = data["user"]["id"].as_str().unwrap_or("").to_string();
                self.state.lock().unwrap().members.retain(|x| x.id != id);
            }
            ("DISPATCH", "SPEAKING_START" | "SPEAKING_STOP") => {
                let id = data["user_id"].as_str().unwrap_or("");
                let mut s = self.state.lock().unwrap();
                if let Some(x) = s.members.iter_mut().find(|x| x.id == id) {
                    x.speaking = evt == "SPEAKING_START";
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn authorize(&mut self) -> Result<(), String> {
        set_status(&self.state, "authorizing", "Click Authorize in Discord");
        self.send("AUTHORIZE", json!({ "client_id": STREAMKIT, "scopes": ["rpc", "messages.read", "rpc.notifications.read"] }), None, "authorize")
    }

    /// The selected voice channel (or none): its members, and voice events for it.
    fn channel(&mut self, data: &Value) -> Result<(), String> {
        let id = data["id"].as_str().map(str::to_string);
        if id != self.channel {
            for e in ["VOICE_STATE_CREATE", "VOICE_STATE_UPDATE", "VOICE_STATE_DELETE", "SPEAKING_START", "SPEAKING_STOP"] {
                if let Some(old) = self.channel.clone() {
                    self.send("UNSUBSCRIBE", json!({ "channel_id": old }), Some(e), "")?;
                }
                if let Some(new) = id.clone() {
                    self.send("SUBSCRIBE", json!({ "channel_id": new }), Some(e), "")?;
                }
            }
            self.channel = id.clone();
        }
        // The server's voice channels, for switching.
        let guild = data["guild_id"].as_str().map(str::to_string);
        if guild != self.guild {
            self.guild = guild.clone();
            match &guild {
                Some(g) => self.send("GET_CHANNELS", json!({ "guild_id": g }), None, "")?,
                None => self.state.lock().unwrap().channels.clear(),
            }
        }
        let mut s = self.state.lock().unwrap();
        let speaking: Vec<String> = s.members.iter().filter(|m| m.speaking).map(|m| m.id.clone()).collect();
        s.channel = data["name"].as_str().unwrap_or("").to_string();
        s.channel_id = id.unwrap_or_default();
        s.members = data["voice_states"].as_array().map(|a| a.iter().map(member).collect()).unwrap_or_default();
        for m in s.members.iter_mut() {
            m.speaking = speaking.contains(&m.id);
        }
        Ok(())
    }
}

fn member(v: &Value) -> Member {
    let u = &v["user"];
    let id = u["id"].as_str().unwrap_or("").to_string();
    let name = [v["nick"].as_str(), u["global_name"].as_str(), u["username"].as_str()].into_iter().flatten().find(|n| !n.is_empty()).unwrap_or("Someone").to_string();
    let avatar = match u["avatar"].as_str() {
        Some(h) => format!("https://cdn.discordapp.com/avatars/{id}/{h}.png?size=64"),
        None => "https://cdn.discordapp.com/embed/avatars/0.png".into(),
    };
    let vs = &v["voice_state"];
    let flag = |k: &str| vs[k].as_bool().unwrap_or(false);
    let mute = flag("mute") || flag("self_mute") || flag("suppress");
    Member { id, name, avatar, speaking: false, mute, deaf: flag("deaf") || flag("self_deaf"), local_mute: v["mute"].as_bool().unwrap_or(false), volume: v["volume"].as_f64().unwrap_or(100.0) }
}
