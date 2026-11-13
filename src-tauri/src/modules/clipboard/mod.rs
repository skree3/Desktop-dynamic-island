//! Менеджер буфера обмена: история текста, изображений и файлов, поиск,
//! закрепление, лимит записей, исключения по приложениям.
//!
//! Безопасность:
//! * данные с маркерами менеджеров паролей (Windows) не сохраняются;
//! * эвристический фильтр отбрасывает строки, похожие на пароли, токены и номера карт;
//! * приложения из списка исключений игнорируются;
//! * историю можно полностью отключить — тогда фоновый опрос не запускается.
//! Wayland: фоновое чтение возможно только при поддержке data-control (см. README).

pub mod sensitive;

use rusqlite::params;
use serde::Serialize;
use std::hash::{Hash, Hasher};
use std::sync::Mutex;
use std::time::Duration;
use tauri::AppHandle;

use crate::core::error::{AppError, AppResult};
use crate::core::state::Shared;
use crate::core::task::TaskSlot;
use crate::database::{now_ms, Database};
use crate::events::names;
use crate::platform::os;
use crate::util::image::{downscale_rgba, encode_png_rgba, bytes_data_url};

const MAX_TEXT: usize = 100_000;
const MAX_IMAGE_SIDE: u32 = 1920;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipItem {
    pub id: i64,
    pub kind: String,
    pub preview: String,
    pub source_app: Option<String>,
    pub pinned: bool,
    pub created_at: i64,
}

#[derive(Default)]
pub struct ClipboardModule {
    task: TaskSlot,
    /// Хеш последнего записанного нами самими содержимого (чтобы не дублировать).
    own_hash: Mutex<Option<String>>,
}

enum Captured {
    Text(String),
    Image { w: u32, h: u32, rgba: Vec<u8> },
    Files(Vec<String>),
}

fn hash_of(kind: &str, data: &[u8]) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    kind.hash(&mut h);
    data.hash(&mut h);
    format!("{:016x}{:08x}", h.finish(), data.len())
}

fn read_clipboard(store_images: bool, store_files: bool) -> Option<Captured> {
    crate::util::clipboard::with(|cb| read_from(cb, store_images, store_files)).ok().flatten()
}

fn read_from(cb: &mut arboard::Clipboard, store_images: bool, store_files: bool) -> Option<Captured> {
    if store_files {
        if let Ok(files) = cb.get().file_list() {
            if !files.is_empty() {
                return Some(Captured::Files(files.into_iter().map(|p| p.to_string_lossy().to_string()).collect()));
            }
        }
    }
    if let Ok(text) = cb.get_text() {
        if !text.trim().is_empty() {
            return Some(Captured::Text(text));
        }
    }
    if store_images {
        if let Ok(img) = cb.get_image() {
            return Some(Captured::Image { w: img.width as u32, h: img.height as u32, rgba: img.bytes.into_owned() });
        }
    }
    None
}

impl ClipboardModule {
    pub fn set_running(&self, app: &AppHandle, st: &Shared, run: bool) {
        if run == self.task.is_running() {
            return;
        }
        if !run {
            self.task.stop();
            return;
        }
        let st = st.clone();
        let _ = app;
        self.task.start(async move {
            let mut last_seq: Option<u64> = None;
            let mut last_hash: Option<String> = None;
            loop {
                // Windows: дешёвая проверка номера последовательности; Linux: сравнение хеша.
                let seq = os::clipboard::sequence();
                let poll = match (seq, last_seq) {
                    (Some(a), Some(b)) => a != b,
                    _ => true,
                };
                if seq.is_some() {
                    last_seq = seq;
                }
                if poll {
                    let st2 = st.clone();
                    let prev = last_hash.clone();
                    if let Ok(Some(h)) = tauri::async_runtime::spawn_blocking(move || st2.clipboard.capture(&st2, prev)).await {
                        last_hash = Some(h);
                    }
                }
                tokio::time::sleep(Duration::from_millis(if seq.is_some() { 600 } else { 1200 })).await;
            }
        });
    }

    /// Читает буфер и сохраняет запись. Возвращает хеш содержимого.
    fn capture(&self, st: &Shared, prev_hash: Option<String>) -> Option<String> {
        let cfg = st.with_settings(|s| s.clipboard.clone());
        if !cfg.history_enabled {
            return None;
        }
        if cfg.ignore_sensitive && os::clipboard::is_sensitive() {
            return None;
        }
        let owner = os::clipboard::owner_app();
        if let Some(o) = &owner {
            if cfg.excluded_apps.iter().any(|e| e.to_lowercase() == *o) {
                return None;
            }
        }
        let captured = read_clipboard(cfg.store_images, cfg.store_files)?;
        let (kind, content, preview, image, hash) = match captured {
            Captured::Text(t) => {
                if cfg.ignore_sensitive && sensitive::looks_sensitive(&t) {
                    return None;
                }
                let t: String = t.chars().take(MAX_TEXT).collect();
                // Общий буфер с телефоном (если он сопряжён и функция включена).
                st.phone.clipboard_from_pc(st, &t);
                let hash = hash_of("text", t.as_bytes());
                let preview: String = t.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(200).collect();
                ("text", t, preview, None, hash)
            }
            Captured::Files(f) => {
                let json = serde_json::to_string(&f).ok()?;
                let hash = hash_of("files", json.as_bytes());
                let names: Vec<String> = f.iter().map(|p| std::path::Path::new(p).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()).collect();
                let preview = if names.len() == 1 { names[0].clone() } else { format!("{} файлов: {}", names.len(), names.join(", ")) };
                ("files", json, preview.chars().take(200).collect(), None, hash)
            }
            Captured::Image { w, h, rgba } => {
                let hash = hash_of("image", &rgba[..rgba.len().min(65536)]) + &format!("{w}x{h}");
                if Some(&hash) == prev_hash.as_ref() {
                    return Some(hash);
                }
                let (nw, nh, data) = downscale_rgba(w, h, &rgba, MAX_IMAGE_SIDE);
                let png = encode_png_rgba(nw, nh, &data)?;
                ("image", String::new(), format!("Изображение {w}×{h}"), Some(png), hash)
            }
        };
        if Some(&hash) == prev_hash.as_ref() {
            return Some(hash);
        }
        if self.own_hash.lock().ok().and_then(|g| g.clone()).as_ref() == Some(&hash) {
            return Some(hash);
        }
        let res = st.db.conn().execute(
            "INSERT INTO clipboard_history(kind, content, preview, image, hash, source_app, pinned, created_at)
             VALUES(?1,?2,?3,?4,?5,?6,0,?7)
             ON CONFLICT(hash) DO UPDATE SET created_at = excluded.created_at, source_app = excluded.source_app",
            params![kind, content, preview, image, hash, owner, now_ms()],
        );
        match res {
            Ok(_) => {
                let _ = self.trim(&st.db, cfg.max_items);
                st.events.emit(names::CLIPBOARD_CHANGED, ());
            }
            Err(e) => st.log.error(crate::logging::category::MODULE, "clipboard", format!("ошибка записи истории: {e}")),
        }
        Some(hash)
    }

    pub fn trim(&self, db: &Database, max: u32) -> AppResult<()> {
        db.conn().execute(
            "DELETE FROM clipboard_history WHERE pinned = 0 AND id NOT IN
             (SELECT id FROM clipboard_history WHERE pinned = 0 ORDER BY created_at DESC LIMIT ?1)",
            params![max],
        )?;
        Ok(())
    }

    pub fn list(&self, db: &Database, query: Option<&str>, limit: u32) -> AppResult<Vec<ClipItem>> {
        let conn = db.conn();
        let pattern = query.filter(|q| !q.trim().is_empty()).map(|q| format!("%{}%", q.trim().replace('%', "\\%").replace('_', "\\_")));
        let mut stmt = conn.prepare(
            "SELECT id, kind, preview, source_app, pinned, created_at FROM clipboard_history
             WHERE (?1 IS NULL OR preview LIKE ?1 ESCAPE '\\' OR (kind = 'text' AND content LIKE ?1 ESCAPE '\\'))
             ORDER BY pinned DESC, created_at DESC LIMIT ?2",
        )?;
        let rows = stmt
            .query_map(params![pattern, limit.min(1000)], |r| {
                Ok(ClipItem {
                    id: r.get(0)?,
                    kind: r.get(1)?,
                    preview: r.get(2)?,
                    source_app: r.get(3)?,
                    pinned: r.get::<_, i64>(4)? != 0,
                    created_at: r.get(5)?,
                })
            })?
            .flatten()
            .collect();
        Ok(rows)
    }

    pub fn image(&self, db: &Database, id: i64) -> AppResult<Option<String>> {
        let blob: Option<Vec<u8>> = db
            .conn()
            .query_row("SELECT image FROM clipboard_history WHERE id = ?1 AND kind = 'image'", params![id], |r| r.get(0))
            .map_err(|_| AppError::NotFound("запись не найдена".into()))?;
        Ok(blob.map(|b| bytes_data_url("image/png", &b)))
    }

    pub fn text(&self, db: &Database, id: i64) -> AppResult<String> {
        db.conn()
            .query_row("SELECT content FROM clipboard_history WHERE id = ?1 AND kind = 'text'", params![id], |r| r.get(0))
            .map_err(|_| AppError::NotFound("запись не найдена".into()))
    }

    /// Копирует запись истории обратно в буфер обмена.
    pub fn copy(&self, db: &Database, id: i64) -> AppResult<()> {
        let (kind, content, image, hash): (String, String, Option<Vec<u8>>, String) = db
            .conn()
            .query_row("SELECT kind, content, image, hash FROM clipboard_history WHERE id = ?1", params![id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
            })
            .map_err(|_| AppError::NotFound("запись не найдена".into()))?;
        *self.own_hash.lock().unwrap_or_else(|p| p.into_inner()) = Some(hash);
        let sys = |e: arboard::Error| AppError::system(e.to_string());
        match kind.as_str() {
            "text" => crate::util::clipboard::with(|cb| cb.set_text(content))?.map_err(sys)?,
            "files" => {
                let files: Vec<String> = serde_json::from_str(&content)?;
                crate::util::clipboard::with(|cb| cb.set().file_list(&files))?.map_err(sys)?
            }
            "image" => {
                let png = image.ok_or_else(|| AppError::NotFound("изображение отсутствует".into()))?;
                let decoder = png::Decoder::new(std::io::Cursor::new(png));
                let mut reader = decoder.read_info().map_err(|e| AppError::system(e.to_string()))?;
                let mut buf = vec![0; reader.output_buffer_size()];
                let info = reader.next_frame(&mut buf).map_err(|e| AppError::system(e.to_string()))?;
                buf.truncate(info.buffer_size());
                let img = arboard::ImageData { width: info.width as usize, height: info.height as usize, bytes: buf.into() };
                crate::util::clipboard::with(|cb| cb.set_image(img))?.map_err(sys)?
            }
            _ => return Err(AppError::invalid("неизвестный тип записи")),
        }
        db.conn().execute("UPDATE clipboard_history SET created_at = ?1 WHERE id = ?2", params![now_ms(), id])?;
        Ok(())
    }

    pub fn pin(&self, db: &Database, id: i64, pinned: bool) -> AppResult<()> {
        db.conn().execute("UPDATE clipboard_history SET pinned = ?1 WHERE id = ?2", params![pinned as i64, id])?;
        Ok(())
    }

    pub fn delete(&self, db: &Database, id: i64) -> AppResult<()> {
        db.conn().execute("DELETE FROM clipboard_history WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn clear(&self, db: &Database, keep_pinned: bool) -> AppResult<usize> {
        let n = if keep_pinned {
            db.conn().execute("DELETE FROM clipboard_history WHERE pinned = 0", [])?
        } else {
            db.conn().execute("DELETE FROM clipboard_history", [])?
        };
        let _ = db.conn().execute_batch("PRAGMA incremental_vacuum;");
        Ok(n)
    }
}
