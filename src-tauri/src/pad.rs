//! Xbox-style controllers (XInput) in the overlay menu: the D-pad or left stick moves, A picks, B
//! closes, LB and RB switch categories, LT and RT switch tabs, and View + Menu together opens or
//! closes the menu. Polled on its own thread: connected controllers every frame while the menu is
//! open, and empty slots only every two seconds (asking an empty slot is slow).

use std::{
    sync::atomic::Ordering,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};

#[repr(C)]
#[derive(Default)]
struct Gamepad {
    buttons: u16,
    left_trigger: u8,
    right_trigger: u8,
    lx: i16,
    ly: i16,
    rx: i16,
    ry: i16,
}

#[repr(C)]
#[derive(Default)]
struct State {
    packet: u32,
    pad: Gamepad,
}

#[link(name = "xinput1_4", kind = "raw-dylib")]
extern "system" {
    fn XInputGetState(user: u32, state: *mut State) -> u32;
}

const UP: u16 = 0x1;
const DOWN: u16 = 0x2;
const LEFT: u16 = 0x4;
const RIGHT: u16 = 0x8;
const MENU: u16 = 0x10;
const VIEW: u16 = 0x20;
const LB: u16 = 0x100;
const RB: u16 = 0x200;
const A: u16 = 0x1000;
const B: u16 = 0x2000;
// Not real XInput buttons: the triggers and the left stick, folded in as buttons.
const LT: u32 = 0x1_0000;
const RT: u32 = 0x2_0000;

/// What every connected controller has pressed right now, combined.
fn pressed(slots: &mut Vec<u32>) -> u32 {
    let mut all = 0u32;
    slots.retain(|&i| {
        let mut s = State::default();
        if unsafe { XInputGetState(i, &mut s) } != 0 {
            return false;
        }
        let p = &s.pad;
        let mut b = p.buttons as u32;
        const DEAD: i16 = 16000;
        if p.ly > DEAD { b |= UP as u32 }
        if p.ly < -DEAD { b |= DOWN as u32 }
        if p.lx < -DEAD { b |= LEFT as u32 }
        if p.lx > DEAD { b |= RIGHT as u32 }
        if p.left_trigger > 128 { b |= LT }
        if p.right_trigger > 128 { b |= RT }
        all |= b;
        true
    });
    all
}

/// The menu's key for a button, as overlay.html's menuKey takes it, and whether it's Shift+.
fn key(b: u32) -> Option<(&'static str, bool)> {
    Some(match b {
        x if x == UP as u32 => ("ArrowUp", false),
        x if x == DOWN as u32 => ("ArrowDown", false),
        x if x == LEFT as u32 => ("ArrowLeft", false),
        x if x == RIGHT as u32 => ("ArrowRight", false),
        x if x == A as u32 => ("Enter", false),
        x if x == B as u32 => ("Escape", false),
        x if x == LB as u32 => ("Tab", true),
        x if x == RB as u32 => ("Tab", false),
        LT => ("ViewPrev", false),
        RT => ("ViewNext", false),
        _ => return None,
    })
}

pub fn start(app: AppHandle) {
    std::thread::spawn(move || {
        let mut slots: Vec<u32> = Vec::new();
        let mut checked = Instant::now() - Duration::from_secs(10);
        let mut last = 0u32;
        // The held direction and when it next repeats.
        let mut repeat: Option<(u32, Instant)> = None;
        loop {
            if checked.elapsed() > Duration::from_secs(2) {
                checked = Instant::now();
                for i in 0..4 {
                    let mut s = State::default();
                    if !slots.contains(&i) && unsafe { XInputGetState(i, &mut s) } == 0 {
                        slots.push(i);
                    }
                }
            }
            if slots.is_empty() {
                std::thread::sleep(Duration::from_millis(500));
                continue;
            }
            let now = pressed(&mut slots);
            let down = now & !last;
            let menu_open = crate::overlay::OVERLAY_MENU.load(Ordering::Relaxed);
            // View + Menu together: the main window opens or closes the menu (if it's allowed).
            let combo = (VIEW | MENU) as u32;
            if now & combo == combo && last & combo != combo {
                let _ = app.emit_to("main", "pad-menu", ());
            } else if menu_open {
                let send = |b: u32| {
                    if let Some((k, shift)) = key(b) {
                        let _ = app.emit_to("overlay", "overlay", serde_json::json!({ "kind": "key", "key": k, "shift": shift }));
                    }
                };
                for bit in 0..18 {
                    let b = 1u32 << bit;
                    if down & b != 0 {
                        send(b);
                        if b & (UP | DOWN | LEFT | RIGHT) as u32 != 0 {
                            repeat = Some((b, Instant::now() + Duration::from_millis(400)));
                        }
                    }
                }
                // A held direction repeats, like a held arrow key.
                if let Some((b, at)) = repeat {
                    if now & b == 0 {
                        repeat = None;
                    } else if Instant::now() >= at {
                        send(b);
                        repeat = Some((b, Instant::now() + Duration::from_millis(110)));
                    }
                }
            }
            last = now;
            std::thread::sleep(Duration::from_millis(if menu_open { 16 } else { 50 }));
        }
    });
}
