//! Питание в Linux: батарея из /sys/class/power_supply, режим энергосбережения
//! через power-profiles-daemon, выключение/перезагрузка через logind
//! (interactive=true — polkit запросит подтверждение, если требуется).

use super::{block, proxy, read_trim, system_bus};
use crate::core::error::{AppError, AppResult};
use crate::platform::types::PowerState;

const PPD: &[(&str, &str, &str)] = &[
    ("org.freedesktop.UPower.PowerProfiles", "/org/freedesktop/UPower/PowerProfiles", "org.freedesktop.UPower.PowerProfiles"),
    ("net.hadess.PowerProfiles", "/net/hadess/PowerProfiles", "net.hadess.PowerProfiles"),
];

async fn active_profile() -> Option<(usize, String)> {
    let conn = system_bus().await.ok()?;
    for (i, (dest, path, iface)) in PPD.iter().enumerate() {
        if let Ok(p) = proxy(conn, dest, path, iface).await {
            if let Ok(v) = p.get_property::<String>("ActiveProfile").await {
                return Some((i, v));
            }
        }
    }
    None
}

pub fn state() -> AppResult<PowerState> {
    let mut st = PowerState::default();
    let mut energy_now = 0f64;
    let mut power_now = 0f64;
    if let Ok(entries) = std::fs::read_dir("/sys/class/power_supply") {
        for e in entries.flatten() {
            let dir = e.path();
            let kind = read_trim(dir.join("type")).unwrap_or_default();
            match kind.as_str() {
                "Battery" => {
                    if read_trim(dir.join("scope")).as_deref() == Some("Device") {
                        continue; // батареи мышей/геймпадов
                    }
                    st.has_battery = true;
                    st.percent = read_trim(dir.join("capacity")).and_then(|s| s.parse::<u8>().ok()).map(|p| p.min(100));
                    let status = read_trim(dir.join("status")).unwrap_or_default();
                    st.charging = status == "Charging";
                    energy_now += read_trim(dir.join("energy_now")).and_then(|s| s.parse().ok()).unwrap_or(0.0);
                    power_now += read_trim(dir.join("power_now")).and_then(|s| s.parse().ok()).unwrap_or(0.0);
                }
                "Mains" => {
                    st.on_ac = Some(read_trim(dir.join("online")).as_deref() == Some("1") || st.on_ac == Some(true));
                }
                _ => {}
            }
        }
    }
    if st.has_battery && !st.charging && power_now > 0.0 {
        st.secs_remaining = Some((energy_now / power_now * 3600.0) as u64);
    }
    if let Some((_, profile)) = block(active_profile()) {
        st.saver = Some(profile == "power-saver");
        st.saver_can_toggle = true;
        st.saver_backend = Some("power-profiles-daemon".into());
    }
    Ok(st)
}

pub fn set_saver(on: bool) -> AppResult<()> {
    block(async {
        let (idx, _) = active_profile()
            .await
            .ok_or_else(|| AppError::unsupported("power-profiles-daemon не установлен или не запущен"))?;
        let (dest, path, iface) = PPD[idx];
        let conn = system_bus().await?;
        let p = proxy(conn, dest, path, iface).await?;
        p.set_property("ActiveProfile", if on { "power-saver" } else { "balanced" }).await?;
        Ok(())
    })
}

fn logind(method: &'static str) -> AppResult<()> {
    block(async {
        let conn = system_bus().await?;
        let p = proxy(conn, "org.freedesktop.login1", "/org/freedesktop/login1", "org.freedesktop.login1.Manager").await?;
        p.call_method(method, &(true,)).await.map_err(|e| {
            let t = e.to_string();
            if t.contains("Interactive") || t.contains("NotAuthorized") || t.contains("AccessDenied") {
                AppError::PermissionDenied(format!("logind отклонил запрос: {t}"))
            } else {
                AppError::system(t)
            }
        })?;
        Ok(())
    })
}

pub fn shutdown() -> AppResult<()> {
    logind("PowerOff")
}

pub fn reboot() -> AppResult<()> {
    logind("Reboot")
}
