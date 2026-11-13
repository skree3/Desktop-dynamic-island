//! Полноэкранное видео или программа на переднем плане — остров сжимается
//! в тонкую полоску (наведите на неё, чтобы вернуть остров). Игры по-прежнему
//! обрабатывает игровой режим.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::core::state::Shared;
use crate::core::task::TaskSlot;

pub const EVENT: &str = "island://strip";

#[derive(Default)]
pub struct FullscreenModule {
    task: TaskSlot,
    strip: AtomicBool,
}

impl FullscreenModule {
    pub fn strip(&self) -> bool {
        self.strip.load(Ordering::SeqCst)
    }

    pub fn set_running(&self, st: &Shared, run: bool) {
        if !run {
            if self.task.is_running() {
                self.task.stop();
            }
            if self.strip.swap(false, Ordering::SeqCst) {
                st.events.emit(EVENT, false);
            }
            return;
        }
        if self.task.is_running() {
            return;
        }
        let st = st.clone();
        self.task.start(async move {
            let me = std::process::id();
            loop {
                tokio::time::sleep(Duration::from_millis(900)).await;
                let fg = tauri::async_runtime::spawn_blocking(crate::platform::os::foreground::current).await.ok().flatten();
                let full = fg.map(|f| f.fullscreen && !f.is_shell && f.pid != me).unwrap_or(false) && !st.game_mode_active();
                if st.fullscreen.strip.swap(full, Ordering::SeqCst) != full {
                    st.events.emit(EVENT, full);
                }
            }
        });
    }
}
