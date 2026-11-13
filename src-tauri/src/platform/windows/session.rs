//! События сеанса Windows для приветствия: разблокировка, пробуждение,
//! включение экрана. Окно-получатель сообщений живёт в своём потоке.

use std::sync::OnceLock;

use windows::core::{w, GUID};
use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::Power::{RegisterPowerSettingNotification, POWERBROADCAST_SETTING};
use windows::Win32::System::RemoteDesktop::{WTSRegisterSessionNotification, NOTIFY_FOR_THIS_SESSION};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetMessageW, RegisterClassW, TranslateMessage, DEVICE_NOTIFY_WINDOW_HANDLE, HWND_MESSAGE, MSG,
    PBT_APMRESUMEAUTOMATIC, PBT_POWERSETTINGCHANGE, WINDOW_EX_STYLE, WINDOW_STYLE, WM_POWERBROADCAST, WM_WTSSESSION_CHANGE, WNDCLASSW,
};

const WTS_SESSION_UNLOCK: usize = 0x8;
/// GUID_CONSOLE_DISPLAY_STATE: 0 — экран выключен, 1 — включён, 2 — затемнён.
const GUID_CONSOLE_DISPLAY_STATE: GUID = GUID::from_u128(0x6fe69556_704a_47a0_8f24_c28d936fda47);

type Callback = Box<dyn Fn(&'static str) + Send + Sync>;
static CALLBACK: OnceLock<Callback> = OnceLock::new();

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let fire = |reason: &'static str| {
        if let Some(cb) = CALLBACK.get() {
            cb(reason);
        }
    };
    match msg {
        WM_WTSSESSION_CHANGE if wparam.0 == WTS_SESSION_UNLOCK => fire("unlock"),
        WM_POWERBROADCAST if wparam.0 == PBT_APMRESUMEAUTOMATIC as usize => fire("resume"),
        WM_POWERBROADCAST if wparam.0 == PBT_POWERSETTINGCHANGE as usize && lparam.0 != 0 => {
            let s = &*(lparam.0 as *const POWERBROADCAST_SETTING);
            if s.PowerSetting == GUID_CONSOLE_DISPLAY_STATE && s.DataLength >= 1 && s.Data[0] == 1 {
                fire("display");
            }
        }
        _ => {}
    }
    DefWindowProcW(hwnd, msg, wparam, lparam)
}

/// Запускает наблюдение (один раз за время работы программы).
pub fn watch(cb: impl Fn(&'static str) + Send + Sync + 'static) {
    if CALLBACK.set(Box::new(cb)).is_err() {
        return;
    }
    std::thread::Builder::new()
        .name("session-watch".into())
        .spawn(|| unsafe {
            let class = w!("DesktopIslandSessionWatch");
            let wc = WNDCLASSW { lpfnWndProc: Some(proc), lpszClassName: class, ..Default::default() };
            RegisterClassW(&wc);
            let Ok(hwnd) = CreateWindowExW(WINDOW_EX_STYLE(0), class, w!(""), WINDOW_STYLE(0), 0, 0, 0, 0, Some(HWND_MESSAGE), None, None, None) else {
                return;
            };
            let _ = WTSRegisterSessionNotification(hwnd, NOTIFY_FOR_THIS_SESSION);
            let _ = RegisterPowerSettingNotification(HANDLE(hwnd.0), &GUID_CONSOLE_DISPLAY_STATE, DEVICE_NOTIFY_WINDOW_HANDLE);
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        })
        .ok();
}
