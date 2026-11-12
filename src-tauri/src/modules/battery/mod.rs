//! Зарядка и низкий заряд: при подключении/отключении зарядки остров на пару
//! секунд показывает анимацию с процентом, при 20% и 10% — предупреждение.
//! Опрос раз в 5 секунд (GetSystemPowerStatus / sysfs — очень дешёвые вызовы);
//! на компьютерах без батареи модуль сам останавливается.

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tauri::AppHandle;

use crate::core::state::Shared;
use crate::core::task::TaskSlot;
use crate::database::now_ms;
use crate::events::{internal_notification, Activity, Priority};

#[derive(Default)]
pub struct BatteryModule {
    task: TaskSlot,
    pulse_gen: AtomicU64,
}

impl BatteryModule {
    pub fn set_running(&self, app: &AppHandle, st: &Shared, run: bool) {
        if run == self.task.is_running() {
            return;
        }
        if !run {
            self.task.stop();
            return;
        }
        let _ = app;
        let st = st.clone();
        self.task.start(async move {
            let mut prev_charging: Option<bool> = None;
            let mut prev_percent: Option<u8> = None;
            loop {
                let state = tauri::async_runtime::spawn_blocking(|| crate::platform::os::power::state().ok()).await.ok().flatten();
                let Some(p) = state else {
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    continue;
                };
                if !p.has_battery {
                    return; // настольный компьютер — следить не за чем
                }
                let charging = p.charging || p.on_ac == Some(true);
                let percent = p.percent;
                if let Some(prev) = prev_charging {
                    if prev != charging {
                        st.battery.pulse(&st, charging, percent);
                    }
                }
                if let (Some(prev), Some(now)) = (prev_percent, percent) {
                    if !charging {
                        for threshold in [20u8, 10u8] {
                            if prev > threshold && now <= threshold {
                                st.events.push(internal_notification(
                                    "battery",
                                    "Батарея",
                                    format!("Низкий заряд: {now}%"),
                                    if threshold == 10 { "Подключите зарядку, скоро компьютер выключится" } else { "Подключите зарядку" },
                                    if threshold == 10 { Priority::Critical } else { Priority::High },
                                ));
                            }
                        }
                    }
                }
                prev_charging = Some(charging);
                prev_percent = percent;
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
        });
    }

    /// Короткая «вспышка» в компактной панели (как на iPhone при подключении зарядки).
    fn pulse(&self, st: &Shared, charging: bool, percent: Option<u8>) {
        let gen = self.pulse_gen.fetch_add(1, Ordering::SeqCst) + 1;
        st.events.set_activity(Activity {
            id: "battery".into(),
            kind: if charging { "charging".into() } else { "unplugged".into() },
            priority: Priority::Critical,
            title: if charging { "Заряжается".into() } else { "Зарядка отключена".into() },
            subtitle: percent.map(|p| format!("{p}%")),
            icon: None,
            progress: percent.map(|p| p as f64 / 100.0),
            updated_at: now_ms(),
        });
        let st2 = st.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(3500)).await;
            if st2.battery.pulse_gen.load(Ordering::SeqCst) == gen {
                st2.events.clear_activity("battery");
            }
        });
    }
}
