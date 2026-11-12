//! Мини-окна веб-версий нейросетей (Claude, ChatGPT, DeepSeek, Gemini и
//! пользовательские веб-сервисы). Каждый сервис получает собственный каталог
//! данных WebView — cookies, localStorage и авторизация не смешиваются.
//! Окна загружают удалённые страницы и не имеют доступа к IPC Desktop Island
//! (capabilities выдаются только локальным окнам island/settings).
//!
//! Память: каждое мини-окно — отдельный процесс WebView (~100–250 МБ).
//! По умолчанию одновременно открыто только одно окно.

use serde::Serialize;
use std::sync::Mutex;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::core::error::{AppError, AppResult};
use crate::core::state::Shared;
use crate::integrations::{catalog, list_custom};

#[derive(Default)]
pub struct AiWindows {
    open: Mutex<Vec<String>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebService {
    pub id: String,
    pub name: String,
    pub url: String,
    pub builtin: bool,
    pub app_id: Option<String>,
    pub open: bool,
}

fn label_for(id: &str) -> String {
    let safe: String = id.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).take(48).collect();
    format!("web-{safe}")
}

impl AiWindows {
    pub async fn services(&self, app: &AppHandle, st: &Shared) -> Vec<WebService> {
        let apps = st.apps.all_entries(st).await.unwrap_or_default();
        let mut out: Vec<WebService> = catalog::CATALOG
            .iter()
            .filter(|k| k.category == "ai")
            .filter_map(|k| {
                Some(WebService {
                    id: k.id.into(),
                    name: k.name.into(),
                    url: k.web_url?.into(),
                    builtin: true,
                    app_id: apps.iter().find(|a| a.catalog_id.as_deref() == Some(k.id) && a.source == "system").map(|a| a.id.clone()),
                    open: app.get_webview_window(&label_for(k.id)).is_some(),
                })
            })
            .collect();
        for c in list_custom(&st.db).unwrap_or_default().into_iter().filter(|c| c.kind == "web") {
            out.push(WebService { open: app.get_webview_window(&label_for(&c.id)).is_some(), id: c.id, name: c.name, url: c.target, builtin: false, app_id: None });
        }
        out
    }

    pub async fn open(&self, app: &AppHandle, st: &Shared, service_id: &str) -> AppResult<()> {
        let services = self.services(app, st).await;
        let svc = services.into_iter().find(|s| s.id == service_id).ok_or_else(|| AppError::NotFound("сервис не найден".into()))?;
        let url = crate::util::validate_web_url(&svc.url)?;
        let label = label_for(&svc.id);
        if let Some(w) = app.get_webview_window(&label) {
            let _ = w.unminimize();
            let _ = w.show();
            let _ = w.set_focus();
            return Ok(());
        }
        if st.with_settings(|s| s.performance.single_ai_window) {
            self.close_all(app);
        }
        let dir_name: String = label.trim_start_matches("web-").to_string();
        let data_dir = st.paths.webdata.join(dir_name);
        std::fs::create_dir_all(&data_dir)?;
        let parsed: tauri::Url = url.parse().map_err(|_| AppError::invalid("некорректный URL"))?;

        // Располагаем окно под панелью.
        let (x, y) = crate::core::window::island(app)
            .and_then(|w| Some((w.outer_position().ok()?, w.outer_size().ok()?, w.scale_factor().ok()?)))
            .map(|(p, s, scale)| ((p.x as f64 + s.width as f64 / 2.0) / scale - 230.0, (p.y as f64 + s.height as f64) / scale + 12.0))
            .unwrap_or((100.0, 80.0));

        WebviewWindowBuilder::new(app, &label, WebviewUrl::External(parsed))
            .title(format!("{} — Desktop Island", svc.name))
            .inner_size(460.0, 700.0)
            .min_inner_size(360.0, 420.0)
            .position(x.max(0.0), y.max(0.0))
            .data_directory(data_dir)
            .always_on_top(true)
            .resizable(true)
            .build()
            .map_err(|e| AppError::system(format!("не удалось открыть окно: {e}")))?;
        self.open.lock().unwrap_or_else(|p| p.into_inner()).push(label);
        Ok(())
    }

    pub fn close(&self, app: &AppHandle, service_id: &str) {
        let label = label_for(service_id);
        if let Some(w) = app.get_webview_window(&label) {
            let _ = w.destroy();
        }
        self.open.lock().unwrap_or_else(|p| p.into_inner()).retain(|l| l != &label);
    }

    pub fn close_all(&self, app: &AppHandle) {
        let labels: Vec<String> = std::mem::take(&mut *self.open.lock().unwrap_or_else(|p| p.into_inner()));
        for l in labels {
            if let Some(w) = app.get_webview_window(&l) {
                let _ = w.destroy();
            }
        }
    }

    /// Удаляет cookies и данные сервиса (выход из аккаунта).
    pub fn clear_data(&self, app: &AppHandle, st: &Shared, service_id: &str) -> AppResult<()> {
        self.close(app, service_id);
        let dir = st.paths.webdata.join(label_for(service_id).trim_start_matches("web-"));
        if dir.starts_with(&st.paths.webdata) && dir.exists() {
            // WebView может удерживать файлы ещё несколько секунд после закрытия.
            let dir2 = dir.clone();
            tauri::async_runtime::spawn(async move {
                for _ in 0..10 {
                    if std::fs::remove_dir_all(&dir2).is_ok() {
                        return;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
            });
        }
        Ok(())
    }
}
