//! Активное окно и полноэкранный режим через EWMH (X11 / XWayland):
//! _NET_ACTIVE_WINDOW, _NET_WM_STATE_FULLSCREEN, _NET_WM_PID.
//! Для нативных Wayland-окон эта информация недоступна (ограничение протокола) —
//! в этом случае игровой режим опирается на список игровых процессов.

use std::sync::Mutex;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt, Window};
use x11rb::rust_connection::RustConnection;

use crate::platform::types::ForegroundInfo;

struct X {
    conn: RustConnection,
    root: Window,
    active: u32,
    state: u32,
    fullscreen: u32,
    pid: u32,
    wtype: u32,
    desktop: u32,
}

static XCONN: Mutex<Option<X>> = Mutex::new(None);

fn atom(conn: &RustConnection, name: &str) -> Option<u32> {
    Some(conn.intern_atom(false, name.as_bytes()).ok()?.reply().ok()?.atom)
}

fn connect() -> Option<X> {
    std::env::var_os("DISPLAY")?;
    let (conn, screen) = x11rb::connect(None).ok()?;
    let root = conn.setup().roots.get(screen)?.root;
    Some(X {
        active: atom(&conn, "_NET_ACTIVE_WINDOW")?,
        state: atom(&conn, "_NET_WM_STATE")?,
        fullscreen: atom(&conn, "_NET_WM_STATE_FULLSCREEN")?,
        pid: atom(&conn, "_NET_WM_PID")?,
        wtype: atom(&conn, "_NET_WM_WINDOW_TYPE")?,
        desktop: atom(&conn, "_NET_WM_WINDOW_TYPE_DESKTOP")?,
        conn,
        root,
    })
}

fn prop32(x: &X, win: Window, prop: u32, ty: impl Into<u32>) -> Vec<u32> {
    x.conn
        .get_property(false, win, prop, ty.into(), 0, 64)
        .ok()
        .and_then(|c| c.reply().ok())
        .and_then(|r| r.value32().map(|v| v.collect()))
        .unwrap_or_default()
}

pub fn current() -> Option<ForegroundInfo> {
    let mut g = XCONN.lock().unwrap_or_else(|p| p.into_inner());
    if g.is_none() {
        *g = connect();
    }
    let x = g.as_ref()?;
    let win = match prop32(x, x.root, x.active, AtomEnum::WINDOW).first() {
        Some(&w) if w != 0 => w,
        _ => {
            return None;
        }
    };
    let states = prop32(x, win, x.state, AtomEnum::ATOM);
    let types = prop32(x, win, x.wtype, AtomEnum::ATOM);
    let pid = prop32(x, win, x.pid, AtomEnum::CARDINAL).first().copied().unwrap_or(0);
    let exe_path = if pid > 0 {
        std::fs::read_link(format!("/proc/{pid}/exe")).map(|p| p.to_string_lossy().to_string()).unwrap_or_default()
    } else {
        String::new()
    };
    let exe_name = std::path::Path::new(&exe_path).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let ok = x.conn.flush().is_ok();
    if !ok {
        *g = None;
        return None;
    }
    Some(ForegroundInfo {
        pid,
        exe_name,
        exe_path,
        fullscreen: states.contains(&x.fullscreen),
        exclusive_fullscreen: false,
        is_shell: types.contains(&x.desktop),
    })
}
