//! Управление компьютером с телефона на Linux: через xdotool (X11), если он установлен.
//! В Wayland программам нельзя вводить мышь и клавиатуру — команды просто не выполняются.

use crate::core::error::{AppError, AppResult};

fn xdotool(args: &[&str]) {
    let _ = std::process::Command::new("xdotool").args(args).status();
}

pub fn mouse_move(dx: f64, dy: f64) {
    xdotool(&["mousemove_relative", "--", &format!("{}", dx.round() as i32), &format!("{}", dy.round() as i32)]);
}

pub fn click(button: &str, double: bool) {
    let b = match button {
        "right" => "3",
        "middle" => "2",
        _ => "1",
    };
    if double {
        xdotool(&["click", "--repeat", "2", b]);
    } else {
        xdotool(&["click", b]);
    }
}

pub fn left_button(down: bool) {
    xdotool(&[if down { "mousedown" } else { "mouseup" }, "1"]);
}

pub fn scroll(_dx: f64, dy: f64) {
    if dy.abs() > 0.01 {
        xdotool(&["click", if dy > 0.0 { "5" } else { "4" }]);
    }
}

pub fn pointer_by_fraction(_fx: f64, _fy: f64) {}

pub fn type_text(text: &str) {
    xdotool(&["type", "--", text]);
}

pub fn special_key(code: u32, ctrl: bool, alt: bool, shift: bool, win: bool) {
    let k = match code {
        1 => "BackSpace",
        2 => "Tab",
        3 | 12 => "Return",
        4 => "Left",
        5 => "Up",
        6 => "Right",
        7 => "Down",
        8 => "Prior",
        9 => "Next",
        10 => "Home",
        11 => "End",
        13 => "Delete",
        14 => "Escape",
        21..=32 => return xdotool(&["key", &format!("F{}", code - 20)]),
        _ => return,
    };
    let mut combo = String::new();
    for (on, m) in [(ctrl, "ctrl+"), (alt, "alt+"), (shift, "shift+"), (win, "super+")] {
        if on {
            combo.push_str(m);
        }
    }
    combo.push_str(k);
    xdotool(&["key", &combo]);
}

pub fn key_with_mods(ch: char, ctrl: bool, alt: bool, shift: bool, win: bool) {
    if !(ctrl || alt || win) {
        return type_text(&ch.to_string());
    }
    let mut combo = String::new();
    for (on, m) in [(ctrl, "ctrl+"), (alt, "alt+"), (shift, "shift+"), (win, "super+")] {
        if on {
            combo.push_str(m);
        }
    }
    combo.push(ch);
    xdotool(&["key", &combo]);
}

pub fn media_play_pause() {
    xdotool(&["key", "XF86AudioPlay"]);
}

pub fn lock() -> AppResult<()> {
    let ok = std::process::Command::new("loginctl").arg("lock-session").status().map(|s| s.success()).unwrap_or(false);
    if ok { Ok(()) } else { Err(AppError::unsupported("блокировка недоступна")) }
}

pub fn sleep() -> AppResult<()> {
    let ok = std::process::Command::new("systemctl").arg("suspend").status().map(|s| s.success()).unwrap_or(false);
    if ok { Ok(()) } else { Err(AppError::unsupported("сон недоступен")) }
}

pub fn screen_off() -> AppResult<()> {
    let ok = std::process::Command::new("xset").args(["dpms", "force", "off"]).status().map(|s| s.success()).unwrap_or(false);
    if ok { Ok(()) } else { Err(AppError::unsupported("выключение экрана недоступно")) }
}
