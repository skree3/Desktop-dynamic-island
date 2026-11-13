//! Система плагинов.
//!
//! Модель изоляции:
//! * плагин — каталог с manifest.json и HTML/JS/CSS, копируется в каталог данных;
//! * код плагина выполняется в `<iframe sandbox="allow-scripts">` с непрозрачным
//!   origin: у него нет доступа к IPC Tauri, cookies и DOM панели;
//! * файлы отдаются через протокол `plugin://` со строгим CSP
//!   (`connect-src 'none'` — сетевые запросы запрещены);
//! * все возможности — только через брокер `plugin_call` в Rust, который
//!   проверяет, что плагин включён, разрешение выдано пользователем, и
//!   ограничивает частоту вызовов.

pub mod builtin;
pub mod manifest;

use rusqlite::params;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};
use std::time::Instant;
use tauri::{AppHandle, Emitter};

use crate::core::error::{AppError, AppResult};
use crate::core::state::Shared;
use crate::database::{now_ms, Database};
use crate::events::{names, Activity, IslandNotification, NotificationAction, NotificationKind, Priority};
use crate::logging::category;
pub use manifest::{Manifest, PERMISSIONS, PLUGIN_API_VERSION};

const SDK_JS: &str = include_str!("sdk.js");
const MAX_PLUGIN_SIZE: u64 = 20 * 1024 * 1024;
const MAX_FILES: usize = 500;
const CALLS_PER_SECOND: u32 = 30;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginInfo {
    pub manifest: Manifest,
    pub enabled: bool,
    pub granted: Vec<String>,
    pub path: String,
    pub installed_at: i64,
    pub updated_at: i64,
    pub missing: bool,
    /// Разрешения, запрошенные, но ещё не одобренные (после обновления).
    pub pending_permissions: Vec<String>,
    /// Встроенный плагин (поставляется с приложением, удалить нельзя — только выключить).
    pub builtin: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallResult {
    pub plugin: PluginInfo,
    pub updated: bool,
    pub previous_version: Option<String>,
}

pub struct PluginManager {
    dir: PathBuf,
    list: RwLock<Vec<PluginInfo>>,
    rate: Mutex<HashMap<String, (Instant, u32)>>,
}

fn copy_dir(src: &Path, dst: &Path, total: &mut u64, files: &mut usize) -> AppResult<()> {
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        let ft = e.file_type()?;
        if ft.is_symlink() {
            continue; // символические ссылки не копируем — защита от выхода за пределы каталога
        }
        let name = e.file_name();
        let name_s = name.to_string_lossy();
        if name_s.starts_with('.') || name_s == "node_modules" {
            continue;
        }
        let target = dst.join(&name);
        if ft.is_dir() {
            copy_dir(&e.path(), &target, total, files)?;
        } else if ft.is_file() {
            *files += 1;
            *total += e.metadata()?.len();
            if *files > MAX_FILES || *total > MAX_PLUGIN_SIZE {
                return Err(AppError::invalid("плагин слишком большой (максимум 20 МБ и 500 файлов)"));
            }
            std::fs::copy(e.path(), target)?;
        }
    }
    Ok(())
}

fn mime_for(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or("").to_lowercase().as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        _ => "application/octet-stream",
    }
}

/// CSP для документов плагина: только собственные ресурсы, без сети.
pub const PLUGIN_CSP: &str = "default-src 'none'; script-src plugin: http://plugin.localhost 'unsafe-inline'; style-src plugin: http://plugin.localhost 'unsafe-inline'; img-src plugin: http://plugin.localhost data: blob:; font-src plugin: http://plugin.localhost data:; connect-src 'none'; frame-src 'none'; form-action 'none'; base-uri 'none'";

impl PluginManager {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir, list: RwLock::new(vec![]), rate: Mutex::new(HashMap::new()) }
    }

    pub fn load(&self, db: &Database) {
        let conn = db.conn();
        let Ok(mut stmt) = conn.prepare("SELECT id, enabled, path, manifest, granted_permissions, installed_at, updated_at FROM plugins") else { return };
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)? != 0,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, i64>(5)?,
                r.get::<_, i64>(6)?,
            ))
        });
        let mut list = vec![];
        if let Ok(rows) = rows {
            for (id, enabled, path, manifest, granted, inst, upd) in rows.flatten() {
                let Ok(m) = serde_json::from_str::<Manifest>(&manifest) else {
                    tracing::warn!(%id, "повреждённый манифест плагина пропущен");
                    continue;
                };
                let granted: Vec<String> = serde_json::from_str(&granted).unwrap_or_default();
                let missing = !Path::new(&path).join("manifest.json").is_file();
                let pending: Vec<String> = m.permissions.iter().filter(|p| !granted.contains(p)).cloned().collect();
                list.push(PluginInfo {
                    enabled: enabled && !missing && pending.is_empty() && m.validate().is_ok(),
                    manifest: m,
                    granted,
                    path,
                    installed_at: inst,
                    updated_at: upd,
                    missing,
                    pending_permissions: pending,
                    builtin: builtin::is_builtin(&id),
                });
            }
        }
        *self.list.write().unwrap_or_else(|p| p.into_inner()) = list;
    }

    pub fn list(&self) -> Vec<PluginInfo> {
        self.list.read().unwrap_or_else(|p| p.into_inner()).clone()
    }

    pub fn get(&self, id: &str) -> Option<PluginInfo> {
        self.list.read().unwrap_or_else(|p| p.into_inner()).iter().find(|p| p.manifest.id == id).cloned()
    }

    /// (id, заголовок карточки) включённых плагинов с карточкой.
    pub fn enabled_cards(&self) -> Vec<(String, String)> {
        self.list()
            .into_iter()
            .filter(|p| p.enabled)
            .filter_map(|p| p.manifest.card.as_ref().map(|c| (p.manifest.id.clone(), c.title.clone())))
            .collect()
    }

    pub fn install(&self, st: &Shared, src: &Path) -> AppResult<InstallResult> {
        let manifest_text = std::fs::read_to_string(src.join("manifest.json"))
            .map_err(|_| AppError::invalid("в выбранной папке нет manifest.json"))?;
        let m = Manifest::parse(&manifest_text)?;
        if !src.join(&m.entry).is_file() {
            return Err(AppError::invalid(format!("файл {} не найден", m.entry)));
        }
        let existing = self.get(&m.id);
        if let Some(e) = &existing {
            let old = semver::Version::parse(&e.manifest.version).ok();
            let new = semver::Version::parse(&m.version).ok();
            if old.is_some() && new.is_some() && new < old {
                return Err(AppError::invalid(format!("установлена более новая версия {} (выбрана {})", e.manifest.version, m.version)));
            }
        }
        std::fs::create_dir_all(&self.dir)?;
        let target = self.dir.join(&m.id);
        let tmp = self.dir.join(format!(".{}.tmp-{}", m.id, now_ms()));
        let mut total = 0;
        let mut files = 0;
        if let Err(e) = copy_dir(src, &tmp, &mut total, &mut files) {
            let _ = std::fs::remove_dir_all(&tmp);
            return Err(e);
        }
        if target.exists() {
            std::fs::remove_dir_all(&target)?;
        }
        std::fs::rename(&tmp, &target)?;

        // При обновлении сохраняем выданные разрешения; новые требуют одобрения.
        let granted: Vec<String> = existing.as_ref().map(|e| e.granted.iter().filter(|g| m.permissions.contains(g)).cloned().collect()).unwrap_or_default();
        let pending: Vec<String> = m.permissions.iter().filter(|p| !granted.contains(p)).cloned().collect();
        let enabled = existing.as_ref().map(|e| e.enabled).unwrap_or(false) && pending.is_empty();
        let now = now_ms();
        st.db.conn().execute(
            "INSERT INTO plugins(id, name, version, enabled, path, manifest, granted_permissions, installed_at, updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?8)
             ON CONFLICT(id) DO UPDATE SET name=excluded.name, version=excluded.version, enabled=excluded.enabled, path=excluded.path,
             manifest=excluded.manifest, granted_permissions=excluded.granted_permissions, updated_at=excluded.updated_at",
            params![m.id, m.name, m.version, enabled as i64, target.to_string_lossy(), serde_json::to_string(&m)?, serde_json::to_string(&granted)?, now],
        )?;
        self.load(&st.db);
        let verb = if existing.is_some() { "обновлён" } else { "установлен" };
        st.log.info(category::PLUGIN, &format!("plugin:{}", m.id), format!("плагин {verb}: {} {}", m.name, m.version));
        st.events.emit(names::PLUGINS_CHANGED, ());
        Ok(InstallResult {
            plugin: self.get(&m.id).ok_or_else(|| AppError::system("плагин не загрузился"))?,
            updated: existing.is_some(),
            previous_version: existing.map(|e| e.manifest.version),
        })
    }

    pub fn set_enabled(&self, st: &Shared, id: &str, enabled: bool, approve_permissions: bool) -> AppResult<PluginInfo> {
        let p = self.get(id).ok_or_else(|| AppError::NotFound("плагин не найден".into()))?;
        if p.missing {
            return Err(AppError::NotFound("файлы плагина отсутствуют — переустановите его".into()));
        }
        let mut granted = p.granted.clone();
        if enabled {
            if !p.pending_permissions.is_empty() && !approve_permissions {
                return Err(AppError::PermissionDenied("необходимо подтвердить разрешения плагина".into()));
            }
            granted = p.manifest.permissions.clone();
        }
        st.db.conn().execute(
            "UPDATE plugins SET enabled = ?1, granted_permissions = ?2 WHERE id = ?3",
            params![enabled as i64, serde_json::to_string(&granted)?, id],
        )?;
        self.load(&st.db);
        if !enabled {
            st.events.clear_activity(&format!("plugin:{id}"));
        }
        st.log.info(category::PLUGIN, &format!("plugin:{id}"), if enabled { "плагин включён" } else { "плагин отключён" });
        st.events.emit(names::PLUGINS_CHANGED, ());
        self.get(id).ok_or_else(|| AppError::NotFound("плагин не найден".into()))
    }

    pub fn uninstall(&self, st: &Shared, id: &str) -> AppResult<()> {
        let p = self.get(id).ok_or_else(|| AppError::NotFound("плагин не найден".into()))?;
        if p.builtin {
            return Err(AppError::invalid("встроенный плагин нельзя удалить — его можно выключить"));
        }
        let path = PathBuf::from(&p.path);
        if path.starts_with(&self.dir) && path.exists() {
            std::fs::remove_dir_all(&path)?;
        }
        st.db.conn().execute("DELETE FROM plugin_settings WHERE plugin_id = ?1", params![id])?;
        st.db.conn().execute("DELETE FROM plugins WHERE id = ?1", params![id])?;
        st.events.clear_activity(&format!("plugin:{id}"));
        self.load(&st.db);
        st.log.info(category::PLUGIN, &format!("plugin:{id}"), "плагин удалён");
        st.events.emit(names::PLUGINS_CHANGED, ());
        Ok(())
    }

    // ---- настройки и хранилище ----

    pub fn get_settings(&self, db: &Database, id: &str) -> AppResult<serde_json::Map<String, serde_json::Value>> {
        let p = self.get(id).ok_or_else(|| AppError::NotFound("плагин не найден".into()))?;
        let mut out = serde_json::Map::new();
        for s in &p.manifest.settings {
            out.insert(s.key.clone(), s.default.clone());
        }
        let conn = db.conn();
        let mut stmt = conn.prepare("SELECT key, value FROM plugin_settings WHERE plugin_id = ?1 AND key LIKE 'setting:%'")?;
        for (k, v) in stmt.query_map(params![id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?.flatten() {
            if let Ok(val) = serde_json::from_str(&v) {
                out.insert(k.trim_start_matches("setting:").to_string(), val);
            }
        }
        Ok(out)
    }

    pub fn set_setting(&self, app: &AppHandle, db: &Database, id: &str, key: &str, value: serde_json::Value) -> AppResult<()> {
        let p = self.get(id).ok_or_else(|| AppError::NotFound("плагин не найден".into()))?;
        p.manifest.validate_setting(key, &value)?;
        db.conn().execute(
            "INSERT INTO plugin_settings(plugin_id, key, value) VALUES(?1,?2,?3) ON CONFLICT(plugin_id, key) DO UPDATE SET value = excluded.value",
            params![id, format!("setting:{key}"), value.to_string()],
        )?;
        let _ = app.emit("plugins://settings", serde_json::json!({ "pluginId": id }));
        Ok(())
    }

    fn storage_get(&self, db: &Database, id: &str, key: &str) -> AppResult<serde_json::Value> {
        let v: Option<String> = db
            .conn()
            .query_row("SELECT value FROM plugin_settings WHERE plugin_id = ?1 AND key = ?2", params![id, format!("storage:{key}")], |r| r.get(0))
            .ok();
        Ok(v.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(serde_json::Value::Null))
    }

    fn storage_set(&self, db: &Database, id: &str, key: &str, value: &serde_json::Value) -> AppResult<()> {
        let text = value.to_string();
        if text.len() > 64 * 1024 {
            return Err(AppError::invalid("значение больше 64 КБ"));
        }
        let count: i64 = db.conn().query_row("SELECT count(*) FROM plugin_settings WHERE plugin_id = ?1 AND key LIKE 'storage:%'", params![id], |r| r.get(0))?;
        if count >= 200 {
            return Err(AppError::invalid("превышен лимит ключей хранилища (200)"));
        }
        if value.is_null() {
            db.conn().execute("DELETE FROM plugin_settings WHERE plugin_id = ?1 AND key = ?2", params![id, format!("storage:{key}")])?;
        } else {
            db.conn().execute(
                "INSERT INTO plugin_settings(plugin_id, key, value) VALUES(?1,?2,?3) ON CONFLICT(plugin_id, key) DO UPDATE SET value = excluded.value",
                params![id, format!("storage:{key}"), text],
            )?;
        }
        Ok(())
    }

    // ---- протокол plugin:// ----

    /// Отдаёт файл плагина: путь `/<id>/<file>`.
    pub fn serve(&self, uri_path: &str) -> Option<(Vec<u8>, &'static str)> {
        let path = uri_path.trim_start_matches('/');
        let (id, file) = path.split_once('/')?;
        let file = file.split(['?', '#']).next().unwrap_or("");
        let p = self.get(id)?;
        if !p.enabled {
            return None;
        }
        if file == "island-sdk.js" {
            let sdk = SDK_JS.replace("__PLUGIN_ID__", id);
            return Some((sdk.into_bytes(), "text/javascript; charset=utf-8"));
        }
        if !manifest::safe_relative(file) {
            return None;
        }
        let base = PathBuf::from(&p.path);
        let full = base.join(file);
        let canon_base = base.canonicalize().ok()?;
        let canon = full.canonicalize().ok()?;
        if !canon.starts_with(&canon_base) {
            return None;
        }
        let meta = std::fs::metadata(&canon).ok()?;
        if !meta.is_file() || meta.len() > 10 * 1024 * 1024 {
            return None;
        }
        Some((std::fs::read(&canon).ok()?, mime_for(file)))
    }

    fn check_rate(&self, id: &str) -> AppResult<()> {
        let mut g = self.rate.lock().unwrap_or_else(|p| p.into_inner());
        let e = g.entry(id.to_string()).or_insert((Instant::now(), 0));
        if e.0.elapsed().as_secs() >= 1 {
            *e = (Instant::now(), 0);
        }
        e.1 += 1;
        if e.1 > CALLS_PER_SECOND {
            return Err(AppError::PermissionDenied("слишком частые вызовы API".into()));
        }
        Ok(())
    }

    pub fn forward_action(&self, app: &AppHandle, plugin_id: &str, notification_id: &str, action: &str) {
        let _ = app.emit(
            "plugins://event",
            serde_json::json!({ "pluginId": plugin_id, "event": "notificationAction", "data": { "notificationId": notification_id, "action": action } }),
        );
    }

    /// Брокер Plugin API: проверка разрешений и выполнение вызова.
    pub async fn call(&self, app: &AppHandle, st: &Shared, id: &str, method: &str, params: serde_json::Value) -> AppResult<serde_json::Value> {
        if !st.with_settings(|s| s.modules.plugins && s.security.plugins_enabled) {
            return Err(AppError::PermissionDenied("плагины отключены в настройках".into()));
        }
        let p = self.get(id).ok_or_else(|| AppError::NotFound("плагин не найден".into()))?;
        if !p.enabled {
            return Err(AppError::PermissionDenied("плагин отключён".into()));
        }
        self.check_rate(id)?;
        let need = |perm: &str| -> AppResult<()> {
            if p.granted.iter().any(|g| g == perm) {
                Ok(())
            } else {
                st.log.warn(category::PERMISSION, &format!("plugin:{id}"), format!("вызов {method} без разрешения {perm}"));
                Err(AppError::PermissionDenied(format!("плагину не выдано разрешение «{perm}»")))
            }
        };
        let str_param = |k: &str, max: usize| params.get(k).and_then(|v| v.as_str()).map(|s| crate::util::clean_text(s, max));
        match method {
            "app.info" => Ok(serde_json::json!({
                "apiVersion": PLUGIN_API_VERSION,
                "appVersion": env!("CARGO_PKG_VERSION"),
                "os": std::env::consts::OS,
                "pluginId": id,
                "permissions": p.granted,
            })),
            "log" => {
                let level = str_param("level", 10).unwrap_or_else(|| "info".into());
                let msg = str_param("message", 500).unwrap_or_default();
                let src = format!("plugin:{id}");
                match level.as_str() {
                    "error" => st.log.error(category::PLUGIN, &src, msg),
                    "warn" => st.log.warn(category::PLUGIN, &src, msg),
                    _ => st.log.info(category::PLUGIN, &src, msg),
                }
                Ok(serde_json::Value::Null)
            }
            "settings.get" => Ok(serde_json::Value::Object(self.get_settings(&st.db, id)?)),
            "notify" => {
                need("notifications")?;
                let title = str_param("title", 120).unwrap_or_default();
                if title.is_empty() {
                    return Err(AppError::invalid("title обязателен"));
                }
                let priority = match params.get("priority").and_then(|v| v.as_str()) {
                    Some("low") => Priority::Low,
                    Some("high") => Priority::High,
                    _ => Priority::Normal, // critical плагинам недоступен
                };
                let actions: Vec<NotificationAction> = params
                    .get("actions")
                    .and_then(|a| a.as_array())
                    .map(|a| {
                        a.iter()
                            .take(3)
                            .filter_map(|x| {
                                Some(NotificationAction {
                                    id: crate::util::clean_text(x.get("id")?.as_str()?, 32),
                                    label: crate::util::clean_text(x.get("label")?.as_str()?, 24),
                                })
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let n = IslandNotification { stack: Vec::new(), count: 1,
                    id: uuid::Uuid::new_v4().to_string(),
                    kind: NotificationKind::Plugin,
                    source: format!("plugin:{id}"),
                    app_name: p.manifest.name.clone(),
                    icon: None,
                    title,
                    body: str_param("body", 500).unwrap_or_default(),
                    priority,
                    timestamp: now_ms(),
                    actions,
                    open_target: None,
                    duration_ms: 0,
                };
                let nid = n.id.clone();
                st.events.push(n);
                Ok(serde_json::json!({ "id": nid }))
            }
            "storage.get" => {
                need("storage")?;
                let key = str_param("key", 64).ok_or_else(|| AppError::invalid("key обязателен"))?;
                self.storage_get(&st.db, id, &key)
            }
            "storage.set" => {
                need("storage")?;
                let key = str_param("key", 64).ok_or_else(|| AppError::invalid("key обязателен"))?;
                self.storage_set(&st.db, id, &key, params.get("value").unwrap_or(&serde_json::Value::Null))?;
                Ok(serde_json::Value::Null)
            }
            "system.snapshot" => {
                need("system.read")?;
                Ok(serde_json::to_value(st.sysmon.snapshot_now().await)?)
            }
            "media.sessions" => {
                need("media.read")?;
                let list = st.music.sessions().await?;
                // Плагины не получают обложки и внутренние идентификаторы приложений.
                Ok(serde_json::to_value(
                    list.into_iter()
                        .map(|s| serde_json::json!({ "id": s.id, "app": s.app_name, "title": s.title, "artist": s.artist, "album": s.album, "status": s.status, "durationMs": s.duration_ms, "positionMs": s.position_ms }))
                        .collect::<Vec<_>>(),
                )?)
            }
            "media.control" => {
                need("media.control")?;
                let action = str_param("action", 16).unwrap_or_default();
                let a = match action.as_str() {
                    "playPause" => crate::platform::types::MediaAction::PlayPause,
                    "next" => crate::platform::types::MediaAction::Next,
                    "previous" => crate::platform::types::MediaAction::Previous,
                    _ => return Err(AppError::invalid("action: playPause | next | previous")),
                };
                st.music.control_current(a).await?;
                Ok(serde_json::Value::Null)
            }
            "activity.set" => {
                need("activity")?;
                let title = str_param("title", 60).unwrap_or_default();
                if title.is_empty() {
                    return Err(AppError::invalid("title обязателен"));
                }
                st.events.set_activity(Activity {
                    id: format!("plugin:{id}"),
                    kind: "plugin".into(),
                    priority: Priority::Low,
                    title,
                    subtitle: str_param("subtitle", 80),
                    icon: None,
                    progress: params.get("progress").and_then(|v| v.as_f64()).map(|v| v.clamp(0.0, 1.0)),
                    updated_at: now_ms(),
                    started_at: None,
                });
                Ok(serde_json::Value::Null)
            }
            "activity.clear" => {
                need("activity")?;
                st.events.clear_activity(&format!("plugin:{id}"));
                Ok(serde_json::Value::Null)
            }
            "web.open" => {
                need("web.open")?;
                let url = crate::util::validate_web_url(&str_param("url", 2048).unwrap_or_default())?;
                let host = crate::util::host_of(&url).unwrap_or_default();
                let allowed = p.manifest.allowed_hosts.iter().any(|h| host == *h || host.ends_with(&format!(".{h}")));
                if !allowed {
                    return Err(AppError::PermissionDenied(format!("хост {host} не указан в allowedHosts")));
                }
                tauri::async_runtime::spawn_blocking(move || crate::platform::os::shell::open_uri(&url)).await??;
                Ok(serde_json::Value::Null)
            }
            _ => {
                let _ = app;
                Err(AppError::unsupported(format!("неизвестный метод Plugin API «{method}»")))
            }
        }
    }
}
