//! Frames per second for any game, from Windows' own graphics events (ETW), the way PresentMon and
//! Xbox Game Bar read them: every frame a game shows sends a "present" event. Nothing touches the
//! game. Windows only lets admins and the Performance Log Users group start the event session, so
//! `allow` asks once (with Windows' admin prompt) to add the user to that group.

use std::{
    collections::{HashMap, VecDeque},
    sync::Mutex,
    time::{Duration, Instant},
};

const SESSION: &str = "HotSwap FPS";
// Providers (data1 of their GUIDs) and the event IDs of a present, from PresentMon's headers.
const DXGI: u32 = 0xCA11C036; // Present_Start = 42 (DirectX 10, 11 and 12)
const D3D9: u32 = 0x783ACA0A; // Present_Start = 1
const DXGKRNL: u32 = 0x802EC45A; // PresentHistory_Start = 171, PresentHistoryDetailed_Start = 215 (any API, Vulkan too)

#[derive(Clone, Copy, PartialEq)]
enum Status {
    Off,
    Running,
    Denied,
    Failed,
}

struct State {
    status: Status,
    /// Present times (system time) per process for the last two seconds: from the graphics API, and from
    /// the kernel's two present events (games on DirectX send both API and kernel events).
    frames: HashMap<u32, [VecDeque<i64>; 3]>,
    asked: Instant,
    tried: Option<Instant>,
}

static STATE: Mutex<Option<State>> = Mutex::new(None);

/// The time now on the events' clock (system time, in 100 ns ticks), and ticks per second.
fn clock() -> (i64, i64) {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetSystemTimePreciseAsFileTime(t: *mut i64);
    }
    let mut now = 0i64;
    unsafe { GetSystemTimePreciseAsFileTime(&mut now) };
    (now, 10_000_000)
}

#[derive(serde::Serialize, Clone, Copy, Default)]
pub struct Fps {
    pub fps: f32,
    /// The 1% low: the frame rate the slowest 1% of frames ran at.
    pub low: f32,
    /// Milliseconds per frame, on average.
    pub ms: f32,
}

/// The game's frame rate over the last second, starting the event session on first use. None until
/// frames arrive, or while Windows refuses the session (see `status`).
pub fn read(pid: u32) -> Option<Fps> {
    ensure();
    let g = STATE.lock().unwrap();
    let s = g.as_ref()?;
    let q = s.frames.get(&pid)?;
    // API events when the game sends them, else the busier of the kernel's two.
    let list = if !q[0].is_empty() { &q[0] } else if q[1].len() >= q[2].len() { &q[1] } else { &q[2] };
    // Events arrive up to a second late, so the second counted is the one before the newest frame;
    // none for three seconds means the game stopped showing frames.
    let (now, freq) = clock();
    let last = *list.back()?;
    if now - last > 3 * freq {
        return None;
    }
    let recent: Vec<i64> = list.iter().copied().filter(|&t| last - t < 2 * freq).collect();
    if recent.len() < 2 {
        return None;
    }
    let fps = recent.iter().filter(|&&t| last - t < freq).count() as f32;
    let mut gaps: Vec<f32> = recent.windows(2).map(|w| (w[1] - w[0]) as f32 * 1000.0 / freq as f32).collect();
    let ms = gaps.iter().sum::<f32>() / gaps.len() as f32;
    gaps.sort_by(|a, b| b.total_cmp(a));
    let worst = gaps[(gaps.len() / 100).min(gaps.len() - 1)];
    Some(Fps { fps, low: if worst > 0.0 { 1000.0 / worst } else { fps }, ms })
}

/// "ok", "starting", "denied" (needs `allow` and signing in again) or "failed".
pub fn status() -> &'static str {
    ensure();
    match STATE.lock().unwrap().as_ref().map(|s| s.status) {
        Some(Status::Running) => "ok",
        Some(Status::Denied) => "denied",
        Some(Status::Failed) => "failed",
        _ => "starting",
    }
}

/// Starts the session if it isn't running (a refused start is retried after a minute), and keeps
/// it alive: it stops by itself 20 seconds after the last read.
fn ensure() {
    let mut g = STATE.lock().unwrap();
    let s = g.get_or_insert_with(|| State { status: Status::Off, frames: HashMap::new(), asked: Instant::now(), tried: None });
    s.asked = Instant::now();
    if s.status == Status::Running || s.tried.is_some_and(|t| t.elapsed() < Duration::from_secs(60)) {
        return;
    }
    s.tried = Some(Instant::now());
    s.status = match start() {
        Ok(()) => Status::Running,
        Err(5) => Status::Denied, // ERROR_ACCESS_DENIED
        Err(e) => {
            crate::logs::write("app", &format!("fps: event session failed ({e})"));
            Status::Failed
        }
    };
    if s.status == Status::Running {
        s.tried = None;
        std::thread::spawn(|| loop {
            std::thread::sleep(Duration::from_secs(5));
            let mut g = STATE.lock().unwrap();
            let Some(s) = g.as_mut() else { return };
            if s.asked.elapsed() <= Duration::from_secs(20) {
                // Games that closed.
                let (now, freq) = clock();
                s.frames.retain(|_, q| q.iter().any(|l| l.back().is_some_and(|&t| now - t < 5 * freq)));
                continue;
            }
            stop();
            s.status = Status::Off;
            s.frames.clear();
            return;
        });
    }
}

#[cfg(windows)]
mod etw {
    pub use windows_sys::Win32::System::Diagnostics::Etw::*;

    /// EVENT_TRACE_PROPERTIES with room for the session name after it, as Windows wants.
    pub fn props(name_len: usize) -> Vec<u64> {
        let size = std::mem::size_of::<EVENT_TRACE_PROPERTIES>() + name_len * 2;
        let mut buf = vec![0u64; size.div_ceil(8)];
        let p = buf.as_mut_ptr() as *mut EVENT_TRACE_PROPERTIES;
        unsafe {
            (*p).Wnode.BufferSize = size as u32;
            (*p).Wnode.ClientContext = 2; // system time stamps, the clock clock() reads
            (*p).Wnode.Flags = WNODE_FLAG_TRACED_GUID;
            (*p).LogFileMode = EVENT_TRACE_REAL_TIME_MODE;
            (*p).FlushTimer = 1;
            (*p).LoggerNameOffset = std::mem::size_of::<EVENT_TRACE_PROPERTIES>() as u32;
        }
        buf
    }
}

fn session_name() -> Vec<u16> {
    SESSION.encode_utf16().chain([0]).collect()
}

/// Stops the session (also one left over from a crash).
fn stop() {
    #[cfg(windows)]
    unsafe {
        use etw::*;
        let name = session_name();
        let mut p = etw::props(name.len());
        ControlTraceW(CONTROLTRACE_HANDLE { Value: 0 }, name.as_ptr(), p.as_mut_ptr() as _, EVENT_TRACE_CONTROL_STOP);
    }
}

#[cfg(windows)]
unsafe extern "system" fn on_event(r: *mut etw::EVENT_RECORD) {
    let h = &(*r).EventHeader;
    let slot = match (h.ProviderId.data1, h.EventDescriptor.Id) {
        (DXGI, 42) | (D3D9, 1) => 0,
        (DXGKRNL, 171) => 1,
        (DXGKRNL, 215) => 2,
        _ => return,
    };
    // Never waits: a frame missed while a read holds the lock for a moment doesn't matter, and
    // the session may be stopped by a thread holding it.
    let Ok(mut g) = STATE.try_lock() else { return };
    let Some(s) = g.as_mut() else { return };
    let (t, freq) = (h.TimeStamp, clock().1);
    let q = &mut s.frames.entry(h.ProcessId).or_default()[slot];
    q.push_back(t);
    while q.front().is_some_and(|&f| t - f > 2 * freq) {
        q.pop_front();
    }
}

fn start() -> Result<(), u32> {
    #[cfg(windows)]
    unsafe {
        use etw::*;
        let mut name = session_name();
        let mut p = props(name.len());
        let mut h = CONTROLTRACE_HANDLE { Value: 0 };
        let mut r = StartTraceW(&mut h, name.as_ptr(), p.as_mut_ptr() as _);
        if r == 183 {
            // ERROR_ALREADY_EXISTS: left over from a crash.
            stop();
            p = props(name.len());
            r = StartTraceW(&mut h, name.as_ptr(), p.as_mut_ptr() as _);
        }
        if r != 0 {
            return Err(r);
        }
        let guid = |u: u128| windows_sys::core::GUID::from_u128(u);
        for (g, keywords) in [
            (guid(0xCA11C036_0102_4A2D_A6AD_F03CFED5D3C9), 0),
            (guid(0x783ACA0A_790E_4D7F_8451_AA850511C6B9), 0),
            (guid(0x802EC45A_1E99_4B83_9920_87C98277BA9D), 0x800_0000), // the Present keyword only: the rest is busy
        ] {
            EnableTraceEx2(h, &g, EVENT_CONTROL_CODE_ENABLE_PROVIDER, 5, keywords, 0, 0, std::ptr::null());
        }
        let mut log: EVENT_TRACE_LOGFILEW = std::mem::zeroed();
        log.LoggerName = name.as_mut_ptr();
        log.Anonymous1.ProcessTraceMode = PROCESS_TRACE_MODE_REAL_TIME | PROCESS_TRACE_MODE_EVENT_RECORD;
        log.Anonymous2.EventRecordCallback = Some(on_event);
        let t = OpenTraceW(&mut log);
        if t.Value == u64::MAX {
            stop();
            return Err(windows_sys::Win32::Foundation::GetLastError());
        }
        std::thread::spawn(move || {
            ProcessTrace(&t, 1, std::ptr::null(), std::ptr::null());
            CloseTrace(t);
        });
        return Ok(());
    }
    #[allow(unreachable_code)]
    Err(0)
}

/// Asks Windows (with its admin prompt) to add this user to the Performance Log Users group, which
/// may read graphics events without admin rights, like Xbox Game Bar's FPS counter asks once.
/// Takes effect after signing out and in again. The prompt runs HotSwap itself with
/// `--fps-allow <your account's SID>`, which does only that (allow_elevated), through Windows' API.
#[tauri::command]
pub fn fps_allow() -> Result<(), String> {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::{
            Foundation::{CloseHandle, WAIT_OBJECT_0},
            System::Threading::{GetExitCodeProcess, WaitForSingleObject},
            UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW},
        };
        let sid = user_sid().ok_or("Couldn't read your Windows account")?;
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
        let (verb, file, args) = (wide("runas"), wide(&exe.to_string_lossy()), wide(&format!("--fps-allow {sid}")));
        let mut info: SHELLEXECUTEINFOW = std::mem::zeroed();
        info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        info.fMask = SEE_MASK_NOCLOSEPROCESS;
        info.lpVerb = verb.as_ptr();
        info.lpFile = file.as_ptr();
        info.lpParameters = args.as_ptr();
        if ShellExecuteExW(&mut info) == 0 || info.hProcess.is_null() {
            return Err("Windows didn't allow it".into());
        }
        let mut code = 1u32;
        let done = WaitForSingleObject(info.hProcess, 60_000) == WAIT_OBJECT_0 && GetExitCodeProcess(info.hProcess, &mut code) != 0;
        CloseHandle(info.hProcess);
        return if done && code == 0 { Ok(()) } else { Err("Windows didn't allow it".into()) };
    }
    #[allow(unreachable_code)]
    Err("Windows only".into())
}

/// This account's SID, e.g. "S-1-5-21-…".
#[cfg(windows)]
fn user_sid() -> Option<String> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, LocalFree},
        Security::{Authorization::ConvertSidToStringSidW, GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER},
        System::Threading::{GetCurrentProcess, OpenProcessToken},
    };
    unsafe {
        let mut token = std::ptr::null_mut();
        if OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) == 0 {
            return None;
        }
        let mut buf = vec![0u8; 256];
        let mut len = 0u32;
        let ok = GetTokenInformation(token, TokenUser, buf.as_mut_ptr().cast(), buf.len() as u32, &mut len) != 0;
        CloseHandle(token);
        if !ok {
            return None;
        }
        let user = &*(buf.as_ptr() as *const TOKEN_USER);
        let mut text = std::ptr::null_mut();
        if ConvertSidToStringSidW(user.User.Sid, &mut text) == 0 {
            return None;
        }
        let n = (0..).take_while(|&i| *text.add(i) != 0).count();
        let sid = String::from_utf16_lossy(std::slice::from_raw_parts(text, n));
        LocalFree(text.cast());
        Some(sid)
    }
}

/// The elevated half of fps_allow (`hotswap --fps-allow <SID>`): adds that account to Performance
/// Log Users (S-1-5-32-559, the same group in every Windows language). Exit code 0 when it's in.
#[cfg(windows)]
pub fn allow_elevated(sid: &str) -> i32 {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        NetworkManagement::NetManagement::{NetLocalGroupAddMembers, LOCALGROUP_MEMBERS_INFO_0},
        Security::{Authorization::ConvertStringSidToSidW, LookupAccountSidW},
    };
    // Only an account SID, nothing else.
    if !sid.starts_with("S-1-5-") || sid.len() > 184 || !sid.bytes().all(|b| b.is_ascii_digit() || b == b'-' || b == b'S') {
        return 2;
    }
    let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
    unsafe {
        let (mut user, mut group) = (std::ptr::null_mut(), std::ptr::null_mut());
        if ConvertStringSidToSidW(wide(sid).as_ptr(), &mut user) == 0 || ConvertStringSidToSidW(wide("S-1-5-32-559").as_ptr(), &mut group) == 0 {
            return 3;
        }
        // The group's name in this Windows' language.
        let (mut name, mut domain) = ([0u16; 256], [0u16; 256]);
        let (mut nlen, mut dlen, mut kind) = (256u32, 256u32, 0);
        let found = LookupAccountSidW(std::ptr::null(), group, name.as_mut_ptr(), &mut nlen, domain.as_mut_ptr(), &mut dlen, &mut kind) != 0;
        let member = LOCALGROUP_MEMBERS_INFO_0 { lgrmi0_sid: user };
        // 0 = added, 1378 = already a member.
        let r = if found { NetLocalGroupAddMembers(std::ptr::null(), name.as_ptr(), 0, (&member as *const LOCALGROUP_MEMBERS_INFO_0).cast(), 1) } else { 4 };
        LocalFree(user);
        LocalFree(group);
        if r == 0 || r == 1378 { 0 } else { 5 }
    }
}

/// "ok", "starting", "denied" or "failed", for Settings.
#[tauri::command]
pub fn fps_status() -> &'static str {
    status()
}

/// PowerShell's -EncodedCommand: the script as UTF-16LE in base64.
/// Dev aid (`hotswap --fps <exe name>`): the app's frame rate once a second for ten seconds.
pub fn test(exe: &str) -> String {
    let sys = sysinfo::System::new_all();
    let pids: Vec<u32> = sys.processes().values().filter(|p| p.name().to_string_lossy().eq_ignore_ascii_case(exe)).map(|p| p.pid().as_u32()).collect();
    if pids.is_empty() {
        return format!("{exe} isn't running");
    }
    let mut out = String::new();
    for _ in 0..10 {
        // Apps with several processes (browsers) present from one of them.
        let f = pids.iter().filter_map(|&p| read(p)).max_by(|a, b| a.fps.total_cmp(&b.fps));
        out += &format!("{}: {}\n", status(), f.map_or("no frames".into(), |f| format!("{:.0} fps, 1% low {:.0}, {:.1} ms", f.fps, f.low, f.ms)));
        std::thread::sleep(Duration::from_secs(1));
    }
    // Everything that presented, to see which events arrive at all.
    if let Some(s) = STATE.lock().unwrap().as_ref() {
        for (pid, q) in &s.frames {
            let name = sys.process(sysinfo::Pid::from_u32(*pid)).map(|p| p.name().to_string_lossy().to_string()).unwrap_or_default();
            out += &format!("  {pid} {name}: api {} kernel {} / {}\n", q[0].len(), q[1].len(), q[2].len());
        }
    }
    out
}
