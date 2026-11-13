//! Полка: файлы и папки, перетащенные на остров. Хранятся ссылки (пути),
//! сами файлы никуда не копируются.

pub mod screenshot;

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

/// Файлы перетаскивают на окно острова: всё, что брошено на остров, кладём на полку.
/// Обрабатывается здесь, а не во фронтенде, чтобы файл попал на полку при любом
/// состоянии панели (свёрнута, анимируется, открыта другая карточка).
pub fn on_drag_drop(app: &tauri::AppHandle, ev: &tauri::DragDropEvent) {
    match ev {
        tauri::DragDropEvent::Enter { paths, .. } => {
            handle_drag(app, Drag::Enter(paths.len()));
        }
        tauri::DragDropEvent::Leave => {
            handle_drag(app, Drag::Leave);
        }
        tauri::DragDropEvent::Drop { paths, .. } => {
            handle_drag(app, Drag::Drop(paths));
        }
        _ => {}
    }
}

pub enum Drag<'a> {
    Enter(usize),
    Leave,
    Drop(&'a [std::path::PathBuf]),
}

/// Возвращает, принимает ли остров перетаскивание (полка включена).
pub fn handle_drag(app: &tauri::AppHandle, ev: Drag) -> bool {
    let st = crate::core::state::shared(app);
    if !st.with_settings(|s| s.modules.shelf) {
        return false;
    }
    match ev {
        Drag::Enter(count) => st.events.emit("shelf://drag", serde_json::json!({ "state": "enter", "count": count })),
        Drag::Leave => st.events.emit("shelf://drag", serde_json::json!({ "state": "leave" })),
        Drag::Drop(paths) => {
            let list: Vec<String> = paths.iter().map(|p| p.to_string_lossy().to_string()).collect();
            let added = match add(&st, &list) {
                Ok(n) => n,
                Err(e) => {
                    st.log.warn(crate::logging::category::APP, "shelf", format!("не удалось добавить файлы на полку: {e}"));
                    0
                }
            };
            st.events.emit("shelf://drag", serde_json::json!({ "state": "drop", "added": added }));
        }
    }
    true
}

/// Windows: следит за перетаскиванием над островом и подставляет окно-приёмник
/// (см. platform::windows::dragdrop). Опрос раз в 50 мс — это только чтение
/// состояния кнопки и курсора, без пробуждения WebView.
#[cfg(windows)]
pub fn start_drop_guard(app: &tauri::AppHandle) {
    use crate::platform::os::dragdrop as dd;
    let a = app.clone();
    let _ = app.run_on_main_thread(move || {
        let h = a.clone();
        let ok = dd::init(Box::new(move |ev| match ev {
            dd::DropEvent::Enter(p) => handle_drag(&h, Drag::Enter(p.len())),
            dd::DropEvent::Leave => handle_drag(&h, Drag::Leave),
            dd::DropEvent::Drop(p) => handle_drag(&h, Drag::Drop(&p)),
        }));
        if !ok {
            crate::core::state::shared(&a).log.warn(crate::logging::category::APP, "shelf", "не удалось подготовить приём перетаскиваемых файлов");
        }
    });
    let app = app.clone();
    std::thread::Builder::new()
        .name("shelf-drop-guard".into())
        .spawn(move || {
            let st = crate::core::state::shared(&app);
            let mut was_down = false;
            let mut armed = false;
            let mut shown: Option<windows::Win32::Foundation::RECT> = None;
            loop {
                std::thread::sleep(std::time::Duration::from_millis(50));
                // Окно острова больше самого острова — приёмник ставится над островом.
                let island = crate::core::window::island(&app)
                    .filter(|w| w.is_visible().unwrap_or(false))
                    .and_then(|_| crate::core::window::island_rect(&app))
                    .map(|(x, y, w, h)| windows::Win32::Foundation::RECT { left: x, top: y, right: x + w, bottom: y + h });
                let down = dd::primary_button_down();
                let cur = dd::cursor();
                if down && !was_down {
                    // Перетаскивание файла начинается где-то ещё; нажатие на самом острове —
                    // это клик или перемещение острова, приёмник там не нужен.
                    armed = match (&island, &cur) {
                        (Some(r), Some(p)) => !dd::contains(r, p, 0),
                        _ => false,
                    };
                }
                if !down {
                    armed = false;
                }
                was_down = down;
                let want = match (&island, &cur) {
                    (Some(r), Some(p)) if armed && dd::contains(r, p, 12) && st.with_settings(|s| s.modules.shelf) => {
                        Some(windows::Win32::Foundation::RECT { left: r.left - 12, top: r.top - 12, right: r.right + 12, bottom: r.bottom + 12 })
                    }
                    _ => None,
                };
                if want != shown {
                    shown = want;
                    let _ = app.run_on_main_thread(move || dd::place(want));
                }
            }
        })
        .ok();
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
    reveal_path(&path_of(st, id)?)
}

/// Показывает файл по пути в проводнике / файловом менеджере.
pub fn reveal_path(p: &str) -> AppResult<()> {
    let p = p.to_string();
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

/// Перетаскивание файлов с полки в другое окно (чат, проводник, браузер).
/// Файлы копируются, а не перемещаются. Вызывать, пока кнопка мыши нажата.
pub fn drag_out(app: &tauri::AppHandle, st: &Shared, ids: &[i64]) -> AppResult<()> {
    let mut paths = vec![];
    for id in ids {
        let p = path_of(st, *id)?;
        if Path::new(&p).exists() {
            paths.push(std::path::PathBuf::from(p));
        }
    }
    if paths.is_empty() {
        return Err(AppError::NotFound("файл перемещён или удалён".into()));
    }
    let win = crate::core::window::island(app).ok_or_else(|| AppError::unsupported("окно острова недоступно"))?;
    // Значок под курсором: иконка файла, иначе значок приложения.
    let target = crate::platform::types::LaunchTarget::Exe { path: paths[0].to_string_lossy().to_string(), args: vec![] };
    let icon = crate::platform::os::apps::icon(&target, None, 48)
        .ok()
        .flatten()
        .and_then(|url| url.split_once("base64,").map(|(_, b)| b.to_string()))
        .and_then(|b| base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b).ok())
        .unwrap_or_else(|| include_bytes!("../../../icons/32x32.png").to_vec());
    let w = win.clone();
    app.run_on_main_thread(move || {
        let opts = drag::Options { mode: drag::DragMode::Copy, ..Default::default() };
        #[cfg(windows)]
        let r = drag::start_drag(&w, drag::DragItem::Files(paths), drag::Image::Raw(icon), |_, _| {}, opts);
        #[cfg(target_os = "linux")]
        let r = match w.gtk_window() {
            Ok(gw) => drag::start_drag(&gw, drag::DragItem::Files(paths), drag::Image::Raw(icon), |_, _| {}, opts),
            Err(_) => Ok(()),
        };
        if let Err(e) = r {
            tracing::warn!(error = %e, "не удалось начать перетаскивание с полки");
        }
    })
    .map_err(|e| AppError::system(e.to_string()))
}
