//! Открытие системных параметров, URL и файлов через ShellExecuteW.

use windows::core::PCWSTR;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::Win32::UI::Shell::ShellExecuteW;

use super::{ensure_com, wide};
use crate::core::error::{AppError, AppResult};
use crate::platform::types::SettingsPage;

pub fn shell_open(file: &str, args: Option<&str>, dir: Option<&str>) -> AppResult<()> {
    ensure_com();
    let op = wide("open");
    let f = wide(file);
    let a = args.map(wide);
    let d = dir.map(wide);
    let h = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(op.as_ptr()),
            PCWSTR(f.as_ptr()),
            a.as_ref().map(|v| PCWSTR(v.as_ptr())).unwrap_or(PCWSTR::null()),
            d.as_ref().map(|v| PCWSTR(v.as_ptr())).unwrap_or(PCWSTR::null()),
            SW_SHOWNORMAL,
        )
    };
    // По документации значение > 32 означает успех.
    let code = h.0 as isize;
    if code > 32 {
        Ok(())
    } else if code == 5 {
        Err(AppError::PermissionDenied("отказано в доступе при запуске".into()))
    } else if code == 2 || code == 3 {
        Err(AppError::NotFound(format!("файл или путь не найден: {file}")))
    } else {
        Err(AppError::system(format!("ShellExecute завершился с кодом {code}")))
    }
}

pub fn settings_uri(page: SettingsPage) -> &'static str {
    match page {
        SettingsPage::Main => "ms-settings:",
        SettingsPage::Bluetooth => "ms-settings:bluetooth",
        SettingsPage::Display => "ms-settings:display",
        SettingsPage::Sound => "ms-settings:sound",
        SettingsPage::Network => "ms-settings:network",
        SettingsPage::Wifi => "ms-settings:network-wifi",
        SettingsPage::Airplane => "ms-settings:network-airplanemode",
        SettingsPage::Battery => "ms-settings:batterysaver",
        SettingsPage::Notifications => "ms-settings:notifications",
        SettingsPage::Apps => "ms-settings:appsfeatures",
        SettingsPage::Power => "ms-settings:powersleep",
    }
}

pub fn open_settings(page: SettingsPage) -> AppResult<()> {
    shell_open(settings_uri(page), None, None)
}

pub fn open_uri(uri: &str) -> AppResult<()> {
    shell_open(uri, None, None)
}
