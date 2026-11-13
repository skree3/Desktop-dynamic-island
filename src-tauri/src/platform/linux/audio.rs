//! Громкость в Linux через PulseAudio-совместимый `pactl` (работает и с
//! PipeWire через pipewire-pulse), с запасным вариантом `wpctl` (WirePlumber).
//! Команды запускаются без shell, аргументы проверяются.

use std::process::Command;

use super::command_exists;
use crate::core::error::{AppError, AppResult};
use crate::platform::types::{AudioDevice, VolumeState};

fn run(cmd: &str, args: &[&str]) -> AppResult<String> {
    let out = Command::new(cmd)
        .args(args)
        .env("LC_ALL", "C")
        .output()
        .map_err(|e| AppError::unsupported(format!("{cmd} недоступен: {e}")))?;
    if !out.status.success() {
        return Err(AppError::system(format!("{cmd} завершился с ошибкой: {}", String::from_utf8_lossy(&out.stderr).trim())));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn parse_percent(s: &str) -> Option<f32> {
    s.split_whitespace().find(|t| t.ends_with('%')).and_then(|t| t.trim_end_matches('%').parse::<f32>().ok()).map(|v| v / 100.0)
}

fn pactl_sinks() -> Vec<(String, String)> {
    let Ok(text) = run("pactl", &["list", "sinks"]) else { return vec![] };
    let mut out = vec![];
    let mut name: Option<String> = None;
    for line in text.lines() {
        let l = line.trim();
        if let Some(n) = l.strip_prefix("Name: ") {
            name = Some(n.to_string());
        } else if let Some(d) = l.strip_prefix("Description: ") {
            if let Some(n) = name.take() {
                out.push((n, d.to_string()));
            }
        }
    }
    out
}

pub fn state() -> AppResult<VolumeState> {
    if command_exists("pactl") {
        let default = run("pactl", &["get-default-sink"]).map(|s| s.trim().to_string()).unwrap_or_default();
        let level = run("pactl", &["get-sink-volume", "@DEFAULT_SINK@"]).ok().and_then(|s| parse_percent(&s)).unwrap_or(0.0);
        let muted = run("pactl", &["get-sink-mute", "@DEFAULT_SINK@"]).map(|s| s.contains("yes")).unwrap_or(false);
        let devices: Vec<AudioDevice> = pactl_sinks()
            .into_iter()
            .map(|(id, name)| AudioDevice { is_default: id == default, id, name })
            .collect();
        return Ok(VolumeState {
            available: true,
            level: level.min(1.5),
            muted,
            device: devices.iter().find(|d| d.is_default).cloned(),
            devices,
            can_select_device: true,
            backend: "PulseAudio / PipeWire (pactl)".into(),
            error: None,
        });
    }
    if command_exists("wpctl") {
        let text = run("wpctl", &["get-volume", "@DEFAULT_AUDIO_SINK@"])?;
        let level = text.split_whitespace().nth(1).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
        return Ok(VolumeState {
            available: true,
            level,
            muted: text.contains("MUTED"),
            device: None,
            devices: vec![],
            can_select_device: false,
            backend: "PipeWire (wpctl)".into(),
            error: None,
        });
    }
    Err(AppError::unsupported("не найдены pactl или wpctl — установите pulseaudio-utils или wireplumber"))
}

pub fn set_volume(level: f32) -> AppResult<()> {
    let pct = (level.clamp(0.0, 1.0) * 100.0).round() as u32;
    if command_exists("pactl") {
        run("pactl", &["set-sink-volume", "@DEFAULT_SINK@", &format!("{pct}%")])?;
    } else {
        run("wpctl", &["set-volume", "@DEFAULT_AUDIO_SINK@", &format!("{:.2}", pct as f32 / 100.0)])?;
    }
    Ok(())
}

pub fn set_mute(muted: bool) -> AppResult<()> {
    let v = if muted { "1" } else { "0" };
    if command_exists("pactl") {
        run("pactl", &["set-sink-mute", "@DEFAULT_SINK@", v])?;
    } else {
        run("wpctl", &["set-mute", "@DEFAULT_AUDIO_SINK@", v])?;
    }
    Ok(())
}

pub fn set_default_device(id: &str) -> AppResult<()> {
    if !command_exists("pactl") {
        return Err(AppError::unsupported("смена устройства требует pactl"));
    }
    // Разрешаем только существующие имена устройств.
    if !pactl_sinks().iter().any(|(n, _)| n == id) {
        return Err(AppError::NotFound("устройство вывода не найдено".into()));
    }
    run("pactl", &["set-default-sink", id])?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn percent() {
        assert_eq!(super::parse_percent("Volume: front-left: 32768 /  50% / -18.06 dB"), Some(0.5));
    }
}

fn pactl(args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("pactl").args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Громкость программ: `pactl list sink-inputs`.
pub fn app_volumes() -> AppResult<Vec<crate::platform::types::AppVolume>> {
    let text = pactl(&["list", "sink-inputs"]).ok_or_else(|| AppError::unsupported("pactl недоступен"))?;
    let mut out = Vec::new();
    let mut cur: Option<crate::platform::types::AppVolume> = None;
    for line in text.lines() {
        let l = line.trim();
        if let Some(n) = l.strip_prefix("Sink Input #") {
            if let Some(c) = cur.take() {
                out.push(c);
            }
            cur = Some(crate::platform::types::AppVolume { id: n.trim().to_string(), name: String::new(), level: 1.0, muted: false });
        } else if let Some(c) = cur.as_mut() {
            if let Some(v) = l.strip_prefix("Mute:") {
                c.muted = v.trim() == "yes";
            } else if l.starts_with("Volume:") {
                if let Some(p) = l.split('/').nth(1).and_then(|p| p.trim().trim_end_matches('%').parse::<f32>().ok()) {
                    c.level = (p / 100.0).clamp(0.0, 1.5);
                }
            } else if let Some(v) = l.strip_prefix("application.name = ") {
                c.name = v.trim_matches('"').to_string();
            }
        }
    }
    if let Some(c) = cur.take() {
        out.push(c);
    }
    for a in out.iter_mut() {
        if a.name.is_empty() {
            a.name = format!("Поток {}", a.id);
        }
    }
    Ok(out)
}

pub fn set_app_volume(id: &str, level: Option<f32>, muted: Option<bool>) -> AppResult<()> {
    if !id.chars().all(|c| c.is_ascii_digit()) {
        return Err(AppError::invalid("неверный поток"));
    }
    if let Some(l) = level {
        let pct = format!("{}%", (l.clamp(0.0, 1.0) * 100.0).round() as u32);
        pactl(&["set-sink-input-volume", id, &pct]).ok_or_else(|| AppError::System("не удалось изменить громкость".into()))?;
    }
    if let Some(m) = muted {
        pactl(&["set-sink-input-mute", id, if m { "1" } else { "0" }]).ok_or_else(|| AppError::System("не удалось выключить звук".into()))?;
    }
    Ok(())
}
