//! Календарь по секретной ICS-ссылке (Google, Outlook, Яндекс): ближайшее
//! событие в острове («Созвон · через 12 мин») и напоминание заранее.
//! Ссылки скачиваются раз в 15 минут; повторяющиеся события (RRULE)
//! учитываются только по первой дате.

use std::sync::Mutex;
use std::time::Duration;

use serde::Serialize;

use crate::core::state::Shared;
use crate::core::task::TaskSlot;
use crate::database::now_ms;
use crate::events::{Activity, NotificationAction, Priority};

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CalEvent {
    pub uid: String,
    pub title: String,
    pub start: i64,
    pub end: i64,
    pub all_day: bool,
    pub location: String,
}

#[derive(Default)]
pub struct CalendarModule {
    task: TaskSlot,
    events: Mutex<Vec<CalEvent>>,
    reminded: Mutex<std::collections::HashSet<String>>,
    pub error: Mutex<Option<String>>,
}

/// Склеивает перенесённые строки ICS (строка, начинающаяся с пробела, — продолжение).
fn unfold(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for raw in text.lines() {
        let line = raw.trim_end_matches('\r');
        if (line.starts_with(' ') || line.starts_with('\t')) && !out.is_empty() {
            out.last_mut().unwrap().push_str(&line[1..]);
        } else {
            out.push(line.to_string());
        }
    }
    out
}

fn unescape(s: &str) -> String {
    s.replace("\\n", " ").replace("\\N", " ").replace("\\,", ",").replace("\\;", ";").replace("\\\\", "\\")
}

/// Дни от 1970-01-01 для даты (григорианский календарь).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Разбор DTSTART/DTEND. Время без «Z» считается местным (смещение сообщает интерфейс).
fn parse_time(params: &str, value: &str, tz_offset_min: i64) -> Option<(i64, bool)> {
    let v = value.trim();
    let date_only = params.contains("VALUE=DATE") && !params.contains("VALUE=DATE-TIME") || v.len() == 8;
    let y: i64 = v.get(0..4)?.parse().ok()?;
    let mo: i64 = v.get(4..6)?.parse().ok()?;
    let d: i64 = v.get(6..8)?.parse().ok()?;
    let days = days_from_civil(y, mo, d);
    if date_only {
        return Some(((days * 86_400 - tz_offset_min * 60) * 1000, true));
    }
    let t = v.get(9..15)?;
    let h: i64 = t.get(0..2)?.parse().ok()?;
    let mi: i64 = t.get(2..4)?.parse().ok()?;
    let s: i64 = t.get(4..6)?.parse().ok()?;
    let mut secs = days * 86_400 + h * 3600 + mi * 60 + s;
    if !v.ends_with('Z') {
        secs -= tz_offset_min * 60;
    }
    Some((secs * 1000, false))
}

pub fn parse_ics(text: &str, tz_offset_min: i64) -> Vec<CalEvent> {
    let mut out = Vec::new();
    let mut cur: Option<CalEvent> = None;
    let mut has_end = false;
    for line in unfold(text) {
        if line == "BEGIN:VEVENT" {
            cur = Some(CalEvent { uid: String::new(), title: String::new(), start: 0, end: 0, all_day: false, location: String::new() });
            has_end = false;
            continue;
        }
        if line == "END:VEVENT" {
            if let Some(mut e) = cur.take() {
                if e.start > 0 {
                    if !has_end {
                        e.end = e.start + if e.all_day { 86_400_000 } else { 3_600_000 };
                    }
                    if e.uid.is_empty() {
                        e.uid = format!("{}-{}", e.start, e.title);
                    }
                    out.push(e);
                }
            }
            continue;
        }
        let Some(e) = cur.as_mut() else { continue };
        let Some((key, value)) = line.split_once(':') else { continue };
        let (name, params) = key.split_once(';').unwrap_or((key, ""));
        match name {
            "SUMMARY" => e.title = unescape(value).chars().take(200).collect(),
            "LOCATION" => e.location = unescape(value).chars().take(200).collect(),
            "UID" => e.uid = value.chars().take(200).collect(),
            "DTSTART" => {
                if let Some((t, all_day)) = parse_time(params, value, tz_offset_min) {
                    e.start = t;
                    e.all_day = all_day;
                }
            }
            "DTEND" => {
                if let Some((t, _)) = parse_time(params, value, tz_offset_min) {
                    e.end = t;
                    has_end = true;
                }
            }
            _ => {}
        }
    }
    out
}

fn fetch(url: &str) -> Result<String, String> {
    let url = url.trim().replacen("webcal://", "https://", 1);
    if !url.starts_with("https://") {
        return Err("нужна ссылка https:// или webcal://".into());
    }
    let resp = ureq::get(&url).timeout(Duration::from_secs(20)).set("User-Agent", concat!("DesktopIsland/", env!("CARGO_PKG_VERSION"))).call().map_err(|e| e.to_string())?;
    let mut body = String::new();
    std::io::Read::read_to_string(&mut std::io::Read::take(resp.into_reader(), 8 * 1024 * 1024), &mut body).map_err(|e| e.to_string())?;
    Ok(body)
}

fn minutes_word(m: i64) -> String {
    let n = m.abs() % 100;
    let w = if (11..=14).contains(&n) {
        "минут"
    } else {
        match n % 10 {
            1 => "минуту",
            2..=4 => "минуты",
            _ => "минут",
        }
    };
    format!("{m} {w}")
}

impl CalendarModule {
    pub fn upcoming(&self) -> Vec<CalEvent> {
        let now = now_ms();
        let mut list: Vec<CalEvent> = self.events.lock().unwrap_or_else(|p| p.into_inner()).iter().filter(|e| e.end > now).cloned().collect();
        list.sort_by_key(|e| e.start);
        list.truncate(30);
        list
    }

    pub fn set_running(&self, st: &Shared, run: bool) {
        if !run {
            if self.task.is_running() {
                self.task.stop();
                st.events.clear_activity("calendar");
            }
            return;
        }
        if self.task.is_running() {
            return;
        }
        let st = st.clone();
        self.task.start(async move {
            let mut last_fetch = 0i64;
            loop {
                let urls = st.with_settings(|s| s.calendar.urls.clone());
                if now_ms() - last_fetch > 15 * 60_000 {
                    last_fetch = now_ms();
                    let tz = crate::core::profiles::TZ_OFFSET_MIN.load(std::sync::atomic::Ordering::Relaxed) as i64;
                    let fetched = tauri::async_runtime::spawn_blocking(move || {
                        let mut all = Vec::new();
                        let mut err = None;
                        for u in urls {
                            match fetch(&u) {
                                Ok(text) => all.extend(parse_ics(&text, tz)),
                                Err(e) => err = Some(e),
                            }
                        }
                        (all, err)
                    })
                    .await;
                    if let Ok((all, err)) = fetched {
                        *st.calendar.events.lock().unwrap_or_else(|p| p.into_inner()) = all;
                        *st.calendar.error.lock().unwrap_or_else(|p| p.into_inner()) = err;
                        st.events.emit("calendar://changed", st.calendar.upcoming());
                    }
                }
                st.calendar.tick(&st);
                tokio::time::sleep(Duration::from_secs(20)).await;
            }
        });
    }

    pub fn refresh(&self, st: &Shared) {
        self.task.stop();
        self.set_running(st, st.with_settings(|s| !s.calendar.urls.is_empty()));
    }

    /// Ближайшее событие — активность острова, за N минут — напоминание.
    fn tick(&self, st: &Shared) {
        let now = now_ms();
        let remind = st.with_settings(|s| s.calendar.remind_minutes) as i64 * 60_000;
        let next = self.upcoming().into_iter().find(|e| !e.all_day && e.end > now);
        match next {
            Some(e) if e.start - now <= 60 * 60_000 => {
                let mins = ((e.start - now) as f64 / 60_000.0).ceil() as i64;
                let subtitle = if e.start > now { format!("через {}", minutes_word(mins)) } else { "идёт сейчас".to_string() };
                st.events.set_activity(Activity {
                    id: "calendar".into(),
                    kind: "calendar".into(),
                    priority: if e.start - now <= remind { Priority::High } else { Priority::Normal },
                    title: e.title.clone(),
                    subtitle: Some(subtitle),
                    icon: None,
                    progress: None,
                    updated_at: now,
                    started_at: Some(e.start),
                });
                let key = format!("{}@{}", e.uid, e.start);
                if e.start > now && e.start - now <= remind && self.reminded.lock().unwrap_or_else(|p| p.into_inner()).insert(key) {
                    let mut n = crate::events::internal_notification(
                        "calendar",
                        "Календарь",
                        e.title.clone(),
                        if e.location.is_empty() { format!("Через {}", minutes_word(mins)) } else { format!("Через {} · {}", minutes_word(mins), e.location) },
                        Priority::High,
                    );
                    n.actions = vec![NotificationAction { id: "dismiss".into(), label: "Понятно".into() }];
                    st.events.push(n);
                }
            }
            _ => st.events.clear_activity("calendar"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_events() {
        let ics = "BEGIN:VCALENDAR\r\nBEGIN:VEVENT\r\nUID:1\r\nSUMMARY:Созвон\\, команда\r\nDTSTART:20261113T150000Z\r\nDTEND:20261113T153000Z\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nSUMMARY:Отпуск\r\nDTSTART;VALUE=DATE:20261120\r\nEND:VEVENT\r\nBEGIN:VEVENT\r\nSUMMARY:Встре\r\n ча\r\nDTSTART;TZID=Europe/Moscow:20261114T100000\r\nEND:VEVENT\r\nEND:VCALENDAR";
        let ev = parse_ics(ics, 180);
        assert_eq!(ev.len(), 3);
        assert_eq!(ev[0].title, "Созвон, команда");
        assert_eq!(ev[0].start, 1_794_582_000_000);
        assert_eq!(ev[0].end - ev[0].start, 30 * 60_000);
        assert!(ev[1].all_day);
        assert_eq!(ev[2].title, "Встреча");
        // 10:00 по Москве (UTC+3) = 07:00 UTC
        assert_eq!(ev[2].start, 1_794_639_600_000);
    }

    #[test]
    fn minute_words() {
        assert_eq!(minutes_word(1), "1 минуту");
        assert_eq!(minutes_word(3), "3 минуты");
        assert_eq!(minutes_word(12), "12 минут");
        assert_eq!(minutes_word(21), "21 минуту");
    }
}
