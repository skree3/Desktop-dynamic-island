//! Чтение (только чтение!) системных настроек прокси из реестра пользователя.
//! Используется для отображения состояния прокси-приложений (Happ, INCY).
//! Desktop Island никогда не изменяет эти настройки.

use windows::core::PCWSTR;
use windows::Win32::System::Registry::{RegGetValueW, HKEY, HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RRF_RT_REG_SZ};

use super::wide;
use crate::platform::types::ProxyInfo;

pub fn read_reg_string(root: HKEY, key: &str, value: &str) -> Option<String> {
    let k = wide(key);
    let v = wide(value);
    let mut buf = vec![0u16; 2048];
    let mut size = (buf.len() * 2) as u32;
    let rc = unsafe {
        RegGetValueW(root, PCWSTR(k.as_ptr()), PCWSTR(v.as_ptr()), RRF_RT_REG_SZ, None, Some(buf.as_mut_ptr() as *mut _), Some(&mut size))
    };
    if rc.is_err() {
        return None;
    }
    let len = (size as usize / 2).saturating_sub(1);
    Some(String::from_utf16_lossy(&buf[..len.min(buf.len())]))
}

pub fn read_reg_dword(root: HKEY, key: &str, value: &str) -> Option<u32> {
    let k = wide(key);
    let v = wide(value);
    let mut data = 0u32;
    let mut size = 4u32;
    let rc = unsafe {
        RegGetValueW(root, PCWSTR(k.as_ptr()), PCWSTR(v.as_ptr()), RRF_RT_REG_DWORD, None, Some(&mut data as *mut _ as *mut _), Some(&mut size))
    };
    if rc.is_err() {
        None
    } else {
        Some(data)
    }
}

pub fn system_proxy() -> ProxyInfo {
    const KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";
    ProxyInfo {
        enabled: read_reg_dword(HKEY_CURRENT_USER, KEY, "ProxyEnable").unwrap_or(0) == 1,
        server: read_reg_string(HKEY_CURRENT_USER, KEY, "ProxyServer").filter(|s| !s.is_empty()),
        auto_config: read_reg_string(HKEY_CURRENT_USER, KEY, "AutoConfigURL").filter(|s| !s.is_empty()),
        source: "Реестр: Internet Settings (только чтение)".into(),
    }
}
