//! Модель настроек. Настройки разбиты на разделы, каждый раздел хранится
//! отдельной строкой таблицы `settings` (ключ = имя раздела, значение = JSON).
//! Любые входящие значения проходят через `sanitize()` — это защищает от
//! некорректных данных из IPC и из повреждённой базы.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::core::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GeneralSettings {
    pub launch_at_startup: bool,
    /// Если включено — закрытие окон не завершает приложение.
    pub background_mode: bool,
    /// Устарело (не используется): остров всегда показывается при запуске.
    pub start_hidden: bool,
    pub language: String,
    /// Проверять новые версии на GitHub (запрос к api.github.com без каких-либо данных пользователя).
    pub check_updates: bool,
}
impl Default for GeneralSettings {
    fn default() -> Self {
        Self { launch_at_startup: false, background_mode: true, start_hidden: false, language: "ru".into(), check_updates: true }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AnimationLevel {
    Full,
    Reduced,
    Off,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppearanceSettings {
    pub theme_id: String,
    /// dark | light | system
    pub mode: String,
    pub accent: String,
    pub opacity: f32,
    pub radius: u32,
    pub animations: AnimationLevel,
    pub animation_speed: f32,
    pub scale: f32,
    /// Цвет острова: graphite | grey | black.
    pub island_tone: String,
    /// Приветствие («Доброе утро», «Добрый вечер»…) при включении и пробуждении.
    pub greeting: bool,
    /// Непрозрачность окна настроек (0.4–1).
    pub settings_opacity: f32,
    /// Эффект под прозрачным окном настроек: none | mica | acrylic.
    pub settings_effect: String,
    /// Свои обои окна настроек (файл в папке программы) и их затемнение/размытие.
    pub wallpaper: Option<String>,
    pub wallpaper_dim: f32,
    pub wallpaper_blur: u32,
    /// Язык интерфейса: ru | en.
    pub language: String,
    /// Полноэкранное видео/программа — остров сжимается в тонкую полоску.
    pub fullscreen_strip: bool,
    /// Шрифт интерфейса: system | встроенный id | custom:<файл в data/fonts>.
    pub font: String,
    /// Стиль острова: solid | glass | neon | aura | graphite.
    pub island_style: String,
    /// В редакторе своих тем доступен подбор цветов из картинки (выключено по умолчанию).
    pub theme_from_image: bool,
}
impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            theme_id: "midnight".into(),
            mode: "system".into(),
            accent: "#4c9aff".into(),
            opacity: 1.0,
            radius: 22,
            animations: AnimationLevel::Full,
            animation_speed: 1.0,
            scale: 1.0,
            island_tone: "graphite".into(),
            greeting: true,
            settings_opacity: 1.0,
            settings_effect: "none".into(),
            wallpaper: None,
            wallpaper_dim: 0.45,
            wallpaper_blur: 12,
            language: "ru".into(),
            fullscreen_strip: true,
            font: "system".into(),
            island_style: "solid".into(),
            theme_from_image: false,
        }
    }
}

/// Положение панели на конкретном мониторе. `rel_x` — доля ширины монитора
/// для центра панели (0..1), `top` — отступ сверху в логических пикселях.
/// Относительные координаты позволяют корректно переживать смену разрешения.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Placement {
    pub rel_x: f64,
    pub top: f64,
}
impl Default for Placement {
    fn default() -> Self {
        Self { rel_x: 0.5, top: 0.0 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct PositionSettings {
    /// Имя монитора, на котором должна находиться панель. `None` — основной.
    pub monitor: Option<String>,
    pub placements: BTreeMap<String, Placement>,
    pub locked: bool,
    /// edge — остров вырастает из верхнего края экрана, float — висит чуть ниже края.
    pub style: String,
}
impl Default for PositionSettings {
    fn default() -> Self {
        Self { monitor: None, placements: BTreeMap::new(), locked: false, style: "float".into() }
    }
}

/// Отступ плавающего острова от верхнего края экрана (логические пиксели).
pub const FLOAT_TOP: f64 = 10.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ModulesSettings {
    pub music: bool,
    pub notifications: bool,
    pub system_monitor: bool,
    pub bluetooth: bool,
    pub timers: bool,
    pub clipboard: bool,
    pub volume: bool,
    pub brightness: bool,
    pub quick_settings: bool,
    pub power: bool,
    pub applications: bool,
    pub ai: bool,
    pub integrations: bool,
    pub plugins: bool,
    pub notes: bool,
    pub shelf: bool,
    /// Анимация при подключении зарядки и предупреждения о низком заряде.
    pub battery: bool,
    /// Индикатор микрофона и камеры, звонки.
    pub privacy: bool,
    /// Загрузки браузеров в острове.
    pub downloads: bool,
    /// Загрузки Steam в острове.
    pub steam: bool,
}
impl Default for ModulesSettings {
    fn default() -> Self {
        Self {
            music: true,
            notifications: true,
            system_monitor: true,
            bluetooth: true,
            timers: true,
            clipboard: true,
            volume: true,
            brightness: true,
            quick_settings: true,
            power: true,
            applications: true,
            ai: true,
            integrations: true,
            plugins: true,
            notes: true,
            shelf: true,
            battery: true,
            privacy: true,
            downloads: true,
            steam: true,
        }
    }
}
impl ModulesSettings {
    pub fn is_enabled(&self, id: &str) -> bool {
        match id {
            "music" => self.music,
            "notifications" => self.notifications,
            "system_monitor" => self.system_monitor,
            "bluetooth" => self.bluetooth,
            "timers" => self.timers,
            "clipboard" => self.clipboard,
            "volume" => self.volume,
            "brightness" => self.brightness,
            "quick_settings" => self.quick_settings,
            "power" => self.power,
            "applications" => self.applications,
            "ai" => self.ai,
            "integrations" => self.integrations,
            "plugins" => self.plugins,
            "notes" => self.notes,
            "shelf" => self.shelf,
            "battery" => self.battery,
            "privacy" => self.privacy,
            "downloads" => self.downloads,
            "steam" => self.steam,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppRule {
    /// Имя приложения (как его сообщает система), сравнивается без учёта регистра.
    pub app: String,
    pub muted: bool,
    /// low | normal | high | critical, `None` — по умолчанию.
    pub priority: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct NotificationSettings {
    pub enabled: bool,
    pub listen_system: bool,
    pub dnd: bool,
    pub dnd_allow_critical: bool,
    pub duration_secs: u32,
    pub history_limit: u32,
    pub show_body: bool,
    pub app_rules: Vec<AppRule>,
    /// Уведомления других приложений показываются только в острове, без системных баннеров.
    pub island_only: bool,
    /// Свой короткий звук острова при новом уведомлении.
    pub sound: bool,
    /// Несколько уведомлений одного приложения показываются одной группой.
    pub group_by_app: bool,
    /// Windows: приложения (AUMID), которые пропускаются через «Не беспокоить» острова.
    pub dnd_allow: Vec<String>,
}
impl Default for NotificationSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            listen_system: true,
            dnd: false,
            dnd_allow_critical: true,
            duration_secs: 6,
            history_limit: 100,
            show_body: true,
            app_rules: vec![],
            island_only: true,
            sound: true,
            group_by_app: true,
            dnd_allow: vec![],
        }
    }
}

/// Встроенные шрифты интерфейса (id из фронтенда).
pub const BUILTIN_FONTS: &[&str] = &["system", "inter", "manrope", "onest", "golos", "montserrat", "rubik", "nunito", "plex", "comfortaa"];
/// Встроенные звуки уведомлений и таймера.
pub const NOTIFICATION_SOUNDS: &[&str] = &["glass", "drop", "pop", "bell", "chord", "pluck", "bubble", "soft"];
pub const TIMER_SOUNDS: &[&str] = &["radar", "beacon", "classic", "chimes", "rise", "marimba", "pulse", "morning"];

/// Звуки острова: уведомления и окончание таймера настраиваются независимо.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct SoundSettings {
    /// id встроенного звука или custom:notification.<ext>.
    pub notification: String,
    pub notification_volume: f32,
    /// Звук по окончании таймера.
    pub timer_enabled: bool,
    pub timer: String,
    pub timer_volume: f32,
    /// Сколько секунд звучит таймер; 0 — пока не остановят.
    pub timer_limit_secs: u32,
}
impl Default for SoundSettings {
    fn default() -> Self {
        Self {
            notification: "glass".into(),
            notification_volume: 0.6,
            timer_enabled: true,
            timer: "radar".into(),
            timer_volume: 0.8,
            timer_limit_secs: 60,
        }
    }
}

/// Индикаторы клавиатуры в острове.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct KeyboardSettings {
    /// Показывать смену раскладки.
    pub layout: bool,
    /// Показывать Caps Lock.
    pub caps: bool,
}
impl Default for KeyboardSettings {
    fn default() -> Self {
        Self { layout: true, caps: true }
    }
}

/// VPN-клиент для кнопки в центре управления.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct VpnSettings {
    /// auto | happ | incy | none
    pub client: String,
    /// Свой путь к программе (если не нашлась сама).
    pub path: String,
    /// Коротко показывать в острове подключение и отключение.
    pub notify: bool,
}
impl Default for VpnSettings {
    fn default() -> Self {
        Self { client: "auto".into(), path: String::new(), notify: true }
    }
}

fn valid_custom(name: &str, prefix: &str, exts: &[&str]) -> bool {
    let Some(file) = name.strip_prefix("custom:") else { return false };
    file.starts_with(prefix)
        && file.len() < 120
        && !file.contains(['/', '\\', ':'])
        && !file.contains("..")
        && exts.iter().any(|e| file.to_ascii_lowercase().ends_with(&format!(".{e}")))
}

/// Календарь по ICS-ссылкам.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct CalendarSettings {
    pub urls: Vec<String>,
    /// За сколько минут напоминать о событии.
    pub remind_minutes: u32,
}
impl Default for CalendarSettings {
    fn default() -> Self {
        Self { urls: vec![], remind_minutes: 10 }
    }
}

/// Голос → текст.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct SpeechSettings {
    /// ru | en
    pub lang: String,
    /// Сразу превращать голосовые заметки в текст.
    pub auto: bool,
}
impl Default for SpeechSettings {
    fn default() -> Self {
        Self { lang: "ru".into(), auto: true }
    }
}

/// Помодоро.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct PomodoroSettings {
    pub focus_min: u32,
    pub short_min: u32,
    pub long_min: u32,
    pub cycles: u32,
    pub dnd_during_focus: bool,
}
impl Default for PomodoroSettings {
    fn default() -> Self {
        Self { focus_min: 25, short_min: 5, long_min: 15, cycles: 4, dnd_during_focus: true }
    }
}

/// Связь с телефоном (протокол KDE Connect).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct PhoneSettings {
    pub enabled: bool,
    /// Имя этого компьютера, которое видит телефон.
    pub device_name: String,
    pub notifications: bool,
    pub calls: bool,
    pub clipboard: bool,
    pub battery: bool,
    pub media: bool,
}
impl Default for PhoneSettings {
    fn default() -> Self {
        Self { enabled: true, device_name: String::new(), notifications: true, calls: true, clipboard: true, battery: true, media: false }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MusicSettings {
    pub show_artwork: bool,
    pub show_in_compact: bool,
    pub preferred_source: Option<String>,
    pub poll_interval_ms: u64,
    /// Строка текста песни в свёрнутом острове (LRCLIB).
    pub lyrics: bool,
}
impl Default for MusicSettings {
    fn default() -> Self {
        Self { show_artwork: true, show_in_compact: true, preferred_source: None, poll_interval_ms: 1000, lyrics: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SystemMonitorSettings {
    pub interval_ms: u64,
    pub show_in_compact: bool,
    pub history_points: u32,
}
impl Default for SystemMonitorSettings {
    fn default() -> Self {
        Self { interval_ms: 2000, show_in_compact: false, history_points: 60 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ClipboardSettings {
    pub history_enabled: bool,
    pub max_items: u32,
    pub store_images: bool,
    pub store_files: bool,
    pub ignore_sensitive: bool,
    pub excluded_apps: Vec<String>,
}
impl Default for ClipboardSettings {
    fn default() -> Self {
        Self {
            history_enabled: true,
            max_items: 100,
            store_images: true,
            store_files: true,
            ignore_sensitive: true,
            excluded_apps: vec![
                "keepass.exe".into(),
                "keepassxc.exe".into(),
                "keepassxc".into(),
                "1password.exe".into(),
                "bitwarden.exe".into(),
                "bitwarden".into(),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct GameModeSettings {
    pub auto: bool,
    /// `Some(true)` — принудительно включён, `Some(false)` — принудительно выключен.
    pub manual: Option<bool>,
    pub detect_fullscreen: bool,
    pub detect_games: bool,
    pub suppress_notifications: bool,
    pub exclusions: Vec<String>,
    pub extra_games: Vec<String>,
    pub check_interval_ms: u64,
}
impl Default for GameModeSettings {
    fn default() -> Self {
        Self {
            auto: true,
            manual: None,
            detect_fullscreen: true,
            detect_games: true,
            suppress_notifications: true,
            exclusions: vec![],
            extra_games: vec![],
            check_interval_ms: 2000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PerformanceSettings {
    pub pause_when_hidden: bool,
    /// Windows: WebView2 без GPU-композитинга (меньше памяти). Применяется после перезапуска.
    pub software_rendering: bool,
    pub free_cards_on_collapse: bool,
    pub single_ai_window: bool,
    pub close_ai_on_hide: bool,
}
impl Default for PerformanceSettings {
    fn default() -> Self {
        Self { pause_when_hidden: true, software_rendering: true, free_cards_on_collapse: true, single_ai_window: true, close_ai_on_hide: false }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SecuritySettings {
    pub plugins_enabled: bool,
    pub log_system_actions: bool,
}
impl Default for SecuritySettings {
    fn default() -> Self {
        Self { plugins_enabled: true, log_system_actions: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct LoggingSettings {
    pub max_entries: u32,
    pub level: String,
}
impl Default for LoggingSettings {
    fn default() -> Self {
        Self { max_entries: 2000, level: "info".into() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ProfileCondition {
    GameMode,
    OnBattery,
    TimeRange { from: String, to: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileRule {
    pub id: String,
    pub condition: ProfileCondition,
    pub profile_id: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct ProfilesSettings {
    pub active: Option<String>,
    pub auto_switch: bool,
    pub rules: Vec<ProfileRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct AppsSettings {
    pub favorites: Vec<String>,
    pub hidden: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct Settings {
    pub general: GeneralSettings,
    pub appearance: AppearanceSettings,
    pub position: PositionSettings,
    pub modules: ModulesSettings,
    pub notifications: NotificationSettings,
    pub music: MusicSettings,
    pub system_monitor: SystemMonitorSettings,
    pub clipboard: ClipboardSettings,
    pub game_mode: GameModeSettings,
    pub performance: PerformanceSettings,
    pub security: SecuritySettings,
    pub logging: LoggingSettings,
    pub profiles: ProfilesSettings,
    pub apps: AppsSettings,
    pub phone: PhoneSettings,
    pub calendar: CalendarSettings,
    pub pomodoro: PomodoroSettings,
    pub speech: SpeechSettings,
    pub sounds: SoundSettings,
    pub keyboard: KeyboardSettings,
    pub vpn: VpnSettings,
}

pub const SECTIONS: &[&str] = &[
    "general",
    "appearance",
    "position",
    "modules",
    "notifications",
    "music",
    "systemMonitor",
    "clipboard",
    "gameMode",
    "performance",
    "security",
    "logging",
    "profiles",
    "apps",
    "phone",
    "calendar",
    "pomodoro",
    "speech",
    "sounds",
    "keyboard",
    "vpn",
];

/// Разделы, которые сохраняются в профиль.
pub const PROFILE_SECTIONS: &[&str] = &["appearance", "position", "modules", "notifications", "music", "systemMonitor", "gameMode", "performance"];

fn clamp_list(list: &mut Vec<String>, max_len: usize, max_item: usize) {
    list.retain(|s| !s.trim().is_empty());
    for s in list.iter_mut() {
        *s = s.trim().chars().take(max_item).collect();
    }
    list.dedup();
    list.truncate(max_len);
}

fn is_hex_color(s: &str) -> bool {
    let b = s.as_bytes();
    (b.len() == 7 || b.len() == 9) && b[0] == b'#' && b[1..].iter().all(|c| c.is_ascii_hexdigit())
}

fn valid_hhmm(s: &str) -> bool {
    let parts: Vec<&str> = s.split(':').collect();
    parts.len() == 2
        && parts[0].parse::<u32>().map(|h| h < 24).unwrap_or(false)
        && parts[1].parse::<u32>().map(|m| m < 60).unwrap_or(false)
}

impl Settings {
    /// Приводит значения к допустимым диапазонам.
    pub fn sanitize(&mut self) {
        let a = &mut self.appearance;
        if !matches!(a.mode.as_str(), "dark" | "light" | "system") {
            a.mode = "system".into();
        }
        if !is_hex_color(&a.accent) {
            a.accent = AppearanceSettings::default().accent;
        }
        a.theme_id = a.theme_id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').take(64).collect();
        if a.theme_id.is_empty() {
            a.theme_id = "midnight".into();
        }
        a.opacity = if a.opacity.is_finite() { a.opacity.clamp(0.35, 1.0) } else { 0.96 };
        a.radius = a.radius.clamp(6, 40);
        a.animation_speed = if a.animation_speed.is_finite() { a.animation_speed.clamp(0.25, 3.0) } else { 1.0 };
        a.scale = if a.scale.is_finite() { a.scale.clamp(0.75, 1.5) } else { 1.0 };
        a.settings_opacity = if a.settings_opacity.is_finite() { a.settings_opacity.clamp(0.4, 1.0) } else { 1.0 };
        if !matches!(a.settings_effect.as_str(), "none" | "mica" | "acrylic") {
            a.settings_effect = "none".into();
        }
        if a.wallpaper.as_deref().is_some_and(|w| !w.starts_with("wallpaper.") || w.contains('/') || w.contains('\\')) {
            a.wallpaper = None;
        }
        a.wallpaper_dim = if a.wallpaper_dim.is_finite() { a.wallpaper_dim.clamp(0.0, 0.9) } else { 0.45 };
        a.wallpaper_blur = a.wallpaper_blur.min(40);
        if !matches!(a.language.as_str(), "ru" | "en") {
            a.language = "ru".into();
        }
        if !matches!(a.island_tone.as_str(), "graphite" | "grey" | "black") {
            a.island_tone = "graphite".into();
        }
        if !matches!(a.island_style.as_str(), "solid" | "glass" | "neon" | "aura" | "graphite") {
            a.island_style = "solid".into();
        }
        if !matches!(self.position.style.as_str(), "edge" | "float") {
            self.position.style = "float".into();
        }
        if !BUILTIN_FONTS.contains(&a.font.as_str()) && !valid_custom(&a.font, "", &["ttf", "otf"]) {
            a.font = "system".into();
        }
        let snd = &mut self.sounds;
        if !NOTIFICATION_SOUNDS.contains(&snd.notification.as_str()) && !valid_custom(&snd.notification, "notification.", &["wav", "mp3", "ogg"]) {
            snd.notification = "glass".into();
        }
        if !TIMER_SOUNDS.contains(&snd.timer.as_str()) && !valid_custom(&snd.timer, "timer.", &["wav", "mp3", "ogg"]) {
            snd.timer = "radar".into();
        }
        snd.notification_volume = if snd.notification_volume.is_finite() { snd.notification_volume.clamp(0.0, 1.0) } else { 0.6 };
        snd.timer_volume = if snd.timer_volume.is_finite() { snd.timer_volume.clamp(0.0, 1.0) } else { 0.8 };
        snd.timer_limit_secs = snd.timer_limit_secs.min(600);
        if !matches!(self.vpn.client.as_str(), "auto" | "happ" | "incy" | "none") {
            self.vpn.client = "auto".into();
        }
        self.vpn.path = self.vpn.path.trim().chars().filter(|c| !c.is_control()).take(1024).collect();
        if !matches!(self.speech.lang.as_str(), "ru" | "en") {
            self.speech.lang = "ru".into();
        }
        clamp_list(&mut self.calendar.urls, 10, 2048);
        self.calendar.urls.retain(|u| u.starts_with("https://") || u.starts_with("webcal://"));
        self.calendar.remind_minutes = self.calendar.remind_minutes.clamp(0, 120);
        let p = &mut self.pomodoro;
        p.focus_min = p.focus_min.clamp(1, 180);
        p.short_min = p.short_min.clamp(1, 60);
        p.long_min = p.long_min.clamp(1, 120);
        p.cycles = p.cycles.clamp(1, 12);
        self.phone.device_name = self.phone.device_name.trim().chars().filter(|c| !c.is_control()).take(48).collect();

        for p in self.position.placements.values_mut() {
            if !p.rel_x.is_finite() {
                p.rel_x = 0.5;
            }
            p.rel_x = p.rel_x.clamp(0.0, 1.0);
            if !p.top.is_finite() {
                p.top = 8.0;
            }
            p.top = p.top.clamp(0.0, 8000.0);
        }
        if self.position.placements.len() > 32 {
            let keys: Vec<String> = self.position.placements.keys().skip(32).cloned().collect();
            for k in keys {
                self.position.placements.remove(&k);
            }
        }

        let n = &mut self.notifications;
        n.duration_secs = n.duration_secs.clamp(2, 60);
        n.history_limit = n.history_limit.clamp(0, 500);
        n.app_rules.retain(|r| !r.app.trim().is_empty());
        n.app_rules.truncate(200);
        clamp_list(&mut n.dnd_allow, 50, 256);
        for r in n.app_rules.iter_mut() {
            r.app = r.app.trim().chars().take(128).collect();
            if let Some(p) = &r.priority {
                if !matches!(p.as_str(), "low" | "normal" | "high" | "critical") {
                    r.priority = None;
                }
            }
        }

        self.music.poll_interval_ms = self.music.poll_interval_ms.clamp(500, 5000);
        self.system_monitor.interval_ms = self.system_monitor.interval_ms.clamp(1000, 30_000);
        self.system_monitor.history_points = self.system_monitor.history_points.clamp(10, 300);

        let c = &mut self.clipboard;
        c.max_items = c.max_items.clamp(5, 1000);
        clamp_list(&mut c.excluded_apps, 200, 260);

        let g = &mut self.game_mode;
        g.check_interval_ms = g.check_interval_ms.clamp(1000, 10_000);
        clamp_list(&mut g.exclusions, 500, 260);
        clamp_list(&mut g.extra_games, 500, 260);

        self.logging.max_entries = self.logging.max_entries.clamp(100, 50_000);
        if !matches!(self.logging.level.as_str(), "error" | "warn" | "info" | "debug") {
            self.logging.level = "info".into();
        }

        self.profiles.rules.retain(|r| match &r.condition {
            ProfileCondition::TimeRange { from, to } => valid_hhmm(from) && valid_hhmm(to),
            _ => true,
        });
        self.profiles.rules.truncate(50);

        clamp_list(&mut self.apps.favorites, 200, 1024);
        clamp_list(&mut self.apps.hidden, 1000, 1024);
        if self.general.language.is_empty() {
            self.general.language = "ru".into();
        }
    }

    pub fn section_value(&self, section: &str) -> AppResult<serde_json::Value> {
        let v = serde_json::to_value(self)?;
        v.get(section).cloned().ok_or_else(|| AppError::invalid(format!("неизвестный раздел настроек «{section}»")))
    }

    /// Применяет JSON раздела поверх текущих настроек с валидацией.
    pub fn apply_section(&mut self, section: &str, value: serde_json::Value) -> AppResult<()> {
        if !SECTIONS.contains(&section) {
            return Err(AppError::invalid(format!("неизвестный раздел настроек «{section}»")));
        }
        let mut whole = serde_json::to_value(&*self)?;
        let target = whole.get_mut(section).ok_or_else(|| AppError::invalid("раздел не найден"))?;
        merge_json(target, value);
        let mut next: Settings = serde_json::from_value(whole)?;
        next.sanitize();
        *self = next;
        Ok(())
    }
}

/// Рекурсивное слияние: объекты сливаются по ключам, остальное заменяется.
pub fn merge_json(target: &mut serde_json::Value, patch: serde_json::Value) {
    match (target, patch) {
        (serde_json::Value::Object(t), serde_json::Value::Object(p)) => {
            for (k, v) in p {
                match t.get_mut(&k) {
                    Some(existing) if existing.is_object() && v.is_object() => merge_json(existing, v),
                    _ => {
                        t.insert(k, v);
                    }
                }
            }
        }
        (t, p) => *t = p,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_clamps_ranges() {
        let mut s = Settings::default();
        s.appearance.opacity = 5.0;
        s.appearance.radius = 500;
        s.appearance.accent = "red; background:url(x)".into();
        s.system_monitor.interval_ms = 1;
        s.notifications.duration_secs = 0;
        s.sanitize();
        assert_eq!(s.appearance.opacity, 1.0);
        assert_eq!(s.appearance.radius, 40);
        assert_eq!(s.appearance.accent, "#4c9aff");
        assert_eq!(s.system_monitor.interval_ms, 1000);
        assert_eq!(s.notifications.duration_secs, 2);
    }

    #[test]
    fn apply_section_merges_partial() {
        let mut s = Settings::default();
        s.apply_section("appearance", serde_json::json!({ "radius": 12 })).unwrap();
        assert_eq!(s.appearance.radius, 12);
        assert_eq!(s.appearance.mode, "system");
        assert!(s.apply_section("unknown", serde_json::json!({})).is_err());
        assert!(s.apply_section("appearance", serde_json::json!({ "radius": "abc" })).is_err());
    }

    #[test]
    fn sanitize_fonts_and_sounds() {
        let mut s = Settings::default();
        s.appearance.font = "custom:../evil.ttf".into();
        s.sounds.notification = "custom:timer.mp3".into();
        s.sounds.timer = "custom:timer.wav".into();
        s.sounds.timer_volume = 7.0;
        s.vpn.client = "openvpn".into();
        s.sanitize();
        assert_eq!(s.appearance.font, "system");
        assert_eq!(s.sounds.notification, "glass");
        assert_eq!(s.sounds.timer, "custom:timer.wav");
        assert_eq!(s.sounds.timer_volume, 1.0);
        assert_eq!(s.vpn.client, "auto");
        s.appearance.font = "custom:My Font.otf".into();
        s.sanitize();
        assert_eq!(s.appearance.font, "custom:My Font.otf");
    }

    #[test]
    fn invalid_time_rules_dropped() {
        let mut s = Settings::default();
        s.profiles.rules.push(ProfileRule {
            id: "r".into(),
            condition: ProfileCondition::TimeRange { from: "25:00".into(), to: "10:00".into() },
            profile_id: "p".into(),
            enabled: true,
        });
        s.sanitize();
        assert!(s.profiles.rules.is_empty());
    }
}
