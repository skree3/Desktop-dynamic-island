//! Wi-Fi через NetworkManager (системная шина). Включение/выключение
//! проходит через polkit — система сама запросит разрешение при необходимости.
//! Режим «в самолёте» читается из rfkill (sysfs), переключается в настройках ОС.

use zbus::zvariant::OwnedObjectPath;

use super::{block, proxy, read_trim, system_bus};
use crate::core::error::{AppError, AppResult};
use crate::platform::types::{AirplaneState, WifiState};

const NM: &str = "org.freedesktop.NetworkManager";
const NM_PATH: &str = "/org/freedesktop/NetworkManager";

pub fn wifi_state() -> AppResult<WifiState> {
    block(async {
        let conn = system_bus().await?;
        let nm = proxy(conn, NM, NM_PATH, NM).await?;
        let enabled: bool = match nm.get_property("WirelessEnabled").await {
            Ok(v) => v,
            Err(e) => {
                return Ok(WifiState { error: Some(format!("NetworkManager недоступен: {e}")), ..Default::default() });
            }
        };
        let hw: bool = nm.get_property("WirelessHardwareEnabled").await.unwrap_or(true);
        let mut st = WifiState { available: hw, on: enabled, can_toggle: hw, ..Default::default() };
        if let Ok(primary) = nm.get_property::<OwnedObjectPath>("PrimaryConnection").await {
            if primary.as_str() != "/" {
                let ac = proxy(conn, NM, primary.as_str(), "org.freedesktop.NetworkManager.Connection.Active").await?;
                let kind: String = ac.get_property("Type").await.unwrap_or_default();
                if kind == "802-11-wireless" {
                    st.connected = true;
                    st.ssid = ac.get_property::<String>("Id").await.ok();
                }
            }
        }
        if !hw {
            st.error = Some("Wi-Fi отключён аппаратно или адаптер отсутствует".into());
        }
        Ok(st)
    })
}

pub fn set_wifi(on: bool) -> AppResult<()> {
    block(async {
        let conn = system_bus().await?;
        let nm = proxy(conn, NM, NM_PATH, NM).await?;
        nm.set_property("WirelessEnabled", on).await.map_err(|e| {
            let text = e.to_string();
            if text.contains("NotAuthorized") || text.contains("AccessDenied") || text.contains("PermissionDenied") {
                AppError::PermissionDenied("polkit отклонил изменение состояния Wi-Fi".into())
            } else {
                AppError::system(text)
            }
        })?;
        Ok(())
    })
}

pub fn airplane_state() -> AirplaneState {
    let mut any = false;
    let mut all_blocked = true;
    if let Ok(entries) = std::fs::read_dir("/sys/class/rfkill") {
        for e in entries.flatten() {
            any = true;
            let soft = read_trim(e.path().join("soft")).unwrap_or_default();
            let hard = read_trim(e.path().join("hard")).unwrap_or_default();
            if soft != "1" && hard != "1" {
                all_blocked = false;
            }
        }
    }
    AirplaneState {
        known: any,
        on: any && all_blocked,
        can_toggle: false,
        note: if any {
            "Состояние определяется через rfkill. Переключение — в настройках окружения рабочего стола.".into()
        } else {
            "Радиомодули rfkill не найдены".into()
        },
    }
}
