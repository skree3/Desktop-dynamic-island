//! Яркость: чтение из /sys/class/backlight, запись через logind
//! (org.freedesktop.login1.Session.SetBrightness) — без прав root.
//! Внешние мониторы (DDC/CI) требуют доступа к /dev/i2c-* и не поддерживаются.

use std::path::Path;

use super::{block, proxy, read_trim, system_bus};
use crate::core::error::{AppError, AppResult};
use crate::platform::types::{BrightnessState, DisplayBrightness};

const BASE: &str = "/sys/class/backlight";

pub fn state() -> AppResult<BrightnessState> {
    let mut displays = vec![];
    if let Ok(entries) = std::fs::read_dir(BASE) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let dir = e.path();
            let cur: Option<u64> = read_trim(dir.join("brightness")).and_then(|s| s.parse().ok());
            let max: Option<u64> = read_trim(dir.join("max_brightness")).and_then(|s| s.parse().ok());
            if let (Some(cur), Some(max)) = (cur, max) {
                if max > 0 {
                    displays.push(DisplayBrightness {
                        id: name.clone(),
                        name: format!("Встроенный дисплей ({name})"),
                        value: ((cur as f64 / max as f64) * 100.0).round() as u32,
                        method: "sysfs + logind".into(),
                    });
                }
            }
        }
    }
    let error = if displays.is_empty() {
        Some("Устройства подсветки не найдены (/sys/class/backlight пуст). Для внешних мониторов программная регулировка недоступна.".into())
    } else {
        None
    };
    Ok(BrightnessState { displays, error })
}

pub fn set(id: &str, value: u32) -> AppResult<()> {
    if id.contains('/') || id.contains("..") || id.is_empty() {
        return Err(AppError::invalid("неверный идентификатор подсветки"));
    }
    let dir = Path::new(BASE).join(id);
    let max: u64 = read_trim(dir.join("max_brightness"))
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| AppError::NotFound("устройство подсветки не найдено".into()))?;
    // Не опускаем яркость до 0, чтобы экран не стал полностью чёрным.
    let raw = ((value.clamp(1, 100) as f64 / 100.0) * max as f64).round().max(1.0) as u32;
    block(async {
        let conn = system_bus().await?;
        let p = proxy(conn, "org.freedesktop.login1", "/org/freedesktop/login1/session/auto", "org.freedesktop.login1.Session").await?;
        p.call_method("SetBrightness", &("backlight", id, raw)).await?;
        Ok(())
    })
}
