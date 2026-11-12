//! Таймеры обратного отсчёта и секундомеры. Несколько независимых таймеров,
//! состояние хранится в SQLite и переживает перезапуск (запущенный таймер
//! продолжает идти по реальному времени). Планировщик не опрашивает таймеры
//! в цикле — он спит до ближайшего срока или до изменения списка.

use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tauri::AppHandle;
use tokio::sync::Notify;

use crate::core::error::{AppError, AppResult};
use crate::core::state::Shared;
use crate::core::task::TaskSlot;
use crate::database::{now_ms, Database};
use crate::events::{names, Activity, IslandNotification, NotificationAction, NotificationKind, Priority};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Timer {
    pub id: String,
    pub label: String,
    pub kind: String,
    pub duration_ms: i64,
    pub elapsed_ms: i64,
    pub started_at: Option<i64>,
    pub state: String,
    pub created_at: i64,
}

impl Timer {
    pub fn elapsed_now(&self, now: i64) -> i64 {
        self.elapsed_ms + self.started_at.map(|s| (now - s).max(0)).unwrap_or(0)
    }
    pub fn deadline(&self) -> Option<i64> {
        if self.kind == "countdown" && self.state == "running" {
            self.started_at.map(|s| s + (self.duration_ms - self.elapsed_ms))
        } else {
            None
        }
    }
}

pub struct TimersModule {
    task: TaskSlot,
    wake: Arc<Notify>,
}

impl Default for TimersModule {
    fn default() -> Self {
        Self { task: TaskSlot::new(), wake: Arc::new(Notify::new()) }
    }
}

fn row(r: &rusqlite::Row) -> rusqlite::Result<Timer> {
    Ok(Timer {
        id: r.get(0)?,
        label: r.get(1)?,
        kind: r.get(2)?,
        duration_ms: r.get(3)?,
        elapsed_ms: r.get(4)?,
        started_at: r.get(5)?,
        state: r.get(6)?,
        created_at: r.get(7)?,
    })
}

const COLS: &str = "id, label, kind, duration_ms, elapsed_ms, started_at, state, created_at";

impl TimersModule {
    pub fn list(&self, db: &Database) -> AppResult<Vec<Timer>> {
        let conn = db.conn();
        let mut stmt = conn.prepare(&format!("SELECT {COLS} FROM timers ORDER BY created_at"))?;
        let rows = stmt.query_map([], row)?.flatten().collect();
        Ok(rows)
    }

    fn get(&self, db: &Database, id: &str) -> AppResult<Timer> {
        db.conn()
            .query_row(&format!("SELECT {COLS} FROM timers WHERE id = ?1"), params![id], row)
            .map_err(|_| AppError::NotFound("таймер не найден".into()))
    }

    fn save(&self, db: &Database, t: &Timer) -> AppResult<()> {
        db.conn().execute(
            "INSERT INTO timers(id, label, kind, duration_ms, elapsed_ms, started_at, state, created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8)
             ON CONFLICT(id) DO UPDATE SET label=excluded.label, duration_ms=excluded.duration_ms, elapsed_ms=excluded.elapsed_ms,
             started_at=excluded.started_at, state=excluded.state",
            params![t.id, t.label, t.kind, t.duration_ms, t.elapsed_ms, t.started_at, t.state, t.created_at],
        )?;
        Ok(())
    }

    fn changed(&self, st: &Shared) {
        self.wake.notify_one();
        let list = self.list(&st.db).unwrap_or_default();
        for t in &list {
            let id = format!("timer:{}", t.id);
            if t.state == "running" {
                st.events.set_activity(Activity {
                    id,
                    kind: t.kind.clone(),
                    priority: if t.kind == "countdown" { Priority::High } else { Priority::Normal },
                    title: t.label.clone(),
                    subtitle: None,
                    icon: None,
                    progress: None,
                    updated_at: t.started_at.unwrap_or(0),
                    started_at: None,
                });
            } else {
                st.events.clear_activity(&id);
            }
        }
        // Удалённые таймеры
        let ids: Vec<String> = list.iter().map(|t| format!("timer:{}", t.id)).collect();
        for a in st.events.activities() {
            if a.id.starts_with("timer:") && !ids.contains(&a.id) {
                st.events.clear_activity(&a.id);
            }
        }
        st.events.emit(names::TIMERS_CHANGED, list);
    }

    pub fn create(&self, st: &Shared, label: String, kind: String, duration_ms: i64, start: bool) -> AppResult<Timer> {
        if kind != "countdown" && kind != "stopwatch" {
            return Err(AppError::invalid("тип таймера должен быть countdown или stopwatch"));
        }
        if kind == "countdown" && !(1000..=7 * 24 * 3600 * 1000).contains(&duration_ms) {
            return Err(AppError::invalid("длительность должна быть от 1 секунды до 7 дней"));
        }
        if self.list(&st.db)?.len() >= 50 {
            return Err(AppError::invalid("слишком много таймеров (максимум 50)"));
        }
        let label = crate::util::clean_text(&label, 60);
        let now = now_ms();
        let t = Timer {
            id: uuid::Uuid::new_v4().to_string(),
            label: if label.is_empty() { if kind == "countdown" { "Таймер".into() } else { "Секундомер".into() } } else { label },
            duration_ms: if kind == "countdown" { duration_ms } else { 0 },
            kind,
            elapsed_ms: 0,
            started_at: if start { Some(now) } else { None },
            state: if start { "running".into() } else { "idle".into() },
            created_at: now,
        };
        self.save(&st.db, &t)?;
        self.changed(st);
        Ok(t)
    }

    pub fn start(&self, st: &Shared, id: &str) -> AppResult<()> {
        let mut t = self.get(&st.db, id)?;
        if t.state == "running" {
            return Ok(());
        }
        if t.state == "finished" {
            t.elapsed_ms = 0;
        }
        t.state = "running".into();
        t.started_at = Some(now_ms());
        self.save(&st.db, &t)?;
        self.changed(st);
        Ok(())
    }

    pub fn pause(&self, st: &Shared, id: &str) -> AppResult<()> {
        let mut t = self.get(&st.db, id)?;
        if t.state != "running" {
            return Ok(());
        }
        t.elapsed_ms = t.elapsed_now(now_ms());
        t.started_at = None;
        t.state = "paused".into();
        self.save(&st.db, &t)?;
        self.changed(st);
        Ok(())
    }

    pub fn reset(&self, st: &Shared, id: &str) -> AppResult<()> {
        let mut t = self.get(&st.db, id)?;
        t.elapsed_ms = 0;
        t.started_at = None;
        t.state = "idle".into();
        self.save(&st.db, &t)?;
        self.changed(st);
        Ok(())
    }

    pub fn delete(&self, st: &Shared, id: &str) -> AppResult<()> {
        st.db.conn().execute("DELETE FROM timers WHERE id = ?1", params![id])?;
        self.changed(st);
        Ok(())
    }

    pub fn rename(&self, st: &Shared, id: &str, label: &str) -> AppResult<()> {
        let mut t = self.get(&st.db, id)?;
        t.label = crate::util::clean_text(label, 60);
        self.save(&st.db, &t)?;
        self.changed(st);
        Ok(())
    }

    fn finish_due(&self, st: &Shared) {
        let now = now_ms();
        let due: Vec<Timer> = self.list(&st.db).unwrap_or_default().into_iter().filter(|t| t.deadline().map(|d| d <= now).unwrap_or(false)).collect();
        if due.is_empty() {
            return;
        }
        for mut t in due {
            t.state = "finished".into();
            t.elapsed_ms = t.duration_ms;
            t.started_at = None;
            let _ = self.save(&st.db, &t);
            st.events.push(IslandNotification {
                id: uuid::Uuid::new_v4().to_string(),
                kind: NotificationKind::Timer,
                source: format!("timer:{}", t.id),
                app_name: "Таймер".into(),
                icon: None,
                title: format!("{} — время вышло", t.label),
                body: format!("Таймер на {} завершён", format_duration(t.duration_ms)),
                priority: Priority::High,
                timestamp: now,
                actions: vec![
                    NotificationAction { id: "restart".into(), label: "Повторить".into() },
                    NotificationAction { id: "dismiss".into(), label: "Закрыть".into() },
                ],
                open_target: None,
                duration_ms: 15_000,
            });
        }
        self.changed(st);
    }

    pub fn set_running(&self, app: &AppHandle, st: &Shared, run: bool) {
        if run == self.task.is_running() {
            return;
        }
        if !run {
            self.task.stop();
            st.events.clear_activities_with_prefix("timer:");
            return;
        }
        let _ = app;
        let st2 = st.clone();
        let wake = self.wake.clone();
        self.changed(st);
        self.task.start(async move {
            loop {
                st2.timers.finish_due(&st2);
                let next = st2.timers.list(&st2.db).unwrap_or_default().iter().filter_map(|t| t.deadline()).min();
                let sleep_ms = next.map(|d| (d - now_ms()).clamp(50, 3_600_000)).unwrap_or(3_600_000) as u64;
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(sleep_ms)) => {}
                    _ = wake.notified() => {}
                }
            }
        });
    }
}

pub fn format_duration(ms: i64) -> String {
    let s = ms / 1000;
    let (h, m, sec) = (s / 3600, (s % 3600) / 60, s % 60);
    if h > 0 {
        format!("{h} ч {m:02} мин")
    } else if m > 0 {
        if sec > 0 { format!("{m} мин {sec} с") } else { format!("{m} мин") }
    } else {
        format!("{sec} с")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadline_math() {
        let t = Timer {
            id: "a".into(),
            label: "x".into(),
            kind: "countdown".into(),
            duration_ms: 60_000,
            elapsed_ms: 10_000,
            started_at: Some(1_000),
            state: "running".into(),
            created_at: 0,
        };
        assert_eq!(t.deadline(), Some(51_000));
        assert_eq!(t.elapsed_now(11_000), 20_000);
        assert_eq!(format_duration(90_000), "1 мин 30 с");
        assert_eq!(format_duration(3_600_000), "1 ч 00 мин");
    }
}
