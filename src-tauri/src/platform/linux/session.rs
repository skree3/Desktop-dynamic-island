//! События сеанса Linux для приветствия: пробуждение (logind PrepareForSleep)
//! и снятие блокировки экрана (ScreenSaver ActiveChanged).

use std::sync::Arc;

use futures_util::StreamExt;

type Callback = Arc<dyn Fn(&'static str) + Send + Sync>;

async fn listen(conn: zbus::Connection, rule: &str, reason: &'static str, cb: Callback) {
    let Ok(rule) = zbus::MatchRule::try_from(rule) else { return };
    let Ok(mut stream) = zbus::MessageStream::for_match_rule(rule, &conn, None).await else { return };
    while let Some(Ok(msg)) = stream.next().await {
        // PrepareForSleep(false) — проснулись; ActiveChanged(false) — экран разблокирован.
        if let Ok(false) = msg.body().deserialize::<bool>() {
            cb(reason);
        }
    }
}

pub async fn watch(cb: Callback) {
    let mut tasks = Vec::new();
    if let Ok(sys) = zbus::Connection::system().await {
        tasks.push(tokio::spawn(listen(sys, "type='signal',interface='org.freedesktop.login1.Manager',member='PrepareForSleep'", "resume", cb.clone())));
    }
    if let Ok(ses) = zbus::Connection::session().await {
        for iface in ["org.freedesktop.ScreenSaver", "org.gnome.ScreenSaver", "org.cinnamon.ScreenSaver", "org.mate.ScreenSaver"] {
            let rule = format!("type='signal',interface='{iface}',member='ActiveChanged'");
            tasks.push(tokio::spawn({
                let (ses, cb) = (ses.clone(), cb.clone());
                async move { listen(ses, &rule, "unlock", cb).await }
            }));
        }
    }
    for t in tasks {
        let _ = t.await;
    }
}
