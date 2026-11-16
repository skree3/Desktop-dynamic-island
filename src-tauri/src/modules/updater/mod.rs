//! Обновления из GitHub Releases (skree3/Desktop-dynamic-island).
//! Проверка — при запуске и раз в сутки (без каких-либо данных пользователя),
//! отключается в настройках. Если api.github.com недоступен или подменён,
//! адрес узнаётся через DNS-over-HTTPS, а версия — через jsDelivr. Установка:
//! * Windows — скачивается установщик, сверяется SHA-256 из SHA256SUMS.txt
//!   релиза и запускается, приложение закрывается;
//! * Linux AppImage — новый файл подменяет текущий и перезапускается;
//! * Linux .deb/.rpm — открывается страница релиза (нужны права администратора).

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::sync::Mutex;
use std::time::Duration;
use tauri::AppHandle;

use crate::core::error::{AppError, AppResult};
use crate::core::state::Shared;
use crate::core::task::TaskSlot;
use crate::events::{IslandNotification, NotificationAction, NotificationKind, Priority};
use crate::logging::category;

pub const REPO: &str = "skree3/Desktop-dynamic-island";
pub const ASSET_WINDOWS: &str = "DesktopIsland-windows-setup.exe";
pub const ASSET_APPIMAGE: &str = "DesktopIsland-linux-x64.AppImage";
const SUMS: &str = "SHA256SUMS.txt";
const MAX_DOWNLOAD: u64 = 300 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current: String,
    pub latest: Option<String>,
    pub available: bool,
    pub notes: String,
    pub page_url: String,
    /// installer | appimage | page
    pub method: String,
    pub checked_at: i64,
}

#[derive(Default)]
pub struct UpdaterModule {
    task: TaskSlot,
    last: Mutex<Option<UpdateInfo>>,
    assets: Mutex<Vec<(String, String)>>,
}

/// Краткое описание изменений из текста релиза: первые пункты списка без разметки.
pub fn summary(notes: &str, max_items: usize) -> String {
    let items: Vec<String> = notes
        .lines()
        .map(|l| l.trim())
        .filter_map(|l| l.strip_prefix("- ").or_else(|| l.strip_prefix("* ")).or_else(|| l.strip_prefix("• ")))
        .map(|l| l.replace("**", "").replace('`', "").trim().trim_end_matches('.').to_string())
        .filter(|l| !l.is_empty())
        .take(max_items)
        .collect();
    if !items.is_empty() {
        return items.join(", ");
    }
    notes
        .lines()
        .map(|l| l.trim().trim_start_matches('#').trim())
        .find(|l| !l.is_empty() && !l.contains("Changelog") && !l.contains("What's Changed") && !l.starts_with("http"))
        .unwrap_or("")
        .replace("**", "")
        .chars()
        .take(120)
        .collect()
}

/// Напоминание о той же версии — не чаще раза в сутки (и после перезапуска тоже).
const REMIND_AFTER_MS: i64 = 24 * 3600 * 1000;
const CHECK_EVERY: Duration = Duration::from_secs(24 * 3600);
/// Нет связи с GitHub — пробуем снова через полчаса.
const RETRY_AFTER_ERROR: Duration = Duration::from_secs(30 * 60);
/// Уведомление не показалось (игровой режим, «Не беспокоить») — повтор через 10 минут.
const RETRY_SUPPRESSED: Duration = Duration::from_secs(10 * 60);

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(10)).timeout(Duration::from_secs(600)).build()
}

/// Адреса хоста через DNS-over-HTTPS (Cloudflare / Google по IP-адресу).
/// Нужен, когда сеть, файл hosts или провайдер подменяют адрес GitHub:
/// тогда вместо api.github.com отвечает чужой сервер (например, dns.google).
fn doh_resolve(host: &str) -> Option<Vec<std::net::IpAddr>> {
    use std::collections::HashMap;
    use std::sync::OnceLock;
    use std::time::Instant;
    static CACHE: OnceLock<Mutex<HashMap<String, (Instant, Vec<std::net::IpAddr>)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    if let Some((at, ips)) = cache.lock().unwrap_or_else(|p| p.into_inner()).get(host) {
        if at.elapsed() < Duration::from_secs(600) {
            return Some(ips.clone());
        }
    }
    let plain = ureq::AgentBuilder::new().timeout(Duration::from_secs(8)).build();
    let urls = [
        format!("https://1.1.1.1/dns-query?name={host}&type=A"),
        format!("https://8.8.8.8/resolve?name={host}&type=A"),
        format!("https://1.0.0.1/dns-query?name={host}&type=A"),
    ];
    for url in urls {
        let Ok(resp) = plain.get(&url).set("Accept", "application/dns-json").call() else { continue };
        let Ok(v) = resp.into_json::<serde_json::Value>() else { continue };
        let ips: Vec<std::net::IpAddr> = v["Answer"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|a| a["type"].as_u64() == Some(1))
            .filter_map(|a| a["data"].as_str()?.parse().ok())
            .collect();
        if !ips.is_empty() {
            cache.lock().unwrap_or_else(|p| p.into_inner()).insert(host.to_string(), (Instant::now(), ips.clone()));
            return Some(ips);
        }
    }
    None
}

/// Агент, который узнаёт адреса через DNS-over-HTTPS, минуя системный DNS и hosts.
fn doh_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout(Duration::from_secs(600))
        .resolver(|netloc: &str| -> std::io::Result<Vec<std::net::SocketAddr>> {
            let (host, port) = netloc.rsplit_once(':').map(|(h, p)| (h, p.parse().unwrap_or(443))).unwrap_or((netloc, 443));
            if let Ok(ip) = host.trim_matches(|c| c == '[' || c == ']').parse::<std::net::IpAddr>() {
                return Ok(vec![std::net::SocketAddr::new(ip, port)]);
            }
            doh_resolve(host)
                .map(|ips| ips.into_iter().map(|ip| std::net::SocketAddr::new(ip, port)).collect())
                .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "DNS-over-HTTPS не ответил"))
        })
        .build()
}

/// GET: сначала обычным путём, при сетевой ошибке — через DNS-over-HTTPS.
fn http_get(url: &str, accept: Option<&str>) -> Result<ureq::Response, ureq::Error> {
    let send = |a: &ureq::Agent| {
        let mut r = a.get(url).set("User-Agent", "DesktopIsland-Updater");
        if let Some(acc) = accept {
            r = r.set("Accept", acc);
        }
        r.call()
    };
    match send(&agent()) {
        Err(ureq::Error::Transport(first)) => {
            tracing::info!(error = %first, url, "обычное соединение не удалось — пробуем DNS-over-HTTPS");
            send(&doh_agent()).map_err(|e| match e {
                // Показываем исходную причину: она понятнее для пользователя.
                ureq::Error::Transport(_) => ureq::Error::Transport(first),
                other => other,
            })
        }
        other => other,
    }
}

/// Понятное описание сетевой ошибки.
fn explain(e: &ureq::Error) -> String {
    let raw = e.to_string();
    let low = raw.to_lowercase();
    if low.contains("certificate") {
        "соединение с GitHub подменяется — вместо github.com отвечает другой сервер. Обычно это DNS, файл hosts, антивирус или фильтр провайдера. Можно скачать новую версию вручную со страницы релизов".into()
    } else if low.contains("dns") || low.contains("connection failed") || low.contains("timed out") || low.contains("connect") {
        "нет связи с GitHub. Проверьте интернет или VPN".into()
    } else {
        raw
    }
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
struct Notified {
    version: String,
    at: i64,
}

fn notified_path(st: &Shared) -> std::path::PathBuf {
    st.paths.data.join("update-notified.json")
}

fn load_notified(st: &Shared) -> Option<Notified> {
    std::fs::read_to_string(notified_path(st)).ok().and_then(|t| serde_json::from_str(&t).ok())
}

fn save_notified(st: &Shared, n: &Notified) {
    if let Ok(t) = serde_json::to_string(n) {
        let _ = std::fs::write(notified_path(st), t);
    }
}

/// Последняя версия через jsDelivr — если api.github.com недоступен.
/// Список берётся из тегов репозитория, описание — из .github/release-notes.md.
fn latest_via_jsdelivr() -> AppResult<(String, String)> {
    let resp = http_get(&format!("https://data.jsdelivr.com/v1/packages/gh/{REPO}"), Some("application/json")).map_err(|e| AppError::system(explain(&e)))?;
    let json: serde_json::Value = resp.into_json().map_err(|e| AppError::system(e.to_string()))?;
    let best = json["versions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| semver::Version::parse(v["version"].as_str()?.trim_start_matches('v')).ok())
        .filter(|v| v.pre.is_empty())
        .max()
        .ok_or_else(|| AppError::NotFound("в репозитории пока нет опубликованных релизов".into()))?;
    let tag = best.to_string();
    let notes = http_get(&format!("https://cdn.jsdelivr.net/gh/{REPO}@v{tag}/.github/release-notes.md"), None)
        .ok()
        .and_then(|r| r.into_string().ok())
        .unwrap_or_default();
    Ok((tag, notes))
}

fn install_method() -> &'static str {
    if cfg!(windows) {
        "installer"
    } else if std::env::var_os("APPIMAGE").is_some() {
        "appimage"
    } else {
        "page"
    }
}

impl UpdaterModule {
    pub fn last(&self) -> Option<UpdateInfo> {
        self.last.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// Последний релиз через api.github.com: (версия, описание, страница, файлы).
    fn latest_via_api() -> AppResult<(String, String, String, Vec<(String, String)>)> {
        let resp = http_get(&format!("https://api.github.com/repos/{REPO}/releases/latest"), Some("application/vnd.github+json")).map_err(|e| match e {
            ureq::Error::Status(404, _) => AppError::NotFound("в репозитории пока нет опубликованных релизов".into()),
            other => AppError::system(format!("не удалось проверить обновления: {}", explain(&other))),
        })?;
        let json: serde_json::Value = resp.into_json().map_err(|e| AppError::system(e.to_string()))?;
        let tag = json["tag_name"].as_str().unwrap_or("").trim_start_matches('v').to_string();
        let assets = json["assets"]
            .as_array()
            .map(|a| a.iter().filter_map(|x| Some((x["name"].as_str()?.to_string(), x["browser_download_url"].as_str()?.to_string()))).collect())
            .unwrap_or_default();
        let notes = json["body"].as_str().unwrap_or("").to_string();
        let page = json["html_url"].as_str().map(str::to_string).unwrap_or_else(|| format!("https://github.com/{REPO}/releases/latest"));
        Ok((tag, notes, page, assets))
    }

    pub fn check_blocking(&self) -> AppResult<UpdateInfo> {
        let (tag, notes, page, assets) = match Self::latest_via_api() {
            Ok(x) => x,
            Err(e @ AppError::NotFound(_)) => return Err(e),
            Err(api_err) => {
                tracing::info!(error = %api_err, "api.github.com недоступен — проверяем через jsDelivr");
                let (tag, notes) = latest_via_jsdelivr().map_err(|_| api_err)?;
                // Имена файлов в релизах постоянные, адреса собираем сами.
                let assets = [ASSET_WINDOWS, ASSET_APPIMAGE, SUMS]
                    .iter()
                    .map(|n| (n.to_string(), format!("https://github.com/{REPO}/releases/download/v{tag}/{n}")))
                    .collect();
                (tag.clone(), notes, format!("https://github.com/{REPO}/releases/tag/v{tag}"), assets)
            }
        };
        let current = env!("CARGO_PKG_VERSION").to_string();
        let available = match (semver::Version::parse(&tag), semver::Version::parse(&current)) {
            (Ok(l), Ok(c)) => l > c,
            _ => false,
        };
        *self.assets.lock().unwrap_or_else(|p| p.into_inner()) = assets;
        let info = UpdateInfo {
            current,
            latest: if tag.is_empty() { None } else { Some(tag) },
            available,
            notes: notes.chars().take(4000).collect(),
            page_url: page,
            method: install_method().into(),
            checked_at: crate::database::now_ms(),
        };
        *self.last.lock().unwrap_or_else(|p| p.into_inner()) = Some(info.clone());
        Ok(info)
    }

    fn asset_url(&self, name: &str) -> Option<String> {
        self.assets.lock().ok()?.iter().find(|(n, _)| n == name).map(|(_, u)| u.clone())
    }

    fn download(url: &str, dest: &std::path::Path) -> AppResult<String> {
        let resp = http_get(url, None).map_err(|e| AppError::system(format!("загрузка не удалась: {}", explain(&e))))?;
        let mut reader = resp.into_reader().take(MAX_DOWNLOAD);
        let mut file = std::fs::File::create(dest)?;
        let mut hasher = Sha256::new();
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = reader.read(&mut buf)?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
            std::io::Write::write_all(&mut file, &buf[..n])?;
        }
        Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
    }

    fn expected_hash(&self, asset: &str) -> AppResult<String> {
        let url = self.asset_url(SUMS).ok_or_else(|| AppError::system("в релизе нет SHA256SUMS.txt — установка отменена"))?;
        let text = http_get(&url, None).map_err(|e| AppError::system(explain(&e)))?.into_string().map_err(|e| AppError::system(e.to_string()))?;
        text.lines()
            .find_map(|l| {
                let mut parts = l.split_whitespace();
                let hash = parts.next()?;
                let name = parts.next()?.trim_start_matches('*');
                (name == asset).then(|| hash.to_lowercase())
            })
            .ok_or_else(|| AppError::system("контрольная сумма для файла не найдена"))
    }

    /// Скачивает и запускает обновление. Возвращает true, если приложению нужно завершиться.
    pub fn install_blocking(&self) -> AppResult<bool> {
        let info = match self.last() {
            Some(i) => i,
            None => self.check_blocking()?,
        };
        if !info.available {
            return Err(AppError::invalid("установлена последняя версия"));
        }
        match install_method() {
            "installer" => {
                let url = self.asset_url(ASSET_WINDOWS).ok_or_else(|| AppError::NotFound("в релизе нет установщика для Windows".into()))?;
                let dir = std::env::temp_dir().join("desktop-island-update");
                std::fs::create_dir_all(&dir)?;
                let dest = dir.join(ASSET_WINDOWS);
                let hash = Self::download(&url, &dest)?;
                if hash != self.expected_hash(ASSET_WINDOWS)? {
                    let _ = std::fs::remove_file(&dest);
                    return Err(AppError::system("контрольная сумма не совпала — файл повреждён, установка отменена"));
                }
                std::process::Command::new(&dest).spawn().map_err(|e| AppError::system(format!("не удалось запустить установщик: {e}")))?;
                Ok(true)
            }
            "appimage" => {
                let current = std::path::PathBuf::from(std::env::var_os("APPIMAGE").unwrap_or_default());
                let url = self.asset_url(ASSET_APPIMAGE).ok_or_else(|| AppError::NotFound("в релизе нет AppImage".into()))?;
                let tmp = current.with_extension("new");
                let hash = Self::download(&url, &tmp)?;
                if hash != self.expected_hash(ASSET_APPIMAGE)? {
                    let _ = std::fs::remove_file(&tmp);
                    return Err(AppError::system("контрольная сумма не совпала — файл повреждён, установка отменена"));
                }
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))?;
                }
                std::fs::rename(&tmp, &current)?;
                std::process::Command::new(&current).spawn().map_err(|e| AppError::system(e.to_string()))?;
                Ok(true)
            }
            _ => {
                crate::platform::os::shell::open_uri(&info.page_url)?;
                Ok(false)
            }
        }
    }

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
            tokio::time::sleep(Duration::from_secs(15)).await;
            loop {
                let st2 = st.clone();
                let wait = match tauri::async_runtime::spawn_blocking(move || st2.updater.check_blocking()).await {
                    Ok(Ok(info)) if info.available => st.updater.notify(&st, &info),
                    Ok(Ok(_)) => CHECK_EVERY,
                    Ok(Err(e)) => {
                        st.log.warn(category::APP, "updater", format!("проверка обновлений не удалась: {e}"));
                        RETRY_AFTER_ERROR
                    }
                    Err(_) => RETRY_AFTER_ERROR,
                };
                tokio::time::sleep(wait).await;
            }
        });
    }

    /// Показывает в острове «Доступно обновление» не чаще раза в сутки для одной версии.
    /// Возвращает, через сколько проверить снова.
    fn notify(&self, st: &Shared, info: &UpdateInfo) -> Duration {
        let latest = info.latest.clone().unwrap_or_default();
        let now = crate::database::now_ms();
        if let Some(n) = load_notified(st).filter(|n| n.version == latest) {
            let left = REMIND_AFTER_MS - (now - n.at);
            if left > 0 {
                return Duration::from_millis(left as u64).min(CHECK_EVERY);
            }
        }
        let changes = summary(&info.notes, 3);
        let outcome = st.events.push(IslandNotification {
            stack: Vec::new(),
            count: 1,
            id: uuid::Uuid::new_v4().to_string(),
            kind: NotificationKind::Internal,
            source: "updater".into(),
            app_name: "Desktop Island".into(),
            icon: None,
            title: format!("Обновление {} → {latest}", info.current),
            body: if changes.is_empty() { "Нажмите «Обновить», чтобы установить".into() } else { changes },
            priority: Priority::Normal,
            timestamp: now,
            actions: vec![
                NotificationAction { id: "update".into(), label: "Обновить".into() },
                NotificationAction { id: "dismiss".into(), label: "Позже".into() },
            ],
            open_target: None,
            duration_ms: 30_000,
        });
        if matches!(outcome, crate::events::queue::PushOutcome::HistoryOnly(_) | crate::events::queue::PushOutcome::Dropped(_)) {
            // Игровой режим или «Не беспокоить»: не считаем показанным, попробуем позже.
            return RETRY_SUPPRESSED;
        }
        st.log.info(category::APP, "updater", format!("доступна версия {latest}"));
        save_notified(st, &Notified { version: latest, at: now });
        CHECK_EVERY
    }
}

#[cfg(test)]
mod tests {
    /// Сетевые проверки обходных путей: `cargo test --lib updater -- --ignored`.
    #[test]
    #[ignore]
    fn network_fallbacks() {
        let ips = super::doh_resolve("api.github.com").expect("DoH не ответил");
        assert!(!ips.is_empty());
        let r = super::doh_agent()
            .get(&format!("https://api.github.com/repos/{}/releases/latest", super::REPO))
            .set("User-Agent", "DesktopIsland-Updater")
            .call()
            .expect("api.github.com через DoH");
        let v: serde_json::Value = r.into_json().unwrap();
        assert!(v["tag_name"].as_str().unwrap().starts_with('v'));
        let (tag, notes) = super::latest_via_jsdelivr().expect("jsDelivr");
        println!("jsDelivr: {tag}, notes: {}", notes.lines().next().unwrap_or(""));
        assert!(semver::Version::parse(&tag).is_ok());
    }

    #[test]
    fn release_summary() {
        let notes = "## Что нового

- **Шрифты**: 10 встроенных.
- Звуки уведомлений и таймера
- Загрузки `Steam`
- Ещё";
        assert_eq!(super::summary(notes, 3), "Шрифты: 10 встроенных, Звуки уведомлений и таймера, Загрузки Steam");
        assert_eq!(super::summary("Исправления ошибок", 3), "Исправления ошибок");
        assert_eq!(super::summary("• Раз
• Два", 3), "Раз, Два");
    }
}

/// Запуск установки из UI/уведомления: при успехе приложение завершается.
pub fn install_and_quit(app: &AppHandle, st: &Shared) -> AppResult<()> {
    let quit = st.updater.install_blocking()?;
    if quit {
        st.log.info(category::APP, "updater", "запущена установка обновления");
        crate::tray::quit(app, st);
    }
    Ok(())
}
