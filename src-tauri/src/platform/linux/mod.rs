//! Адаптеры Linux: D-Bus (zbus) — MPRIS, BlueZ, NetworkManager, logind,
//! power-profiles-daemon, org.freedesktop.Notifications; sysfs — батарея,
//! подсветка, GPU AMD; X11 (x11rb) — активное окно и полноэкранный режим;
//! PulseAudio/PipeWire — через `pactl`/`wpctl`.
//!
//! Публичные функции блокирующие (вызываются из `spawn_blocking`) и внутри
//! выполняют async-код zbus на рантайме Tokio.

pub mod apps;
pub mod audio;
pub mod banners;
pub mod bluetooth;
pub mod brightness;
pub mod clipboard;
pub mod foreground;
pub mod gpu;
pub mod media;
pub mod network;
pub mod notifications;
pub mod power;
pub mod privacy;
pub mod session;
pub mod proxy;
pub mod shell;
pub mod window;

use std::future::Future;
use tokio::sync::OnceCell;
use zbus::zvariant::Value;

use super::types::EnvironmentInfo;
use crate::core::error::{AppError, AppResult};

static SESSION: OnceCell<zbus::Connection> = OnceCell::const_new();
static SYSTEM: OnceCell<zbus::Connection> = OnceCell::const_new();

pub async fn session_bus() -> AppResult<&'static zbus::Connection> {
    SESSION
        .get_or_try_init(|| async { zbus::Connection::session().await })
        .await
        .map_err(|e| AppError::unsupported(format!("сессионная шина D-Bus недоступна: {e}")))
}

pub async fn system_bus() -> AppResult<&'static zbus::Connection> {
    SYSTEM
        .get_or_try_init(|| async { zbus::Connection::system().await })
        .await
        .map_err(|e| AppError::unsupported(format!("системная шина D-Bus недоступна: {e}")))
}

/// Выполняет future из блокирующего потока (spawn_blocking).
pub fn block<F: Future>(f: F) -> F::Output {
    tauri::async_runtime::block_on(f)
}

pub async fn proxy<'a>(
    conn: &'a zbus::Connection,
    dest: &'a str,
    path: &'a str,
    iface: &'a str,
) -> AppResult<zbus::Proxy<'a>> {
    Ok(zbus::proxy::Builder::<zbus::Proxy>::new(conn)
        .destination(dest)?
        .path(path)?
        .interface(iface)?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await?)
}

pub fn val_str(v: &Value) -> Option<String> {
    match v {
        Value::Str(s) => Some(s.to_string()),
        Value::ObjectPath(p) => Some(p.to_string()),
        Value::Value(inner) => val_str(inner),
        _ => None,
    }
}

pub fn val_i64(v: &Value) -> Option<i64> {
    match v {
        Value::I64(x) => Some(*x),
        Value::U64(x) => Some(*x as i64),
        Value::I32(x) => Some(*x as i64),
        Value::U32(x) => Some(*x as i64),
        Value::U8(x) => Some(*x as i64),
        Value::F64(x) => Some(*x as i64),
        Value::Value(inner) => val_i64(inner),
        _ => None,
    }
}

pub fn val_bool(v: &Value) -> Option<bool> {
    match v {
        Value::Bool(b) => Some(*b),
        Value::Value(inner) => val_bool(inner),
        _ => None,
    }
}

pub fn val_str_list(v: &Value) -> Vec<String> {
    match v {
        Value::Array(a) => a.iter().filter_map(val_str).collect(),
        Value::Str(s) => vec![s.to_string()],
        Value::Value(inner) => val_str_list(inner),
        _ => vec![],
    }
}

pub fn read_trim(path: impl AsRef<std::path::Path>) -> Option<String> {
    std::fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

pub fn command_exists(name: &str) -> bool {
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|p| p.join(name).is_file()))
        .unwrap_or(false)
}

pub fn desktop_name() -> String {
    std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default()
}

pub fn is_wayland() -> bool {
    std::env::var("XDG_SESSION_TYPE").map(|s| s.eq_ignore_ascii_case("wayland")).unwrap_or(false)
        || std::env::var_os("WAYLAND_DISPLAY").is_some()
}

/// Запуск внутри WSL (Windows Subsystem for Linux, графика через WSLg).
pub fn is_wsl() -> bool {
    std::env::var_os("WSL_DISTRO_NAME").is_some()
        || read_trim("/proc/sys/kernel/osrelease").map(|s| s.to_lowercase().contains("microsoft")).unwrap_or(false)
}

/// Окно работает через XWayland, если мы принудительно выставили GDK_BACKEND=x11.
pub fn running_on_xwayland() -> bool {
    is_wayland() && std::env::var("GDK_BACKEND").map(|v| v == "x11").unwrap_or(false)
}

/// Вызывается до создания окон: под Wayland клиент не может сам задавать
/// позицию окна и «поверх всех», поэтому по умолчанию используем XWayland.
pub fn prepare_environment() {
    if is_wayland() && std::env::var_os("GDK_BACKEND").is_none() && std::env::var_os("DESKTOP_ISLAND_NATIVE_WAYLAND").is_none() {
        std::env::set_var("GDK_BACKEND", "x11");
    }
    if std::env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_none() {
        // Обходит пустые окна WebKitGTK на некоторых драйверах (NVIDIA).
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }
    if std::env::var_os("WEBKIT_DISABLE_COMPOSITING_MODE").is_none() {
        // Без аппаратного композитинга WebKitGTK 2.4x–2.5x рисует пустое (чёрное)
        // окно на части драйверов и в виртуальных машинах. Остров маленький,
        // программной отрисовки ему хватает с запасом (как и на Windows).
        std::env::set_var("WEBKIT_DISABLE_COMPOSITING_MODE", "1");
    }
    if is_wsl() && std::env::var_os("LIBGL_ALWAYS_SOFTWARE").is_none() {
        // WSLg: OpenGL через d3d12 не рисует содержимое WebKit.
        std::env::set_var("LIBGL_ALWAYS_SOFTWARE", "1");
    }
}

pub fn environment() -> EnvironmentInfo {
    let desktop = desktop_name();
    let wayland = is_wayland();
    let xwayland = running_on_xwayland();
    let mut notes = vec![];
    if wayland {
        if xwayland {
            notes.push("Wayland: панель работает через XWayland, чтобы задавать позицию и режим «поверх окон».".into());
            notes.push("Wayland: глобальные горячие клавиши срабатывают только когда активно окно XWayland. Назначьте сочетания в настройках окружения рабочего стола (команда: desktop-island --toggle).".into());
        } else {
            notes.push("Нативный Wayland: позиционирование окна и «поверх окон» контролирует композитор.".into());
        }
        notes.push("Wayland: чтение буфера обмена в фоне возможно только при поддержке протокола data-control (KDE, wlroots). В GNOME — частично через XWayland.".into());
        notes.push("Wayland: определение полноэкранного окна доступно только для приложений XWayland; для нативных — по списку игровых процессов.".into());
    }
    let os_version = read_trim("/etc/os-release")
        .and_then(|s| s.lines().find(|l| l.starts_with("PRETTY_NAME=")).map(|l| l[12..].trim_matches('"').to_string()))
        .unwrap_or_else(|| "Linux".into());
    EnvironmentInfo {
        os: "linux".into(),
        os_version,
        desktop: if desktop.is_empty() { None } else { Some(desktop) },
        session_type: std::env::var("XDG_SESSION_TYPE").ok(),
        global_hotkeys_supported: !wayland || xwayland,
        notes,
    }
}
