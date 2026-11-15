//! Кто сейчас использует микрофон и камеру. Windows ведёт это в реестре
//! (то же, что показывает значок микрофона в трее и «Параметры → Конфиденциальность»):
//! HKCU\…\CapabilityAccessManager\ConsentStore\{microphone,webcam}\<приложение>
//! со значениями LastUsedTimeStart / LastUsedTimeStop (FILETIME). Устройство занято,
//! пока Stop = 0. Только чтение.

use windows::core::{PCWSTR, PWSTR};
use windows::Win32::Foundation::ERROR_SUCCESS;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{eCapture, eCommunications, IMMDeviceEnumerator, MMDeviceEnumerator};
use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_ALL};
use windows::Win32::System::Registry::{RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ};

use super::{ensure_com, wide};
use crate::core::error::{AppError, AppResult};
use crate::platform::types::DeviceUse;

const STORE: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore";

struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        unsafe {
            let _ = RegCloseKey(self.0);
        }
    }
}

fn open(root: HKEY, path: &str) -> Option<Key> {
    let p = wide(path);
    let mut h = HKEY::default();
    let rc = unsafe { RegOpenKeyExW(root, PCWSTR(p.as_ptr()), Some(0), KEY_READ, &mut h) };
    (rc == ERROR_SUCCESS).then_some(Key(h))
}

fn subkeys(k: &Key) -> Vec<String> {
    let mut out = vec![];
    for i in 0.. {
        let mut buf = [0u16; 512];
        let mut len = buf.len() as u32;
        let rc = unsafe { RegEnumKeyExW(k.0, i, Some(PWSTR(buf.as_mut_ptr())), &mut len, None, None, None, None) };
        if rc != ERROR_SUCCESS {
            break;
        }
        out.push(String::from_utf16_lossy(&buf[..len as usize]));
    }
    out
}

fn qword(k: &Key, name: &str) -> Option<u64> {
    let n = wide(name);
    let mut data = 0u64;
    let mut size = 8u32;
    let rc = unsafe { RegQueryValueExW(k.0, PCWSTR(n.as_ptr()), None, None, Some(&mut data as *mut u64 as *mut u8), Some(&mut size)) };
    (rc == ERROR_SUCCESS && size == 8).then_some(data)
}

fn filetime_to_ms(ft: u64) -> i64 {
    (ft as i64 - 116_444_736_000_000_000) / 10_000
}

/// Приложение из записи реестра: путь к exe (NonPackaged, «\» заменены на «#») или имя пакета.
fn app_of(entry: &str, packaged: bool) -> (String, Option<String>) {
    if packaged {
        // Microsoft.WindowsCamera_8wekyb3d8bbwe → WindowsCamera
        let family = entry.split('_').next().unwrap_or(entry);
        let short = family.rsplit('.').next().unwrap_or(family);
        (short.to_string(), None)
    } else {
        let path = entry.replace('#', "\\");
        let stem = std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| path.clone());
        (stem, Some(path))
    }
}

fn scan(kind: &str, out: &mut Vec<DeviceUse>) {
    let base = format!(r"{STORE}\{kind}");
    let Some(root) = open(HKEY_CURRENT_USER, &base) else { return };
    let own = std::env::current_exe().ok().map(|p| p.to_string_lossy().to_lowercase());
    let mut check = |k: &Key, entry: &str, packaged: bool| {
        let start = qword(k, "LastUsedTimeStart").unwrap_or(0);
        let stop = qword(k, "LastUsedTimeStop").unwrap_or(1);
        if start == 0 || stop != 0 {
            return;
        }
        let (name, path) = app_of(entry, packaged);
        if path.as_ref().map(|p| Some(p.to_lowercase()) == own).unwrap_or(false) {
            return;
        }
        out.push(DeviceUse {
            device: kind.replace("webcam", "camera"),
            app_name: name,
            app_path: path,
            app_id: packaged.then(|| entry.to_string()),
            since: filetime_to_ms(start),
        });
    };
    for entry in subkeys(&root) {
        if entry == "NonPackaged" {
            let Some(np) = open(HKEY_CURRENT_USER, &format!(r"{base}\NonPackaged")) else { continue };
            for e in subkeys(&np) {
                if let Some(k) = open(HKEY_CURRENT_USER, &format!(r"{base}\NonPackaged\{e}")) {
                    check(&k, &e, false);
                }
            }
        } else if let Some(k) = open(HKEY_CURRENT_USER, &format!(r"{base}\{entry}")) {
            check(&k, &entry, true);
        }
    }
}

/// Запасной способ для микрофона: активные сеансы записи звука (WASAPI).
/// Работает, даже когда Windows не ведёт журнал в реестре — например, если
/// «твикер» отключил службу «Диспетчер доступа к возможностям» (camsvc).
fn capture_sessions(out: &mut Vec<DeviceUse>) {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    use windows::core::Interface;
    use windows::Win32::Media::Audio::{IAudioSessionControl2, IAudioSessionManager2, DEVICE_STATE_ACTIVE};
    // Когда сеанс замечен впервые — чтобы длительность звонка не сбрасывалась.
    static SINCE: OnceLock<Mutex<HashMap<String, i64>>> = OnceLock::new();
    ensure_com();
    let own_pid = std::process::id();
    let own = std::env::current_exe().ok().map(|p| p.to_string_lossy().to_lowercase());
    let mut found: Vec<String> = Vec::new();
    unsafe {
        let Ok(en) = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) else { return };
        let Ok(col) = en.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE) else { return };
        for i in 0..col.GetCount().unwrap_or(0) {
            let Ok(dev) = col.Item(i) else { continue };
            let Ok(mgr) = dev.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else { continue };
            let Ok(list) = mgr.GetSessionEnumerator() else { continue };
            for j in 0..list.GetCount().unwrap_or(0) {
                let Ok(ctl) = list.GetSession(j) else { continue };
                // 1 = AudioSessionStateActive: программа сейчас пишет звук.
                if ctl.GetState().map(|s| s.0).unwrap_or(0) != 1 {
                    continue;
                }
                let Ok(c2) = ctl.cast::<IAudioSessionControl2>() else { continue };
                let pid = c2.GetProcessId().unwrap_or(0);
                if pid == 0 || pid == own_pid {
                    continue;
                }
                let Some(path) = super::foreground::process_path(pid) else { continue };
                let low = path.to_lowercase();
                if Some(&low) == own.as_ref() || found.contains(&low) {
                    continue;
                }
                found.push(low);
                let known = out.iter().any(|u| u.device == "microphone" && u.app_path.as_deref().map(|p| p.eq_ignore_ascii_case(&path)).unwrap_or(false));
                if known {
                    continue;
                }
                let stem = std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| path.clone());
                out.push(DeviceUse { device: "microphone".into(), app_name: stem, app_path: Some(path), app_id: None, since: 0 });
            }
        }
    }
    let mut since = SINCE.get_or_init(Default::default).lock().unwrap_or_else(|p| p.into_inner());
    since.retain(|k, _| found.contains(k));
    let now = crate::database::now_ms();
    for u in out.iter_mut().filter(|u| u.since == 0) {
        let key = u.app_path.as_deref().unwrap_or("").to_lowercase();
        u.since = *since.entry(key).or_insert(now);
    }
}

pub fn in_use() -> Vec<DeviceUse> {
    let mut out = vec![];
    // DI_PRIVACY_NO_REGISTRY — проверка запасного способа, как будто журнала в реестре нет.
    if std::env::var_os("DI_PRIVACY_NO_REGISTRY").is_none() {
        scan("microphone", &mut out);
    }
    capture_sessions(&mut out);
    scan("webcam", &mut out);
    out
}

fn capture_endpoint() -> AppResult<IAudioEndpointVolume> {
    ensure_com();
    unsafe {
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let dev = en.GetDefaultAudioEndpoint(eCapture, eCommunications).map_err(|_| AppError::unsupported("микрофон не найден"))?;
        Ok(dev.Activate(CLSCTX_ALL, None)?)
    }
}

/// Выключен ли микрофон по умолчанию (для звонков).
pub fn mic_muted() -> Option<bool> {
    capture_endpoint().ok().and_then(|v| unsafe { v.GetMute().ok() }).map(|b| b.as_bool())
}

pub fn set_mic_muted(muted: bool) -> AppResult<()> {
    let v = capture_endpoint()?;
    unsafe { v.SetMute(muted, std::ptr::null())? };
    Ok(())
}
