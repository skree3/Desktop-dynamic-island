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
        can_select_device: false,
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

pub fn set_default_device(_id: &str) -> AppResult<()> {
    Err(AppError::unsupported(
        "Windows не предоставляет документированного API для смены устройства вывода. Откройте «Параметры → Звук».",
    ))
}
