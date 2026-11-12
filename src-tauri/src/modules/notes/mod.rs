//! Заметки: короткие записи прямо в острове. Хранятся локально в SQLite.

use rusqlite::params;
use serde::Serialize;

use crate::core::error::{AppError, AppResult};
use crate::core::state::Shared;
use crate::database::now_ms;

pub const COLORS: &[&str] = &["default", "yellow", "green", "blue", "pink", "purple"];
const MAX_NOTES: i64 = 500;
const MAX_LEN: usize = 10_000;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Note {
    pub id: String,
    pub text: String,
    pub color: String,
    pub pinned: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

fn changed(st: &Shared) {
    st.events.emit("notes://changed", ());
}

pub fn list(st: &Shared) -> AppResult<Vec<Note>> {
    let conn = st.db.conn();
    let mut stmt = conn.prepare("SELECT id, text, color, pinned, created_at, updated_at FROM notes ORDER BY pinned DESC, updated_at DESC")?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Note {
                id: r.get(0)?,
                text: r.get(1)?,
                color: r.get(2)?,
                pinned: r.get::<_, i64>(3)? != 0,
                created_at: r.get(4)?,
                updated_at: r.get(5)?,
            })
        })?
        .flatten()
        .collect();
    Ok(rows)
}

fn clean(text: &str) -> AppResult<String> {
    let t: String = text.chars().take(MAX_LEN).collect();
    if t.trim().is_empty() {
        return Err(AppError::invalid("заметка пустая"));
    }
    Ok(t)
}

fn valid_color(c: &str) -> String {
    if COLORS.contains(&c) { c.to_string() } else { "default".into() }
}

pub fn create(st: &Shared, text: &str, color: &str) -> AppResult<Note> {
    let count: i64 = st.db.conn().query_row("SELECT count(*) FROM notes", [], |r| r.get(0))?;
    if count >= MAX_NOTES {
        return Err(AppError::invalid("слишком много заметок (максимум 500)"));
    }
    let now = now_ms();
    let note = Note { id: uuid::Uuid::new_v4().to_string(), text: clean(text)?, color: valid_color(color), pinned: false, created_at: now, updated_at: now };
    st.db.conn().execute(
        "INSERT INTO notes(id, text, color, pinned, created_at, updated_at) VALUES(?1,?2,?3,0,?4,?4)",
        params![note.id, note.text, note.color, now],
    )?;
    changed(st);
    Ok(note)
}

pub fn update(st: &Shared, id: &str, text: Option<&str>, color: Option<&str>, pinned: Option<bool>) -> AppResult<()> {
    let n = st.db.conn().execute(
        "UPDATE notes SET text = COALESCE(?2, text), color = COALESCE(?3, color), pinned = COALESCE(?4, pinned), updated_at = ?5 WHERE id = ?1",
        params![id, text.map(clean).transpose()?, color.map(valid_color), pinned.map(|p| p as i64), now_ms()],
    )?;
    if n == 0 {
        return Err(AppError::NotFound("заметка не найдена".into()));
    }
    changed(st);
    Ok(())
}

pub fn delete(st: &Shared, id: &str) -> AppResult<()> {
    st.db.conn().execute("DELETE FROM notes WHERE id = ?1", params![id])?;
    changed(st);
    Ok(())
}
