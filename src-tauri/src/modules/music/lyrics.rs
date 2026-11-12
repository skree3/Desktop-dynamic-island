//! Тексты песен с синхронизацией по времени из открытой базы LRCLIB (lrclib.net).
//! Отправляются только исполнитель, название, альбом и длительность трека.
//! Ответы (в том числе «не найдено») кэшируются в памяти.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::Duration;

use crate::core::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Lyrics {
    /// Строки с временем начала (мс), отсортированы по времени.
    pub lines: Vec<(u64, String)>,
    pub instrumental: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LrcResponse {
    synced_lyrics: Option<String>,
    #[serde(default)]
    instrumental: bool,
}

static CACHE: Mutex<Option<(HashMap<String, Option<Lyrics>>, VecDeque<String>)>> = Mutex::new(None);
const CACHE_SIZE: usize = 64;

/// Разбирает формат LRC: «[01:23.45] текст», у строки может быть несколько меток.
pub fn parse_lrc(text: &str) -> Vec<(u64, String)> {
    let mut out = vec![];
    for line in text.lines() {
        let mut rest = line.trim();
        let mut stamps = vec![];
        while let Some(stripped) = rest.strip_prefix('[') {
            let Some(end) = stripped.find(']') else { break };
            let tag = &stripped[..end];
            rest = stripped[end + 1..].trim_start();
            let Some((m, s)) = tag.split_once(':') else { continue };
            let (Ok(m), Ok(s)) = (m.trim().parse::<u64>(), s.trim().parse::<f64>()) else { continue };
            stamps.push(m * 60_000 + (s * 1000.0).round() as u64);
        }
        let text = strip_word_stamps(rest);
        for t in stamps {
            out.push((t, text.clone()));
        }
    }
    out.sort_by_key(|l| l.0);
    out
}

/// Расширенный LRC размечает каждое слово: «<00:18.89>We're <00:19.25>no» → «We're no».
fn strip_word_stamps(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find('<') {
        let Some(end) = rest[start..].find('>') else { break };
        let tag = &rest[start + 1..start + end];
        out.push_str(&rest[..start]);
        if !tag.contains(':') || !tag.chars().all(|c| c.is_ascii_digit() || c == ':' || c == '.') {
            out.push_str(&rest[start..start + end + 1]);
        }
        rest = &rest[start + end + 1..];
    }
    out.push_str(rest);
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Блокирующий запрос — вызывать из spawn_blocking.
pub fn fetch(artist: &str, title: &str, album: &str, duration_ms: Option<u64>) -> AppResult<Option<Lyrics>> {
    let artist = artist.trim();
    let title = title.trim();
    if artist.is_empty() || title.is_empty() {
        return Ok(None);
    }
    let key = format!("{}\u{1}{}\u{1}{}", artist.to_lowercase(), title.to_lowercase(), duration_ms.unwrap_or(0) / 2000);
    if let Some((map, _)) = CACHE.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
        if let Some(hit) = map.get(&key) {
            return Ok(hit.clone());
        }
    }
    let mut url = format!("https://lrclib.net/api/get?artist_name={}&track_name={}", enc(artist), enc(title));
    if !album.trim().is_empty() {
        url.push_str(&format!("&album_name={}", enc(album.trim())));
    }
    if let Some(d) = duration_ms {
        url.push_str(&format!("&duration={}", (d as f64 / 1000.0).round() as u64));
    }
    let agent = ureq::AgentBuilder::new().timeout(Duration::from_secs(8)).build();
    let result = match agent.get(&url).set("User-Agent", concat!("DesktopIsland/", env!("CARGO_PKG_VERSION"))).call() {
        Ok(resp) => {
            let body: LrcResponse = resp.into_json().map_err(|e| AppError::system(format!("LRCLIB: {e}")))?;
            let lines = body.synced_lyrics.as_deref().map(parse_lrc).unwrap_or_default();
            if lines.is_empty() && !body.instrumental {
                None
            } else {
                Some(Lyrics { lines, instrumental: body.instrumental })
            }
        }
        Err(ureq::Error::Status(404, _)) => None,
        Err(e) => return Err(AppError::system(format!("LRCLIB недоступен: {e}"))),
    };
    let mut g = CACHE.lock().unwrap_or_else(|p| p.into_inner());
    let (map, order) = g.get_or_insert_with(Default::default);
    map.insert(key.clone(), result.clone());
    order.push_back(key);
    while order.len() > CACHE_SIZE {
        if let Some(old) = order.pop_front() {
            map.remove(&old);
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lrc() {
        let l = parse_lrc("[00:12.50] Привет\n[01:02.00][00:05.00]Хор\n[ar:Artist]\nбез метки");
        assert_eq!(l, vec![(5000, "Хор".into()), (12500, "Привет".into()), (62000, "Хор".into())]);
        assert_eq!(enc("AC/DC & Co"), "AC%2FDC%20%26%20Co");
        assert_eq!(strip_word_stamps("<00:18.89>We're <00:19.25>no  <00:19.53>strangers"), "We're no strangers");
        assert_eq!(strip_word_stamps("a <b> c"), "a <b> c");
    }
}
