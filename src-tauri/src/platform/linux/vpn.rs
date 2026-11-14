//! Состояние VPN по сетевым интерфейсам (/sys/class/net).

pub fn adapter_up(names: &[&str]) -> bool {
    names.iter().any(|n| {
        std::fs::read_to_string(format!("/sys/class/net/{n}/operstate"))
            .map(|s| s.trim() != "down")
            .unwrap_or(false)
    })
}

/// Программа, зарегистрированная для схемы happ:// или incy://.
pub fn protocol_handler(scheme: &str) -> Option<String> {
    let out = std::process::Command::new("xdg-mime").args(["query", "default", &format!("x-scheme-handler/{scheme}")]).output().ok()?;
    let desktop = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!desktop.is_empty()).then_some(desktop)
}
