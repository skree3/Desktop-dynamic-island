//! `desktop-island --selftest` — вызывает все системные адаптеры текущей ОС и
//! печатает фактический результат (или ошибку) и время выполнения. Используется
//! для проверки интеграций на реальной системе и для отчёта о тестировании.

use std::time::{Duration, Instant};

use crate::platform::os;

fn check<T: std::fmt::Debug + Send + 'static>(name: &str, f: impl FnOnce() -> T + Send + 'static) {
    let started = Instant::now();
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(f());
    });
    match rx.recv_timeout(Duration::from_secs(15)) {
        Ok(v) => {
            let mut text = format!("{v:?}");
            if text.len() > 600 {
                text.truncate(600);
                text.push_str("…");
            }
            println!("[{:>6} ms] {name}: {text}", started.elapsed().as_millis());
        }
        Err(_) => println!("[TIMEOUT ] {name}: нет ответа за 15 с"),
    }
}

pub fn run() {
    let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("tokio");
    let _guard = rt.enter();
    tauri::async_runtime::set(rt.handle().clone());
    println!("Desktop Island {} — самопроверка адаптеров ({})", env!("CARGO_PKG_VERSION"), std::env::consts::OS);
    check("environment", os::environment);
    check("media::list_sessions", || os::media::list_sessions().map(|l| l.into_iter().map(|s| (s.app_name, s.title, s.status, s.duration_ms, s.can_next)).collect::<Vec<_>>()));
    check("audio::state", || os::audio::state().map(|v| (v.level, v.muted, v.device.map(|d| d.name), v.devices.len(), v.backend)));
    check("brightness::state", os::brightness::state);
    check("network::wifi_state", os::network::wifi_state);
    check("network::airplane_state", os::network::airplane_state);
    check("bluetooth::state", || os::bluetooth::state().map(|b| (b.available, b.powered, b.devices.iter().map(|d| (d.name.clone(), d.connected)).collect::<Vec<_>>(), b.error)));
    check("power::state", os::power::state);
    check("foreground::current", os::foreground::current);
    check("proxy::system_proxy", os::proxy::system_proxy);
    check("clipboard::sequence/is_sensitive/owner", || (os::clipboard::sequence(), os::clipboard::is_sensitive(), os::clipboard::owner_app()));
    check("apps::scan", || {
        let t = Instant::now();
        os::apps::scan().map(|l| {
            let sample: Vec<String> = l.iter().take(8).map(|a| a.name.clone()).collect();
            (l.len(), format!("{} мс", t.elapsed().as_millis()), sample)
        })
    });
    check("apps::icon(first)", || {
        let list = os::apps::scan().unwrap_or_default();
        list.first().map(|a| os::apps::icon(&a.target, a.icon_hint.as_deref(), 32).map(|i| i.map(|s| s.len())))
    });
    check("integrations::catalog matches", || {
        let list = os::apps::scan().unwrap_or_default();
        list.iter()
            .filter_map(|a| crate::integrations::catalog::match_app(&a.name, a.exe_path.as_deref()).map(|k| format!("{} → {}", a.name, k.id)))
            .collect::<Vec<_>>()
    });
    #[cfg(windows)]
    check("notifications::open (UserNotificationListener)", || match os::notifications::open() {
        Ok(l) => l.status(),
        Err(s) => s,
    });
    #[cfg(windows)]
    check("gpu::PdhGpu", || {
        let mut g = os::gpu::PdhGpu::new()?;
        std::thread::sleep(Duration::from_millis(500));
        g.sample()
    });
    check("nvml", || nvml_wrapper::Nvml::init().map(|n| n.device_by_index(0).and_then(|d| d.name()).map_err(|e| e.to_string())).map_err(|e| e.to_string()));
    check("sysinfo cpu/ram", || {
        let mut s = sysinfo::System::new();
        s.refresh_cpu_usage();
        std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
        s.refresh_cpu_usage();
        s.refresh_memory();
        (s.global_cpu_usage(), s.used_memory() / 1024 / 1024, s.total_memory() / 1024 / 1024)
    });
    check("sysinfo components (temps)", || sysinfo::Components::new_with_refreshed_list().iter().map(|c| (c.label().to_string(), c.temperature())).collect::<Vec<_>>());
    println!("Готово.");
}
