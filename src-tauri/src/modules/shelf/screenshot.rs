//! Снимок области экрана: результат кладётся на полку и в буфер обмена.
//! Windows: системный инструмент выделения («Ножницы», ms-screenclip:) кладёт
//! картинку в буфер обмена — забираем её оттуда и сохраняем в PNG.
//! Linux: портал xdg-desktop-portal Screenshot (интерактивный режим GNOME/KDE)
//! возвращает файл — добавляем его на полку и копируем картинку в буфер.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::core::error::{AppError, AppResult};
use crate::core::state::Shared;
use crate::events::{internal_notification, Priority};

static BUSY: AtomicBool = AtomicBool::new(false);

fn file_name() -> String {
    format!("Снимок {}.png", local_stamp())
}

/// Местное время «2026-11-12 20-41-05» для имени файла.
fn local_stamp() -> String {
    #[cfg(windows)]
    {
        let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
        format!("{:04}-{:02}-{:02} {:02}-{:02}-{:02}", t.wYear, t.wMonth, t.wDay, t.wHour, t.wMinute, t.wSecond)
    }
    #[cfg(not(windows))]
    {
        std::process::Command::new("date")
            .arg("+%Y-%m-%d %H-%M-%S")
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| crate::database::now_ms().to_string())
    }
}

fn screenshots_dir(st: &Shared) -> AppResult<PathBuf> {
    let dir = st.paths.data.join("screenshots");
    std::fs::create_dir_all(&dir).map_err(|e| AppError::system(e.to_string()))?;
    Ok(dir)
}

fn done(st: &Shared, path: &std::path::Path) -> AppResult<()> {
    super::add(st, &[path.to_string_lossy().to_string()])?;
    let mut n = internal_notification("screenshot", "Полка", "Снимок на полке", "Картинка также скопирована в буфер обмена", Priority::Low);
    n.duration_ms = 2500;
    st.events.push(n);
    Ok(())
}

/// Запускает выделение области и ждёт результата (до 2 минут).
pub async fn capture(st: Shared) -> AppResult<bool> {
    if BUSY.swap(true, Ordering::SeqCst) {
        return Ok(false);
    }
    let res = capture_inner(&st).await;
    BUSY.store(false, Ordering::SeqCst);
    res
}

#[cfg(windows)]
async fn capture_inner(st: &Shared) -> AppResult<bool> {
    use crate::platform::os::clipboard::sequence;
    let before = sequence();
    crate::platform::os::shell::open_uri("ms-screenclip:")?;
    let deadline = crate::database::now_ms() + 120_000;
    while crate::database::now_ms() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        if sequence() == before {
            continue;
        }
        let img = tauri::async_runtime::spawn_blocking(|| crate::util::clipboard::with(|c| c.get_image().ok()).ok().flatten()).await.ok().flatten();
        let Some(img) = img else { continue };
        let png = crate::util::image::encode_png_rgba(img.width as u32, img.height as u32, &img.bytes).ok_or_else(|| AppError::system("не удалось сохранить снимок"))?;
        let path = screenshots_dir(st)?.join(file_name());
        std::fs::write(&path, png).map_err(|e| AppError::system(e.to_string()))?;
        done(st, &path)?;
        return Ok(true);
    }
    Ok(false)
}

#[cfg(not(windows))]
async fn capture_inner(st: &Shared) -> AppResult<bool> {
    use futures_util::StreamExt;
    use std::collections::HashMap;
    use zbus::zvariant::{OwnedValue, Value};

    let conn = zbus::Connection::session().await.map_err(|e| AppError::unsupported(format!("D-Bus недоступен: {e}")))?;
    let token = format!("di{}", crate::database::now_ms());
    let sender = conn.unique_name().map(|n| n.trim_start_matches(':').replace('.', "_")).unwrap_or_default();
    let req_path = format!("/org/freedesktop/portal/desktop/request/{sender}/{token}");
    let request = zbus::Proxy::new(&conn, "org.freedesktop.portal.Desktop", req_path.as_str(), "org.freedesktop.portal.Request")
        .await
        .map_err(|e| AppError::unsupported(e.to_string()))?;
    let mut responses = request.receive_signal("Response").await.map_err(|e| AppError::unsupported(e.to_string()))?;
    let portal = zbus::Proxy::new(&conn, "org.freedesktop.portal.Desktop", "/org/freedesktop/portal/desktop", "org.freedesktop.portal.Screenshot")
        .await
        .map_err(|e| AppError::unsupported(e.to_string()))?;
    let mut opts: HashMap<&str, Value> = HashMap::new();
    opts.insert("handle_token", Value::from(token.as_str()));
    opts.insert("interactive", Value::from(true));
    let _: zbus::zvariant::OwnedObjectPath =
        portal.call("Screenshot", &("", opts)).await.map_err(|e| AppError::unsupported(format!("портал скриншотов недоступен: {e}")))?;
    let Ok(Some(msg)) = tokio::time::timeout(std::time::Duration::from_secs(180), responses.next()).await else { return Ok(false) };
    let (code, results): (u32, HashMap<String, OwnedValue>) = msg.body().deserialize().map_err(|e| AppError::system(e.to_string()))?;
    if code != 0 {
        return Ok(false); // пользователь отменил
    }
    let Some(uri) = results.get("uri").and_then(|v| crate::platform::linux::val_str(v)) else { return Ok(false) };
    let path = PathBuf::from(percent_decode(uri.trim_start_matches("file://")));
    // Картинку — в буфер обмена, как на Windows.
    let p2 = path.clone();
    let _ = tauri::async_runtime::spawn_blocking(move || copy_png_to_clipboard(&p2)).await;
    let _ = screenshots_dir(st);
    done(st, &path)?;
    Ok(true)
}

#[cfg(not(windows))]
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

#[cfg(not(windows))]
fn copy_png_to_clipboard(path: &std::path::Path) -> Option<()> {
    let file = std::fs::File::open(path).ok()?;
    let mut dec = png::Decoder::new(std::io::BufReader::new(file));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).ok()?;
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => buf[..info.buffer_size()].to_vec(),
        png::ColorType::Rgb => buf[..info.buffer_size()].chunks(3).flat_map(|c| [c[0], c[1], c[2], 255]).collect(),
        _ => return None,
    };
    let img = arboard::ImageData { width: info.width as usize, height: info.height as usize, bytes: rgba.into() };
    crate::util::clipboard::with(|c| c.set_image(img).ok()).ok().flatten()
}

#[cfg(test)]
mod tests {
    #[test]
    fn date_format() {
        let s = super::local_stamp();
        assert_eq!(s.len(), 19);
        assert!(s.starts_with("20"));
    }
}
