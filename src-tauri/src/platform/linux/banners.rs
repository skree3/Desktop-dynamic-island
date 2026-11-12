//! «Уведомления только в острове» в Linux: выключает всплывающие окна уведомлений
//! средствами самого окружения. Уведомления по-прежнему проходят через D-Bus,
//! поэтому остров их видит.
//! - GNOME: `org.gnome.desktop.notifications show-banners false` (то же, что «Не беспокоить»);
//! - XFCE: `xfce4-notifyd /do-not-disturb true`;
//! - KDE Plasma: org.freedesktop.Notifications.Inhibit — блокировка снимается сама,
//!   если остров закрылся или упал.
//!
//! Возвращаемый «токен» хранит прежнее значение для восстановления.

use std::process::Command;
use std::sync::Mutex;

fn desktop() -> String {
    std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().to_lowercase()
}

fn out(cmd: &str, args: &[&str]) -> Option<String> {
    let o = Command::new(cmd).args(args).output().ok()?;
    o.status.success().then(|| String::from_utf8_lossy(&o.stdout).trim().to_string())
}

fn ok(cmd: &str, args: &[&str]) -> bool {
    Command::new(cmd).args(args).status().map(|s| s.success()).unwrap_or(false)
}

/// Соединение с удержанной блокировкой KDE и её номер.
static KDE: Mutex<Option<(zbus::blocking::Connection, u32)>> = Mutex::new(None);

/// Прячет всплывающие уведомления. Возвращает токен для восстановления
/// (None — окружение не поддерживается).
pub fn hide_popups() -> Option<String> {
    let d = desktop();
    if d.contains("kde") {
        let conn = zbus::blocking::Connection::session().ok()?;
        let reply = conn
            .call_method(
                Some("org.freedesktop.Notifications"),
                "/org/freedesktop/Notifications",
                Some("org.freedesktop.Notifications"),
                "Inhibit",
                &("desktop-island", "Уведомления показываются в Desktop Island", std::collections::HashMap::<&str, zbus::zvariant::Value>::new()),
            )
            .ok()?;
        let cookie: u32 = reply.body().deserialize().ok()?;
        *KDE.lock().unwrap_or_else(|p| p.into_inner()) = Some((conn, cookie));
        return Some("kde".into());
    }
    if d.contains("xfce") {
        let prev = out("xfconf-query", &["-c", "xfce4-notifyd", "-p", "/do-not-disturb"]).unwrap_or_else(|| "false".into());
        return ok("xfconf-query", &["-c", "xfce4-notifyd", "-p", "/do-not-disturb", "-n", "-t", "bool", "-s", "true"]).then(|| format!("xfce:{prev}"));
    }
    // GNOME, Ubuntu, Budgie, Cinnamon используют схему GNOME.
    let prev = out("gsettings", &["get", "org.gnome.desktop.notifications", "show-banners"])?;
    ok("gsettings", &["set", "org.gnome.desktop.notifications", "show-banners", "false"]).then(|| format!("gnome:{prev}"))
}

/// Возвращает всплывающие уведомления как было.
pub fn restore(token: &str) {
    if token == "kde" {
        if let Some((conn, cookie)) = KDE.lock().unwrap_or_else(|p| p.into_inner()).take() {
            let _ = conn.call_method(
                Some("org.freedesktop.Notifications"),
                "/org/freedesktop/Notifications",
                Some("org.freedesktop.Notifications"),
                "UnInhibit",
                &(cookie,),
            );
        }
    } else if let Some(prev) = token.strip_prefix("xfce:") {
        let v = if prev.trim() == "true" { "true" } else { "false" };
        let _ = ok("xfconf-query", &["-c", "xfce4-notifyd", "-p", "/do-not-disturb", "-n", "-t", "bool", "-s", v]);
    } else if let Some(prev) = token.strip_prefix("gnome:") {
        let v = if prev.trim() == "false" { "false" } else { "true" };
        let _ = ok("gsettings", &["set", "org.gnome.desktop.notifications", "show-banners", v]);
    }
}
