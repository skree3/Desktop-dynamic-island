//! Общие команды: начальная загрузка UI, окна, мониторы, окружение, память.

use serde::Serialize;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, State};

use crate::core::error::{AppError, AppResult};
use crate::core::settings::Settings;
use crate::core::state::Shared;
use crate::core::window::{self, MonitorInfo};
use crate::database::DbStatus;
use crate::events::{Activity, NotificationPayload};
use crate::modules::game_mode::GameModeState;
use crate::modules::CardInfo;
use crate::platform::types::EnvironmentInfo;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Bootstrap {
    pub version: String,
    pub os: String,
    pub settings: Settings,
    pub cards: Vec<CardInfo>,
    pub environment: EnvironmentInfo,
    pub db: DbStatus,
    pub activities: Vec<Activity>,
    pub notification: NotificationPayload,
    pub game_mode: GameModeState,
    pub themes: Vec<crate::database::repo::ThemeRow>,
}

#[tauri::command]
pub async fn app_bootstrap(state: State<'_, Shared>) -> AppResult<Bootstrap> {
    let st = state.inner().clone();
    Ok(Bootstrap {
        version: env!("CARGO_PKG_VERSION").into(),
        os: std::env::consts::OS.into(),
        settings: st.settings(),
        cards: crate::modules::list_cards(&st),
        environment: crate::platform::os::environment(),
        db: st.db.status.clone(),
        activities: st.events.activities(),
        notification: st.events.notification_payload(),
        game_mode: st.game_mode.state(),
        themes: st.db.list_themes()?,
    })
}

#[tauri::command]
pub async fn app_set_timezone(offset_minutes: i32) -> AppResult<()> {
    if !(-14 * 60..=14 * 60).contains(&offset_minutes) {
        return Err(AppError::invalid("смещение часового пояса"));
    }
    crate::core::profiles::TZ_OFFSET_MIN.store(offset_minutes, Ordering::Relaxed);
    Ok(())
}

/// Фронтенд сообщает, какие карточки сейчас на экране — по этому списку
/// запускаются и останавливаются опросы (ленивая работа модулей).
#[tauri::command]
pub async fn ui_set_active_cards(app: AppHandle, state: State<'_, Shared>, cards: Vec<String>, expanded: bool) -> AppResult<()> {
    let st = state.inner().clone();
    let cards: Vec<String> = cards.into_iter().take(16).map(|c| c.chars().take(80).collect()).collect();
    *st.flags.active_cards.lock().unwrap_or_else(|p| p.into_inner()) = cards;
    st.flags.island_expanded.store(expanded, Ordering::SeqCst);
    crate::modules::reconcile(&app, &st);
    Ok(())
}

#[tauri::command]
pub async fn island_resize(app: AppHandle, state: State<'_, Shared>, width: f64, height: f64) -> AppResult<()> {
    if !width.is_finite() || !height.is_finite() {
        return Err(AppError::invalid("размер"));
    }
    window::resize_island(&app, state.inner(), width, height);
    Ok(())
}

/// Размер самого острова внутри окна — область, которая принимает мышь.
#[tauri::command]
pub async fn island_hitbox(app: AppHandle, width: f64, height: f64) -> AppResult<()> {
    if !width.is_finite() || !height.is_finite() {
        return Err(AppError::invalid("размер"));
    }
    window::set_hitbox(&app, width, height);
    Ok(())
}

/// Фронтенд отрисовал первый кадр — показываем окно (если оно не должно быть скрыто).
#[tauri::command]
pub async fn island_ready(app: AppHandle, state: State<'_, Shared>) -> AppResult<()> {
    let st = state.inner();
    if window::ISLAND_READY.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let hidden = st.flags.user_hidden.load(Ordering::SeqCst) || st.game_mode_active();
    if hidden {
        window::hide_island(&app, st);
    } else {
        window::apply_position(&app, st);
        window::show_island(&app, st);
    }
    // Остров стал видимым — модули, зависящие от этого (баннеры «только в острове»), включаются.
    crate::modules::reconcile(&app, st);
    crate::tray::refresh(&app);
    Ok(())
}

#[tauri::command]
pub async fn island_toggle(app: AppHandle, state: State<'_, Shared>) -> AppResult<()> {
    window::toggle_island(&app, state.inner());
    Ok(())
}

#[tauri::command]
pub async fn island_hide_temporarily(app: AppHandle, state: State<'_, Shared>, minutes: u64) -> AppResult<()> {
    window::hide_temporarily(&app, state.inner(), minutes.clamp(1, 24 * 60));
    Ok(())
}

#[tauri::command]
pub async fn island_reset_position(app: AppHandle, state: State<'_, Shared>) -> AppResult<()> {
    window::reset_position(&app, state.inner());
    Ok(())
}

#[tauri::command]
pub async fn island_center(app: AppHandle, state: State<'_, Shared>) -> AppResult<()> {
    window::center_island(&app, state.inner());
    Ok(())
}

#[tauri::command]
pub async fn island_attached() -> AppResult<bool> {
    Ok(window::is_attached())
}

#[tauri::command]
pub async fn monitors_list(app: AppHandle) -> AppResult<Vec<MonitorInfo>> {
    Ok(window::list_monitors(&app))
}

#[tauri::command]
pub async fn island_move_to_monitor(app: AppHandle, state: State<'_, Shared>, key: Option<String>) -> AppResult<()> {
    if let Some(k) = &key {
        if !window::list_monitors(&app).iter().any(|m| &m.key == k) {
            return Err(AppError::NotFound("монитор не подключён".into()));
        }
    }
    window::move_to_monitor(&app, state.inner(), key);
    Ok(())
}

#[tauri::command]
pub async fn open_settings_window(app: AppHandle, section: Option<String>) -> AppResult<()> {
    window::open_settings(&app, section.as_deref());
    Ok(())
}

#[tauri::command]
pub async fn app_quit(app: AppHandle, state: State<'_, Shared>) -> AppResult<()> {
    crate::tray::quit(&app, state.inner());
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessMem {
    pub pid: u32,
    pub name: String,
    pub memory: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryReport {
    pub total: u64,
    pub processes: Vec<ProcessMem>,
    pub note: String,
}

/// Суммарная память всех процессов приложения: основной процесс Rust +
/// дочерние процессы WebView (msedgewebview2.exe / WebKitWebProcess).
#[tauri::command]
pub async fn perf_memory() -> AppResult<MemoryReport> {
    tauri::async_runtime::spawn_blocking(|| {
        use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate};
        let mut sys = sysinfo::System::new();
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_memory());
        let root = Pid::from_u32(std::process::id());
        let mut ours = vec![root];
        // Обходим дерево потомков (WebView2 создаёт несколько уровней процессов).
        let mut changed = true;
        while changed {
            changed = false;
            for (pid, p) in sys.processes() {
                if !ours.contains(pid) {
                    if let Some(parent) = p.parent() {
                        if ours.contains(&parent) {
                            ours.push(*pid);
                            changed = true;
                        }
                    }
                }
            }
        }
        let mut processes: Vec<ProcessMem> = ours
            .iter()
            .filter_map(|pid| sys.process(*pid))
            .map(|p| ProcessMem { pid: p.pid().as_u32(), name: p.name().to_string_lossy().to_string(), memory: p.memory() })
            .collect();
        processes.sort_by(|a, b| b.memory.cmp(&a.memory));
        let total = processes.iter().map(|p| p.memory).sum();
        MemoryReport {
            total,
            processes,
            note: "Рабочий набор (RSS) основного процесса и всех дочерних процессов WebView. Разделяемая память может учитываться в нескольких процессах.".into(),
        }
    })
    .await
    .map_err(AppError::from)
}

#[tauri::command]
pub async fn environment_info() -> AppResult<EnvironmentInfo> {
    Ok(crate::platform::os::environment())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AboutInfo {
    pub version: String,
    pub os: String,
    pub arch: String,
    pub data_dir: String,
    pub logs_dir: String,
    pub db: DbStatus,
    pub autostart: bool,
    pub plugin_api: u32,
}

#[tauri::command]
pub async fn about_info(app: AppHandle, state: State<'_, Shared>) -> AppResult<AboutInfo> {
    let st = state.inner();
    Ok(AboutInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        data_dir: st.paths.data.display().to_string(),
        logs_dir: st.paths.logs.display().to_string(),
        db: st.db.status.clone(),
        autostart: crate::tray::autostart_enabled(&app),
        plugin_api: crate::plugins::PLUGIN_API_VERSION,
    })
}

/// Открывает https-ссылку в системном браузере.
#[tauri::command]
pub async fn open_url(url: String) -> AppResult<()> {
    let url = crate::util::validate_web_url(&url)?;
    tauri::async_runtime::spawn_blocking(move || crate::platform::os::shell::open_uri(&url)).await?
}

#[tauri::command]
pub async fn open_path(kind: String, state: State<'_, Shared>) -> AppResult<()> {
    let st = state.inner();
    let path = match kind.as_str() {
        "data" => st.paths.data.clone(),
        "logs" => st.paths.logs.clone(),
        "plugins" => st.paths.plugins.clone(),
        _ => return Err(AppError::invalid("неизвестный каталог")),
    };
    std::fs::create_dir_all(&path)?;
    let p = path.display().to_string();
    tauri::async_runtime::spawn_blocking(move || crate::platform::os::shell::open_uri(&p)).await?
}

/// Выбор файла/папки системным диалогом. Типы фиксированы — фронтенд не может
/// передать произвольный фильтр.
#[tauri::command]
pub async fn pick_path(app: AppHandle, kind: String) -> AppResult<Option<String>> {
    use tauri_plugin_dialog::DialogExt;
    tauri::async_runtime::spawn_blocking(move || {
        let d = app.dialog().file();
        let res = match kind.as_str() {
            "exe" => {
                let d = if cfg!(windows) { d.add_filter("Программы", &["exe", "lnk", "bat", "cmd"]) } else { d };
                d.set_title("Выберите программу").blocking_pick_file()
            }
            "icon" => d.add_filter("Изображения", &["png", "jpg", "jpeg", "svg", "ico", "webp"]).set_title("Выберите иконку").blocking_pick_file(),

            "plugin" => d.set_title("Выберите папку плагина (с manifest.json)").blocking_pick_folder(),
            _ => return Err(AppError::invalid("неизвестный тип выбора")),
        };
        Ok(res.and_then(|p| p.into_path().ok()).map(|p| p.display().to_string()))
    })
    .await?
}

