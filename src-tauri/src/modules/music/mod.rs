//! Модуль «Музыка»: опрос медиасессий (SMTC / MPRIS), отправка событий во
//! фронтенд только при реальных изменениях, кэш обложек, управление.
//!
//! Позиция воспроизведения не отправляется каждую секунду: фронтенд
//! интерполирует её по `positionMs + (now - positionUpdatedAt)`. Событие
//! отправляется при смене трека, статуса или перемотке.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;
use tauri::AppHandle;

use crate::core::error::{AppError, AppResult};
use crate::core::state::Shared;
use crate::core::task::TaskSlot;
use crate::database::now_ms;
use crate::events::{names, Activity, Priority};
use crate::platform::os;
use crate::platform::types::{MediaAction, MediaSessionInfo};

#[derive(Default)]
pub struct MusicModule {
    task: TaskSlot,
    sessions: Mutex<Vec<MediaSessionInfo>>,
    artwork: Mutex<HashMap<String, (String, Option<String>)>>,
    selected: Mutex<Option<String>>,
}

/// Человекочитаемое имя по идентификатору источника.
pub fn friendly_app_name(source: &str) -> String {
    let s = source.to_lowercase();
    let known: &[(&str, &str)] = &[
        ("spotify", "Spotify"),
        ("yandex", "Яндекс Музыка"),
        ("soundcloud", "SoundCloud"),
        ("vlc", "VLC"),
        ("chromium", "Chromium"),
        ("chrome", "Google Chrome"),
        ("msedge", "Microsoft Edge"),
        ("edge", "Microsoft Edge"),
        ("308046b0af4a39cb", "Firefox"),
        ("firefox", "Firefox"),
        ("opera", "Opera"),
        ("brave", "Brave"),
        ("zunemusic", "Медиаплеер"),
        ("music.ui", "Медиаплеер"),
        ("telegram", "Telegram"),
        ("discord", "Discord"),
        ("mpv", "mpv"),
        ("rhythmbox", "Rhythmbox"),
        ("elisa", "Elisa"),
        ("youtube", "YouTube Music"),
        ("audacious", "Audacious"),
        ("foobar", "foobar2000"),
        ("aimp", "AIMP"),
    ];
    for (needle, name) in known {
        if s.contains(needle) {
            return name.to_string();
        }
    }
    let base = source.rsplit(['\\', '/']).next().unwrap_or(source);
    let base = base.split('!').next().unwrap_or(base).trim_end_matches(".exe");
    let base = base.split('_').next().unwrap_or(base);
    let base = base.rsplit('.').next().unwrap_or(base);
    if base.is_empty() {
        "Медиаплеер".into()
    } else {
        let mut c = base.chars();
        c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
    }
}

fn significant_change(prev: &[MediaSessionInfo], next: &[MediaSessionInfo]) -> bool {
    if prev.len() != next.len() {
        return true;
    }
    let now = now_ms();
    for (a, b) in prev.iter().zip(next.iter()) {
        let mut a2 = a.clone();
        let mut b2 = b.clone();
        a2.position_ms = None;
        b2.position_ms = None;
        a2.position_updated_at = 0;
        b2.position_updated_at = 0;
        if a2 != b2 {
            return true;
        }
        if let (Some(pa), Some(pb)) = (a.position_ms, b.position_ms) {
            let elapsed = if a.status == "playing" { (now - a.position_updated_at).max(0) as u64 } else { 0 };
            let expected = pa + elapsed;
            let actual = pb + if b.status == "playing" { (now - b.position_updated_at).max(0) as u64 } else { 0 };
            if expected.abs_diff(actual) > 2500 {
                return true;
            }
        } else if a.position_ms.is_some() != b.position_ms.is_some() {
            return true;
        }
    }
    false
}

impl MusicModule {
    pub fn set_running(&self, app: &AppHandle, st: &Shared, run: bool) {
        if run == self.task.is_running() {
            return;
        }
        if !run {
            self.task.stop();
            return;
        }
        let app = app.clone();
        let st = st.clone();
        self.task.start(async move {
            let mut errors = 0u32;
            loop {
                let interval = st.with_settings(|s| s.music.poll_interval_ms);
                match tauri::async_runtime::spawn_blocking(os::media::list_sessions).await {
                    Ok(Ok(list)) => {
                        errors = 0;
                        st.music.update(&st, list);
                    }
                    Ok(Err(e)) => {
                        errors += 1;
                        if errors == 1 {
                            st.log.warn(crate::logging::category::MODULE, "music", format!("медиасессии недоступны: {e}"));
                        }
                    }
                    Err(e) => tracing::error!(error = %e, "опрос медиасессий завершился с ошибкой"),
                }
                // При повторяющихся ошибках снижаем частоту опроса.
                let delay = if errors > 3 { 10_000 } else { interval };
                tokio::time::sleep(Duration::from_millis(delay)).await;
                let _ = &app;
            }
        });
    }

    fn update(&self, st: &Shared, list: Vec<MediaSessionInfo>) {
        let changed = {
            let mut g = self.sessions.lock().unwrap_or_else(|p| p.into_inner());
            let changed = significant_change(&g, &list);
            *g = list.clone();
            changed
        };
        if !changed {
            return;
        }
        {
            let ids: Vec<&str> = list.iter().map(|s| s.id.as_str()).collect();
            self.artwork.lock().unwrap_or_else(|p| p.into_inner()).retain(|k, _| ids.contains(&k.as_str()));
        }
        st.events.emit(names::MEDIA_SESSIONS, list.clone());
        match self.pick(&list) {
            Some(s) if !s.title.is_empty() => st.events.set_activity(Activity {
                id: "music".into(),
                kind: "music".into(),
                priority: if s.status == "playing" { Priority::Normal } else { Priority::Low },
                title: s.title.clone(),
                subtitle: Some(if s.artist.is_empty() { s.app_name.clone() } else { s.artist.clone() }),
                icon: None,
                progress: None,
                updated_at: now_ms(),
            }),
            _ => st.events.clear_activity("music"),
        }
    }

    fn pick<'a>(&self, list: &'a [MediaSessionInfo]) -> Option<&'a MediaSessionInfo> {
        let selected = self.selected.lock().ok().and_then(|g| g.clone());
        selected
            .and_then(|id| list.iter().find(|s| s.id == id))
            .or_else(|| list.iter().find(|s| s.status == "playing"))
            .or_else(|| list.iter().find(|s| s.is_current))
            .or_else(|| list.first())
    }

    pub async fn sessions(&self) -> AppResult<Vec<MediaSessionInfo>> {
        if self.task.is_running() {
            return Ok(self.sessions.lock().unwrap_or_else(|p| p.into_inner()).clone());
        }
        tauri::async_runtime::spawn_blocking(os::media::list_sessions).await?
    }

    pub fn select(&self, id: Option<String>) {
        *self.selected.lock().unwrap_or_else(|p| p.into_inner()) = id;
    }

    pub async fn control(&self, id: String, action: MediaAction) -> AppResult<()> {
        if let MediaAction::SetVolume { volume } = &action {
            if !volume.is_finite() {
                return Err(AppError::invalid("громкость"));
            }
        }
        tauri::async_runtime::spawn_blocking(move || os::media::control(&id, &action)).await??;
        Ok(())
    }

    pub async fn control_current(&self, action: MediaAction) -> AppResult<()> {
        let list = self.sessions().await?;
        let id = self.pick(&list).map(|s| s.id.clone()).ok_or_else(|| AppError::NotFound("нет активной медиасессии".into()))?;
        self.control(id, action).await
    }

    pub async fn artwork(&self, id: String) -> AppResult<Option<String>> {
        let key = {
            let g = self.sessions.lock().unwrap_or_else(|p| p.into_inner());
            g.iter().find(|s| s.id == id).map(|s| s.artwork_key.clone())
        };
        if let Some(k) = &key {
            if let Some((ck, url)) = self.artwork.lock().unwrap_or_else(|p| p.into_inner()).get(&id) {
                if ck == k {
                    return Ok(url.clone());
                }
            }
        }
        let id2 = id.clone();
        let url = tauri::async_runtime::spawn_blocking(move || os::media::artwork(&id2)).await?.unwrap_or(None);
        if let Some(k) = key {
            let mut g = self.artwork.lock().unwrap_or_else(|p| p.into_inner());
            if g.len() > 6 {
                g.clear();
            }
            g.insert(id, (k, url.clone()));
        }
        Ok(url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(friendly_app_name("Spotify.exe"), "Spotify");
        assert_eq!(friendly_app_name("308046B0AF4A39CB"), "Firefox");
        assert_eq!(friendly_app_name("MSEdge"), "Microsoft Edge");
        assert_eq!(friendly_app_name("org.mpris.MediaPlayer2.vlc"), "VLC");
        assert_eq!(friendly_app_name("foo.exe"), "Foo");
    }

    #[test]
    fn change_detection_ignores_drift() {
        let a = MediaSessionInfo { id: "x".into(), title: "t".into(), status: "paused".into(), position_ms: Some(1000), position_updated_at: now_ms(), ..Default::default() };
        let mut b = a.clone();
        b.position_ms = Some(1500);
        assert!(!significant_change(&[a.clone()], &[b.clone()]));
        b.position_ms = Some(60_000);
        assert!(significant_change(&[a.clone()], &[b.clone()]));
        b = a.clone();
        b.title = "other".into();
        assert!(significant_change(&[a], &[b]));
    }
}
