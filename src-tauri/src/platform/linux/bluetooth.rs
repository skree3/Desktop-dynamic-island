//! Bluetooth через BlueZ (org.bluez на системной шине).

use std::collections::HashMap;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

use super::{block, proxy, system_bus, val_bool, val_i64, val_str};
use crate::core::error::{AppError, AppResult};
use crate::platform::types::{BluetoothDeviceInfo, BluetoothState};

type Managed = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

async fn managed() -> AppResult<Managed> {
    let conn = system_bus().await?;
    let om = proxy(conn, "org.bluez", "/", "org.freedesktop.DBus.ObjectManager").await?;
    let reply = om.call_method("GetManagedObjects", &()).await.map_err(|e| {
        AppError::unsupported(format!("служба BlueZ недоступна ({e}). Установите и запустите bluetooth.service."))
    })?;
    Ok(reply.body().deserialize::<Managed>()?)
}

pub fn state() -> AppResult<BluetoothState> {
    block(async {
        let objects = match managed().await {
            Ok(o) => o,
            Err(e) => return Ok(BluetoothState { error: Some(e.to_string()), ..Default::default() }),
        };
        let mut st = BluetoothState::default();
        let mut devices = vec![];
        for (path, ifaces) in &objects {
            if let Some(a) = ifaces.get("org.bluez.Adapter1") {
                st.available = true;
                st.can_toggle = true;
                st.powered |= a.get("Powered").and_then(|v| val_bool(v)).unwrap_or(false);
            }
            if let Some(d) = ifaces.get("org.bluez.Device1") {
                let paired = d.get("Paired").and_then(|v| val_bool(v)).unwrap_or(false);
                let connected = d.get("Connected").and_then(|v| val_bool(v)).unwrap_or(false);
                if !paired && !connected {
                    continue;
                }
                let name = d
                    .get("Alias")
                    .and_then(|v| val_str(v))
                    .or_else(|| d.get("Name").and_then(|v| val_str(v)))
                    .unwrap_or_else(|| "Без имени".into());
                let battery = ifaces
                    .get("org.bluez.Battery1")
                    .and_then(|b| b.get("Percentage"))
                    .and_then(|v| val_i64(v))
                    .map(|p| p.clamp(0, 100) as u8);
                devices.push(BluetoothDeviceInfo {
                    id: path.to_string(),
                    name,
                    connected,
                    paired,
                    kind: d.get("Icon").and_then(|v| val_str(v)).unwrap_or_else(|| "bluetooth".into()),
                    battery,
                });
            }
        }
        if !st.available {
            st.error = Some("Bluetooth-адаптер не найден".into());
        }
        devices.sort_by(|a, b| b.connected.cmp(&a.connected).then(a.name.cmp(&b.name)));
        st.devices = devices;
        Ok(st)
    })
}

pub fn set_powered(on: bool) -> AppResult<()> {
    block(async {
        let objects = managed().await?;
        let conn = system_bus().await?;
        let mut found = false;
        for (path, ifaces) in &objects {
            if ifaces.contains_key("org.bluez.Adapter1") {
                found = true;
                let a = proxy(conn, "org.bluez", path.as_str(), "org.bluez.Adapter1").await?;
                a.set_property("Powered", on).await?;
            }
        }
        if found {
            Ok(())
        } else {
            Err(AppError::unsupported("Bluetooth-адаптер не найден"))
        }
    })
}
