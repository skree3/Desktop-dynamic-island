-- Заметки и полка файлов.
CREATE TABLE IF NOT EXISTS notes (
    id          TEXT PRIMARY KEY,
    text        TEXT NOT NULL,
    color       TEXT NOT NULL DEFAULT 'default',
    pinned      INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_notes_updated ON notes(updated_at DESC);

CREATE TABLE IF NOT EXISTS shelf (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    path        TEXT NOT NULL UNIQUE,
    name        TEXT NOT NULL,
    is_dir      INTEGER NOT NULL DEFAULT 0,
    size        INTEGER NOT NULL DEFAULT 0,
    added_at    INTEGER NOT NULL
);
