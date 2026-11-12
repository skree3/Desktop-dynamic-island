//! Превращает рост счётчика непрочитанных в заголовке окна мессенджера в уведомление.
//! Если то же приложение прислало настоящее системное уведомление, запасное не
//! показываем: настоящее содержит текст сообщения.

use std::collections::HashMap;

use crate::platform::os::unread::{family_of, UnreadWindow};
use crate::platform::types::{LaunchTarget, SystemNotification};

/// Сколько ждём системное уведомление, прежде чем показать запасное.
const GRACE_MS: i64 = 3000;

struct Pending {
    pid: u32,
    app: &'static str,
    exe_path: String,
    added: u32,
    total: u32,
    at: i64,
}

#[derive(Default)]
pub struct Tracker {
    last: HashMap<u32, u32>,
    last_native: HashMap<&'static str, i64>,
    pending: Vec<Pending>,
    icons: HashMap<&'static str, Option<String>>,
}

impl Tracker {
    /// Пришло системное уведомление (из центра уведомлений Windows).
    pub fn native(&mut self, app_name: &str, now: i64) {
        if let Some(f) = family_of(app_name) {
            self.last_native.insert(f, now);
        }
    }

    pub fn update(&mut self, seen: Vec<UnreadWindow>, now: i64) -> Vec<SystemNotification> {
        for u in &seen {
            let prev = self.last.insert(u.pid, u.count);
            if u.count == 0 || u.foreground {
                // Прочитано или приложение открыто: показывать нечего.
                self.pending.retain(|p| p.pid != u.pid);
                continue;
            }
            // Первое наблюдение — только запоминаем текущее число.
            let Some(prev) = prev else { continue };
            if u.count <= prev {
                if let Some(p) = self.pending.iter_mut().find(|p| p.pid == u.pid) {
                    p.total = u.count;
                }
                continue;
            }
            match self.pending.iter_mut().find(|p| p.pid == u.pid) {
                Some(p) => {
                    p.added += u.count - prev;
                    p.total = u.count;
                }
                None => self.pending.push(Pending { pid: u.pid, app: u.app, exe_path: u.exe_path.clone(), added: u.count - prev, total: u.count, at: now }),
            }
        }
        // Приложение закрыли — при следующем запуске начнём с нуля.
        self.last.retain(|k, _| seen.iter().any(|u| u.pid == *k));

        let Self { pending, last_native, icons, .. } = self;
        let mut out = vec![];
        pending.retain(|p| {
            if now - p.at < GRACE_MS {
                return true;
            }
            let had_native = last_native.get(p.app).map(|t| *t >= p.at - GRACE_MS * 2).unwrap_or(false);
            if !had_native {
                let icon = icons
                    .entry(p.app)
                    .or_insert_with(|| crate::platform::os::apps::icon(&LaunchTarget::Exe { path: p.exe_path.clone(), args: vec![] }, None, 48).ok().flatten())
                    .clone();
                out.push(SystemNotification {
                    key: format!("unread-{}-{now}", p.pid),
                    app_name: p.app.to_string(),
                    app_id: None,
                    title: if p.added == 1 { "Новое сообщение".into() } else { format!("Новых сообщений: {}", p.added) },
                    body: format!("Непрочитанных: {}", p.total),
                    icon,
                    timestamp: now,
                });
            }
            false
        });
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(count: u32, fg: bool) -> Vec<UnreadWindow> {
        vec![UnreadWindow { app: "Telegram", pid: 7, exe_path: String::new(), count, foreground: fg }]
    }

    #[test]
    fn fallback_flow() {
        let mut t = Tracker::default();
        t.icons.insert("Telegram", None);
        assert!(t.update(w(5, false), 0).is_empty()); // базовое значение
        assert!(t.update(w(6, false), 1000).is_empty()); // ждём системное уведомление
        assert!(t.update(w(7, false), 2500).is_empty());
        let out = t.update(w(7, false), 4100);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].title, "Новых сообщений: 2");
        assert_eq!(out[0].body, "Непрочитанных: 7");

        // Системное уведомление пришло — запасное не нужно.
        t.update(w(8, false), 10_000);
        t.native("Telegram Desktop", 11_000);
        assert!(t.update(w(8, false), 14_000).is_empty());

        // Прочитали до истечения ожидания.
        t.update(w(9, false), 20_000);
        t.update(w(0, false), 21_000);
        assert!(t.update(w(0, false), 25_000).is_empty());

        // Окно Telegram активно.
        t.update(w(3, true), 30_000);
        assert!(t.update(w(3, true), 34_000).is_empty());
    }
}
