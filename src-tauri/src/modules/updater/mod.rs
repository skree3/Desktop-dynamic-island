//! Обновления из GitHub Releases (skree3/Desktop-dynamic-island).
//! Проверка — один запрос к api.github.com раз в 6 часов (без каких-либо
//! данных пользователя), отключается в настройках. Установка:
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
    notified: Mutex<Option<String>>,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new().timeout_connect(Duration::from_secs(10)).timeout(Duration::from_secs(600)).build()
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

    pub fn check_blocking(&self) -> AppResult<UpdateInfo> {
        let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
        let resp = agent()
            .get(&url)
            .set("User-Agent", "DesktopIsland-Updater")
            .set("Accept", "application/vnd.github+json")
            .call()
            .map_err(|e| match e {
                ureq::Error::Status(404, _) => AppError::NotFound("в репозитории пока нет опубликованных релизов".into()),
                other => AppError::system(format!("не удалось проверить обновления: {other}")),
            })?;
        let json: serde_json::Value = resp.into_json().map_err(|e| AppError::system(e.to_string()))?;
        let tag = json["tag_name"].as_str().unwrap_or("").trim_start_matches('v').to_string();
        let current = env!("CARGO_PKG_VERSION").to_string();
        let available = match (semver::Version::parse(&tag), semver::Version::parse(&current)) {
            (Ok(l), Ok(c)) => l > c,
            _ => false,
        };
        let assets: Vec<(String, String)> = json["assets"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|x| Some((x["name"].as_str()?.to_string(), x["browser_download_url"].as_str()?.to_string())))
                    .collect()
            })
            .unwrap_or_default();
        *self.assets.lock().unwrap_or_else(|p| p.into_inner()) = assets;
        let info = UpdateInfo {
            current,
            latest: if tag.is_empty() { None } else { Some(tag) },
            available,
            notes: json["body"].as_str().unwrap_or("").chars().take(4000).collect(),
            page_url: json["html_url"].as_str().unwrap_or(&format!("https://github.com/{REPO}/releases/latest")).to_string(),
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
        let resp = agent().get(url).set("User-Agent", "DesktopIsland-Updater").call().map_err(|e| AppError::system(format!("загрузка не удалась: {e}")))?;
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
        let text = agent().get(&url).set("User-Agent", "DesktopIsland-Updater").call().map_err(|e| AppError::system(e.to_string()))?.into_string().map_err(|e| AppError::system(e.to_string()))?;
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
            tokio::time::sleep(Duration::from_secs(30)).await;
            loop {
                let st2 = st.clone();
                match tauri::async_runtime::spawn_blocking(move || st2.updater.check_blocking()).await {
                    Ok(Ok(info)) if info.available => {
                        let latest = info.latest.clone().unwrap_or_default();
                        let mut notified = st.updater.notified.lock().unwrap_or_else(|p| p.into_inner());
                        if notified.as_deref() != Some(latest.as_str()) {
                            *notified = Some(latest.clone());
                            drop(notified);
                            st.log.info(category::APP, "updater", format!("доступна версия {latest}"));
                            st.events.push(IslandNotification { stack: Vec::new(), count: 1,
                                id: uuid::Uuid::new_v4().to_string(),
                                kind: NotificationKind::Internal,
                                source: "updater".into(),
                                app_name: "Desktop Island".into(),
                                icon: None,
                                title: format!("Доступна версия {latest}"),
                                body: "Нажмите «Обновить», чтобы установить".into(),
                                priority: Priority::Normal,
                                timestamp: crate::database::now_ms(),
                                actions: vec![
                                    NotificationAction { id: "update".into(), label: "Обновить".into() },
                                    NotificationAction { id: "dismiss".into(), label: "Позже".into() },
                                ],
                                open_target: None,
                                duration_ms: 12_000,
                            });
                        }
                    }
                    Ok(Err(e)) => tracing::debug!(error = %e, "проверка обновлений"),
                    _ => {}
                }
                tokio::time::sleep(Duration::from_secs(6 * 3600)).await;
            }
        });
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
