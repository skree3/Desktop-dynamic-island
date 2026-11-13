//! Текст со снимка экрана (без интернета: Windows OCR / tesseract на Linux)
//! и перевод по кнопке (translate.googleapis.com — уходит только этот текст).

use std::path::Path;

use crate::core::error::{AppError, AppResult};

#[cfg(windows)]
pub fn recognize(path: &Path) -> AppResult<String> {
    use windows::Graphics::Imaging::{BitmapDecoder, BitmapPixelFormat, SoftwareBitmap};
    use windows::Media::Ocr::OcrEngine;
    use windows::Storage::Streams::{DataWriter, InMemoryRandomAccessStream};
    let bytes = std::fs::read(path)?;
    let run = || -> windows::core::Result<String> {
        let stream = InMemoryRandomAccessStream::new()?;
        let writer = DataWriter::CreateDataWriter(&stream)?;
        writer.WriteBytes(&bytes)?;
        writer.StoreAsync()?.get()?;
        writer.FlushAsync()?.get()?;
        writer.DetachStream()?;
        stream.Seek(0)?;
        let decoder = BitmapDecoder::CreateAsync(&stream)?.get()?;
        let bmp = decoder.GetSoftwareBitmapAsync()?.get()?;
        let bmp = SoftwareBitmap::Convert(&bmp, BitmapPixelFormat::Bgra8)?;
        let engine = OcrEngine::TryCreateFromUserProfileLanguages()?;
        let result = engine.RecognizeAsync(&bmp)?.get()?;
        let mut lines = Vec::new();
        for line in result.Lines()? {
            lines.push(line.Text()?.to_string());
        }
        Ok(lines.join("\n"))
    };
    run().map_err(|e| AppError::System(format!("распознавание текста недоступно: {e}")))
}

#[cfg(target_os = "linux")]
pub fn recognize(path: &Path) -> AppResult<String> {
    let out = std::process::Command::new("tesseract")
        .arg(path)
        .arg("stdout")
        .args(["-l", "rus+eng"])
        .output()
        .map_err(|_| AppError::unsupported("установите tesseract-ocr и пакеты tesseract-ocr-rus (sudo apt install tesseract-ocr tesseract-ocr-rus)"))?;
    if !out.status.success() {
        return Err(AppError::System(String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("tesseract не сработал").to_string()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Перевод: русский текст — на английский, остальное — на русский.
pub fn translate(text: &str) -> AppResult<(String, String)> {
    let text: String = text.chars().take(4500).collect();
    if text.trim().is_empty() {
        return Err(AppError::invalid("нет текста"));
    }
    let cyr = text.chars().filter(|c| ('а'..='я').contains(&c.to_lowercase().next().unwrap_or(*c)) || *c == 'ё').count();
    let letters = text.chars().filter(|c| c.is_alphabetic()).count().max(1);
    let target = if cyr * 2 > letters { "en" } else { "ru" };
    let url = format!("https://translate.googleapis.com/translate_a/single?client=gtx&sl=auto&tl={target}&dt=t&q={}", enc(&text));
    let v: serde_json::Value = ureq::get(&url)
        .timeout(std::time::Duration::from_secs(15))
        .call()
        .map_err(|e| AppError::Other(format!("перевод недоступен: {e}")))?
        .into_json()
        .map_err(|e| AppError::Other(e.to_string()))?;
    let out: String = v.get(0).and_then(|a| a.as_array()).map(|parts| parts.iter().filter_map(|p| p.get(0).and_then(|s| s.as_str())).collect()).unwrap_or_default();
    if out.is_empty() {
        return Err(AppError::Other("перевод пустой".into()));
    }
    Ok((out, target.to_string()))
}
