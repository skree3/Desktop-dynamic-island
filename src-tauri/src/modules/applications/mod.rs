//! Поиск и запуск установленных приложений. Список строится автоматически
//! (Windows: shell:AppsFolder; Linux: desktop entries) и не ограничен заранее
//! заданными названиями. Пользовательские приложения и веб-сервисы берутся
//! из таблицы application_integrations. Иконки загружаются лениво и кэшируются.

use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::core::error::{AppError, AppResult};
use crate::core::state::Shared;
use crate::integrations::{self, CustomApp};
use crate::platform::os;
use crate::platform::types::{InstalledApp, LaunchTarget};

const INDEX_TTL: Duration = Duration::from_secs(600);
const ICON_CACHE: usize = 300;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppEntry {
    pub id: String,
    pub name: String,
    /// system | custom | web | plugin
    pub source: String,
    pub target: LaunchTarget,
    pub exe_path: Option<String>,
    pub favorite: bool,
    /// Совпадение с каталогом известных интеграций.
    pub catalog_id: Option<String>,
    pub icon: Option<String>,
}

#[derive(Default)]
pub struct AppsModule {
    index: Mutex<Option<(Instant, Arc<Vec<InstalledApp>>)>>,
    icons: Mutex<(HashMap<String, Option<String>>, VecDeque<String>)>,
    scanning: tokio::sync::Mutex<()>,
    /// Поколение анимации запуска: старый таймер не гасит новую анимацию.
    launch_gen: std::sync::atomic::AtomicU64,
}

fn score(name: &str, keywords: &[String], q: &str) -> Option<i32> {
    if q.is_empty() {
        return Some(0);
    }
    let n = name.to_lowercase();
    if n == q {
        return Some(1000);
    }
    if n.starts_with(q) {
        return Some(800 - n.len() as i32);
    }
    if n.split(|c: char| !c.is_alphanumeric()).any(|w| w.starts_with(q)) {
        return Some(600 - n.len() as i32);
    }
    if n.contains(q) {
        return Some(400 - n.len() as i32);
    }
    if keywords.iter().any(|k| k.starts_with(q)) {
        return Some(300);
    }
    if keywords.iter().any(|k| k.contains(q)) {
        return Some(200);
    }
    // Нечёткий поиск по аббревиатуре: «vsc» → «Visual Studio Code».
    let initials: String = n.split(|c: char| !c.is_alphanumeric()).filter_map(|w| w.chars().next()).collect();
    if initials.starts_with(q) && q.len() >= 2 {
        return Some(250);
    }
    None
}

impl AppsModule {
    pub async fn index(&self, force: bool) -> AppResult<Arc<Vec<InstalledApp>>> {
        if !force {
            if let Some((t, list)) = self.index.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
                if t.elapsed() < INDEX_TTL {
                    return Ok(list.clone());
                }
            }
        }
        let _guard = self.scanning.lock().await;
        if !force {
            if let Some((t, list)) = self.index.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
                if t.elapsed() < INDEX_TTL {
                    return Ok(list.clone());
                }
            }
        }
        let list = Arc::new(tauri::async_runtime::spawn_blocking(os::apps::scan).await??);
        *self.index.lock().unwrap_or_else(|p| p.into_inner()) = Some((Instant::now(), list.clone()));
        Ok(list)
    }

    pub async fn all_entries(&self, st: &Shared) -> AppResult<Vec<AppEntry>> {
        let index = self.index(false).await?;
        let favorites = st.with_settings(|s| s.apps.favorites.clone());
        let hidden = st.with_settings(|s| s.apps.hidden.clone());
        let mut out: Vec<AppEntry> = vec![];
        for c in integrations::list_custom(&st.db).unwrap_or_default() {
            out.push(custom_entry(&c, favorites.contains(&c.id)));
        }
        for a in index.iter() {
            if hidden.contains(&a.id) {
                continue;
            }
            out.push(AppEntry {
                id: a.id.clone(),
                name: a.name.clone(),
                source: "system".into(),
                target: a.target.clone(),
                exe_path: a.exe_path.clone(),
                favorite: favorites.contains(&a.id),
                catalog_id: integrations::catalog::match_app(&a.name, a.exe_path.as_deref()).map(|k| k.id.to_string()),
                icon: None,
            });
        }
        Ok(out)
    }

    pub async fn search(&self, st: &Shared, query: &str, limit: usize) -> AppResult<Vec<AppEntry>> {
        let q = query.trim().to_lowercase();
        let entries = self.all_entries(st).await?;
        let index = self.index(false).await?;
        let kw: HashMap<&str, &Vec<String>> = index.iter().map(|a| (a.id.as_str(), &a.keywords)).collect();
        let empty = vec![];
        let mut scored: Vec<(i32, AppEntry)> = entries
            .into_iter()
            .filter_map(|e| {
                let k = kw.get(e.id.as_str()).copied().unwrap_or(&empty);
                score(&e.name, k, &q).map(|s| (s + if e.favorite { 150 } else { 0 } + if e.source != "system" { 20 } else { 0 }, e))
            })
            .collect();
        if q.is_empty() {
            scored.sort_by(|a, b| b.1.favorite.cmp(&a.1.favorite).then(a.1.name.to_lowercase().cmp(&b.1.name.to_lowercase())));
        } else {
            scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.name.cmp(&b.1.name)));
        }
        Ok(scored.into_iter().take(limit.clamp(1, 500)).map(|(_, e)| e).collect())
    }

    pub async fn find(&self, st: &Shared, id: &str) -> AppResult<AppEntry> {
        if let Some(c) = integrations::get_custom(&st.db, id)? {
            return Ok(custom_entry(&c, false));
        }
        let index = self.index(false).await?;
        index
            .iter()
            .find(|a| a.id == id)
            .map(|a| AppEntry {
                id: a.id.clone(),
                name: a.name.clone(),
                source: "system".into(),
                target: a.target.clone(),
                exe_path: a.exe_path.clone(),
                favorite: false,
                catalog_id: None,
                icon: a.icon_hint.clone(),
            })
            .ok_or_else(|| AppError::NotFound("приложение не найдено".into()))
    }

    pub async fn icon(&self, st: &Shared, id: &str) -> AppResult<Option<String>> {
        if let Some(v) = self.icons.lock().unwrap_or_else(|p| p.into_inner()).0.get(id) {
            return Ok(v.clone());
        }
        let entry = self.find(st, id).await?;
        let result = if entry.source == "custom" || entry.source == "web" {
            match integrations::get_custom(&st.db, id)?.and_then(|c| c.icon) {
                Some(icon) if icon.starts_with("data:image/") => Some(icon),
                _ => {
                    let target = entry.target.clone();
                    tauri::async_runtime::spawn_blocking(move || os::apps::icon(&target, None, 48)).await?.unwrap_or(None)
                }
            }
        } else {
            let target = entry.target.clone();
            let hint = entry.icon.clone();
            tauri::async_runtime::spawn_blocking(move || os::apps::icon(&target, hint.as_deref(), 48)).await?.unwrap_or(None)
        };
        let mut g = self.icons.lock().unwrap_or_else(|p| p.into_inner());
        g.0.insert(id.to_string(), result.clone());
        g.1.push_back(id.to_string());
        while g.1.len() > ICON_CACHE {
            if let Some(old) = g.1.pop_front() {
                g.0.remove(&old);
            }
        }
        Ok(result)
    }

    pub async fn launch(&self, st: &Shared, id: &str) -> AppResult<()> {
        let entry = self.find(st, id).await?;
        let target = entry.target.clone();
        let res = tauri::async_runtime::spawn_blocking(move || os::apps::launch(&target)).await?;
        match &res {
            Ok(_) => {
                tracing::info!(app = %entry.name, "приложение запущено");
                self.launch_animation(st, id, &entry.name, false).await;
            }
            Err(e) => st.log.warn(crate::logging::category::INTEGRATION, "apps", format!("не удалось запустить «{}»: {e}", entry.name)),
        }
        res
    }

    /// Запуск от имени администратора (Windows — запрос UAC, Linux — pkexec).
    pub async fn launch_admin(&self, st: &Shared, id: &str) -> AppResult<()> {
        let entry = self.find(st, id).await?;
        let target = entry.target.clone();
        let exe = entry.exe_path.clone();
        let res = tauri::async_runtime::spawn_blocking(move || os::apps::launch_admin(&target, exe.as_deref())).await?;
        match &res {
            Ok(_) => {
                st.log.info(crate::logging::category::INTEGRATION, "apps", format!("«{}» запущено от имени администратора", entry.name));
                self.launch_animation(st, id, &entry.name, true).await;
            }
            Err(e) => st.log.warn(crate::logging::category::INTEGRATION, "apps", format!("не удалось запустить «{}» от имени администратора: {e}", entry.name)),
        }
        res
    }

    /// Анимация запуска в острове: иконка приложения «вылетает» из свёрнутой панели.
    async fn launch_animation(&self, st: &Shared, id: &str, name: &str, admin: bool) {
        use crate::events::{Activity, Priority};
        use std::sync::atomic::Ordering;
        let gen = self.launch_gen.fetch_add(1, Ordering::SeqCst) + 1;
        let icon = self.icon(st, id).await.ok().flatten();
        st.events.set_activity(Activity {
            id: "launch".into(),
            kind: if admin { "launch-admin".into() } else { "launch".into() },
            priority: Priority::Critical,
            title: name.to_string(),
            subtitle: Some(if admin { "От имени администратора".into() } else { "Открывается…".into() }),
            icon,
            progress: None,
            updated_at: crate::database::now_ms(),
            started_at: None,
        });
        let st = st.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(if admin { 3200 } else { 1800 })).await;
            if st.apps.launch_gen.load(Ordering::SeqCst) == gen {
                st.events.clear_activity("launch");
            }
        });
    }

    pub fn clear_icon_cache(&self) {
        let mut g = self.icons.lock().unwrap_or_else(|p| p.into_inner());
        g.0.clear();
        g.1.clear();
    }
}

fn custom_entry(c: &CustomApp, favorite: bool) -> AppEntry {
    let target = if c.kind == "web" {
        LaunchTarget::Uri { uri: c.target.clone() }
    } else if c.target.starts_with("shell:") || c.target.contains('!') {
        LaunchTarget::AppsFolder { id: c.target.trim_start_matches("shell:AppsFolder\\").to_string() }
    } else if c.target.ends_with(".desktop") {
        LaunchTarget::Desktop { path: c.target.clone() }
    } else {
        LaunchTarget::Exe { path: c.target.clone(), args: crate::integrations::split_args(&c.args) }
    };
    AppEntry {
        id: c.id.clone(),
        name: c.name.clone(),
        source: c.kind.clone(),
        exe_path: if c.kind == "custom" { Some(c.target.clone()) } else { None },
        target,
        favorite: favorite || c.favorite,
        catalog_id: c.catalog_id.clone(),
        icon: None,
    }
}

/// Открывает приложение по подсказке из уведомления (AUMID, desktop-id или имя).
pub fn open_by_hint(st: &Shared, hint: &str, name: &str) -> AppResult<()> {
    let st = st.clone();
    let hint = hint.to_string();
    let name = name.to_lowercase();
    tauri::async_runtime::spawn(async move {
        let entries = match st.apps.all_entries(&st).await {
            Ok(e) => e,
            Err(_) => return,
        };
        let h = hint.to_lowercase();
        let found = entries
            .iter()
            .find(|e| e.id.to_lowercase().ends_with(&h) || e.id.to_lowercase().contains(&format!("{h}.desktop")))
            .or_else(|| entries.iter().find(|e| e.name.to_lowercase() == name));
        if let Some(e) = found {
            let _ = st.apps.launch(&st, &e.id).await;
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::score;

    #[test]
    fn scoring_order() {
        let none: Vec<String> = vec![];
        let exact = score("Spotify", &none, "spotify").unwrap();
        let prefix = score("Spotify Premium", &none, "spot").unwrap();
        let contains = score("My Spotify", &none, "otif").unwrap();
        assert!(exact > prefix && prefix > contains);
        assert!(score("Visual Studio Code", &none, "vsc").is_some());
        assert!(score("Calculator", &none, "zzz").is_none());
    }
}
