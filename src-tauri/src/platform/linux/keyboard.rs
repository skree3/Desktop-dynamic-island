//! Caps Lock (светодиод в sysfs) и раскладка. Раскладку в Linux каждое окружение
//! хранит по-своему, поэтому показывается только там, где её можно надёжно узнать
//! (GNOME — через gsettings).

use crate::core::error::{AppError, AppResult};

pub fn caps() -> bool {
    std::fs::read_dir("/sys/class/leds")
        .map(|rd| {
            rd.flatten().any(|e| {
                e.file_name().to_string_lossy().contains("capslock")
                    && std::fs::read_to_string(e.path().join("brightness")).map(|v| v.trim() != "0").unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

/// GNOME: первая раскладка из mru-sources (текущая) → «RU».
pub fn layout() -> Option<String> {
    let out = std::process::Command::new("gsettings").args(["get", "org.gnome.desktop.input-sources", "mru-sources"]).output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let first = text.split("'xkb', '").nth(1)?;
    let code: String = first.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
    (!code.is_empty()).then(|| code.to_uppercase())
}

pub fn next_layout() -> AppResult<()> {
    xdotool("super+space")
}

pub fn toggle_caps() -> AppResult<()> {
    xdotool("Caps_Lock")
}

fn xdotool(key: &str) -> AppResult<()> {
    match std::process::Command::new("xdotool").args(["key", key]).status() {
        Ok(s) if s.success() => Ok(()),
        _ => Err(AppError::unsupported("нужна утилита xdotool (только X11)")),
    }
}
