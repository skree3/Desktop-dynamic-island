//! Логирование: `tracing` в файл (ротация по дням) + пользовательский журнал
//! событий в таблице `event_log` с ограничением размера.
//!
//! В журнал событий не попадают содержимое уведомлений, буфера обмена и
//! другие персональные данные — только факты (что произошло и где).

use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use crate::core::error::AppResult;
use crate::database::{now_ms, Database};

pub fn init_tracing(log_dir: &Path) -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let _ = std::fs::create_dir_all(log_dir);
    let appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("desktop-island")
        .filename_suffix("log")
        .max_log_files(5)
        .build(log_dir)
        .ok();
    let filter = EnvFilter::try_from_env("DESKTOP_ISLAND_LOG").unwrap_or_else(|_| EnvFilter::new("info,tao=warn,wry=warn"));
    match appender {
        Some(appender) => {
            let (nb, guard) = tracing_appender::non_blocking(appender);
            let _ = tracing_subscriber::registry()
                .with(filter)
                .with(fmt::layer().with_writer(nb).with_ansi(false))
                .with(fmt::layer().with_writer(std::io::stderr))
                .try_init();
            Some(guard)
        }
        None => {
            let _ = tracing_subscriber::registry().with(filter).with(fmt::layer()).try_init();
            None
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Error,
    Warn,
    Info,
    Debug,
}
impl Level {
    fn as_str(self) -> &'static str {
        match self {
            Level::Error => "error",
            Level::Warn => "warn",
            Level::Info => "info",
            Level::Debug => "debug",
        }
    }
    fn rank(self) -> u8 {
        match self {
            Level::Error => 0,
            Level::Warn => 1,
            Level::Info => 2,
            Level::Debug => 3,
        }
    }
}

/// Категории журнала событий (соответствуют требованиям ТЗ).
pub mod category {
    pub const APP: &str = "app";
    pub const MODULE: &str = "module";
    pub const INTEGRATION: &str = "integration";
    pub const PLUGIN: &str = "plugin";
    pub const SYSTEM_ACTION: &str = "system_action";
    pub const PERMISSION: &str = "permission";
    pub const GAME_MODE: &str = "game_mode";
    pub const DATABASE: &str = "database";
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub id: i64,
    pub ts: i64,
    pub level: String,
    pub category: String,
    pub source: String,
    pub message: String,
}

pub struct EventLog {
    db: Arc<Database>,
    max_entries: AtomicU64,
    min_level: AtomicU32,
    inserts: AtomicU32,
}

impl EventLog {
    pub fn new(db: Arc<Database>, max_entries: u32, level: &str) -> Self {
        let s = Self { db, max_entries: AtomicU64::new(max_entries as u64), min_level: AtomicU32::new(2), inserts: AtomicU32::new(0) };
        s.configure(max_entries, level);
        s
    }

    pub fn configure(&self, max_entries: u32, level: &str) {
        self.max_entries.store(max_entries.max(100) as u64, Ordering::Relaxed);
        let rank = match level {
            "error" => 0,
            "warn" => 1,
            "debug" => 3,
            _ => 2,
        };
        self.min_level.store(rank, Ordering::Relaxed);
    }

    pub fn log(&self, level: Level, category: &str, source: &str, message: impl AsRef<str>) {
        let message = message.as_ref();
        match level {
            Level::Error => tracing::error!(%category, %source, "{message}"),
            Level::Warn => tracing::warn!(%category, %source, "{message}"),
            Level::Info => tracing::info!(%category, %source, "{message}"),
            Level::Debug => tracing::debug!(%category, %source, "{message}"),
        }
        if level.rank() as u32 > self.min_level.load(Ordering::Relaxed) {
            return;
        }
        let msg: String = message.chars().take(500).collect();
        let res = self.db.conn().execute(
            "INSERT INTO event_log(ts, level, category, source, message) VALUES(?1,?2,?3,?4,?5)",
            params![now_ms(), level.as_str(), category, source, msg],
        );
        if let Err(e) = res {
            tracing::error!(error = %e, "не удалось записать событие в журнал");
            return;
        }
        // Обрезаем журнал не на каждую запись, а раз в 50 вставок.
        if self.inserts.fetch_add(1, Ordering::Relaxed) % 50 == 0 {
            self.trim();
        }
    }

    pub fn info(&self, category: &str, source: &str, message: impl AsRef<str>) {
        self.log(Level::Info, category, source, message)
    }
    pub fn warn(&self, category: &str, source: &str, message: impl AsRef<str>) {
        self.log(Level::Warn, category, source, message)
    }
    pub fn error(&self, category: &str, source: &str, message: impl AsRef<str>) {
        self.log(Level::Error, category, source, message)
    }

    pub fn trim(&self) {
        let max = self.max_entries.load(Ordering::Relaxed) as i64;
        let _ = self.db.conn().execute(
            "DELETE FROM event_log WHERE id <= (SELECT id FROM event_log ORDER BY id DESC LIMIT 1 OFFSET ?1)",
            params![max],
        );
    }

    pub fn list(&self, limit: u32, category: Option<&str>, level: Option<&str>) -> AppResult<Vec<LogEntry>> {
        let conn = self.db.conn();
        let mut stmt = conn.prepare(
            "SELECT id, ts, level, category, source, message FROM event_log
             WHERE (?1 IS NULL OR category = ?1) AND (?2 IS NULL OR level = ?2)
             ORDER BY id DESC LIMIT ?3",
        )?;
        let rows = stmt
            .query_map(params![category, level, limit.min(5000)], |r| {
                Ok(LogEntry {
                    id: r.get(0)?,
                    ts: r.get(1)?,
                    level: r.get(2)?,
                    category: r.get(3)?,
                    source: r.get(4)?,
                    message: r.get(5)?,
                })
            })?
            .flatten()
            .collect();
        Ok(rows)
    }

    pub fn clear(&self) -> AppResult<()> {
        self.db.conn().execute("DELETE FROM event_log", [])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_is_trimmed() {
        let db = Arc::new(Database::open_memory());
        let log = EventLog::new(db, 100, "info");
        for i in 0..260 {
            log.info(category::APP, "test", format!("event {i}"));
        }
        log.trim();
        let entries = log.list(1000, None, None).unwrap();
        assert!(entries.len() <= 100);
        assert_eq!(entries[0].message, "event 259");
    }

    #[test]
    fn level_filter() {
        let db = Arc::new(Database::open_memory());
        let log = EventLog::new(db, 100, "warn");
        log.info(category::APP, "t", "skip");
        log.warn(category::APP, "t", "keep");
        assert_eq!(log.list(10, None, None).unwrap().len(), 1);
    }
}
