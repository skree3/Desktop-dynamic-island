//! Вспомогательные функции буфера обмена Windows:
//! * GetClipboardSequenceNumber — дешёвая проверка изменений без открытия буфера;
//! * форматы-маркеры чувствительных данных, которые выставляют менеджеры паролей
//!   (ExcludeClipboardContentFromMonitorProcessing, CanIncludeInClipboardHistory = 0,
//!   Clipboard Viewer Ignore) — такие данные не сохраняются;
//! * определение приложения-владельца для списка исключений.

use windows::core::PCWSTR;
use windows::Win32::System::DataExchange::{
    CloseClipboard, GetClipboardData, GetClipboardOwner, GetClipboardSequenceNumber, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatW,
};
use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::Foundation::HGLOBAL;

use super::{foreground, wide};

pub fn sequence() -> Option<u64> {
    Some(unsafe { GetClipboardSequenceNumber() } as u64)
}

fn format_id(name: &str) -> u32 {
    let w = wide(name);
    unsafe { RegisterClipboardFormatW(PCWSTR(w.as_ptr())) }
}

fn available(fmt: u32) -> bool {
    fmt != 0 && unsafe { IsClipboardFormatAvailable(fmt).is_ok() }
}

/// Читает DWORD из формата CanIncludeInClipboardHistory (0 = запрещено).
fn read_dword_format(fmt: u32) -> Option<u32> {
    unsafe {
        OpenClipboard(None).ok()?;
        let result = (|| {
            let h = GetClipboardData(fmt).ok()?;
            let g = HGLOBAL(h.0);
            if GlobalSize(g) < 4 {
                return None;
            }
            let p = GlobalLock(g) as *const u32;
            if p.is_null() {
                return None;
            }
            let v = *p;
            let _ = GlobalUnlock(g);
            Some(v)
        })();
        let _ = CloseClipboard();
        result
    }
}

pub fn is_sensitive() -> bool {
    if available(format_id("ExcludeClipboardContentFromMonitorProcessing")) || available(format_id("Clipboard Viewer Ignore")) {
        return true;
    }
    let can_include = format_id("CanIncludeInClipboardHistory");
    if available(can_include) {
        if let Some(0) = read_dword_format(can_include) {
            return true;
        }
    }
    false
}

/// Имя exe приложения, поместившего данные в буфер (или активного окна).
pub fn owner_app() -> Option<String> {
    let path = unsafe { GetClipboardOwner() }
        .ok()
        .filter(|h| !h.is_invalid())
        .and_then(foreground::hwnd_process)
        .map(|(_, p)| p)
        .or_else(|| foreground::current().map(|f| f.exe_path))?;
    std::path::Path::new(&path).file_name().map(|s| s.to_string_lossy().to_lowercase())
}
