//! Индикаторы: смена раскладки и Caps Lock (короткое уведомление без звука),
//! режим «в самолёте» (самолётик летит, пока режим включён) и подключение VPN
//! (Happ, INCY). Один фоновый опрос: клавиатура — 150 мс, радио и VPN — 2 с.

use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter};

use crate::core::error::{AppError, AppResult};
use crate::core::state::Shared;
use crate::core::task::TaskSlot;
use crate::database::now_ms;
use crate::events::{Activity, IslandNotification, NotificationKind, Priority};
use crate::platform::os;

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct KbdState {
    pub layout: Option<String>,
    pub caps: bool,
}

#[derive(Debug, Clone, Serialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VpnStatus {
    /// happ | incy | "" (клиент не найден)
    pub client: String,
    pub name: String,
    pub installed: bool,
    pub connected: bool,
    /// Можно ли подключать и отключать одной кнопкой (иначе кнопка открывает программу).
    pub can_control: bool,
    pub path: Option<String>,
}

#[derive(Default)]
pub struct IndicatorsModule {
    task: TaskSlot,
    kbd: Mutex<KbdState>,
    flash_gen: AtomicU64,
}

struct Client {
    id: &'static str,
    name: &'static str,
    scheme: &'static str,
    adapters: &'static [&'static str],
}

const CLIENTS: &[Client] = &[
    Client { id: "happ", name: "Happ", scheme: "happ", adapters: &["happ-tun"] },
    Client { id: "incy", name: "INCY", scheme: "incy", adapters: &["wwan99", "incy", "incy-tun"] },
];

fn client_path(c: &Client, custom: &str) -> Option<String> {
    if !custom.is_empty() && std::path::Path::new(custom).exists() {
        return Some(custom.to_string());
    }
    os::vpn::protocol_handler(c.scheme)
}

/// Состояние выбранного (или найденного) VPN-клиента. Блокирующая.
pub fn vpn_status(st: &Shared) -> VpnStatus {
    let cfg = st.settings().vpn;
    if cfg.client == "none" {
        return VpnStatus::default();
    }
    let candidates: Vec<&Client> = CLIENTS.iter().filter(|c| cfg.client == "auto" || cfg.client == c.id).collect();
    let mut found: Vec<VpnStatus> = candidates
        .iter()
        .map(|c| {
            let path = client_path(c, if cfg.client == c.id { &cfg.path } else { "" });
            VpnStatus {
                client: c.id.into(),
                name: c.name.into(),
                installed: path.is_some(),
                connected: os::vpn::adapter_up(c.adapters),
                can_control: c.id == "happ",
                path,
            }
        })
        .collect();
    // Сначала подключённый, потом установленный, потом по порядку в списке.
    found.sort_by_key(|s| (!s.connected, !s.installed));
    found.into_iter().find(|s| s.installed || s.connected || cfg.client != "auto").unwrap_or_default()
}

/// Подключить/отключить VPN. Happ понимает happ://connect и happ://disconnect
/// (подключается к серверу, выбранному в самом Happ); INCY просто открывается.
pub fn vpn_set(st: &Shared, on: bool) -> AppResult<VpnStatus> {
    let s = vpn_status(st);
    if s.client.is_empty() || !s.installed {
        return Err(AppError::NotFound("VPN-клиент не найден. Укажите программу в «Настройки → VPN».".into()));
    }
    match s.client.as_str() {
        "happ" => os::shell::open_uri(if on { "happ://connect" } else { "happ://disconnect" })?,
        _ => open_program(s.path.as_deref().unwrap_or_default())?,
    }
    Ok(s)
}

fn open_program(path: &str) -> AppResult<()> {
    #[cfg(windows)]
    {
        os::shell::shell_open(path, None, std::path::Path::new(path).parent().and_then(|p| p.to_str()))
    }
    #[cfg(not(windows))]
    {
        let ok = if path.ends_with(".desktop") {
            std::process::Command::new("gtk-launch").arg(path.trim_end_matches(".desktop")).spawn().is_ok()
        } else {
            std::process::Command::new(path).spawn().is_ok()
        };
        if ok {
            Ok(())
        } else {
            Err(AppError::system("не удалось открыть VPN-клиент"))
        }
    }
}

impl IndicatorsModule {
    pub fn kbd(&self) -> KbdState {
        self.kbd.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// Короткая вспышка в острове (VPN).
    fn flash(&self, st: &Shared, kind: &str, title: String, subtitle: Option<String>, ms: u64) {
        let gen = self.flash_gen.fetch_add(1, Ordering::SeqCst) + 1;
        st.events.set_activity(Activity {
            id: "flash".into(),
            kind: kind.into(),
            priority: Priority::High,
            title,
            subtitle,
            icon: None,
            progress: None,
            updated_at: now_ms(),
            started_at: None,
        });
        let st = st.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(Duration::from_millis(ms)).await;
            if st.indicators.flash_gen.load(Ordering::SeqCst) == gen {
                st.events.clear_activity("flash");
            }
        });
    }

    /// Смена раскладки или Caps Lock — короткое уведомление острова без звука
    /// (новое сразу заменяет предыдущее, в историю не сохраняется).
    fn kbd_notice(st: &Shared, title: String, body: &str) {
        st.events.dismiss_source("kbd");
        st.events.push(IslandNotification {
            stack: Vec::new(),
            count: 1,
            id: uuid::Uuid::new_v4().to_string(),
            kind: NotificationKind::Internal,
            source: "kbd".into(),
            app_name: "Клавиатура".into(),
            icon: None,
            title,
            body: body.into(),
            priority: Priority::High,
            timestamp: now_ms(),
            actions: vec![],
            open_target: None,
            duration_ms: 1600,
        });
    }

    fn set_airplane_activity(st: &Shared, on: bool) {
        if on {
            st.events.set_activity(Activity {
                id: "airplane".into(),
                kind: "airplane".into(),
                priority: Priority::Normal,
                title: "Режим полёта".into(),
                subtitle: None,
                icon: None,
                progress: None,
                updated_at: now_ms(),
                started_at: Some(now_ms()),
            });
        } else {
            st.events.clear_activity("airplane");
        }
    }

    pub fn set_running(&self, app: &AppHandle, st: &Shared, run: bool) {
        if run == self.task.is_running() {
            return;
        }
        if !run {
            self.task.stop();
            st.events.clear_activity("airplane");
            return;
        }
        let app = app.clone();
        let st = st.clone();
        self.task.start(async move {
            let mut last_kbd: Option<KbdState> = None;
            let mut last_air: Option<bool> = None;
            let mut last_vpn: Option<bool> = None;
            let mut tick: u64 = 0;
            loop {
                let cfg = st.settings();
                if cfg.keyboard.layout || cfg.keyboard.caps {
                    let k = tauri::async_runtime::spawn_blocking(|| KbdState { layout: os::keyboard::layout(), caps: os::keyboard::caps() }).await.unwrap_or_default();
                    if let Some(prev) = &last_kbd {
                        if cfg.keyboard.caps && k.caps != prev.caps {
                            Self::kbd_notice(&st, if k.caps { "Caps Lock включён".into() } else { "Caps Lock выключен".into() }, "");
                        } else if cfg.keyboard.layout && k.layout.is_some() && k.layout != prev.layout && prev.layout.is_some() {
                            Self::kbd_notice(&st, k.layout.clone().unwrap_or_default(), "Раскладка клавиатуры");
                        }
                    }
                    if last_kbd.as_ref() != Some(&k) {
                        *st.indicators.kbd.lock().unwrap_or_else(|p| p.into_inner()) = k.clone();
                        let _ = app.emit("island://kbd", &k);
                        last_kbd = Some(k);
                    }
                }
                if tick % 13 == 0 {
                    let air = tauri::async_runtime::spawn_blocking(os::network::airplane_on).await.ok().flatten();
                    if let Some(on) = air {
                        if last_air != Some(on) {
                            Self::set_airplane_activity(&st, on);
                            last_air = Some(on);
                        }
                    }
                    if cfg.vpn.client != "none" {
                        let st2 = st.clone();
                        let v = tauri::async_runtime::spawn_blocking(move || vpn_status(&st2)).await.unwrap_or_default();
                        if !v.client.is_empty() {
                            if let Some(prev) = last_vpn {
                                if prev != v.connected && cfg.vpn.notify {
                                    st.indicators.flash(&st, "vpn", if v.connected { "VPN подключён".into() } else { "VPN отключён".into() }, Some(v.name.clone()), 2200);
                                }
                            }
                            last_vpn = Some(v.connected);
                        }
                    }
                }
                tick = tick.wrapping_add(1);
                tokio::time::sleep(Duration::from_millis(150)).await;
            }
        });
    }
}
