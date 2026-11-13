//! Режим «Не беспокоить» Windows (в Windows 10 — «Фокусировка внимания»).
//! Пока он включён, Windows не показывает всплывающие баннеры и не играет звук
//! уведомлений, но сами уведомления по-прежнему попадают в центр уведомлений —
//! оттуда их читает остров. Состояние хранится в WNF-канале
//! WNF_SHEL_QUIETHOURS_ACTIVE_PROFILE_CHANGED: 0 — выключен, 1 — только
//! приоритетные, 2 — только будильники.

use std::ffi::c_void;

#[link(name = "ntdll")]
extern "system" {
    fn NtQueryWnfStateData(name: *const u64, type_id: *const c_void, scope: *const c_void, stamp: *mut u32, buf: *mut c_void, size: *mut u32) -> i32;
    fn NtUpdateWnfStateData(name: *const u64, buf: *const c_void, size: u32, type_id: *const c_void, scope: *const c_void, match_stamp: u32, check_stamp: u32) -> i32;
}

const QUIET_HOURS: u64 = 0x0D83_063E_A3BF_1C75;

/// Только будильники — самый тихий режим.
pub const ALARMS_ONLY: u32 = 2;

/// Текущий режим; `None`, если канал недоступен (старые сборки Windows).
pub fn get() -> Option<u32> {
    let mut stamp = 0u32;
    let mut buf = [0u8; 16];
    let mut size = buf.len() as u32;
    let status = unsafe { NtQueryWnfStateData(&QUIET_HOURS, std::ptr::null(), std::ptr::null(), &mut stamp, buf.as_mut_ptr().cast(), &mut size) };
    if status < 0 {
        return None;
    }
    if size < 4 {
        return Some(0);
    }
    Some(u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]))
}

pub fn set(value: u32) -> bool {
    let data = value.to_le_bytes();
    let status = unsafe { NtUpdateWnfStateData(&QUIET_HOURS, data.as_ptr().cast(), 4, std::ptr::null(), std::ptr::null(), 0, 0) };
    status >= 0
}
