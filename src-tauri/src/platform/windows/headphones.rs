//! Bluetooth-наушники: список сопряжённых аудиоустройств, заряд и подключение.
//! Заряд — свойство PnP {104EA319-6EE2-4701-BD47-8DDBF425BBE5},2 (его выставляет
//! стек Bluetooth для гарнитур с профилем Hands-Free). Подключение — включение
//! служб A2DP/HFP на устройстве (BluetoothSetServiceState), как в панели Windows.

use windows::core::GUID;
use windows::Win32::Devices::Bluetooth::{
    BluetoothFindDeviceClose, BluetoothFindFirstDevice, BluetoothFindNextDevice, BluetoothSetServiceState, BLUETOOTH_DEVICE_INFO,
    BLUETOOTH_DEVICE_SEARCH_PARAMS, BLUETOOTH_SERVICE_DISABLE, BLUETOOTH_SERVICE_ENABLE,
};
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo, SetupDiGetClassDevsW, SetupDiGetDevicePropertyW, DIGCF_ALLCLASSES, DIGCF_PRESENT, SP_DEVINFO_DATA,
};
use windows::Win32::Devices::Properties::{DEVPKEY_Device_FriendlyName, DEVPROPTYPE};
use windows::Win32::Foundation::DEVPROPKEY;

use crate::core::error::{AppError, AppResult};
use crate::platform::types::Headphone;

const AUDIO_SINK: GUID = GUID::from_u128(0x0000110b_0000_1000_8000_00805f9b34fb);
const HANDSFREE: GUID = GUID::from_u128(0x0000111e_0000_1000_8000_00805f9b34fb);
const BATTERY_KEY: DEVPROPKEY = DEVPROPKEY { fmtid: GUID::from_u128(0x104ea319_6ee2_4701_bd47_8ddbf425bbe5), pid: 2 };

/// Аудиоустройство по классу Bluetooth (Major class 0x04 — Audio/Video).
fn is_audio(class: u32) -> bool {
    (class >> 8) & 0x1f == 0x04
}

fn paired() -> Vec<BLUETOOTH_DEVICE_INFO> {
    let mut out = Vec::new();
    unsafe {
        let params = BLUETOOTH_DEVICE_SEARCH_PARAMS {
            dwSize: std::mem::size_of::<BLUETOOTH_DEVICE_SEARCH_PARAMS>() as u32,
            fReturnAuthenticated: true.into(),
            fReturnRemembered: true.into(),
            fReturnConnected: true.into(),
            fReturnUnknown: false.into(),
            fIssueInquiry: false.into(),
            cTimeoutMultiplier: 0,
            hRadio: Default::default(),
        };
        let mut info = BLUETOOTH_DEVICE_INFO { dwSize: std::mem::size_of::<BLUETOOTH_DEVICE_INFO>() as u32, ..Default::default() };
        let Ok(find) = BluetoothFindFirstDevice(&params, &mut info) else { return out };
        loop {
            out.push(info);
            info = BLUETOOTH_DEVICE_INFO { dwSize: std::mem::size_of::<BLUETOOTH_DEVICE_INFO>() as u32, ..Default::default() };
            if BluetoothFindNextDevice(find, &mut info).is_err() {
                break;
            }
        }
        let _ = BluetoothFindDeviceClose(find);
    }
    out
}

fn wide_name(n: &[u16]) -> String {
    let end = n.iter().position(|c| *c == 0).unwrap_or(n.len());
    String::from_utf16_lossy(&n[..end])
}

/// Заряд по имени устройства: ищем узлы PnP со свойством заряда.
fn batteries() -> Vec<(String, u8)> {
    let mut out = Vec::new();
    unsafe {
        let Ok(set) = SetupDiGetClassDevsW(None, None, None, DIGCF_PRESENT | DIGCF_ALLCLASSES) else { return out };
        let mut i = 0;
        loop {
            let mut data = SP_DEVINFO_DATA { cbSize: std::mem::size_of::<SP_DEVINFO_DATA>() as u32, ..Default::default() };
            if SetupDiEnumDeviceInfo(set, i, &mut data).is_err() {
                break;
            }
            i += 1;
            let mut ty = DEVPROPTYPE(0);
            let mut level = [0u8; 1];
            if SetupDiGetDevicePropertyW(set, &data, &BATTERY_KEY, &mut ty, Some(&mut level), None, 0).is_ok() {
                let mut buf = [0u8; 512];
                let mut ty2 = DEVPROPTYPE(0);
                if SetupDiGetDevicePropertyW(set, &data, &DEVPKEY_Device_FriendlyName, &mut ty2, Some(&mut buf), None, 0).is_ok() {
                    let words: Vec<u16> = buf.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                    out.push((wide_name(&words), level[0].min(100)));
                }
            }
        }
        let _ = SetupDiDestroyDeviceInfoList(set);
    }
    out
}

pub fn list() -> AppResult<Vec<Headphone>> {
    let batt = batteries();
    let mut out = Vec::new();
    for d in paired() {
        if !is_audio(d.ulClassofDevice) {
            continue;
        }
        let name = wide_name(&d.szName);
        let addr = unsafe { d.Address.Anonymous.ullLong };
        // Узел Hands-Free обычно называется «<имя> Hands-Free AG» — сравниваем по началу.
        let battery = batt.iter().find(|(n, _)| n.starts_with(&name) || name.starts_with(n.as_str())).map(|(_, l)| *l);
        let connected = d.fConnected.as_bool();
        // У отключённых устройств Windows хранит последний известный заряд — не показываем его.
        out.push(Headphone { id: format!("{addr:012x}"), name, connected, battery: battery.filter(|_| connected) });
    }
    Ok(out)
}

pub fn set_connected(id: &str, connect: bool) -> AppResult<()> {
    let addr = u64::from_str_radix(id, 16).map_err(|_| AppError::invalid("неверное устройство"))?;
    let mut dev = paired().into_iter().find(|d| unsafe { d.Address.Anonymous.ullLong } == addr).ok_or_else(|| AppError::NotFound("устройство не сопряжено".into()))?;
    unsafe {
        for svc in [AUDIO_SINK, HANDSFREE] {
            if connect {
                // Выключить и снова включить службу — так Windows переподключает гарнитуру.
                let _ = BluetoothSetServiceState(None, &mut dev, &svc, BLUETOOTH_SERVICE_DISABLE);
                let r = BluetoothSetServiceState(None, &mut dev, &svc, BLUETOOTH_SERVICE_ENABLE);
                if r != 0 && svc == AUDIO_SINK {
                    return Err(AppError::System(format!("не удалось подключить ({r})")));
                }
            } else {
                let _ = BluetoothSetServiceState(None, &mut dev, &svc, BLUETOOTH_SERVICE_DISABLE);
            }
        }
    }
    Ok(())
}
