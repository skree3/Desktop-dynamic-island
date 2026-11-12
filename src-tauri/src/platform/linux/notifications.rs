//! Получение уведомлений других приложений в Linux: мониторинг сессионной шины
//! D-Bus (BecomeMonitor, тот же механизм, что у dbus-monitor). Сами уведомления
//! по-прежнему показывает системный сервер уведомлений.
//!
//! Приложения отправляют уведомления тремя путями, слушаем все:
//! - `org.freedesktop.Notifications.Notify` — классический (libnotify, Electron, Qt);
//! - `org.gtk.Notifications.AddNotification` — GNotification в GNOME;
//! - `org.freedesktop.portal.Notification.AddNotification` — Flatpak и Snap.
//!
//! Портал пересылает уведомление дальше одним из первых двух способов, поэтому
//! одинаковые уведомления, пришедшие подряд, склеиваются.
//!
//! Ограничения: действия уведомлений вызвать нельзя (их обрабатывает сервер
//! уведомлений), в песочнице Flatpak мониторинг шины запрещён.

use futures_util::StreamExt;
use std::collections::{HashMap, VecDeque};
use tokio::sync::mpsc;
use zbus::zvariant::OwnedValue;

use crate::database::now_ms;
use crate::platform::types::{ListenerStatus, SystemNotification};

const BACKEND: &str = "D-Bus (Notifications, GNotification, портал)";

const FDO: &str = "org.freedesktop.Notifications";
const GTK: &str = "org.gtk.Notifications";
const PORTAL: &str = "org.freedesktop.portal.Notification";

pub fn status_ok() -> ListenerStatus {
    ListenerStatus { available: true, active: true, backend: BACKEND.into(), message: "Получение уведомлений через D-Bus активно".into() }
}

pub fn status_err(msg: String) -> ListenerStatus {
    ListenerStatus { available: false, active: false, backend: BACKEND.into(), message: msg }
}

type Dict = HashMap<String, OwnedValue>;

/// Уведомление в общем виде, до того как стало понятно, от какого оно приложения.
#[derive(Debug, Default, PartialEq)]
struct Raw {
    app_name: String,
    app_id: Option<String>,
    icon: Option<String>,
    title: String,
    body: String,
}

fn dict_str(d: &Dict, key: &str) -> Option<String> {
    d.get(key).and_then(|v| super::val_str(v)).filter(|s| !s.is_empty())
}

fn parse(msg: &zbus::Message) -> Option<Raw> {
    let header = msg.header();
    let member = header.member()?.as_str().to_string();
    let iface = header.interface()?.as_str().to_string();
    match (iface.as_str(), member.as_str()) {
        (FDO, "Notify") => {
            type Body = (String, u32, String, String, String, Vec<String>, Dict, i32);
            let (app_name, _replaces, icon, summary, body, _actions, hints, _timeout) = msg.body().deserialize::<Body>().ok()?;
            Some(Raw {
                app_name,
                app_id: dict_str(&hints, "desktop-entry"),
                icon: Some(icon).filter(|i| !i.is_empty() && !i.starts_with('/') && !i.starts_with("file:")),
                title: summary,
                body: strip_markup(&body),
            })
        }
        (GTK, "AddNotification") => {
            let (app_id, _id, n) = msg.body().deserialize::<(String, String, Dict)>().ok()?;
            Some(Raw {
                app_name: String::new(),
                icon: Some(app_id.clone()).filter(|s| !s.is_empty()),
                app_id: Some(app_id).filter(|s| !s.is_empty()),
                title: dict_str(&n, "title").unwrap_or_default(),
                body: strip_markup(&dict_str(&n, "body").unwrap_or_default()),
            })
        }
        (PORTAL, "AddNotification") => {
            let (_id, n) = msg.body().deserialize::<(String, Dict)>().ok()?;
            Some(Raw {
                title: dict_str(&n, "title").unwrap_or_default(),
                body: strip_markup(&dict_str(&n, "body").unwrap_or_default()),
                ..Default::default()
            })
        }
        _ => None,
    }
}

/// Имя приложения из .desktop-файла (Name=) по его идентификатору.
fn desktop_name(app_id: &str) -> Option<String> {
    let id = app_id.trim_end_matches(".desktop");
    let mut dirs: Vec<std::path::PathBuf> = vec![];
    if let Ok(home) = std::env::var("HOME") {
        dirs.push(format!("{home}/.local/share/applications").into());
        dirs.push(format!("{home}/.local/share/flatpak/exports/share/applications").into());
    }
    let data = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    dirs.extend(data.split(':').filter(|d| !d.is_empty()).map(|d| std::path::Path::new(d).join("applications")));
    dirs.push("/var/lib/flatpak/exports/share/applications".into());
    dirs.push("/var/lib/snapd/desktop/applications".into());
    for d in dirs {
        let Ok(text) = std::fs::read_to_string(d.join(format!("{id}.desktop"))) else { continue };
        let mut in_entry = false;
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                in_entry = line == "[Desktop Entry]";
            } else if in_entry {
                if let Some(name) = line.strip_prefix("Name=") {
                    return Some(name.to_string());
                }
            }
        }
    }
    None
}

/// Приложение-отправитель по процессу: идентификатор Flatpak или имя программы.
fn sender_app(pid: u32) -> (Option<String>, Option<String>) {
    if let Ok(info) = std::fs::read_to_string(format!("/proc/{pid}/root/.flatpak-info")) {
        if let Some(id) = info.lines().find_map(|l| l.trim().strip_prefix("name=")) {
            return (Some(id.to_string()), None);
        }
    }
    if let Ok(cg) = std::fs::read_to_string(format!("/proc/{pid}/cgroup")) {
        // snap.telegram-desktop.telegram-desktop-<uuid>.scope
        if let Some(pos) = cg.find("snap.") {
            let rest = &cg[pos + 5..];
            if let Some(name) = rest.split('.').next().filter(|s| !s.is_empty()) {
                return (Some(format!("{name}_{name}")), Some(name.to_string()));
            }
        }
    }
    let comm = super::read_trim(format!("/proc/{pid}/comm")).filter(|s| !s.is_empty());
    (None, comm)
}

/// Красивое имя из идентификатора: org.telegram.desktop → Telegram.
fn pretty_from_id(id: &str) -> String {
    let parts: Vec<&str> = id.trim_end_matches(".desktop").split(['.', '_', '-']).collect();
    let skip = ["org", "com", "io", "net", "app", "desktop", "client", "github", "gitlab", "flathub"];
    let word = parts.iter().copied().filter(|p| !p.is_empty() && !skip.contains(&p.to_lowercase().as_str())).next().unwrap_or(id);
    let mut c = word.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

struct Resolver {
    dbus: Option<zbus::fdo::DBusProxy<'static>>,
    cache: HashMap<String, (Option<String>, Option<String>)>,
}

impl Resolver {
    async fn by_sender(&mut self, sender: &str) -> (Option<String>, Option<String>) {
        if let Some(hit) = self.cache.get(sender) {
            return hit.clone();
        }
        let mut res = (None, None);
        if let Some(proxy) = &self.dbus {
            if let Ok(name) = zbus::names::BusName::try_from(sender.to_string()) {
                if let Ok(pid) = proxy.get_connection_unix_process_id(name).await {
                    res = sender_app(pid);
                }
            }
        }
        if self.cache.len() > 256 {
            self.cache.clear();
        }
        self.cache.insert(sender.to_string(), res.clone());
        res
    }
}

/// Работает до ошибки или закрытия канала. Отдельное соединение обязательно:
/// после BecomeMonitor соединение нельзя использовать для обычных вызовов.
pub async fn run(tx: mpsc::Sender<SystemNotification>, ready: tokio::sync::oneshot::Sender<ListenerStatus>) {
    let setup = async {
        let conn = zbus::connection::Builder::session()?.build().await?;
        let rules = [
            zbus::MatchRule::try_from(format!("type='method_call',interface='{FDO}',member='Notify'").as_str())?.into_owned(),
            zbus::MatchRule::try_from(format!("type='method_call',interface='{GTK}',member='AddNotification'").as_str())?.into_owned(),
            zbus::MatchRule::try_from(format!("type='method_call',interface='{PORTAL}',member='AddNotification'").as_str())?.into_owned(),
        ];
        let monitor = zbus::fdo::MonitoringProxy::new(&conn).await?;
        monitor.become_monitor(&rules, 0).await?;
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
    // Обычное соединение для вопросов к шине: какой процесс отправил уведомление.
    let dbus = match zbus::Connection::session().await {
        Ok(c) => zbus::fdo::DBusProxy::new(&c).await.ok(),
        Err(_) => None,
    };
    let mut resolver = Resolver { dbus, cache: HashMap::new() };
    let mut recent: VecDeque<(String, String, i64)> = VecDeque::new();
    let mut stream = zbus::MessageStream::from(&conn);
    let mut counter: u64 = 0;
    while let Some(msg) = stream.next().await {
        let Ok(msg) = msg else { continue };
        let Some(mut raw) = parse(&msg) else { continue };
        if raw.title.is_empty() && raw.body.is_empty() {
            continue;
        }
        let now = now_ms();
        // Портал пересылает то же уведомление серверу: второй экземпляр не нужен.
        recent.retain(|(_, _, t)| now - t < 4000);
        if recent.iter().any(|(t, b, _)| *t == raw.title && *b == raw.body) {
            continue;
        }
        recent.push_back((raw.title.clone(), raw.body.clone(), now));

        if raw.app_name.is_empty() || raw.app_id.is_none() {
            let sender = msg.header().sender().map(|s| s.to_string());
            let (flatpak_id, comm) = match sender {
                Some(s) => resolver.by_sender(&s).await,
                None => (None, None),
            };
            if raw.app_id.is_none() {
                raw.app_id = flatpak_id.clone();
            }
            if raw.icon.is_none() {
                raw.icon = raw.app_id.clone();
            }
            if raw.app_name.is_empty() {
                raw.app_name = comm.filter(|c| raw.app_id.is_none() && !c.is_empty()).unwrap_or_default();
            }
        }
        if raw.app_name.is_empty() {
            if let Some(id) = raw.app_id.as_deref() {
                raw.app_name = desktop_name(id).unwrap_or_else(|| pretty_from_id(id));
            }
        }
        counter += 1;
        let n = SystemNotification {
            key: format!("dbus-{counter}-{now}"),
            app_name: if raw.app_name.is_empty() { "Приложение".into() } else { raw.app_name },
            app_id: raw.app_id,
            title: raw.title,
            body: raw.body,
            icon: raw.icon,
            timestamp: now,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_from_ids() {
        assert_eq!(pretty_from_id("org.telegram.desktop"), "Telegram");
        assert_eq!(pretty_from_id("com.discordapp.Discord"), "Discordapp");
        assert_eq!(pretty_from_id("telegram-desktop_telegram-desktop"), "Telegram");
        assert_eq!(strip_markup("<b>Hi</b> &amp; bye"), "Hi & bye");
    }
}
