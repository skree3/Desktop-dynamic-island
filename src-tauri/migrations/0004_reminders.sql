-- Напоминания и голосовые заметки.
ALTER TABLE notes ADD COLUMN remind_at INTEGER;
ALTER TABLE notes ADD COLUMN reminded INTEGER NOT NULL DEFAULT 0;
ALTER TABLE notes ADD COLUMN audio TEXT;
ALTER TABLE notes ADD COLUMN audio_ms INTEGER NOT NULL DEFAULT 0;
CREATE INDEX IF NOT EXISTS idx_notes_remind ON notes(reminded, remind_at);
