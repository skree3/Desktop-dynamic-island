//! Приветствие в острове: при запуске программы, пробуждении компьютера,
//! включении экрана и разблокировке. Текст («Доброе утро», «Добрый вечер»)
//! выбирает интерфейс по местному времени.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use crate::core::state::Shared;
use crate::database::now_ms;

pub const EVENT: &str = "island://greet";
static LAST: AtomicI64 = AtomicI64::new(0);
/// Не чаще раза в 20 минут — чтобы частые блокировки не надоедали.
const MIN_GAP_MS: i64 = 20 * 60_000;

pub fn greet(st: &Shared, reason: &'static str) {
    if !st.with_settings(|s| s.appearance.greeting) || st.game_mode_active() {
        return;
    }
    let now = now_ms();
    let last = LAST.load(Ordering::SeqCst);
    if last != 0 && now - last < MIN_GAP_MS {
        return;
    }
    LAST.store(now, Ordering::SeqCst);
    st.events.emit(EVENT, reason);
}

pub fn start(st: &Shared) {
    let first = st.clone();
    tauri::async_runtime::spawn(async move {
        // Даём острову отрисоваться после запуска.
        tokio::time::sleep(std::time::Duration::from_millis(2500)).await;
        greet(&first, "start");
    });
    let st2 = st.clone();
    let cb = Arc::new(move |reason: &'static str| {
        let st3 = st2.clone();
        // Экран и сеть просыпаются не сразу — приветствие чуть позже.
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
            greet(&st3, reason);
        });
    });
    #[cfg(windows)]
    crate::platform::os::session::watch(move |r| cb(r));
    #[cfg(target_os = "linux")]
    tauri::async_runtime::spawn(crate::platform::os::session::watch(cb));
}
