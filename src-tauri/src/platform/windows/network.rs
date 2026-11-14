//! Wi-Fi и радиомодули: Windows.Devices.Radios (вкл/выкл с запросом доступа)
//! и Windows.Networking.Connectivity (текущее подключение / SSID).
//! Режим «в самолёте» — недокументированный COM RadioManagementAPI (тот же,
//! что у переключателя в центре уведомлений); работает без прав администратора.

use windows::Devices::Radios::{Radio, RadioAccessStatus, RadioKind, RadioState};
use windows::Networking::Connectivity::{NetworkConnectivityLevel, NetworkInformation};

use super::ensure_com;
use crate::core::error::{AppError, AppResult};
use crate::platform::types::{AirplaneState, WifiState};

pub fn radios() -> AppResult<Vec<Radio>> {
    ensure_com();
    let list = Radio::GetRadiosAsync()?.get()?;
    let mut out = vec![];
    for i in 0..list.Size()? {
        out.push(list.GetAt(i)?);
    }
    Ok(out)
}

pub fn radio_state(kind: RadioKind) -> (bool, bool) {
    match radios() {
        Ok(list) => {
            let of_kind: Vec<_> = list.into_iter().filter(|r| r.Kind().map(|k| k == kind).unwrap_or(false)).collect();
            let available = !of_kind.is_empty();
            let on = of_kind.iter().any(|r| r.State().map(|s| s == RadioState::On).unwrap_or(false));
            (available, on)
        }
        Err(_) => (false, false),
    }
}

pub fn set_radio(kind: RadioKind, on: bool) -> AppResult<()> {
    ensure_com();
    let access = Radio::RequestAccessAsync()?.get()?;
    if access != RadioAccessStatus::Allowed {
        return Err(AppError::PermissionDenied(
            "Windows запретил управление радиомодулями. Проверьте «Конфиденциальность → Радио».".into(),
        ));
    }
    let list = radios()?;
    let mut found = false;
    for r in list {
        if r.Kind()? == kind {
            found = true;
            let st = r.SetStateAsync(if on { RadioState::On } else { RadioState::Off })?.get()?;
            if st != RadioAccessStatus::Allowed {
                return Err(AppError::PermissionDenied(format!("система отклонила изменение состояния ({:?})", st.0)));
            }
        }
    }
    if found {
        Ok(())
    } else {
        Err(AppError::unsupported("радиомодуль не найден"))
    }
}

pub fn wifi_state() -> AppResult<WifiState> {
    let (available, on) = radio_state(RadioKind::WiFi);
    let mut st = WifiState { available, on, can_toggle: available, ..Default::default() };
    // Ищем Wi-Fi среди всех подключений, а не только «интернет-профиль»: при
    // включённом VPN/TUN (Happ, INCY, v2RayTun) интернет-профилем становится
    // виртуальный адаптер, и Wi-Fi ошибочно выглядел бы отключённым.
    if let Ok(list) = NetworkInformation::GetConnectionProfiles() {
        for i in 0..list.Size().unwrap_or(0) {
            let Ok(profile) = list.GetAt(i) else { continue };
            if !profile.IsWlanConnectionProfile().unwrap_or(false) {
                continue;
            }
            let level = profile.GetNetworkConnectivityLevel().unwrap_or(NetworkConnectivityLevel::None);
            if level == NetworkConnectivityLevel::None {
                continue;
            }
            st.connected = true;
            st.ssid = profile
                .WlanConnectionProfileDetails()
                .and_then(|d| d.GetConnectedSsid())
                .map(|h| h.to_string())
                .ok()
                .filter(|s| !s.is_empty())
                .or_else(|| profile.ProfileName().map(|h| h.to_string()).ok());
            break;
        }
    }
    if !available {
        st.error = Some("Wi-Fi адаптер не обнаружен".into());
    }
    Ok(st)
}

pub fn set_wifi(on: bool) -> AppResult<()> {
    set_radio(RadioKind::WiFi, on)
}

#[allow(non_snake_case, dead_code)]
mod rm {
    use windows::core::{interface, IUnknown, IUnknown_Vtbl, GUID, HRESULT};

    #[interface("db3afbfb-08e6-46c6-aa70-bf9a34c30ab7")]
    pub unsafe trait IRadioManager: IUnknown {
        fn IsRMSupported(&self, supported: *mut i32) -> HRESULT;
        fn GetUIRadioInstances(&self, out: *mut *mut core::ffi::c_void) -> HRESULT;
        fn GetSystemRadioState(&self, enabled: *mut i32, p2: *mut i32, reason: *mut i32) -> HRESULT;
        fn SetSystemRadioState(&self, enabled: i32) -> HRESULT;
        fn Refresh(&self) -> HRESULT;
        fn OnHardwareSliderChange(&self, a: i32, b: i32) -> HRESULT;
    }

    pub const CLSID_RADIO_MANAGEMENT: GUID = GUID::from_u128(0x581333f6_28db_41be_bc7a_ff201f12f3f6);

    fn manager() -> Option<IRadioManager> {
        use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, CLSCTX_LOCAL_SERVER, COINIT_MULTITHREADED};
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            CoCreateInstance(&CLSID_RADIO_MANAGEMENT, None, CLSCTX_INPROC_SERVER | CLSCTX_LOCAL_SERVER).ok()
        }
    }

    /// Some(true) — режим «в самолёте» включён.
    pub fn get() -> Option<bool> {
        let m = manager()?;
        let (mut en, mut p2, mut reason) = (0i32, 0i32, 0i32);
        unsafe { m.GetSystemRadioState(&mut en, &mut p2, &mut reason).ok().ok()? };
        Some(en == 0)
    }

    pub fn set(on: bool) -> bool {
        manager().map(|m| unsafe { m.SetSystemRadioState(if on { 0 } else { 1 }).is_ok() }).unwrap_or(false)
    }
}

/// COM-вызовы — в отдельном MTA-потоке (вызывающий поток может быть STA).
fn on_mta<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> Option<R> {
    std::thread::spawn(f).join().ok()
}

pub fn airplane_on() -> Option<bool> {
    on_mta(rm::get).flatten()
}

pub fn set_airplane(on: bool) -> AppResult<()> {
    if on_mta(move || rm::set(on)).unwrap_or(false) {
        Ok(())
    } else {
        Err(AppError::unsupported("Windows не дал переключить режим «в самолёте». Откройте «Параметры → Сеть»."))
    }
}

pub fn airplane_state() -> AirplaneState {
    if let Some(on) = airplane_on() {
        return AirplaneState { known: true, on, can_toggle: true, note: "Режим «в самолёте» Windows".into() };
    }
    match radios() {
        Ok(list) if !list.is_empty() => {
            let all_off = list.iter().all(|r| r.State().map(|s| s != RadioState::On).unwrap_or(true));
            AirplaneState {
                known: true,
                on: all_off,
                can_toggle: false,
                note: "Состояние определяется по радиомодулям. Переключение — в параметрах Windows.".into(),
            }
        }
        _ => AirplaneState { known: false, on: false, can_toggle: false, note: "Радиомодули не обнаружены".into() },
    }
}
