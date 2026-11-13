//! Глобальное состояние приложения и применение изменений настроек.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use tauri::{AppHandle, Manager};

use crate::core::confirm::ConfirmStore;
use crate::core::error::AppResult;
use crate::core::hotkeys::HotkeyManager;
use crate::core::settings::Settings;
use crate::database::Database;
use crate::events::{names, EventManager};
use crate::integrations::ai::AiWindows;
use crate::logging::{category, EventLog};
use crate::modules;
use crate::plugins::PluginManager;

pub type Shared = Arc<AppState>;

pub struct Paths {
    pub data: PathBuf,
    pub plugins: PathBuf,
    pub webdata: PathBuf,
    pub logs: PathBuf,
}

#[derive(Default)]
pub struct RuntimeFlags {
    pub game_mode: AtomicBool,
    /// Панель видима (не скрыта пользователем и не игровым режимом).
    pub island_visible: AtomicBool,
    /// Пользователь скрыл панель вручную (трей / горячая клавиша).
    pub user_hidden: AtomicBool,
    /// Unix ms, до которого панель временно скрыта (0 — не скрыта).
    pub hidden_until: AtomicI64,
    pub quitting: AtomicBool,
    pub island_expanded: AtomicBool,
    /// Карточки, которые сейчас отображаются во фронтенде.
    pub active_cards: Mutex<Vec<String>>,
}

impl RuntimeFlags {
    pub fn card_active(&self, id: &str) -> bool {
        self.active_cards.lock().map(|g| g.iter().any(|c| c == id)).unwrap_or(false)
    }
}

pub struct AppState {
    pub db: Arc<Database>,
    pub log: Arc<EventLog>,
    pub events: Arc<EventManager>,
    settings: RwLock<Settings>,
    pub paths: Paths,
    pub flags: RuntimeFlags,
    pub confirm: ConfirmStore,
    pub music: modules::music::MusicModule,
    pub sysmon: modules::system_monitor::SysMonModule,
    pub clipboard: modules::clipboard::ClipboardModule,
    pub timers: modules::timers::TimersModule,
    pub notifications: modules::notifications::NotificationsModule,
    pub game_mode: modules::game_mode::GameModeModule,
    pub apps: modules::applications::AppsModule,
    pub battery: modules::battery::BatteryModule,
    pub updater: modules::updater::UpdaterModule,
    pub privacy: modules::privacy::PrivacyModule,
    pub downloads: modules::downloads::DownloadsModule,
    pub phone: modules::phone::PhoneModule,
    pub voice: modules::voice::VoiceModule,
    pub calendar: modules::calendar::CalendarModule,
    pub pomodoro: modules::pomodoro::PomodoroModule,
    pub fullscreen: modules::fullscreen::FullscreenModule,
    pub speech: modules::speech::SpeechModule,
    pub plugins: PluginManager,
    pub hotkeys: HotkeyManager,
    pub ai: AiWindows,
    pub monitor_watch: crate::core::task::TaskSlot,
}

impl AppState {
    pub fn new(db: Arc<Database>, log: Arc<EventLog>, settings: Settings, paths: Paths) -> Self {
        let events = EventManager::new(settings.notifications.history_limit as usize);
        events.configure(&settings.notifications);
        Self {
            plugins: PluginManager::new(paths.plugins.clone()),
            db,
            log,
            events,
            settings: RwLock::new(settings),
            paths,
            flags: RuntimeFlags { island_visible: AtomicBool::new(true), ..Default::default() },
            confirm: ConfirmStore::default(),
            music: Default::default(),
            sysmon: Default::default(),
            clipboard: Default::default(),
            timers: Default::default(),
            notifications: Default::default(),
            game_mode: Default::default(),
            apps: Default::default(),
            battery: Default::default(),
            updater: Default::default(),
            privacy: Default::default(),
            phone: Default::default(),
            voice: Default::default(),
            calendar: Default::default(),
            pomodoro: Default::default(),
            fullscreen: Default::default(),
            speech: Default::default(),
            downloads: Default::default(),
            hotkeys: Default::default(),
            ai: Default::default(),
            monitor_watch: Default::default(),
        }
    }

    pub fn settings(&self) -> Settings {
        self.settings.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    pub fn with_settings<R>(&self, f: impl FnOnce(&Settings) -> R) -> R {
        f(&self.settings.read().unwrap_or_else(|p| p.into_inner()))
    }

    pub fn game_mode_active(&self) -> bool {
        self.flags.game_mode.load(Ordering::SeqCst)
    }

    pub fn island_visible(&self) -> bool {
        self.flags.island_visible.load(Ordering::SeqCst)
    }

    /// Изменяет настройки: валидация → сохранение изменённых разделов → событие → побочные эффекты.
    pub fn update_settings(self: &Arc<Self>, app: &AppHandle, f: impl FnOnce(&mut Settings) -> AppResult<()>) -> AppResult<Settings> {
        let (old, new) = {
            let mut g = self.settings.write().unwrap_or_else(|p| p.into_inner());
            let old = g.clone();
            let mut next = old.clone();
            f(&mut next)?;
            next.sanitize();
            *g = next.clone();
            (old, next)
        };
        let old_v = serde_json::to_value(&old)?;
        let new_v = serde_json::to_value(&new)?;
        for section in crate::core::settings::SECTIONS {
            if old_v.get(section) != new_v.get(section) {
                if let Err(e) = self.db.save_section(&new, section) {
                    self.log.error(category::DATABASE, "settings", format!("не удалось сохранить раздел {section}: {e}"));
                }
            }
        }
        self.events.emit(names::SETTINGS_CHANGED, new.clone());
        self.apply_side_effects(app, &old, &new);
        Ok(new)
    }

    fn apply_side_effects(self: &Arc<Self>, app: &AppHandle, old: &Settings, new: &Settings) {
        self.events.configure(&new.notifications);
        self.log.configure(new.logging.max_entries, &new.logging.level);

        if old.general.launch_at_startup != new.general.launch_at_startup {
            crate::tray::set_autostart(app, new.general.launch_at_startup, &self.log);
        }
        if old.appearance.settings_effect != new.appearance.settings_effect || old.appearance.settings_opacity != new.appearance.settings_opacity {
            if let Some(w) = tauri::Manager::get_webview_window(app, crate::core::window::SETTINGS) {
                crate::core::window::apply_settings_effect(&w, self);
            }
        }
        if old.calendar.urls != new.calendar.urls {
            self.calendar.refresh(self);
        }
        if old.notifications.dnd_allow != new.notifications.dnd_allow {
            // Список исключений изменился — переустанавливаем «Не беспокоить» с новым списком.
            self.notifications.sync_popups(self, false);
        }
        if old.position != new.position && !crate::core::window::take_skip_apply() {
            crate::core::window::apply_position(app, self);
        }
        if old.clipboard.max_items != new.clipboard.max_items {
            let _ = self.clipboard.trim(&self.db, new.clipboard.max_items);
        }
        if old.game_mode.manual != new.game_mode.manual || old.game_mode.auto != new.game_mode.auto {
            self.game_mode.force_check();
        }
        if old.modules.plugins != new.modules.plugins || old.security.plugins_enabled != new.security.plugins_enabled {
            self.events.emit(names::PLUGINS_CHANGED, ());
        }
        modules::reconcile(app, self);
    }

    /// Заменяет набор разделов целиком (применение профиля).
    pub fn replace_sections(self: &Arc<Self>, app: &AppHandle, data: &serde_json::Value) -> AppResult<Settings> {
        self.update_settings(app, |s| {
            for section in crate::core::settings::PROFILE_SECTIONS {
                if let Some(v) = data.get(*section) {
                    s.apply_section(section, v.clone())?;
                }
            }
            Ok(())
        })
    }
}

pub fn shared(app: &AppHandle) -> Shared {
    app.state::<Shared>().inner().clone()
}
