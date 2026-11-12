//! Windows System Media Transport Controls (SMTC):
//! GlobalSystemMediaTransportControlsSessionManager.
//! Работает с любым приложением, публикующим медиасессию (Spotify, браузеры,
//! Яндекс Музыка, Groove и т.д.). Управление вкладками браузера невозможно.

use base64::Engine;
use std::sync::Mutex;
use windows::Media::Control::{
    GlobalSystemMediaTransportControlsSession as Session, GlobalSystemMediaTransportControlsSessionManager as Manager,
    GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
};
use windows::Storage::Streams::DataReader;

use super::ensure_com;
use crate::core::error::{AppError, AppResult};
use crate::database::now_ms;
use crate::platform::types::{MediaAction, MediaSessionInfo};

static MANAGER: Mutex<Option<Manager>> = Mutex::new(None);

fn manager() -> AppResult<Manager> {
    ensure_com();
    let mut g = MANAGER.lock().unwrap_or_else(|p| p.into_inner());
    if let Some(m) = g.as_ref() {
        return Ok(m.clone());
    }
    let m = Manager::RequestAsync()?.get()?;
    *g = Some(m.clone());
    Ok(m)
}

const TICKS_PER_MS: i64 = 10_000;
/// Разница между эпохой Windows (1601) и Unix (1970) в тиках по 100 нс.
const EPOCH_DIFF_TICKS: i64 = 116_444_736_000_000_000;

fn sessions_with_ids() -> AppResult<Vec<(String, Session)>> {
    let mgr = manager()?;
    let list = mgr.GetSessions()?;
    let mut out: Vec<(String, Session)> = vec![];
    for i in 0..list.Size()? {
        let s = list.GetAt(i)?;
        let aumid = s.SourceAppUserModelId().map(|h| h.to_string()).unwrap_or_default();
        let dup = out.iter().filter(|(id, _)| id == &aumid || id.starts_with(&format!("{aumid}#"))).count();
        let id = if dup == 0 { aumid } else { format!("{aumid}#{dup}") };
        out.push((id, s));
    }
    Ok(out)
}

pub fn list_sessions() -> AppResult<Vec<MediaSessionInfo>> {
    let mgr = manager()?;
    let current = mgr.GetCurrentSession().ok().and_then(|s| s.SourceAppUserModelId().ok()).map(|h| h.to_string());
    let mut result = vec![];
    for (id, s) in sessions_with_ids()? {
        let source = s.SourceAppUserModelId().map(|h| h.to_string()).unwrap_or_default();
        let (title, artist, album) = match s.TryGetMediaPropertiesAsync().and_then(|op| op.get()) {
            Ok(p) => (
                p.Title().map(|h| h.to_string()).unwrap_or_default(),
                p.Artist().map(|h| h.to_string()).unwrap_or_default(),
                p.AlbumTitle().map(|h| h.to_string()).unwrap_or_default(),
            ),
            Err(_) => (String::new(), String::new(), String::new()),
        };
        let mut info = MediaSessionInfo {
            app_name: crate::modules::music::friendly_app_name(&source),
            id: id.clone(),
            source_app: source.clone(),
            artwork_key: format!("{id}|{title}|{artist}|{album}"),
            title,
            artist,
            album,
            status: "unknown".into(),
            position_updated_at: now_ms(),
            is_current: current.as_deref() == Some(source.as_str()),
            ..Default::default()
        };
        if let Ok(pb) = s.GetPlaybackInfo() {
            info.status = match pb.PlaybackStatus() {
                Ok(Status::Playing) => "playing",
                Ok(Status::Paused) => "paused",
                Ok(Status::Stopped) | Ok(Status::Closed) => "stopped",
                _ => "unknown",
            }
            .into();
            if let Ok(c) = pb.Controls() {
                info.can_play_pause = c.IsPlayPauseToggleEnabled().unwrap_or(false)
                    || c.IsPlayEnabled().unwrap_or(false)
                    || c.IsPauseEnabled().unwrap_or(false);
                info.can_next = c.IsNextEnabled().unwrap_or(false);
                info.can_prev = c.IsPreviousEnabled().unwrap_or(false);
                info.can_seek = c.IsPlaybackPositionEnabled().unwrap_or(false);
            }
        }
        if let Ok(tl) = s.GetTimelineProperties() {
            let start = tl.StartTime().map(|t| t.Duration).unwrap_or(0);
            let end = tl.EndTime().map(|t| t.Duration).unwrap_or(0);
            let pos = tl.Position().map(|t| t.Duration).unwrap_or(0);
            if end > start {
                info.duration_ms = Some(((end - start) / TICKS_PER_MS) as u64);
                info.position_ms = Some(((pos - start).max(0) / TICKS_PER_MS) as u64);
                if let Ok(updated) = tl.LastUpdatedTime() {
                    let unix = (updated.UniversalTime - EPOCH_DIFF_TICKS) / TICKS_PER_MS;
                    if unix > 0 {
                        info.position_updated_at = unix;
                    }
                }
            }
        }
        result.push(info);
    }
    Ok(result)
}

fn find(id: &str) -> AppResult<Session> {
    sessions_with_ids()?
        .into_iter()
        .find(|(sid, _)| sid == id)
        .map(|(_, s)| s)
        .ok_or_else(|| AppError::NotFound("медиасессия больше не активна".into()))
}

pub fn control(id: &str, action: &MediaAction) -> AppResult<()> {
    let s = find(id)?;
    let accepted = match action {
        MediaAction::PlayPause => s.TryTogglePlayPauseAsync()?.get()?,
        MediaAction::Play => s.TryPlayAsync()?.get()?,
        MediaAction::Pause => s.TryPauseAsync()?.get()?,
        MediaAction::Next => s.TrySkipNextAsync()?.get()?,
        MediaAction::Previous => s.TrySkipPreviousAsync()?.get()?,
        MediaAction::Seek { position_ms } => {
            let start = s.GetTimelineProperties().and_then(|t| t.StartTime()).map(|t| t.Duration).unwrap_or(0);
            s.TryChangePlaybackPositionAsync(start + *position_ms as i64 * TICKS_PER_MS)?.get()?
        }
        MediaAction::SetVolume { .. } => {
            return Err(AppError::unsupported(
                "SMTC не предоставляет громкость отдельной сессии — используйте общую громкость",
            ))
        }
    };
    if accepted {
        Ok(())
    } else {
        Err(AppError::system("приложение отклонило команду (возможно, действие сейчас недоступно)"))
    }
}

/// Возвращает обложку текущего трека как data URL (≤ 4 МБ).
pub fn artwork(id: &str) -> AppResult<Option<String>> {
    let s = find(id)?;
    let props = s.TryGetMediaPropertiesAsync()?.get()?;
    let thumb = match props.Thumbnail() {
        Ok(t) => t,
        Err(_) => return Ok(None),
    };
    let stream = thumb.OpenReadAsync()?.get()?;
    let size = stream.Size()?;
    if size == 0 || size > 4 * 1024 * 1024 {
        return Ok(None);
    }
    let mime = stream.ContentType().map(|h| h.to_string()).unwrap_or_default();
    let mime = if mime.starts_with("image/") { mime } else { "image/png".into() };
    let input = stream.GetInputStreamAt(0)?;
    let reader = DataReader::CreateDataReader(&input)?;
    let loaded = reader.LoadAsync(size as u32)?.get()?;
    let mut buf = vec![0u8; loaded as usize];
    reader.ReadBytes(&mut buf)?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&buf);
    Ok(Some(format!("data:{mime};base64,{b64}")))
}
