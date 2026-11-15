// Типы, зеркалирующие структуры Rust (serde camelCase).

export type Priority = "low" | "normal" | "high" | "critical";
export type AnimationLevel = "full" | "reduced" | "off";

export interface AppError {
  kind: "unsupported" | "permission_denied" | "invalid_input" | "not_found" | "database" | "io" | "system" | "other";
  message: string;
}

export interface Placement {
  relX: number;
  top: number;
}

export interface SoundSettings {
  notification: string;
  notificationVolume: number;
  timerEnabled: boolean;
  timer: string;
  timerVolume: number;
  /** 0 — звучит, пока не остановят. */
  timerLimitSecs: number;
}

export interface KbdState {
  layout: string | null;
  caps: boolean;
}

export interface VpnStatus {
  client: "" | "happ" | "incy";
  name: string;
  installed: boolean;
  connected: boolean;
  canControl: boolean;
  path: string | null;
}

export interface FontFile {
  file: string;
  family: string;
  size: number;
}

export interface PhoneSettings {
  enabled: boolean;
  deviceName: string;
  notifications: boolean;
  calls: boolean;
  clipboard: boolean;
  battery: boolean;
  media: boolean;
}

export interface Settings {
  general: { launchAtStartup: boolean; backgroundMode: boolean; startHidden: boolean; language: string; checkUpdates: boolean };
  appearance: {
    themeId: string;
    mode: "dark" | "light" | "system";
    accent: string;
    opacity: number;
    radius: number;
    animations: AnimationLevel;
    animationSpeed: number;
    scale: number;
    islandTone: "graphite" | "grey" | "black";
    greeting: boolean;
    settingsOpacity: number;
    settingsEffect: "none" | "mica" | "acrylic";
    wallpaper: string | null;
    wallpaperDim: number;
    wallpaperBlur: number;
    language: "ru" | "en";
    fullscreenStrip: boolean;
    font: string;
  };
  position: { monitor: string | null; placements: Record<string, Placement>; locked: boolean; style: "edge" | "float" };
  modules: Record<ModuleId, boolean>;
  notifications: {
    enabled: boolean;
    listenSystem: boolean;
    dnd: boolean;
    dndAllowCritical: boolean;
    durationSecs: number;
    historyLimit: number;
    showBody: boolean;
    appRules: AppRule[];
    islandOnly: boolean;
    sound: boolean;
    groupByApp: boolean;
    dndAllow: string[];
  };
  music: { showArtwork: boolean; showInCompact: boolean; preferredSource: string | null; pollIntervalMs: number; lyrics: boolean };
  phone: PhoneSettings;
  calendar: { urls: string[]; remindMinutes: number };
  speech: { lang: "ru" | "en"; auto: boolean };
  sounds: SoundSettings;
  keyboard: { layout: boolean; caps: boolean };
  vpn: { client: "auto" | "happ" | "incy" | "none"; path: string; notify: boolean };
  pomodoro: { focusMin: number; shortMin: number; longMin: number; cycles: number; dndDuringFocus: boolean };
  systemMonitor: { intervalMs: number; showInCompact: boolean; historyPoints: number };
  clipboard: {
    historyEnabled: boolean;
    maxItems: number;
    storeImages: boolean;
    storeFiles: boolean;
    ignoreSensitive: boolean;
    excludedApps: string[];
  };
  gameMode: {
    auto: boolean;
    manual: boolean | null;
    detectFullscreen: boolean;
    detectGames: boolean;
    suppressNotifications: boolean;
    exclusions: string[];
    extraGames: string[];
    checkIntervalMs: number;
  };
  performance: { pauseWhenHidden: boolean; softwareRendering: boolean; freeCardsOnCollapse: boolean; singleAiWindow: boolean; closeAiOnHide: boolean };
  security: { pluginsEnabled: boolean; logSystemActions: boolean };
  logging: { maxEntries: number; level: string };
  profiles: { active: string | null; autoSwitch: boolean; rules: ProfileRule[] };
  apps: { favorites: string[]; hidden: string[] };
}

export type SettingsSection = keyof Settings;

export type ModuleId =
  | "music"
  | "notifications"
  | "systemMonitor"
  | "bluetooth"
  | "timers"
  | "clipboard"
  | "volume"
  | "brightness"
  | "quickSettings"
  | "power"
  | "applications"
  | "ai"
  | "integrations"
  | "plugins"
  | "notes"
  | "shelf"
  | "battery"
  | "privacy"
  | "downloads"
  | "steam";

export interface AppRule {
  app: string;
  muted: boolean;
  priority: Priority | null;
}

export type ProfileCondition = { kind: "gameMode" } | { kind: "onBattery" } | { kind: "timeRange"; from: string; to: string };

export interface ProfileRule {
  id: string;
  condition: ProfileCondition;
  profileId: string;
  enabled: boolean;
}

export interface CardInfo {
  id: string;
  title: string;
  module: string;
  moduleEnabled: boolean;
  enabled: boolean;
  sortOrder: number;
  priority: number;
  pluginId: string | null;
}

export interface EnvironmentInfo {
  os: string;
  osVersion: string;
  desktop: string | null;
  sessionType: string | null;
  globalHotkeysSupported: boolean;
  notes: string[];
}

export interface DbStatus {
  path: string | null;
  inMemory: boolean;
  recoveredFromCorruption: boolean;
  schemaVersion: number;
  warning: string | null;
}

export interface Activity {
  id: string;
  kind: string;
  priority: Priority;
  title: string;
  subtitle: string | null;
  icon: string | null;
  progress: number | null;
  updatedAt: number;
  /** Начало длительного события (звонок), unix ms. */
  startedAt?: number | null;
}

/** Кто сейчас использует микрофон и камеру. */
export interface PrivacyState {
  microphone: string[];
  camera: string[];
  micMuted: boolean | null;
}

/** Текст песни: строки с временем начала (мс). */
export interface Lyrics {
  lines: [number, string][];
  instrumental: boolean;
}

export interface NotificationAction {
  id: string;
  label: string;
}

export interface IslandNotification {
  id: string;
  kind: "system" | "timer" | "plugin" | "internal";
  source: string;
  appName: string;
  icon: string | null;
  title: string;
  body: string;
  priority: Priority;
  timestamp: number;
  actions: NotificationAction[];
  openTarget: string | null;
  durationMs: number;
  /** Предыдущие сообщения группы (новые сначала). */
  stack?: { title: string; body: string; timestamp: number }[];
  /** Сколько уведомлений в группе. */
  count?: number;
}

export interface NotificationPayload {
  current: IslandNotification | null;
  queued: number;
  dnd: boolean;
  missed: number;
}

export interface GameModeState {
  active: boolean;
  reason: string | null;
  app: string | null;
}

export interface ThemeRow {
  id: string;
  name: string;
  builtin: boolean;
  data: { dark: Record<string, string>; light: Record<string, string>; source?: { accent: string; base: string | null; tint: string | null } };
}

export interface Bootstrap {
  version: string;
  os: string;
  settings: Settings;
  cards: CardInfo[];
  environment: EnvironmentInfo;
  db: DbStatus;
  activities: Activity[];
  notification: NotificationPayload;
  gameMode: GameModeState;
  themes: ThemeRow[];
}

export interface MediaSession {
  id: string;
  sourceApp: string;
  appName: string;
  title: string;
  artist: string;
  album: string;
  status: "playing" | "paused" | "stopped" | "unknown";
  positionMs: number | null;
  durationMs: number | null;
  positionUpdatedAt: number;
  canPlayPause: boolean;
  canNext: boolean;
  canPrev: boolean;
  canSeek: boolean;
  supportsVolume: boolean;
  volume: number | null;
  artworkKey: string;
  artworkUrl: string | null;
  isCurrent: boolean;
}

export type MediaAction =
  | { type: "playPause" }
  | { type: "play" }
  | { type: "pause" }
  | { type: "next" }
  | { type: "previous" }
  | { type: "seek"; positionMs: number }
  | { type: "setVolume"; volume: number };

export interface GpuInfo {
  available: boolean;
  name: string | null;
  usage: number | null;
  temperature: number | null;
  memoryUsed: number | null;
  memoryTotal: number | null;
  backend: string | null;
}

export interface PowerState {
  hasBattery: boolean;
  percent: number | null;
  charging: boolean;
  onAc: boolean | null;
  secsRemaining: number | null;
  saver: boolean | null;
  saverCanToggle: boolean;
  saverBackend: string | null;
}

export interface SysSnapshot {
  ts: number;
  cpu: number;
  cpuName: string;
  cores: number;
  cpuTemp: number | null;
  ramUsed: number;
  ramTotal: number;
  swapUsed: number;
  swapTotal: number;
  gpu: GpuInfo;
  disks: { name: string; mount: string; total: number; available: number; removable: boolean }[];
  netRxBps: number | null;
  netTxBps: number | null;
  power: PowerState | null;
}

export interface BluetoothDevice {
  id: string;
  name: string;
  connected: boolean;
  paired: boolean;
  kind: string;
  battery: number | null;
}

export interface BluetoothState {
  available: boolean;
  powered: boolean;
  canToggle: boolean;
  devices: BluetoothDevice[];
  error: string | null;
}

export interface WifiState {
  available: boolean;
  on: boolean;
  canToggle: boolean;
  connected: boolean;
  ssid: string | null;
  error: string | null;
}

export interface AudioDevice {
  id: string;
  name: string;
  isDefault: boolean;
}

export interface VolumeState {
  available: boolean;
  level: number;
  muted: boolean;
  device: AudioDevice | null;
  devices: AudioDevice[];
  canSelectDevice: boolean;
  backend: string;
  error: string | null;
}

export interface QuickState {
  wifi: WifiState;
  bluetooth: BluetoothState;
  airplane: { known: boolean; on: boolean; canToggle: boolean; note: string };
  power: PowerState | null;
  volume: VolumeState | null;
  volumeError: string | null;
}

export interface BrightnessState {
  displays: { id: string; name: string; value: number; method: string }[];
  error: string | null;
}

export interface Timer {
  id: string;
  label: string;
  kind: "countdown" | "stopwatch";
  durationMs: number;
  elapsedMs: number;
  startedAt: number | null;
  state: "idle" | "running" | "paused" | "finished";
  createdAt: number;
}

export interface ClipItem {
  id: number;
  kind: "text" | "image" | "files";
  preview: string;
  sourceApp: string | null;
  pinned: boolean;
  createdAt: number;
}

export type LaunchTarget =
  | { type: "appsFolder"; id: string }
  | { type: "exe"; path: string; args: string[] }
  | { type: "desktop"; path: string }
  | { type: "uri"; uri: string };

export interface AppEntry {
  id: string;
  name: string;
  source: "system" | "custom" | "web" | "plugin";
  target: LaunchTarget;
  exePath: string | null;
  favorite: boolean;
  catalogId: string | null;
}

export type SupportLevel = "full" | "partial" | "notifications_only" | "launch_only" | "web_only";

export interface ProxyInfo {
  enabled: boolean;
  server: string | null;
  autoConfig: string | null;
  source: string;
}

export interface IntegrationStatus {
  id: string;
  name: string;
  category: string;
  support: SupportLevel;
  supportLabel: string;
  notes: string;
  installed: boolean;
  appId: string | null;
  running: boolean;
  webUrl: string | null;
  protocol: string | null;
  proxy: ProxyInfo | null;
}

export interface Capability {
  id: string;
  label: string;
  available: boolean;
  reason: string;
}

export interface CustomApp {
  id: string;
  name: string;
  kind: "custom" | "web";
  target: string;
  args: string;
  icon: string | null;
  adapter: string;
  catalogId: string | null;
  features: Record<string, unknown>;
  notificationsEnabled: boolean;
  showCard: boolean;
  favorite: boolean;
  createdAt: number;
}

export interface WebService {
  id: string;
  name: string;
  url: string;
  builtin: boolean;
  appId: string | null;
  open: boolean;
}

export interface PluginManifest {
  id: string;
  name: string;
  version: string;
  apiVersion: number;
  minAppVersion: string | null;
  description: string;
  author: string;
  entry: string;
  icon: string | null;
  permissions: string[];
  card: { title: string; height: number } | null;
  settings: { key: string; type: "string" | "number" | "boolean" | "select"; label: string; default: unknown; options: string[] }[];
  apps: { name: string; url: string }[];
  allowedHosts: string[];
}

export interface PluginInfo {
  manifest: PluginManifest;
  enabled: boolean;
  granted: string[];
  path: string;
  installedAt: number;
  updatedAt: number;
  missing: boolean;
  pendingPermissions: string[];
  builtin: boolean;
}

export interface ProfileRow {
  id: string;
  name: string;
  builtin: boolean;
  data: Record<string, unknown>;
  updatedAt: number;
}

export interface HotkeyStatus {
  action: string;
  label: string;
  accelerator: string;
  enabled: boolean;
  registered: boolean;
  error: string | null;
}

export interface LogEntry {
  id: number;
  ts: number;
  level: string;
  category: string;
  source: string;
  message: string;
}

export interface MonitorInfo {
  key: string;
  name: string;
  width: number;
  height: number;
  x: number;
  y: number;
  scale: number;
  primary: boolean;
  current: boolean;
}

export interface ListenerStatus {
  available: boolean;
  active: boolean;
  backend: string;
  message: string;
}

export interface MemoryReport {
  total: number;
  processes: { pid: number; name: string; memory: number }[];
  note: string;
}

export interface AboutInfo {
  version: string;
  os: string;
  arch: string;
  dataDir: string;
  logsDir: string;
  db: DbStatus;
  autostart: boolean;
  pluginApi: number;
}

export interface ForegroundInfo {
  pid: number;
  exeName: string;
  exePath: string;
  fullscreen: boolean;
  exclusiveFullscreen: boolean;
  isShell: boolean;
}

export interface Note {
  id: string;
  text: string;
  color: string;
  pinned: boolean;
  createdAt: number;
  updatedAt: number;
  remindAt: number | null;
  reminded: boolean;
  audio: string | null;
  audioMs: number;
}

export interface ShelfItem {
  id: number;
  path: string;
  name: string;
  isDir: boolean;
  size: number;
  addedAt: number;
  exists: boolean;
}

export interface UpdateInfo {
  current: string;
  latest: string | null;
  available: boolean;
  notes: string;
  pageUrl: string;
  method: "installer" | "appimage" | "page";
  checkedAt: number;
}

export interface PhoneDevice {
  id: string;
  name: string;
  kind: string;
  paired: boolean;
  reachable: boolean;
  pairing: "none" | "requested" | "incoming";
  battery: number | null;
  charging: boolean;
}

export interface PhoneStatus {
  enabled: boolean;
  available: boolean;
  message: string;
  deviceName: string;
  devices: PhoneDevice[];
}

export interface CalEvent {
  uid: string;
  title: string;
  start: number;
  end: number;
  allDay: boolean;
  location: string;
}

export interface PomodoroState {
  phase: "idle" | "focus" | "short" | "long";
  cycle: number;
  cycles: number;
  startedAt: number;
  endsAt: number;
  pausedLeftMs: number | null;
  today: number;
  week: number[];
}

export interface AppVolume {
  id: string;
  name: string;
  level: number;
  muted: boolean;
}

export interface Headphone {
  id: string;
  name: string;
  connected: boolean;
  battery: number | null;
}

export interface SpeechStatus {
  library: boolean;
  models: string[];
  installing: string | null;
}
