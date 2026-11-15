//! Индикатор микрофона и камеры (кто их сейчас использует) и звонки.
//! Звонок — это микрофон, занятый мессенджером или программой для созвонов:
//! остров показывает приложение, длительность и кнопку выключения микрофона.
//! Опрос раз в 2 с — чтение реестра (Windows) или `pactl` и /proc (Linux).

use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use tauri::AppHandle;

use crate::core::state::Shared;
use crate::core::task::TaskSlot;
use crate::database::now_ms;
use crate::events::{Activity, Priority};
use crate::platform::types::{DeviceUse, LaunchTarget};

pub const EVENT: &str = "privacy://state";

/// Приложения, у которых занятый микрофон почти всегда означает звонок.
const CALL_APPS: &[(&[&str], &str)] = &[
    (&["telegram", "ayugram", "kotatogram", "64gram"], "Telegram"),
    (&["discord", "vesktop", "legcord", "armcord", "webcord", "dorion", "equibop", "goofcord"], "Discord"),
    (&["zoom"], "Zoom"),
    (&["teams"], "Microsoft Teams"),
    (&["skype"], "Skype"),
    (&["whatsapp"], "WhatsApp"),
    (&["viber"], "Viber"),
    (&["slack"], "Slack"),
    (&["element"], "Element"),
    (&["signal"], "Signal"),
];

pub fn call_app(u: &DeviceUse) -> Option<&'static str> {
    if u.device != "microphone" {
        return None;
    }
    let hay = format!("{} {} {}", u.app_name, u.app_path.as_deref().unwrap_or(""), u.app_id.as_deref().unwrap_or("")).to_lowercase();
    CALL_APPS.iter().find(|(keys, _)| keys.iter().any(|k| hay.contains(k))).map(|(_, name)| *name)
}

/// Красивое имя приложения для индикатора.
fn display_name(u: &DeviceUse) -> String {
    if let Some(n) = call_app(u) {
        return n.to_string();
    }
    if let Some(k) = crate::integrations::catalog::match_app(&u.app_name, u.app_path.as_deref()) {
        return k.name.to_string();
    }
    let mut c = u.app_name.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

#[derive(Debug, Clone, Serialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct PrivacyState {
    pub microphone: Vec<String>,
    pub camera: Vec<String>,
    pub mic_muted: Option<bool>,
}

#[derive(Default)]
pub struct PrivacyModule {
    task: TaskSlot,
    state: Mutex<PrivacyState>,
    /// Иконки приложений (путь → data URL).
    icons: Mutex<HashMap<String, Option<String>>>,
    /// Путь/имя приложения текущего звонка — для кнопки «Открыть».
    call: Mutex<Option<(String, Option<String>)>>,
}

impl PrivacyModule {
    pub fn state(&self) -> PrivacyState {
        self.state.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    pub fn set_running(&self, app: &AppHandle, st: &Shared, run: bool) {
        if run == self.task.is_running() {
            return;
        }
        if !run {
            self.task.stop();
            *self.state.lock().unwrap_or_else(|p| p.into_inner()) = PrivacyState::default();
            st.events.emit(EVENT, PrivacyState::default());
            st.events.clear_activity("call");
            return;
        }
        let _ = app;
        let st = st.clone();
        self.task.start(async move {
            loop {
                let polled = tauri::async_runtime::spawn_blocking(|| (crate::platform::os::privacy::in_use(), crate::platform::os::privacy::mic_muted())).await;
                if let Ok((uses, muted)) = polled {
                    st.privacy.apply(&st, uses, muted).await;
                }
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });
    }

    async fn icon(&self, path: &str) -> Option<String> {
        if let Some(hit) = self.icons.lock().unwrap_or_else(|p| p.into_inner()).get(path) {
            return hit.clone();
        }
        let target = LaunchTarget::Exe { path: path.to_string(), args: vec![] };
        let icon = tauri::async_runtime::spawn_blocking(move || crate::platform::os::apps::icon(&target, None, 48).ok().flatten()).await.ok().flatten();
        self.icons.lock().unwrap_or_else(|p| p.into_inner()).insert(path.to_string(), icon.clone());
        icon
    }

    /// Имя и иконка: для приложений Microsoft Store — из списка установленных приложений.
    async fn name_icon(&self, st: &Shared, u: &DeviceUse) -> (String, Option<String>) {
        if let Some(fam) = &u.app_id {
            if let Ok(entries) = st.apps.all_entries(st).await {
                let key = format!("{}!", fam.to_lowercase());
                if let Some(e) = entries.iter().find(|e| e.id.to_lowercase().contains(&key)) {
                    let icon = st.apps.icon(st, &e.id).await.ok().flatten();
                    return (call_app(u).map(str::to_string).unwrap_or_else(|| e.name.clone()), icon);
                }
            }
        }
        let icon = match u.app_path.as_deref() {
            Some(p) => self.icon(p).await,
            None => None,
        };
        (display_name(u), icon)
    }

    async fn apply(&self, st: &Shared, uses: Vec<DeviceUse>, muted: Option<bool>) {
        let mut next = PrivacyState { mic_muted: muted, ..Default::default() };
        let mut named = Vec::with_capacity(uses.len());
        for u in &uses {
            named.push(self.name_icon(st, u).await);
        }
        for (u, (name, _)) in uses.iter().zip(named.iter()) {
            let name = name.clone();
            let list = if u.device == "camera" { &mut next.camera } else { &mut next.microphone };
            if !list.contains(&name) {
                list.push(name);
            }
        }
        let changed = {
            let mut g = self.state.lock().unwrap_or_else(|p| p.into_inner());
            let changed = *g != next;
            *g = next.clone();
            changed
        };
        if changed {
            st.events.emit(EVENT, next.clone());
        }

        // Звонок: самый ранний микрофон из приложений для созвонов.
        let call = uses.iter().zip(named.iter()).filter(|(u, _)| call_app(u).is_some()).min_by_key(|(u, _)| u.since);
        match call {
            Some((u, (_, icon))) => {
                let name = call_app(u).unwrap_or("Звонок").to_string();
                let icon = icon.clone();
                let video = uses.iter().any(|c| c.device == "camera" && c.app_name == u.app_name);
                *self.call.lock().unwrap_or_else(|p| p.into_inner()) = Some((name.clone(), u.app_path.clone()));
                st.events.set_activity(Activity {
                    id: "call".into(),
                    kind: "call".into(),
                    priority: Priority::High,
                    title: name,
                    subtitle: Some(if video { "Видеозвонок".into() } else { "Звонок".into() }),
                    icon,
                    progress: muted.map(|m| if m { 0.0 } else { 1.0 }),
                    updated_at: now_ms(),
                    started_at: Some(u.since),
                });
            }
            None => {
                *self.call.lock().unwrap_or_else(|p| p.into_inner()) = None;
                st.events.clear_activity("call");
            }
        }
    }

    /// Включает/выключает микрофон по умолчанию. Возвращает новое состояние.
    pub fn toggle_mic(&self, st: &Shared) -> crate::core::error::AppResult<bool> {
        let muted = !crate::platform::os::privacy::mic_muted().unwrap_or(false);
        crate::platform::os::privacy::set_mic_muted(muted)?;
        {
            let mut g = self.state.lock().unwrap_or_else(|p| p.into_inner());
            g.mic_muted = Some(muted);
            st.events.emit(EVENT, g.clone());
        }
        Ok(muted)
    }

    /// Открывает приложение текущего звонка.
    pub fn open_call(&self, st: &Shared) -> crate::core::error::AppResult<()> {
        let call = self.call.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let Some((name, path)) = call else { return Ok(()) };
        crate::modules::applications::open_by_hint(st, path.as_deref().unwrap_or(&name), &name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u(device: &str, name: &str, path: Option<&str>) -> DeviceUse {
        DeviceUse { device: device.into(), app_name: name.into(), app_path: path.map(Into::into), app_id: None, since: 1 }
    }

    #[test]
    fn calls() {
        assert_eq!(call_app(&u("microphone", "Telegram", Some(r"D:\Games\Telegram Desktop\Telegram.exe"))), Some("Telegram"));
        assert_eq!(call_app(&u("microphone", "Zoom", None)), Some("Zoom"));
        assert_eq!(call_app(&u("camera", "Discord", None)), None);
        assert_eq!(call_app(&u("microphone", "Vesktop", Some("D:/Apps/Vesktop/Vesktop.exe"))), Some("Discord"));
        assert_eq!(call_app(&u("microphone", "chrome", None)), None);
        let teams = DeviceUse { app_id: Some("MSTeams_8wekyb3d8bbwe".into()), ..u("microphone", "MSTeams", None) };
        assert_eq!(call_app(&teams), Some("Microsoft Teams"));
    }
}
