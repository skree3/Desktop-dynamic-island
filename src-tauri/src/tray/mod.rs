//! Системный трей и автозапуск.

use std::sync::atomic::Ordering;
use std::sync::OnceLock;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Wry};
use tauri_plugin_autostart::ManagerExt;

use crate::core::state::{shared, Shared};
use crate::core::window;
use crate::logging::{category, EventLog};

struct TrayItems {
    toggle: MenuItem<Wry>,
    game: CheckMenuItem<Wry>,
    dnd: CheckMenuItem<Wry>,
}

static ITEMS: OnceLock<TrayItems> = OnceLock::new();

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let toggle = MenuItem::with_id(app, "toggle", "Скрыть панель", true, None::<&str>)?;
    let hide15 = MenuItem::with_id(app, "hide15", "15 минут", true, None::<&str>)?;
    let hide60 = MenuItem::with_id(app, "hide60", "1 час", true, None::<&str>)?;
    let temp = Submenu::with_items(app, "Скрыть временно", true, &[&hide15, &hide60])?;
    let game = CheckMenuItem::with_id(app, "game", "Игровой режим", true, false, None::<&str>)?;
    let dnd = CheckMenuItem::with_id(app, "dnd", "Не беспокоить", true, false, None::<&str>)?;
    let reset = MenuItem::with_id(app, "reset_pos", "Поставить панель по центру", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Настройки…", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Выход", true, None::<&str>)?;
    let sep = || PredefinedMenuItem::separator(app);
    let menu = Menu::with_items(app, &[&toggle, &temp, &sep()?, &game, &dnd, &reset, &sep()?, &settings, &sep()?, &quit_item])?;

    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("Desktop Island")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let st = shared(app);
            match event.id().as_ref() {
                "toggle" => window::toggle_island(app, &st),
                "hide15" => window::hide_temporarily(app, &st, 15),
                "hide60" => window::hide_temporarily(app, &st, 60),
                "game" => {
                    let active = st.game_mode_active();
                    let _ = st.update_settings(app, |s| {
                        s.game_mode.manual = if active { Some(false) } else { Some(true) };
                        Ok(())
                    });
                    // Ручное выключение действует до следующего ручного переключения;
                    // через «Авто» в настройках возвращается автоматическое определение.
                }
                "dnd" => crate::core::hotkeys::run_action(app, &st, "toggle_dnd"),
                "reset_pos" => window::center_island(app, &st),
                "settings" => window::open_settings(app, None),
                "quit" => quit(app, &st),
                _ => {}
            }
            refresh(app);
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                let app = tray.app_handle();
                let st = shared(app);
                window::toggle_island(app, &st);
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    let _ = ITEMS.set(TrayItems { toggle, game, dnd });
    refresh(app);
    Ok(())
}

pub fn refresh(app: &AppHandle) {
    let Some(items) = ITEMS.get() else { return };
    let st = shared(app);
    let hidden = st.flags.user_hidden.load(Ordering::SeqCst) || st.flags.hidden_until.load(Ordering::SeqCst) > crate::database::now_ms();
    let _ = items.toggle.set_text(if hidden { "Показать панель" } else { "Скрыть панель" });
    let _ = items.game.set_checked(st.game_mode_active());
    let _ = items.dnd.set_checked(st.with_settings(|s| s.notifications.dnd));
}

pub fn quit(app: &AppHandle, st: &Shared) {
    st.flags.quitting.store(true, Ordering::SeqCst);
    st.log.info(category::APP, "app", "завершение работы");
    st.ai.close_all(app);
    app.exit(0);
}

pub fn set_autostart(app: &AppHandle, enabled: bool, log: &EventLog) {
    let al = app.autolaunch();
    let res = if enabled { al.enable() } else { al.disable() };
    match res {
        Ok(_) => log.info(category::SYSTEM_ACTION, "autostart", if enabled { "автозапуск включён" } else { "автозапуск выключен" }),
        Err(e) => log.error(category::PERMISSION, "autostart", format!("не удалось изменить автозапуск: {e}")),
    }
}

pub fn autostart_enabled(app: &AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}
