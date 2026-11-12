//! Чтение (только чтение) системного прокси в Linux: переменные окружения и GNOME gsettings.

use std::process::Command;

use crate::platform::types::ProxyInfo;

pub fn system_proxy() -> ProxyInfo {
    for var in ["https_proxy", "HTTPS_PROXY", "http_proxy", "HTTP_PROXY", "all_proxy", "ALL_PROXY"] {
        if let Ok(v) = std::env::var(var) {
            if !v.is_empty() {
                return ProxyInfo { enabled: true, server: Some(v), auto_config: None, source: format!("переменная окружения {var}") };
            }
        }
    }
    if super::command_exists("gsettings") {
        let get = |schema: &str, key: &str| {
            Command::new("gsettings").args(["get", schema, key]).output().ok().map(|o| String::from_utf8_lossy(&o.stdout).trim().trim_matches('\'').to_string())
        };
        if let Some(mode) = get("org.gnome.system.proxy", "mode") {
            let enabled = mode == "manual" || mode == "auto";
            let server = if mode == "manual" {
                let host = get("org.gnome.system.proxy.http", "host").unwrap_or_default();
                let port = get("org.gnome.system.proxy.http", "port").unwrap_or_default();
                if host.is_empty() { None } else { Some(format!("{host}:{port}")) }
            } else {
                None
            };
            let auto = if mode == "auto" { get("org.gnome.system.proxy", "autoconfig-url") } else { None };
            return ProxyInfo { enabled, server, auto_config: auto, source: "GNOME gsettings (только чтение)".into() };
        }
    }
    ProxyInfo { enabled: false, server: None, auto_config: None, source: "переменные окружения".into() }
}
