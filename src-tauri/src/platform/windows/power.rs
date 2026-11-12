//! Питание: GetSystemPowerStatus (батарея, сеть, экономия заряда) и
//! ExitWindowsEx для выключения/перезагрузки (без EWX_FORCE — приложения
//! могут сохранить данные или отменить завершение работы).

use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_NOT_ALL_ASSIGNED, HANDLE, LUID};
use windows::Win32::Security::{
    AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES, SE_PRIVILEGE_ENABLED, SE_SHUTDOWN_NAME, TOKEN_ADJUST_PRIVILEGES,
    TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
use windows::Win32::System::Shutdown::{
    ExitWindowsEx, EWX_POWEROFF, EWX_REBOOT, EWX_SHUTDOWN, SHTDN_REASON_FLAG_PLANNED, SHTDN_REASON_MAJOR_OTHER,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

use crate::core::error::{AppError, AppResult};
use crate::platform::types::PowerState;

pub fn state() -> AppResult<PowerState> {
    let mut s = SYSTEM_POWER_STATUS::default();
    unsafe { GetSystemPowerStatus(&mut s)? };
    let has_battery = s.BatteryFlag != 128 && s.BatteryFlag != 255;
    Ok(PowerState {
        has_battery,
        percent: if has_battery && s.BatteryLifePercent <= 100 { Some(s.BatteryLifePercent) } else { None },
        charging: has_battery && (s.BatteryFlag & 8) != 0,
        on_ac: match s.ACLineStatus {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        },
        secs_remaining: if s.BatteryLifeTime != u32::MAX && has_battery { Some(s.BatteryLifeTime as u64) } else { None },
        saver: Some(s.SystemStatusFlag == 1),
        // Публичного API включения «Экономии заряда» нет — только чтение состояния.
        saver_can_toggle: false,
        saver_backend: Some("Windows Battery Saver (только чтение)".into()),
    })
}

pub fn set_saver(_on: bool) -> AppResult<()> {
    Err(AppError::unsupported(
        "Windows не предоставляет публичного API для переключения экономии заряда. Откройте «Параметры → Питание».",
    ))
}

fn enable_shutdown_privilege() -> AppResult<()> {
    unsafe {
        let mut token = HANDLE::default();
        OpenProcessToken(GetCurrentProcess(), TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY, &mut token)
            .map_err(|e| AppError::PermissionDenied(format!("не удалось открыть токен процесса: {}", e.message())))?;
        let mut luid = LUID::default();
        let res = LookupPrivilegeValueW(None, SE_SHUTDOWN_NAME, &mut luid).and_then(|_| {
            let tp = TOKEN_PRIVILEGES {
                PrivilegeCount: 1,
                Privileges: [LUID_AND_ATTRIBUTES { Luid: luid, Attributes: SE_PRIVILEGE_ENABLED }],
            };
            AdjustTokenPrivileges(token, false, Some(&tp), 0, None, None)
        });
        let last = GetLastError();
        let _ = CloseHandle(token);
        res.map_err(|e| AppError::PermissionDenied(format!("привилегия завершения работы недоступна: {}", e.message())))?;
        if last == ERROR_NOT_ALL_ASSIGNED {
            return Err(AppError::PermissionDenied("политика безопасности запрещает завершение работы этому пользователю".into()));
        }
    }
    Ok(())
}

pub fn shutdown() -> AppResult<()> {
    enable_shutdown_privilege()?;
    unsafe { ExitWindowsEx(EWX_SHUTDOWN | EWX_POWEROFF, SHTDN_REASON_MAJOR_OTHER | SHTDN_REASON_FLAG_PLANNED)? };
    Ok(())
}

pub fn reboot() -> AppResult<()> {
    enable_shutdown_privilege()?;
    unsafe { ExitWindowsEx(EWX_REBOOT, SHTDN_REASON_MAJOR_OTHER | SHTDN_REASON_FLAG_PLANNED)? };
    Ok(())
}
