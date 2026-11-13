//! SQLite-хранилище. Особенности:
//! * миграции по `PRAGMA user_version`;
//! * при повреждении файл переименовывается в `*.corrupt-<ts>` и создаётся новая база;
//! * если файл открыть невозможно совсем — работаем с базой в памяти и сообщаем об этом.

pub mod repo;

use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};

use crate::core::error::{AppError, AppResult};

/// Список миграций по порядку. Индекс + 1 = версия схемы.
const MIGRATIONS: &[&str] = &[include_str!("../../migrations/0001_init.sql"), include_str!("../../migrations/0002_compact_panels.sql"), include_str!("../../migrations/0003_notes_shelf.sql"), include_str!("../../migrations/0004_reminders.sql")];

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DbStatus {
    pub path: Option<String>,
    pub in_memory: bool,
    pub recovered_from_corruption: bool,
    pub schema_version: i64,
    pub warning: Option<String>,
}

pub struct Database {
    conn: Mutex<Connection>,
    pub status: DbStatus,
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn configure(conn: &Connection) -> rusqlite::Result<()> {
    conn.busy_timeout(std::time::Duration::from_secs(3))?;
    conn.execute_batch(
        "PRAGMA journal_mode = WAL;
         PRAGMA synchronous = NORMAL;
         PRAGMA foreign_keys = ON;
         PRAGMA temp_store = MEMORY;
         PRAGMA cache_size = -2000;",
    )?;
    Ok(())
}

fn integrity_ok(conn: &Connection) -> bool {
    conn.query_row("PRAGMA quick_check", [], |r| r.get::<_, String>(0))
        .map(|s| s == "ok")
        .unwrap_or(false)
}

fn migrate(conn: &mut Connection) -> AppResult<i64> {
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    let target = MIGRATIONS.len() as i64;
    if current > target {
        return Err(AppError::system(format!(
            "схема базы ({current}) новее, чем поддерживает приложение ({target})"
        )));
    }
    for (idx, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.execute_batch(&format!("PRAGMA user_version = {}", idx + 1))?;
        tx.commit()?;
        tracing::info!(version = idx + 1, "применена миграция базы данных");
    }
    Ok(target)
}

fn open_file(path: &Path) -> AppResult<Connection> {
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    configure(&conn)?;
    Ok(conn)
}

fn quarantine(path: &Path) -> Option<PathBuf> {
    let ts = now_ms();
    let target = path.with_extension(format!("db.corrupt-{ts}"));
    for suffix in ["-wal", "-shm"] {
        let side = PathBuf::from(format!("{}{}", path.display(), suffix));
        let _ = std::fs::remove_file(side);
    }
    std::fs::rename(path, &target).ok().map(|_| target)
}

impl Database {
    pub fn open(path: &Path) -> Self {
        let mut recovered = false;
        let mut warning = None;

        let attempt = |p: &Path| -> AppResult<(Connection, i64)> {
            let mut conn = open_file(p)?;
            if !integrity_ok(&conn) {
                return Err(AppError::system("проверка целостности не пройдена"));
            }
            let v = migrate(&mut conn)?;
            Ok((conn, v))
        };

        let result = match attempt(path) {
            Ok(ok) => Ok(ok),
            Err(e) if path.exists() => {
                tracing::error!(error = %e, "база данных повреждена или несовместима, создаётся новая");
                let moved = quarantine(path);
                recovered = true;
                warning = Some(match moved {
                    Some(p) => format!("База данных была повреждена и сохранена как {}. Настройки сброшены.", p.display()),
                    None => "База данных была повреждена и не может быть сохранена. Настройки сброшены.".into(),
                });
                attempt(path)
            }
            Err(e) => Err(e),
        };

        match result {
            Ok((conn, version)) => Self {
                conn: Mutex::new(conn),
                status: DbStatus {
                    path: Some(path.display().to_string()),
                    in_memory: false,
                    recovered_from_corruption: recovered,
                    schema_version: version,
                    warning,
                },
            },
            Err(e) => {
                tracing::error!(error = %e, "не удалось открыть базу данных, используется временная база в памяти");
                let mut conn = Connection::open_in_memory().expect("SQLite in-memory");
                let _ = configure(&conn);
                let version = migrate(&mut conn).unwrap_or(0);
                Self {
                    conn: Mutex::new(conn),
                    status: DbStatus {
                        path: None,
                        in_memory: true,
                        recovered_from_corruption: recovered,
                        schema_version: version,
                        warning: Some(format!(
                            "Не удалось открыть файл базы данных ({e}). Изменения не будут сохранены после перезапуска."
                        )),
                    },
                }
            }
        }
    }

    #[cfg(test)]
    pub fn open_memory() -> Self {
        let mut conn = Connection::open_in_memory().unwrap();
        configure(&conn).unwrap();
        let v = migrate(&mut conn).unwrap();
        Self {
            conn: Mutex::new(conn),
            status: DbStatus { path: None, in_memory: true, recovered_from_corruption: false, schema_version: v, warning: None },
        }
    }

    pub fn conn(&self) -> MutexGuard<'_, Connection> {
        // Отравленный мьютекс не должен «ронять» приложение: соединение остаётся валидным.
        self.conn.lock().unwrap_or_else(|p| p.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_apply() {
        let db = Database::open_memory();
        assert_eq!(db.status.schema_version, MIGRATIONS.len() as i64);
        let n: i64 = db
            .conn()
            .query_row("SELECT count(*) FROM sqlite_master WHERE type='table' AND name='event_log'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn corrupted_file_is_quarantined() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("island.db");
        std::fs::write(&path, b"this is definitely not a sqlite database file at all.....").unwrap();
        let db = Database::open(&path);
        assert!(!db.status.in_memory);
        assert!(db.status.recovered_from_corruption);
        assert!(db.status.warning.is_some());
        let entries: Vec<_> = std::fs::read_dir(dir.path()).unwrap().flatten().collect();
        assert!(entries.iter().any(|e| e.file_name().to_string_lossy().contains("corrupt")));
    }
}
