//! Поиск приложений, интеграции, веб-окна нейросетей, плагины.

use tauri::{AppHandle, State};

use crate::core::error::{AppError, AppResult};
use crate::core::state::Shared;
use crate::integrations::ai::WebService;
use crate::integrations::{self, Capability, CustomApp, CustomAppInput, IntegrationStatus};
use crate::modules::applications::AppEntry;
use crate::plugins::{InstallResult, PluginInfo};

fn require(st: &Shared, module: &str) -> AppResult<()> {
    if st.with_settings(|s| s.modules.is_enabled(module)) {
        Ok(())
    } else {
        Err(AppError::unsupported("модуль отключён в настройках"))
    }
}

// ---- приложения ----

#[tauri::command]
pub async fn apps_search(state: State<'_, Shared>, query: String, limit: Option<usize>) -> AppResult<Vec<AppEntry>> {
    require(&state, "applications")?;
    let st = state.inner().clone();
    st.apps.search(&st, &query.chars().take(100).collect::<String>(), limit.unwrap_or(40)).await
}

#[tauri::command]
pub async fn apps_icon(state: State<'_, Shared>, id: String) -> AppResult<Option<String>> {
    let st = state.inner().clone();
    st.apps.icon(&st, &id).await
}

#[tauri::command]
pub async fn apps_launch(state: State<'_, Shared>, id: String) -> AppResult<()> {
    let st = state.inner().clone();
    st.apps.launch(&st, &id).await
}

#[tauri::command]
pub async fn apps_refresh(state: State<'_, Shared>) -> AppResult<usize> {
    let st = state.inner().clone();
    st.apps.clear_icon_cache();
    Ok(st.apps.index(true).await?.len())
}

#[tauri::command]
pub async fn apps_set_favorite(app: AppHandle, state: State<'_, Shared>, id: String, favorite: bool) -> AppResult<()> {
    let st = state.inner().clone();
    st.update_settings(&app, |s| {
        s.apps.favorites.retain(|f| f != &id);
        if favorite {
            s.apps.favorites.insert(0, id.clone());
        }
        Ok(())
    })?;
    Ok(())
}

#[tauri::command]
pub async fn apps_set_hidden(app: AppHandle, state: State<'_, Shared>, id: String, hidden: bool) -> AppResult<()> {
    let st = state.inner().clone();
    st.update_settings(&app, |s| {
        s.apps.hidden.retain(|f| f != &id);
        if hidden {
            s.apps.hidden.push(id.clone());
        }
        Ok(())
    })?;
    Ok(())
}

// ---- интеграции ----

#[tauri::command]
pub async fn integrations_status(state: State<'_, Shared>) -> AppResult<Vec<IntegrationStatus>> {
    require(&state, "integrations")?;
    integrations::status(state.inner()).await
}

#[tauri::command]
pub async fn integration_capabilities(state: State<'_, Shared>, kind: String, target: String, catalog_id: Option<String>) -> AppResult<Vec<Capability>> {
    Ok(integrations::capabilities(state.inner(), &kind, &target, catalog_id.as_deref()))
}

#[tauri::command]
pub async fn custom_apps_list(state: State<'_, Shared>) -> AppResult<Vec<CustomApp>> {
    integrations::list_custom(&state.db)
}

#[tauri::command]
pub async fn custom_app_save(state: State<'_, Shared>, input: CustomAppInput) -> AppResult<CustomApp> {
    let st = state.inner().clone();
    let app = tauri::async_runtime::spawn_blocking(move || integrations::save_custom(&st, input)).await??;
    state.log.info(crate::logging::category::INTEGRATION, "apps", format!("добавлено приложение «{}»", app.name));
    Ok(app)
}

#[tauri::command]
pub async fn custom_app_delete(state: State<'_, Shared>, id: String) -> AppResult<()> {
    integrations::delete_custom(state.inner(), &id)
}

// ---- нейросети / веб-сервисы ----

#[tauri::command]
pub async fn ai_services(app: AppHandle, state: State<'_, Shared>) -> AppResult<Vec<WebService>> {
    require(&state, "ai")?;
    let st = state.inner().clone();
    Ok(st.ai.services(&app, &st).await)
}

#[tauri::command]
pub async fn ai_open(app: AppHandle, state: State<'_, Shared>, id: String) -> AppResult<()> {
    require(&state, "ai")?;
    let st = state.inner().clone();
    st.ai.open(&app, &st, &id).await
}

#[tauri::command]
pub async fn ai_close(app: AppHandle, state: State<'_, Shared>, id: String) -> AppResult<()> {
    state.ai.close(&app, &id);
    Ok(())
}

#[tauri::command]
pub async fn ai_clear_data(app: AppHandle, state: State<'_, Shared>, id: String) -> AppResult<()> {
    let st = state.inner().clone();
    st.ai.clear_data(&app, &st, &id)
}

// ---- плагины ----

#[tauri::command]
pub async fn plugins_list(state: State<'_, Shared>) -> AppResult<Vec<PluginInfo>> {
    Ok(state.plugins.list())
}

#[tauri::command]
pub async fn plugins_permissions_catalog() -> AppResult<Vec<(String, String)>> {
    Ok(crate::plugins::PERMISSIONS.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect())
}

#[tauri::command]
pub async fn plugin_install(state: State<'_, Shared>, path: String) -> AppResult<InstallResult> {
    let st = state.inner().clone();
    let res = tauri::async_runtime::spawn_blocking(move || st.plugins.install(&st, std::path::Path::new(&path))).await?;
    if let Err(e) = &res {
        state.log.error(crate::logging::category::PLUGIN, "plugins", format!("ошибка установки плагина: {e}"));
    }
    res
}

#[tauri::command]
pub async fn plugin_set_enabled(state: State<'_, Shared>, id: String, enabled: bool, approve_permissions: bool) -> AppResult<PluginInfo> {
    let st = state.inner().clone();
    st.plugins.set_enabled(&st, &id, enabled, approve_permissions)
}

#[tauri::command]
pub async fn plugin_uninstall(state: State<'_, Shared>, id: String) -> AppResult<()> {
    let st = state.inner().clone();
    st.plugins.uninstall(&st, &id)
}

#[tauri::command]
pub async fn plugin_settings_get(state: State<'_, Shared>, id: String) -> AppResult<serde_json::Map<String, serde_json::Value>> {
    state.plugins.get_settings(&state.db, &id)
}

#[tauri::command]
pub async fn plugin_setting_set(app: AppHandle, state: State<'_, Shared>, id: String, key: String, value: serde_json::Value) -> AppResult<()> {
    state.plugins.set_setting(&app, &state.db, &id, &key, value)
}

/// Единственная точка входа для кода плагинов (через postMessage-мост во фронтенде).
#[tauri::command]
pub async fn plugin_call(app: AppHandle, state: State<'_, Shared>, plugin_id: String, method: String, params: serde_json::Value) -> AppResult<serde_json::Value> {
    if method.len() > 64 {
        return Err(AppError::invalid("method"));
    }
    let st = state.inner().clone();
    st.plugins.call(&app, &st, &plugin_id, &method, params).await
}
