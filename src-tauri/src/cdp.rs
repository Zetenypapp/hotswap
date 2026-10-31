//! Games made with web tech: Electron and NW.js games, which includes every RPG Maker MV and MZ
//! game. HotSwap starts them with Chromium's debugging port open (see `launch_args`) and talks to
//! the page over the DevTools protocol: mods are plain JavaScript on the game's own variables,
//! e.g. `$gameParty._gold`, so they don't depend on a game version's memory layout at all.

use crate::engines::Spec;
use serde_json::{json, Value};
use std::{collections::HashMap, net::TcpStream, time::Duration};
use tungstenite::{stream::MaybeTlsStream, Message, WebSocket};

pub const PORT: u16 = 47830;

pub fn is_web(engine: &str) -> bool {
    matches!(engine, "electron" | "nwjs" | "rpgmaker-mv" | "rpgmaker-mz")
}

/// The game's real executable for a web engine: the biggest .exe next to the engine's files.
pub fn main_exe(install: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut dirs = vec![install.to_path_buf()];
    dirs.extend(std::fs::read_dir(install).ok()?.flatten().map(|e| e.path()).filter(|p| p.is_dir()));
    let dir = dirs.into_iter().find(|d| ["nw.dll", "LICENSE.electron.txt", "resources/app.asar", "resources/app"].iter().any(|f| d.join(f).exists()))?;
    std::fs::read_dir(&dir).ok()?.flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x.eq_ignore_ascii_case("exe")))
        .filter(|e| !["elevate.exe", "notification_helper.exe", "crashpad_handler.exe", "nwjc.exe"].contains(&e.file_name().to_string_lossy().to_lowercase().as_str()))
        .max_by_key(|e| e.metadata().map(|m| m.len()).unwrap_or(0))
        .map(|e| e.path())
}

/// Applies the mods in the page. Paths are JavaScript expressions; `[]` goes through every
/// element of an array (e.g. `$gameParty.members()[]._hp`). Same operations as engines.rs, plus
/// "js": code run when the mod turns on (`at`) and off (`from`), or once for actions.
const HELPER: &str = r#"
window.__hotswap = window.__hotswap || (() => {
  const geval = eval;
  const orig = {}, on = {};
  const walk = (o, path) => path.split('.').filter(Boolean).reduce((x, k) => x == null ? x : k.endsWith('()') ? x[k.slice(0, -2)]() : x[k], o);
  const targets = at => {
    const parts = at.split('[]');
    if (parts.length === 1) return [{ get: () => geval(at), set: v => geval(at + ' = ' + JSON.stringify(v)) }];
    let objs = Array.from(geval(parts[0]) || []);
    for (let i = 1; i < parts.length - 1; i++) objs = objs.flatMap(o => Array.from(walk(o, parts[i]) || []));
    const last = parts[parts.length - 1].split('.').filter(Boolean);
    if (!last.length) return [];
    const key = last.pop();
    return objs.map(o => walk(o, last.join('.'))).filter(o => o != null).map(o => ({ get: () => o[key], set: v => { o[key] = v; } }));
  };
  const read = (t, s) => { const v = t.get(); return typeof v === 'boolean' ? +v : v; };
  const write = (t, s, v) => { const cur = t.get(); if (typeof cur === 'boolean') v = !!v; if (cur !== v) t.set(v); };
  const apply = (id, specs, value) => specs.forEach((s, k) => {
    const slider = s.op === 'scale' || s.op === 'value';
    const active = slider ? value !== s.def : !!value;
    const key = id + '#' + k;
    if (s.op === 'js') {
      if (active && !on[key]) geval(s.at);
      if (!active && on[key] && s.from) geval(s.from);
      on[key] = active;
      return;
    }
    let ts; try { ts = targets(s.at); } catch { return; }
    let sib = [];
    if (s.op === 'fill' && s.from) try { sib = targets(s.at.replace(/[^.]+$/, s.from)); } catch { }
    ts.forEach((t, i) => {
      const tk = key + '#' + i;
      if (!active) {
        if (tk in orig) { if (['set', 'scale', 'value'].includes(s.op)) write(t, s, orig[tk]); delete orig[tk]; }
        return;
      }
      const cur = read(t, s);
      if (typeof cur !== 'number') return;
      if (!(tk in orig)) orig[tk] = cur;
      const want = { set: s.v, min: Math.max(cur, s.v), max: Math.min(cur, s.v), freeze: orig[tk], scale: orig[tk] * value / 100, value: value * s.mul,
        fill: sib[i] ? read(sib[i], s) : undefined }[s.op];
      if (typeof want === 'number' && !isNaN(want)) write(t, s, want);
    });
  });
  return {
    tick(mods, actions) {
      for (const m of mods) try { apply(m.id, m.specs, m.value); } catch (e) { }
      for (const a of actions) for (const s of a.specs) try {
        if (s.op === 'js') { geval(s.at); continue; }
        for (const t of targets(s.at)) { const cur = read(t, s); if (typeof cur === 'number') write(t, s, s.op === 'add' ? cur + s.v : s.v); }
      } catch (e) { }
      return true;
    },
  };
})();
"#;

pub struct Session {
    ws: WebSocket<MaybeTlsStream<TcpStream>>,
    next: u64,
}

impl Session {
    /// Connects to the game's page, if it was started with the debugging port.
    pub fn attach() -> Result<Session, String> {
        let list: Value = ureq::get(&format!("http://127.0.0.1:{PORT}/json/list"))
            .timeout(Duration::from_secs(2))
            .call()
            .map_err(|_| "Start the game from HotSwap (the Play button) so HotSwap can connect. Already running? Close it and press Play.".to_string())?
            .into_json()
            .map_err(|e| e.to_string())?;
        let url = list.as_array().into_iter().flatten()
            .find(|t| t["type"] == "page" && !t["url"].as_str().unwrap_or("").starts_with("devtools:"))
            .and_then(|t| t["webSocketDebuggerUrl"].as_str())
            .ok_or("the game's page isn't open yet")?;
        let (ws, _) = tungstenite::connect(url).map_err(|e| format!("couldn't connect to the game: {e}"))?;
        if let MaybeTlsStream::Plain(s) = ws.get_ref() {
            let _ = s.set_read_timeout(Some(Duration::from_secs(3)));
        }
        Ok(Session { ws, next: 1 })
    }

    /// Runs JavaScript in the page and returns its value.
    pub fn eval(&mut self, expr: &str) -> Result<Value, String> {
        let id = self.next;
        self.next += 1;
        let msg = json!({ "id": id, "method": "Runtime.evaluate", "params": { "expression": expr, "returnByValue": true } });
        self.ws.send(Message::text(msg.to_string())).map_err(|e| format!("lost the game: {e}"))?;
        loop {
            match self.ws.read().map_err(|e| format!("lost the game: {e}"))? {
                Message::Text(t) => {
                    let v: Value = serde_json::from_str(&t).unwrap_or_default();
                    if v["id"] == id {
                        if let Some(e) = v["result"]["exceptionDetails"]["exception"]["description"].as_str() {
                            return Err(e.lines().next().unwrap_or("").to_string());
                        }
                        return Ok(v["result"]["result"]["value"].clone());
                    }
                }
                Message::Close(_) => return Err("the game closed".into()),
                _ => {}
            }
        }
    }

    pub fn tick(&mut self, specs: &HashMap<String, Vec<Spec>>, values: Option<&HashMap<String, f64>>, actions: &[String]) -> Result<bool, String> {
        let is_action = |l: &Vec<Spec>| l.iter().any(|s| s.act);
        let mods: Vec<Value> = specs.iter().filter(|(_, l)| !is_action(l)).map(|(id, l)| {
            let off = l.first().map_or(0.0, |s| if matches!(s.op.as_str(), "scale" | "value") { s.def } else { 0.0 });
            json!({ "id": id, "specs": l, "value": values.and_then(|v| v.get(id)).copied().unwrap_or(off) })
        }).collect();
        let acts: Vec<Value> = actions.iter().filter_map(|id| specs.get(id)).map(|l| json!({ "specs": l })).collect();
        let call = format!("{HELPER};window.__hotswap.tick({}, {})", Value::from(mods), Value::from(acts));
        self.eval(&call).map(|v| v.as_bool().unwrap_or(false))
    }

    pub fn restore(&mut self, specs: &HashMap<String, Vec<Spec>>) {
        let _ = self.tick(specs, None, &[]);
    }
}
