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

/// Фронтенд сообщил о первой отрисовке (команда island_ready).
pub static ISLAND_READY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Страховка: если фронтенд за несколько секунд не сообщил о готовности
/// (например, WebKitGTK не отрисовывает скрытое окно), показываем окно сами —
/// иначе приложение выглядит так, будто не запустилось.
pub fn show_if_frontend_silent(app: &AppHandle, st: &Shared) {
    let (app, st) = (app.clone(), st.clone());
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(4)).await;
        if ISLAND_READY.swap(true, Ordering::SeqCst) {
            return;
        }
        st.log.warn(category::APP, "window", "интерфейс не сообщил о готовности за 4 с — окно панели показано принудительно");
        if !st.flags.user_hidden.load(Ordering::SeqCst) && !st.game_mode_active() {
            apply_position(&app, &st);
            show_island(&app, &st);
            crate::modules::reconcile(&app, &st);
        }
        crate::tray::refresh(&app);
    });
}

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
    IGNORE_MOVES_UNTIL.fetch_max(now_ms() + 400, Ordering::SeqCst);
    if !crate::platform::os::window::set_bounds(win, x, y, w, h) {
        // GTK не принимает нулевой размер (gtk_window_resize: assertion 'width > 0').
        if w > 0 && h > 0 {
            let _ = win.set_size(PhysicalSize::new(w, h));
        }
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
    let (mut placement, style) = st.with_settings(|s| (s.position.placements.get(&key).copied().unwrap_or_default(), s.position.style.clone()));
    // «У края» — остров вырастает из верхнего края; «плавающий» — висит чуть ниже.
    if style == "edge" {
        placement.top = 0.0;
    } else if placement.top < 4.0 {
        placement.top = crate::core::settings::FLOAT_TOP;
    }
    // В Linux до первой отрисовки окна outer_size() возвращает 0×0.
    let scale = m.scale_factor();
    let last = *LAST_LOGICAL.lock().unwrap_or_else(|p| p.into_inner());
    let size = match last {
        // Размер, который задал интерфейс, точнее outer_size(): в GTK тот бывает завышен.
        Some((lw, lh)) => PhysicalSize::new((lw * scale).round() as u32, (lh * scale).round() as u32),
        None => win
            .outer_size()
            .ok()
            .filter(|s| s.width > 0 && s.height > 0)
            .unwrap_or_else(|| PhysicalSize::new((240.0 * scale).round() as u32, (52.0 * scale).round() as u32)),
    };
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
/// Последний размер острова, заданный интерфейсом (логические пиксели).
static LAST_LOGICAL: Mutex<Option<(f64, f64)>> = Mutex::new(None);

pub fn resize_island(app: &AppHandle, st: &AppState, logical_w: f64, logical_h: f64) {
    *LAST_LOGICAL.lock().unwrap_or_else(|p| p.into_inner()) = Some((logical_w, logical_h));
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
    // «Магнит»: рядом с верхним краем панель встаёт на своё место по стилю
    // (вплотную к краю или на стандартном отступе), в стиле «у края» — всегда к краю.
    let edge = st.with_settings(|s| s.position.style == "edge");
    let snap = edge || top < 18.0;
    if snap {
        top = if edge { 0.0 } else { crate::core::settings::FLOAT_TOP };
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

/// Размер самого острова внутри окна (логические пиксели). Окно острова теперь
/// постоянного размера — при раскрытии меняется только форма внутри, поэтому
/// WebView не перестраивается на каждом кадре и остров не дёргается. Всё вне
/// острова пропускает клики насквозь.
static HITBOX: Mutex<(f64, f64)> = Mutex::new((240.0, 40.0));

/// Экранный прямоугольник острова (физические пиксели): x, y, ширина, высота.
pub fn island_rect(app: &AppHandle) -> Option<(i32, i32, i32, i32)> {
    let win = island(app)?;
    let pos = win.outer_position().ok()?;
    let size = win.outer_size().ok()?;
    let scale = win.scale_factor().unwrap_or(1.0);
    let (w, h) = *HITBOX.lock().unwrap_or_else(|p| p.into_inner());
    let (w, h) = ((w * scale).round() as i32, (h * scale).round() as i32);
    let w = w.min(size.width as i32);
    Some((pos.x + (size.width as i32 - w) / 2, pos.y, w, h.min(size.height as i32)))
}

pub fn set_hitbox(app: &AppHandle, logical_w: f64, logical_h: f64) {
    *HITBOX.lock().unwrap_or_else(|p| p.into_inner()) = (logical_w.clamp(40.0, 1200.0), logical_h.clamp(16.0, 1200.0));
    #[cfg(target_os = "linux")]
    apply_input_shape(app);
    #[cfg(not(target_os = "linux"))]
    let _ = app;
}

/// Linux: окно принимает мышь только в области острова (input shape GTK) —
/// работает и в X11, и в Wayland, где положение курсора узнать нельзя.
#[cfg(target_os = "linux")]
fn apply_input_shape(app: &AppHandle) {
    let Some(win) = island(app) else { return };
    let (w, h) = *HITBOX.lock().unwrap_or_else(|p| p.into_inner());
    let win2 = win.clone();
    let _ = win.run_on_main_thread(move || {
        use gtk::prelude::*;
        let Ok(gw) = win2.gtk_window() else { return };
        let Some(gdk) = gw.window() else { return };
        let width = gw.allocated_width();
        let (w, h) = ((w as i32).min(width), h as i32);
        let rect = gtk::cairo::RectangleInt::new((width - w) / 2, 0, w, h + 2);
        let region = gtk::cairo::Region::create_rectangle(&rect);
        gdk.input_shape_combine_region(&region, 0, 0);
    });
}

/// Windows: курсор над островом — окно принимает мышь, вне острова — клики
/// проходят к окнам под ним. Проверка 40 раз в секунду, переключение только
/// при пересечении границы.
#[cfg(windows)]
pub fn start_hit_test(app: &AppHandle) {
    let app = app.clone();
    std::thread::Builder::new()
        .name("island-hit-test".into())
        .spawn(move || {
            let mut through: Option<bool> = None;
            loop {
                std::thread::sleep(Duration::from_millis(25));
                let Some(win) = island(&app) else { continue };
                if !win.is_visible().unwrap_or(false) {
                    through = None;
                    continue;
                }
                let (Some((x, y, w, h)), Some(p)) = (island_rect(&app), crate::platform::os::dragdrop::cursor()) else { continue };
                let inside = p.x >= x - 2 && p.x < x + w + 2 && p.y >= y - 2 && p.y < y + h + 2;
                // Пока зажата кнопка мыши (перетаскивание острова), состояние не меняем.
                if crate::platform::os::dragdrop::primary_button_down() && through == Some(false) {
                    continue;
                }
                if through != Some(!inside) {
                    through = Some(!inside);
                    let _ = win.set_ignore_cursor_events(!inside);
                }
            }
        })
        .ok();
}

/// После входа в систему оболочка (Проводник, DWM, драйвер видео) ещё
/// запускается и может «потерять» остров: окно уже показано, но не видно.
/// Несколько раз в первые секунды возвращаем его на место и заставляем
/// перерисоваться. Заодно проверяется, что остров действительно показан.
pub fn refresh_after_boot(app: &AppHandle, st: &Shared) {
    let (app, st) = (app.clone(), st.clone());
    tauri::async_runtime::spawn(async move {
        for secs in [3u64, 6, 12, 25] {
            tokio::time::sleep(Duration::from_secs(secs)).await;
            if st.flags.user_hidden.load(Ordering::SeqCst) || st.game_mode_active() {
                continue;
            }
            apply_position(&app, &st);
            show_island(&app, &st);
            let last = *LAST_LOGICAL.lock().unwrap_or_else(|p| p.into_inner());
            if let Some((w, h)) = last {
                resize_island(&app, &st, w, h + 1.0);
                tokio::time::sleep(Duration::from_millis(80)).await;
                resize_island(&app, &st, w, h);
            }
        }
    });
}

pub fn show_island(app: &AppHandle, st: &AppState) {
    // Менеджер окон Linux при показе может сам переставить окно — это не
    // перетаскивание пользователем, такую позицию запоминать нельзя.
    IGNORE_MOVES_UNTIL.store(now_ms() + 1500, Ordering::SeqCst);
    if let Some(w) = island(app) {
        let _ = w.show();
        let _ = w.set_always_on_top(true);
    }
    st.flags.island_visible.store(true, Ordering::SeqCst);
    #[cfg(target_os = "linux")]
    kick_after_show(app);
}

/// Linux: после показа окна WebKitGTK иногда не рисует уже готовую страницу
/// (окно остаётся пустым до следующего изменения размера), а менеджер окон
/// может поставить окно в другое место. Слегка меняем размер и возвращаем
/// остров на его позицию.
#[cfg(target_os = "linux")]
fn kick_after_show(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_millis(150)).await;
        let st = crate::core::state::shared(&app);
        let last = *LAST_LOGICAL.lock().unwrap_or_else(|p| p.into_inner());
        apply_position(&app, &st);
        if let Some((w, h)) = last {
            resize_island(&app, &st, w + 1.0, h);
            tokio::time::sleep(Duration::from_millis(60)).await;
            resize_island(&app, &st, w, h);
        }
    });
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
    // Windows: встроенный обработчик wry при создании снимает (RevokeDragDrop) цели
    // сброса и с окон Chromium из процесса WebView2 (межпроцессный Revoke срабатывает),
    // а свою поставить туда не может. Если эти окна уже созданы, файлы на остров
    // перестают приниматься совсем. Поэтому обработчик wry отключён, а файлы принимает
    // наша цель (platform::windows::dragdrop), которой WebView2 передаёт сброс сам.
    #[cfg(windows)]
    let builder = builder.additional_browser_args(browser_args()).disable_drag_drop_handler();
    let win = builder.build()?;
    crate::platform::os::window::prepare_overlay(&win);
    #[cfg(windows)]
    let _ = win.with_webview(|w| unsafe {
        use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Controller4;
        use windows_core_wv2::Interface;
        // Сброс файлов не обрабатывается страницей, а передаётся окну-хозяину.
        if let Ok(c) = w.controller().cast::<ICoreWebView2Controller4>() {
            let _ = c.SetAllowExternalDrop(false);
        }
    });
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
        // Прозрачность и эффекты (Mica/Acrylic) окна настроек — по настройкам «Внешний вид».
        .transparent(true)
        .visible(true);
    #[cfg(windows)]
    let builder = builder.additional_browser_args(browser_args());
    match builder.build() {
        Ok(w) => apply_settings_effect(&w, &crate::core::state::shared(app)),
        Err(e) => tracing::error!(error = %e, "не удалось открыть окно настроек"),
    }
}

/// Эффект под окном настроек (Windows 11; в Linux прозрачность даёт композитор).
pub fn apply_settings_effect(w: &WebviewWindow, st: &AppState) {
    #[cfg(windows)]
    {
        use tauri::window::{Effect, EffectsBuilder};
        let (effect, opacity) = st.with_settings(|s| (s.appearance.settings_effect.clone(), s.appearance.settings_opacity));
        let effects = match effect.as_str() {
            "mica" if opacity < 1.0 => Some(EffectsBuilder::new().effect(Effect::Mica).build()),
            "acrylic" if opacity < 1.0 => Some(EffectsBuilder::new().effect(Effect::Acrylic).build()),
            _ => None,
        };
        let _ = w.set_effects(effects);
    }
    #[cfg(not(windows))]
    let _ = (w, st);
}

/// Экономия памяти, пока остров свёрнут: WebView2 переводится в режим низкого
/// потребления (сбрасывает кэши отрисовки и сжимает кучу), а рабочий набор
/// процесса отдаётся системе. При раскрытии режим сразу возвращается в обычный.
static MEMORY_GEN: AtomicI64 = AtomicI64::new(0);

pub fn island_memory_saver(app: &AppHandle, collapsed: bool) {
    let gen = MEMORY_GEN.fetch_add(1, Ordering::SeqCst) + 1;
    if !collapsed {
        set_memory_low(app, false);
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // Свёрнут ненадолго (листают карточки, пришло уведомление) — ничего не делаем.
        tokio::time::sleep(Duration::from_secs(20)).await;
        if MEMORY_GEN.load(Ordering::SeqCst) == gen {
            set_memory_low(&app, true);
        }
    });
}

fn set_memory_low(app: &AppHandle, low: bool) {
    #[cfg(windows)]
    {
        let Some(win) = app.get_webview_window(ISLAND) else { return };
        let _ = win.with_webview(move |w| unsafe {
            use webview2_com::Microsoft::Web::WebView2::Win32::{ICoreWebView2_19, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL};
            use windows_core_wv2::Interface;
            if let Ok(core) = w.controller().CoreWebView2() {
                if let Ok(c19) = core.cast::<ICoreWebView2_19>() {
                    let _ = c19.SetMemoryUsageTargetLevel(if low { COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW } else { COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL });
                }
            }
        });
        if low {
            unsafe {
                let _ = windows::Win32::System::ProcessStatus::EmptyWorkingSet(windows::Win32::System::Threading::GetCurrentProcess());
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (app, low);
    }
}
