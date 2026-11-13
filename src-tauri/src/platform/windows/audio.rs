//! Windows Core Audio: IMMDeviceEnumerator + IAudioEndpointVolume.
//! Смена устройства вывода по умолчанию не имеет документированного API
//! (только недокументированный IPolicyConfig), поэтому мы показываем список
//! устройств и предлагаем открыть системные параметры звука.

use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{eConsole, eRender, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator, DEVICE_STATE_ACTIVE};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL, STGM_READ};

use super::{ensure_com, take_pwstr};
use crate::core::error::{AppError, AppResult};
use crate::platform::types::{AudioDevice, VolumeState};

fn enumerator() -> AppResult<IMMDeviceEnumerator> {
    ensure_com();
    Ok(unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)? })
}

fn default_device(en: &IMMDeviceEnumerator) -> AppResult<IMMDevice> {
    unsafe { en.GetDefaultAudioEndpoint(eRender, eConsole) }
        .map_err(|_| AppError::unsupported("устройство вывода звука не найдено"))
}

fn endpoint() -> AppResult<IAudioEndpointVolume> {
    let en = enumerator()?;
    let dev = default_device(&en)?;
    Ok(unsafe { dev.Activate(CLSCTX_ALL, None)? })
}

fn device_info(dev: &IMMDevice, default_id: &str) -> Option<AudioDevice> {
    unsafe {
        let id = take_pwstr(dev.GetId().ok()?);
        let store = dev.OpenPropertyStore(STGM_READ).ok()?;
        let name = store.GetValue(&PKEY_Device_FriendlyName).map(|v| v.to_string()).unwrap_or_default();
        Some(AudioDevice { is_default: id == default_id, name: if name.is_empty() { "Устройство".into() } else { name }, id })
    }
}

pub fn state() -> AppResult<VolumeState> {
    let en = enumerator()?;
    let dev = default_device(&en)?;
    let default_id = unsafe { take_pwstr(dev.GetId()?) };
    let vol: IAudioEndpointVolume = unsafe { dev.Activate(CLSCTX_ALL, None)? };
    let level = unsafe { vol.GetMasterVolumeLevelScalar()? };
    let muted = unsafe { vol.GetMute()?.as_bool() };
    let mut devices = vec![];
    unsafe {
        if let Ok(col) = en.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) {
            for i in 0..col.GetCount().unwrap_or(0) {
                if let Ok(d) = col.Item(i) {
                    if let Some(info) = device_info(&d, &default_id) {
                        devices.push(info);
                    }
                }
            }
        }
    }
    let device = devices.iter().find(|d| d.is_default).cloned().or_else(|| device_info(&dev, &default_id));
    Ok(VolumeState {
        available: true,
        level,
        muted,
        device,
        devices,
        can_select_device: true,
        backend: "Windows Core Audio".into(),
        error: None,
    })
}

pub fn set_volume(level: f32) -> AppResult<()> {
    let vol = endpoint()?;
    unsafe { vol.SetMasterVolumeLevelScalar(level.clamp(0.0, 1.0), std::ptr::null())? };
    Ok(())
}

pub fn set_mute(muted: bool) -> AppResult<()> {
    let vol = endpoint()?;
    unsafe { vol.SetMute(muted, std::ptr::null())? };
    Ok(())
}

/// Недокументированный IPolicyConfig — единственный способ сменить устройство
/// по умолчанию (им пользуются и сама панель звука, и SoundSwitch/EarTrumpet).
#[allow(non_snake_case, dead_code)]
mod policy {
    use std::ffi::c_void;
    use windows::core::{interface, IUnknown, IUnknown_Vtbl, GUID, HRESULT, PCWSTR};

    #[interface("f8679f50-850a-41cf-9c72-430f290290c8")]
    pub unsafe trait IPolicyConfig: IUnknown {
        fn GetMixFormat(&self, a: PCWSTR, b: *mut c_void) -> HRESULT;
        fn GetDeviceFormat(&self, a: PCWSTR, b: i32, c: *mut c_void) -> HRESULT;
        fn ResetDeviceFormat(&self, a: PCWSTR) -> HRESULT;
        fn SetDeviceFormat(&self, a: PCWSTR, b: *mut c_void, c: *mut c_void) -> HRESULT;
        fn GetProcessingPeriod(&self, a: PCWSTR, b: i32, c: *mut i64, d: *mut i64) -> HRESULT;
        fn SetProcessingPeriod(&self, a: PCWSTR, b: *mut i64) -> HRESULT;
        fn GetShareMode(&self, a: PCWSTR, b: *mut c_void) -> HRESULT;
        fn SetShareMode(&self, a: PCWSTR, b: *mut c_void) -> HRESULT;
        fn GetPropertyValue(&self, a: PCWSTR, b: *const c_void, c: *mut c_void) -> HRESULT;
        fn SetPropertyValue(&self, a: PCWSTR, b: *const c_void, c: *mut c_void) -> HRESULT;
        fn SetDefaultEndpoint(&self, id: PCWSTR, role: i32) -> HRESULT;
        fn SetEndpointVisibility(&self, id: PCWSTR, visible: i32) -> HRESULT;
    }

    pub const CLSID_POLICY_CONFIG: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

    pub fn set_default(id: &str) -> windows::core::Result<()> {
        use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
        let w: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe {
            let cfg: IPolicyConfig = CoCreateInstance(&CLSID_POLICY_CONFIG, None, CLSCTX_ALL)?;
            // eConsole, eMultimedia, eCommunications — как при выборе в панели звука.
            for role in 0..3 {
                cfg.SetDefaultEndpoint(PCWSTR(w.as_ptr()), role).ok()?;
            }
        }
        Ok(())
    }
}

pub fn set_default_device(id: &str) -> AppResult<()> {
    ensure_com();
    policy::set_default(id).map_err(|e| AppError::System(format!("не удалось сменить устройство вывода: {e}")))
}

/// Громкость программ (сеансы звука устройства по умолчанию).
pub fn app_volumes() -> AppResult<Vec<crate::platform::types::AppVolume>> {
    use windows::core::Interface;
    use windows::Win32::Media::Audio::{IAudioSessionControl2, IAudioSessionManager2, ISimpleAudioVolume};
    let en = enumerator()?;
    let dev = default_device(&en)?;
    let mgr: IAudioSessionManager2 = unsafe { dev.Activate(CLSCTX_ALL, None)? };
    let list = unsafe { mgr.GetSessionEnumerator()? };
    let mut out: Vec<crate::platform::types::AppVolume> = Vec::new();
    for i in 0..unsafe { list.GetCount()? } {
        let Ok(ctl) = (unsafe { list.GetSession(i) }) else { continue };
        let Ok(ctl2) = ctl.cast::<IAudioSessionControl2>() else { continue };
        // Только активные и «на паузе» сеансы, без отключённых.
        let state = unsafe { ctl.GetState() }.map(|s| s.0).unwrap_or(2);
        if state == 2 {
            continue;
        }
        let pid = unsafe { ctl2.GetProcessId() }.unwrap_or(0);
        let Ok(vol) = ctl.cast::<ISimpleAudioVolume>() else { continue };
        let name = if pid == 0 {
            "Системные звуки".to_string()
        } else {
            super::foreground::process_path(pid)
                .and_then(|p| std::path::Path::new(&p).file_stem().map(|s| s.to_string_lossy().to_string()))
                .unwrap_or_else(|| format!("Процесс {pid}"))
        };
        let level = unsafe { vol.GetMasterVolume() }.unwrap_or(1.0);
        let muted = unsafe { vol.GetMute() }.map(|m| m.as_bool()).unwrap_or(false);
        let id = format!("pid:{pid}");
        if !out.iter().any(|a| a.id == id) {
            out.push(crate::platform::types::AppVolume { id, name, level, muted });
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(out)
}

pub fn set_app_volume(id: &str, level: Option<f32>, muted: Option<bool>) -> AppResult<()> {
    use windows::core::Interface;
    use windows::Win32::Media::Audio::{IAudioSessionControl2, IAudioSessionManager2, ISimpleAudioVolume};
    let pid: u32 = id.strip_prefix("pid:").and_then(|p| p.parse().ok()).ok_or_else(|| AppError::invalid("неверный сеанс"))?;
    let en = enumerator()?;
    let dev = default_device(&en)?;
    let mgr: IAudioSessionManager2 = unsafe { dev.Activate(CLSCTX_ALL, None)? };
    let list = unsafe { mgr.GetSessionEnumerator()? };
    for i in 0..unsafe { list.GetCount()? } {
        let Ok(ctl) = (unsafe { list.GetSession(i) }) else { continue };
        let Ok(ctl2) = ctl.cast::<IAudioSessionControl2>() else { continue };
        if unsafe { ctl2.GetProcessId() }.unwrap_or(u32::MAX) != pid {
            continue;
        }
        let vol: ISimpleAudioVolume = ctl.cast()?;
        unsafe {
            if let Some(l) = level {
                vol.SetMasterVolume(l.clamp(0.0, 1.0), std::ptr::null())?;
            }
            if let Some(m) = muted {
                vol.SetMute(m, std::ptr::null())?;
            }
        }
    }
    Ok(())
}
