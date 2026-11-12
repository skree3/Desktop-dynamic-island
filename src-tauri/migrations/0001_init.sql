-- Desktop Island: начальная схема базы данных.
-- Храним только настройки, пользовательские данные и ограниченную историю.

CREATE TABLE IF NOT EXISTS settings (
    key         TEXT PRIMARY KEY,
    value       TEXT NOT NULL,
    updated_at  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS widget_settings (
    card_id     TEXT PRIMARY KEY,
    enabled     INTEGER NOT NULL DEFAULT 1,
    sort_order  INTEGER NOT NULL DEFAULT 0,
    priority    INTEGER NOT NULL DEFAULT 1,
    config      TEXT NOT NULL DEFAULT '{}'
);

CREATE TABLE IF NOT EXISTS themes (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    builtin     INTEGER NOT NULL DEFAULT 0,
    data        TEXT NOT NULL,
    created_at  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS profiles (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    builtin     INTEGER NOT NULL DEFAULT 0,
    data        TEXT NOT NULL,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS hotkeys (
    action      TEXT PRIMARY KEY,
    accelerator TEXT NOT NULL,
    enabled     INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS plugins (
    id                  TEXT PRIMARY KEY,
    name                TEXT NOT NULL,
    version             TEXT NOT NULL,
    enabled             INTEGER NOT NULL DEFAULT 0,
    path                TEXT NOT NULL,
    manifest            TEXT NOT NULL,
    granted_permissions TEXT NOT NULL DEFAULT '[]',
    installed_at        INTEGER NOT NULL,
    updated_at          INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS plugin_settings (
    plugin_id   TEXT NOT NULL,
    key         TEXT NOT NULL,
    value       TEXT NOT NULL,
    PRIMARY KEY (plugin_id, key),
    FOREIGN KEY (plugin_id) REFERENCES plugins(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS application_integrations (
    id                    TEXT PRIMARY KEY,
    name                  TEXT NOT NULL,
    kind                  TEXT NOT NULL,          -- custom | web | plugin
    target                TEXT NOT NULL,          -- путь к exe, AUMID или URL
    args                  TEXT NOT NULL DEFAULT '',
    icon                  TEXT,
    adapter               TEXT NOT NULL DEFAULT 'launch',
    catalog_id            TEXT,
    features              TEXT NOT NULL DEFAULT '{}',
    notifications_enabled INTEGER NOT NULL DEFAULT 1,
    show_card             INTEGER NOT NULL DEFAULT 1,
    favorite              INTEGER NOT NULL DEFAULT 0,
    created_at            INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS clipboard_history (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    kind        TEXT NOT NULL,                  -- text | image | files
    content     TEXT NOT NULL,                  -- текст или JSON-список файлов
    preview     TEXT NOT NULL,
    image       BLOB,                           -- PNG для изображений
    hash        TEXT NOT NULL UNIQUE,
    source_app  TEXT,
    pinned      INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_clipboard_created ON clipboard_history(created_at DESC);

CREATE TABLE IF NOT EXISTS timers (
    id          TEXT PRIMARY KEY,
    label       TEXT NOT NULL,
    kind        TEXT NOT NULL,                  -- countdown | stopwatch
    duration_ms INTEGER NOT NULL DEFAULT 0,
    elapsed_ms  INTEGER NOT NULL DEFAULT 0,     -- накопленное время до последнего запуска
    started_at  INTEGER,                        -- unix ms, NULL если не запущен
    state       TEXT NOT NULL,                  -- idle | running | paused | finished
    created_at  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS event_log (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    ts          INTEGER NOT NULL,
    level       TEXT NOT NULL,
    category    TEXT NOT NULL,
    source      TEXT NOT NULL,
    message     TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_event_log_ts ON event_log(ts DESC);
