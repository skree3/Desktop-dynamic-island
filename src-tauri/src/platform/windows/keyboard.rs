//! Раскладка клавиатуры и Caps Lock.
//! Раскладка берётся у потока активного окна (у каждого окна она своя), а
//! переключение отправляется туда же сообщением WM_INPUTLANGCHANGEREQUEST —
//! так же поступает сама Windows по Alt+Shift. Если активен сам остров,
//! используется последнее «чужое» окно.

use std::sync::atomic::{AtomicIsize, Ordering};
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::Globalization::{GetLocaleInfoW, LOCALE_SISO639LANGNAME};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, GetKeyboardLayout, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VK_CAPITAL,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId, IsWindow, PostMessageW, WM_INPUTLANGCHANGEREQUEST};

use crate::core::error::{AppError, AppResult};

static LAST_OTHER: AtomicIsize = AtomicIsize::new(0);

/// Активное окно другой программы (не острова и не настроек).
fn target_window() -> Option<HWND> {
    unsafe {
        let fg = GetForegroundWindow();
        if !fg.is_invalid() {
            let mut pid = 0u32;
            GetWindowThreadProcessId(fg, Some(&mut pid));
            if pid != std::process::id() {
                LAST_OTHER.store(fg.0 as isize, Ordering::Relaxed);
                return Some(fg);
            }
        }
        let last = HWND(LAST_OTHER.load(Ordering::Relaxed) as _);
        (!last.is_invalid() && IsWindow(Some(last)).as_bool()).then_some(last)
    }
}

/// Двухбуквенный код языка: 0x0419 → «RU».
fn lang_code(langid: u16) -> String {
    let mut buf = [0u16; 16];
    let n = unsafe { GetLocaleInfoW(langid as u32, LOCALE_SISO639LANGNAME, Some(&mut buf)) };
    if n > 1 {
        String::from_utf16_lossy(&buf[..(n as usize - 1)]).to_uppercase()
    } else {
        format!("{langid:04X}")
    }
}

/// Текущая раскладка активного окна.
pub fn layout() -> Option<String> {
    let hwnd = target_window()?;
    unsafe {
        let tid = GetWindowThreadProcessId(hwnd, None);
        let hkl = GetKeyboardLayout(tid);
        let langid = (hkl.0 as usize & 0xffff) as u16;
        (langid != 0).then(|| lang_code(langid))
    }
}

pub fn caps() -> bool {
    unsafe { (GetKeyState(VK_CAPITAL.0 as i32) & 1) != 0 }
}

/// Следующая раскладка в активном окне.
pub fn next_layout() -> AppResult<()> {
    let hwnd = target_window().ok_or_else(|| AppError::unsupported("нет активного окна для смены раскладки"))?;
    // lParam = HKL_NEXT (1): следующая раскладка из списка пользователя.
    unsafe { PostMessageW(Some(hwnd), WM_INPUTLANGCHANGEREQUEST, WPARAM(0), LPARAM(1)) }.map_err(|e| AppError::system(e.message()))?;
    Ok(())
}

fn key(vk: u16, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT { wVk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY(vk), wScan: 0x3a, dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }, time: 0, dwExtraInfo: 0 },
        },
    }
}

/// Переключает Caps Lock так же, как нажатие клавиши.
pub fn toggle_caps() -> AppResult<()> {
    let inputs = [key(VK_CAPITAL.0, false), key(VK_CAPITAL.0, true)];
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize != inputs.len() {
        return Err(AppError::system("система не приняла нажатие Caps Lock"));
    }
    Ok(())
}
