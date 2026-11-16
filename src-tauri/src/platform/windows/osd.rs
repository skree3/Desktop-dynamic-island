//! Громкость и яркость в острове вместо плашек Windows.
//!
//! * Громкость: клавиши громкости зарегистрированы как глобальные сочетания —
//!   Windows их не обрабатывает и свою плашку не показывает, а громкость меняем
//!   сами (шаг 2%, как у системы).
//! * Яркость: клавиши яркости обрабатывает драйвер ноутбука, перехватить их
//!   нельзя. Изменение ловим уведомлением о параметре питания «Яркость дисплея»
//!   (оно приходит при любой смене яркости) — системная плашка при этом остаётся.

use std::sync::mpsc::Sender;
use std::sync::Mutex;
use windows::core::GUID;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{VK_VOLUME_DOWN, VK_VOLUME_MUTE, VK_VOLUME_UP};
use windows::Win32::UI::WindowsAndMessaging::*;

#[derive(Debug, Clone, Copy)]
pub enum VolumeKey {
    Up,
    Down,
    Mute,
}

static HOOK_THREAD: Mutex<Option<u32>> = Mutex::new(None);

/// Забирает клавиши громкости себе: они регистрируются как глобальные сочетания
/// (RegisterHotKey) в отдельном потоке с очередью сообщений. Пока сочетание занято
/// нами, Windows клавишу не обрабатывает — громкость не меняется сама и плашка
/// не появляется. Удержание клавиши даёт повторы (без MOD_NOREPEAT).
pub fn start_volume_keys(tx: Sender<VolumeKey>) {
    use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS};
    stop_volume_keys();
    std::thread::Builder::new()
        .name("volume-keys".into())
        .spawn(move || unsafe {
            let keys = [(1, VK_VOLUME_UP, VolumeKey::Up), (2, VK_VOLUME_DOWN, VolumeKey::Down), (3, VK_VOLUME_MUTE, VolumeKey::Mute)];
            let mut ok = 0;
            for (id, vk, _) in keys {
                if RegisterHotKey(None, id, HOT_KEY_MODIFIERS(0), vk.0 as u32).is_ok() {
                    ok += 1;
                } else {
                    tracing::warn!(key = vk.0, "клавиша громкости занята другой программой");
                }
            }
            if ok == 0 {
                return;
            }
            *HOOK_THREAD.lock().unwrap_or_else(|p| p.into_inner()) = Some(GetCurrentThreadId());
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                if msg.message == WM_HOTKEY {
                    if let Some((_, _, k)) = keys.iter().find(|(id, _, _)| *id as usize == msg.wParam.0) {
                        let _ = tx.send(*k);
                    }
                }
            }
            for (id, _, _) in keys {
                let _ = UnregisterHotKey(None, id);
            }
        })
        .ok();
}

pub fn stop_volume_keys() {
    if let Some(tid) = HOOK_THREAD.lock().unwrap_or_else(|p| p.into_inner()).take() {
        unsafe {
            let _ = PostThreadMessageW(tid, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }
}

// ---- яркость ----

/// «Яркость дисплея» (VIDEONORMALLEVEL) в группе «Экран».
const GUID_BRIGHTNESS: GUID = GUID::from_u128(0xaded5e82_b909_4619_9949_f5d71dac0bcb);

static BRIGHTNESS: Mutex<Option<(Sender<u32>, usize)>> = Mutex::new(None);

unsafe extern "system" fn brightness_cb(_ctx: *const core::ffi::c_void, kind: u32, setting: *const core::ffi::c_void) -> u32 {
    use windows::Win32::System::Power::POWERBROADCAST_SETTING;
    const PBT_POWERSETTINGCHANGE: u32 = 0x8013;
    if kind == PBT_POWERSETTINGCHANGE && !setting.is_null() {
        let s = &*(setting as *const POWERBROADCAST_SETTING);
        if s.PowerSetting == GUID_BRIGHTNESS && s.DataLength >= 4 {
            let v = std::ptr::read_unaligned(s.Data.as_ptr() as *const u32);
            if let Some((tx, _)) = BRIGHTNESS.lock().ok().and_then(|g| g.as_ref().map(|(t, h)| (t.clone(), *h))) {
                let _ = tx.send(v.min(100));
            }
        }
    }
    0
}

/// Подписка на смену яркости. Первое уведомление (текущее значение) приходит сразу.
pub fn start_brightness(tx: Sender<u32>) {
    use windows::Win32::System::Power::{PowerSettingRegisterNotification, DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS};
    use windows::Win32::UI::WindowsAndMessaging::DEVICE_NOTIFY_CALLBACK;
    stop_brightness();
    // Параметры подписки должны жить, пока она действует, — держим их всю жизнь процесса.
    let params: &'static mut DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS = Box::leak(Box::new(DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS { Callback: Some(brightness_cb), Context: std::ptr::null_mut() }));
    let mut handle: *mut core::ffi::c_void = std::ptr::null_mut();
    let rc = unsafe {
        PowerSettingRegisterNotification(&GUID_BRIGHTNESS, DEVICE_NOTIFY_CALLBACK, windows::Win32::Foundation::HANDLE(params as *mut _ as *mut _), &mut handle)
    };
    if rc.0 == 0 && !handle.is_null() {
        *BRIGHTNESS.lock().unwrap_or_else(|p| p.into_inner()) = Some((tx, handle as usize));
    } else {
        tracing::warn!(code = rc.0, "не удалось подписаться на изменения яркости");
    }
}

pub fn stop_brightness() {
    use windows::Win32::System::Power::{PowerSettingUnregisterNotification, HPOWERNOTIFY};
    if let Some((_, h)) = BRIGHTNESS.lock().unwrap_or_else(|p| p.into_inner()).take() {
        unsafe {
            let _ = PowerSettingUnregisterNotification(HPOWERNOTIFY(h as _));
        }
    }
}
