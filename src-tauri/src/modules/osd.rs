//! Громкость и яркость в острове вместо системных плашек (см. platform::*::osd).

use serde::Serialize;
use std::sync::mpsc;
use std::sync::Mutex;

use crate::core::state::Shared;
use crate::platform::os;

pub const EVENT: &str = "island://osd";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OsdPayload {
    /// volume | brightness
    pub kind: &'static str,
    /// 0–100
    pub value: u32,
    pub muted: bool,
}

#[derive(Default)]
pub struct OsdModule {
    /// (громкость, яркость) — что сейчас включено.
    running: Mutex<(bool, bool)>,
}

/// Шаг громкости — как у Windows.
const STEP: f32 = 0.02;

impl OsdModule {
    pub fn set(&self, st: &Shared, volume: bool, brightness: bool) {
        let mut g = self.running.lock().unwrap_or_else(|p| p.into_inner());
        if g.0 != volume {
            if volume {
                let (tx, rx) = mpsc::channel::<os::osd::VolumeKey>();
                let st2 = st.clone();
                std::thread::Builder::new()
                    .name("osd-volume".into())
                    .spawn(move || {
                        while let Ok(k) = rx.recv() {
                            let cur = match os::audio::state() {
                                Ok(c) => c,
                                Err(e) => {
                                    tracing::warn!(error = %e, "громкость недоступна");
                                    continue;
                                }
                            };
                            let (level, muted) = match k {
                                os::osd::VolumeKey::Up => ((cur.level + STEP).min(1.0), false),
                                os::osd::VolumeKey::Down => ((cur.level - STEP).max(0.0), cur.muted && cur.level - STEP > 0.0),
                                os::osd::VolumeKey::Mute => (cur.level, !cur.muted),
                            };
                            if (level - cur.level).abs() > f32::EPSILON {
                                let _ = os::audio::set_volume(level);
                            }
                            if muted != cur.muted {
                                let _ = os::audio::set_mute(muted);
                            }
                            st2.events.emit(EVENT, OsdPayload { kind: "volume", value: (level * 100.0).round() as u32, muted });
                        }
                    })
                    .ok();
                os::osd::start_volume_keys(tx);
            } else {
                os::osd::stop_volume_keys();
            }
            g.0 = volume;
        }
        if g.1 != brightness {
            if brightness {
                let (tx, rx) = mpsc::channel::<u32>();
                let st2 = st.clone();
                std::thread::Builder::new()
                    .name("osd-brightness".into())
                    .spawn(move || {
                        // Первое уведомление — текущее значение при подписке, его не показываем.
                        let mut last: Option<u32> = None;
                        while let Ok(v) = rx.recv() {
                            if last.is_some() && last != Some(v) {
                                st2.events.emit(EVENT, OsdPayload { kind: "brightness", value: v, muted: false });
                            }
                            last = Some(v);
                        }
                    })
                    .ok();
                os::osd::start_brightness(tx);
            } else {
                os::osd::stop_brightness();
            }
            g.1 = brightness;
        }
    }

    /// При выходе — вернуть клавиши громкости системе.
    pub fn shutdown(&self) {
        os::osd::stop_volume_keys();
        os::osd::stop_brightness();
    }
}
