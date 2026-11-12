//! Полка: файлы и папки, перетащенные на остров. Хранятся ссылки (пути),
//! сами файлы никуда не копируются.

use rusqlite::params;
use serde::Serialize;
use std::path::Path;

use crate::core::error::{AppError, AppResult};
use crate::core::state::Shared;
use crate::database::now_ms;

const MAX_ITEMS: i64 = 100;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShelfItem {
    pub id: i64,
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub added_at: i64,
    pub exists: bool,
}

fn changed(st: &Shared) {
    st.events.emit("shelf://changed", ());
}

pub fn list(st: &Shared) -> AppResult<Vec<ShelfItem>> {
    let conn = st.db.conn();
    let mut stmt = conn.prepare("SELECT id, path, name, is_dir, size, added_at FROM shelf ORDER BY added_at DESC")?;
    let rows = stmt
        .query_map([], |r| {
            let path: String = r.get(1)?;
            Ok(ShelfItem {
                id: r.get(0)?,
                exists: Path::new(&path).exists(),
                path,
                name: r.get(2)?,
                is_dir: r.get::<_, i64>(3)? != 0,
                size: r.get::<_, i64>(4)? as u64,
                added_at: r.get(5)?,
            })
        })?
        .flatten()
        .collect();
    Ok(rows)
}

pub fn add(st: &Shared, paths: &[String]) -> AppResult<usize> {
    let mut added = 0;
    for p in paths.iter().take(50) {
        let path = Path::new(p);
        let Ok(meta) = std::fs::metadata(path) else { continue };
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| p.clone());
        let count: i64 = st.db.conn().query_row("SELECT count(*) FROM shelf", [], |r| r.get(0))?;
        if count >= MAX_ITEMS {
            // Полка переполнена — убираем самый старый элемент.
            st.db.conn().execute("DELETE FROM shelf WHERE id = (SELECT id FROM shelf ORDER BY added_at ASC LIMIT 1)", [])?;
        }
        st.db.conn().execute(
            "INSERT INTO shelf(path, name, is_dir, size, added_at) VALUES(?1,?2,?3,?4,?5)
             ON CONFLICT(path) DO UPDATE SET added_at = excluded.added_at, size = excluded.size",
            params![p, name, meta.is_dir() as i64, if meta.is_dir() { 0 } else { meta.len() as i64 }, now_ms()],
        )?;
        added += 1;
    }
    if added > 0 {
        changed(st);
    }
    Ok(added)
}

fn path_of(st: &Shared, id: i64) -> AppResult<String> {
    st.db
        .conn()
        .query_row("SELECT path FROM shelf WHERE id = ?1", params![id], |r| r.get(0))
        .map_err(|_| AppError::NotFound("элемент не найден".into()))
}

pub fn remove(st: &Shared, id: i64) -> AppResult<()> {
    st.db.conn().execute("DELETE FROM shelf WHERE id = ?1", params![id])?;
    changed(st);
    Ok(())
}

pub fn clear(st: &Shared) -> AppResult<()> {
    st.db.conn().execute("DELETE FROM shelf", [])?;
    changed(st);
    Ok(())
}

pub fn open(st: &Shared, id: i64) -> AppResult<()> {
    let p = path_of(st, id)?;
    if !Path::new(&p).exists() {
        return Err(AppError::NotFound("файл перемещён или удалён".into()));
    }
    crate::platform::os::shell::open_uri(&p)
}

/// Показывает файл в проводнике / файловом менеджере.
pub fn reveal(st: &Shared, id: i64) -> AppResult<()> {
    let p = path_of(st, id)?;
    #[cfg(windows)]
    {
        std::process::Command::new("explorer.exe").arg(format!("/select,{p}")).spawn().map_err(|e| AppError::system(e.to_string()))?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let parent = Path::new(&p).parent().map(|d| d.to_string_lossy().to_string()).unwrap_or(p);
        crate::platform::os::shell::open_uri(&parent)
    }
}

/// Кладёт файл в буфер обмена — его можно вставить в проводнике или в чат.
pub fn copy(st: &Shared, ids: &[i64]) -> AppResult<()> {
    let mut paths = vec![];
    for id in ids {
        let p = path_of(st, *id)?;
        if Path::new(&p).exists() {
            paths.push(p);
        }
    }
    if paths.is_empty() {
        return Err(AppError::NotFound("файлы не найдены".into()));
    }
    crate::util::clipboard::with(|cb| cb.set().file_list(&paths))?.map_err(|e| AppError::system(e.to_string()))
}
