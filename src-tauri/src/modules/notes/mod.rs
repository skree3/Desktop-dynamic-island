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
    /// Время напоминания (мс), если заметка — напоминание.
    pub remind_at: Option<i64>,
    pub reminded: bool,
    /// Файл голосовой заметки (в папке voice) и её длительность.
    pub audio: Option<String>,
    pub audio_ms: i64,
}

const COLS: &str = "id, text, color, pinned, created_at, updated_at, remind_at, reminded, audio, audio_ms";

fn row(r: &rusqlite::Row) -> rusqlite::Result<Note> {
    Ok(Note {
        id: r.get(0)?,
        text: r.get(1)?,
        color: r.get(2)?,
        pinned: r.get::<_, i64>(3)? != 0,
        created_at: r.get(4)?,
        updated_at: r.get(5)?,
        remind_at: r.get(6)?,
        reminded: r.get::<_, i64>(7)? != 0,
        audio: r.get(8)?,
        audio_ms: r.get(9)?,
    })
}

fn changed(st: &Shared) {
    st.events.emit("notes://changed", ());
}

pub fn list(st: &Shared) -> AppResult<Vec<Note>> {
    let conn = st.db.conn();
    let mut stmt = conn.prepare(&format!("SELECT {COLS} FROM notes ORDER BY pinned DESC, (remind_at IS NULL OR reminded), remind_at, updated_at DESC"))?;
    let rows = stmt.query_map([], row)?.flatten().collect();
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

pub fn create(st: &Shared, text: &str, color: &str, remind_at: Option<i64>, audio: Option<(String, i64)>) -> AppResult<Note> {
    let count: i64 = st.db.conn().query_row("SELECT count(*) FROM notes", [], |r| r.get(0))?;
    if count >= MAX_NOTES {
        return Err(AppError::invalid("слишком много заметок (максимум 500)"));
    }
    if let Some((name, _)) = &audio {
        if !crate::modules::voice::valid_name(name) {
            return Err(AppError::invalid("неверный файл записи"));
        }
    }
    let now = now_ms();
    let text = if text.trim().is_empty() && audio.is_some() { "Голосовая заметка".to_string() } else { clean(text)? };
    let remind_at = remind_at.filter(|t| *t > now - 60_000 && *t < now + 400 * 86_400_000);
    let note = Note {
        id: uuid::Uuid::new_v4().to_string(),
        text,
        color: valid_color(color),
        pinned: false,
        created_at: now,
        updated_at: now,
        remind_at,
        reminded: false,
        audio: audio.as_ref().map(|a| a.0.clone()),
        audio_ms: audio.as_ref().map(|a| a.1).unwrap_or(0),
    };
    st.db.conn().execute(
        "INSERT INTO notes(id, text, color, pinned, created_at, updated_at, remind_at, reminded, audio, audio_ms) VALUES(?1,?2,?3,0,?4,?4,?5,0,?6,?7)",
        params![note.id, note.text, note.color, now, note.remind_at, note.audio, note.audio_ms],
    )?;
    changed(st);
    Ok(note)
}

/// Отложить напоминание или убрать его.
pub fn set_reminder(st: &Shared, id: &str, remind_at: Option<i64>) -> AppResult<()> {
    st.db.conn().execute("UPDATE notes SET remind_at = ?2, reminded = 0, updated_at = ?3 WHERE id = ?1", params![id, remind_at, now_ms()])?;
    changed(st);
    Ok(())
}

fn get(st: &Shared, id: &str) -> Option<Note> {
    st.db.conn().query_row(&format!("SELECT {COLS} FROM notes WHERE id = ?1"), params![id], row).ok()
}

/// Раз в 10 секунд проверяет наступившие напоминания и показывает их в острове.
pub fn start_reminders(st: &Shared) {
    let st = st.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            let due: Vec<Note> = {
                let conn = st.db.conn();
                let Ok(mut stmt) = conn.prepare(&format!("SELECT {COLS} FROM notes WHERE reminded = 0 AND remind_at IS NOT NULL AND remind_at <= ?1")) else { continue };
                stmt.query_map(params![now_ms()], row).map(|r| r.flatten().collect()).unwrap_or_default()
            };
            for n in due {
                let _ = st.db.conn().execute("UPDATE notes SET reminded = 1 WHERE id = ?1", params![n.id]);
                let mut actions = vec![];
                if n.audio.is_some() {
                    actions.push(crate::events::NotificationAction { id: "play".into(), label: "Прослушать".into() });
                }
                actions.push(crate::events::NotificationAction { id: "snooze".into(), label: "Через 10 мин".into() });
                actions.push(crate::events::NotificationAction { id: "done".into(), label: "Готово".into() });
                let mut notif = crate::events::internal_notification(&format!("note:{}", n.id), "Напоминание", n.text.chars().take(200).collect::<String>(), String::new(), crate::events::Priority::High);
                notif.actions = actions;
                notif.duration_ms = 20_000;
                st.events.push(notif);
                if let Some(a) = &n.audio {
                    st.events.emit("voice://play", a.clone());
                }
                changed(&st);
            }
        }
    });
}

/// Кнопки напоминания в острове.
pub fn reminder_action(st: &Shared, source: &str, action: &str) -> AppResult<()> {
    let id = source.strip_prefix("note:").unwrap_or_default();
    match action {
        "snooze" => set_reminder(st, id, Some(now_ms() + 10 * 60_000)),
        "play" => {
            if let Some(a) = get(st, id).and_then(|n| n.audio) {
                st.events.emit("voice://play", a);
            }
            Ok(())
        }
        "done" => Ok(()),
        _ => Err(AppError::unsupported("действие не поддерживается")),
    }
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
    if let Some(a) = get(st, id).and_then(|n| n.audio) {
        crate::modules::voice::remove(st, &a);
    }
    st.db.conn().execute("DELETE FROM notes WHERE id = ?1", params![id])?;
    changed(st);
    Ok(())
}
