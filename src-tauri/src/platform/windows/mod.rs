//! Адаптеры Windows (windows-rs): WinRT (SMTC, Radios, Bluetooth, уведомления)
//! и Win32 (Core Audio, WMI, DDC/CI, Shell, PDH, питание).
//!
//! Все функции блокирующие — вызываются из `spawn_blocking`.

pub mod apps;
pub mod audio;
pub mod bluetooth;
pub mod brightness;
pub mod clip_history;
pub mod clipboard;
pub mod dragdrop;
pub mod foreground;
pub mod gpu;
pub mod headphones;
pub mod keyboard;
pub mod media;
pub mod network;
pub mod notifications;
pub mod power;
pub mod privacy;
pub mod proxy;
pub mod quiet;
pub mod session;
pub mod shell;
pub mod unread;
pub mod vpn;
pub mod window;

use std::cell::Cell;
use windows::core::PWSTR;
use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, COINIT_APARTMENTTHREADED, COINIT_MULTITHREADED};

use super::types::EnvironmentInfo;

thread_local! {
    static COM_INIT: Cell<bool> = const { Cell::new(false) };
}

/// Инициализирует COM (MTA) в текущем потоке один раз.
pub fn ensure_com() {
    COM_INIT.with(|c| {
        if !c.get() {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            }
            c.set(true);
        }
    });
}

/// STA нужен объектам оболочки (Shell). Используется в выделенных потоках.
pub fn ensure_com_sta() {
    COM_INIT.with(|c| {
        if !c.get() {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            }
            c.set(true);
        }
    });
}

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Преобразует PWSTR, выделенный через CoTaskMemAlloc, в String и освобождает память.
pub unsafe fn take_pwstr(p: PWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    let s = p.to_string().unwrap_or_default();
    CoTaskMemFree(Some(p.0 as *const _));
    s
}

pub fn from_wide_buf(buf: &[u16]) -> String {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..end])
}

pub fn environment() -> EnvironmentInfo {
    let version = os_build();
    let mut notes = vec![];
    if version.1 < 22000 {
        notes.push("Windows 10: часть элементов оформления Windows 11 недоступна".to_string());
    }
    EnvironmentInfo {
        os: "windows".into(),
        os_version: format!("{}.{}.{}", version.0, 0, version.1),
        desktop: Some(if version.1 >= 22000 { "Windows 11".into() } else { "Windows 10".into() }),
        session_type: None,
        global_hotkeys_supported: true,
        notes,
    }
}

/// (major, build) из реестра — GetVersionEx врёт без манифеста совместимости.
fn os_build() -> (u32, u32) {
    let build = proxy::read_reg_string(
        windows::Win32::System::Registry::HKEY_LOCAL_MACHINE,
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion",
        "CurrentBuildNumber",
    )
    .and_then(|s| s.parse().ok())
    .unwrap_or(0);
    (10, build)
}
