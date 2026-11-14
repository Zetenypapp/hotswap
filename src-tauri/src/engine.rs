//! hotswap-engine.exe: HotSwap's mod engine, the only part of HotSwap that reads or writes another
//! program's memory (the trainer and the game readers: trainer.rs, engines.rs, mono.rs, il2cpp.rs,
//! remote.rs, ue/ and the game files). It's a separate program so the app itself never contains
//! that code: if a security tool objects to the engine, only mods stop and the rest of HotSwap
//! keeps working.
//!
//! The app starts it when a game with mods is chosen (engine_host.rs) and talks to it over its
//! standard input and output, one JSON message per line. From the app: "set" (the game, its mods
//! and their values), "action", "teleport", "world". From the engine: "ev" (an event for the
//! interface: "trainer-status" or "radar"), "pos" (the player's position for the map) and
//! "reply" (the answer to "world"). The engine exits when the app closes its input.

use crate::{engines::Spec, trainer::Trainer, watch::Watch};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{BufRead, Write},
    path::PathBuf,
    time::Duration,
};

#[derive(Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
enum Msg {
    Set {
        game: Option<String>,
        paused: bool,
        values: HashMap<String, f64>,
        engine: Option<String>,
        specs: HashMap<String, Vec<Spec>>,
        guard: Vec<Spec>,
        /// The game's install folder, to find its process.
        install: Option<PathBuf>,
    },
    Action {
        id: String,
    },
    Teleport {
        to: [f64; 3],
    },
    World {
        id: u64,
        pid: u32,
    },
}

/// One message to the app. Whole lines only, so the app never reads half of one.
fn out(v: Value) {
    let mut o = std::io::stdout().lock();
    let _ = writeln!(o, "{v}");
    let _ = o.flush();
}

pub fn serve() {
    crate::logs::write("engine", "started");
    let trainer = Trainer::default();
    let watch = Watch::default();
    let (w1, w2) = (watch.clone(), watch.clone());
    trainer.start(|name, data| out(json!({ "t": "ev", "name": name, "data": data })), move |id| w1.pid_of(id), move |id| w2.install_of(id));
    // The player's position for the map, whenever it changes.
    let t = trainer.clone();
    std::thread::spawn(move || {
        let mut last = None;
        loop {
            let pos = t.position();
            if pos != last {
                out(json!({ "t": "pos", "pos": pos }));
                last = pos;
            }
            std::thread::sleep(Duration::from_millis(150));
        }
    });
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let msg = match serde_json::from_str::<Msg>(&line) {
            Ok(m) => m,
            Err(e) => {
                crate::logs::write("engine", &format!("bad message: {e}"));
                continue;
            }
        };
        match msg {
            Msg::Set { game, paused, values, engine, specs, guard, install } => {
                watch.set_targets(match (&game, install) {
                    (Some(g), Some(dir)) => vec![(g.clone(), dir)],
                    _ => vec![],
                });
                trainer.set(game, paused, values, engine, specs, guard);
            }
            Msg::Action { id } => trainer.action(id),
            Msg::Teleport { to } => trainer.teleport(to),
            Msg::World { id, pid } => {
                std::thread::spawn(move || {
                    let r = crate::ue::objects::Ue::attach(pid).map(|ue| crate::world::scan(&ue));
                    out(match r {
                        Ok(w) => json!({ "t": "reply", "id": id, "ok": w }),
                        Err(e) => json!({ "t": "reply", "id": id, "err": e }),
                    });
                });
            }
        }
    }
    // The app closed (or crashed): nothing left to do.
    crate::logs::write("engine", "app gone, stopping");
    std::process::exit(0);
}
