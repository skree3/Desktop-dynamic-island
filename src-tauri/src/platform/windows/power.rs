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
        // Включить можно только от батареи: порог «включать при заряде ниже» = 100 %.
        saver_can_toggle: has_battery && s.ACLineStatus == 0,
        saver_backend: Some(if has_battery && s.ACLineStatus == 0 {
            "Энергосбережение Windows".into()
        } else {
            "От сети энергосбережение включается в параметрах Windows".into()
        }),
    })
}

const SUB_ENERGYSAVER: windows::core::GUID = windows::core::GUID::from_u128(0xde830923_a562_41af_a086_e3a2c6bad2da);
const ESBATTTHRESHOLD: windows::core::GUID = windows::core::GUID::from_u128(0xe69653ca_cf7f_4f05_aa73_cb833fa90ad4);

fn saved_threshold_path() -> std::path::PathBuf {
    std::env::temp_dir().join("desktop-island-saver-threshold.txt")
}

/// Порог заряда, ниже которого Windows сама включает энергосбережение (от батареи).
fn threshold_rw(new: Option<u32>) -> AppResult<u32> {
    use windows::Win32::Foundation::{LocalFree, HLOCAL, WIN32_ERROR};
    use windows::Win32::System::Power::{PowerGetActiveScheme, PowerReadDCValueIndex, PowerSetActiveScheme, PowerWriteDCValueIndex};
    unsafe {
        let mut scheme: *mut windows::core::GUID = std::ptr::null_mut();
        if PowerGetActiveScheme(None, &mut scheme) != WIN32_ERROR(0) || scheme.is_null() {
            return Err(AppError::system("не удалось прочитать схему питания"));
        }
        let mut cur = 0u32;
        let r = PowerReadDCValueIndex(None, Some(scheme), Some(&SUB_ENERGYSAVER), Some(&ESBATTTHRESHOLD), &mut cur);
        let mut result = if r == 0 { Ok(cur) } else { Err(AppError::system("параметр энергосбережения недоступен")) };
        if let (Ok(_), Some(v)) = (&result, new) {
            let w = PowerWriteDCValueIndex(None, scheme, Some(&SUB_ENERGYSAVER), Some(&ESBATTTHRESHOLD), v);
            if w != 0 || PowerSetActiveScheme(None, Some(scheme)) != WIN32_ERROR(0) {
                result = Err(AppError::PermissionDenied("Windows не дал изменить энергосбережение".into()));
            }
        }
        let _ = LocalFree(Some(HLOCAL(scheme as _)));
        result
    }
}

pub fn set_saver(on: bool) -> AppResult<()> {
    let st = state()?;
    if !(st.has_battery && st.on_ac == Some(false)) {
        return Err(AppError::unsupported("от сети энергосбережение включается только в «Параметры → Питание»"));
    }
    let path = saved_threshold_path();
    if on {
        let cur = threshold_rw(None)?;
        if cur < 100 && !path.exists() {
            let _ = std::fs::write(&path, cur.to_string());
        }
        threshold_rw(Some(100))?;
    } else {
        let saved = std::fs::read_to_string(&path).ok().and_then(|s| s.trim().parse::<u32>().ok()).unwrap_or(20);
        // Если заряд уже ниже прежнего порога, Windows сразу включила бы режим снова.
        let pct = st.percent.unwrap_or(100) as u32;
        threshold_rw(Some(if pct <= saved { 0 } else { saved }))?;
        let _ = std::fs::remove_file(&path);
    }
    Ok(())
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
