//! Game Boost: while a game runs, a gear decides how hard HotSwap pushes the PC: 1 Eco, 2 Sport,
//! 3 Turbo, 4 Redline, or 5 Custom. Each gear is a full set of settings the user can change (the
//! UI ships sensible ones: Eco raises the game's priority, Sport adds the High performance plan,
//! Turbo closes or freezes picked apps, Redline uses the hidden Ultimate Performance plan, added
//! for the game and removed after). The settings cover: priority, power plan, Game Mode, no background
//! recording, no mouse acceleration, no transparency, and Efficiency mode (what Task Manager's leaf
//! does) for Steam's web helper, browsers, Discord and picked apps. Changes apply live, only the
//! difference, and everything goes back when the game closes (or, after a crash, when HotSwap
//! starts next). No "RAM cleaning": Windows already hands its cached memory to a game the moment
//! it asks, and emptying it makes games load slower.

use serde::{Deserialize, Serialize};
use std::{collections::HashSet, path::PathBuf, process::Command, sync::Mutex, time::Duration};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Deserialize, Serialize, Clone, Default, PartialEq)]
#[serde(default)]
pub struct Adv {
    /// "auto" (from the gear), "normal", "above" or "high".
    priority: String,
    /// "auto" (from the gear), "none", "high" or "ultimate".
    plan: String,
    game_mode: bool,
    no_capture: bool,
    no_mouse_accel: bool,
    no_transparency: bool,
    eco_steam: bool,
    eco_browsers: bool,
    eco_discord: bool,
    /// More executable names to put in Efficiency mode.
    eco_apps: Vec<String>,
    /// The game's monitor at its highest refresh rate.
    max_refresh: bool,
    /// The Windows key does nothing while the game is in front.
    block_win: bool,
    /// No Sticky Keys (and Filter or Toggle Keys) popup from pressing Shift five times.
    no_sticky: bool,
    pause_wallpaper: bool,
    /// HotSwap's window hides and frees memory while the game runs.
    hide_hotswap: bool,
    /// Windows never runs the game in power-saving mode (EcoQoS).
    no_throttle: bool,
    /// The game runs on the dedicated graphics card (laptops). Kept for that game, since Windows
    /// reads it when the game starts.
    fast_gpu: bool,
    /// No Xbox Game Bar from the controller's Xbox button or Win+G while the game is in front.
    no_gamebar: bool,
    /// Close the apps on the close list.
    close_apps: bool,
    /// Freeze the apps on the freeze list.
    freeze_apps: bool,
}

#[derive(Deserialize, Clone, Default)]
#[serde(default)]
pub struct Config {
    /// 0 off, 1 Eco, 2 Sport, 3 Turbo, 4 Redline.
    level: u8,
    /// Executable names to close in Turbo and Redline, e.g. "chrome.exe".
    close: Vec<String>,
    /// Executable names to freeze (pause completely) in Turbo and Redline, resumed after.
    freeze: Vec<String>,
    reopen: bool,
    /// Each gear's settings, by gear number (0, Off, is unused).
    gears: Vec<Adv>,
    /// A gear remembered for a game (game ID -> gear), instead of `level`.
    per_game: std::collections::HashMap<String, u8>,
    /// On battery, Game Boost stays at Eco.
    battery_eco: bool,
}

impl Config {
    /// The gear for a game: its own one if remembered, else the default; Eco at most on battery.
    fn level_for(&self, game: &str) -> u8 {
        let l = self.per_game.get(game).copied().unwrap_or(self.level).min(5);
        if self.battery_eco && on_battery() { l.min(1) } else { l }
    }

    fn gear(&self, level: u8) -> Adv {
        self.gears.get(level as usize).cloned().unwrap_or_default()
    }
}

/// True when a laptop runs on its battery.
fn on_battery() -> bool {
    #[repr(C)]
    struct PowerStatus { ac: u8, flags: u8, percent: u8, saver: u8, life: u32, full: u32 }
    #[link(name = "kernel32")]
    extern "system" {
        fn GetSystemPowerStatus(s: *mut PowerStatus) -> i32;
    }
    let mut s = PowerStatus { ac: 255, flags: 0, percent: 0, saver: 0, life: 0, full: 0 };
    unsafe { GetSystemPowerStatus(&mut s) != 0 && s.ac == 0 }
}

/// Windows settings changed by the Advanced extras, to put back.
#[derive(Serialize, Deserialize, Default, Clone)]
struct Extras {
    /// (key under HKEY_CURRENT_USER, value name, value before; None = it didn't exist)
    reg: Vec<(String, String, Option<u32>)>,
    #[serde(skip)]
    mouse_before: Option<[i32; 3]>,
    #[serde(skip)]
    eco_names: Vec<String>,
    #[serde(skip)]
    eco: HashSet<u32>,
    /// The monitor (device name) switched to its highest refresh rate; reset to its normal mode after.
    display: Option<String>,
    #[serde(skip)]
    want_refresh: bool,
    #[serde(skip)]
    keys_before: Option<[u32; 3]>,
    #[serde(skip)]
    wallpaper: Option<PathBuf>,
    #[serde(skip)]
    hid_hotswap: bool,
    #[serde(skip)]
    unthrottled: Vec<u32>,
}

/// A running boost: what it changed, to change further or undo.
struct Active {
    game: String,
    pids: Vec<u32>,
    level: u8,
    /// Applied game priority: 0 normal, 1 above normal, 2 high.
    prio: u8,
    /// Applied power plan: 0 unchanged, 1 High performance, 2 Ultimate Performance.
    plan: u8,
    power_before: Option<String>,
    /// An Ultimate Performance plan HotSwap added (deleted when it's no longer used).
    added_plan: Option<String>,
    apps_closed: bool,
    apps_frozen: bool,
    closed: Vec<PathBuf>,
    /// Apps frozen for the game: (process, its executable), resumed after.
    frozen: Vec<(u32, PathBuf)>,
    /// The battery state the gear was last picked with.
    battery: bool,
    extras: Option<Extras>,
    /// The Advanced settings the extras were applied from.
    adv: Option<Adv>,
}

#[derive(Default)]
pub struct Boost {
    cfg: Mutex<Config>,
    active: Mutex<Option<Active>>,
}

/// What the UI shows when a boost starts, changes or ends.
#[derive(Serialize, Clone)]
struct Report {
    started: bool,
    level: u8,
    lines: Vec<String>,
    /// Everything the boost is doing right now (empty once it ended).
    status: Vec<String>,
}

impl Active {
    /// What this boost is doing right now, one line each.
    fn status(&self) -> Vec<String> {
        let mut s = Vec::new();
        if self.level == 0 {
            return s;
        }
        s.push(format!("Gear: {}{}", GEARS[self.level as usize], if self.battery && self.level == 1 { " (on battery)" } else { "" }));
        if self.prio > 0 {
            s.push(format!("Game priority: {}", if self.prio == 2 { "high" } else { "above normal" }));
        }
        match self.plan {
            1 => s.push("Power plan: High performance".into()),
            2 => s.push("Power plan: Ultimate Performance".into()),
            _ => {}
        }
        if !self.closed.is_empty() {
            s.push(format!("Closed: {}", self.closed.iter().filter_map(|e| e.file_stem()).map(|x| x.to_string_lossy().to_string()).collect::<Vec<_>>().join(", ")));
        }
        if !self.frozen.is_empty() {
            let mut names: Vec<String> = self.frozen.iter().filter_map(|f| f.1.file_stem()).map(|x| x.to_string_lossy().to_string()).collect();
            names.dedup();
            s.push(format!("Frozen: {}", names.join(", ")));
        }
        if let Some(e) = &self.extras {
            if e.display.is_some() {
                s.push("Monitor at its highest refresh rate".into());
            }
            if !e.eco.is_empty() {
                s.push(format!("Efficiency mode: {} background processes", e.eco.len()));
            }
            if !e.unthrottled.is_empty() {
                s.push("The game is never throttled".into());
            }
            for (_, name, _) in &e.reg {
                match name.as_str() {
                    "AutoGameModeEnabled" => s.push("Windows Game Mode on".into()),
                    "AppCaptureEnabled" => s.push("Background recording off".into()),
                    "EnableTransparency" => s.push("Transparency off".into()),
                    "UseNexusForGameBarEnabled" => s.push("Xbox Game Bar off".into()),
                    _ => {}
                }
            }
            if e.mouse_before.is_some() {
                s.push("Mouse acceleration off".into());
            }
            if e.keys_before.is_some() {
                s.push("No Sticky Keys popup".into());
            }
            if e.wallpaper.is_some() {
                s.push("Wallpaper Engine paused".into());
            }
        }
        s
    }
}

const GEARS: [&str; 6] = ["Off", "Eco", "Sport", "Turbo", "Redline", "Custom"];
/// Windows' built-in (normally hidden) Ultimate Performance plan template.
const ULTIMATE_TEMPLATE: &str = "e9a42b02-d5df-448d-aa00-03f14749eb61";
const BROWSERS: &[&str] = &["chrome.exe", "msedge.exe", "firefox.exe", "opera.exe", "brave.exe", "vivaldi.exe"];

/// Apps that must never be closed: Windows itself, Steam (games need it), HotSwap.
const NEVER: &[&str] = &[
    "explorer.exe", "dwm.exe", "csrss.exe", "winlogon.exe", "svchost.exe", "lsass.exe", "services.exe", "smss.exe", "wininit.exe",
    "steam.exe", "steamwebhelper.exe", "steamservice.exe", "hotswap.exe", "msedgewebview2.exe", "audiodg.exe", "sihost.exe",
    "ctfmon.exe", "fontdrvhost.exe", "searchhost.exe", "startmenuexperiencehost.exe", "shellexperiencehost.exe", "textinputhost.exe",
    "runtimebroker.exe", "taskmgr.exe", "conhost.exe", "securityhealthsystray.exe", "msmpeng.exe", "nissrv.exe",
];

/// Runs a console program without a window and returns its output.
fn run(exe: &str, args: &[&str]) -> Option<String> {
    let mut cmd = Command::new(exe);
    cmd.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = cmd.output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).to_string())
}

fn guid_in(text: &str) -> Option<String> {
    let i = text.find("GUID: ")? + 6;
    Some(text[i..].chars().take(36).collect())
}

fn active_plan() -> Option<String> {
    guid_in(&run("powercfg", &["/getactivescheme"])?)
}

/// What to undo after a crash, kept while a boost runs: the plan before, the plan added, and the
/// registry values changed. Undone the next time HotSwap starts.
#[derive(Serialize, Deserialize, Default)]
struct Saved {
    power_before: Option<String>,
    added_plan: Option<String>,
    reg: Vec<(String, String, Option<u32>)>,
    #[serde(default)]
    display: Option<String>,
    /// Frozen apps (process, executable): resumed if still the same process.
    #[serde(default)]
    frozen: Vec<(u32, PathBuf)>,
}

fn saved_file(app: &AppHandle) -> Option<PathBuf> {
    Some(app.path().app_data_dir().ok()?.join("boost_restore.json"))
}

fn save_state(app: &AppHandle, a: &Active) {
    let Some(f) = saved_file(app) else { return };
    let s = Saved {
        power_before: a.power_before.clone(),
        added_plan: a.added_plan.clone(),
        reg: a.extras.as_ref().map(|e| e.reg.clone()).unwrap_or_default(),
        display: a.extras.as_ref().and_then(|e| e.display.clone()),
        frozen: a.frozen.clone(),
    };
    if s.power_before.is_none() && s.added_plan.is_none() && s.reg.is_empty() && s.display.is_none() && s.frozen.is_empty() {
        let _ = std::fs::remove_file(f);
    } else if let Ok(j) = serde_json::to_string(&s) {
        let _ = std::fs::write(f, j);
    }
}

/// Undoes what a boost that never ended (HotSwap closed mid-game) left behind.
pub fn recover(app: &AppHandle) {
    let Some(f) = saved_file(app) else { return };
    // The file of 0.3.10's test builds, with only power plans.
    let old = app.path().app_data_dir().ok().map(|d| d.join("boost_power.txt"));
    if let Some(old) = old.filter(|o| o.exists()) {
        if let Some(b) = std::fs::read_to_string(&old).ok().and_then(|t| t.lines().next().map(str::to_string)).filter(|l| !l.is_empty()) {
            let _ = run("powercfg", &["/setactive", b.trim()]);
        }
        let _ = std::fs::remove_file(old);
    }
    let Some(s) = std::fs::read_to_string(&f).ok().and_then(|t| serde_json::from_str::<Saved>(&t).ok()) else { return };
    if let Some(b) = &s.power_before {
        let _ = run("powercfg", &["/setactive", b]);
    }
    if let Some(p) = &s.added_plan {
        let _ = run("powercfg", &["/delete", p]);
    }
    for (key, name, before) in &s.reg {
        reg_restore(key, name, *before);
    }
    if !s.reg.is_empty() {
        broadcast_theme();
    }
    if let Some(d) = &s.display {
        display_reset(d);
    }
    if !s.frozen.is_empty() {
        let sys = processes();
        for (pid, exe) in &s.frozen {
            if sys.process(sysinfo::Pid::from_u32(*pid)).and_then(|p| p.exe()).is_some_and(|e| e == exe) {
                suspend(*pid, false);
            }
        }
    }
    let _ = std::fs::remove_file(f);
}

/// The biggest visible window of the game's processes: the one it plays in.
fn game_window(pids: &[u32]) -> Option<isize> {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::{
            Foundation::{HWND, LPARAM, RECT},
            UI::WindowsAndMessaging::{EnumWindows, GetWindowRect, GetWindowThreadProcessId, IsWindowVisible},
        };
        struct Find<'a> { pids: &'a [u32], best: Option<(i64, isize)> }
        unsafe extern "system" fn each(h: HWND, l: LPARAM) -> i32 {
            let f = &mut *(l as *mut Find);
            let mut pid = 0u32;
            GetWindowThreadProcessId(h, &mut pid);
            if f.pids.contains(&pid) && IsWindowVisible(h) != 0 {
                let mut r: RECT = std::mem::zeroed();
                GetWindowRect(h, &mut r);
                let area = (r.right - r.left) as i64 * (r.bottom - r.top) as i64;
                if area > 200 * 200 && f.best.is_none_or(|b| area > b.0) {
                    f.best = Some((area, h as isize));
                }
            }
            1
        }
        let mut f = Find { pids, best: None };
        EnumWindows(Some(each), &mut f as *mut Find as LPARAM);
        return f.best.map(|b| b.1);
    }
    #[allow(unreachable_code)]
    None
}

/// Switches the monitor showing `window` to the highest refresh rate it offers at its current
/// resolution. Returns the monitor's device name if it changed anything. The change isn't saved:
/// Windows' own setting comes back after the game (or a restart).
fn display_max(window: isize) -> Option<(String, u32, u32)> {
    display_max_why(window).ok()
}

fn display_max_why(window: isize) -> Result<(String, u32, u32), String> {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::Graphics::Gdi::{
            ChangeDisplaySettingsExW, EnumDisplaySettingsW, GetMonitorInfoW, MonitorFromWindow, DEVMODEW, DM_BITSPERPEL, DM_DISPLAYFREQUENCY, DM_PELSHEIGHT, DM_PELSWIDTH, ENUM_CURRENT_SETTINGS, MONITORINFO,
            MONITORINFOEXW, MONITOR_DEFAULTTONEAREST,
        };
        let mon = MonitorFromWindow(window as _, MONITOR_DEFAULTTONEAREST);
        let mut info: MONITORINFOEXW = std::mem::zeroed();
        info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
        if GetMonitorInfoW(mon, &mut info as *mut MONITORINFOEXW as *mut MONITORINFO) == 0 {
            return Err("no monitor info".into());
        }
        let device = info.szDevice;
        let mut cur: DEVMODEW = std::mem::zeroed();
        cur.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
        if EnumDisplaySettingsW(device.as_ptr(), ENUM_CURRENT_SETTINGS, &mut cur) == 0 {
            return Err("no current mode".into());
        }
        let mut best = cur.dmDisplayFrequency;
        for i in 0..1000 {
            let mut m: DEVMODEW = std::mem::zeroed();
            m.dmSize = std::mem::size_of::<DEVMODEW>() as u16;
            if EnumDisplaySettingsW(device.as_ptr(), i, &mut m) == 0 {
                break;
            }
            if m.dmPelsWidth == cur.dmPelsWidth && m.dmPelsHeight == cur.dmPelsHeight && m.dmBitsPerPel == cur.dmBitsPerPel {
                best = best.max(m.dmDisplayFrequency);
            }
        }
        let name = String::from_utf16_lossy(&device[..device.iter().position(|&c| c == 0).unwrap_or(32)]);
        if best <= cur.dmDisplayFrequency {
            return Err(format!("{name} already at its best ({} Hz at {}x{})", cur.dmDisplayFrequency, cur.dmPelsWidth, cur.dmPelsHeight));
        }
        let from = cur.dmDisplayFrequency;
        cur.dmDisplayFrequency = best;
        cur.dmFields = DM_PELSWIDTH | DM_PELSHEIGHT | DM_BITSPERPEL | DM_DISPLAYFREQUENCY;
        // Flags 0: for now only, not written to Windows' settings.
        let r = ChangeDisplaySettingsExW(device.as_ptr(), &cur, std::ptr::null_mut(), 0, std::ptr::null());
        if r != 0 {
            return Err(format!("{name}: Windows refused {best} Hz (code {r})"));
        }
        return Ok((name, from, best));
    }
    #[allow(unreachable_code)]
    Err("not Windows".into())
}

/// Dev aid (`hotswap --boost-display <exe name>`): finds the app's window, switches its monitor to the
/// highest refresh rate for five seconds, then back.
pub fn test_display(exe: &str) -> String {
    let sys = processes();
    let pids: Vec<u32> = sys.processes().values().filter(|p| p.name().to_string_lossy().eq_ignore_ascii_case(exe)).map(|p| p.pid().as_u32()).collect();
    let Some(w) = game_window(&pids) else { return format!("no window for {exe} ({} processes)", pids.len()) };
    let (name, from, to) = match display_max_why(w) { Ok(v) => v, Err(e) => return e };
    std::thread::sleep(Duration::from_secs(5));
    display_reset(&name);
    format!("{name}: {from} Hz -> {to} Hz, then back")
}

/// Puts a monitor back to the mode saved in Windows' settings.
fn display_reset(device: &str) {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::Graphics::Gdi::ChangeDisplaySettingsExW;
        let d: Vec<u16> = device.encode_utf16().chain([0]).collect();
        ChangeDisplaySettingsExW(d.as_ptr(), std::ptr::null(), std::ptr::null_mut(), 0, std::ptr::null());
    }
}

/// The "press Shift five times" shortcuts of Sticky, Filter and Toggle Keys: their flags before.
fn keys_shortcuts_off() -> Option<[u32; 3]> {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::UI::{
            Accessibility::{FILTERKEYS, STICKYKEYS, TOGGLEKEYS},
            WindowsAndMessaging::{SystemParametersInfoW, SPI_GETFILTERKEYS, SPI_GETSTICKYKEYS, SPI_GETTOGGLEKEYS, SPI_SETFILTERKEYS, SPI_SETSTICKYKEYS, SPI_SETTOGGLEKEYS},
        };
        const HOTKEY: u32 = 4; // SKF_, FKF_ and TKF_HOTKEYACTIVE
        let mut s = STICKYKEYS { cbSize: std::mem::size_of::<STICKYKEYS>() as u32, dwFlags: 0 };
        let mut f: FILTERKEYS = std::mem::zeroed();
        f.cbSize = std::mem::size_of::<FILTERKEYS>() as u32;
        let mut t = TOGGLEKEYS { cbSize: std::mem::size_of::<TOGGLEKEYS>() as u32, dwFlags: 0 };
        SystemParametersInfoW(SPI_GETSTICKYKEYS, s.cbSize, &mut s as *mut _ as _, 0);
        SystemParametersInfoW(SPI_GETFILTERKEYS, f.cbSize, &mut f as *mut _ as _, 0);
        SystemParametersInfoW(SPI_GETTOGGLEKEYS, t.cbSize, &mut t as *mut _ as _, 0);
        let before = [s.dwFlags, f.dwFlags, t.dwFlags];
        s.dwFlags &= !HOTKEY;
        f.dwFlags &= !HOTKEY;
        t.dwFlags &= !HOTKEY;
        // Not written to Windows' settings: a restart brings them back in any case.
        SystemParametersInfoW(SPI_SETSTICKYKEYS, s.cbSize, &mut s as *mut _ as _, 0);
        SystemParametersInfoW(SPI_SETFILTERKEYS, f.cbSize, &mut f as *mut _ as _, 0);
        SystemParametersInfoW(SPI_SETTOGGLEKEYS, t.cbSize, &mut t as *mut _ as _, 0);
        return Some(before);
    }
    #[allow(unreachable_code)]
    None
}

fn keys_shortcuts_restore(before: [u32; 3]) {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::UI::{
            Accessibility::{FILTERKEYS, STICKYKEYS, TOGGLEKEYS},
            WindowsAndMessaging::{SystemParametersInfoW, SPI_GETFILTERKEYS, SPI_SETFILTERKEYS, SPI_SETSTICKYKEYS, SPI_SETTOGGLEKEYS},
        };
        let mut s = STICKYKEYS { cbSize: std::mem::size_of::<STICKYKEYS>() as u32, dwFlags: before[0] };
        let mut f: FILTERKEYS = std::mem::zeroed();
        f.cbSize = std::mem::size_of::<FILTERKEYS>() as u32;
        SystemParametersInfoW(SPI_GETFILTERKEYS, f.cbSize, &mut f as *mut _ as _, 0);
        f.dwFlags = before[1];
        let mut t = TOGGLEKEYS { cbSize: std::mem::size_of::<TOGGLEKEYS>() as u32, dwFlags: before[2] };
        SystemParametersInfoW(SPI_SETSTICKYKEYS, s.cbSize, &mut s as *mut _ as _, 0);
        SystemParametersInfoW(SPI_SETFILTERKEYS, f.cbSize, &mut f as *mut _ as _, 0);
        SystemParametersInfoW(SPI_SETTOGGLEKEYS, t.cbSize, &mut t as *mut _ as _, 0);
    }
}

/// Game processes whose window, when in front, gets the Windows key swallowed (empty: none).
static BLOCK_WIN: Mutex<Vec<u32>> = Mutex::new(Vec::new());
/// The same for Win+G only (Xbox Game Bar).
static BLOCK_GAMEBAR: Mutex<Vec<u32>> = Mutex::new(Vec::new());

/// Pauses (freezes) or resumes every thread of a process, like Resource Monitor's "Suspend".
fn suspend(pid: u32, on: bool) -> bool {
    #[link(name = "ntdll")]
    extern "system" {
        fn NtSuspendProcess(h: *mut core::ffi::c_void) -> i32;
        fn NtResumeProcess(h: *mut core::ffi::c_void) -> i32;
    }
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::{Foundation::CloseHandle, System::Threading::OpenProcess};
        const PROCESS_SUSPEND_RESUME: u32 = 0x0800;
        let h = OpenProcess(PROCESS_SUSPEND_RESUME, 0, pid);
        if h.is_null() {
            return false;
        }
        let ok = if on { NtSuspendProcess(h) } else { NtResumeProcess(h) } == 0;
        CloseHandle(h);
        return ok;
    }
    #[allow(unreachable_code)]
    false
}

/// Windows' power throttling for a process: Some(true) throttled (Efficiency mode), Some(false)
/// never throttled, None Windows decides again.
fn set_qos(pid: u32, throttled: Option<bool>) -> bool {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::{
            Foundation::CloseHandle,
            System::Threading::{OpenProcess, ProcessPowerThrottling, SetProcessInformation, PROCESS_POWER_THROTTLING_EXECUTION_SPEED, PROCESS_POWER_THROTTLING_STATE, PROCESS_SET_INFORMATION},
        };
        let h = OpenProcess(PROCESS_SET_INFORMATION, 0, pid);
        if h.is_null() {
            return false;
        }
        let state = PROCESS_POWER_THROTTLING_STATE {
            Version: 1,
            ControlMask: if throttled.is_some() { PROCESS_POWER_THROTTLING_EXECUTION_SPEED } else { 0 },
            StateMask: if throttled == Some(true) { PROCESS_POWER_THROTTLING_EXECUTION_SPEED } else { 0 },
        };
        let ok = SetProcessInformation(h, ProcessPowerThrottling, &state as *const _ as _, std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32) != 0;
        CloseHandle(h);
        return ok;
    }
    #[allow(unreachable_code)]
    false
}

/// A process's memory priority: low (its memory is moved out first when RAM is short) or normal.
fn set_mem_priority(pid: u32, low: bool) {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::{Foundation::CloseHandle, System::Threading::{OpenProcess, SetProcessInformation, PROCESS_SET_INFORMATION}};
        const PROCESS_MEMORY_PRIORITY: i32 = 0; // ProcessMemoryPriority
        let prio: u32 = if low { 2 } else { 5 }; // MEMORY_PRIORITY_LOW / _NORMAL
        let h = OpenProcess(PROCESS_SET_INFORMATION, 0, pid);
        if !h.is_null() {
            SetProcessInformation(h, PROCESS_MEMORY_PRIORITY, &prio as *const u32 as _, 4);
            CloseHandle(h);
        }
    }
}

/// The game's main executable: its process with the most memory.
fn game_exe(pids: &[u32]) -> Option<PathBuf> {
    let sys = processes();
    pids.iter().filter_map(|&p| sys.process(sysinfo::Pid::from_u32(p))).max_by_key(|p| p.memory()).and_then(|p| p.exe().map(|e| e.to_path_buf()))
}

/// Windows' per-app graphics preference ("High performance" GPU) for `exe`. On: set (kept, Windows
/// reads it when the game starts). Off: removed again if HotSwap set it. Returns whether it was set.
fn gpu_pref(exe: &std::path::Path, on: bool) -> bool {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    const KEY: &str = r"Software\Microsoft\DirectX\UserGpuPreferences";
    const MINE: &str = "GpuPreference=2;";
    let Ok((k, _)) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(KEY) else { return false };
    let name = exe.to_string_lossy().to_string();
    let had = k.get_value::<String, _>(&name).ok();
    if on && had.as_deref() != Some(MINE) {
        let _ = k.set_value(&name, &MINE.to_string());
    } else if !on && had.as_deref() == Some(MINE) {
        let _ = k.delete_value(&name);
    }
    had.as_deref() == Some(MINE)
}

/// A low-level keyboard hook, installed once on its own thread, that drops the Windows keys while
/// a game in BLOCK_WIN is the foreground window. Everything else passes straight through.
fn win_key_hook() {
    static STARTED: std::sync::Once = std::sync::Once::new();
    #[cfg(windows)]
    STARTED.call_once(|| {
        std::thread::spawn(|| unsafe {
            use windows_sys::Win32::UI::{
                Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LWIN, VK_RWIN},
                WindowsAndMessaging::{CallNextHookEx, GetForegroundWindow, GetMessageW, GetWindowThreadProcessId, SetWindowsHookExW, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL},
            };
            unsafe extern "system" fn hook(code: i32, w: usize, l: isize) -> isize {
                if code >= 0 {
                    let k = &*(l as *const KBDLLHOOKSTRUCT);
                    // Win+G (Xbox Game Bar): the G while a Windows key is held.
                    if k.vkCode == 0x47 && (GetAsyncKeyState(VK_LWIN as i32) < 0 || GetAsyncKeyState(VK_RWIN as i32) < 0) {
                        let pids = BLOCK_GAMEBAR.lock().map(|p| p.clone()).unwrap_or_default();
                        let mut pid = 0u32;
                        GetWindowThreadProcessId(GetForegroundWindow(), &mut pid);
                        if pids.contains(&pid) {
                            return 1;
                        }
                    }
                    if k.vkCode == VK_LWIN as u32 || k.vkCode == VK_RWIN as u32 {
                        let pids = BLOCK_WIN.lock().map(|p| p.clone()).unwrap_or_default();
                        if !pids.is_empty() {
                            let mut pid = 0u32;
                            GetWindowThreadProcessId(GetForegroundWindow(), &mut pid);
                            if pids.contains(&pid) {
                                return 1;
                            }
                        }
                    }
                }
                CallNextHookEx(std::ptr::null_mut(), code, w, l)
            }
            SetWindowsHookExW(WH_KEYBOARD_LL, Some(hook), std::ptr::null_mut(), 0);
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {}
        });
    });
}

/// Wallpaper Engine's running executable, if it runs.
fn wallpaper_engine() -> Option<PathBuf> {
    processes().processes().values()
        .find(|p| matches!(p.name().to_string_lossy().to_lowercase().as_str(), "wallpaper64.exe" | "wallpaper32.exe"))
        .and_then(|p| p.exe().map(|e| e.to_path_buf()))
}

fn reg_set(key: &str, name: &str, value: u32) -> Option<Option<u32>> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    let (k, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(key).ok()?;
    let before = k.get_value::<u32, _>(name).ok();
    k.set_value(name, &value).ok()?;
    Some(before)
}

fn reg_restore(key: &str, name: &str, before: Option<u32>) {
    use winreg::{enums::{HKEY_CURRENT_USER, KEY_SET_VALUE}, RegKey};
    let Ok(k) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(key, KEY_SET_VALUE) else { return };
    let _ = match before {
        Some(v) => k.set_value(name, &v),
        None => k.delete_value(name),
    };
}

/// Tells Windows the theme settings changed, so transparency switches right away.
fn broadcast_theme() {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{SendMessageTimeoutW, HWND_BROADCAST, SMTO_ABORTIFHUNG, WM_SETTINGCHANGE};
        let s: Vec<u16> = "ImmersiveColorSet\0".encode_utf16().collect();
        let mut r = 0usize;
        SendMessageTimeoutW(HWND_BROADCAST, WM_SETTINGCHANGE, 0, s.as_ptr() as isize, SMTO_ABORTIFHUNG, 200, &mut r);
    }
}

/// Mouse acceleration ("Enhance pointer precision") as [threshold 1, threshold 2, on]. Changes are
/// for this sign-in only (not saved), so a crash can't leave it changed past a restart.
fn mouse() -> Option<[i32; 3]> {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPI_GETMOUSE};
        let mut m = [0i32; 3];
        return (SystemParametersInfoW(SPI_GETMOUSE, 0, m.as_mut_ptr() as _, 0) != 0).then_some(m);
    }
    #[allow(unreachable_code)]
    None
}

fn set_mouse(mut m: [i32; 3]) {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{SystemParametersInfoW, SPIF_SENDCHANGE, SPI_SETMOUSE};
        SystemParametersInfoW(SPI_SETMOUSE, 0, m.as_mut_ptr() as _, SPIF_SENDCHANGE);
    }
}

fn set_priority(pid: u32, prio: u8) -> bool {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::{
            Foundation::CloseHandle,
            System::Threading::{OpenProcess, SetPriorityClass, ABOVE_NORMAL_PRIORITY_CLASS, HIGH_PRIORITY_CLASS, NORMAL_PRIORITY_CLASS, PROCESS_SET_INFORMATION},
        };
        let h = OpenProcess(PROCESS_SET_INFORMATION, 0, pid);
        if h.is_null() {
            return false;
        }
        let class = match prio { 0 => NORMAL_PRIORITY_CLASS, 1 => ABOVE_NORMAL_PRIORITY_CLASS, _ => HIGH_PRIORITY_CLASS };
        let ok = SetPriorityClass(h, class) != 0;
        CloseHandle(h);
        return ok;
    }
    #[allow(unreachable_code)]
    false
}

/// Efficiency mode, as Task Manager's leaf sets it: Windows' power throttling (EcoQoS) and idle
/// priority. Off gives Windows the decision back and normal priority.
fn set_eco(pid: u32, on: bool) -> bool {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::{
            Foundation::CloseHandle,
            System::Threading::{
                OpenProcess, ProcessPowerThrottling, SetPriorityClass, SetProcessInformation, IDLE_PRIORITY_CLASS, NORMAL_PRIORITY_CLASS,
                PROCESS_POWER_THROTTLING_EXECUTION_SPEED, PROCESS_POWER_THROTTLING_STATE, PROCESS_SET_INFORMATION,
            },
        };
        let h = OpenProcess(PROCESS_SET_INFORMATION, 0, pid);
        if h.is_null() {
            return false;
        }
        let state = PROCESS_POWER_THROTTLING_STATE {
            Version: 1,
            ControlMask: if on { PROCESS_POWER_THROTTLING_EXECUTION_SPEED } else { 0 },
            StateMask: if on { PROCESS_POWER_THROTTLING_EXECUTION_SPEED } else { 0 },
        };
        let ok = SetProcessInformation(h, ProcessPowerThrottling, &state as *const _ as _, std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32) != 0;
        SetPriorityClass(h, if on { IDLE_PRIORITY_CLASS } else { NORMAL_PRIORITY_CLASS });
        CloseHandle(h);
        // Their memory goes first when the game needs more.
        set_mem_priority(pid, on);
        return ok;
    }
    #[allow(unreachable_code)]
    false
}

fn processes() -> System {
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_exe(UpdateKind::Always).with_memory());
    sys
}

fn mb(bytes: u64) -> String {
    if bytes >= 1 << 30 { format!("{:.1} GB", bytes as f64 / (1u64 << 30) as f64) } else { format!("{} MB", bytes >> 20) }
}

impl Extras {
    fn apply(app: &AppHandle, adv: &Adv, pids: &[u32]) -> (Extras, Vec<String>) {
        let mut e = Extras::default();
        let mut lines = Vec::new();
        let mut reg = |key: &str, name: &str, v: u32| {
            if let Some(before) = reg_set(key, name, v) {
                e.reg.push((key.to_string(), name.to_string(), before));
            }
        };
        if adv.game_mode {
            reg(r"Software\Microsoft\GameBar", "AutoGameModeEnabled", 1);
            lines.push("Windows Game Mode on".to_string());
        }
        if adv.no_capture {
            reg(r"System\GameConfigStore", "GameDVR_Enabled", 0);
            reg(r"Software\Microsoft\Windows\CurrentVersion\GameDVR", "AppCaptureEnabled", 0);
            lines.push("Background recording off".to_string());
        }
        if adv.no_transparency {
            reg(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize", "EnableTransparency", 0);
            broadcast_theme();
            lines.push("Transparency effects off".to_string());
        }
        if adv.no_mouse_accel {
            if let Some(m) = mouse().filter(|m| m[2] != 0) {
                set_mouse([m[0], m[1], 0]);
                e.mouse_before = Some(m);
                lines.push("Mouse acceleration off".to_string());
            }
        }
        let mut eco: Vec<String> = Vec::new();
        if adv.eco_steam {
            eco.push("steamwebhelper.exe".into());
        }
        if adv.eco_browsers {
            eco.extend(BROWSERS.iter().map(|s| s.to_string()));
        }
        if adv.eco_discord {
            eco.push("discord.exe".into());
        }
        eco.extend(adv.eco_apps.iter().map(|n| n.to_lowercase()).filter(|n| !NEVER.contains(&n.as_str())));
        e.eco_names = eco;
        if !e.eco_names.is_empty() {
            lines.push("Efficiency mode for background apps".to_string());
        }
        // The game's window may not be open yet: the refresh rate is set once it is (refresh_display).
        e.want_refresh = adv.max_refresh;
        if adv.block_win {
            win_key_hook();
            *BLOCK_WIN.lock().unwrap() = pids.to_vec();
            lines.push("Windows key blocked in the game".to_string());
        }
        if adv.no_sticky {
            e.keys_before = keys_shortcuts_off();
            lines.push("No Sticky Keys popup".to_string());
        }
        if adv.pause_wallpaper {
            if let Some(w) = wallpaper_engine() {
                let _ = Command::new(&w).args(["-control", "pause"]).spawn();
                e.wallpaper = Some(w);
                lines.push("Wallpaper Engine paused".to_string());
            }
        }
        if adv.no_throttle {
            e.unthrottled = pids.iter().copied().filter(|&p| set_qos(p, Some(false))).collect();
        }
        if let Some(exe) = game_exe(pids) {
            let had = gpu_pref(&exe, adv.fast_gpu);
            if adv.fast_gpu && !had {
                lines.push("The game will use the fast graphics card from its next start".to_string());
            }
        }
        if adv.no_gamebar {
            reg(r"Software\Microsoft\GameBar", "UseNexusForGameBarEnabled", 0);
            win_key_hook();
            *BLOCK_GAMEBAR.lock().unwrap() = pids.to_vec();
            lines.push("Xbox Game Bar off".to_string());
        }
        if adv.hide_hotswap {
            if let Some(win) = app.get_webview_window("main").filter(|w| w.is_visible().unwrap_or(false)) {
                let _ = win.hide();
                crate::webview_memory(&win, true);
                e.hid_hotswap = true;
                lines.push("HotSwap tucked away in the tray".to_string());
            }
        }
        (e, lines)
    }

    /// The game's monitor at its highest refresh rate, once the game has a window.
    fn refresh_display(&mut self, pids: &[u32]) -> Option<String> {
        if !self.want_refresh || self.display.is_some() {
            return None;
        }
        let (name, from, to) = display_max(game_window(pids)?)?;
        self.want_refresh = false;
        self.display = Some(name);
        Some(format!("The game's monitor runs at {to} Hz (was {from} Hz)"))
    }

    /// Puts the listed apps' processes in Efficiency mode, including ones started since last time.
    fn eco_refresh(&mut self, game: &[u32]) {
        if self.eco_names.is_empty() {
            return;
        }
        let sys = processes();
        for p in sys.processes().values() {
            let pid = p.pid().as_u32();
            if self.eco.contains(&pid) || game.contains(&pid) {
                continue;
            }
            if self.eco_names.iter().any(|n| p.name().to_string_lossy().eq_ignore_ascii_case(n)) && set_eco(pid, true) {
                self.eco.insert(pid);
            }
        }
    }

    fn undo(self, app: &AppHandle) {
        if let Some(d) = &self.display {
            display_reset(d);
        }
        BLOCK_WIN.lock().unwrap().clear();
        BLOCK_GAMEBAR.lock().unwrap().clear();
        for pid in &self.unthrottled {
            set_qos(*pid, None);
        }
        if let Some(k) = self.keys_before {
            keys_shortcuts_restore(k);
        }
        if let Some(w) = &self.wallpaper {
            let _ = Command::new(w).args(["-control", "play"]).spawn();
        }
        if self.hid_hotswap {
            if let Some(win) = app.get_webview_window("main").filter(|w| !w.is_visible().unwrap_or(true)) {
                crate::webview_memory(&win, false);
                let _ = win.show();
            }
        }
        for (key, name, before) in &self.reg {
            reg_restore(key, name, *before);
        }
        if self.reg.iter().any(|r| r.1 == "EnableTransparency") {
            broadcast_theme();
        }
        if let Some(m) = self.mouse_before {
            set_mouse(m);
        }
        for pid in self.eco {
            set_eco(pid, false);
        }
    }
}

impl Boost {
    /// New settings. A running boost moves to them right away.
    pub fn set(&self, app: &AppHandle, cfg: Config) {
        *self.cfg.lock().unwrap() = cfg;
        if self.active.lock().unwrap().is_some() {
            let app = app.clone();
            std::thread::spawn(move || {
                let boost = app.state::<Boost>();
                let mut guard = boost.active.lock().unwrap();
                if let Some(a) = guard.as_mut() {
                    let lines = boost.reconcile(&app, a, false);
                    let _ = app.emit("boost", Report { started: true, level: a.level, lines, status: a.status() });
                }
            });
        }
    }

    /// Called when the running game changes. Starts a boost for a new game, ends the old one.
    pub fn game_changed(&self, app: &AppHandle, game: Option<String>, pids_of: impl Fn(&str) -> Vec<u32>) {
        let ended = {
            let mut a = self.active.lock().unwrap();
            if a.as_ref().is_some_and(|a| Some(&a.game) != game.as_ref()) { a.take() } else { None }
        };
        if let Some(mut a) = ended {
            let lines = self.reconcile(app, &mut a, true);
            crate::logs::write("app", &format!("game boost off for {}", a.game));
            let _ = app.emit("boost", Report { started: false, level: 0, lines, status: vec![] });
        }
        let Some(game) = game else { return };
        let level = self.cfg.lock().unwrap().level_for(&game);
        if level == 0 || self.active.lock().unwrap().is_some() {
            return;
        }
        // The game's processes (some games run several) may take a moment to show up.
        let mut pids = Vec::new();
        for _ in 0..10 {
            pids = pids_of(&game);
            if !pids.is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        let mut a = Active { game: game.clone(), pids, level: 0, prio: 0, plan: 0, power_before: None, added_plan: None, apps_closed: false, apps_frozen: false, closed: vec![], frozen: vec![], battery: on_battery(), extras: None, adv: None };
        let lines = self.reconcile(app, &mut a, false);
        crate::logs::write("app", &format!("game boost {} for {game}: {}", GEARS[a.level as usize], lines.join("; ")));
        let _ = app.emit("boost", Report { started: true, level: a.level, lines, status: a.status() });
        *self.active.lock().unwrap() = Some(a);
        // Apps started while the game runs (a new browser tab is a new process) join Efficiency mode.
        let app = app.clone();
        std::thread::spawn(move || {
            for tick in 1u64.. {
                std::thread::sleep(Duration::from_secs(3));
                let boost = app.state::<Boost>();
                let mut guard = boost.active.lock().unwrap();
                match guard.as_mut() {
                    Some(a) if a.game == game => {
                        let pids = a.pids.clone();
                        let mut changed = None;
                        if let Some(e) = a.extras.as_mut() {
                            changed = e.refresh_display(&pids);
                            if tick % 3 == 0 {
                                e.eco_refresh(&pids);
                            }
                        }
                        if let Some(line) = changed {
                            save_state(&app, a);
                            let _ = app.emit("boost", Report { started: true, level: a.level, lines: vec![line], status: a.status() });
                        }
                        // Plugged in or unplugged: the gear follows.
                        if on_battery() != a.battery {
                            let lines = boost.reconcile(&app, a, false);
                            let _ = app.emit("boost", Report { started: true, level: a.level, lines, status: a.status() });
                        }
                    }
                    _ => break,
                }
            }
        });
    }

    /// Brings a boost to what the settings want now (`ending`: everything back), changing only
    /// what differs. Returns what changed.
    fn reconcile(&self, app: &AppHandle, a: &mut Active, ending: bool) -> Vec<String> {
        let cfg = self.cfg.lock().unwrap().clone();
        a.battery = on_battery();
        let level = if ending { 0 } else { cfg.level_for(&a.game) };
        let adv = cfg.gear(level);
        let adv = &adv;
        let mut lines = Vec::new();

        // Priority.
        let prio = if level == 0 { 0 } else { match adv.priority.as_str() { "normal" => 0, "high" => 2, _ => 1 } };
        if prio != a.prio {
            if a.pids.iter().fold(false, |any, &p| set_priority(p, prio) || any) && prio > 0 {
                lines.push(format!("The game runs at {} priority", if prio == 2 { "high" } else { "above normal" }));
            }
            a.prio = prio;
        }

        // Power plan.
        let plan = if level == 0 {
            0
        } else {
            match adv.plan.as_str() {
                "none" => 0,
                "high" => 1,
                "ultimate" => 2,
                _ => match level { 1 => 0, 2 | 3 => 1, _ => 2 },
            }
        };
        if plan != a.plan {
            if a.power_before.is_none() && plan != 0 {
                a.power_before = active_plan();
            }
            match plan {
                0 => {
                    if let Some(b) = a.power_before.take() {
                        let _ = run("powercfg", &["/setactive", &b]);
                        lines.push("Power plan put back".to_string());
                    }
                }
                1 => lines.push(if run("powercfg", &["/setactive", "SCHEME_MIN"]).is_some() {
                    "High performance power plan".to_string()
                } else {
                    "High performance power plan isn't available on this PC".to_string()
                }),
                _ => {
                    // Windows hides Ultimate Performance; it's added from its template for the game.
                    let p = a.added_plan.clone().or_else(|| run("powercfg", &["-duplicatescheme", ULTIMATE_TEMPLATE]).and_then(|o| guid_in(&o)));
                    if let Some(p) = p.filter(|p| run("powercfg", &["/setactive", p]).is_some()) {
                        a.added_plan = Some(p);
                        lines.push("Ultimate Performance power plan".to_string());
                    } else if run("powercfg", &["/setactive", "SCHEME_MIN"]).is_some() {
                        lines.push("Ultimate Performance isn't available here: High performance instead".to_string());
                    }
                }
            }
            if plan != 2 {
                if let Some(p) = a.added_plan.take() {
                    let _ = run("powercfg", &["/delete", &p]);
                }
            }
            a.plan = plan;
        }

        // Closing apps.
        let close = level > 0 && adv.close_apps;
        let sys = if (close && !a.apps_closed) || (level > 0 && adv.freeze_apps && !a.apps_frozen) { Some(processes()) } else { None };
        if close && !a.apps_closed {
            let sys = sys.as_ref().unwrap();
            let names: Vec<String> = cfg.close.iter().map(|n| n.to_lowercase()).filter(|n| !NEVER.contains(&n.as_str())).collect();
            let (mut freed, mut done) = (0u64, Vec::new());
            for name in &names {
                let procs: Vec<_> = sys.processes().values().filter(|p| p.name().to_string_lossy().to_lowercase() == *name && !a.pids.contains(&p.pid().as_u32())).collect();
                if procs.is_empty() {
                    continue;
                }
                freed += procs.iter().map(|p| p.memory()).sum::<u64>();
                if let Some(exe) = procs.iter().find_map(|p| p.exe().map(|e| e.to_path_buf())) {
                    a.closed.push(exe);
                }
                // Without /F: apps get the normal close message and can save first.
                let _ = run("taskkill", &["/IM", name]);
                done.push(name.trim_end_matches(".exe").to_string());
            }
            if !done.is_empty() {
                lines.push(format!("Closed {} (about {} freed)", done.join(", "), mb(freed)));
            }
            a.apps_closed = true;
        } else if !close && a.apps_closed {
            let closed = std::mem::take(&mut a.closed);
            if cfg.reopen && !closed.is_empty() {
                for exe in &closed {
                    let _ = Command::new(exe).current_dir(exe.parent().unwrap_or(exe)).spawn();
                }
                lines.push(format!("Reopened {}", closed.iter().filter_map(|e| e.file_stem()).map(|s| s.to_string_lossy().to_string()).collect::<Vec<_>>().join(", ")));
            }
            a.apps_closed = false;
        }

        // Freezing apps: they keep everything they had open, and are resumed after.
        let freezing = level > 0 && adv.freeze_apps;
        if freezing && !a.apps_frozen {
            let sys = sys.as_ref().unwrap();
            let freeze: Vec<String> = cfg.freeze.iter().map(|n| n.to_lowercase()).filter(|n| !NEVER.contains(&n.as_str())).collect();
            let mut frozen_names = Vec::new();
            for p in sys.processes().values() {
                let name = p.name().to_string_lossy().to_lowercase();
                let pid = p.pid().as_u32();
                if freeze.contains(&name) && !a.pids.contains(&pid) && suspend(pid, true) {
                    a.frozen.push((pid, p.exe().map(|e| e.to_path_buf()).unwrap_or_default()));
                    if !frozen_names.contains(&name) {
                        frozen_names.push(name);
                    }
                }
            }
            if !frozen_names.is_empty() {
                lines.push(format!("Froze {}", frozen_names.iter().map(|n| n.trim_end_matches(".exe")).collect::<Vec<_>>().join(", ")));
            }
            a.apps_frozen = true;
        } else if !freezing && a.apps_frozen {
            let n = a.frozen.len();
            for (pid, _) in std::mem::take(&mut a.frozen) {
                suspend(pid, false);
            }
            if n > 0 {
                lines.push("Frozen apps resumed".to_string());
            }
            a.apps_frozen = false;
        }

        // Advanced extras: redone from scratch when they change (they're quick and cheap).
        let want = (level > 0).then(|| adv.clone());
        if want != a.adv {
            if let Some(e) = a.extras.take() {
                e.undo(app);
                if want.is_none() {
                    lines.push("Windows settings put back".to_string());
                }
            }
            if let Some(w) = &want {
                let (mut e, l) = Extras::apply(app, w, &a.pids);
                e.eco_refresh(&a.pids);
                lines.extend(l);
                a.extras = Some(e);
            }
            a.adv = want;
        }

        a.level = level;
        save_state(app, a);
        lines
    }
}

#[derive(Serialize)]
pub struct App {
    name: String,
    mb: u64,
}

/// Running apps the user could close or slow down while playing, biggest first (one row per program).
#[tauri::command]
pub(crate) async fn boost_apps() -> Vec<App> {
    tauri::async_runtime::spawn_blocking(|| {
        let sys = processes();
        let mut by: std::collections::HashMap<String, u64> = Default::default();
        for p in sys.processes().values() {
            let Some(exe) = p.exe() else { continue };
            // Windows' own programs stay out of the list.
            if exe.to_string_lossy().to_lowercase().starts_with(r"c:\windows\") {
                continue;
            }
            let name = p.name().to_string_lossy().to_lowercase();
            if NEVER.contains(&name.as_str()) || !name.ends_with(".exe") {
                continue;
            }
            *by.entry(name).or_default() += p.memory();
        }
        let mut out: Vec<App> = by.into_iter().filter(|(_, m)| *m > 20 << 20).map(|(name, m)| App { name, mb: m >> 20 }).collect();
        out.sort_by(|a, b| b.mb.cmp(&a.mb));
        out.truncate(40);
        out
    })
    .await
    .unwrap_or_default()
}

#[derive(Serialize)]
pub struct Cleaned {
    freed_mb: u64,
    apps: usize,
}

/// Quick clean, by hand: asks background apps to hand back memory they hold but aren't using
/// (Windows' EmptyWorkingSet). The game and Windows' own processes are left alone. Apps reload
/// what they need when used again, so it's a one-off, not something to run all the time.
#[tauri::command]
pub(crate) async fn boost_clean(boost: tauri::State<'_, Boost>) -> Result<Cleaned, String> {
    let game: Vec<u32> = boost.active.lock().unwrap().as_ref().map(|a| a.pids.clone()).unwrap_or_default();
    tauri::async_runtime::spawn_blocking(move || {
        #[link(name = "kernel32")]
        extern "system" {
            fn K32EmptyWorkingSet(h: *mut core::ffi::c_void) -> i32;
        }
        let mut sys = processes();
        sys.refresh_memory();
        let before = sys.used_memory();
        let mut apps = 0;
        for p in sys.processes().values() {
            let name = p.name().to_string_lossy().to_lowercase();
            let pid = p.pid().as_u32();
            if game.contains(&pid) || NEVER.contains(&name.as_str()) || p.exe().is_some_and(|e| e.to_string_lossy().to_lowercase().starts_with(r"c:\windows\")) {
                continue;
            }
            #[cfg(windows)]
            unsafe {
                use windows_sys::Win32::{Foundation::CloseHandle, System::Threading::OpenProcess};
                const ACCESS: u32 = 0x0100 | 0x1000; // PROCESS_SET_QUOTA | PROCESS_QUERY_LIMITED_INFORMATION
                let h = OpenProcess(ACCESS, 0, pid);
                if !h.is_null() {
                    if K32EmptyWorkingSet(h) != 0 {
                        apps += 1;
                    }
                    CloseHandle(h);
                }
            }
        }
        std::thread::sleep(Duration::from_millis(500));
        sys.refresh_memory();
        let freed_mb = before.saturating_sub(sys.used_memory()) >> 20;
        crate::logs::write("app", &format!("quick clean: {freed_mb} MB from {apps} apps"));
        Cleaned { freed_mb, apps }
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) fn boost_set(app: AppHandle, boost: tauri::State<Boost>, cfg: Config) {
    boost.set(&app, cfg);
}
