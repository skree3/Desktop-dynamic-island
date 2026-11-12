//! Функциональные модули. Каждый модуль независим, включается/отключается
//! в настройках, а его фоновые задачи запускаются только при необходимости
//! (`reconcile`). Это ключевой механизм экономии CPU и памяти.

pub mod applications;
pub mod battery;
pub mod clipboard;
pub mod game_mode;
pub mod music;
pub mod notes;
pub mod notifications;
pub mod shelf;
pub mod system_monitor;
pub mod timers;
pub mod updater;

use serde::Serialize;
use tauri::AppHandle;

use crate::core::state::Shared;
use crate::database::repo::WidgetSetting;

/// Карточки, видимые в панели по умолчанию (остальные включаются в настройках).
pub const DEFAULT_VISIBLE: &[&str] = &["music", "quick", "timers", "notifications", "notes", "shelf"];

/// Встроенные карточки: (id, название, модуль, приоритет по умолчанию).
pub const CARDS: &[(&str, &str, &str, i64)] = &[
    ("music", "Музыка", "music", 2),
    ("quick", "Центр управления", "quick_settings", 1),
    ("timers", "Таймеры и часы", "timers", 2),
    ("notifications", "Уведомления", "notifications", 2),
    ("notes", "Заметки", "notes", 1),
    ("shelf", "Полка", "shelf", 1),
    ("system", "Системный монитор", "system_monitor", 1),
    ("bluetooth", "Bluetooth", "bluetooth", 1),
    ("clipboard", "Буфер обмена", "clipboard", 1),
    ("apps", "Приложения", "applications", 1),
    ("ai", "Нейросети", "ai", 1),
    ("integrations", "Интеграции", "integrations", 1),
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardInfo {
    pub id: String,
    pub title: String,
    pub module: String,
    pub module_enabled: bool,
    pub enabled: bool,
    pub sort_order: i64,
    pub priority: i64,
    pub plugin_id: Option<String>,
}

pub fn ensure_widgets(st: &Shared) {
    let existing = st.db.list_widgets().unwrap_or_default();
    for (i, (id, _, _, prio)) in CARDS.iter().enumerate() {
        if !existing.iter().any(|w| w.card_id == *id) {
            let _ = st.db.upsert_widget(&WidgetSetting {
                card_id: id.to_string(),
                enabled: DEFAULT_VISIBLE.contains(id),
                sort_order: i as i64,
                priority: *prio,
                config: serde_json::json!({}),
            });
        }
    }
}

pub fn list_cards(st: &Shared) -> Vec<CardInfo> {
    let s = st.settings();
    let widgets = st.db.list_widgets().unwrap_or_default();
    let mut out = vec![];
    for (i, (id, title, module, prio)) in CARDS.iter().enumerate() {
        let w = widgets.iter().find(|w| w.card_id == *id);
        out.push(CardInfo {
            id: id.to_string(),
            title: title.to_string(),
            module: module.to_string(),
            module_enabled: s.modules.is_enabled(module),
            enabled: w.map(|w| w.enabled).unwrap_or(true),
            sort_order: w.map(|w| w.sort_order).unwrap_or(i as i64),
            priority: w.map(|w| w.priority).unwrap_or(*prio),
            plugin_id: None,
        });
    }
    if s.modules.plugins && s.security.plugins_enabled {
        for p in st.plugins.enabled_cards() {
            let card_id = format!("plugin:{}", p.0);
            let w = widgets.iter().find(|w| w.card_id == card_id);
            out.push(CardInfo {
                id: card_id,
                title: p.1,
                module: "plugins".into(),
                module_enabled: true,
                enabled: w.map(|w| w.enabled).unwrap_or(true),
                sort_order: w.map(|w| w.sort_order).unwrap_or(100),
                priority: w.map(|w| w.priority).unwrap_or(1),
                plugin_id: Some(p.0),
            });
        }
    }
    out.sort_by_key(|c| c.sort_order);
    out
}

/// Приводит запущенные фоновые задачи в соответствие с настройками и состоянием UI.
pub fn reconcile(app: &AppHandle, st: &Shared) {
    let s = st.settings();
    let game = st.game_mode_active();
    let ui_live = st.island_visible() || !s.performance.pause_when_hidden;

    st.music.set_running(app, st, s.modules.music && !game && ui_live);
    let sys_needed = st.flags.card_active("system") || s.system_monitor.show_in_compact;
    st.sysmon.set_running(app, st, s.modules.system_monitor && !game && ui_live && sys_needed);
    st.clipboard.set_running(app, st, s.modules.clipboard && s.clipboard.history_enabled);
    st.notifications.set_running(app, st, s.modules.notifications && s.notifications.enabled && s.notifications.listen_system);
    st.timers.set_running(app, st, s.modules.timers);
    st.game_mode.ensure_running(app, st);
    st.battery.set_running(app, st, s.modules.battery);
    st.updater.set_running(app, st, s.general.check_updates);
}
