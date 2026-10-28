//! Discord Rich Presence: shows what you're playing with HotSwap on your Discord profile. Talks to the
//! Discord app on this PC through its local pipe (no SDK, no login, nothing sent anywhere else).
//! Needs the ID of HotSwap's application in Discord's developer portal.

use serde_json::{json, Value};
use std::{
    fs::File,
    io::{Read, Write},
    sync::{mpsc, Mutex},
    time::Duration,
};

/// HotSwap's application ID from discord.com/developers (empty: presence is off).
pub const APP_ID: &str = "";

/// Sends wanted activities to the presence thread (None clears it).
pub struct Discord(Mutex<Option<mpsc::Sender<Option<Value>>>>);

impl Default for Discord {
    fn default() -> Self {
        Discord(Mutex::new(None))
    }
}

impl Discord {
    pub fn set(&self, activity: Option<Value>) -> Result<(), String> {
        if APP_ID.is_empty() {
            return Err("Discord isn't set up in this build".into());
        }
        let mut tx = self.0.lock().unwrap();
        if tx.is_none() {
            let (s, r) = mpsc::channel();
            std::thread::spawn(move || run(r));
            *tx = Some(s);
        }
        tx.as_ref().unwrap().send(activity).map_err(|e| e.to_string())
    }
}

fn frame(pipe: &mut File, op: u32, body: &Value) -> std::io::Result<Value> {
    let data = body.to_string();
    let mut msg = Vec::with_capacity(8 + data.len());
    msg.extend_from_slice(&op.to_le_bytes());
    msg.extend_from_slice(&(data.len() as u32).to_le_bytes());
    msg.extend_from_slice(data.as_bytes());
    pipe.write_all(&msg)?;
    // Every request gets one reply: an 8-byte header (op, length), then JSON.
    let mut head = [0u8; 8];
    pipe.read_exact(&mut head)?;
    let len = u32::from_le_bytes(head[4..8].try_into().unwrap()) as usize;
    let mut reply = vec![0u8; len.min(1 << 20)];
    pipe.read_exact(&mut reply)?;
    Ok(serde_json::from_slice(&reply).unwrap_or(Value::Null))
}

fn connect() -> Option<File> {
    (0..10).find_map(|i| {
        let mut pipe = std::fs::OpenOptions::new().read(true).write(true).open(format!(r"\\.\pipe\discord-ipc-{i}")).ok()?;
        let ready = frame(&mut pipe, 0, &json!({ "v": 1, "client_id": APP_ID })).ok()?;
        (ready["evt"] == "READY").then_some(pipe)
    })
}

fn run(rx: mpsc::Receiver<Option<Value>>) {
    let mut want: Option<Value> = None;
    let mut pipe: Option<File> = None;
    let mut dirty = false;
    let mut nonce = 0u64;
    loop {
        // Wake for a new activity, or now and then to reconnect if Discord was closed.
        match rx.recv_timeout(Duration::from_secs(20)) {
            Ok(a) => {
                want = a;
                while let Ok(a) = rx.try_recv() {
                    want = a;
                }
                dirty = true;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        }
        if pipe.is_none() {
            if want.is_none() {
                dirty = false;
                continue;
            }
            pipe = connect();
            dirty = pipe.is_some();
        }
        if !dirty {
            continue;
        }
        if let Some(p) = pipe.as_mut() {
            nonce += 1;
            let msg = json!({ "cmd": "SET_ACTIVITY", "args": { "pid": std::process::id(), "activity": want }, "nonce": nonce.to_string() });
            if frame(p, 1, &msg).is_ok() {
                dirty = false;
            } else {
                pipe = None; // Discord closed: try again on the next wake
            }
        }
    }
}
