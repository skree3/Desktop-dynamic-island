//! Команды 1.1: раскладка и Caps Lock, режим «в самолёте», VPN, запуск от имени
//! администратора, свои шрифты и звуки.

use tauri::{AppHandle, State};

use crate::core::error::{AppError, AppResult};
use crate::core::state::Shared;
use crate::modules::assets::{self, FontFile};
use crate::modules::indicators::{self, KbdState, VpnStatus};
use crate::platform::os;

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> AppResult<T> + Send + 'static) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(f).await?
}

// ---- клавиатура ----

#[tauri::command]
pub async fn kbd_state() -> AppResult<KbdState> {
    blocking(|| Ok(KbdState { layout: os::keyboard::layout(), caps: os::keyboard::caps() })).await
}

#[tauri::command]
pub async fn kbd_next_layout() -> AppResult<()> {
    blocking(os::keyboard::next_layout).await
}

#[tauri::command]
pub async fn kbd_caps_toggle() -> AppResult<()> {
    blocking(os::keyboard::toggle_caps).await
}

// ---- режим «в самолёте» ----

#[tauri::command]
pub async fn airplane_set(state: State<'_, Shared>, on: bool) -> AppResult<()> {
    let st = state.inner().clone();
    let res = blocking(move || os::network::set_airplane(on)).await;
    match &res {
        Ok(_) => st.log.info(crate::logging::category::SYSTEM_ACTION, "airplane", format!("Режим «в самолёте» {}", if on { "включён" } else { "выключен" })),
        Err(e) => st.log.warn(crate::logging::category::SYSTEM_ACTION, "airplane", e.to_string()),
    }
    res
}

// ---- VPN ----

#[tauri::command]
pub async fn vpn_status(state: State<'_, Shared>) -> AppResult<VpnStatus> {
    let st = state.inner().clone();
    blocking(move || Ok(indicators::vpn_status(&st))).await
}

#[tauri::command]
pub async fn vpn_set(state: State<'_, Shared>, on: bool) -> AppResult<VpnStatus> {
    let st = state.inner().clone();
    blocking(move || indicators::vpn_set(&st, on)).await
}

#[tauri::command]
pub async fn vpn_pick_path(app: AppHandle, state: State<'_, Shared>) -> AppResult<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    let app2 = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        let d = app2.dialog().file().set_title("Программа VPN");
        #[cfg(windows)]
        let d = d.add_filter("Программы", &["exe"]);
        d.blocking_pick_file()
    })
    .await?;
    let Some(file) = picked.and_then(|f| f.into_path().ok()) else { return Ok(None) };
    let path = file.to_string_lossy().to_string();
    let st = state.inner().clone();
    st.update_settings(&app, |s| {
        s.vpn.path = path.clone();
        Ok(())
    })?;
    Ok(Some(path))
}

// ---- приложения ----

#[tauri::command]
pub async fn apps_launch_admin(state: State<'_, Shared>, id: String) -> AppResult<()> {
    let st = state.inner().clone();
    st.apps.launch_admin(&st, &id).await
}

// ---- шрифты ----

#[tauri::command]
pub async fn fonts_list(state: State<'_, Shared>) -> AppResult<Vec<FontFile>> {
    let data = state.paths.data.clone();
    blocking(move || Ok(assets::list_fonts(&data))).await
}

#[tauri::command]
pub async fn font_pick(app: AppHandle, state: State<'_, Shared>) -> AppResult<Option<FontFile>> {
    use tauri_plugin_dialog::DialogExt;
    let app2 = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || app2.dialog().file().set_title("Шрифт для острова").add_filter("Шрифты", &["ttf", "otf"]).blocking_pick_file()).await?;
    let Some(file) = picked.and_then(|f| f.into_path().ok()) else { return Ok(None) };
    let data = state.paths.data.clone();
    let font = blocking(move || assets::import_font(&data, &file)).await?;
    Ok(Some(font))
}

#[tauri::command]
pub async fn font_data(state: State<'_, Shared>, file: String) -> AppResult<String> {
    let data = state.paths.data.clone();
    blocking(move || assets::font_data(&data, &file)).await
}

#[tauri::command]
pub async fn font_remove(app: AppHandle, state: State<'_, Shared>, file: String) -> AppResult<()> {
    let data = state.paths.data.clone();
    let f2 = file.clone();
    blocking(move || assets::remove_font(&data, &f2)).await?;
    let st = state.inner().clone();
    if st.with_settings(|s| s.appearance.font == format!("custom:{file}")) {
        st.update_settings(&app, |s| {
            s.appearance.font = "system".into();
            Ok(())
        })?;
    }
    Ok(())
}

// ---- звуки ----

#[tauri::command]
pub async fn sound_pick(app: AppHandle, state: State<'_, Shared>, kind: String) -> AppResult<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    if kind != "notification" && kind != "timer" {
        return Err(AppError::invalid("неизвестный тип звука"));
    }
    let app2 = app.clone();
    let picked = tauri::async_runtime::spawn_blocking(move || app2.dialog().file().set_title("Свой звук").add_filter("Звук", &["wav", "mp3", "ogg"]).blocking_pick_file()).await?;
    let Some(file) = picked.and_then(|f| f.into_path().ok()) else { return Ok(None) };
    let data = state.paths.data.clone();
    let k = kind.clone();
    let value = blocking(move || assets::import_sound(&data, &k, &file)).await?;
    let st = state.inner().clone();
    st.update_settings(&app, |s| {
        if kind == "timer" {
            s.sounds.timer = value.clone();
        } else {
            s.sounds.notification = value.clone();
        }
        Ok(())
    })?;
    Ok(Some(value))
}

#[tauri::command]
pub async fn sound_data(state: State<'_, Shared>, file: String) -> AppResult<String> {
    let data = state.paths.data.clone();
    blocking(move || assets::sound_data(&data, &file)).await
}
