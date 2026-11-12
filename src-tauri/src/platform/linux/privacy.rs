//! Кто сейчас использует микрофон и камеру в Linux.
//! Микрофон: потоки записи PulseAudio/PipeWire (`pactl list source-outputs`).
//! Камера: процессы пользователя, открывшие /dev/video* (видно только свои процессы).
//! Время начала неизвестно системе — модуль запоминает момент первого обнаружения.

use std::collections::HashMap;
use std::process::Command;
use std::sync::Mutex;

use crate::core::error::{AppError, AppResult};
use crate::database::now_ms;
use crate::platform::types::DeviceUse;

static FIRST_SEEN: Mutex<Option<HashMap<String, i64>>> = Mutex::new(None);

fn since(key: &str, live: &mut Vec<String>) -> i64 {
    live.push(key.to_string());
    let mut g = FIRST_SEEN.lock().unwrap_or_else(|p| p.into_inner());
    *g.get_or_insert_with(HashMap::new).entry(key.to_string()).or_insert_with(now_ms)
}

fn pactl(args: &[&str]) -> Option<String> {
    let out = Command::new("pactl").args(args).env("LC_ALL", "C").output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).to_string())
}

/// Значение свойства вида `application.name = "Telegram"`.
fn prop(line: &str, key: &str) -> Option<String> {
    let rest = line.trim().strip_prefix(key)?.trim_start().strip_prefix('=')?.trim();
    Some(rest.trim_matches('"').to_string())
}

/// Служебные программы, которые держат устройство постоянно или только для индикаторов.
const IGNORED: &[&str] = &["pipewire", "wireplumber", "pulseaudio", "pavucontrol", "gnome-control-center", "desktop-island", "xdg-desktop-portal"];

fn ignored(name: &str) -> bool {
    let l = name.to_lowercase();
    IGNORED.iter().any(|i| l.contains(i))
}

fn microphones(out: &mut Vec<DeviceUse>, live: &mut Vec<String>) {
    let Some(text) = pactl(&["list", "source-outputs"]) else { return };
    for block in text.split("Source Output #").skip(1) {
        let mut name = None;
        let mut binary = None;
        let mut corked = false;
        let mut monitor = false;
        for line in block.lines() {
            let t = line.trim();
            if t == "Corked: yes" {
                corked = true;
            }
            if let Some(v) = prop(t, "application.name") {
                name = Some(v);
            } else if let Some(v) = prop(t, "application.process.binary") {
                binary = Some(v);
            } else if t.starts_with("Source:") && t.contains(".monitor") {
                monitor = true;
            }
        }
        if corked || monitor {
            continue;
        }
        let app = name.clone().or(binary.clone()).unwrap_or_else(|| "Приложение".into());
        if ignored(&app) || binary.as_deref().map(ignored).unwrap_or(false) {
            continue;
        }
        if out.iter().any(|u| u.device == "microphone" && u.app_name == app) {
            continue;
        }
        let since = since(&format!("mic:{app}"), live);
        out.push(DeviceUse { device: "microphone".into(), app_name: app, app_path: binary, app_id: None, since });
    }
}

fn cameras(out: &mut Vec<DeviceUse>, live: &mut Vec<String>) {
    let me = std::process::id();
    let Ok(procs) = std::fs::read_dir("/proc") else { return };
    for p in procs.flatten() {
        let Some(pid) = p.file_name().to_str().and_then(|s| s.parse::<u32>().ok()) else { continue };
        if pid == me {
            continue;
        }
        let Ok(fds) = std::fs::read_dir(p.path().join("fd")) else { continue };
        let uses_camera = fds.flatten().any(|fd| std::fs::read_link(fd.path()).map(|t| t.to_string_lossy().starts_with("/dev/video")).unwrap_or(false));
        if !uses_camera {
            continue;
        }
        let comm = super::read_trim(p.path().join("comm")).unwrap_or_default();
        if comm.is_empty() || ignored(&comm) || out.iter().any(|u| u.device == "camera" && u.app_name == comm) {
            continue;
        }
        let exe = std::fs::read_link(p.path().join("exe")).ok().map(|e| e.to_string_lossy().to_string());
        let since = since(&format!("cam:{comm}"), live);
        out.push(DeviceUse { device: "camera".into(), app_name: comm, app_path: exe, app_id: None, since });
    }
}

pub fn in_use() -> Vec<DeviceUse> {
    let mut out = vec![];
    let mut live = vec![];
    microphones(&mut out, &mut live);
    cameras(&mut out, &mut live);
    // Забываем время начала для того, что уже не используется.
    if let Some(map) = FIRST_SEEN.lock().unwrap_or_else(|p| p.into_inner()).as_mut() {
        map.retain(|k, _| live.contains(k));
    }
    out
}

pub fn mic_muted() -> Option<bool> {
    let text = pactl(&["get-source-mute", "@DEFAULT_SOURCE@"])?;
    Some(text.contains("yes"))
}

pub fn set_mic_muted(muted: bool) -> AppResult<()> {
    let ok = Command::new("pactl")
        .args(["set-source-mute", "@DEFAULT_SOURCE@", if muted { "1" } else { "0" }])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok {
        Ok(())
    } else {
        Err(AppError::unsupported("не удалось переключить микрофон (нужен pactl)"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn props() {
        assert_eq!(prop("\t\tapplication.name = \"Telegram\"", "application.name").as_deref(), Some("Telegram"));
        assert!(ignored("WirePlumber"));
        assert!(!ignored("Discord"));
    }
}
