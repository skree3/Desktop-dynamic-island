//! Настройки, карточки, профили, темы, горячие клавиши, журнал событий.

use tauri::{AppHandle, State};

use crate::core::error::{AppError, AppResult};
use crate::core::hotkeys::{self, HotkeyStatus};
use crate::core::settings::Settings;
use crate::core::state::Shared;
use crate::database::repo::{HotkeyRow, ProfileRow, ThemeRow, WidgetSetting};
use crate::logging::{category, LogEntry};
use crate::modules::CardInfo;

#[tauri::command]
pub async fn settings_get(state: State<'_, Shared>) -> AppResult<Settings> {
    Ok(state.settings())
}

#[tauri::command]
pub async fn settings_update(app: AppHandle, state: State<'_, Shared>, section: String, value: serde_json::Value) -> AppResult<Settings> {
    let st = state.inner().clone();
    st.update_settings(&app, |s| s.apply_section(&section, value))
}

#[tauri::command]
pub async fn settings_reset(app: AppHandle, state: State<'_, Shared>, section: String) -> AppResult<Settings> {
    let st = state.inner().clone();
    let defaults = Settings::default().section_value(&section)?;
    st.update_settings(&app, |s| {
        let mut whole = serde_json::to_value(&*s)?;
        whole[&section] = defaults;
        *s = serde_json::from_value(whole)?;
        Ok(())
    })
}

#[tauri::command]
pub async fn cards_list(state: State<'_, Shared>) -> AppResult<Vec<CardInfo>> {
    Ok(crate::modules::list_cards(state.inner()))
}

#[tauri::command]
pub async fn cards_update(state: State<'_, Shared>, cards: Vec<WidgetSetting>) -> AppResult<Vec<CardInfo>> {
    let st = state.inner();
    if cards.len() > 200 {
        return Err(AppError::invalid("слишком много карточек"));
    }
    for mut c in cards {
        let known = crate::modules::CARDS.iter().any(|(id, ..)| *id == c.card_id) || c.card_id.starts_with("plugin:");
        if !known || c.card_id.len() > 100 {
            continue;
        }
        c.priority = c.priority.clamp(0, 3);
        c.sort_order = c.sort_order.clamp(0, 10_000);
        st.db.upsert_widget(&c)?;
    }
    let list = crate::modules::list_cards(st);
    st.events.emit("widgets://changed", list.clone());
    Ok(list)
}

// ---- профили ----

#[tauri::command]
pub async fn profiles_list(state: State<'_, Shared>) -> AppResult<Vec<ProfileRow>> {
    state.db.list_profiles()
}

#[tauri::command]
pub async fn profile_create(state: State<'_, Shared>, name: String) -> AppResult<ProfileRow> {
    let st = state.inner();
    let name = crate::util::clean_text(&name, 48);
    if name.is_empty() {
        return Err(AppError::invalid("укажите название профиля"));
    }
    if st.db.list_profiles()?.len() >= 30 {
        return Err(AppError::invalid("слишком много профилей"));
    }
    let row = ProfileRow { id: format!("p-{}", uuid::Uuid::new_v4()), name, builtin: false, data: crate::core::profiles::snapshot(st)?, updated_at: 0 };
    st.db.upsert_profile(&row)?;
    Ok(row)
}

#[tauri::command]
pub async fn profile_save_current(state: State<'_, Shared>, id: String) -> AppResult<()> {
    let st = state.inner();
    let mut p = st.db.get_profile(&id)?.ok_or_else(|| AppError::NotFound("профиль не найден".into()))?;
    p.data = crate::core::profiles::snapshot(st)?;
    st.db.upsert_profile(&p)
}

#[tauri::command]
pub async fn profile_rename(state: State<'_, Shared>, id: String, name: String) -> AppResult<()> {
    let st = state.inner();
    let mut p = st.db.get_profile(&id)?.ok_or_else(|| AppError::NotFound("профиль не найден".into()))?;
    p.name = crate::util::clean_text(&name, 48);
    if p.name.is_empty() {
        return Err(AppError::invalid("укажите название"));
    }
    st.db.upsert_profile(&p)
}

#[tauri::command]
pub async fn profile_apply(app: AppHandle, state: State<'_, Shared>, id: String) -> AppResult<()> {
    crate::core::profiles::apply(&app, state.inner(), &id, true)
}

#[tauri::command]
pub async fn profile_delete(app: AppHandle, state: State<'_, Shared>, id: String) -> AppResult<()> {
    let st = state.inner().clone();
    st.db.delete_profile(&id)?;
    if st.with_settings(|s| s.profiles.active.as_deref() == Some(id.as_str())) {
        st.update_settings(&app, |s| {
            s.profiles.active = None;
            Ok(())
        })?;
    }
    Ok(())
}

// ---- темы ----

#[tauri::command]
pub async fn themes_list(state: State<'_, Shared>) -> AppResult<Vec<ThemeRow>> {
    state.db.list_themes()
}

#[tauri::command]
pub async fn theme_delete(app: AppHandle, state: State<'_, Shared>, id: String) -> AppResult<()> {
    let st = state.inner().clone();
    if st.db.delete_theme(&id)? == 0 {
        return Err(AppError::invalid("встроенные темы удалить нельзя"));
    }
    if st.with_settings(|s| s.appearance.theme_id == id) {
        st.update_settings(&app, |s| {
            s.appearance.theme_id = "midnight".into();
            Ok(())
        })?;
    }
    st.events.emit(crate::events::names::THEME_CHANGED, st.db.list_themes()?);
    Ok(())
}

// ---- горячие клавиши ----

#[tauri::command]
pub async fn hotkeys_list(state: State<'_, Shared>) -> AppResult<Vec<HotkeyStatus>> {
    Ok(state.hotkeys.status())
}

#[tauri::command]
pub async fn hotkey_set(app: AppHandle, state: State<'_, Shared>, action: String, accelerator: String, enabled: bool) -> AppResult<Vec<HotkeyStatus>> {
    let st = state.inner().clone();
    if !hotkeys::ACTIONS.iter().any(|(a, ..)| *a == action) {
        return Err(AppError::invalid("неизвестное действие"));
    }
    let acc = accelerator.trim().to_string();
    if !acc.is_empty() {
        hotkeys::validate_accelerator(&acc).map_err(AppError::invalid)?;
        let dup = st.db.list_hotkeys()?.into_iter().any(|h| h.action != action && h.enabled && h.accelerator.eq_ignore_ascii_case(&acc));
        if dup && enabled {
            return Err(AppError::invalid("это сочетание уже назначено другому действию"));
        }
    }
    st.db.upsert_hotkey(&HotkeyRow { action, accelerator: acc, enabled })?;
    st.hotkeys.register_all(&app, &st);
    Ok(st.hotkeys.status())
}

#[tauri::command]
pub async fn hotkeys_reset(app: AppHandle, state: State<'_, Shared>) -> AppResult<Vec<HotkeyStatus>> {
    let st = state.inner().clone();
    for (a, _, acc) in hotkeys::ACTIONS {
        st.db.upsert_hotkey(&HotkeyRow { action: a.to_string(), accelerator: acc.to_string(), enabled: true })?;
    }
    st.hotkeys.register_all(&app, &st);
    Ok(st.hotkeys.status())
}

/// Приостанавливает глобальные сочетания, пока пользователь записывает новое.
#[tauri::command]
pub async fn hotkeys_suspend(app: AppHandle, state: State<'_, Shared>, suspended: bool) -> AppResult<()> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    if suspended {
        let _ = app.global_shortcut().unregister_all();
    } else {
        state.hotkeys.register_all(&app, state.inner());
    }
    Ok(())
}

// ---- журнал ----

#[tauri::command]
pub async fn logs_list(state: State<'_, Shared>, limit: Option<u32>, category: Option<String>, level: Option<String>) -> AppResult<Vec<LogEntry>> {
    state.log.list(limit.unwrap_or(300), category.as_deref(), level.as_deref())
}

#[tauri::command]
pub async fn logs_clear(state: State<'_, Shared>) -> AppResult<()> {
    state.log.clear()?;
    state.log.info(category::APP, "logs", "журнал событий очищен пользователем");
    Ok(())
}

/// Сохраняет свою тему (из редактора стиля): тот же формат, что и импорт.
#[tauri::command]
pub async fn theme_save(state: State<'_, Shared>, json: String) -> AppResult<ThemeRow> {
    let row = crate::core::themes::import(state.inner(), &json)?;
    state.events.emit("theme://changed", ());
    Ok(row)
}

/// Выбор своих обоев окна настроек: файл копируется в папку программы.
#[tauri::command]
pub async fn wallpaper_pick(app: AppHandle, state: State<'_, Shared>) -> AppResult<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    let app2 = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app2.dialog().file().set_title("Обои для окна настроек").add_filter("Изображения", &["jpg", "jpeg", "png", "webp"]).blocking_pick_file()
    })
    .await?;
    let Some(file) = picked.and_then(|f| f.into_path().ok()) else { return Ok(None) };
    let ext = file.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).filter(|e| ["jpg", "jpeg", "png", "webp"].contains(&e.as_str()));
    let Some(ext) = ext else { return Err(AppError::invalid("нужна картинка JPG, PNG или WebP")) };
    if std::fs::metadata(&file)?.len() > 25 * 1024 * 1024 {
        return Err(AppError::invalid("картинка больше 25 МБ"));
    }
    let st = state.inner().clone();
    for old in ["jpg", "jpeg", "png", "webp"] {
        let _ = std::fs::remove_file(st.paths.data.join(format!("wallpaper.{old}")));
    }
    let name = format!("wallpaper.{ext}");
    std::fs::copy(&file, st.paths.data.join(&name))?;
    st.update_settings(&app, |s| {
        s.appearance.wallpaper = Some(name.clone());
        Ok(())
    })?;
    Ok(Some(name))
}

#[tauri::command]
pub async fn wallpaper_clear(app: AppHandle, state: State<'_, Shared>) -> AppResult<()> {
    let st = state.inner().clone();
    for old in ["jpg", "jpeg", "png", "webp"] {
        let _ = std::fs::remove_file(st.paths.data.join(format!("wallpaper.{old}")));
    }
    st.update_settings(&app, |s| {
        s.appearance.wallpaper = None;
        Ok(())
    })?;
    Ok(())
}

/// Обои в виде data-URL.
#[tauri::command]
pub async fn wallpaper_data(state: State<'_, Shared>) -> AppResult<Option<String>> {
    use base64::Engine;
    let Some(name) = state.with_settings(|s| s.appearance.wallpaper.clone()) else { return Ok(None) };
    let mime = if name.ends_with(".png") { "image/png" } else if name.ends_with(".webp") { "image/webp" } else { "image/jpeg" };
    let bytes = std::fs::read(state.paths.data.join(&name))?;
    Ok(Some(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))))
}
