//! Определение активного окна и полноэкранного режима для игрового режима.
//! Используются GetForegroundWindow + сравнение с прямоугольником монитора
//! и SHQueryUserNotificationState (эксклюзивный D3D / режим презентации).

use windows::core::PWSTR;
use windows::Win32::Foundation::{CloseHandle, HWND, RECT};
use windows::Win32::Graphics::Gdi::{GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST};
use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
use windows::Win32::UI::Shell::{SHQueryUserNotificationState, QUNS_PRESENTATION_MODE, QUNS_RUNNING_D3D_FULL_SCREEN};
use windows::Win32::UI::WindowsAndMessaging::{GetClassNameW, GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId};

use super::from_wide_buf;
use crate::platform::types::ForegroundInfo;

const SHELL_CLASSES: &[&str] = &["Progman", "WorkerW", "Shell_TrayWnd", "Shell_SecondaryTrayWnd", "Windows.UI.Core.CoreWindow"];

pub fn process_path(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(h);
        if ok {
            Some(String::from_utf16_lossy(&buf[..len as usize]))
        } else {
            None
        }
    }
}

pub fn hwnd_process(hwnd: HWND) -> Option<(u32, String)> {
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == 0 {
        return None;
    }
    process_path(pid).map(|p| (pid, p))
}

pub fn current() -> Option<ForegroundInfo> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_invalid() {
            return None;
        }
        let mut cls = [0u16; 256];
        let n = GetClassNameW(hwnd, &mut cls);
        let class = if n > 0 { from_wide_buf(&cls[..n as usize]) } else { String::new() };
        let is_shell = SHELL_CLASSES.contains(&class.as_str());

        let mut rect = RECT::default();
        let _ = GetWindowRect(hwnd, &mut rect);
        let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: std::mem::size_of::<MONITORINFO>() as u32, ..Default::default() };
        let fullscreen = GetMonitorInfoW(mon, &mut mi).as_bool()
            && rect.left <= mi.rcMonitor.left
            && rect.top <= mi.rcMonitor.top
            && rect.right >= mi.rcMonitor.right
            && rect.bottom >= mi.rcMonitor.bottom;

        let exclusive = matches!(SHQueryUserNotificationState(), Ok(s) if s == QUNS_RUNNING_D3D_FULL_SCREEN || s == QUNS_PRESENTATION_MODE);

        let (pid, path) = hwnd_process(hwnd).unwrap_or((0, String::new()));
        let exe_name = std::path::Path::new(&path).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        Some(ForegroundInfo { pid, exe_name, exe_path: path, fullscreen, exclusive_fullscreen: exclusive, is_shell })
    }
}
