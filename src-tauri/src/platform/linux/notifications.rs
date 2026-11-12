//! Получение уведомлений других приложений в Linux: мониторинг вызовов
//! org.freedesktop.Notifications.Notify на сессионной шине (BecomeMonitor).
//! Это штатный механизм D-Bus (используется dbus-monitor). Сами уведомления
//! по-прежнему показывает системный сервер уведомлений.
//!
//! Ограничения: действия уведомлений вызвать нельзя (их обрабатывает сервер
//! уведомлений), в песочнице Flatpak мониторинг шины запрещён.

use futures_util::StreamExt;
use std::collections::HashMap;
use tokio::sync::mpsc;
use zbus::zvariant::OwnedValue;

use crate::database::now_ms;
use crate::platform::types::{ListenerStatus, SystemNotification};

const BACKEND: &str = "D-Bus org.freedesktop.Notifications (мониторинг)";

pub fn status_ok() -> ListenerStatus {
    ListenerStatus { available: true, active: true, backend: BACKEND.into(), message: "Получение уведомлений через D-Bus активно".into() }
}

pub fn status_err(msg: String) -> ListenerStatus {
    ListenerStatus { available: false, active: false, backend: BACKEND.into(), message: msg }
}

/// Работает до ошибки или закрытия канала. Отдельное соединение обязательно:
/// после BecomeMonitor соединение нельзя использовать для обычных вызовов.
pub async fn run(tx: mpsc::Sender<SystemNotification>, ready: tokio::sync::oneshot::Sender<ListenerStatus>) {
    let setup = async {
        let conn = zbus::connection::Builder::session()?.build().await?;
        let rule = zbus::MatchRule::try_from("type='method_call',interface='org.freedesktop.Notifications',member='Notify'")?;
        let monitor = zbus::fdo::MonitoringProxy::new(&conn).await?;
        monitor.become_monitor(&[rule], 0).await?;
        Ok::<_, zbus::Error>(conn)
    };
    let conn = match setup.await {
        Ok(c) => {
            let _ = ready.send(status_ok());
            c
        }
        Err(e) => {
            let _ = ready.send(status_err(format!("Мониторинг уведомлений D-Bus недоступен: {e}")));
            return;
        }
    };
    let mut stream = zbus::MessageStream::from(&conn);
    let mut counter: u64 = 0;
    while let Some(msg) = stream.next().await {
        let Ok(msg) = msg else { continue };
        let header = msg.header();
        if header.member().map(|m| m.as_str()) != Some("Notify") {
            continue;
        }
        type Body = (String, u32, String, String, String, Vec<String>, HashMap<String, OwnedValue>, i32);
        let Ok((app_name, _replaces, icon, summary, body, _actions, hints, _timeout)) = msg.body().deserialize::<Body>() else {
            continue;
        };
        let desktop_entry = hints.get("desktop-entry").and_then(|v| super::val_str(v));
        counter += 1;
        let n = SystemNotification {
            key: format!("dbus-{counter}-{}", now_ms()),
            app_name: if app_name.is_empty() { "Приложение".into() } else { app_name },
            app_id: desktop_entry,
            title: summary,
            body: strip_markup(&body),
            icon: if icon.starts_with('/') || icon.is_empty() { None } else { Some(icon) },
            timestamp: now_ms(),
        };
        if tx.send(n).await.is_err() {
            break;
        }
    }
}

/// Тело уведомления может содержать простую разметку (<b>, <i>, <a>).
fn strip_markup(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"")
}
