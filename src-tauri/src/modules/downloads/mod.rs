//! Загрузки браузеров: пока файл скачивается, свёрнутый остров показывает его имя,
//! размер и скорость; по окончании приходит уведомление с кнопками «Открыть»,
//! «В папке» и «На полку».
//!
//! Браузеры не сообщают прогресс другим программам, поэтому следим за папкой
//! «Загрузки»: Chrome/Edge/Яндекс пишут во временный *.crdownload, Firefox — *.part,
//! Opera — *.opdownload. Общий размер файла заранее неизвестен — показываем
//! скачанный объём и скорость. Опрос раз в 1,5 с — это один listing папки.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::AppHandle;

use crate::core::state::Shared;
use crate::core::task::TaskSlot;
use crate::database::now_ms;
use crate::events::{internal_notification, Activity, NotificationAction, Priority};

const TEMP_EXT: &[&str] = &["crdownload", "part", "opdownload", "partial"];
/// Файл, который не растёт дольше этого времени, считаем брошенным.
const STALL_MS: i64 = 20_000;

#[derive(Default)]
pub struct DownloadsModule {
    task: TaskSlot,
}

struct Tracked {
    size: u64,
    changed_at: i64,
    speed: f64,
    /// Имя готового файла (если его можно узнать по временному).
    final_name: Option<String>,
}

pub fn downloads_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        use windows::Win32::UI::Shell::{FOLDERID_Downloads, SHGetKnownFolderPath, KF_FLAG_DEFAULT};
        unsafe {
            let p = SHGetKnownFolderPath(&FOLDERID_Downloads, KF_FLAG_DEFAULT, None).ok()?;
            let s = p.to_string().ok();
            windows::Win32::System::Com::CoTaskMemFree(Some(p.0 as _));
            s.map(PathBuf::from)
        }
    }
    #[cfg(not(windows))]
    {
        let home = PathBuf::from(std::env::var_os("HOME")?);
        let cfg = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".config"));
        if let Ok(text) = std::fs::read_to_string(cfg.join("user-dirs.dirs")) {
            for line in text.lines() {
                if let Some(v) = line.trim().strip_prefix("XDG_DOWNLOAD_DIR=") {
                    let v = v.trim_matches('"').replace("$HOME", &home.to_string_lossy());
                    return Some(PathBuf::from(v));
                }
            }
        }
        ["Downloads", "Загрузки"].iter().map(|d| home.join(d)).find(|p| p.is_dir())
    }
}

fn is_temp(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()).map(|e| TEMP_EXT.contains(&e.to_ascii_lowercase().as_str())).unwrap_or(false)
}

/// «file.zip.crdownload» → «file.zip»; «Unconfirmed 123.crdownload» → None.
fn final_name(p: &Path) -> Option<String> {
    let stem = p.file_stem()?.to_string_lossy().to_string();
    if stem.starts_with("Unconfirmed ") || stem.starts_with("Не подтверждено ") || stem.is_empty() {
        return None;
    }
    Some(stem)
}

pub fn format_size(bytes: f64) -> String {
    const U: [&str; 4] = ["Б", "КБ", "МБ", "ГБ"];
    let mut v = bytes;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{} {}", v as u64, U[0])
    } else {
        format!("{:.1} {}", v, U[i]).replace('.', ",")
    }
}

impl DownloadsModule {
    pub fn set_running(&self, app: &AppHandle, st: &Shared, run: bool) {
        if run == self.task.is_running() {
            return;
        }
        if !run {
            self.task.stop();
            st.events.clear_activity("download");
            return;
        }
        let _ = app;
        let st = st.clone();
        self.task.start(async move {
            let Some(dir) = downloads_dir() else { return };
            let mut tracked: HashMap<PathBuf, Tracked> = HashMap::new();
            let mut first = true;
            loop {
                let d = dir.clone();
                let listing = tauri::async_runtime::spawn_blocking(move || {
                    let mut temps = vec![];
                    if let Ok(rd) = std::fs::read_dir(&d) {
                        for e in rd.flatten() {
                            let p = e.path();
                            if is_temp(&p) {
                                // Размер — у самого файла: в записи каталога Windows он обновляется с задержкой.
                                let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
                                temps.push((p, size));
                            }
                        }
                    }
                    temps
                })
                .await
                .unwrap_or_default();
                let now = now_ms();

                // Новые и растущие файлы.
                for (p, size) in &listing {
                    match tracked.get_mut(p) {
                        Some(t) => {
                            if *size != t.size {
                                let dt = ((now - t.changed_at).max(1)) as f64 / 1000.0;
                                let inst = size.saturating_sub(t.size) as f64 / dt;
                                t.speed = if t.speed == 0.0 { inst } else { t.speed * 0.6 + inst * 0.4 };
                                t.size = *size;
                                t.changed_at = now;
                            }
                        }
                        None => {
                            // Файлы, лежавшие до запуска, не показываем, пока они не начнут расти.
                            let changed_at = if first { 0 } else { now };
                            tracked.insert(p.clone(), Tracked { size: *size, changed_at, speed: 0.0, final_name: final_name(p) });
                        }
                    }
                }

                // Исчезнувшие временные файлы: загрузка завершена или отменена.
                let gone: Vec<PathBuf> = tracked.keys().filter(|k| !listing.iter().any(|(p, _)| p == *k)).cloned().collect();
                for g in gone {
                    let Some(t) = tracked.remove(&g) else { continue };
                    if now - t.changed_at > STALL_MS {
                        continue; // брошенный файл удалили
                    }
                    let done = match &t.final_name {
                        Some(n) => Some(dir.join(n)).filter(|p| p.is_file()),
                        None => newest_file(&dir, now),
                    };
                    if let Some(path) = done {
                        st.downloads.finished(&st, &path);
                    }
                }

                // Активность в острове.
                let active: Vec<&Tracked> = tracked.values().filter(|t| now - t.changed_at <= STALL_MS && t.changed_at > 0).collect();
                if active.is_empty() {
                    st.events.clear_activity("download");
                } else {
                    let total: u64 = active.iter().map(|t| t.size).sum();
                    let speed: f64 = active.iter().map(|t| t.speed).sum();
                    let title = if active.len() == 1 {
                        active[0].final_name.clone().unwrap_or_else(|| "Загрузка".into())
                    } else {
                        format!("Загрузки: {}", active.len())
                    };
                    let mut subtitle = format_size(total as f64);
                    if speed > 0.0 {
                        subtitle.push_str(&format!(" · {}/с", format_size(speed)));
                    }
                    st.events.set_activity(Activity {
                        id: "download".into(),
                        kind: "download".into(),
                        priority: Priority::Normal,
                        title,
                        subtitle: Some(subtitle),
                        icon: None,
                        progress: None,
                        updated_at: now,
                        started_at: None,
                    });
                }
                first = false;
                let wait = if tracked.is_empty() { 2500 } else { 1500 };
                tokio::time::sleep(Duration::from_millis(wait)).await;
            }
        });
    }

    fn finished(&self, st: &Shared, path: &Path) {
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let size = std::fs::metadata(path).map(|m| format_size(m.len() as f64)).unwrap_or_default();
        let mut n = internal_notification(
            &format!("download:{}", path.to_string_lossy()),
            "Загрузки",
            "Загрузка завершена",
            if size.is_empty() { name } else { format!("{name} · {size}") },
            Priority::Normal,
        );
        n.actions = vec![
            NotificationAction { id: "dl-open".into(), label: "Открыть".into() },
            NotificationAction { id: "dl-reveal".into(), label: "В папке".into() },
            NotificationAction { id: "dl-shelf".into(), label: "На полку".into() },
        ];
        st.events.push(n);
    }
}

/// Самый свежий (последние 5 с) обычный файл в папке — имя готовой загрузки Chrome,
/// которая до конца называлась «Unconfirmed NNN.crdownload».
fn newest_file(dir: &Path, now: i64) -> Option<PathBuf> {
    let mut best: Option<(i64, PathBuf)> = None;
    for e in std::fs::read_dir(dir).ok()?.flatten() {
        let p = e.path();
        if !p.is_file() || is_temp(&p) {
            continue;
        }
        let Ok(m) = e.metadata().and_then(|m| m.modified()) else { continue };
        let ms = m.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0);
        if now - ms < 5000 && best.as_ref().map(|b| ms > b.0).unwrap_or(true) {
            best = Some((ms, p));
        }
    }
    best.map(|b| b.1)
}

/// Действия уведомления «Загрузка завершена».
pub fn action(st: &Shared, source: &str, action: &str) -> crate::core::error::AppResult<()> {
    let Some(path) = source.strip_prefix("download:") else { return Ok(()) };
    match action {
        "dl-open" => crate::platform::os::shell::open_uri(path),
        "dl-reveal" => crate::modules::shelf::reveal_path(path),
        "dl-shelf" => crate::modules::shelf::add(st, &[path.to_string()]).map(|_| ()),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(final_name(Path::new("/d/setup.exe.crdownload")).as_deref(), Some("setup.exe"));
        assert_eq!(final_name(Path::new("/d/Unconfirmed 1234.crdownload")), None);
        assert_eq!(final_name(Path::new("/d/movie.mkv.part")).as_deref(), Some("movie.mkv"));
        assert!(is_temp(Path::new("a.zip.CRDOWNLOAD")));
        assert!(!is_temp(Path::new("a.zip")));
        assert_eq!(format_size(1536.0 * 1024.0), "1,5 МБ");
    }
}
