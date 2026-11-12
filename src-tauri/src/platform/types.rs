//! Общие для всех платформ типы данных системных адаптеров.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct MediaSessionInfo {
    pub id: String,
    /// Исходный идентификатор (AUMID на Windows, D-Bus имя на Linux).
    pub source_app: String,
    pub app_name: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// playing | paused | stopped | unknown
    pub status: String,
    pub position_ms: Option<u64>,
    pub duration_ms: Option<u64>,
    /// Время (unix ms), к которому относится `position_ms`.
    pub position_updated_at: i64,
    pub can_play_pause: bool,
    pub can_next: bool,
    pub can_prev: bool,
    pub can_seek: bool,
    pub supports_volume: bool,
    pub volume: Option<f32>,
    /// Ключ трека для кэша обложек.
    pub artwork_key: String,
    /// Прямая ссылка на обложку (Linux MPRIS `mpris:artUrl`, https).
    pub artwork_url: Option<String>,
    pub is_current: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "type")]
pub enum MediaAction {
    PlayPause,
    Play,
    Pause,
    Next,
    Previous,
    Seek { position_ms: u64 },
    SetVolume { volume: f32 },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AudioDevice {
    pub id: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct VolumeState {
    pub available: bool,
    pub level: f32,
    pub muted: bool,
    pub device: Option<AudioDevice>,
    pub devices: Vec<AudioDevice>,
    pub can_select_device: bool,
    pub backend: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DisplayBrightness {
    pub id: String,
    pub name: String,
    pub value: u32,
    pub method: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BrightnessState {
    pub displays: Vec<DisplayBrightness>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct WifiState {
    pub available: bool,
    pub on: bool,
    pub can_toggle: bool,
    pub connected: bool,
    pub ssid: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BluetoothDeviceInfo {
    pub id: String,
    pub name: String,
    pub connected: bool,
    pub paired: bool,
    pub kind: String,
    pub battery: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BluetoothState {
    pub available: bool,
    pub powered: bool,
    pub can_toggle: bool,
    pub devices: Vec<BluetoothDeviceInfo>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PowerState {
    pub has_battery: bool,
    pub percent: Option<u8>,
    pub charging: bool,
    pub on_ac: Option<bool>,
    pub secs_remaining: Option<u64>,
    /// Режим энергосбережения: `None` — неизвестно/недоступно.
    pub saver: Option<bool>,
    pub saver_can_toggle: bool,
    pub saver_backend: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AirplaneState {
    /// Известно ли состояние (Windows: все радиомодули выключены).
    pub known: bool,
    pub on: bool,
    /// Системного API переключения режима «в самолёте» нет — только переход в настройки.
    pub can_toggle: bool,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ForegroundInfo {
    pub pid: u32,
    pub exe_name: String,
    pub exe_path: String,
    pub fullscreen: bool,
    /// Эксклюзивный полноэкранный режим Direct3D / режим презентации.
    pub exclusive_fullscreen: bool,
    /// Рабочий стол / оболочка — никогда не считается игрой.
    pub is_shell: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", rename_all_fields = "camelCase", tag = "type")]
pub enum LaunchTarget {
    /// Windows: элемент shell:AppsFolder (AUMID или путь с known-folder GUID).
    AppsFolder { id: String },
    /// Исполняемый файл с аргументами.
    Exe { path: String, args: Vec<String> },
    /// Linux: .desktop-файл.
    Desktop { path: String },
    /// URI протокола (tg://, steam://, https://).
    Uri { uri: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledApp {
    pub id: String,
    pub name: String,
    pub target: LaunchTarget,
    /// Путь к исполняемому файлу, если удалось определить.
    pub exe_path: Option<String>,
    /// Подсказка для иконки (Linux: имя иконки темы).
    pub icon_hint: Option<String>,
    pub keywords: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SystemNotification {
    pub key: String,
    pub app_name: String,
    pub app_id: Option<String>,
    pub title: String,
    pub body: String,
    pub icon: Option<String>,
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ListenerStatus {
    pub available: bool,
    pub active: bool,
    pub backend: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProxyInfo {
    pub enabled: bool,
    pub server: Option<String>,
    pub auto_config: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SettingsPage {
    Main,
    Bluetooth,
    Display,
    Sound,
    Network,
    Wifi,
    Airplane,
    Battery,
    Notifications,
    Apps,
    Power,
}

#[derive(Debug, Clone, Default)]
pub struct GpuSample {
    pub name: Option<String>,
    pub usage: Option<f32>,
    pub temperature: Option<f32>,
    pub memory_used: Option<u64>,
    pub memory_total: Option<u64>,
    pub backend: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct EnvironmentInfo {
    pub os: String,
    pub os_version: String,
    pub desktop: Option<String>,
    pub session_type: Option<String>,
    pub global_hotkeys_supported: bool,
    pub notes: Vec<String>,
}

/// Устройство (микрофон или камера), которое сейчас использует приложение.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DeviceUse {
    /// "microphone" | "camera"
    pub device: String,
    pub app_name: String,
    /// Путь к исполняемому файлу (если известен).
    pub app_path: Option<String>,
    /// Windows: семейство пакета Microsoft Store (Microsoft.WindowsSoundRecorder_8wekyb3d8bbwe).
    pub app_id: Option<String>,
    /// Когда начато использование (unix ms).
    pub since: i64,
}
