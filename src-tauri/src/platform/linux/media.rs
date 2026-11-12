//! MPRIS (org.mpris.MediaPlayer2.*) — стандарт управления медиаплеерами в Linux.
//! Поддерживают Spotify, VLC, Firefox, Chromium/Chrome/Edge (через медиасессии),
//! Rhythmbox, mpv (с плагином) и многие другие.

use std::collections::HashMap;
use zbus::zvariant::{ObjectPath, OwnedValue};

use super::{block, proxy, session_bus, val_i64, val_str, val_str_list};
use crate::core::error::{AppError, AppResult};
use crate::database::now_ms;
use crate::platform::types::{MediaAction, MediaSessionInfo};
use crate::util::image::bytes_data_url;

const PREFIX: &str = "org.mpris.MediaPlayer2.";
const PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";

async fn player_names() -> AppResult<Vec<String>> {
    let conn = session_bus().await?;
    let dbus = zbus::fdo::DBusProxy::new(conn).await?;
    let names = dbus.list_names().await?;
    Ok(names.into_iter().map(|n| n.to_string()).filter(|n| n.starts_with(PREFIX)).collect())
}

async fn session(name: &str) -> AppResult<MediaSessionInfo> {
    let conn = session_bus().await?;
    let p = proxy(conn, name, PATH, PLAYER).await?;
    let root = proxy(conn, name, PATH, "org.mpris.MediaPlayer2").await?;
    let meta: HashMap<String, OwnedValue> = p.get_property("Metadata").await.unwrap_or_default();
    let get = |k: &str| meta.get(k).map(|v| &**v);
    let title = get("xesam:title").and_then(val_str).unwrap_or_default();
    let artist = get("xesam:artist").map(val_str_list).unwrap_or_default().join(", ");
    let album = get("xesam:album").and_then(val_str).unwrap_or_default();
    let length_us = get("mpris:length").and_then(val_i64);
    let art = get("mpris:artUrl").and_then(val_str);
    let status: String = p.get_property("PlaybackStatus").await.unwrap_or_else(|_| "Stopped".into());
    let position_us: Option<i64> = p.get_property("Position").await.ok();
    let can_play: bool = p.get_property("CanPlay").await.unwrap_or(false);
    let can_pause: bool = p.get_property("CanPause").await.unwrap_or(false);
    let can_next: bool = p.get_property("CanGoNext").await.unwrap_or(false);
    let can_prev: bool = p.get_property("CanGoPrevious").await.unwrap_or(false);
    let can_seek: bool = p.get_property("CanSeek").await.unwrap_or(false);
    let volume: Option<f64> = p.get_property("Volume").await.ok();
    let identity: String = root.get_property("Identity").await.unwrap_or_default();

    let short = name.trim_start_matches(PREFIX).split('.').next().unwrap_or("").to_string();
    let app_name = if identity.is_empty() { crate::modules::music::friendly_app_name(&short) } else { identity };
    let artwork_url = art.as_ref().filter(|u| u.starts_with("https://") || u.starts_with("http://")).cloned();
    Ok(MediaSessionInfo {
        id: name.to_string(),
        source_app: short,
        app_name,
        artwork_key: format!("{name}|{title}|{artist}|{album}|{}", art.clone().unwrap_or_default()),
        title,
        artist,
        album,
        status: match status.as_str() {
            "Playing" => "playing",
            "Paused" => "paused",
            "Stopped" => "stopped",
            _ => "unknown",
        }
        .into(),
        position_ms: position_us.filter(|_| length_us.is_some()).map(|u| (u.max(0) / 1000) as u64),
        duration_ms: length_us.filter(|l| *l > 0).map(|l| (l / 1000) as u64),
        position_updated_at: now_ms(),
        can_play_pause: can_play || can_pause,
        can_next,
        can_prev,
        can_seek,
        supports_volume: volume.is_some(),
        volume: volume.map(|v| v.clamp(0.0, 1.0) as f32),
        artwork_url,
        is_current: false,
    })
}

pub fn list_sessions() -> AppResult<Vec<MediaSessionInfo>> {
    block(async {
        let mut out = vec![];
        for name in player_names().await? {
            match session(&name).await {
                Ok(s) => out.push(s),
                Err(e) => tracing::debug!(%name, error = %e, "MPRIS-плеер не ответил"),
            }
        }
        // «Текущий» — первый играющий.
        if let Some(i) = out.iter().position(|s| s.status == "playing") {
            out[i].is_current = true;
        } else if let Some(first) = out.first_mut() {
            first.is_current = true;
        }
        Ok(out)
    })
}

pub fn control(id: &str, action: &MediaAction) -> AppResult<()> {
    if !id.starts_with(PREFIX) {
        return Err(AppError::invalid("неверный идентификатор плеера"));
    }
    block(async {
        let conn = session_bus().await?;
        let p = proxy(conn, id, PATH, PLAYER).await?;
        match action {
            MediaAction::PlayPause => p.call_method("PlayPause", &()).await.map(|_| ())?,
            MediaAction::Play => p.call_method("Play", &()).await.map(|_| ())?,
            MediaAction::Pause => p.call_method("Pause", &()).await.map(|_| ())?,
            MediaAction::Next => p.call_method("Next", &()).await.map(|_| ())?,
            MediaAction::Previous => p.call_method("Previous", &()).await.map(|_| ())?,
            MediaAction::Seek { position_ms } => {
                let meta: HashMap<String, OwnedValue> = p.get_property("Metadata").await?;
                let track = meta
                    .get("mpris:trackid")
                    .and_then(|v| val_str(v))
                    .ok_or_else(|| AppError::unsupported("плеер не сообщает идентификатор трека — перемотка невозможна"))?;
                let path = ObjectPath::try_from(track.as_str()).map_err(|e| AppError::system(e.to_string()))?;
                p.call_method("SetPosition", &(path, (*position_ms as i64) * 1000)).await.map(|_| ())?
            }
            MediaAction::SetVolume { volume } => p.set_property("Volume", (*volume as f64).clamp(0.0, 1.0)).await?,
        };
        Ok(())
    })
}

pub fn artwork(id: &str) -> AppResult<Option<String>> {
    let sessions = list_sessions()?;
    let Some(s) = sessions.into_iter().find(|s| s.id == id) else { return Ok(None) };
    if s.artwork_url.is_some() {
        return Ok(None); // фронтенд загрузит https-URL напрямую
    }
    let url = block(async {
        let conn = session_bus().await?;
        let p = proxy(conn, id, PATH, PLAYER).await?;
        let meta: HashMap<String, OwnedValue> = p.get_property("Metadata").await?;
        Ok::<_, AppError>(meta.get("mpris:artUrl").and_then(|v| val_str(v)))
    })?;
    let Some(url) = url else { return Ok(None) };
    let Some(path) = url.strip_prefix("file://") else { return Ok(None) };
    let path = percent_decode(path);
    let meta = std::fs::metadata(&path)?;
    if meta.len() > 4 * 1024 * 1024 {
        return Ok(None);
    }
    let bytes = std::fs::read(&path)?;
    let mime = if bytes.starts_with(&[0x89, b'P', b'N', b'G']) { "image/png" } else { "image/jpeg" };
    Ok(Some(bytes_data_url(mime, &bytes)))
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}
