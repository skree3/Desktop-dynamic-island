//! Шим-крейт для проверки компиляции Linux-адаптеров (см. Cargo.toml).
#![allow(dead_code)]

pub mod core {
    pub mod error {
        #[derive(Debug, thiserror::Error)]
        pub enum AppError {
            #[error("{0}")]
            Unsupported(String),
            #[error("{0}")]
            PermissionDenied(String),
            #[error("{0}")]
            InvalidInput(String),
            #[error("{0}")]
            NotFound(String),
            #[error("{0}")]
            Io(#[from] std::io::Error),
            #[error("{0}")]
            System(String),
        }
        impl AppError {
            pub fn unsupported(m: impl Into<String>) -> Self { Self::Unsupported(m.into()) }
            pub fn invalid(m: impl Into<String>) -> Self { Self::InvalidInput(m.into()) }
            pub fn system(m: impl Into<String>) -> Self { Self::System(m.into()) }
        }
        impl From<zbus::Error> for AppError {
            fn from(e: zbus::Error) -> Self { Self::System(e.to_string()) }
        }
        impl From<zbus::fdo::Error> for AppError {
            fn from(e: zbus::fdo::Error) -> Self { Self::System(e.to_string()) }
        }
        impl From<zbus::zvariant::Error> for AppError {
            fn from(e: zbus::zvariant::Error) -> Self { Self::System(e.to_string()) }
        }
        pub type AppResult<T> = Result<T, AppError>;
    }
}

pub mod database {
    pub fn now_ms() -> i64 { 0 }
}

pub mod modules {
    pub mod music {
        pub fn friendly_app_name(s: &str) -> String { s.to_string() }
    }
}

pub mod util {
    #[path = "../../../../src-tauri/src/util/image.rs"]
    pub mod image;
}

pub mod platform {
    #[path = "../../../../src-tauri/src/platform/types.rs"]
    pub mod types;
    #[path = "../../../../src-tauri/src/platform/linux/mod.rs"]
    pub mod linux;
}
