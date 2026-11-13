//! Команды функциональных модулей: уведомления, музыка, системный монитор,
//! Bluetooth, быстрые настройки, громкость, яркость, питание, таймеры,
//! буфер обмена, игровой режим.

use serde::Serialize;
use tauri::{AppHandle, State};

use crate::core::error::{AppError, AppResult};
use crate::core::state::Shared;
use crate::events::{internal_notification, IslandNotification, NotificationPayload, Priority};
use crate::logging::category;
use crate::modules::clipboard::ClipItem;
use crate::modules::game_mode::GameModeState;
use crate::modules::system_monitor::SysSnapshot;
use crate::modules::timers::Timer;
use crate::platform::os;
use crate::platform::types::*;

fn require(st: &Shared, module: &str) -> AppResult<()> {
    if st.with_settings(|s| s.modules.is_enabled(module)) {
        Ok(())
    } else {
        Err(AppError::unsupported("модуль отключён в настройках"))
    }
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> AppResult<T> + Send + 'static) -> AppResult<T> {
    tauri::async_runtime::spawn_blocking(f).await?
}

// ---- уведомления ----

#[tauri::command]
pub async fn notifications_state(state: State<'_, Shared>) -> AppResult<NotificationPayload> {
    Ok(state.events.notification_payload())
}

#[tauri::command]
pub async fn notifications_history(state: State<'_, Shared>) -> AppResult<Vec<IslandNotification>> {
    Ok(state.events.history())
}

#[tauri::command]
pub async fn notifications_clear_history(state: State<'_, Shared>) -> AppResult<()> {
    state.events.clear_history();
    Ok(())
}

#[tauri::command]
pub async fn notification_remove(state: State<'_, Shared>, id: String) -> AppResult<()> {
    state.events.remove_from_history(&id);
    Ok(())
}

#[tauri::command]
pub async fn notification_dismiss(state: State<'_, Shared>, id: String) -> AppResult<()> {
    state.events.dismiss(&id);
    Ok(())
}

#[tauri::command]
pub async fn notification_hold(state: State<'_, Shared>, id: String, hold: bool) -> AppResult<()> {
    if hold {
        state.events.hold(&id)
    } else {
        state.events.release(&id)
    }
    Ok(())
}

#[tauri::command]
pub async fn notification_action(app: AppHandle, state: State<'_, Shared>, id: String, action: String) -> AppResult<()> {
    let st = state.inner().clone();
    st.notifications.action(&app, &st, &id, &action)
}

#[tauri::command]
pub async fn notifications_listener_status(state: State<'_, Shared>) -> AppResult<ListenerStatus> {
    Ok(state.notifications.status())
}

#[tauri::command]
pub async fn notifications_retry(app: AppHandle, state: State<'_, Shared>) -> AppResult<()> {
    let st = state.inner().clone();
    st.notifications.retry(&app, &st);
    Ok(())
}

#[tauri::command]
pub async fn notifications_test(state: State<'_, Shared>, priority: Option<String>) -> AppResult<()> {
    let p = priority.as_deref().and_then(Priority::parse).unwrap_or(Priority::Normal);
    let mut n = internal_notification("test", "Desktop Island", "Тестовое уведомление", "Так будут выглядеть уведомления в панели.", p);
    n.actions = vec![crate::events::NotificationAction { id: "dismiss".into(), label: "Понятно".into() }];
    state.events.push(n);
    Ok(())
}

// ---- музыка ----

#[tauri::command]
pub async fn media_sessions(state: State<'_, Shared>) -> AppResult<Vec<MediaSessionInfo>> {
    require(&state, "music")?;
    state.music.sessions().await
}

#[tauri::command]
pub async fn media_control(state: State<'_, Shared>, id: String, action: MediaAction) -> AppResult<()> {
    require(&state, "music")?;
    if id.len() > 512 {
        return Err(AppError::invalid("id"));
    }
    state.music.control(id, action).await
}

#[tauri::command]
pub async fn media_artwork(state: State<'_, Shared>, id: String) -> AppResult<Option<String>> {
    require(&state, "music")?;
    state.music.artwork(id).await
}

#[tauri::command]
pub async fn media_select(state: State<'_, Shared>, id: Option<String>) -> AppResult<()> {
    state.music.select(id);
    Ok(())
}

// ---- системный монитор ----

#[tauri::command]
pub async fn sysmon_snapshot(state: State<'_, Shared>) -> AppResult<Option<SysSnapshot>> {
    require(&state, "system_monitor")?;
    Ok(state.sysmon.snapshot_now().await)
}

// ---- Bluetooth ----

#[tauri::command]
pub async fn bluetooth_state(state: State<'_, Shared>) -> AppResult<BluetoothState> {
    require(&state, "bluetooth")?;
    blocking(os::bluetooth::state).await
}

#[tauri::command]
pub async fn bluetooth_set_powered(state: State<'_, Shared>, on: bool) -> AppResult<()> {
    require(&state, "bluetooth")?;
    let st = state.inner().clone();
    let res = blocking(move || os::bluetooth::set_powered(on)).await;
    log_system_action(&st, "bluetooth", &format!("Bluetooth {}", if on { "включён" } else { "выключен" }), &res);
    res
}

fn log_system_action<T>(st: &Shared, source: &str, what: &str, res: &AppResult<T>) {
    if !st.with_settings(|s| s.security.log_system_actions) {
        return;
    }
    match res {
        Ok(_) => st.log.info(category::SYSTEM_ACTION, source, what),
        Err(AppError::PermissionDenied(m)) => st.log.warn(category::PERMISSION, source, format!("{what}: отказано — {m}")),
        Err(e) => st.log.error(category::SYSTEM_ACTION, source, format!("{what}: ошибка — {e}")),
    }
}

// ---- быстрые настройки ----

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickState {
    pub wifi: WifiState,
    pub bluetooth: BluetoothState,
    pub airplane: AirplaneState,
    pub power: Option<PowerState>,
    pub volume: Option<VolumeState>,
    pub volume_error: Option<String>,
}

#[tauri::command]
pub async fn quick_state(state: State<'_, Shared>) -> AppResult<QuickState> {
    require(&state, "quick_settings")?;
    let (bt_on, vol_on) = state.with_settings(|s| (s.modules.bluetooth, s.modules.volume));
    blocking(move || {
        let wifi = os::network::wifi_state().unwrap_or_else(|e| WifiState { error: Some(e.to_string()), ..Default::default() });
        let mut bluetooth = if bt_on { os::bluetooth::state().unwrap_or_default() } else { BluetoothState::default() };
        bluetooth.devices.retain(|d| d.connected); // в плитке — только подключённые
        let (volume, volume_error) = if vol_on {
            match os::audio::state() {
                Ok(v) => (Some(v), None),
                Err(e) => (None, Some(e.to_string())),
            }
        } else {
            (None, None)
        };
        Ok(QuickState { wifi, bluetooth, airplane: os::network::airplane_state(), power: os::power::state().ok(), volume, volume_error })
    })
    .await
}

#[tauri::command]
pub async fn wifi_set(state: State<'_, Shared>, on: bool) -> AppResult<()> {
    require(&state, "quick_settings")?;
    let st = state.inner().clone();
    let res = blocking(move || os::network::set_wifi(on)).await;
    log_system_action(&st, "wifi", &format!("Wi-Fi {}", if on { "включён" } else { "выключен" }), &res);
    res
}

#[tauri::command]
pub async fn energy_saver_set(state: State<'_, Shared>, on: bool) -> AppResult<()> {
    require(&state, "quick_settings")?;
    let st = state.inner().clone();
    let res = blocking(move || os::power::set_saver(on)).await;
    log_system_action(&st, "power", &format!("Энергосбережение {}", if on { "включено" } else { "выключено" }), &res);
    res
}

#[tauri::command]
pub async fn open_system_settings(page: SettingsPage) -> AppResult<()> {
    blocking(move || os::shell::open_settings(page)).await
}

// ---- громкость ----

#[tauri::command]
pub async fn volume_state(state: State<'_, Shared>) -> AppResult<VolumeState> {
    require(&state, "volume")?;
    blocking(os::audio::state).await
}

#[tauri::command]
pub async fn volume_set(state: State<'_, Shared>, level: f32) -> AppResult<()> {
    require(&state, "volume")?;
    if !level.is_finite() {
        return Err(AppError::invalid("уровень громкости"));
    }
    blocking(move || os::audio::set_volume(level)).await
}

#[tauri::command]
pub async fn volume_mute(state: State<'_, Shared>, muted: bool) -> AppResult<()> {
    require(&state, "volume")?;
    blocking(move || os::audio::set_mute(muted)).await
}

#[tauri::command]
pub async fn volume_set_device(state: State<'_, Shared>, id: String) -> AppResult<()> {
    require(&state, "volume")?;
    let st = state.inner().clone();
    let res = blocking(move || os::audio::set_default_device(&id)).await;
    log_system_action(&st, "audio", "смена устройства вывода", &res);
    res
}

// ---- яркость ----

#[tauri::command]
pub async fn brightness_state(state: State<'_, Shared>) -> AppResult<BrightnessState> {
    require(&state, "brightness")?;
    blocking(os::brightness::state).await
}

#[tauri::command]
pub async fn brightness_set(state: State<'_, Shared>, id: String, value: u32) -> AppResult<()> {
    require(&state, "brightness")?;
    if id.len() > 64 {
        return Err(AppError::invalid("id"));
    }
    blocking(move || os::brightness::set(&id, value.min(100))).await
}

// ---- питание ----

/// Шаг 1: UI показывает диалог подтверждения и получает одноразовый токен.
#[tauri::command]
pub async fn power_request(state: State<'_, Shared>, action: String) -> AppResult<String> {
    require(&state, "power")?;
    if action != "shutdown" && action != "reboot" {
        return Err(AppError::invalid("действие должно быть shutdown или reboot"));
    }
    Ok(state.confirm.issue(&action))
}

/// Шаг 2: выполнение после явного подтверждения пользователем.
#[tauri::command]
pub async fn power_execute(state: State<'_, Shared>, action: String, token: String) -> AppResult<()> {
    require(&state, "power")?;
    let st = state.inner().clone();
    st.confirm.consume(&token, &action)?;
    let label = if action == "shutdown" { "выключение компьютера" } else { "перезагрузка компьютера" };
    st.log.info(category::SYSTEM_ACTION, "power", format!("запрошено: {label}"));
    let res = blocking(move || if action == "shutdown" { os::power::shutdown() } else { os::power::reboot() }).await;
    log_system_action(&st, "power", label, &res);
    res
}

// ---- таймеры ----

#[tauri::command]
pub async fn timers_list(state: State<'_, Shared>) -> AppResult<Vec<Timer>> {
    state.timers.list(&state.db)
}

#[tauri::command]
pub async fn timer_create(state: State<'_, Shared>, label: String, kind: String, duration_ms: i64, start: bool) -> AppResult<Timer> {
    require(&state, "timers")?;
    let st = state.inner().clone();
    st.timers.create(&st, label, kind, duration_ms, start)
}

#[tauri::command]
pub async fn timer_command(state: State<'_, Shared>, id: String, command: String, label: Option<String>) -> AppResult<()> {
    let st = state.inner().clone();
    match command.as_str() {
        "start" => st.timers.start(&st, &id),
        "pause" => st.timers.pause(&st, &id),
        "reset" => st.timers.reset(&st, &id),
        "delete" => st.timers.delete(&st, &id),
        "rename" => st.timers.rename(&st, &id, label.as_deref().unwrap_or("")),
        _ => Err(AppError::invalid("неизвестная команда таймера")),
    }
}

// ---- буфер обмена ----

#[tauri::command]
pub async fn clipboard_list(state: State<'_, Shared>, query: Option<String>, limit: Option<u32>) -> AppResult<Vec<ClipItem>> {
    state.clipboard.list(&state.db, query.as_deref(), limit.unwrap_or(100))
}

#[tauri::command]
pub async fn clipboard_image(state: State<'_, Shared>, id: i64) -> AppResult<Option<String>> {
    state.clipboard.image(&state.db, id)
}

#[tauri::command]
pub async fn clipboard_text(state: State<'_, Shared>, id: i64) -> AppResult<String> {
    state.clipboard.text(&state.db, id)
}

#[tauri::command]
pub async fn clipboard_copy(state: State<'_, Shared>, id: i64) -> AppResult<()> {
    let st = state.inner().clone();
    blocking(move || st.clipboard.copy(&st.db, id)).await
}

#[tauri::command]
pub async fn clipboard_pin(state: State<'_, Shared>, id: i64, pinned: bool) -> AppResult<()> {
    state.clipboard.pin(&state.db, id, pinned)?;
    state.events.emit(crate::events::names::CLIPBOARD_CHANGED, ());
    Ok(())
}

#[tauri::command]
pub async fn clipboard_delete(state: State<'_, Shared>, id: i64) -> AppResult<()> {
    state.clipboard.delete(&state.db, id)?;
    state.events.emit(crate::events::names::CLIPBOARD_CHANGED, ());
    Ok(())
}

#[tauri::command]
pub async fn clipboard_clear(state: State<'_, Shared>, keep_pinned: bool) -> AppResult<usize> {
    let n = state.clipboard.clear(&state.db, keep_pinned)?;
    state.events.emit(crate::events::names::CLIPBOARD_CHANGED, ());
    Ok(n)
}

// ---- журнал буфера обмена ОС (Windows Win+V) ----

#[tauri::command]
pub async fn clipboard_system_history() -> AppResult<serde_json::Value> {
    #[cfg(windows)]
    {
        let list = blocking(|| os::clip_history::list(true)).await?;
        Ok(serde_json::to_value(list)?)
    }
    #[cfg(not(windows))]
    Err(AppError::unsupported("журнал буфера обмена ОС есть только в Windows"))
}

#[tauri::command]
pub async fn clipboard_system_item(id: String, command: String) -> AppResult<()> {
    #[cfg(windows)]
    {
        blocking(move || match command.as_str() {
            "copy" => os::clip_history::copy(&id),
            "delete" => os::clip_history::delete(&id),
            _ => Err(AppError::invalid("команда")),
        })
        .await
    }
    #[cfg(not(windows))]
    {
        let _ = (id, command);
        Err(AppError::unsupported("журнал буфера обмена ОС есть только в Windows"))
    }
}

/// Очищает журнал Win+V и собственную историю Desktop Island.
#[tauri::command]
pub async fn clipboard_system_clear(state: State<'_, Shared>) -> AppResult<()> {
    state.clipboard.clear(&state.db, true)?;
    state.events.emit(crate::events::names::CLIPBOARD_CHANGED, ());
    #[cfg(windows)]
    blocking(os::clip_history::clear).await?;
    Ok(())
}

// ---- заметки ----

#[tauri::command]
pub async fn notes_list(state: State<'_, Shared>) -> AppResult<Vec<crate::modules::notes::Note>> {
    crate::modules::notes::list(state.inner())
}

#[tauri::command]
pub async fn note_create(
    state: State<'_, Shared>,
    text: String,
    color: Option<String>,
    remind_at: Option<i64>,
    audio: Option<String>,
    audio_ms: Option<i64>,
) -> AppResult<crate::modules::notes::Note> {
    require(&state, "notes")?;
    let audio = audio.map(|a| (a, audio_ms.unwrap_or(0)));
    crate::modules::notes::create(state.inner(), &text, color.as_deref().unwrap_or("default"), remind_at, audio)
}

#[tauri::command]
pub async fn note_remind(state: State<'_, Shared>, id: String, remind_at: Option<i64>) -> AppResult<()> {
    crate::modules::notes::set_reminder(state.inner(), &id, remind_at)
}

// ---- голосовые заметки ----

#[tauri::command]
pub async fn voice_start(state: State<'_, Shared>) -> AppResult<()> {
    let st = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || st.voice.start(&st)).await?
}

#[tauri::command]
pub async fn voice_stop(state: State<'_, Shared>) -> AppResult<crate::modules::voice::Recorded> {
    let st = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || st.voice.stop()).await?
}

#[tauri::command]
pub async fn voice_cancel(state: State<'_, Shared>) -> AppResult<()> {
    let st = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || st.voice.cancel(&st)).await?;
    Ok(())
}

#[tauri::command]
pub async fn voice_data(state: State<'_, Shared>, name: String) -> AppResult<String> {
    crate::modules::voice::data_url(state.inner(), &name)
}

// ---- телефон ----

#[tauri::command]
pub async fn phone_status(state: State<'_, Shared>) -> AppResult<crate::modules::phone::PhoneStatus> {
    Ok(state.phone.status(state.inner()))
}

#[tauri::command]
pub async fn phone_pair(state: State<'_, Shared>, id: String) -> AppResult<()> {
    state.phone.pair(state.inner(), &id)
}

#[tauri::command]
pub async fn phone_unpair(state: State<'_, Shared>, id: String) -> AppResult<()> {
    state.phone.unpair(state.inner(), &id)
}

#[tauri::command]
pub async fn phone_accept(state: State<'_, Shared>, id: String, accept: bool) -> AppResult<()> {
    state.phone.accept(state.inner(), &id, accept)
}

#[tauri::command]
pub async fn phone_ring(state: State<'_, Shared>, id: String) -> AppResult<()> {
    state.phone.ring(&id)
}

#[tauri::command]
pub async fn note_update(state: State<'_, Shared>, id: String, text: Option<String>, color: Option<String>, pinned: Option<bool>) -> AppResult<()> {
    crate::modules::notes::update(state.inner(), &id, text.as_deref(), color.as_deref(), pinned)
}

#[tauri::command]
pub async fn note_delete(state: State<'_, Shared>, id: String) -> AppResult<()> {
    crate::modules::notes::delete(state.inner(), &id)
}

// ---- полка ----

#[tauri::command]
pub async fn shelf_list(state: State<'_, Shared>) -> AppResult<Vec<crate::modules::shelf::ShelfItem>> {
    crate::modules::shelf::list(state.inner())
}

#[tauri::command]
pub async fn shelf_add(state: State<'_, Shared>, paths: Vec<String>) -> AppResult<usize> {
    require(&state, "shelf")?;
    crate::modules::shelf::add(state.inner(), &paths)
}

#[tauri::command]
pub async fn shelf_command(state: State<'_, Shared>, id: Option<i64>, ids: Option<Vec<i64>>, command: String) -> AppResult<()> {
    let st = state.inner().clone();
    blocking(move || {
        let one = || id.ok_or_else(|| AppError::invalid("id"));
        match command.as_str() {
            "open" => crate::modules::shelf::open(&st, one()?),
            "reveal" => crate::modules::shelf::reveal(&st, one()?),
            "remove" => crate::modules::shelf::remove(&st, one()?),
            "copy" => crate::modules::shelf::copy(&st, &ids.unwrap_or_else(|| id.into_iter().collect())),
            "clear" => crate::modules::shelf::clear(&st),
            _ => Err(AppError::invalid("неизвестная команда полки")),
        }
    })
    .await
}

#[tauri::command]
pub async fn shelf_drag_out(app: AppHandle, state: State<'_, Shared>, ids: Vec<i64>) -> AppResult<()> {
    require(&state, "shelf")?;
    let st = state.inner().clone();
    blocking(move || crate::modules::shelf::drag_out(&app, &st, &ids)).await
}

#[tauri::command]
pub async fn screenshot_region(state: State<'_, Shared>) -> AppResult<bool> {
    require(&state, "shelf")?;
    crate::modules::shelf::screenshot::capture(state.inner().clone()).await
}

// ---- микрофон, камера, звонки ----

#[tauri::command]
pub async fn privacy_state(state: State<'_, Shared>) -> AppResult<crate::modules::privacy::PrivacyState> {
    Ok(state.privacy.state())
}

#[tauri::command]
pub async fn mic_toggle(state: State<'_, Shared>) -> AppResult<bool> {
    let st = state.inner().clone();
    blocking(move || st.privacy.toggle_mic(&st)).await
}

#[tauri::command]
pub async fn call_open(state: State<'_, Shared>) -> AppResult<()> {
    state.privacy.open_call(state.inner())
}

// ---- тексты песен ----

#[tauri::command]
pub async fn lyrics_get(
    state: State<'_, Shared>,
    artist: String,
    title: String,
    album: Option<String>,
    duration_ms: Option<u64>,
) -> AppResult<Option<crate::modules::music::lyrics::Lyrics>> {
    if !state.with_settings(|s| s.modules.music && s.music.lyrics) {
        return Ok(None);
    }
    blocking(move || crate::modules::music::lyrics::fetch(&artist, &title, album.as_deref().unwrap_or(""), duration_ms)).await
}

// ---- обновления ----

#[tauri::command]
pub async fn update_check(state: State<'_, Shared>) -> AppResult<crate::modules::updater::UpdateInfo> {
    let st = state.inner().clone();
    blocking(move || st.updater.check_blocking()).await
}

#[tauri::command]
pub async fn update_install(app: AppHandle, state: State<'_, Shared>) -> AppResult<()> {
    let st = state.inner().clone();
    blocking(move || crate::modules::updater::install_and_quit(&app, &st)).await
}

// ---- игровой режим ----

#[tauri::command]
pub async fn game_mode_state(state: State<'_, Shared>) -> AppResult<GameModeState> {
    Ok(state.game_mode.state())
}

#[tauri::command]
pub async fn game_mode_set_manual(app: AppHandle, state: State<'_, Shared>, manual: Option<bool>) -> AppResult<()> {
    let st = state.inner().clone();
    st.update_settings(&app, |s| {
        s.game_mode.manual = manual;
        Ok(())
    })?;
    st.game_mode.force_check();
    Ok(())
}

/// Список недавно активных приложений — для удобного добавления в исключения.
#[tauri::command]
pub async fn game_mode_foreground() -> AppResult<Option<ForegroundInfo>> {
    blocking(|| Ok(os::foreground::current())).await
}
