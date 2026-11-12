//! Манифест плагина и его валидация.

use serde::{Deserialize, Serialize};

use crate::core::error::{AppError, AppResult};

/// Текущая версия Plugin API. Плагин должен указать совпадающую major-версию.
pub const PLUGIN_API_VERSION: u32 = 1;

/// Все разрешения, которые может запросить плагин.
pub const PERMISSIONS: &[(&str, &str)] = &[
    ("notifications", "Показывать уведомления в панели"),
    ("storage", "Хранить собственные данные (до 64 КБ на ключ)"),
    ("system.read", "Читать показатели системы (CPU, RAM, GPU, батарея)"),
    ("media.read", "Видеть текущий трек и медиасессии"),
    ("media.control", "Управлять воспроизведением"),
    ("activity", "Показывать свою активность в компактной панели"),
    ("web.open", "Открывать ссылки из allowedHosts в браузере"),
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CardSpec {
    pub title: String,
    #[serde(default = "default_height")]
    pub height: u32,
}
fn default_height() -> u32 {
    160
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SettingSpec {
    pub key: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub label: String,
    #[serde(default)]
    pub default: serde_json::Value,
    #[serde(default)]
    pub options: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PluginApp {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api_version: u32,
    #[serde(default)]
    pub min_app_version: Option<String>,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub author: String,
    #[serde(default = "default_entry")]
    pub entry: String,
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub card: Option<CardSpec>,
    #[serde(default)]
    pub settings: Vec<SettingSpec>,
    /// Веб-приложения/сервисы, которые плагин добавляет в поиск.
    #[serde(default)]
    pub apps: Vec<PluginApp>,
    /// Хосты, которые можно открывать через web.open.
    #[serde(default)]
    pub allowed_hosts: Vec<String>,
}
fn default_entry() -> String {
    "index.html".into()
}

pub fn safe_relative(path: &str) -> bool {
    !path.is_empty()
        && path.len() < 200
        && !path.starts_with('/')
        && !path.starts_with('\\')
        && !path.contains("..")
        && !path.contains(':')
        && path.chars().all(|c| c.is_ascii_alphanumeric() || "/._-".contains(c))
}

impl Manifest {
    pub fn parse(text: &str) -> AppResult<Self> {
        if text.len() > 64 * 1024 {
            return Err(AppError::invalid("manifest.json слишком большой"));
        }
        let m: Manifest = serde_json::from_str(text).map_err(|e| AppError::invalid(format!("ошибка в manifest.json: {e}")))?;
        m.validate()?;
        Ok(m)
    }

    pub fn validate(&self) -> AppResult<()> {
        crate::util::validate_id(&self.id)?;
        if self.name.trim().is_empty() || self.name.len() > 64 {
            return Err(AppError::invalid("название плагина должно быть от 1 до 64 символов"));
        }
        semver::Version::parse(&self.version).map_err(|_| AppError::invalid("version должна быть в формате semver (1.0.0)"))?;
        if self.api_version != PLUGIN_API_VERSION {
            return Err(AppError::invalid(format!(
                "плагин рассчитан на Plugin API v{}, приложение поддерживает v{}",
                self.api_version, PLUGIN_API_VERSION
            )));
        }
        if let Some(min) = &self.min_app_version {
            let min = semver::Version::parse(min).map_err(|_| AppError::invalid("minAppVersion должна быть в формате semver"))?;
            let app = semver::Version::parse(env!("CARGO_PKG_VERSION")).expect("app version");
            if app < min {
                return Err(AppError::invalid(format!("требуется Desktop Island {min} или новее (установлена {app})")));
            }
        }
        if !safe_relative(&self.entry) || !(self.entry.ends_with(".html") || self.entry.ends_with(".htm")) {
            return Err(AppError::invalid("entry должен указывать на HTML-файл внутри плагина"));
        }
        if let Some(icon) = &self.icon {
            if !safe_relative(icon) {
                return Err(AppError::invalid("недопустимый путь к иконке"));
            }
        }
        for p in &self.permissions {
            if !PERMISSIONS.iter().any(|(k, _)| k == p) {
                return Err(AppError::invalid(format!("неизвестное разрешение «{p}»")));
            }
        }
        if self.settings.len() > 32 {
            return Err(AppError::invalid("слишком много настроек (максимум 32)"));
        }
        for s in &self.settings {
            if !matches!(s.kind.as_str(), "string" | "number" | "boolean" | "select") {
                return Err(AppError::invalid(format!("неизвестный тип настройки «{}»", s.kind)));
            }
            if s.key.is_empty() || s.key.len() > 40 || !s.key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Err(AppError::invalid("ключ настройки может содержать только латиницу, цифры и _"));
            }
        }
        for a in &self.apps {
            crate::util::validate_web_url(&a.url)?;
        }
        for h in &self.allowed_hosts {
            if h.is_empty() || h.len() > 253 || !h.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-') {
                return Err(AppError::invalid(format!("недопустимый хост «{h}»")));
            }
        }
        if let Some(c) = &self.card {
            if c.title.trim().is_empty() || c.title.len() > 48 || !(60..=600).contains(&c.height) {
                return Err(AppError::invalid("card: title 1–48 символов, height 60–600"));
            }
        }
        Ok(())
    }

    pub fn validate_setting(&self, key: &str, value: &serde_json::Value) -> AppResult<()> {
        let spec = self.settings.iter().find(|s| s.key == key).ok_or_else(|| AppError::invalid("неизвестная настройка"))?;
        let ok = match spec.kind.as_str() {
            "string" => value.as_str().map(|s| s.len() <= 2000).unwrap_or(false),
            "number" => value.as_f64().map(|n| n.is_finite()).unwrap_or(false),
            "boolean" => value.is_boolean(),
            "select" => value.as_str().map(|s| spec.options.iter().any(|o| o == s)).unwrap_or(false),
            _ => false,
        };
        if ok {
            Ok(())
        } else {
            Err(AppError::invalid(format!("недопустимое значение для «{}»", spec.label)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> serde_json::Value {
        serde_json::json!({
            "id": "com.example.hello", "name": "Hello", "version": "1.0.0", "apiVersion": 1,
            "permissions": ["notifications"], "card": { "title": "Hello", "height": 150 }
        })
    }

    #[test]
    fn valid_manifest() {
        assert!(Manifest::parse(&base().to_string()).is_ok());
    }

    #[test]
    fn rejects_bad_manifests() {
        let mut m = base();
        m["apiVersion"] = 2.into();
        assert!(Manifest::parse(&m.to_string()).is_err());
        let mut m = base();
        m["permissions"] = serde_json::json!(["fs.write"]);
        assert!(Manifest::parse(&m.to_string()).is_err());
        let mut m = base();
        m["entry"] = "../../evil.html".into();
        assert!(Manifest::parse(&m.to_string()).is_err());
        let mut m = base();
        m["minAppVersion"] = "99.0.0".into();
        assert!(Manifest::parse(&m.to_string()).is_err());
    }

    #[test]
    fn relative_paths() {
        assert!(safe_relative("assets/app.js"));
        assert!(!safe_relative("../x"));
        assert!(!safe_relative("C:/x"));
        assert!(!safe_relative("/etc/passwd"));
    }
}
