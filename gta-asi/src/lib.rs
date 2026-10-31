//! HotSwap's plugin for GTA V story mode (Legacy and Enhanced). Script Hook V loads it into the
//! game; every frame it applies the mods you turn on in HotSwap, using the game's own script
//! commands ("natives"), which keep working after game updates. It asks HotSwap for the mods over
//! local UDP (see src-tauri/src/bridge.rs), and does nothing at all in an online session.

use serde_json::Value;
use std::{
    collections::HashMap,
    ffi::{c_void, CString},
    net::UdpSocket,
    sync::OnceLock,
    time::{Duration, Instant},
};

const PORT: u16 = 47831;

type ScriptFn = extern "C" fn();

#[link(name = "kernel32")]
extern "system" {
    fn GetModuleHandleA(name: *const u8) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
}

/// Script Hook V's exports (C++ names).
struct Shv {
    register: extern "C" fn(*mut c_void, ScriptFn),
    unregister: extern "C" fn(*mut c_void),
    wait: extern "C" fn(u32),
    init: extern "C" fn(u64),
    push: extern "C" fn(u64),
    call: extern "C" fn() -> *mut u64,
}

fn shv() -> Option<&'static Shv> {
    static S: OnceLock<Option<Shv>> = OnceLock::new();
    S.get_or_init(|| unsafe {
        let m = GetModuleHandleA(b"ScriptHookV.dll\0".as_ptr());
        if m.is_null() {
            return None;
        }
        let f = |name: &[u8]| {
            let p = GetProcAddress(m, name.as_ptr());
            (!p.is_null()).then_some(p)
        };
        Some(Shv {
            register: std::mem::transmute(f(b"?scriptRegister@@YAXPEAUHINSTANCE__@@P6AXXZ@Z\0")?),
            unregister: std::mem::transmute(f(b"?scriptUnregister@@YAXPEAUHINSTANCE__@@@Z\0")?),
            wait: std::mem::transmute(f(b"?scriptWait@@YAXK@Z\0")?),
            init: std::mem::transmute(f(b"?nativeInit@@YAX_K@Z\0")?),
            push: std::mem::transmute(f(b"?nativePush64@@YAX_K@Z\0")?),
            call: std::mem::transmute(f(b"?nativeCall@@YAPEA_KXZ\0")?),
        })
    })
    .as_ref()
}

#[no_mangle]
pub extern "system" fn DllMain(module: *mut c_void, reason: u32, _: *mut c_void) -> i32 {
    if let Some(s) = shv() {
        match reason {
            1 => (s.register)(module, script_main),
            0 => (s.unregister)(module),
            _ => {}
        }
    }
    1
}

// Natives used, by hash (stable across game versions; Script Hook V maps them).
const PLAYER_ID: u64 = 0x4F8644AF03D0E0D6;
const PLAYER_PED_ID: u64 = 0xD80958FC74E988A6;
const NETWORK_IS_SESSION_STARTED: u64 = 0x9DE624D2FC4B603F;
const SET_PLAYER_INVINCIBLE: u64 = 0x239528EACDC3E7DE;
const CLEAR_PLAYER_WANTED_LEVEL: u64 = 0xB302540597885499;
const SET_MAX_WANTED_LEVEL: u64 = 0xAA5F02DB48D704B9;
const SET_PED_INFINITE_AMMO_CLIP: u64 = 0x183DADC6AA953186;
const SET_SUPER_JUMP_THIS_FRAME: u64 = 0x57FFF03E423A4C0B;
const SET_RUN_SPRINT_MULTIPLIER_FOR_PLAYER: u64 = 0x6DB47AA77FD94E09;
const SET_SWIM_MULTIPLIER_FOR_PLAYER: u64 = 0xA91C6F0FF7D16A13;
const IS_PED_IN_ANY_VEHICLE: u64 = 0x997ABD671D25CA0B;
const GET_VEHICLE_PED_IS_IN: u64 = 0x9A9112A0FE9A4713;
const SET_ENTITY_INVINCIBLE: u64 = 0x3882114BDE571AD4;
const SET_VEHICLE_CAN_BE_VISIBLY_DAMAGED: u64 = 0x4C7028F78FFD3681;
const SET_VEHICLE_FIXED: u64 = 0x115722B1B9C14C1C;
const SET_VEHICLE_DIRT_LEVEL: u64 = 0x79D3B596FE44EE8B;
const SET_VEHICLE_ON_GROUND_PROPERLY: u64 = 0x49733E92263139D1;
const STAT_GET_INT: u64 = 0x767FBC2AC802EF3D;
const STAT_SET_INT: u64 = 0xB3271D7AB655B441;
const GET_ENTITY_MODEL: u64 = 0x9F47B058362C84B5;
const GET_ENTITY_COORDS: u64 = 0x3FEF770D40960D5A;
const GET_ENTITY_HEADING: u64 = 0xE83D4F9BA2A38914;
const SET_ENTITY_HEALTH: u64 = 0x6B76DC1F3AE6E6A3;
const GET_ENTITY_MAX_HEALTH: u64 = 0x15D757606D170C3C;
const SET_PED_ARMOUR: u64 = 0xCEA04D83135264CC;
const GET_FIRST_BLIP_INFO_ID: u64 = 0x1BEDE233E6CD2A1F;
const DOES_BLIP_EXIST: u64 = 0xA6DB27D19ECBB7DA;
const GET_BLIP_INFO_ID_COORD: u64 = 0xFA7C7F0AADF25D09;
const SET_ENTITY_COORDS_NO_OFFSET: u64 = 0x239A3351AC1DA385;
const GET_GROUND_Z_FOR_3D_COORD: u64 = 0xC906A7DAB05C8D2B;
const REQUEST_MODEL: u64 = 0x963D27A58DF860AC;
const HAS_MODEL_LOADED: u64 = 0x98A4EB5D89A0C952;
const CREATE_VEHICLE: u64 = 0xAF35D0D2583051B0;
const SET_PED_INTO_VEHICLE: u64 = 0xF75B0D629E1C063D;
const SET_MODEL_AS_NO_LONGER_NEEDED: u64 = 0xE532F5D78798DAAB;
const SET_CLOCK_TIME: u64 = 0x47C3B5848C3E45D8;
const SET_WEATHER_TYPE_NOW_PERSIST: u64 = 0xED712CA327900C8A;

fn native(hash: u64, args: &[u64]) -> u64 {
    let Some(s) = shv() else { return 0 };
    (s.init)(hash);
    for &a in args {
        (s.push)(a);
    }
    let r = (s.call)();
    if r.is_null() { 0 } else { unsafe { *r } }
}

/// A native that returns a Vector3 (x, y, z, each in an 8-byte slot).
fn native_v3(hash: u64, args: &[u64]) -> [f32; 3] {
    let Some(s) = shv() else { return [0.0; 3] };
    (s.init)(hash);
    for &a in args {
        (s.push)(a);
    }
    let r = (s.call)() as *const f32;
    if r.is_null() { [0.0; 3] } else { unsafe { [*r, *r.add(2), *r.add(4)] } }
}

fn f(v: f32) -> u64 {
    v.to_bits() as u64
}

fn wait(ms: u32) {
    if let Some(s) = shv() {
        (s.wait)(ms);
    }
}

/// Rockstar's name hash (Jenkins one-at-a-time, lowercase).
fn joaat(s: &str) -> u32 {
    let mut h: u32 = 0;
    for c in s.bytes() {
        h = h.wrapping_add(c.to_ascii_lowercase() as u32);
        h = h.wrapping_add(h << 10);
        h ^= h >> 6;
    }
    h = h.wrapping_add(h << 3);
    h ^= h >> 11;
    h.wrapping_add(h << 15)
}

#[derive(Default)]
struct State {
    /// Toggles that are on (by key), and the ones that were on last frame.
    on: HashMap<String, bool>,
    was: HashMap<String, bool>,
    run: Option<Value>,
    /// Action presses already done this run, by mod id.
    done: HashMap<String, u64>,
    last_vehicle: u64,
}

extern "C" fn script_main() {
    // Connected: only HotSwap's port can answer.
    let sock = UdpSocket::bind(("127.0.0.1", 0)).ok().filter(|s| s.connect(("127.0.0.1", PORT)).is_ok() && s.set_nonblocking(true).is_ok());
    let mut st = State::default();
    let (mut last_ask, mut last_answer) = (Instant::now() - Duration::from_secs(10), None::<Instant>);
    loop {
        let online = native(NETWORK_IS_SESSION_STARTED, &[]) & 0xFF != 0;
        if let Some(s) = &sock {
            if last_ask.elapsed() > Duration::from_millis(500) {
                last_ask = Instant::now();
                let _ = s.send(b"hs");
            }
            let mut buf = vec![0u8; 65536];
            while let Ok(n) = s.recv(&mut buf) {
                if let Ok(msg) = serde_json::from_slice::<Value>(&buf[..n]) {
                    last_answer = Some(Instant::now());
                    if !online {
                        read_message(&mut st, &msg);
                    }
                }
            }
        }
        // HotSwap closed, or an online session: everything off.
        if online || last_answer.is_none_or(|t| t.elapsed() > Duration::from_secs(5)) {
            st.on.clear();
        }
        frame(&mut st);
        wait(0);
    }
}

/// Takes the toggles and new action presses from HotSwap's message.
fn read_message(st: &mut State, msg: &Value) {
    if msg.get("run") != st.run.as_ref() {
        st.run = msg.get("run").cloned();
        st.done.clear();
    }
    st.on.clear();
    for m in msg["mods"].as_array().into_iter().flatten() {
        let on = m["value"].as_f64().unwrap_or(0.0) != 0.0;
        for s in m["specs"].as_array().into_iter().flatten() {
            if let Some(key) = s["at"].as_str() {
                st.on.insert(key.to_string(), on);
            }
        }
    }
    for a in msg["actions"].as_array().into_iter().flatten() {
        let id = a["id"].as_str().unwrap_or("").to_string();
        let n = a["n"].as_u64().unwrap_or(0);
        let before = st.done.insert(id, n).unwrap_or(0);
        for _ in before..n {
            for s in a["specs"].as_array().into_iter().flatten() {
                if let Some(key) = s["at"].as_str() {
                    action(key);
                }
            }
        }
    }
}

/// Applies the toggles for this frame, and undoes the ones just turned off.
fn frame(st: &mut State) {
    let player = native(PLAYER_ID, &[]);
    let ped = native(PLAYER_PED_ID, &[]);
    let on = |k: &str| st.on.get(k).copied().unwrap_or(false);
    let vehicle = if native(IS_PED_IN_ANY_VEHICLE, &[ped, 0]) & 0xFF != 0 { native(GET_VEHICLE_PED_IS_IN, &[ped, 0]) } else { 0 };

    if on("god") {
        native(SET_PLAYER_INVINCIBLE, &[player, 1]);
    }
    if on("never-wanted") {
        native(CLEAR_PLAYER_WANTED_LEVEL, &[player]);
        native(SET_MAX_WANTED_LEVEL, &[0]);
    }
    if on("ammo") {
        native(SET_PED_INFINITE_AMMO_CLIP, &[ped, 1]);
    }
    if on("super-jump") {
        native(SET_SUPER_JUMP_THIS_FRAME, &[player]);
    }
    if on("fast-run") {
        native(SET_RUN_SPRINT_MULTIPLIER_FOR_PLAYER, &[player, f(1.49)]);
    }
    if on("fast-swim") {
        native(SET_SWIM_MULTIPLIER_FOR_PLAYER, &[player, f(1.49)]);
    }
    if on("vehicle-god") && vehicle != 0 {
        native(SET_ENTITY_INVINCIBLE, &[vehicle, 1]);
        native(SET_VEHICLE_CAN_BE_VISIBLY_DAMAGED, &[vehicle, 0]);
        if st.last_vehicle != vehicle && st.last_vehicle != 0 {
            undo_vehicle(st.last_vehicle);
        }
        st.last_vehicle = vehicle;
    }

    // Turned off since last frame: put the game's own state back.
    let off: Vec<String> = st.was.iter().filter(|(k, &w)| w && !on(k)).map(|(k, _)| k.clone()).collect();
    for k in off {
        match k.as_str() {
            "god" => { native(SET_PLAYER_INVINCIBLE, &[player, 0]); }
            "never-wanted" => { native(SET_MAX_WANTED_LEVEL, &[5]); }
            "ammo" => { native(SET_PED_INFINITE_AMMO_CLIP, &[ped, 0]); }
            "fast-run" => { native(SET_RUN_SPRINT_MULTIPLIER_FOR_PLAYER, &[player, f(1.0)]); }
            "fast-swim" => { native(SET_SWIM_MULTIPLIER_FOR_PLAYER, &[player, f(1.0)]); }
            "vehicle-god" => {
                if st.last_vehicle != 0 {
                    undo_vehicle(st.last_vehicle);
                    st.last_vehicle = 0;
                }
            }
            _ => {}
        }
    }
    st.was = st.on.clone();
}

fn undo_vehicle(v: u64) {
    native(SET_ENTITY_INVINCIBLE, &[v, 0]);
    native(SET_VEHICLE_CAN_BE_VISIBLY_DAMAGED, &[v, 1]);
}

/// One-shot mods. Keys like "spawn:adder", "time:12", "weather:CLEAR".
fn action(key: &str) {
    let ped = native(PLAYER_PED_ID, &[]);
    let (name, arg) = key.split_once(':').unwrap_or((key, ""));
    match name {
        "heal" => {
            let max = native(GET_ENTITY_MAX_HEALTH, &[ped]) as u32 as u64;
            native(SET_ENTITY_HEALTH, &[ped, max, 0, 0]);
            native(SET_PED_ARMOUR, &[ped, 100]);
        }
        "money" => {
            // Story money is a stat per character: Michael, Franklin, Trevor.
            let model = native(GET_ENTITY_MODEL, &[ped]) as u32;
            let Some(i) = ["player_zero", "player_one", "player_two"].iter().position(|m| joaat(m) == model) else { return };
            let stat = joaat(&format!("SP{i}_TOTAL_CASH")) as u64;
            let mut cash: i32 = 0;
            native(STAT_GET_INT, &[stat, &mut cash as *mut i32 as u64, u64::MAX]);
            let add: i32 = arg.parse().unwrap_or(1_000_000);
            native(STAT_SET_INT, &[stat, cash.saturating_add(add) as u32 as u64, 1]);
        }
        "repair" => {
            if native(IS_PED_IN_ANY_VEHICLE, &[ped, 0]) & 0xFF != 0 {
                let v = native(GET_VEHICLE_PED_IS_IN, &[ped, 0]);
                native(SET_VEHICLE_FIXED, &[v]);
                native(SET_VEHICLE_DIRT_LEVEL, &[v, f(0.0)]);
            }
        }
        "waypoint" => teleport_to_waypoint(ped),
        "spawn" => spawn_vehicle(ped, arg),
        "time" => {
            native(SET_CLOCK_TIME, &[arg.parse().unwrap_or(12), 0, 0]);
        }
        "weather" => {
            if let Ok(w) = CString::new(arg) {
                native(SET_WEATHER_TYPE_NOW_PERSIST, &[w.as_ptr() as u64]);
            }
        }
        _ => {}
    }
}

fn teleport_to_waypoint(ped: u64) {
    let blip = native(GET_FIRST_BLIP_INFO_ID, &[8]);
    if native(DOES_BLIP_EXIST, &[blip]) & 0xFF == 0 {
        return;
    }
    let [x, y, _] = native_v3(GET_BLIP_INFO_ID_COORD, &[blip]);
    let entity = if native(IS_PED_IN_ANY_VEHICLE, &[ped, 0]) & 0xFF != 0 { native(GET_VEHICLE_PED_IS_IN, &[ped, 0]) } else { ped };
    // The ground there isn't loaded yet: go up high, then down until the game finds the ground.
    for h in (0..=16).rev().map(|i| i * 50) {
        let z = h as f32;
        native(SET_ENTITY_COORDS_NO_OFFSET, &[entity, f(x), f(y), f(z), 0, 0, 1]);
        wait(50);
        let mut ground: f32 = 0.0;
        if native(GET_GROUND_Z_FOR_3D_COORD, &[f(x), f(y), f(z), &mut ground as *mut f32 as u64, 0, 0]) & 0xFF != 0 {
            native(SET_ENTITY_COORDS_NO_OFFSET, &[entity, f(x), f(y), f(ground + 1.0), 0, 0, 1]);
            return;
        }
    }
}

fn spawn_vehicle(ped: u64, model_name: &str) {
    let model = joaat(model_name) as u64;
    native(REQUEST_MODEL, &[model]);
    for _ in 0..200 {
        if native(HAS_MODEL_LOADED, &[model]) & 0xFF != 0 {
            break;
        }
        wait(0);
    }
    if native(HAS_MODEL_LOADED, &[model]) & 0xFF == 0 {
        return;
    }
    let [x, y, z] = native_v3(GET_ENTITY_COORDS, &[ped, 1]);
    let heading = f32::from_bits(native(GET_ENTITY_HEADING, &[ped]) as u32);
    let v = native(CREATE_VEHICLE, &[model, f(x), f(y), f(z), f(heading), 0, 0, 0]) as u32 as u64;
    if v != 0 {
        native(SET_VEHICLE_ON_GROUND_PROPERLY, &[v, f(5.0)]);
        native(SET_PED_INTO_VEHICLE, &[ped, v, (-1i64) as u64]);
    }
    native(SET_MODEL_AS_NO_LONGER_NEEDED, &[model]);
}
