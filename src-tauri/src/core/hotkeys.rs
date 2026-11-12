//! Глобальные горячие клавиши (tauri-plugin-global-shortcut).
//! В Wayland глобальный перехват клавиш приложениями запрещён протоколом —
//! вместо этого пользователь может назначить сочетание в настройках окружения
//! рабочего стола на команду `desktop-island --toggle` (см. README).

use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};

use crate::core::state::Shared;
use crate::database::repo::HotkeyRow;
use crate::events::names;
use crate::logging::category;

pub const ACTIONS: &[(&str, &str, &str)] = &[
    ("toggle_island", "Показать / скрыть панель", "Ctrl+Alt+I"),
    ("expand_island", "Раскрыть карточку", "Ctrl+Alt+E"),
    ("app_search", "Поиск приложений", "Ctrl+Alt+Space"),
    ("media_play_pause", "Музыка: пауза / воспроизведение", "Ctrl+Alt+P"),
    ("media_next", "Музыка: следующий трек", "Ctrl+Alt+Right"),
    ("media_prev", "Музыка: предыдущий трек", "Ctrl+Alt+Left"),
    ("quick_settings", "Быстрые настройки", "Ctrl+Alt+Q"),
    ("toggle_dnd", "Режим «Не беспокоить»", "Ctrl+Alt+D"),
    ("next_profile", "Следующий профиль", "Ctrl+Alt+R"),
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyStatus {
    pub action: String,
    pub label: String,
    pub accelerator: String,
    pub enabled: bool,
    pub registered: bool,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct HotkeyManager {
    bound: Mutex<Vec<(Shortcut, String)>>,
    status: Mutex<Vec<HotkeyStatus>>,
}

pub fn ensure_defaults(st: &Shared) {
    let existing = st.db.list_hotkeys().unwrap_or_default();
    for (action, _, acc) in ACTIONS {
        if !existing.iter().any(|h| h.action == *action) {
            let _ = st.db.upsert_hotkey(&HotkeyRow { action: action.to_string(), accelerator: acc.to_string(), enabled: true });
        }
    }
}

pub fn validate_accelerator(acc: &str) -> Result<Shortcut, String> {
    if acc.len() > 64 {
        return Err("слишком длинное сочетание".into());
    }
    acc.parse::<Shortcut>().map_err(|e| format!("некорректное сочетание: {e}"))
}

impl HotkeyManager {
    pub fn status(&self) -> Vec<HotkeyStatus> {
        self.status.lock().map(|g| g.clone()).unwrap_or_default()
    }

    pub fn register_all(&self, app: &AppHandle, st: &Shared) {
        let gs = app.global_shortcut();
        let _ = gs.unregister_all();
        let supported = crate::platform::os::environment().global_hotkeys_supported;
        let rows = st.db.list_hotkeys().unwrap_or_default();
        let mut bound = vec![];
        let mut status = vec![];
        for (action, label, default) in ACTIONS {
            let row = rows.iter().find(|r| r.action == *action);
            let acc = row.map(|r| r.accelerator.clone()).unwrap_or_else(|| default.to_string());
            let enabled = row.map(|r| r.enabled).unwrap_or(true);
            let mut s = HotkeyStatus {
                action: action.to_string(),
                label: label.to_string(),
                accelerator: acc.clone(),
                enabled,
                registered: false,
                error: None,
            };
            if enabled && !acc.is_empty() {
                if !supported {
                    s.error = Some("глобальные сочетания недоступны в этом окружении (Wayland)".into());
                } else {
                    match validate_accelerator(&acc) {
                        Ok(sc) => match gs.register(sc) {
                            Ok(_) => {
                                s.registered = true;
                                bound.push((sc, action.to_string()));
                            }
                            Err(e) => {
                                s.error = Some(format!("сочетание занято другим приложением или недоступно: {e}"));
                                st.log.warn(category::PERMISSION, "hotkeys", format!("не удалось зарегистрировать {acc} для {action}"));
                            }
                        },
                        Err(e) => s.error = Some(e),
                    }
                }
            }
            status.push(s);
        }
        *self.bound.lock().unwrap_or_else(|p| p.into_inner()) = bound;
        *self.status.lock().unwrap_or_else(|p| p.into_inner()) = status;
    }

    fn action_for(&self, sc: &Shortcut) -> Option<String> {
        self.bound.lock().ok()?.iter().find(|(s, _)| s.id() == sc.id()).map(|(_, a)| a.clone())
    }
}

pub fn on_shortcut(app: &AppHandle, sc: &Shortcut) {
    let st = crate::core::state::shared(app);
    let Some(action) = st.hotkeys.action_for(sc) else { return };
    run_action(app, &st, &action);
}

pub fn run_action(app: &AppHandle, st: &Shared, action: &str) {
    use crate::core::window;
    let ui = |cmd: serde_json::Value| {
        if !st.island_visible() && !st.game_mode_active() {
            st.flags.user_hidden.store(false, std::sync::atomic::Ordering::SeqCst);
            window::refresh_visibility(app, st);
        }
        if let Some(w) = window::island(app) {
            let _ = w.set_focus();
        }
        let _ = app.emit_to(window::ISLAND, names::COMMAND, cmd);
    };
    match action {
        "toggle_island" => window::toggle_island(app, st),
        "expand_island" => ui(serde_json::json!({ "action": "expand" })),
        "app_search" => ui(serde_json::json!({ "action": "open-card", "card": "apps" })),
        "quick_settings" => ui(serde_json::json!({ "action": "open-card", "card": "quick" })),
        "media_play_pause" | "media_next" | "media_prev" => {
            let act = match action {
                "media_next" => crate::platform::types::MediaAction::Next,
                "media_prev" => crate::platform::types::MediaAction::Previous,
                _ => crate::platform::types::MediaAction::PlayPause,
            };
            let st = st.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = st.music.control_current(act).await {
                    tracing::debug!(error = %e, "горячая клавиша музыки: нет активной сессии");
                }
            });
        }
        "toggle_dnd" => {
            let on = !st.with_settings(|s| s.notifications.dnd);
            let _ = st.update_settings(app, |s| {
                s.notifications.dnd = on;
                Ok(())
            });
            st.events.push(crate::events::internal_notification(
                "dnd",
                "Desktop Island",
                if on { "«Не беспокоить» включено" } else { "«Не беспокоить» выключено" },
                "",
                crate::events::Priority::Critical,
            ));
        }
        "next_profile" => {
            if let Err(e) = crate::core::profiles::cycle(app, st) {
                st.log.warn(category::APP, "profiles", e.to_string());
            }
        }
        _ => {}
    }
}
