//! Модуль уведомлений: получение уведомлений ОС (если система это позволяет)
//! и передача их в Event Manager. Внутренние уведомления (таймеры, плагины,
//! системные события Desktop Island) работают независимо от доступности API ОС.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::AppHandle;

use crate::core::state::Shared;
use crate::core::task::TaskSlot;
use crate::events::{names, IslandNotification, NotificationKind, Priority, PushOutcome};
use crate::logging::category;
use crate::platform::types::{ListenerStatus, SystemNotification};

#[cfg(windows)]
mod unread;

#[derive(Default)]
pub struct NotificationsModule {
    task: TaskSlot,
    status: Mutex<ListenerStatus>,
    /// API недоступен — не повторяем попытки до явного запроса пользователя.
    gave_up: AtomicBool,
    /// Токен скрытых системных баннеров: Linux — см. platform::linux::banners,
    /// Windows — «win:<прежний режим Не беспокоить>».
    popups: Mutex<Option<String>>,
}

/// Уведомления безопасности Windows всегда остаются системными.
#[allow(dead_code)]
fn protected_app(app: &str) -> bool {
    let a = app.to_lowercase();
    ["безопасность", "security", "defender", "защитник"].iter().any(|k| a.contains(k))
}

/// Windows: «Не беспокоить», пока работает остров. Если есть приложения-исключения,
/// включается «Только приоритет», а они добавляются в приоритетные (и потом убираются).
/// Токен: `win:<прежний режим>|<добавленные приложения через TAB>`.
#[cfg(windows)]
fn hide_system_popups(st: &Shared) -> Option<String> {
    use crate::platform::os::quiet;
    // Пользователь уже сам включил «Не беспокоить» — ничего не трогаем и потом не выключаем.
    if quiet::get() != Some(0) {
        return None;
    }
    let allow = st.with_settings(|s| s.notifications.dnd_allow.clone());
    let (mode, added) = if allow.is_empty() { (quiet::ALARMS_ONLY, vec![]) } else { (quiet::PRIORITY_ONLY, quiet::allow_apps(&allow)) };
    if !quiet::set(mode) {
        quiet::disallow_apps(&added);
        return None;
    }
    Some(format!("win:0|{}", added.join(";")))
}

#[cfg(windows)]
fn restore_system_popups(token: &str) {
    use crate::platform::os::quiet;
    let Some(rest) = token.strip_prefix("win:") else { return };
    let (prev, added) = rest.split_once('|').unwrap_or((rest, ""));
    let added: Vec<String> = added.split(';').filter(|a| !a.is_empty()).map(String::from).collect();
    quiet::disallow_apps(&added);
    if let Ok(prev) = prev.parse::<u32>() {
        // Вернуть, только если режим не меняли вручную, пока работал остров.
        if matches!(quiet::get(), Some(quiet::ALARMS_ONLY) | Some(quiet::PRIORITY_ONLY)) {
            quiet::set(prev);
        }
    }
}

#[cfg(target_os = "linux")]
fn hide_system_popups(_st: &Shared) -> Option<String> {
    crate::platform::linux::banners::hide_popups()
}

#[cfg(target_os = "linux")]
fn restore_system_popups(token: &str) {
    crate::platform::linux::banners::restore(token)
}

fn convert(n: SystemNotification, show_body: bool) -> IslandNotification {
    IslandNotification { stack: Vec::new(), count: 1,
        id: uuid::Uuid::new_v4().to_string(),
        kind: NotificationKind::System,
        source: format!("system:{}", n.key),
        app_name: n.app_name,
        icon: n.icon,
        title: crate::util::clean_text(&n.title, 200),
        body: if show_body { crate::util::clean_text(&n.body, 1000) } else { String::new() },
        priority: Priority::Normal,
        timestamp: if n.timestamp > 0 { n.timestamp } else { crate::database::now_ms() },
        actions: vec![],
        open_target: n.app_id,
        duration_ms: 0,
    }
}

impl NotificationsModule {
    pub fn status(&self) -> ListenerStatus {
        self.status.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    fn set_status(&self, st: &Shared, s: ListenerStatus) {
        *self.status.lock().unwrap_or_else(|p| p.into_inner()) = s.clone();
        st.events.emit(names::NOTIFICATIONS_STATUS, s);
    }

    pub fn retry(&self, app: &AppHandle, st: &Shared) {
        self.gave_up.store(false, Ordering::SeqCst);
        self.task.stop();
        crate::modules::reconcile(app, st);
    }

    /// Прячет или возвращает системные всплывающие уведомления (баннеры и их звук).
    /// Windows — режим «Не беспокоить», Linux — настройки окружения рабочего стола.
    /// Прежнее состояние хранится в файле — после сбоя оно восстанавливается при запуске.
    pub fn sync_popups(&self, st: &Shared, desired: bool) {
        let backup = st.paths.data.join("popups-backup.txt");
        let mut g = self.popups.lock().unwrap_or_else(|p| p.into_inner());
        if g.is_none() {
            if let Ok(token) = std::fs::read_to_string(&backup) {
                // Блокировка KDE умерла вместе с прошлым процессом — восстанавливать нечего.
                if token.trim() == "kde" {
                    let _ = std::fs::remove_file(&backup);
                } else {
                    *g = Some(token.trim().to_string());
                }
            }
        }
        match (desired, g.clone()) {
            (true, None) => {
                if let Some(token) = hide_system_popups(st) {
                    let _ = std::fs::write(&backup, &token);
                    st.log.info(category::SYSTEM_ACTION, "notifications", "системные баннеры скрыты — уведомления показывает остров");
                    *g = Some(token);
                }
            }
            (false, Some(token)) => {
                restore_system_popups(&token);
                let _ = std::fs::remove_file(&backup);
                st.log.info(category::SYSTEM_ACTION, "notifications", "системные баннеры возвращены");
                *g = None;
            }
            _ => {}
        }
    }

    /// «Только в острове» действует, пока остров виден (скрыл его пользователь —
    /// уведомления снова показывает система).
    pub fn island_only_active(&self, st: &Shared) -> bool {
        st.with_settings(|s| s.notifications.island_only) && st.island_visible()
    }

    fn deliver(st: &Shared, #[allow(unused_mut)] mut n: SystemNotification) -> PushOutcome {
        // Linux: имя иконки темы (например, "telegram") → картинка.
        #[cfg(target_os = "linux")]
        if let Some(name) = n.icon.clone().filter(|i| !i.starts_with("data:")) {
            let target = crate::platform::types::LaunchTarget::Uri { uri: String::new() };
            n.icon = crate::platform::os::apps::icon(&target, Some(&name), 48).ok().flatten();
        }
        let show_body = st.with_settings(|s| s.notifications.show_body);
        st.events.push(convert(n, show_body))
    }

    pub fn set_running(&self, app: &AppHandle, st: &Shared, run: bool) {
        if !run {
            if self.task.is_running() {
                self.task.stop();
            }
            if !self.gave_up.load(Ordering::SeqCst) {
                self.set_status(st, ListenerStatus { message: "Получение системных уведомлений отключено в настройках".into(), ..self.status() });
            }
            return;
        }
        if self.task.is_running() || self.gave_up.load(Ordering::SeqCst) {
            return;
        }
        let _ = app;
        let st = st.clone();
        #[cfg(windows)]
        self.task.start(async move {
            use crate::platform::os::notifications as win;
            let opened = tauri::async_runtime::spawn_blocking(win::open).await;
            // Без доступа к центру уведомлений продолжаем работать: счётчик
            // непрочитанных Telegram читается из заголовка окна и без него.
            let mut listener = match opened {
                Ok(Ok(l)) => {
                    st.notifications.set_status(&st, l.status());
                    Some(l)
                }
                Ok(Err(status)) => {
                    st.log.warn(category::INTEGRATION, "notifications", &status.message);
                    st.notifications.set_status(&st, status);
                    None
                }
                Err(_) => return,
            };
            let mut tracker = self::unread::Tracker::default();
            let mut first = true;
            loop {
                // «Только в острове»: уведомление показывается в острове и убирается из
                // центра уведомлений Windows, чтобы не копилось там второй раз.
                let island_only = st.notifications.island_only_active(&st);
                let st2 = st.clone();
                let res = tauri::async_runtime::spawn_blocking(move || {
                    let now = crate::database::now_ms();
                    let mut err = None;
                    match listener.as_mut().map(|l| l.poll(first)) {
                        Some(Ok(list)) => {
                            for n in list {
                                tracker.native(&n.app_name, now);
                                let (key, app) = (n.key.clone(), n.app_name.clone());
                                let outcome = Self::deliver(&st2, n);
                                if island_only && !matches!(outcome, PushOutcome::Dropped(_)) && !protected_app(&app) {
                                    if let Some(l) = listener.as_ref() {
                                        let _ = l.remove(&key);
                                    }
                                }
                            }
                        }
                        Some(Err(e)) => err = Some(e.to_string()),
                        None => {}
                    }
                    for n in tracker.update(crate::platform::os::unread::scan(), now) {
                        Self::deliver(&st2, n);
                    }
                    (listener, tracker, err)
                })
                .await;
                let Ok((l, t, err)) = res else { return };
                (listener, tracker) = (l, t);
                first = false;
                if let Some(e) = err {
                    st.log.warn(category::INTEGRATION, "notifications", format!("ошибка чтения уведомлений: {e}"));
                }
                // Баннеры в режиме «только в острове» гасит «Не беспокоить», поэтому
                // частый опрос нужен только для быстроты появления в острове.
                tokio::time::sleep(std::time::Duration::from_millis(if island_only { 600 } else { 1500 })).await;
            }
        });
        #[cfg(target_os = "linux")]
        self.task.start(async move {
            use crate::platform::os::notifications as lin;
            let (tx, mut rx) = tokio::sync::mpsc::channel::<SystemNotification>(64);
            let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
            let monitor = tauri::async_runtime::spawn(lin::run(tx, ready_tx));
            match ready_rx.await {
                Ok(status) => {
                    let ok = status.available;
                    if !ok {
                        st.log.warn(category::INTEGRATION, "notifications", &status.message);
                        st.notifications.gave_up.store(true, Ordering::SeqCst);
                    }
                    st.notifications.set_status(&st, status);
                    if !ok {
                        return;
                    }
                }
                Err(_) => return,
            }
            while let Some(n) = rx.recv().await {
                Self::deliver(&st, n);
            }
            monitor.abort();
        });
    }

    /// Обработка действий над уведомлением.
    pub fn action(&self, app: &AppHandle, st: &Shared, notification_id: &str, action_id: &str) -> crate::core::error::AppResult<()> {
        let n = st.events.find(notification_id).ok_or_else(|| crate::core::error::AppError::NotFound("уведомление не найдено".into()))?;
        match (n.kind.clone(), action_id) {
            (_, "dismiss") => {}
            (NotificationKind::Timer, "restart") => {
                if let Some(id) = n.source.strip_prefix("timer:") {
                    st.timers.start(st, id)?;
                }
            }
            (NotificationKind::Internal, "update") if n.source == "updater" => {
                let (app, st) = (app.clone(), st.clone());
                tauri::async_runtime::spawn_blocking(move || {
                    if let Err(e) = crate::modules::updater::install_and_quit(&app, &st) {
                        st.events.push(crate::events::internal_notification("updater", "Desktop Island", "Обновление не установлено", e.to_string(), Priority::High));
                    }
                });
            }
            (NotificationKind::Internal, a) if n.source.starts_with("download:") => {
                crate::modules::downloads::action(st, &n.source, a)?;
            }
            (NotificationKind::Internal, a) if n.source.starts_with("phone") => {
                st.phone.action(st, &n.source, a)?;
            }
            (NotificationKind::Internal, a) if n.source.starts_with("note:") => {
                crate::modules::notes::reminder_action(st, &n.source, a)?;
            }
            (_, "open") => {
                if let Some(target) = &n.open_target {
                    crate::modules::applications::open_by_hint(st, target, &n.app_name)?;
                } else {
                    crate::modules::applications::open_by_hint(st, &n.app_name, &n.app_name)?;
                }
            }
            (NotificationKind::Plugin, a) => {
                if let Some(pid) = n.source.strip_prefix("plugin:") {
                    st.plugins.forward_action(app, pid, &n.id, a);
                }
            }
            _ => return Err(crate::core::error::AppError::unsupported("действие не поддерживается")),
        }
        st.events.dismiss(notification_id);
        Ok(())
    }
}
