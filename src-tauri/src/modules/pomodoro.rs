//! Помодоро: фокус → короткий перерыв, после N циклов — длинный перерыв.
//! Во время фокуса по желанию включается «Не беспокоить» острова.
//! Статистика (сколько фокусов за день) хранится в pomodoro-stats.json.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;

use crate::core::error::{AppError, AppResult};
use crate::core::state::Shared;
use crate::core::task::TaskSlot;
use crate::database::now_ms;
use crate::events::{Activity, Priority};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PomodoroState {
    /// idle | focus | short | long
    pub phase: String,
    /// Номер фокуса в текущей серии (с 1).
    pub cycle: u32,
    pub cycles: u32,
    pub started_at: i64,
    pub ends_at: i64,
    pub paused_left_ms: Option<i64>,
    pub today: u32,
    pub week: Vec<u32>,
}

#[derive(Default)]
pub struct PomodoroModule {
    task: TaskSlot,
    state: Mutex<Option<PomodoroState>>,
    /// «Не беспокоить» включили мы — значит, и выключаем мы.
    dnd_by_us: Mutex<bool>,
}

pub const EVENT: &str = "pomodoro://state";

fn day_key(now: i64) -> String {
    let offset = crate::core::profiles::TZ_OFFSET_MIN.load(std::sync::atomic::Ordering::Relaxed) as i64;
    let days = (now / 60_000 + offset).div_euclid(24 * 60);
    days.to_string()
}

fn stats_path(st: &Shared) -> std::path::PathBuf {
    st.paths.data.join("pomodoro-stats.json")
}

fn load_stats(st: &Shared) -> BTreeMap<String, u32> {
    std::fs::read_to_string(stats_path(st)).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn stats_summary(st: &Shared) -> (u32, Vec<u32>) {
    let stats = load_stats(st);
    let today: i64 = day_key(now_ms()).parse().unwrap_or(0);
    let week: Vec<u32> = (0..7).rev().map(|i| stats.get(&(today - i).to_string()).copied().unwrap_or(0)).collect();
    (week.last().copied().unwrap_or(0), week)
}

fn add_focus(st: &Shared) {
    let mut stats = load_stats(st);
    *stats.entry(day_key(now_ms())).or_insert(0) += 1;
    // Храним только последние 60 дней.
    while stats.len() > 60 {
        let first = stats.keys().next().cloned();
        if let Some(k) = first {
            stats.remove(&k);
        }
    }
    if let Ok(s) = serde_json::to_string(&stats) {
        let _ = std::fs::write(stats_path(st), s);
    }
}

impl PomodoroModule {
    pub fn status(&self, st: &Shared) -> PomodoroState {
        let (today, week) = stats_summary(st);
        let cycles = st.with_settings(|s| s.pomodoro.cycles);
        let mut s = self.state.lock().unwrap_or_else(|p| p.into_inner()).clone().unwrap_or(PomodoroState {
            phase: "idle".into(),
            cycle: 0,
            cycles,
            started_at: 0,
            ends_at: 0,
            paused_left_ms: None,
            today: 0,
            week: vec![],
        });
        s.today = today;
        s.week = week;
        s
    }

    fn emit(&self, st: &Shared) {
        let s = self.status(st);
        st.events.emit(EVENT, s.clone());
        if s.phase == "idle" {
            st.events.clear_activity("pomodoro");
        } else {
            let title = match s.phase.as_str() {
                "focus" => format!("Фокус {}/{}", s.cycle, s.cycles),
                "short" => "Перерыв".to_string(),
                _ => "Длинный перерыв".to_string(),
            };
            st.events.set_activity(Activity {
                id: "pomodoro".into(),
                kind: "pomodoro".into(),
                priority: Priority::Normal,
                title,
                subtitle: s.paused_left_ms.map(|_| "пауза".to_string()),
                icon: None,
                progress: None,
                updated_at: now_ms(),
                started_at: Some(s.ends_at),
            });
        }
    }

    fn set_dnd(&self, st: &Shared, on: bool) {
        let mut by_us = self.dnd_by_us.lock().unwrap_or_else(|p| p.into_inner());
        if on && !st.with_settings(|s| s.pomodoro.dnd_during_focus) {
            return;
        }
        if on && !st.events.dnd() {
            st.events.set_dnd(true);
            *by_us = true;
        } else if !on && *by_us {
            st.events.set_dnd(false);
            *by_us = false;
        }
    }

    fn begin(&self, st: &Shared, phase: &str, cycle: u32) {
        let p = st.with_settings(|s| s.pomodoro.clone());
        let mins = match phase {
            "focus" => p.focus_min,
            "short" => p.short_min,
            _ => p.long_min,
        } as i64;
        let now = now_ms();
        *self.state.lock().unwrap_or_else(|p| p.into_inner()) =
            Some(PomodoroState { phase: phase.into(), cycle, cycles: p.cycles, started_at: now, ends_at: now + mins * 60_000, paused_left_ms: None, today: 0, week: vec![] });
        self.set_dnd(st, phase == "focus");
        self.emit(st);
    }

    pub fn start(&self, st: &Shared) -> AppResult<()> {
        let resumed = {
            let mut g = self.state.lock().unwrap_or_else(|p| p.into_inner());
            match g.as_mut() {
                Some(s) if s.paused_left_ms.is_some() => {
                    s.ends_at = now_ms() + s.paused_left_ms.take().unwrap_or(0);
                    true
                }
                Some(_) => return Ok(()),
                None => false,
            }
        };
        if resumed {
            self.emit(st);
        } else {
            self.begin(st, "focus", 1);
        }
        self.ensure_loop(st);
        Ok(())
    }

    pub fn pause(&self, st: &Shared) -> AppResult<()> {
        {
            let mut g = self.state.lock().unwrap_or_else(|p| p.into_inner());
            let s = g.as_mut().ok_or_else(|| AppError::invalid("помодоро не запущен"))?;
            if s.paused_left_ms.is_none() {
                s.paused_left_ms = Some((s.ends_at - now_ms()).max(0));
            }
        }
        self.emit(st);
        Ok(())
    }

    pub fn stop(&self, st: &Shared) {
        *self.state.lock().unwrap_or_else(|p| p.into_inner()) = None;
        self.task.stop();
        self.set_dnd(st, false);
        self.emit(st);
    }

    /// Следующая фаза: фокус → перерыв (каждый N-й — длинный) → фокус.
    pub fn skip(&self, st: &Shared) {
        let cur = self.state.lock().unwrap_or_else(|p| p.into_inner()).clone();
        let Some(s) = cur else { return };
        if s.phase == "focus" {
            add_focus(st);
            let long = s.cycle >= s.cycles;
            st.events.push(crate::events::internal_notification(
                "pomodoro",
                "Помодоро",
                if long { "Серия завершена — длинный перерыв" } else { "Фокус завершён — перерыв" },
                String::new(),
                Priority::Normal,
            ));
            self.begin(st, if long { "long" } else { "short" }, s.cycle);
        } else {
            let next = if s.phase == "long" { 1 } else { s.cycle + 1 };
            st.events.push(crate::events::internal_notification("pomodoro", "Помодоро", "Перерыв окончен — снова фокус", String::new(), Priority::Normal));
            self.begin(st, "focus", next);
        }
    }

    fn ensure_loop(&self, st: &Shared) {
        if self.task.is_running() {
            return;
        }
        let st = st.clone();
        self.task.start(async move {
            loop {
                tokio::time::sleep(Duration::from_millis(500)).await;
                let due = st
                    .pomodoro
                    .state
                    .lock()
                    .unwrap_or_else(|p| p.into_inner())
                    .as_ref()
                    .map(|s| s.paused_left_ms.is_none() && now_ms() >= s.ends_at)
                    .unwrap_or(false);
                if due {
                    st.pomodoro.skip(&st);
                }
            }
        });
    }
}
