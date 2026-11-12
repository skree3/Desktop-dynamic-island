//! Универсальная система интеграций: каталог известных приложений, ручное
//! добавление программ и веб-сервисов, адаптеры (медиасессии, уведомления ОС,
//! протоколы, запуск, веб-окна, плагины) и статус прокси-приложений.

pub mod ai;
pub mod catalog;

use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::core::error::{AppError, AppResult};
use crate::core::state::Shared;
use crate::database::{now_ms, Database};
use crate::platform::types::ProxyInfo;
use catalog::{platform_support, SupportLevel, CATALOG};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomApp {
    pub id: String,
    pub name: String,
    /// custom | web
    pub kind: String,
    pub target: String,
    #[serde(default)]
    pub args: String,
    pub icon: Option<String>,
    #[serde(default = "default_adapter")]
    pub adapter: String,
    pub catalog_id: Option<String>,
    #[serde(default)]
    pub features: serde_json::Value,
    #[serde(default = "yes")]
    pub notifications_enabled: bool,
    #[serde(default = "yes")]
    pub show_card: bool,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub created_at: i64,
}

fn default_adapter() -> String {
    "launch".into()
}
fn yes() -> bool {
    true
}

const COLS: &str = "id, name, kind, target, args, icon, adapter, catalog_id, features, notifications_enabled, show_card, favorite, created_at";

fn row(r: &rusqlite::Row) -> rusqlite::Result<CustomApp> {
    Ok(CustomApp {
        id: r.get(0)?,
        name: r.get(1)?,
        kind: r.get(2)?,
        target: r.get(3)?,
        args: r.get(4)?,
        icon: r.get(5)?,
        adapter: r.get(6)?,
        catalog_id: r.get(7)?,
        features: serde_json::from_str(&r.get::<_, String>(8)?).unwrap_or(serde_json::json!({})),
        notifications_enabled: r.get::<_, i64>(9)? != 0,
        show_card: r.get::<_, i64>(10)? != 0,
        favorite: r.get::<_, i64>(11)? != 0,
        created_at: r.get(12)?,
    })
}

pub fn list_custom(db: &Database) -> AppResult<Vec<CustomApp>> {
    let conn = db.conn();
    let mut stmt = conn.prepare(&format!("SELECT {COLS} FROM application_integrations ORDER BY created_at"))?;
    let rows = stmt.query_map([], row)?.flatten().collect();
    Ok(rows)
}

pub fn get_custom(db: &Database, id: &str) -> AppResult<Option<CustomApp>> {
    Ok(db.conn().query_row(&format!("SELECT {COLS} FROM application_integrations WHERE id = ?1"), params![id], row).optional()?)
}

/// Простое разбиение строки аргументов с учётом кавычек.
pub fn split_args(s: &str) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    let mut q = false;
    for c in s.chars() {
        match c {
            '"' => q = !q,
            c if c.is_whitespace() && !q => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

fn load_icon(path_or_data: &str) -> AppResult<Option<String>> {
    if path_or_data.is_empty() {
        return Ok(None);
    }
    if path_or_data.starts_with("data:image/") {
        if path_or_data.len() > 700_000 {
            return Err(AppError::invalid("иконка слишком большая (максимум 512 КБ)"));
        }
        return Ok(Some(path_or_data.to_string()));
    }
    let p = std::path::Path::new(path_or_data);
    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    let mime = match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "webp" => "image/webp",
        _ => return Err(AppError::invalid("поддерживаются иконки PNG, JPG, SVG, ICO, WEBP")),
    };
    let meta = std::fs::metadata(p).map_err(|_| AppError::NotFound("файл иконки не найден".into()))?;
    if meta.len() > 512 * 1024 {
        return Err(AppError::invalid("иконка слишком большая (максимум 512 КБ)"));
    }
    Ok(Some(crate::util::image::bytes_data_url(mime, &std::fs::read(p)?)))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomAppInput {
    pub id: Option<String>,
    pub name: String,
    pub kind: String,
    pub target: String,
    #[serde(default)]
    pub args: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub adapter: Option<String>,
    #[serde(default)]
    pub catalog_id: Option<String>,
    #[serde(default)]
    pub notifications_enabled: Option<bool>,
    #[serde(default)]
    pub show_card: Option<bool>,
    #[serde(default)]
    pub favorite: Option<bool>,
    #[serde(default)]
    pub features: Option<serde_json::Value>,
}

pub fn save_custom(st: &Shared, input: CustomAppInput) -> AppResult<CustomApp> {
    let name = crate::util::clean_text(&input.name, 80);
    if name.is_empty() {
        return Err(AppError::invalid("укажите название"));
    }
    let target = match input.kind.as_str() {
        "web" => crate::util::validate_web_url(&input.target)?,
        "custom" => {
            let t = input.target.trim().to_string();
            let is_aumid = t.starts_with("shell:AppsFolder\\") || (cfg!(windows) && t.contains('!') && !t.contains(['/', '\\']));
            if !is_aumid {
                let p = std::path::Path::new(&t);
                if !p.is_file() {
                    return Err(AppError::NotFound("исполняемый файл не найден".into()));
                }
                #[cfg(windows)]
                {
                    let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                    if !matches!(ext.as_str(), "exe" | "lnk" | "bat" | "cmd" | "url" | "appref-ms") {
                        return Err(AppError::invalid("выберите исполняемый файл (.exe, .lnk)"));
                    }
                }
                #[cfg(target_os = "linux")]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let is_desktop = t.ends_with(".desktop");
                    let exec = std::fs::metadata(p).map(|m| m.permissions().mode() & 0o111 != 0).unwrap_or(false);
                    if !is_desktop && !exec {
                        return Err(AppError::invalid("файл не является исполняемым"));
                    }
                }
            }
            t
        }
        _ => return Err(AppError::invalid("тип должен быть custom или web")),
    };
    let args = crate::util::clean_text(&input.args, 512);
    let icon = load_icon(input.icon.trim())?;
    let catalog_id = input.catalog_id.filter(|c| catalog::get(c).is_some()).or_else(|| catalog::match_app(&name, Some(&target)).map(|k| k.id.to_string()));
    let adapters = capabilities(st, &input.kind, &target, catalog_id.as_deref());
    let adapter = input
        .adapter
        .filter(|a| adapters.iter().any(|c| &c.id == a && c.available))
        .unwrap_or_else(|| if input.kind == "web" { "web_window".into() } else { "launch".into() });
    let id = match input.id {
        Some(id) if get_custom(&st.db, &id)?.is_some() => id,
        Some(_) => return Err(AppError::NotFound("запись не найдена".into())),
        None => format!("custom:{}", uuid::Uuid::new_v4()),
    };
    let app = CustomApp {
        id: id.clone(),
        name,
        kind: input.kind,
        target,
        args,
        icon,
        adapter,
        catalog_id,
        features: input.features.unwrap_or(serde_json::json!({})),
        notifications_enabled: input.notifications_enabled.unwrap_or(true),
        show_card: input.show_card.unwrap_or(true),
        favorite: input.favorite.unwrap_or(false),
        created_at: now_ms(),
    };
    st.db.conn().execute(
        "INSERT INTO application_integrations(id, name, kind, target, args, icon, adapter, catalog_id, features, notifications_enabled, show_card, favorite, created_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)
         ON CONFLICT(id) DO UPDATE SET name=excluded.name, kind=excluded.kind, target=excluded.target, args=excluded.args, icon=excluded.icon,
         adapter=excluded.adapter, catalog_id=excluded.catalog_id, features=excluded.features, notifications_enabled=excluded.notifications_enabled,
         show_card=excluded.show_card, favorite=excluded.favorite",
        params![
            app.id, app.name, app.kind, app.target, app.args, app.icon, app.adapter, app.catalog_id, app.features.to_string(),
            app.notifications_enabled as i64, app.show_card as i64, app.favorite as i64, app.created_at
        ],
    )?;
    refresh_muted(st);
    st.apps.clear_icon_cache();
    Ok(app)
}

pub fn delete_custom(st: &Shared, id: &str) -> AppResult<()> {
    st.db.conn().execute("DELETE FROM application_integrations WHERE id = ?1", params![id])?;
    refresh_muted(st);
    Ok(())
}

/// Синхронизирует список приложений с отключёнными уведомлениями с Event Manager.
pub fn refresh_muted(st: &Shared) {
    let muted: Vec<String> = list_custom(&st.db)
        .unwrap_or_default()
        .into_iter()
        .filter(|a| !a.notifications_enabled)
        .flat_map(|a| {
            let mut names = vec![a.name.clone()];
            if let Some(k) = a.catalog_id.as_deref().and_then(catalog::get) {
                names.push(k.name.to_string());
            }
            names
        })
        .collect();
    st.events.set_extra_muted(muted);
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Capability {
    pub id: String,
    pub label: String,
    pub available: bool,
    pub reason: String,
}

/// Реально доступные способы интеграции для приложения на текущей платформе.
pub fn capabilities(st: &Shared, kind: &str, target: &str, catalog_id: Option<&str>) -> Vec<Capability> {
    let k = catalog_id.and_then(catalog::get).or_else(|| catalog::match_app("", Some(target)));
    let listener = st.notifications.status();
    let cap = |id: &str, label: &str, available: bool, reason: &str| Capability { id: id.into(), label: label.into(), available, reason: reason.into() };
    let mut out = vec![];
    if kind == "web" {
        out.push(cap("web_window", "Мини-окно (изолированная сессия)", true, "Отдельные cookies и данные для каждого сервиса"));
        out.push(cap("launch", "Открыть в браузере", true, "Системный браузер по умолчанию"));
        return out;
    }
    out.push(cap("launch", "Запуск приложения", true, "Запуск через систему"));
    let media = k.map(|k| k.media_session).unwrap_or(false);
    let backend = if cfg!(windows) { "SMTC" } else { "MPRIS" };
    out.push(cap(
        "media_session",
        "Системная медиасессия",
        media,
        if media { "Приложение публикует медиасессию — управление воспроизведением в карточке «Музыка»" } else { "Приложение не известно как источник медиасессии (если оно её публикует, она появится автоматически)" },
    ));
    let _ = backend;
    out.push(cap(
        "notifications",
        "Уведомления ОС",
        listener.available,
        if listener.available { "Уведомления приложения будут показаны в панели" } else { &listener.message },
    ));
    if let Some(p) = k.and_then(|k| k.protocol) {
        out.push(cap("protocol", "Протокол", true, &format!("Ссылки {p}")));
    }
    out.push(cap("plugin", "Плагин", false, "Установите плагин, расширяющий интеграцию с этим приложением"));
    out
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IntegrationStatus {
    pub id: String,
    pub name: String,
    pub category: String,
    pub support: SupportLevel,
    pub support_label: String,
    pub notes: String,
    pub installed: bool,
    pub app_id: Option<String>,
    pub running: bool,
    pub web_url: Option<String>,
    pub protocol: Option<String>,
    pub proxy: Option<ProxyInfo>,
}

fn running_processes() -> Vec<String> {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate};
    let mut sys = sysinfo::System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    sys.processes()
        .values()
        .map(|p| {
            let n = p.name().to_string_lossy().to_lowercase();
            n.strip_suffix(".exe").map(|s| s.to_string()).unwrap_or(n)
        })
        .collect()
}

pub async fn status(st: &Shared) -> AppResult<Vec<IntegrationStatus>> {
    let apps = st.apps.all_entries(st).await.unwrap_or_default();
    let (procs, proxy) = tauri::async_runtime::spawn_blocking(|| (running_processes(), crate::platform::os::proxy::system_proxy())).await?;
    let listener_ok = st.notifications.status().available;
    let mut out = vec![];
    for k in CATALOG {
        let (mut support, notes) = platform_support(k);
        if k.notification_capable && listener_ok && support == SupportLevel::LaunchOnly {
            support = SupportLevel::NotificationsOnly;
        }
        // Среди совпадений выбираем самое «точное» (короткое название), например
        // «Steam», а не «Steam Support Center».
        let app = apps.iter().filter(|a| a.catalog_id.as_deref() == Some(k.id)).min_by_key(|a| (a.source != "system", a.name.len()));
        let running = k.processes.iter().any(|p| procs.iter().any(|r| r == p));
        out.push(IntegrationStatus {
            id: k.id.into(),
            name: k.name.into(),
            category: k.category.into(),
            support,
            support_label: support.label().into(),
            notes: notes.into(),
            installed: app.is_some(),
            app_id: app.map(|a| a.id.clone()),
            running,
            web_url: k.web_url.map(|s| s.to_string()),
            protocol: k.protocol.map(|s| s.to_string()),
            proxy: if k.category == "proxy" { Some(proxy.clone()) } else { None },
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn args() {
        assert_eq!(super::split_args(r#"--a "b c" d"#), vec!["--a", "b c", "d"]);
    }
}
