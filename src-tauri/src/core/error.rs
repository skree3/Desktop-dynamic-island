//! Единый тип ошибок приложения. Сериализуется во фронтенд как
//! `{ kind, message }`, чтобы UI мог отличать «недоступно» от «отказано в доступе».

use serde::ser::SerializeStruct;
use serde::{Serialize, Serializer};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Функция недоступна: {0}")]
    Unsupported(String),
    #[error("Отказано в доступе: {0}")]
    PermissionDenied(String),
    #[error("Неверные данные: {0}")]
    InvalidInput(String),
    #[error("Не найдено: {0}")]
    NotFound(String),
    #[error("Ошибка базы данных: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("Ошибка ввода-вывода: {0}")]
    Io(#[from] std::io::Error),
    #[error("Системная ошибка: {0}")]
    System(String),
    #[error("{0}")]
    Other(String),
}

impl AppError {
    pub fn kind(&self) -> &'static str {
        match self {
            AppError::Unsupported(_) => "unsupported",
            AppError::PermissionDenied(_) => "permission_denied",
            AppError::InvalidInput(_) => "invalid_input",
            AppError::NotFound(_) => "not_found",
            AppError::Database(_) => "database",
            AppError::Io(_) => "io",
            AppError::System(_) => "system",
            AppError::Other(_) => "other",
        }
    }

    pub fn unsupported(msg: impl Into<String>) -> Self {
        AppError::Unsupported(msg.into())
    }
    pub fn invalid(msg: impl Into<String>) -> Self {
        AppError::InvalidInput(msg.into())
    }
    pub fn system(msg: impl Into<String>) -> Self {
        AppError::System(msg.into())
    }
}

impl Serialize for AppError {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("kind", self.kind())?;
        s.serialize_field("message", &self.to_string())?;
        s.end()
    }
}

impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        AppError::InvalidInput(e.to_string())
    }
}

impl From<tauri::Error> for AppError {
    fn from(e: tauri::Error) -> Self {
        AppError::System(e.to_string())
    }
}

impl From<tokio::task::JoinError> for AppError {
    fn from(e: tokio::task::JoinError) -> Self {
        AppError::System(format!("фоновая задача завершилась с ошибкой: {e}"))
    }
}

#[cfg(windows)]
impl From<windows::core::Error> for AppError {
    fn from(e: windows::core::Error) -> Self {
        const E_ACCESSDENIED: i32 = 0x8007_0005_u32 as i32;
        let code = e.code().0;
        let msg = e.message();
        let text = if msg.is_empty() {
            format!("HRESULT 0x{:08X}", code as u32)
        } else {
            format!("{} (0x{:08X})", msg.trim(), code as u32)
        };
        if code == E_ACCESSDENIED {
            AppError::PermissionDenied(text)
        } else {
            AppError::System(text)
        }
    }
}

#[cfg(target_os = "linux")]
impl From<zbus::Error> for AppError {
    fn from(e: zbus::Error) -> Self {
        let text = e.to_string();
        if text.contains("AccessDenied") || text.contains("NotAuthorized") || text.contains("PermissionDenied") {
            AppError::PermissionDenied(text)
        } else if text.contains("ServiceUnknown") || text.contains("NameHasNoOwner") {
            AppError::Unsupported(format!("служба D-Bus не найдена: {text}"))
        } else {
            AppError::System(text)
        }
    }
}

#[cfg(target_os = "linux")]
impl From<zbus::fdo::Error> for AppError {
    fn from(e: zbus::fdo::Error) -> Self {
        AppError::from(zbus::Error::from(e))
    }
}

pub type AppResult<T> = Result<T, AppError>;
