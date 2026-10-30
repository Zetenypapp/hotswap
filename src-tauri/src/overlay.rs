//! The in-game overlay: one transparent window over the game for notes, the cheat sheet, the HUD
//! and the menu, plus the radar window. It never touches the game itself.

use super::*;

/// Where a window of size w x h goes on the main monitor: "top" (centered), "top-left",
/// "top-right", "bottom-left" or "bottom-right" (logical pixels; bottom stays clear of the taskbar).
pub(crate) fn corner_pos(app: &AppHandle, corner: &str, w: f64, h: f64, margin: f64) -> Result<(f64, f64), String> {
    let m = app.primary_monitor().ok().flatten().ok_or("no monitor")?;
    let s = m.scale_factor();
    let (mx, my) = (m.position().x as f64 / s, m.position().y as f64 / s);
    let (mw, mh) = (m.size().width as f64 / s, m.size().height as f64 / s);
    let x = if corner.ends_with("left") {
        mx + margin
    } else if corner.ends_with("right") {
        mx + mw - w - margin
    } else {
        mx + mw / 2.0 - w / 2.0
    };
    let y = if corner.starts_with("bottom") { my + mh - h - margin - 48.0 } else { my + margin };
    Ok((x, y))
}

/// The overlay: one transparent, always-on-top window over the main monitor that ignores the mouse
/// (except while its menu is open). Created on first use.
pub(crate) async fn overlay_window(app: &AppHandle) -> Result<tauri::WebviewWindow, String> {
    if let Some(win) = app.get_webview_window("overlay") {
        return Ok(win);
    }
    let m = app.primary_monitor().ok().flatten().ok_or("no monitor")?;
    let s = m.scale_factor();
    let (x, y) = (m.position().x as f64 / s, m.position().y as f64 / s);
    // One pixel short of the full height, so Windows doesn't treat it as a fullscreen app.
    let (w, h) = (m.size().width as f64 / s, m.size().height as f64 / s - 1.0);
    let win = tauri::WebviewWindowBuilder::new(app, "overlay", tauri::WebviewUrl::App("overlay.html".into()))
        .title("HotSwap overlay")
        .inner_size(w, h)
        .position(x, y)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .visible(false)
        .resizable(false)
        .build()
        .map_err(|e| e.to_string())?;
    let _ = win.set_ignore_cursor_events(true);
    // Wait until the page listens (it calls overlay_ready), so the first note or menu isn't lost.
    for _ in 0..100 {
        if OVERLAY_READY.load(Ordering::Relaxed) {
            break;
        }
        tokio_sleep(50).await;
    }
    Ok(win)
}

pub(crate) static OVERLAY_READY: AtomicBool = AtomicBool::new(false);
/// Low memory mode is on (read from the settings at startup).
pub(crate) static LOW_MEMORY: AtomicBool = AtomicBool::new(false);

/// Closes the overlay window after two minutes without notes, menu or HUD, freeing its memory;
/// the next note or menu opens it again.
pub(crate) fn overlay_idle_close(app: &AppHandle) {
    let id = OVERLAY_GEN.fetch_add(1, Ordering::Relaxed) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio_sleep(if LOW_MEMORY.load(Ordering::Relaxed) { 20_000 } else { 120_000 }).await;
        if OVERLAY_GEN.load(Ordering::Relaxed) == id && !OVERLAY_KEEP.load(Ordering::Relaxed) && !OVERLAY_MENU.load(Ordering::Relaxed) {
            if let Some(win) = app.get_webview_window("overlay") {
                OVERLAY_READY.store(false, Ordering::Relaxed);
                let _ = win.destroy();
            }
        }
    });
}

#[tauri::command]
pub(crate) fn overlay_ready() {
    OVERLAY_READY.store(true, Ordering::Relaxed);
}

/// The overlay window covers only the part of the screen with something on it (x, y, width and
/// height in logical pixels from the main monitor's top-left), so the game keeps the screen to
/// itself where nothing is drawn. The page works out the part (see overlay.html).
#[tauri::command]
pub(crate) fn overlay_fit(app: AppHandle, x: f64, y: f64, w: f64, h: f64) {
    let Some(win) = app.get_webview_window("overlay") else { return };
    let Some(m) = app.primary_monitor().ok().flatten() else { return };
    let s = m.scale_factor();
    let (mx, my) = (m.position().x as f64 / s, m.position().y as f64 / s);
    let _ = win.set_position(tauri::LogicalPosition::new(mx + x, my + y));
    let _ = win.set_size(tauri::LogicalSize::new(w.max(1.0), h.max(1.0)));
}

/// Shows the overlay without taking focus from the game.
pub(crate) fn overlay_show(win: &tauri::WebviewWindow) {
    #[cfg(windows)]
    if let Ok(hwnd) = win.hwnd() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_SHOWNOACTIVATE};
        unsafe { ShowWindow(hwnd.0 as _, SW_SHOWNOACTIVATE) };
        return;
    }
    let _ = win.show();
}

/// Overlay state: kept visible (HUD on or menu open), menu open, and the window the menu took focus from.
pub(crate) static OVERLAY_KEEP: AtomicBool = AtomicBool::new(false);
pub(crate) static OVERLAY_MENU: AtomicBool = AtomicBool::new(false);
pub(crate) static OVERLAY_PREV: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

/// Sends something to the overlay: a mod change (default), or `kind` "sheet", "hud" or "menu".
/// `keep` says whether something stays on screen (HUD or menu); otherwise the window hides itself
/// once notes are gone: a see-through window kept over a game can make Windows drop the game's
/// fast fullscreen path, which shows up as stutter.
#[tauri::command]
pub(crate) async fn overlay_notify(app: AppHandle, note: serde_json::Value) -> Result<(), String> {
    if let Some(k) = note.get("keep").and_then(|k| k.as_bool()) {
        OVERLAY_KEEP.store(k, Ordering::Relaxed);
    }
    let sheet_open = note.get("kind").and_then(|k| k.as_str()) == Some("sheet") && note.get("show").and_then(|v| v.as_bool()) == Some(true);
    let win = overlay_window(&app).await?;
    overlay_show(&win);
    app.emit_to("overlay", "overlay", note).map_err(|e| e.to_string())?;
    let id = OVERLAY_GEN.fetch_add(1, Ordering::Relaxed) + 1;
    let hold = if sheet_open { 10_500 } else { 3_000 };
    tauri::async_runtime::spawn(async move {
        tokio_sleep(hold).await;
        if OVERLAY_GEN.load(Ordering::Relaxed) == id && !OVERLAY_KEEP.load(Ordering::Relaxed) && !OVERLAY_MENU.load(Ordering::Relaxed) {
            if let Some(win) = app.get_webview_window("overlay") {
                let _ = win.hide();
            }
            overlay_idle_close(&app);
        }
    });
    Ok(())
}

/// Opens or closes the overlay menu: it takes the mouse and keyboard while open, then gives focus
/// back to the game.
#[tauri::command]
pub(crate) async fn overlay_menu(app: AppHandle, open: bool) -> Result<(), String> {
    let win = overlay_window(&app).await?;
    #[cfg(windows)]
    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
    if open {
        #[cfg(windows)]
        {
            let fg = unsafe { GetForegroundWindow() } as isize;
            if win.hwnd().map_or(true, |h| h.0 as isize != fg) {
                OVERLAY_PREV.store(fg, Ordering::Relaxed);
            }
        }
        OVERLAY_MENU.store(true, Ordering::Relaxed);
        // The menu needs the whole screen (it dims the game behind it).
        if let Some(m) = app.primary_monitor().ok().flatten() {
            let sc = m.scale_factor();
            overlay_fit(app.clone(), 0.0, 0.0, m.size().width as f64 / sc, m.size().height as f64 / sc - 1.0);
        }
        let _ = win.set_ignore_cursor_events(false);
        let _ = win.show();
        let _ = win.set_focus();
    } else {
        OVERLAY_MENU.store(false, Ordering::Relaxed);
        let _ = win.set_ignore_cursor_events(true);
        let keep = OVERLAY_KEEP.load(Ordering::Relaxed);
        // Hide first, so the overlay can't keep the focus the game needs back.
        if !keep {
            let _ = win.hide();
            overlay_idle_close(&app);
        }
        #[cfg(windows)]
        {
            let prev = OVERLAY_PREV.swap(0, Ordering::Relaxed);
            if prev != 0 {
                give_focus(prev);
            }
        }
    }
    Ok(())
}

/// Gives keyboard and mouse focus back to a window (the game). Windows can refuse a plain
/// SetForegroundWindow, so this joins the game's input thread for the switch, as tools that
/// return focus to games do.
#[cfg(windows)]
pub(crate) fn give_focus(hwnd: isize) {
    use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows_sys::Win32::UI::WindowsAndMessaging::{BringWindowToTop, GetForegroundWindow, GetWindowThreadProcessId, SetForegroundWindow, ShowWindow, SW_SHOW};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    unsafe {
        let target = hwnd as _;
        let fg_thread = GetWindowThreadProcessId(GetForegroundWindow(), std::ptr::null_mut());
        let game_thread = GetWindowThreadProcessId(target, std::ptr::null_mut());
        let me = GetCurrentThreadId();
        let joined: Vec<u32> = [fg_thread, game_thread].into_iter().filter(|&t| t != 0 && t != me).collect();
        for &t in &joined {
            AttachThreadInput(me, t, 1);
        }
        ShowWindow(target, SW_SHOW);
        BringWindowToTop(target);
        SetForegroundWindow(target);
        SetFocus(target);
        for &t in &joined {
            AttachThreadInput(me, t, 0);
        }
    }
}

/// Clicks and keys from the overlay menu, passed on to the main window, which owns the mods.
#[tauri::command]
pub(crate) fn overlay_input(app: AppHandle, input: serde_json::Value) -> Result<(), String> {
    app.emit_to("main", "overlay-input", input).map_err(|e| e.to_string())
}

/// Bumped on every overlay note; a pending hide only runs if no newer note came.
pub(crate) static OVERLAY_GEN: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub(crate) async fn tokio_sleep(ms: u64) {
    let _ = tauri::async_runtime::spawn_blocking(move || std::thread::sleep(std::time::Duration::from_millis(ms))).await;
}

/// Shows, moves or closes the radar: a small transparent, click-through window in a corner.
#[tauri::command]
pub(crate) async fn radar_show(app: AppHandle, show: bool, corner: Option<String>) -> Result<(), String> {
    let existing = app.get_webview_window("radar");
    if !show {
        if let Some(w) = existing {
            let _ = w.close();
        }
        return Ok(());
    }
    let size = 260.0;
    let (x, y) = corner_pos(&app, corner.as_deref().unwrap_or("top-right"), size, size, 24.0)?;
    if let Some(w) = existing {
        let _ = w.set_position(tauri::LogicalPosition::new(x, y));
        return Ok(());
    }
    let win = tauri::WebviewWindowBuilder::new(&app, "radar", tauri::WebviewUrl::App("radar.html".into()))
        .title("HotSwap radar")
        .inner_size(size, size)
        .position(x, y)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .focused(false)
        .resizable(false)
        .build()
        .map_err(|e| e.to_string())?;
    let _ = win.set_ignore_cursor_events(true);
    Ok(())
}

#[tauri::command]
pub(crate) fn overlay_close(app: AppHandle) {
    if let Some(w) = app.get_webview_window("overlay") {
        let _ = w.close();
    }
}
