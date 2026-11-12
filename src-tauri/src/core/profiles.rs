//! Профили настроек: снимок разделов (внешний вид, положение, модули,
//! уведомления, частоты опроса…) и настроек карточек. Автопереключение по
//! условиям: игровой режим, работа от батареи, интервал времени.

use std::sync::Mutex;
use tauri::AppHandle;

use crate::core::error::{AppError, AppResult};
use crate::core::settings::{ProfileCondition, PROFILE_SECTIONS};
use crate::core::state::Shared;
use crate::database::repo::ProfileRow;
use crate::logging::category;

/// Профиль, активный до автопереключения (чтобы вернуться к нему).
static RESTORE_TO: Mutex<Option<String>> = Mutex::new(None);

pub fn snapshot(st: &Shared) -> AppResult<serde_json::Value> {
    let s = st.settings();
    let mut obj = serde_json::Map::new();
    for sec in PROFILE_SECTIONS {
        obj.insert(sec.to_string(), s.section_value(sec)?);
    }
    obj.insert("widgets".into(), serde_json::to_value(st.db.list_widgets()?)?);
    Ok(serde_json::Value::Object(obj))
}

pub fn ensure_builtin(st: &Shared) {
    if !st.db.list_profiles().unwrap_or_default().is_empty() {
        return;
    }
    let Ok(base) = snapshot(st) else { return };
    let variant = |patch: serde_json::Value| {
        let mut v = base.clone();
        crate::core::settings::merge_json(&mut v, patch);
        v
    };
    let builtin = [
        ("normal", "Обычный", variant(serde_json::json!({}))),
        (
            "gaming",
            "Игровой",
            variant(serde_json::json!({
                "notifications": { "dnd": true },
                "systemMonitor": { "intervalMs": 5000 },
                "appearance": { "animations": "reduced" }
            })),
        ),
        (
            "work",
            "Рабочий",
            variant(serde_json::json!({
                "notifications": { "dnd": false, "durationSecs": 8 },
                "systemMonitor": { "showInCompact": false }
            })),
        ),
        (
            "saver",
            "Энергосбережение",
            variant(serde_json::json!({
                "appearance": { "animations": "off" },
                "systemMonitor": { "intervalMs": 10000 },
                "music": { "pollIntervalMs": 3000 },
                "modules": { "systemMonitor": false }
            })),
        ),
    ];
    for (id, name, data) in builtin {
        let _ = st.db.upsert_profile(&ProfileRow { id: id.into(), name: name.into(), builtin: true, data, updated_at: 0 });
    }
}

pub fn apply(app: &AppHandle, st: &Shared, id: &str, manual: bool) -> AppResult<()> {
    let p = st.db.get_profile(id)?.ok_or_else(|| AppError::NotFound("профиль не найден".into()))?;
    st.replace_sections(app, &p.data)?;
    if let Some(widgets) = p.data.get("widgets").cloned() {
        if let Ok(list) = serde_json::from_value::<Vec<crate::database::repo::WidgetSetting>>(widgets) {
            for w in list {
                let _ = st.db.upsert_widget(&w);
            }
        }
    }
    st.update_settings(app, |s| {
        s.profiles.active = Some(id.to_string());
        Ok(())
    })?;
    if manual {
        *RESTORE_TO.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
    st.log.info(category::APP, "profiles", format!("применён профиль «{}»", p.name));
    st.events.emit("widgets://changed", st.db.list_widgets().unwrap_or_default());
    Ok(())
}

pub fn cycle(app: &AppHandle, st: &Shared) -> AppResult<()> {
    let list = st.db.list_profiles()?;
    if list.is_empty() {
        return Err(AppError::NotFound("профили не созданы".into()));
    }
    let active = st.with_settings(|s| s.profiles.active.clone());
    let idx = active.and_then(|a| list.iter().position(|p| p.id == a)).map(|i| (i + 1) % list.len()).unwrap_or(0);
    let p = &list[idx];
    apply(app, st, &p.id, true)?;
    st.events.push(crate::events::internal_notification(
        "profiles",
        "Desktop Island",
        format!("Профиль: {}", p.name),
        "",
        crate::events::Priority::High,
    ));
    Ok(())
}

fn hhmm_to_min(s: &str) -> Option<u32> {
    let (h, m) = s.split_once(':')?;
    Some(h.parse::<u32>().ok()? * 60 + m.parse::<u32>().ok()?)
}

fn local_minutes() -> u32 {
    // std не знает часовой пояс, поэтому смещение сообщает фронтенд (Date.getTimezoneOffset).
    let offset = crate::core::profiles::TZ_OFFSET_MIN.load(std::sync::atomic::Ordering::Relaxed);
    let now = crate::database::now_ms() / 60_000 + offset as i64;
    (now.rem_euclid(24 * 60)) as u32
}

/// Смещение часового пояса в минутах (сообщает фронтенд при старте).
pub static TZ_OFFSET_MIN: std::sync::atomic::AtomicI32 = std::sync::atomic::AtomicI32::new(0);

/// Проверяет правила автопереключения. Вызывается при смене игрового режима,
/// питания и раз в минуту.
pub fn evaluate_rules(app: &AppHandle, st: &Shared, on_battery: Option<bool>) {
    let s = st.settings();
    if !s.profiles.auto_switch {
        return;
    }
    let game = st.game_mode_active();
    let now_min = local_minutes();
    let matched = s.profiles.rules.iter().filter(|r| r.enabled).find(|r| match &r.condition {
        ProfileCondition::GameMode => game,
        ProfileCondition::OnBattery => on_battery == Some(true),
        ProfileCondition::TimeRange { from, to } => match (hhmm_to_min(from), hhmm_to_min(to)) {
            (Some(f), Some(t)) if f <= t => now_min >= f && now_min < t,
            (Some(f), Some(t)) => now_min >= f || now_min < t,
            _ => false,
        },
    });
    let active = s.profiles.active.clone();
    let mut restore = RESTORE_TO.lock().unwrap_or_else(|e| e.into_inner());
    match matched {
        Some(rule) if active.as_deref() != Some(rule.profile_id.as_str()) => {
            if restore.is_none() {
                *restore = active.clone().or(Some("normal".into()));
            }
            let id = rule.profile_id.clone();
            drop(restore);
            if let Err(e) = apply(app, st, &id, false) {
                st.log.warn(category::APP, "profiles", format!("автопереключение не удалось: {e}"));
            }
        }
        None => {
            if let Some(prev) = restore.take() {
                drop(restore);
                let _ = apply(app, st, &prev, false);
            }
        }
        _ => {}
    }
}
