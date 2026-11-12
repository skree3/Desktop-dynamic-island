//! Управление окнами: положение панели на мониторах, изменение размера при
//! раскрытии, отслеживание подключения/отключения мониторов, окно настроек.

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Manager, Monitor, PhysicalPosition, PhysicalSize, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::core::settings::Placement;
use crate::core::state::{AppState, Shared};
use crate::database::now_ms;
use crate::logging::category;

pub const ISLAND: &str = "island";
pub const SETTINGS: &str = "settings";

static IGNORE_MOVES_UNTIL: AtomicI64 = AtomicI64::new(0);
static MOVE_GEN: AtomicI64 = AtomicI64::new(0);
static LAST_SIGNATURE: Mutex<String> = Mutex::new(String::new());

pub fn island(app: &AppHandle) -> Option<WebviewWindow> {
    app.get_webview_window(ISLAND)
}

pub fn monitor_key(m: &Monitor) -> String {
    match m.name() {
        Some(n) if !n.is_empty() => n.clone(),
        _ => format!("{}x{}@{},{}", m.size().width, m.size().height, m.position().x, m.position().y),
    }
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorInfo {
    pub key: String,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub scale: f64,
    pub primary: bool,
    pub current: bool,
}

pub fn list_monitors(app: &AppHandle) -> Vec<MonitorInfo> {
    let primary = app.primary_monitor().ok().flatten().map(|m| monitor_key(&m));
    let current = island(app).and_then(|w| w.current_monitor().ok().flatten()).map(|m| monitor_key(&m));
    app.available_monitors()
        .unwrap_or_default()
        .iter()
        .enumerate()
        .map(|(i, m)| {
            let key = monitor_key(m);
            MonitorInfo {
                // Windows отдаёт технические имена вида \\.\DISPLAY1 — показываем «Монитор 1».
                name: m
                    .name()
                    .cloned()
                    .filter(|n| !n.is_empty() && !n.starts_with(r"\\.\"))
                    .unwrap_or_else(|| format!("Монитор {}", i + 1)),
                width: m.size().width,
                height: m.size().height,
                x: m.position().x,
                y: m.position().y,
                scale: m.scale_factor(),
                primary: primary.as_deref() == Some(key.as_str()),
                current: current.as_deref() == Some(key.as_str()),
                key,
            }
        })
        .collect()
}

/// Монитор из настроек; если он отключён — основной; если и его нет — первый.
fn target_monitor(app: &AppHandle, st: &AppState) -> Option<Monitor> {
    let wanted = st.with_settings(|s| s.position.monitor.clone());
    let all = app.available_monitors().unwrap_or_default();
    if let Some(w) = wanted {
        if let Some(m) = all.iter().find(|m| monitor_key(m) == w) {
            return Some(m.clone());
        }
    }
    app.primary_monitor().ok().flatten().or_else(|| all.into_iter().next())
}

fn set_bounds(win: &WebviewWindow, x: i32, y: i32, w: u32, h: u32) {
    IGNORE_MOVES_UNTIL.store(now_ms() + 400, Ordering::SeqCst);
    if !crate::platform::os::window::set_bounds(win, x, y, w, h) {
        let _ = win.set_size(PhysicalSize::new(w, h));
        let _ = win.set_position(PhysicalPosition::new(x, y));
    }
}

fn compute(m: &Monitor, p: Placement, w: u32, h: u32) -> (i32, i32) {
    let (mx, my) = (m.position().x as f64, m.position().y as f64);
    let (mw, mh) = (m.size().width as f64, m.size().height as f64);
    let scale = m.scale_factor();
    let x = mx + p.rel_x * mw - w as f64 / 2.0;
    let y = my + p.top * scale;
    let x = x.clamp(mx, (mx + mw - w as f64).max(mx));
    let y = y.clamp(my, (my + mh - h as f64).max(my));
    (x.round() as i32, y.round() as i32)
}

pub fn apply_position(app: &AppHandle, st: &AppState) {
    let Some(win) = island(app) else { return };
    let Some(m) = target_monitor(app, st) else { return };
    let key = monitor_key(&m);
    let placement = st.with_settings(|s| s.position.placements.get(&key).copied().unwrap_or_default());
    let size = win.outer_size().unwrap_or(PhysicalSize::new(240, 48));
    let (x, y) = compute(&m, placement, size.width, size.height);
    set_bounds(&win, x, y, size.width, size.height);
    emit_attached(app, y <= m.position().y);
}

static ATTACHED: AtomicBool = AtomicBool::new(true);
/// Положение сохраняется после перетаскивания — не нужно повторно двигать окно.
static SKIP_APPLY: AtomicBool = AtomicBool::new(false);

pub fn take_skip_apply() -> bool {
    SKIP_APPLY.swap(false, Ordering::SeqCst)
}

/// «Прилипла» ли панель к верхнему краю монитора (форма «чёлки» с вогнутыми углами).
pub fn is_attached() -> bool {
    ATTACHED.load(Ordering::SeqCst)
}

fn emit_attached(app: &AppHandle, attached: bool) {
    ATTACHED.store(attached, Ordering::SeqCst);
    let _ = tauri::Emitter::emit_to(app, ISLAND, "island://attached", attached);
}

/// Возвращает панель сверху по центру того монитора, где она сейчас находится.
pub fn center_island(app: &AppHandle, st: &Shared) {
    let current = island(app).and_then(|w| w.current_monitor().ok().flatten()).map(|m| monitor_key(&m));
    let _ = st.update_settings(app, |s| {
        let key = current.clone().or_else(|| s.position.monitor.clone());
        if let Some(k) = key {
            s.position.monitor = Some(k.clone());
            s.position.placements.insert(k, Placement::default());
        } else {
            s.position.placements.clear();
        }
        Ok(())
    });
    apply_position(app, st);
}

/// Изменение размера окна панели с сохранением точки привязки (центр сверху).
pub fn resize_island(app: &AppHandle, st: &AppState, logical_w: f64, logical_h: f64) {
    let Some(win) = island(app) else { return };
    let m = win.current_monitor().ok().flatten().or_else(|| target_monitor(app, st));
    let scale = m.as_ref().map(|m| m.scale_factor()).unwrap_or(1.0);
    let w = (logical_w.clamp(80.0, 1200.0) * scale).round() as u32;
    let h = (logical_h.clamp(24.0, 1200.0) * scale).round() as u32;
    let pos = win.outer_position().unwrap_or_default();
    let size = win.outer_size().unwrap_or_default();
    let center_x = pos.x as f64 + size.width as f64 / 2.0;
    let mut x = (center_x - w as f64 / 2.0).round() as i32;
    let mut y = pos.y;
    if let Some(m) = m {
        let (mx, my) = (m.position().x, m.position().y);
        let (mw, mh) = (m.size().width as i32, m.size().height as i32);
        x = x.clamp(mx, (mx + mw - w as i32).max(mx));
        y = y.clamp(my, (my + mh - h as i32).max(my));
    }
    set_bounds(&win, x, y, w, h);
}

/// Вызывается при перемещении окна пользователем: сохраняет положение
/// (с задержкой, чтобы не писать в базу на каждый пиксель).
pub fn on_island_moved(app: &AppHandle) {
    if now_ms() < IGNORE_MOVES_UNTIL.load(Ordering::SeqCst) {
        return;
    }
    let gen = MOVE_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        if MOVE_GEN.load(Ordering::SeqCst) != gen {
            return;
        }
        save_current_placement(&app);
    });
}

fn save_current_placement(app: &AppHandle) {
    let st: Shared = crate::core::state::shared(app);
    if st.with_settings(|s| s.position.locked) {
        apply_position(app, &st);
        return;
    }
    let Some(win) = island(app) else { return };
    let (Ok(pos), Ok(size)) = (win.outer_position(), win.outer_size()) else { return };
    let cx = pos.x + size.width as i32 / 2;
    let monitors = app.available_monitors().unwrap_or_default();
    let Some(m) = monitors.iter().find(|m| {
        let p = m.position();
        let s = m.size();
        cx >= p.x && cx < p.x + s.width as i32 && pos.y >= p.y - 50 && pos.y < p.y + s.height as i32
    }) else {
        return;
    };
    let key = monitor_key(m);
    let mut top = ((pos.y - m.position().y) as f64 / m.scale_factor()).max(0.0);
    // «Магнит»: рядом с верхним краем панель прилипает к нему.
    let snap = top < 14.0;
    if snap {
        top = 0.0;
    }
    let placement = Placement { rel_x: ((cx - m.position().x) as f64 / m.size().width as f64).clamp(0.0, 1.0), top };
    SKIP_APPLY.store(true, Ordering::SeqCst);
    let _ = st.update_settings(app, |s| {
        s.position.monitor = Some(key.clone());
        s.position.placements.insert(key, placement);
        Ok(())
    });
    SKIP_APPLY.store(false, Ordering::SeqCst);
    if snap {
        apply_position(app, &st);
    } else {
        emit_attached(app, false);
    }
}

pub fn move_to_monitor(app: &AppHandle, st: &Shared, key: Option<String>) {
    let _ = st.update_settings(app, |s| {
        s.position.monitor = key;
        Ok(())
    });
    apply_position(app, st);
}

pub fn reset_position(app: &AppHandle, st: &Shared) {
    let _ = st.update_settings(app, |s| {
        let key = s.position.monitor.clone();
        match key {
            Some(k) => {
                s.position.placements.insert(k, Placement::default());
            }
            None => s.position.placements.clear(),
        }
        Ok(())
    });
    apply_position(app, st);
}

fn signature(app: &AppHandle) -> String {
    app.available_monitors()
        .unwrap_or_default()
        .iter()
        .map(|m| format!("{}:{}x{}@{},{}*{}", monitor_key(m), m.size().width, m.size().height, m.position().x, m.position().y, m.scale_factor()))
        .collect::<Vec<_>>()
        .join("|")
}

/// Раз в 3 секунды сравнивает конфигурацию мониторов. При изменении
/// (отключение, подключение, смена разрешения) — восстанавливает положение.
pub fn start_monitor_watch(app: &AppHandle, st: &Shared) {
    *LAST_SIGNATURE.lock().unwrap_or_else(|p| p.into_inner()) = signature(app);
    let app = app.clone();
    let st2 = st.clone();
    st.monitor_watch.start(async move {
        loop {
            tokio::time::sleep(Duration::from_secs(3)).await;
            let sig = signature(&app);
            let changed = {
                let mut g = LAST_SIGNATURE.lock().unwrap_or_else(|p| p.into_inner());
                if *g != sig {
                    *g = sig;
                    true
                } else {
                    false
                }
            };
            if changed {
                st2.log.info(category::APP, "monitors", "конфигурация мониторов изменилась — положение панели восстановлено");
                apply_position(&app, &st2);
                st2.events.emit("monitors://changed", list_monitors(&app));
            }
        }
    });
}

pub fn show_island(app: &AppHandle, st: &AppState) {
    if let Some(w) = island(app) {
        let _ = w.show();
        let _ = w.set_always_on_top(true);
    }
    st.flags.island_visible.store(true, Ordering::SeqCst);
}

pub fn hide_island(app: &AppHandle, st: &AppState) {
    if let Some(w) = island(app) {
        let _ = w.hide();
    }
    st.flags.island_visible.store(false, Ordering::SeqCst);
}

/// Итоговая видимость панели = не скрыта пользователем, не временно, не игровой режим.
pub fn refresh_visibility(app: &AppHandle, st: &Shared) {
    let hidden_until = st.flags.hidden_until.load(Ordering::SeqCst);
    let temp_hidden = hidden_until > now_ms();
    let should_show = !st.flags.user_hidden.load(Ordering::SeqCst) && !temp_hidden && !st.game_mode_active();
    if should_show != st.island_visible() {
        if should_show {
            show_island(app, st);
        } else {
            hide_island(app, st);
            if st.with_settings(|s| s.performance.close_ai_on_hide) {
                st.ai.close_all(app);
            }
        }
        crate::modules::reconcile(app, st);
    }
    crate::tray::refresh(app);
}

pub fn hide_temporarily(app: &AppHandle, st: &Shared, minutes: u64) {
    let until = now_ms() + minutes as i64 * 60_000;
    st.flags.hidden_until.store(until, Ordering::SeqCst);
    refresh_visibility(app, st);
    let app = app.clone();
    let st = st.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(minutes * 60)).await;
        if st.flags.hidden_until.load(Ordering::SeqCst) == until {
            st.flags.hidden_until.store(0, Ordering::SeqCst);
            refresh_visibility(&app, &st);
        }
    });
}

pub fn toggle_island(app: &AppHandle, st: &Shared) {
    let hidden = st.flags.user_hidden.load(Ordering::SeqCst) || st.flags.hidden_until.load(Ordering::SeqCst) > now_ms();
    st.flags.user_hidden.store(!hidden, Ordering::SeqCst);
    st.flags.hidden_until.store(0, Ordering::SeqCst);
    refresh_visibility(app, st);
}

/// Аргументы WebView2 по умолчанию в Tauri + опциональная программная отрисовка.
/// Окна с общим каталогом данных обязаны использовать одинаковые аргументы,
/// поэтому значение фиксируется при запуске и применяется к панели и настройкам.
static BROWSER_ARGS: std::sync::OnceLock<String> = std::sync::OnceLock::new();

#[cfg_attr(not(windows), allow(dead_code))]
fn browser_args() -> &'static str {
    BROWSER_ARGS.get().map(|s| s.as_str()).unwrap_or(DEFAULT_ARGS)
}

const DEFAULT_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection";

/// Создаёт окно панели. Программная отрисовка (без GPU-композитинга) экономит
/// ~100 МБ памяти GPU-процесса WebView2 для маленького анимированного окна
/// (см. README, раздел «Производительность»).
pub fn create_island(app: &AppHandle, software_rendering: bool) -> tauri::Result<()> {
    let args = if software_rendering { format!("{DEFAULT_ARGS} --disable-gpu --disable-gpu-compositing") } else { DEFAULT_ARGS.to_string() };
    let _ = BROWSER_ARGS.set(args);
    let builder = WebviewWindowBuilder::new(app, ISLAND, WebviewUrl::App("index.html?window=island".into()))
        .title("Desktop Island")
        .inner_size(240.0, 52.0)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .resizable(false)
        .maximizable(false)
        .minimizable(false)
        .shadow(false)
        .visible(false)
        .focused(false);
    #[cfg(windows)]
    let builder = builder.additional_browser_args(browser_args());
    let win = builder.build()?;
    crate::platform::os::window::prepare_overlay(&win);
    Ok(())
}

/// Окно настроек создаётся по требованию и уничтожается при закрытии —
/// это экономит память (отдельный WebView не висит в фоне).
pub fn open_settings(app: &AppHandle, section: Option<&str>) {
    if let Some(w) = app.get_webview_window(SETTINGS) {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        if let Some(s) = section {
            let _ = tauri::Emitter::emit_to(app, SETTINGS, "settings://navigate", s.to_string());
        }
        return;
    }
    let url = match section {
        Some(s) if s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') => format!("index.html?window=settings&section={s}"),
        _ => "index.html?window=settings".into(),
    };
    let builder = WebviewWindowBuilder::new(app, SETTINGS, WebviewUrl::App(url.into()))
        .title("Desktop Island — Настройки")
        .inner_size(1000.0, 700.0)
        .min_inner_size(780.0, 520.0)
        .center()
        .resizable(true)
        .decorations(true)
        .visible(true);
    #[cfg(windows)]
    let builder = builder.additional_browser_args(browser_args());
    let res = builder.build();
    if let Err(e) = res {
        tracing::error!(error = %e, "не удалось открыть окно настроек");
    }
}
