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

/// Только приоритетные уведомления: Windows показывает баннеры лишь разрешённых приложений.
pub const PRIORITY_ONLY: u32 = 1;

/// Недокументированный COM «Фокусировки внимания» (им же пользуется, например, Telegram):
/// список приложений, которые пропускаются в режиме «Только приоритет».
#[allow(non_snake_case, dead_code)]
mod com {
    use windows::core::{interface, IUnknown, IUnknown_Vtbl, GUID, HRESULT, PCWSTR, PWSTR};

    #[interface("e813fe81-62b6-417d-b951-9d2e08486ac1")]
    pub unsafe trait IQuietHoursProfile: IUnknown {
        fn get_DisplayName(&self, out: *mut PWSTR) -> HRESULT;
        fn get_ProfileId(&self, out: *mut PWSTR) -> HRESULT;
        fn GetSetting(&self, setting: i32, value: *mut i32) -> HRESULT;
        fn PutSetting(&self, setting: i32, value: i32) -> HRESULT;
        fn get_IsCustomizable(&self, value: *mut i32) -> HRESULT;
        fn GetAllowedContacts(&self, count: *mut u32, list: *mut *mut PWSTR) -> HRESULT;
        fn AddAllowedContact(&self, c: PCWSTR) -> HRESULT;
        fn RemoveAllowedContact(&self, c: PCWSTR) -> HRESULT;
        fn GetAllowedApps(&self, count: *mut u32, list: *mut *mut PWSTR) -> HRESULT;
        fn AddAllowedApp(&self, a: PCWSTR) -> HRESULT;
        fn RemoveAllowedApp(&self, a: PCWSTR) -> HRESULT;
    }

    #[interface("6bff4732-81ec-4ffb-ae67-b6c1bc29631f")]
    pub unsafe trait IQuietHoursSettings: IUnknown {
        fn get_UserSelectedProfile(&self, out: *mut PWSTR) -> HRESULT;
        fn put_UserSelectedProfile(&self, id: PCWSTR) -> HRESULT;
        fn GetProfile(&self, id: PCWSTR, profile: *mut Option<IQuietHoursProfile>) -> HRESULT;
    }

    pub const CLSID_QUIET_HOURS: GUID = GUID::from_u128(0xf53321fa_34f8_4b7f_b9a3_361877cb94cf);

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn with_priority_profile<R>(f: impl FnOnce(&IQuietHoursProfile) -> R) -> Option<R> {
        use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_LOCAL_SERVER, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED};
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            let settings: IQuietHoursSettings = CoCreateInstance(&CLSID_QUIET_HOURS, None, CLSCTX_LOCAL_SERVER | CLSCTX_INPROC_SERVER).ok()?;
            let id = wide("Microsoft.QuietHoursProfile.PriorityOnly");
            let mut profile = None;
            settings.GetProfile(windows::core::PCWSTR(id.as_ptr()), &mut profile).ok().ok()?;
            profile.map(|p| f(&p))
        }
    }

    /// Разрешённые в «Только приоритет» приложения (AUMID).
    pub fn allowed_apps() -> Vec<String> {
        with_priority_profile(|p| unsafe {
            let mut count = 0u32;
            let mut list: *mut windows::core::PWSTR = std::ptr::null_mut();
            if p.GetAllowedApps(&mut count, &mut list).is_err() || list.is_null() {
                return vec![];
            }
            let mut out = Vec::new();
            for i in 0..count as usize {
                let s = *list.add(i);
                if let Ok(v) = s.to_string() {
                    out.push(v);
                }
                windows::Win32::System::Com::CoTaskMemFree(Some(s.0 as *const _));
            }
            windows::Win32::System::Com::CoTaskMemFree(Some(list as *const _));
            out
        })
        .unwrap_or_default()
    }

    /// Добавляет приложения в приоритетные; возвращает те, которых там не было (их потом убираем).
    pub fn allow_apps(apps: &[String]) -> Vec<String> {
        let existing = allowed_apps();
        let missing: Vec<String> = apps.iter().filter(|a| !existing.iter().any(|e| e.eq_ignore_ascii_case(a))).cloned().collect();
        with_priority_profile(|p| {
            missing
                .iter()
                .filter(|a| {
                    let w = wide(a);
                    unsafe { p.AddAllowedApp(windows::core::PCWSTR(w.as_ptr())).is_ok() }
                })
                .cloned()
                .collect()
        })
        .unwrap_or_default()
    }

    pub fn disallow_apps(apps: &[String]) {
        with_priority_profile(|p| {
            for a in apps {
                let w = wide(a);
                unsafe {
                    let _ = p.RemoveAllowedApp(windows::core::PCWSTR(w.as_ptr()));
                }
            }
        });
    }
}

/// COM «Фокусировки» вызываем в отдельном потоке (MTA): главный поток окна —
/// STA, и при выходе из программы вызовы оттуда не срабатывали.
fn on_mta<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> Option<R> {
    std::thread::spawn(f).join().ok()
}

pub fn allowed_apps() -> Vec<String> {
    on_mta(com::allowed_apps).unwrap_or_default()
}

pub fn allow_apps(apps: &[String]) -> Vec<String> {
    let apps = apps.to_vec();
    on_mta(move || com::allow_apps(&apps)).unwrap_or_default()
}

pub fn disallow_apps(apps: &[String]) {
    let apps = apps.to_vec();
    on_mta(move || com::disallow_apps(&apps));
}
