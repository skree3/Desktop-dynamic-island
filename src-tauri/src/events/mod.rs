//! Event Manager — единая точка, через которую проходят уведомления и
//! «активности» (музыка, таймеры, плагины). Гарантирует, что одновременные
//! события не накладываются: в каждый момент показывается одно уведомление,
//! остальные ждут в очереди по приоритету; активности сортируются по приоритету.

pub mod queue;

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::{AppHandle, Emitter};

pub use queue::{IslandNotification, NotificationAction, NotificationKind, Priority, PushOutcome, QueueState, RuleView};

/// Имена событий, отправляемых во фронтенд.
pub mod names {
    pub const SETTINGS_CHANGED: &str = "settings://changed";
    pub const NOTIFICATION: &str = "island://notification";
    pub const ACTIVITIES: &str = "island://activities";
    pub const COMMAND: &str = "island://command";
    pub const MEDIA_SESSIONS: &str = "media://sessions";
    pub const SYSMON_SNAPSHOT: &str = "sysmon://snapshot";
    pub const TIMERS_CHANGED: &str = "timers://changed";
    pub const CLIPBOARD_CHANGED: &str = "clipboard://changed";
    pub const GAME_MODE: &str = "game-mode://changed";
    pub const PLUGINS_CHANGED: &str = "plugins://changed";
    pub const THEME_CHANGED: &str = "theme://changed";
    pub const NOTIFICATIONS_STATUS: &str = "notifications://status";
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Activity {
    /// Уникальный ключ: "music", "timer:<id>", "plugin:<id>".
    pub id: String,
    pub kind: String,
    pub priority: Priority,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: Option<String>,
    /// Прогресс 0..1, если применимо.
    pub progress: Option<f64>,
    pub updated_at: i64,
    /// Начало длительного события (звонок): интерфейс показывает прошедшее время.
    #[serde(default)]
    pub started_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationPayload {
    pub current: Option<IslandNotification>,
    pub queued: usize,
    pub dnd: bool,
    pub missed: u32,
}

struct Inner {
    queue: QueueState,
    activities: Vec<Activity>,
    muted: Vec<String>,
    /// Приложения, для которых уведомления отключены в интеграциях.
    extra_muted: Vec<String>,
    overrides: Vec<(String, Priority)>,
    default_duration_ms: u64,
    /// Счётчик поколений для таймера автоскрытия: старые таймеры игнорируются.
    generation: u64,
}

pub struct EventManager {
    inner: Mutex<Inner>,
    app: OnceLock<AppHandle>,
}

impl EventManager {
    pub fn new(history_limit: usize) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(Inner {
                queue: QueueState::new(history_limit),
                activities: vec![],
                muted: vec![],
                extra_muted: vec![],
                overrides: vec![],
                default_duration_ms: 6000,
                generation: 0,
            }),
            app: OnceLock::new(),
        })
    }

    pub fn attach(&self, app: AppHandle) {
        let _ = self.app.set(app);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn emit<S: Serialize + Clone>(&self, event: &str, payload: S) {
        if let Some(app) = self.app.get() {
            if let Err(e) = app.emit(event, payload) {
                tracing::warn!(%event, error = %e, "не удалось отправить событие во фронтенд");
            }
        }
    }

    pub fn configure(&self, s: &crate::core::settings::NotificationSettings) {
        let mut g = self.lock();
        g.queue.history_limit = s.history_limit as usize;
        while g.queue.history.len() > g.queue.history_limit {
            g.queue.history.pop_back();
        }
        g.queue.dnd = s.dnd;
        g.queue.dnd_allow_critical = s.dnd_allow_critical;
        g.queue.group_by_app = s.group_by_app;
        g.default_duration_ms = s.duration_secs as u64 * 1000;
        g.muted = s.app_rules.iter().filter(|r| r.muted).map(|r| r.app.clone()).collect();
        g.overrides = s
            .app_rules
            .iter()
            .filter_map(|r| r.priority.as_deref().and_then(Priority::parse).map(|p| (r.app.clone(), p)))
            .collect();
        drop(g);
        self.broadcast_notification();
    }

    pub fn set_extra_muted(&self, apps: Vec<String>) {
        self.lock().extra_muted = apps;
    }

    pub fn dnd(&self) -> bool {
        self.lock().queue.dnd
    }

    pub fn set_dnd(&self, on: bool) {
        self.lock().queue.dnd = on;
        self.broadcast_notification();
    }

    /// Игровой режим: уведомления не показываются поверх игры, но сохраняются в истории.
    pub fn set_suppressed(self: &Arc<Self>, on: bool) {
        let mut g = self.lock();
        g.queue.suppressed = on;
        if on {
            // Текущее и ожидающие уведомления уходят в историю (они уже там).
            g.queue.clear_queue();
        }
        drop(g);
        self.broadcast_notification();
        if !on {
            self.pump();
        }
    }

    pub fn take_missed(&self) -> u32 {
        let mut g = self.lock();
        std::mem::take(&mut g.queue.missed_while_suppressed)
    }

    pub fn push(self: &Arc<Self>, mut n: IslandNotification) -> PushOutcome {
        let mut g = self.lock();
        if n.duration_ms == 0 {
            n.duration_ms = match n.priority {
                Priority::Critical => g.default_duration_ms * 2,
                _ => g.default_duration_ms,
            };
        }
        let mut muted = g.muted.clone();
        muted.extend(g.extra_muted.iter().cloned());
        let overrides = g.overrides.clone();
        let outcome = g.queue.push(n, &RuleView { muted_apps: &muted, priority_overrides: &overrides });
        let preempted = matches!(outcome, PushOutcome::Preempted | PushOutcome::Grouped(true));
        drop(g);
        if preempted {
            // Новое сообщение в показываемой группе — показываем его заново на полное время.
            self.start_timer();
            self.broadcast_notification();
        } else if outcome == PushOutcome::Grouped(false) {
            self.broadcast_notification();
        } else {
            self.pump();
        }
        outcome
    }

    /// Показывает следующее уведомление, если сейчас ничего не показывается.
    fn pump(self: &Arc<Self>) {
        let started = {
            let mut g = self.lock();
            let had = g.queue.current.is_some();
            let now = g.queue.advance().is_some();
            !had && now
        };
        if started {
            self.start_timer();
        }
        self.broadcast_notification();
    }

    fn start_timer(self: &Arc<Self>) {
        let (gen, id, dur) = {
            let mut g = self.lock();
            g.generation += 1;
            match &g.queue.current {
                Some(c) => (g.generation, c.id.clone(), c.duration_ms),
                None => return,
            }
        };
        let me = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(dur)).await;
            let expired = {
                let mut g = me.lock();
                if g.generation == gen && g.queue.current.as_ref().map(|c| c.id == id).unwrap_or(false) {
                    g.queue.current = None;
                    true
                } else {
                    false
                }
            };
            if expired {
                me.pump();
            }
        });
    }

    /// Пользователь навёл курсор/раскрыл уведомление — продлеваем показ.
    pub fn hold(self: &Arc<Self>, id: &str) {
        let matches = self.lock().queue.current.as_ref().map(|c| c.id == id).unwrap_or(false);
        if matches {
            self.lock().generation += 1; // отменяет текущий таймер
        }
    }

    pub fn release(self: &Arc<Self>, id: &str) {
        let matches = self.lock().queue.current.as_ref().map(|c| c.id == id).unwrap_or(false);
        if matches {
            self.start_timer();
        }
    }

    pub fn dismiss(self: &Arc<Self>, id: &str) {
        let was_current = self.lock().queue.dismiss(id);
        if was_current {
            self.pump();
        } else {
            self.broadcast_notification();
        }
    }

    /// Закрывает показываемые и ожидающие уведомления указанного источника.
    pub fn dismiss_source(self: &Arc<Self>, source: &str) {
        let ids: Vec<String> = {
            let g = self.lock();
            g.queue.current.iter().chain(g.queue.queue.iter()).filter(|n| n.source == source).map(|n| n.id.clone()).collect()
        };
        for id in ids {
            self.dismiss(&id);
        }
    }

    pub fn find(&self, id: &str) -> Option<IslandNotification> {
        let g = self.lock();
        g.queue
            .current
            .iter()
            .chain(g.queue.queue.iter())
            .chain(g.queue.history.iter())
            .find(|n| n.id == id)
            .cloned()
    }

    pub fn history(&self) -> Vec<IslandNotification> {
        self.lock().queue.history.iter().cloned().collect()
    }

    pub fn clear_history(&self) {
        self.lock().queue.history.clear();
    }

    pub fn remove_from_history(&self, id: &str) {
        self.lock().queue.history.retain(|n| n.id != id);
    }

    pub fn notification_payload(&self) -> NotificationPayload {
        let g = self.lock();
        NotificationPayload {
            current: g.queue.current.clone(),
            queued: g.queue.queue.len(),
            dnd: g.queue.dnd,
            missed: g.queue.missed_while_suppressed,
        }
    }

    fn broadcast_notification(&self) {
        let p = self.notification_payload();
        self.emit(names::NOTIFICATION, p);
    }

    // ---- активности ----

    pub fn set_activity(&self, a: Activity) {
        let changed = {
            let mut g = self.lock();
            if let Some(existing) = g.activities.iter_mut().find(|x| x.id == a.id) {
                if *existing == a {
                    false
                } else {
                    *existing = a;
                    true
                }
            } else {
                g.activities.push(a);
                true
            }
        };
        if changed {
            self.broadcast_activities();
        }
    }

    pub fn clear_activity(&self, id: &str) {
        let changed = {
            let mut g = self.lock();
            let before = g.activities.len();
            g.activities.retain(|a| a.id != id);
            before != g.activities.len()
        };
        if changed {
            self.broadcast_activities();
        }
    }

    pub fn clear_activities_with_prefix(&self, prefix: &str) {
        let changed = {
            let mut g = self.lock();
            let before = g.activities.len();
            g.activities.retain(|a| !a.id.starts_with(prefix));
            before != g.activities.len()
        };
        if changed {
            self.broadcast_activities();
        }
    }

    pub fn activities(&self) -> Vec<Activity> {
        let mut list = self.lock().activities.clone();
        list.sort_by(|a, b| b.priority.cmp(&a.priority).then(b.updated_at.cmp(&a.updated_at)));
        list
    }

    fn broadcast_activities(&self) {
        self.emit(names::ACTIVITIES, self.activities());
    }
}

/// Удобный конструктор внутренних уведомлений.
pub fn internal_notification(source: &str, app_name: &str, title: impl Into<String>, body: impl Into<String>, priority: Priority) -> IslandNotification {
    IslandNotification { stack: Vec::new(), count: 1,
        id: uuid::Uuid::new_v4().to_string(),
        kind: NotificationKind::Internal,
        source: source.into(),
        app_name: app_name.into(),
        icon: None,
        title: title.into(),
        body: body.into(),
        priority,
        timestamp: crate::database::now_ms(),
        actions: vec![],
        open_target: None,
        duration_ms: 0,
    }
}
