//! Чистая (без таймеров и Tauri) логика очереди уведомлений.
//! Вынесена отдельно, чтобы её можно было покрыть модульными тестами.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(rename_all = "lowercase")]
pub enum Priority {
    Low = 0,
    Normal = 1,
    High = 2,
    Critical = 3,
}

impl Priority {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "low" => Some(Priority::Low),
            "normal" => Some(Priority::Normal),
            "high" => Some(Priority::High),
            "critical" => Some(Priority::Critical),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct NotificationAction {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum NotificationKind {
    /// Получено из ОС (Windows UserNotificationListener / D-Bus).
    System,
    Timer,
    Plugin,
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct IslandNotification {
    pub id: String,
    pub kind: NotificationKind,
    /// Идентификатор источника: имя приложения, id таймера, id плагина.
    pub source: String,
    pub app_name: String,
    pub icon: Option<String>,
    pub title: String,
    pub body: String,
    pub priority: Priority,
    pub timestamp: i64,
    pub actions: Vec<NotificationAction>,
    /// Цель для «открыть приложение» (AUMID, путь, desktop-id), если известна.
    pub open_target: Option<String>,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PushOutcome {
    /// Будет показано (сразу или после текущего).
    Queued,
    /// Вытеснило текущее уведомление (более высокий приоритет).
    Preempted,
    /// Только в историю (DND, игровой режим, отключённое приложение).
    HistoryOnly(&'static str),
    Dropped(&'static str),
}

#[derive(Debug, Default)]
pub struct QueueState {
    pub current: Option<IslandNotification>,
    pub queue: Vec<IslandNotification>,
    pub history: VecDeque<IslandNotification>,
    pub history_limit: usize,
    pub dnd: bool,
    pub dnd_allow_critical: bool,
    pub suppressed: bool,
    /// Сколько уведомлений пришло, пока панель была подавлена (игровой режим).
    pub missed_while_suppressed: u32,
}

pub struct RuleView<'a> {
    pub muted_apps: &'a [String],
    pub priority_overrides: &'a [(String, Priority)],
}

impl QueueState {
    pub fn new(history_limit: usize) -> Self {
        Self { history_limit, dnd_allow_critical: true, ..Default::default() }
    }

    fn add_history(&mut self, n: &IslandNotification) {
        if self.history_limit == 0 {
            return;
        }
        // Обновлённое уведомление (тот же id) заменяет старое.
        self.history.retain(|h| h.id != n.id);
        self.history.push_front(n.clone());
        while self.history.len() > self.history_limit {
            self.history.pop_back();
        }
    }

    fn is_duplicate(&self, n: &IslandNotification) -> bool {
        let same = |o: &IslandNotification| {
            o.source == n.source && o.title == n.title && o.body == n.body && (n.timestamp - o.timestamp).abs() < 2000
        };
        self.current.as_ref().map(same).unwrap_or(false) || self.queue.iter().any(same)
    }

    fn insert_sorted(&mut self, n: IslandNotification) {
        // Сортировка: приоритет по убыванию, затем время по возрастанию (FIFO внутри приоритета).
        let pos = self
            .queue
            .iter()
            .position(|o| o.priority < n.priority || (o.priority == n.priority && o.timestamp > n.timestamp))
            .unwrap_or(self.queue.len());
        self.queue.insert(pos, n);
    }

    pub fn push(&mut self, mut n: IslandNotification, rules: &RuleView) -> PushOutcome {
        let app_lower = n.app_name.to_lowercase();
        if rules.muted_apps.iter().any(|m| m.to_lowercase() == app_lower) {
            return PushOutcome::Dropped("приложение отключено");
        }
        if let Some((_, p)) = rules.priority_overrides.iter().find(|(a, _)| a.to_lowercase() == app_lower) {
            n.priority = *p;
        }
        if self.is_duplicate(&n) {
            return PushOutcome::Dropped("дубликат");
        }
        self.add_history(&n);

        if self.suppressed {
            self.missed_while_suppressed += 1;
            return PushOutcome::HistoryOnly("игровой режим");
        }
        if self.dnd && !(self.dnd_allow_critical && n.priority == Priority::Critical) {
            return PushOutcome::HistoryOnly("не беспокоить");
        }

        if let Some(cur) = &self.current {
            if n.priority == Priority::Critical && cur.priority < Priority::Critical {
                let prev = self.current.take().unwrap();
                self.insert_sorted(prev);
                self.current = Some(n);
                return PushOutcome::Preempted;
            }
        }
        self.insert_sorted(n);
        PushOutcome::Queued
    }

    /// Если ничего не показывается — берёт следующее из очереди.
    pub fn advance(&mut self) -> Option<&IslandNotification> {
        if self.current.is_none() && !self.queue.is_empty() {
            self.current = Some(self.queue.remove(0));
        }
        self.current.as_ref()
    }

    /// Закрывает уведомление по id. Возвращает true, если было закрыто текущее.
    pub fn dismiss(&mut self, id: &str) -> bool {
        if self.current.as_ref().map(|c| c.id == id).unwrap_or(false) {
            self.current = None;
            return true;
        }
        self.queue.retain(|n| n.id != id);
        false
    }

    pub fn clear_queue(&mut self) {
        self.queue.clear();
        self.current = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn n(id: &str, p: Priority, ts: i64) -> IslandNotification {
        IslandNotification {
            id: id.into(),
            kind: NotificationKind::Internal,
            source: format!("src-{id}"),
            app_name: "Test".into(),
            icon: None,
            title: format!("t{id}"),
            body: String::new(),
            priority: p,
            timestamp: ts,
            actions: vec![],
            open_target: None,
            duration_ms: 1000,
        }
    }
    const NO_RULES: RuleView = RuleView { muted_apps: &[], priority_overrides: &[] };

    #[test]
    fn priority_order_and_fifo() {
        let mut q = QueueState::new(10);
        q.push(n("a", Priority::Normal, 1), &NO_RULES);
        q.push(n("b", Priority::High, 2), &NO_RULES);
        q.push(n("c", Priority::Normal, 3), &NO_RULES);
        assert_eq!(q.advance().unwrap().id, "b");
        q.dismiss("b");
        assert_eq!(q.advance().unwrap().id, "a");
        q.dismiss("a");
        assert_eq!(q.advance().unwrap().id, "c");
    }

    #[test]
    fn critical_preempts_and_requeues() {
        let mut q = QueueState::new(10);
        q.push(n("a", Priority::Normal, 1), &NO_RULES);
        q.advance();
        assert_eq!(q.push(n("x", Priority::Critical, 2), &NO_RULES), PushOutcome::Preempted);
        assert_eq!(q.current.as_ref().unwrap().id, "x");
        q.dismiss("x");
        assert_eq!(q.advance().unwrap().id, "a");
    }

    #[test]
    fn dnd_and_suppression_go_to_history() {
        let mut q = QueueState::new(10);
        q.dnd = true;
        assert!(matches!(q.push(n("a", Priority::High, 1), &NO_RULES), PushOutcome::HistoryOnly(_)));
        assert!(matches!(q.push(n("b", Priority::Critical, 2), &NO_RULES), PushOutcome::Queued));
        q.dnd = false;
        q.suppressed = true;
        assert!(matches!(q.push(n("c", Priority::Critical, 3), &NO_RULES), PushOutcome::HistoryOnly(_)));
        assert_eq!(q.missed_while_suppressed, 1);
        assert_eq!(q.history.len(), 3);
    }

    #[test]
    fn muted_and_duplicates_dropped() {
        let mut q = QueueState::new(10);
        let muted = vec!["test".to_string()];
        let rules = RuleView { muted_apps: &muted, priority_overrides: &[] };
        assert!(matches!(q.push(n("a", Priority::Normal, 1), &rules), PushOutcome::Dropped(_)));
        q.push(n("b", Priority::Normal, 1), &NO_RULES);
        let mut dup = n("b2", Priority::Normal, 500);
        dup.source = "src-b".into();
        dup.title = "tb".into();
        assert!(matches!(q.push(dup, &NO_RULES), PushOutcome::Dropped(_)));
    }

    #[test]
    fn history_is_limited() {
        let mut q = QueueState::new(3);
        for i in 0..10 {
            q.push(n(&i.to_string(), Priority::Low, i * 10_000), &NO_RULES);
        }
        assert_eq!(q.history.len(), 3);
        assert_eq!(q.history[0].id, "9");
    }
}
