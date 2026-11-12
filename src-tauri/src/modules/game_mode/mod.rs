//! Игровой режим: автоматически скрывает панель во время игр и полноэкранных
//! приложений, подавляет всплывающие уведомления (они сохраняются в истории),
//! приостанавливает лишние опросы. Приложение не завершается; фоновые функции
//! (таймеры, буфер обмена, горячие клавиши) продолжают работать.

pub mod detect;

use serde::Serialize;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::AppHandle;
use tokio::sync::Notify;

use crate::core::state::Shared;
use crate::core::task::TaskSlot;
use crate::events::{internal_notification, names, Priority};
use crate::logging::category;

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct GameModeState {
    pub active: bool,
    /// manual | game | fullscreen | exclusive
    pub reason: Option<String>,
    pub app: Option<String>,
}

pub struct GameModeModule {
    task: TaskSlot,
    wake: Arc<Notify>,
    state: Mutex<GameModeState>,
}

impl Default for GameModeModule {
    fn default() -> Self {
        Self { task: TaskSlot::new(), wake: Arc::new(Notify::new()), state: Mutex::new(GameModeState::default()) }
    }
}

impl GameModeModule {
    pub fn state(&self) -> GameModeState {
        self.state.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    pub fn force_check(&self) {
        self.wake.notify_one();
    }

    pub fn ensure_running(&self, app: &AppHandle, st: &Shared) {
        if self.task.is_running() {
            return;
        }
        let app = app.clone();
        let st = st.clone();
        let wake = self.wake.clone();
        self.task.start(async move {
            let mut detector = detect::Detector::new();
            let mut tick: u64 = 0;
            loop {
                tick += 1;
                let cfg = st.with_settings(|s| s.game_mode.clone());
                let next = match cfg.manual {
                    Some(true) => GameModeState { active: true, reason: Some("manual".into()), app: None },
                    Some(false) => GameModeState::default(),
                    None if !cfg.auto => GameModeState::default(),
                    None => {
                        let cfg2 = cfg.clone();
                        let d = std::mem::take(&mut detector);
                        match tauri::async_runtime::spawn_blocking(move || {
                            let mut d = d;
                            let r = d.detect(&cfg2, tick);
                            (d, r)
                        })
                        .await
                        {
                            Ok((d, r)) => {
                                detector = d;
                                r
                            }
                            Err(_) => GameModeState::default(),
                        }
                    }
                };
                let prev = st.game_mode.state();
                if prev.active != next.active || (next.active && prev.app != next.app) {
                    apply(&app, &st, next, prev.active);
                }
                // Правила профилей (батарея / время) — раз в 30 секунд.
                if tick % 15 == 1 {
                    let on_battery = tauri::async_runtime::spawn_blocking(|| crate::platform::os::power::state().ok())
                        .await
                        .ok()
                        .flatten()
                        .and_then(|p| p.on_ac.map(|ac| !ac && p.has_battery));
                    crate::core::profiles::evaluate_rules(&app, &st, on_battery);
                }
                let interval = if st.game_mode_active() { cfg.check_interval_ms.max(2000) } else { cfg.check_interval_ms };
                tokio::select! {
                    _ = tokio::time::sleep(Duration::from_millis(interval)) => {}
                    _ = wake.notified() => {}
                }
            }
        });
    }
}

fn apply(app: &AppHandle, st: &Shared, next: GameModeState, was_active: bool) {
    *st.game_mode.state.lock().unwrap_or_else(|p| p.into_inner()) = next.clone();
    st.flags.game_mode.store(next.active, Ordering::SeqCst);
    let suppress = st.with_settings(|s| s.game_mode.suppress_notifications);
    st.events.set_suppressed(next.active && suppress);
    crate::core::window::refresh_visibility(app, st);
    crate::modules::reconcile(app, st);
    st.events.emit(names::GAME_MODE, next.clone());
    crate::tray::refresh(app);

    if next.active && !was_active {
        let what = next.app.clone().unwrap_or_else(|| "ручное включение".into());
        st.log.info(category::GAME_MODE, "game_mode", format!("игровой режим включён ({}: {what})", next.reason.clone().unwrap_or_default()));
    } else if !next.active && was_active {
        st.log.info(category::GAME_MODE, "game_mode", "игровой режим выключен");
        let missed = st.events.take_missed();
        if missed > 0 {
            st.events.push(internal_notification(
                "game_mode",
                "Desktop Island",
                format!("Пропущено уведомлений: {missed}"),
                "Они сохранены в истории уведомлений",
                Priority::Normal,
            ));
        }
    }
    crate::core::profiles::evaluate_rules(app, st, None);
}
