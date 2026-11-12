//! Bluetooth на Windows: питание адаптера через Windows.Devices.Radios,
//! список сопряжённых/подключённых устройств через Windows.Devices.Enumeration
//! (селекторы BluetoothDevice / BluetoothLEDevice).

use std::collections::HashSet;
use windows::core::HSTRING;
use windows::Devices::Bluetooth::{BluetoothConnectionStatus, BluetoothDevice, BluetoothLEDevice};
use windows::Devices::Enumeration::DeviceInformation;
use windows::Devices::Radios::RadioKind;

use super::{ensure_com, network};
use crate::core::error::AppResult;
use crate::platform::types::{BluetoothDeviceInfo, BluetoothState};

fn find(selector: &HSTRING) -> Vec<(String, String)> {
    let mut out = vec![];
    if let Ok(op) = DeviceInformation::FindAllAsyncAqsFilter(selector) {
        if let Ok(col) = op.get() {
            for i in 0..col.Size().unwrap_or(0) {
                if let Ok(d) = col.GetAt(i) {
                    let id = d.Id().map(|h| h.to_string()).unwrap_or_default();
                    let name = d.Name().map(|h| h.to_string()).unwrap_or_default();
                    out.push((id, name));
                }
            }
        }
    }
    out
}

pub fn state() -> AppResult<BluetoothState> {
    ensure_com();
    let (available, powered) = network::radio_state(RadioKind::Bluetooth);
    let mut st = BluetoothState { available, powered, can_toggle: available, ..Default::default() };
    if !available {
        st.error = Some("Bluetooth-адаптер не обнаружен или отключён в диспетчере устройств".into());
        return Ok(st);
    }

    let mut connected_ids: HashSet<String> = HashSet::new();
    let mut entries: Vec<(String, String, &'static str)> = vec![];
    if let Ok(sel) = BluetoothDevice::GetDeviceSelectorFromConnectionStatus(BluetoothConnectionStatus::Connected) {
        for (id, _) in find(&sel) {
            connected_ids.insert(id);
        }
    }
    if let Ok(sel) = BluetoothLEDevice::GetDeviceSelectorFromConnectionStatus(BluetoothConnectionStatus::Connected) {
        for (id, _) in find(&sel) {
            connected_ids.insert(id);
        }
    }
    if let Ok(sel) = BluetoothDevice::GetDeviceSelectorFromPairingState(true) {
        for (id, name) in find(&sel) {
            entries.push((id, name, "classic"));
        }
    }
    if let Ok(sel) = BluetoothLEDevice::GetDeviceSelectorFromPairingState(true) {
        for (id, name) in find(&sel) {
            entries.push((id, name, "le"));
        }
    }

    // Двухрежимные устройства видны дважды (Classic + LE) — объединяем по имени.
    let mut seen: Vec<BluetoothDeviceInfo> = vec![];
    for (id, name, kind) in entries {
        let connected = connected_ids.contains(&id);
        if let Some(existing) = seen.iter_mut().find(|d| d.name == name && !name.is_empty()) {
            existing.connected |= connected;
            continue;
        }
        seen.push(BluetoothDeviceInfo {
            id,
            name: if name.is_empty() { "Без имени".into() } else { name },
            connected,
            paired: true,
            kind: kind.into(),
            battery: None,
        });
    }
    seen.sort_by(|a, b| b.connected.cmp(&a.connected).then(a.name.cmp(&b.name)));
    st.devices = seen;
    Ok(st)
}

pub fn set_powered(on: bool) -> AppResult<()> {
    network::set_radio(RadioKind::Bluetooth, on)
}
