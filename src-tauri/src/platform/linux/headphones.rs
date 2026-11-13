//! Bluetooth-наушники через bluetoothctl (BlueZ): список сопряжённых
//! аудиоустройств, заряд («Battery Percentage») и подключение.

use crate::core::error::{AppError, AppResult};
use crate::platform::types::Headphone;

fn ctl(args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("bluetoothctl").args(args).output().ok()?;
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn valid_mac(m: &str) -> bool {
    m.len() == 17 && m.split(':').all(|p| p.len() == 2 && p.chars().all(|c| c.is_ascii_hexdigit()))
}

pub fn list() -> AppResult<Vec<Headphone>> {
    let text = ctl(&["devices", "Paired"]).or_else(|| ctl(&["paired-devices"])).ok_or_else(|| AppError::unsupported("bluetoothctl недоступен"))?;
    let mut out = Vec::new();
    for line in text.lines() {
        let mut parts = line.splitn(3, ' ');
        if parts.next() != Some("Device") {
            continue;
        }
        let (Some(mac), Some(name)) = (parts.next(), parts.next()) else { continue };
        if !valid_mac(mac) {
            continue;
        }
        let info = ctl(&["info", mac]).unwrap_or_default();
        let audio = info.lines().any(|l| l.trim().starts_with("Icon: audio"));
        if !audio {
            continue;
        }
        let connected = info.lines().any(|l| l.trim() == "Connected: yes");
        let battery = info
            .lines()
            .find(|l| l.trim().starts_with("Battery Percentage:"))
            .and_then(|l| l.rsplit('(').next())
            .and_then(|v| v.trim_end_matches(')').trim().parse::<u8>().ok());
        out.push(Headphone { id: mac.to_string(), name: name.trim().to_string(), connected, battery });
    }
    Ok(out)
}

pub fn set_connected(id: &str, connect: bool) -> AppResult<()> {
    if !valid_mac(id) {
        return Err(AppError::invalid("неверное устройство"));
    }
    let out = ctl(&[if connect { "connect" } else { "disconnect" }, id]).unwrap_or_default();
    if out.contains("successful") || out.contains("Successful") {
        Ok(())
    } else {
        Err(AppError::System(out.lines().last().unwrap_or("bluetoothctl не ответил").to_string()))
    }
}
