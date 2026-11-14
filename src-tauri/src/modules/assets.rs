//! Свои шрифты и звуки: проверка файлов и хранение в папке программы.
//!
//! Шрифт проверяется по структуре sfnt (TrueType/OpenType): сигнатура, таблица
//! таблиц в пределах файла и обязательные таблицы cmap/head/name/(glyf|CFF).
//! Имя семейства читается из таблицы name. Звук — по сигнатуре WAV/MP3/OGG.

use base64::Engine;
use serde::Serialize;
use std::path::{Path, PathBuf};

use crate::core::error::{AppError, AppResult};

pub const MAX_FONT: u64 = 15 * 1024 * 1024;
pub const MAX_SOUND: u64 = 5 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FontFile {
    /// Имя файла в data/fonts (значение настройки — `custom:<file>`).
    pub file: String,
    pub family: String,
    pub size: u64,
}

fn be16(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?))
}
fn be32(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// Проверяет шрифт и возвращает имя семейства.
pub fn validate_font(bytes: &[u8]) -> AppResult<String> {
    let bad = |m: &str| AppError::invalid(format!("файл не похож на шрифт: {m}"));
    let tag = bytes.get(0..4).ok_or_else(|| bad("слишком короткий"))?;
    match tag {
        [0, 1, 0, 0] | b"OTTO" | b"true" => {}
        b"ttcf" => return Err(AppError::invalid("коллекции шрифтов (.ttc) не поддерживаются — выберите один TTF или OTF")),
        b"wOFF" | b"wOF2" => return Err(AppError::invalid("веб-шрифты WOFF не поддерживаются — нужен TTF или OTF")),
        _ => return Err(bad("неизвестная сигнатура")),
    }
    let num = be16(bytes, 4).ok_or_else(|| bad("нет заголовка"))? as usize;
    if num == 0 || num > 64 {
        return Err(bad("неверное число таблиц"));
    }
    let mut tables = std::collections::HashMap::new();
    for i in 0..num {
        let rec = 12 + i * 16;
        let t = bytes.get(rec..rec + 4).ok_or_else(|| bad("обрезан каталог таблиц"))?;
        let off = be32(bytes, rec + 8).ok_or_else(|| bad("обрезан каталог"))? as usize;
        let len = be32(bytes, rec + 12).ok_or_else(|| bad("обрезан каталог"))? as usize;
        if off.checked_add(len).map(|e| e > bytes.len()).unwrap_or(true) {
            return Err(bad("таблица выходит за пределы файла"));
        }
        tables.insert(String::from_utf8_lossy(t).to_string(), (off, len));
    }
    for need in ["cmap", "head", "name"] {
        if !tables.contains_key(need) {
            return Err(bad(&format!("нет таблицы {need}")));
        }
    }
    if !tables.contains_key("glyf") && !tables.contains_key("CFF ") && !tables.contains_key("CFF2") {
        return Err(bad("нет глифов"));
    }
    let (off, len) = tables["name"];
    Ok(font_family(&bytes[off..off + len]).unwrap_or_else(|| "Свой шрифт".into()))
}

/// Имя семейства (nameID 16, иначе 1) из таблицы name.
fn font_family(t: &[u8]) -> Option<String> {
    let count = be16(t, 2)? as usize;
    let strings = be16(t, 4)? as usize;
    let mut best: Option<(u8, String)> = None;
    for i in 0..count {
        let r = 6 + i * 12;
        let platform = be16(t, r)?;
        let lang = be16(t, r + 4)?;
        let id = be16(t, r + 6)?;
        let len = be16(t, r + 8)? as usize;
        let off = be16(t, r + 10)? as usize;
        if id != 1 && id != 16 {
            continue;
        }
        let raw = t.get(strings + off..strings + off + len)?;
        let text = if platform == 3 || platform == 0 {
            let u: Vec<u16> = raw.chunks_exact(2).map(|c| u16::from_be_bytes([c[0], c[1]])).collect();
            String::from_utf16_lossy(&u)
        } else {
            raw.iter().map(|&c| c as char).collect()
        };
        let text = text.trim().to_string();
        if text.is_empty() {
            continue;
        }
        // Предпочтение: typographic family (16), английский (0x409), платформа Windows.
        let rank = (id == 16) as u8 * 4 + (lang == 0x409) as u8 * 2 + (platform == 3) as u8;
        if best.as_ref().map(|(r, _)| rank > *r).unwrap_or(true) {
            best = Some((rank, text));
        }
    }
    best.map(|(_, s)| s.chars().filter(|c| !c.is_control()).take(64).collect())
}

/// Безопасное имя файла: буквы, цифры, пробел, - и _.
pub fn safe_name(stem: &str, ext: &str) -> String {
    let clean: String = stem.chars().filter(|c| c.is_alphanumeric() || matches!(c, ' ' | '-' | '_')).take(60).collect();
    let clean = clean.trim();
    format!("{}.{ext}", if clean.is_empty() { "font" } else { clean })
}

pub fn fonts_dir(data: &Path) -> PathBuf {
    data.join("fonts")
}
pub fn sounds_dir(data: &Path) -> PathBuf {
    data.join("sounds")
}

/// Копирует шрифт в папку программы после проверки.
pub fn import_font(data: &Path, src: &Path) -> AppResult<FontFile> {
    let ext = src.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).unwrap_or_default();
    if ext != "ttf" && ext != "otf" {
        return Err(AppError::invalid("нужен файл TTF или OTF"));
    }
    let size = std::fs::metadata(src)?.len();
    if size > MAX_FONT {
        return Err(AppError::invalid("шрифт больше 15 МБ"));
    }
    let bytes = std::fs::read(src)?;
    let family = validate_font(&bytes)?;
    let dir = fonts_dir(data);
    std::fs::create_dir_all(&dir)?;
    let file = safe_name(&src.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(), &ext);
    std::fs::write(dir.join(&file), &bytes)?;
    Ok(FontFile { file, family, size })
}

pub fn list_fonts(data: &Path) -> Vec<FontFile> {
    let mut out = vec![];
    if let Ok(rd) = std::fs::read_dir(fonts_dir(data)) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let lower = name.to_lowercase();
            if !(lower.ends_with(".ttf") || lower.ends_with(".otf")) {
                continue;
            }
            if let Ok(bytes) = std::fs::read(e.path()) {
                if let Ok(family) = validate_font(&bytes) {
                    out.push(FontFile { file: name, family, size: bytes.len() as u64 });
                }
            }
        }
    }
    out.sort_by(|a, b| a.family.to_lowercase().cmp(&b.family.to_lowercase()));
    out
}

fn checked_file(dir: &Path, file: &str) -> AppResult<PathBuf> {
    if file.is_empty() || file.contains(['/', '\\', ':']) || file.contains("..") {
        return Err(AppError::invalid("неверное имя файла"));
    }
    let p = dir.join(file);
    if !p.is_file() {
        return Err(AppError::NotFound("файл не найден".into()));
    }
    Ok(p)
}

pub fn font_data(data: &Path, file: &str) -> AppResult<String> {
    let p = checked_file(&fonts_dir(data), file)?;
    let mime = if file.to_lowercase().ends_with(".otf") { "font/otf" } else { "font/ttf" };
    Ok(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(std::fs::read(p)?)))
}

pub fn remove_font(data: &Path, file: &str) -> AppResult<()> {
    let p = checked_file(&fonts_dir(data), file)?;
    std::fs::remove_file(p)?;
    Ok(())
}

/// Тип звука по сигнатуре: wav | mp3 | ogg.
pub fn sound_kind(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WAVE" {
        Some("wav")
    } else if bytes.len() >= 4 && &bytes[0..4] == b"OggS" {
        Some("ogg")
    } else if bytes.len() >= 3 && (&bytes[0..3] == b"ID3" || (bytes[0] == 0xFF && (bytes[1] & 0xE0) == 0xE0)) {
        Some("mp3")
    } else {
        None
    }
}

/// Копирует свой звук (kind = notification | timer). Возвращает значение настройки.
pub fn import_sound(data: &Path, kind: &str, src: &Path) -> AppResult<String> {
    if kind != "notification" && kind != "timer" {
        return Err(AppError::invalid("неизвестный тип звука"));
    }
    if std::fs::metadata(src)?.len() > MAX_SOUND {
        return Err(AppError::invalid("звук больше 5 МБ — выберите короткий файл"));
    }
    let bytes = std::fs::read(src)?;
    let ext = sound_kind(&bytes).ok_or_else(|| AppError::invalid("нужен файл WAV, MP3 или OGG"))?;
    let dir = sounds_dir(data);
    std::fs::create_dir_all(&dir)?;
    for old in ["wav", "mp3", "ogg"] {
        let _ = std::fs::remove_file(dir.join(format!("{kind}.{old}")));
    }
    let file = format!("{kind}.{ext}");
    std::fs::write(dir.join(&file), &bytes)?;
    Ok(format!("custom:{file}"))
}

pub fn sound_data(data: &Path, file: &str) -> AppResult<String> {
    let p = checked_file(&sounds_dir(data), file)?;
    let bytes = std::fs::read(p)?;
    let mime = match sound_kind(&bytes) {
        Some("wav") => "audio/wav",
        Some("ogg") => "audio/ogg",
        _ => "audio/mpeg",
    };
    Ok(format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Минимальный sfnt с таблицами cmap/head/name/glyf и именем семейства «Test Sans».
    fn tiny_font() -> Vec<u8> {
        let name_str: Vec<u8> = "Test Sans".encode_utf16().flat_map(|c| c.to_be_bytes()).collect();
        let mut name = vec![];
        name.extend(0u16.to_be_bytes()); // format
        name.extend(1u16.to_be_bytes()); // count
        name.extend(18u16.to_be_bytes()); // string offset (6 + 12)
        for v in [3u16, 1, 0x409, 1, name_str.len() as u16, 0] {
            name.extend(v.to_be_bytes());
        }
        name.extend(&name_str);
        let tables: Vec<(&[u8; 4], Vec<u8>)> = vec![(b"cmap", vec![0; 4]), (b"glyf", vec![0; 4]), (b"head", vec![0; 54]), (b"name", name)];
        let mut out: Vec<u8> = vec![0, 1, 0, 0];
        out.extend((tables.len() as u16).to_be_bytes());
        out.extend([0u8; 6]);
        let mut offset = 12 + tables.len() * 16;
        let mut body: Vec<u8> = vec![];
        for (tag, data) in &tables {
            out.extend(*tag);
            out.extend(0u32.to_be_bytes());
            out.extend((offset as u32).to_be_bytes());
            out.extend((data.len() as u32).to_be_bytes());
            offset += data.len();
            body.extend(data);
        }
        out.extend(body);
        out
    }

    #[test]
    fn font_validation() {
        let f = tiny_font();
        assert_eq!(validate_font(&f).unwrap(), "Test Sans");
        assert!(validate_font(b"hello world, not a font").is_err());
        assert!(validate_font(b"wOF2aaaaaaaaaaaa").is_err());
        let mut broken = f.clone();
        broken.truncate(40);
        assert!(validate_font(&broken).is_err());
        assert_eq!(safe_name("../My Font!", "ttf"), "My Font.ttf");
    }

    #[test]
    fn sound_signatures() {
        assert_eq!(sound_kind(b"RIFF\0\0\0\0WAVEfmt "), Some("wav"));
        assert_eq!(sound_kind(b"ID3\x04"), Some("mp3"));
        assert_eq!(sound_kind(&[0xFF, 0xFB, 0x90, 0]), Some("mp3"));
        assert_eq!(sound_kind(b"OggS\0"), Some("ogg"));
        assert_eq!(sound_kind(b"<html>"), None);
    }
}
