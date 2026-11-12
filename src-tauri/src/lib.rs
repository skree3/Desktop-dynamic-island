//! Desktop Island — точка входа: инициализация хранилища, логирования,
//! модулей, окон, трея, горячих клавиш и протокола плагинов.

pub mod commands;
pub mod core;
pub mod database;
pub mod events;
pub mod integrations;
pub mod logging;
pub mod modules;
pub mod platform;
pub mod plugins;
pub mod selftest;
pub mod tray;
pub mod util;

use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use tauri::{Manager, RunEvent, WindowEvent};
use tauri_plugin_global_shortcut::ShortcutState;

use crate::core::state::{AppState, Paths, Shared};
use crate::core::window;
use crate::database::Database;
use crate::logging::{category, EventLog};

static LOG_GUARD: Mutex<Option<tracing_appender::non_blocking::WorkerGuard>> = Mutex::new(None);

fn handle_cli(app: &tauri::AppHandle, args: &[String]) {
    let Some(st) = app.try_state::<Shared>().map(|s| s.inner().clone()) else { return };
    if args.iter().any(|a| a == "--quit") {
        tray::quit(app, &st);
    } else if args.iter().any(|a| a == "--toggle") {
        window::toggle_island(app, &st);
    } else if args.iter().any(|a| a == "--search") {
        crate::core::hotkeys::run_action(app, &st, "app_search");
    } else if args.iter().any(|a| a == "--quick") {
        crate::core::hotkeys::run_action(app, &st, "quick_settings");
    } else if args.iter().any(|a| a == "--dnd") {
        crate::core::hotkeys::run_action(app, &st, "toggle_dnd");
    } else if args.iter().any(|a| a == "--play-pause") {
        crate::core::hotkeys::run_action(app, &st, "media_play_pause");
    } else if args.iter().any(|a| a == "--screenshot") {
        let st2 = st.clone();
        tauri::async_runtime::spawn(async move {
            let _ = crate::modules::shelf::screenshot::capture(st2).await;
        });
    } else {
        window::open_settings(app, None);
    }
}

fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let handle = app.handle().clone();
    let data = app.path().app_data_dir()?;
    let logs = app.path().app_log_dir().unwrap_or_else(|_| data.join("logs"));
    std::fs::create_dir_all(&data)?;
    *LOG_GUARD.lock().unwrap_or_else(|p| p.into_inner()) = logging::init_tracing(&logs);

    let db = Arc::new(Database::open(&data.join("island.db")));
    let mut settings = db.load_settings().unwrap_or_default();
    settings.sanitize();
    if !db.has_settings() {
        let _ = db.save_all_settings(&settings);
    }
    let log = Arc::new(EventLog::new(db.clone(), settings.logging.max_entries, &settings.logging.level));
    let paths = Paths { plugins: data.join("plugins"), webdata: data.join("webdata"), logs, data };
    let st: Shared = Arc::new(AppState::new(db.clone(), log.clone(), settings.clone(), paths));
    app.manage(st.clone());
    st.events.attach(handle.clone());

    log.info(category::APP, "app", format!("запуск Desktop Island {} ({})", env!("CARGO_PKG_VERSION"), std::env::consts::OS));
    if let Some(w) = &db.status.warning {
        log.error(category::DATABASE, "database", w);
    }

    core::themes::ensure_builtin(&st);
    modules::ensure_widgets(&st);
    core::hotkeys::ensure_defaults(&st);
    core::profiles::ensure_builtin(&st);
    st.plugins.load(&st.db);
    plugins::builtin::ensure(&st);
    integrations::refresh_muted(&st);

    if let Err(e) = tray::create(&handle) {
        log.error(category::APP, "tray", format!("не удалось создать значок в трее: {e}"));
    }
    st.hotkeys.register_all(&handle, &st);

    if settings.general.launch_at_startup != tray::autostart_enabled(&handle) {
        tray::set_autostart(&handle, settings.general.launch_at_startup, &log);
    }

    if let Err(e) = window::create_island(&handle, settings.performance.software_rendering) {
        log.error(category::APP, "window", format!("не удалось создать окно панели: {e}"));
    }
    if let Some(win) = window::island(&handle) {
        let _ = win.set_always_on_top(true);
        let _ = win.set_skip_taskbar(true);
        window::apply_position(&handle, &st);
        if settings.general.start_hidden {
            st.flags.user_hidden.store(true, Ordering::SeqCst);
            st.flags.island_visible.store(false, Ordering::SeqCst);
        }
        // Окно показывается фронтендом после первой отрисовки (без белой вспышки).
        window::show_if_frontend_silent(&handle, &st);
    }

    window::start_monitor_watch(&handle, &st);
    #[cfg(windows)]
    modules::shelf::start_drop_guard(&handle);
    modules::reconcile(&handle, &st);

    if let Some(w) = db.status.warning.clone() {
        let st2 = st.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            st2.events.push(events::internal_notification("database", "Desktop Island", "Проблема с базой данных", w, events::Priority::High));
        });
    }
    Ok(())
}

pub fn run() {
    if std::env::args().any(|a| a == "--selftest") {
        selftest::run();
        return;
    }
    #[cfg(target_os = "linux")]
    platform::linux::prepare_environment();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| handle_cli(app, &argv)))
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        core::hotkeys::on_shortcut(app, shortcut);
                    }
                })
                .build(),
        )
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, Some(vec!["--autostart"])))
        .plugin(tauri_plugin_dialog::init())
        .register_asynchronous_uri_scheme_protocol("plugin", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            let path = request.uri().path().to_string();
            std::thread::spawn(move || {
                let st = app.state::<Shared>();
                let resp = match st.plugins.serve(&path) {
                    Some((body, mime)) => tauri::http::Response::builder()
                        .status(200)
                        .header("Content-Type", mime)
                        .header("Content-Security-Policy", plugins::PLUGIN_CSP)
                        .header("X-Content-Type-Options", "nosniff")
                        .header("Cache-Control", "no-store")
                        .body(body),
                    None => tauri::http::Response::builder().status(404).header("Content-Type", "text/plain").body(b"not found".to_vec()),
                };
                match resp {
                    Ok(r) => responder.respond(r),
                    Err(_) => responder.respond(tauri::http::Response::builder().status(500).body(Vec::new()).expect("response")),
                }
            });
        })
        .setup(setup)
        .on_window_event(|window, event| {
            if window.label() != window::ISLAND {
                return;
            }
            let app = window.app_handle();
            match event {
                WindowEvent::Moved(_) => window::on_island_moved(app),
                WindowEvent::DragDrop(ev) => modules::shelf::on_drag_drop(app, ev),
                // «Показать рабочий стол» (Win+D) сворачивает и остров — сразу возвращаем его.
                WindowEvent::Resized(_) if window.is_minimized().unwrap_or(false) => {
                    let st = core::state::shared(app);
                    if st.island_visible() {
                        let _ = window.unminimize();
                        let _ = window.set_always_on_top(true);
                    }
                }
                WindowEvent::ScaleFactorChanged { .. } => {
                    let st = core::state::shared(app);
                    window::apply_position(app, &st);
                }
                WindowEvent::CloseRequested { api, .. } => {
                    // Alt+F4 по панели не должен завершать приложение в фоновом режиме.
                    let st = core::state::shared(app);
                    if st.with_settings(|s| s.general.background_mode) && !st.flags.quitting.load(Ordering::SeqCst) {
                        api.prevent_close();
                        st.flags.user_hidden.store(true, Ordering::SeqCst);
                        window::refresh_visibility(app, &st);
                    } else {
                        tray::quit(app, &st);
                    }
                }
                _ => {}
            }
        })
        .invoke_handler(all_commands!())
        .build(tauri::generate_context!())
        .expect("не удалось инициализировать Desktop Island");

    app.run(|app, event| {
        if let RunEvent::ExitRequested { api, code, .. } = &event {
            let quitting = app.try_state::<Shared>().map(|s| s.flags.quitting.load(Ordering::SeqCst)).unwrap_or(true);
            if code.is_none() && !quitting {
                api.prevent_exit();
            }
        }
        if let RunEvent::Exit = event {
            if let Some(st) = app.try_state::<Shared>() {
                st.log.info(category::APP, "app", "приложение завершено");
            }
        }
    });
}
