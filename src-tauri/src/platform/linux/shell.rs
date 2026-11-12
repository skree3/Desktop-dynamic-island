//! Открытие URL и системных настроек в Linux (GNOME / KDE Plasma / xdg-open).

use std::process::{Command, Stdio};

use super::{command_exists, desktop_name};
use crate::core::error::{AppError, AppResult};
use crate::platform::types::SettingsPage;

fn spawn(program: &str, args: &[&str]) -> AppResult<()> {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| AppError::system(format!("не удалось запустить {program}: {e}")))
}

pub fn open_uri(uri: &str) -> AppResult<()> {
    if !command_exists("xdg-open") {
        return Err(AppError::unsupported("xdg-open не найден (пакет xdg-utils)"));
    }
    spawn("xdg-open", &[uri])
}

pub fn open_settings(page: SettingsPage) -> AppResult<()> {
    let de = desktop_name().to_uppercase();
    if de.contains("KDE") && command_exists("systemsettings") {
        let kcm = match page {
            SettingsPage::Main => None,
            SettingsPage::Bluetooth => Some("kcm_bluetooth"),
            SettingsPage::Display => Some("kcm_kscreen"),
            SettingsPage::Sound => Some("kcm_pulseaudio"),
            SettingsPage::Network | SettingsPage::Wifi | SettingsPage::Airplane => Some("kcm_networkmanagement"),
            SettingsPage::Battery | SettingsPage::Power => Some("kcm_powerdevilprofilesconfig"),
            SettingsPage::Notifications => Some("kcm_notifications"),
            SettingsPage::Apps => Some("kcm_componentchooser"),
        };
        return match kcm {
            Some(k) => spawn("systemsettings", &[k]),
            None => spawn("systemsettings", &[]),
        };
    }
    if command_exists("gnome-control-center") {
        let panel = match page {
            SettingsPage::Main => None,
            SettingsPage::Bluetooth => Some("bluetooth"),
            SettingsPage::Display => Some("display"),
            SettingsPage::Sound => Some("sound"),
            SettingsPage::Network => Some("network"),
            SettingsPage::Wifi | SettingsPage::Airplane => Some("wifi"),
            SettingsPage::Battery | SettingsPage::Power => Some("power"),
            SettingsPage::Notifications => Some("notifications"),
            SettingsPage::Apps => Some("applications"),
        };
        return match panel {
            Some(p) => spawn("gnome-control-center", &[p]),
            None => spawn("gnome-control-center", &[]),
        };
    }
    Err(AppError::unsupported(format!(
        "не удалось определить приложение настроек для окружения «{}»",
        if de.is_empty() { "неизвестно" } else { &de }
    )))
}
