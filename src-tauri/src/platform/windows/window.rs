//! Атомарное перемещение + изменение размера окна (без мерцания) через SetWindowPos.

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_TRANSITIONS_FORCEDISABLED, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_DONOTROUND,
};

/// Отключает анимации DWM при показе/скрытии окна, системное скругление и рамку
/// Windows 11 — все анимации панели рисуем сами, плавно и коротко.
pub fn prepare_overlay(window: &tauri::WebviewWindow) {
    let Ok(hwnd) = window.hwnd() else { return };
    let hwnd = HWND(hwnd.0 as *mut _);
    unsafe {
        let disable: i32 = 1;
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_TRANSITIONS_FORCEDISABLED, &disable as *const _ as *const _, 4);
        let corner = DWMWCP_DONOTROUND;
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &corner as *const _ as *const _, 4);
        let no_border: u32 = 0xFFFF_FFFE; // DWMWA_COLOR_NONE
        let _ = DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR, &no_border as *const _ as *const _, 4);
    }
}
use windows::Win32::UI::WindowsAndMessaging::{SetWindowPos, SWP_NOACTIVATE, SWP_NOOWNERZORDER, SWP_NOZORDER};

pub fn set_bounds(window: &tauri::WebviewWindow, x: i32, y: i32, w: u32, h: u32) -> bool {
    let Ok(hwnd) = window.hwnd() else { return false };
    let hwnd = HWND(hwnd.0 as *mut _);
    unsafe { SetWindowPos(hwnd, None, x, y, w as i32, h as i32, SWP_NOZORDER | SWP_NOACTIVATE | SWP_NOOWNERZORDER).is_ok() }
}
