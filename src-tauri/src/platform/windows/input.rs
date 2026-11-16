//! Управление компьютером с телефона: мышь, клавиатура, системные действия
//! (блокировка, сон, выключение экрана). Ввод — через SendInput.

use std::mem::size_of;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::{GetCursorPos, GetSystemMetrics, SetCursorPos, SM_CXSCREEN, SM_CYSCREEN};

use crate::core::error::{AppError, AppResult};

fn send(inputs: &[INPUT]) {
    unsafe {
        SendInput(inputs, size_of::<INPUT>() as i32);
    }
}

fn mouse(flags: MOUSE_EVENT_FLAGS, dx: i32, dy: i32, data: i32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 { mi: MOUSEINPUT { dx, dy, mouseData: data as u32, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
    }
}

fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: vk, wScan: 0, dwFlags: if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }, time: 0, dwExtraInfo: 0 } },
    }
}

fn unicode(ch: u16, up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VIRTUAL_KEY(0), wScan: ch, dwFlags: KEYEVENTF_UNICODE | if up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) }, time: 0, dwExtraInfo: 0 } },
    }
}

pub fn mouse_move(dx: f64, dy: f64) {
    send(&[mouse(MOUSEEVENTF_MOVE, dx.round() as i32, dy.round() as i32, 0)]);
}

/// Кнопка мыши: left | right | middle; `double` — двойной щелчок.
pub fn click(button: &str, double: bool) {
    let (down, up) = match button {
        "right" => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
        "middle" => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
        _ => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
    };
    let one = [mouse(down, 0, 0, 0), mouse(up, 0, 0, 0)];
    send(&one);
    if double {
        send(&one);
    }
}

pub fn left_button(down: bool) {
    send(&[mouse(if down { MOUSEEVENTF_LEFTDOWN } else { MOUSEEVENTF_LEFTUP }, 0, 0, 0)]);
}

/// Прокрутка: положительное `dy` — вниз, `dx` — вправо (в «щелчках» колеса, дробные допустимы).
pub fn scroll(dx: f64, dy: f64) {
    if dy.abs() > 0.01 {
        send(&[mouse(MOUSEEVENTF_WHEEL, 0, 0, (-dy * 120.0).round() as i32)]);
    }
    if dx.abs() > 0.01 {
        send(&[mouse(MOUSEEVENTF_HWHEEL, 0, 0, (dx * 120.0).round() as i32)]);
    }
}

/// Перемещение указателя на долю экрана (указка в режиме презентации).
pub fn pointer_by_fraction(fx: f64, fy: f64) {
    unsafe {
        let mut p = Default::default();
        if GetCursorPos(&mut p).is_ok() {
            let w = GetSystemMetrics(SM_CXSCREEN) as f64;
            let h = GetSystemMetrics(SM_CYSCREEN) as f64;
            let _ = SetCursorPos((p.x as f64 + fx * w).round() as i32, (p.y as f64 + fy * h).round() as i32);
        }
    }
}

pub fn type_text(text: &str) {
    let mut inputs = Vec::new();
    for unit in text.encode_utf16() {
        inputs.push(unicode(unit, false));
        inputs.push(unicode(unit, true));
    }
    if !inputs.is_empty() {
        send(&inputs);
    }
}

/// Специальная клавиша в нумерации KDE Connect (1 Backspace … 32 F12) с модификаторами.
pub fn special_key(code: u32, ctrl: bool, alt: bool, shift: bool, win: bool) {
    let vk = match code {
        1 => VK_BACK,
        2 => VK_TAB,
        3 | 12 => VK_RETURN,
        4 => VK_LEFT,
        5 => VK_UP,
        6 => VK_RIGHT,
        7 => VK_DOWN,
        8 => VK_PRIOR,
        9 => VK_NEXT,
        10 => VK_HOME,
        11 => VK_END,
        13 => VK_DELETE,
        14 => VK_ESCAPE,
        15 => VK_SNAPSHOT,
        16 => VK_SCROLL,
        21..=32 => VIRTUAL_KEY(VK_F1.0 + (code - 21) as u16),
        _ => return,
    };
    let mods: Vec<VIRTUAL_KEY> = [(ctrl, VK_CONTROL), (alt, VK_MENU), (shift, VK_SHIFT), (win, VK_LWIN)].into_iter().filter(|(on, _)| *on).map(|(_, k)| k).collect();
    let mut inputs: Vec<INPUT> = mods.iter().map(|k| key(*k, false)).collect();
    inputs.push(key(vk, false));
    inputs.push(key(vk, true));
    inputs.extend(mods.iter().rev().map(|k| key(*k, true)));
    send(&inputs);
}

/// Обычная клавиша с модификаторами (Ctrl+C с телефона и т.п.).
pub fn key_with_mods(ch: char, ctrl: bool, alt: bool, shift: bool, win: bool) {
    if !(ctrl || alt || win) {
        type_text(&ch.to_string());
        return;
    }
    let vk = unsafe { VkKeyScanW(ch as u16) };
    if vk == -1 {
        return;
    }
    let vk = VIRTUAL_KEY((vk & 0xff) as u16);
    let mods: Vec<VIRTUAL_KEY> = [(ctrl, VK_CONTROL), (alt, VK_MENU), (shift, VK_SHIFT), (win, VK_LWIN)].into_iter().filter(|(on, _)| *on).map(|(_, k)| k).collect();
    let mut inputs: Vec<INPUT> = mods.iter().map(|k| key(*k, false)).collect();
    inputs.push(key(vk, false));
    inputs.push(key(vk, true));
    inputs.extend(mods.iter().rev().map(|k| key(*k, true)));
    send(&inputs);
}

/// Медиаклавиша «пауза/воспроизведение».
pub fn media_play_pause() {
    send(&[key(VK_MEDIA_PLAY_PAUSE, false), key(VK_MEDIA_PLAY_PAUSE, true)]);
}

pub fn lock() -> AppResult<()> {
    unsafe { windows::Win32::System::Shutdown::LockWorkStation() }.map_err(|e| AppError::system(format!("не удалось заблокировать: {e}")))
}

pub fn sleep() -> AppResult<()> {
    let ok = unsafe { windows::Win32::System::Power::SetSuspendState(false, false, false) };
    if ok { Ok(()) } else { Err(AppError::system("не удалось перевести компьютер в сон")) }
}

pub fn screen_off() -> AppResult<()> {
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, SC_MONITORPOWER, WM_SYSCOMMAND};
    // HWND_BROADCAST = 0xFFFF; PostMessage, чтобы не ждать ответа всех окон.
    unsafe { PostMessageW(Some(HWND(0xffff as _)), WM_SYSCOMMAND, WPARAM(SC_MONITORPOWER as usize), LPARAM(2)) }.map_err(|e| AppError::system(e.to_string()))
}
