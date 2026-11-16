// Типизированные обёртки над IPC Tauri. Весь доступ к бэкенду — только здесь.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type * as T from "../types";

export function errorMessage(e: unknown): string {
  if (!e) return "Неизвестная ошибка";
  if (typeof e === "string") return e;
  if (typeof e === "object" && "message" in (e as any)) return String((e as any).message);
  return String(e);
}

export function errorKind(e: unknown): T.AppError["kind"] | null {
  if (e && typeof e === "object" && "kind" in (e as any)) return (e as any).kind;
  return null;
}

const call = <R>(cmd: string, args?: Record<string, unknown>) => invoke<R>(cmd, args);

export const ipc = {
  // приложение и окна
  bootstrap: () => call<T.Bootstrap>("app_bootstrap"),
  setTimezone: (offsetMinutes: number) => call<void>("app_set_timezone", { offsetMinutes }),
  setActiveCards: (cards: string[], expanded: boolean) => call<void>("ui_set_active_cards", { cards, expanded }),
  islandResize: (width: number, height: number) => call<void>("island_resize", { width, height }),
  islandHitbox: (width: number, height: number) => call<void>("island_hitbox", { width, height }),
  islandReady: () => call<void>("island_ready"),
  islandToggle: () => call<void>("island_toggle"),
  islandHideTemporarily: (minutes: number) => call<void>("island_hide_temporarily", { minutes }),
  islandResetPosition: () => call<void>("island_reset_position"),
  islandCenter: () => call<void>("island_center"),
  islandAttached: () => call<boolean>("island_attached"),
  monitors: () => call<T.MonitorInfo[]>("monitors_list"),
  moveToMonitor: (key: string | null) => call<void>("island_move_to_monitor", { key }),
  openSettings: (section?: string) => call<void>("open_settings_window", { section: section ?? null }),
  quit: () => call<void>("app_quit"),
  memory: () => call<T.MemoryReport>("perf_memory"),
  environment: () => call<T.EnvironmentInfo>("environment_info"),
  about: () => call<T.AboutInfo>("about_info"),
  openUrl: (url: string) => call<void>("open_url", { url }),
  openPath: (kind: "data" | "logs" | "plugins") => call<void>("open_path", { kind }),
  pickPath: (kind: "exe" | "icon" | "plugin") => call<string | null>("pick_path", { kind }),

  // настройки
  settings: () => call<T.Settings>("settings_get"),
  updateSettings: <K extends T.SettingsSection>(section: K, value: Partial<T.Settings[K]>) =>
    call<T.Settings>("settings_update", { section, value }),
  resetSettings: (section: T.SettingsSection) => call<T.Settings>("settings_reset", { section }),
  cards: () => call<T.CardInfo[]>("cards_list"),
  updateCards: (cards: { cardId: string; enabled: boolean; sortOrder: number; priority: number; config?: unknown }[]) =>
    call<T.CardInfo[]>("cards_update", { cards: cards.map((c) => ({ config: {}, ...c })) }),
  profiles: () => call<T.ProfileRow[]>("profiles_list"),
  profileCreate: (name: string) => call<T.ProfileRow>("profile_create", { name }),
  profileSaveCurrent: (id: string) => call<void>("profile_save_current", { id }),
  profileRename: (id: string, name: string) => call<void>("profile_rename", { id, name }),
  profileApply: (id: string) => call<void>("profile_apply", { id }),
  profileDelete: (id: string) => call<void>("profile_delete", { id }),
  themes: () => call<T.ThemeRow[]>("themes_list"),
  themeDelete: (id: string) => call<void>("theme_delete", { id }),
  themeSave: (json: string) => call<T.ThemeRow>("theme_save", { json }),
  themeShareCode: (id: string) => call<string>("theme_share_code", { id }),
  themePreview: (json: string | null, expand = false) => call<void>("theme_preview", { json, expand }),
  themeImport: (text: string) => call<T.ThemeRow>("theme_import", { text }),
  themeExportFile: (id: string) => call<string | null>("theme_export_file", { id }),
  themeImportFile: () => call<T.ThemeRow | null>("theme_import_file"),
  wallpaperPick: () => call<string | null>("wallpaper_pick"),
  wallpaperClear: () => call<void>("wallpaper_clear"),
  wallpaperData: () => call<string | null>("wallpaper_data"),
  fontsList: () => call<T.FontFile[]>("fonts_list"),
  fontPick: () => call<T.FontFile | null>("font_pick"),
  fontData: (file: string) => call<string>("font_data", { file }),
  fontRemove: (file: string) => call<void>("font_remove", { file }),
  soundPick: (kind: "notification" | "timer") => call<string | null>("sound_pick", { kind }),
  soundData: (file: string) => call<string>("sound_data", { file }),
  kbdState: () => call<T.KbdState>("kbd_state"),
  kbdNextLayout: () => call<void>("kbd_next_layout"),
  kbdCapsToggle: () => call<void>("kbd_caps_toggle"),
  airplaneSet: (on: boolean) => call<void>("airplane_set", { on }),
  vpnStatus: () => call<T.VpnStatus>("vpn_status"),
  vpnSet: (on: boolean) => call<T.VpnStatus>("vpn_set", { on }),
  vpnPickPath: () => call<string | null>("vpn_pick_path"),
  hotkeys: () => call<T.HotkeyStatus[]>("hotkeys_list"),
  hotkeySet: (action: string, accelerator: string, enabled: boolean) =>
    call<T.HotkeyStatus[]>("hotkey_set", { action, accelerator, enabled }),
  hotkeysReset: () => call<T.HotkeyStatus[]>("hotkeys_reset"),
  hotkeysSuspend: (suspended: boolean) => call<void>("hotkeys_suspend", { suspended }),
  logs: (limit = 300, category?: string | null, level?: string | null) =>
    call<T.LogEntry[]>("logs_list", { limit, category: category ?? null, level: level ?? null }),
  logsClear: () => call<void>("logs_clear"),

  // уведомления
  notificationsState: () => call<T.NotificationPayload>("notifications_state"),
  notificationsHistory: () => call<T.IslandNotification[]>("notifications_history"),
  notificationsClearHistory: () => call<void>("notifications_clear_history"),
  notificationRemove: (id: string) => call<void>("notification_remove", { id }),
  notificationDismiss: (id: string) => call<void>("notification_dismiss", { id }),
  notificationHold: (id: string, hold: boolean) => call<void>("notification_hold", { id, hold }),
  notificationAction: (id: string, action: string) => call<void>("notification_action", { id, action }),
  listenerStatus: () => call<T.ListenerStatus>("notifications_listener_status"),
  listenerRetry: () => call<void>("notifications_retry"),
  notificationTest: (priority?: T.Priority) => call<void>("notifications_test", { priority: priority ?? null }),

  // музыка
  mediaSessions: () => call<T.MediaSession[]>("media_sessions"),
  mediaControl: (id: string, action: T.MediaAction) => call<void>("media_control", { id, action }),
  mediaArtwork: (id: string) => call<string | null>("media_artwork", { id }),
  mediaSelect: (id: string | null) => call<void>("media_select", { id }),

  // система
  sysmon: () => call<T.SysSnapshot | null>("sysmon_snapshot"),
  bluetooth: () => call<T.BluetoothState>("bluetooth_state"),
  bluetoothSetPowered: (on: boolean) => call<void>("bluetooth_set_powered", { on }),
  quick: () => call<T.QuickState>("quick_state"),
  wifiSet: (on: boolean) => call<void>("wifi_set", { on }),
  energySaverSet: (on: boolean) => call<void>("energy_saver_set", { on }),
  openSystemSettings: (page: string) => call<void>("open_system_settings", { page }),
  volume: () => call<T.VolumeState>("volume_state"),
  volumeSet: (level: number) => call<void>("volume_set", { level }),
  volumeMute: (muted: boolean) => call<void>("volume_mute", { muted }),
  volumeSetDevice: (id: string) => call<void>("volume_set_device", { id }),
  brightness: () => call<T.BrightnessState>("brightness_state"),
  brightnessSet: (id: string, value: number) => call<void>("brightness_set", { id, value }),
  powerRequest: (action: "shutdown" | "reboot") => call<string>("power_request", { action }),
  powerExecute: (action: "shutdown" | "reboot", token: string) => call<void>("power_execute", { action, token }),

  // таймеры
  timers: () => call<T.Timer[]>("timers_list"),
  timerCreate: (label: string, kind: "countdown" | "stopwatch", durationMs: number, start: boolean) =>
    call<T.Timer>("timer_create", { label, kind, durationMs, start }),
  timerCommand: (id: string, command: "start" | "pause" | "reset" | "delete" | "rename", label?: string) =>
    call<void>("timer_command", { id, command, label: label ?? null }),

  // буфер обмена
  clipboardList: (query?: string, limit?: number) => call<T.ClipItem[]>("clipboard_list", { query: query ?? null, limit: limit ?? null }),
  clipboardImage: (id: number) => call<string | null>("clipboard_image", { id }),
  clipboardText: (id: number) => call<string>("clipboard_text", { id }),
  clipboardCopy: (id: number) => call<void>("clipboard_copy", { id }),
  clipboardPin: (id: number, pinned: boolean) => call<void>("clipboard_pin", { id, pinned }),
  clipboardDelete: (id: number) => call<void>("clipboard_delete", { id }),
  clipboardClear: (keepPinned: boolean) => call<number>("clipboard_clear", { keepPinned }),
  clipboardSystemHistory: () => call<{ id: string; kind: string; preview: string; createdAt: number; image: string | null }[]>("clipboard_system_history"),
  clipboardSystemItem: (id: string, command: "copy" | "delete") => call<void>("clipboard_system_item", { id, command }),
  clipboardSystemClear: () => call<void>("clipboard_system_clear"),

  // заметки
  notes: () => call<T.Note[]>("notes_list"),
  noteCreate: (text: string, color?: string, extra?: { remindAt?: number | null; audio?: string | null; audioMs?: number | null }) =>
    call<T.Note>("note_create", { text, color: color ?? null, remindAt: extra?.remindAt ?? null, audio: extra?.audio ?? null, audioMs: extra?.audioMs ?? null }),
  noteRemind: (id: string, remindAt: number | null) => call<void>("note_remind", { id, remindAt }),
  voiceStart: () => call<void>("voice_start"),
  voiceStop: () => call<{ name: string; durationMs: number }>("voice_stop"),
  voiceCancel: () => call<void>("voice_cancel"),
  voiceData: (name: string) => call<string>("voice_data", { name }),
  phoneStatus: () => call<T.PhoneStatus>("phone_status"),
  phonePair: (id: string) => call<void>("phone_pair", { id }),
  phoneUnpair: (id: string) => call<void>("phone_unpair", { id }),
  phoneAccept: (id: string, accept: boolean) => call<void>("phone_accept", { id, accept }),
  phoneRing: (id: string) => call<void>("phone_ring", { id }),
  phoneSendFile: (path: string) => call<void>("phone_send_file", { path }),
  calendarEvents: () => call<[T.CalEvent[], string | null]>("calendar_events"),
  calendarRefresh: () => call<void>("calendar_refresh"),
  pomodoroStatus: () => call<T.PomodoroState>("pomodoro_status"),
  pomodoro: (action: "start" | "pause" | "stop" | "skip") => call<void>("pomodoro_command", { action }),
  islandStrip: () => call<boolean>("island_strip"),
  appVolumes: () => call<T.AppVolume[]>("app_volumes"),
  appVolumeSet: (id: string, level: number | null, muted: boolean | null) => call<void>("app_volume_set", { id, level, muted }),
  headphones: () => call<T.Headphone[]>("headphones_list"),
  headphoneConnect: (id: string, connect: boolean) => call<void>("headphone_connect", { id, connect }),
  ocrShelfItem: (id: number) => call<string>("ocr_shelf_item", { id }),
  translate: (text: string) => call<[string, string]>("translate_text", { text }),
  speechStatus: () => call<T.SpeechStatus>("speech_status"),
  speechInstall: (lang: string) => call<void>("speech_install", { lang }),
  voiceTranscribe: (name: string) => call<string>("voice_transcribe", { name }),
  noteUpdate: (id: string, patch: { text?: string; color?: string; pinned?: boolean }) =>
    call<void>("note_update", { id, text: patch.text ?? null, color: patch.color ?? null, pinned: patch.pinned ?? null }),
  noteDelete: (id: string) => call<void>("note_delete", { id }),

  // полка
  shelf: () => call<T.ShelfItem[]>("shelf_list"),
  shelfAdd: (paths: string[]) => call<number>("shelf_add", { paths }),
  shelfCommand: (command: "open" | "reveal" | "remove" | "copy" | "clear", id?: number, ids?: number[]) =>
    call<void>("shelf_command", { command, id: id ?? null, ids: ids ?? null }),
  shelfDragOut: (ids: number[]) => call<void>("shelf_drag_out", { ids }),
  screenshotRegion: () => call<boolean>("screenshot_region"),

  // микрофон, камера, звонки
  privacyState: () => call<T.PrivacyState>("privacy_state"),
  micToggle: () => call<boolean>("mic_toggle"),
  callOpen: () => call<void>("call_open"),

  // тексты песен
  lyrics: (artist: string, title: string, album: string, durationMs: number | null) =>
    call<T.Lyrics | null>("lyrics_get", { artist, title, album, durationMs }),

  // обновления
  updateCheck: () => call<T.UpdateInfo>("update_check"),
  updateLast: () => call<T.UpdateInfo | null>("update_last"),
  updateInstall: () => call<void>("update_install"),

  // игровой режим
  gameMode: () => call<T.GameModeState>("game_mode_state"),
  gameModeSetManual: (manual: boolean | null) => call<void>("game_mode_set_manual", { manual }),
  foreground: () => call<T.ForegroundInfo | null>("game_mode_foreground"),

  // приложения и интеграции
  appsSearch: (query: string, limit?: number) => call<T.AppEntry[]>("apps_search", { query, limit: limit ?? null }),
  appsIcon: (id: string) => call<string | null>("apps_icon", { id }),
  appsLaunch: (id: string) => call<void>("apps_launch", { id }),
  appsLaunchAdmin: (id: string) => call<void>("apps_launch_admin", { id }),
  appsRefresh: () => call<number>("apps_refresh"),
  appsSetFavorite: (id: string, favorite: boolean) => call<void>("apps_set_favorite", { id, favorite }),
  appsSetHidden: (id: string, hidden: boolean) => call<void>("apps_set_hidden", { id, hidden }),
  integrations: () => call<T.IntegrationStatus[]>("integrations_status"),
  capabilities: (kind: string, target: string, catalogId?: string | null) =>
    call<T.Capability[]>("integration_capabilities", { kind, target, catalogId: catalogId ?? null }),
  customApps: () => call<T.CustomApp[]>("custom_apps_list"),
  customAppSave: (input: Record<string, unknown>) => call<T.CustomApp>("custom_app_save", { input }),
  customAppDelete: (id: string) => call<void>("custom_app_delete", { id }),
  aiServices: () => call<T.WebService[]>("ai_services"),
  aiOpen: (id: string) => call<void>("ai_open", { id }),
  aiClose: (id: string) => call<void>("ai_close", { id }),
  aiClearData: (id: string) => call<void>("ai_clear_data", { id }),

  // плагины
  plugins: () => call<T.PluginInfo[]>("plugins_list"),
  pluginPermissions: () => call<[string, string][]>("plugins_permissions_catalog"),
  pluginInstall: (path: string) => call<{ plugin: T.PluginInfo; updated: boolean; previousVersion: string | null }>("plugin_install", { path }),
  pluginSetEnabled: (id: string, enabled: boolean, approvePermissions: boolean) =>
    call<T.PluginInfo>("plugin_set_enabled", { id, enabled, approvePermissions }),
  pluginUninstall: (id: string) => call<void>("plugin_uninstall", { id }),
  pluginSettings: (id: string) => call<Record<string, unknown>>("plugin_settings_get", { id }),
  pluginSettingSet: (id: string, key: string, value: unknown) => call<void>("plugin_setting_set", { id, key, value }),
  pluginCall: (pluginId: string, method: string, params: unknown) => call<unknown>("plugin_call", { pluginId, method, params }),
};

export const events = {
  SETTINGS_CHANGED: "settings://changed",
  NOTIFICATION: "island://notification",
  ACTIVITIES: "island://activities",
  COMMAND: "island://command",
  MEDIA_SESSIONS: "media://sessions",
  SYSMON_SNAPSHOT: "sysmon://snapshot",
  TIMERS_CHANGED: "timers://changed",
  CLIPBOARD_CHANGED: "clipboard://changed",
  GAME_MODE: "game-mode://changed",
  PLUGINS_CHANGED: "plugins://changed",
  PLUGINS_EVENT: "plugins://event",
  THEME_CHANGED: "theme://changed",
  WIDGETS_CHANGED: "widgets://changed",
  NOTIFICATIONS_STATUS: "notifications://status",
  MONITORS_CHANGED: "monitors://changed",
  SETTINGS_NAVIGATE: "settings://navigate",
  KBD: "island://kbd",
} as const;

export function on<P>(event: string, handler: (payload: P) => void): Promise<UnlistenFn> {
  return listen<P>(event, (e) => handler(e.payload));
}

/** URL файла плагина: на Windows кастомные протоколы доступны как http://<scheme>.localhost. */
export function pluginUrl(pluginId: string, file: string): string {
  const isWindows = navigator.userAgent.includes("Windows");
  const base = isWindows ? "http://plugin.localhost" : "plugin://localhost";
  return `${base}/${encodeURIComponent(pluginId)}/${file.split("/").map(encodeURIComponent).join("/")}`;
}
