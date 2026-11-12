//! Яркость на Windows:
//! * встроенные панели ноутбуков — WMI (`root\WMI`, WmiMonitorBrightness/Methods);
//! * внешние мониторы — DDC/CI через Dxva2 (Get/SetMonitorBrightness).
//! Мониторы без DDC/CI и настольные ПК без встроенной панели программно не управляются.

use windows::core::{BSTR, PCWSTR};
use windows::Win32::Devices::Display::{
    DestroyPhysicalMonitors, GetMonitorBrightness, GetNumberOfPhysicalMonitorsFromHMONITOR, GetPhysicalMonitorsFromHMONITOR,
    SetMonitorBrightness, PHYSICAL_MONITOR,
};
use windows::Win32::Foundation::{LPARAM, RECT};
use windows::Win32::Graphics::Gdi::{EnumDisplayMonitors, HDC, HMONITOR};
use windows::Win32::System::Com::{
    CoCreateInstance, CoSetProxyBlanket, CLSCTX_INPROC_SERVER, EOAC_NONE, RPC_C_AUTHN_LEVEL_CALL, RPC_C_IMP_LEVEL_IMPERSONATE,
};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::System::Wmi::{
    IWbemClassObject, IWbemLocator, IWbemServices, WbemLocator, WBEM_FLAG_FORWARD_ONLY, WBEM_FLAG_RETURN_IMMEDIATELY, WBEM_INFINITE,
};

use super::{ensure_com, from_wide_buf, wide};
use crate::core::error::{AppError, AppResult};
use crate::platform::types::{BrightnessState, DisplayBrightness};

// ---------- WMI ----------

fn wmi() -> AppResult<IWbemServices> {
    ensure_com();
    unsafe {
        let locator: IWbemLocator = CoCreateInstance(&WbemLocator, None, CLSCTX_INPROC_SERVER)?;
        let svc = locator.ConnectServer(&BSTR::from("ROOT\\WMI"), &BSTR::new(), &BSTR::new(), &BSTR::new(), 0, &BSTR::new(), None)?;
        // RPC_C_AUTHN_WINNT = 10, RPC_C_AUTHZ_NONE = 0
        CoSetProxyBlanket(&svc, 10, 0, PCWSTR::null(), RPC_C_AUTHN_LEVEL_CALL, RPC_C_IMP_LEVEL_IMPERSONATE, None, EOAC_NONE)?;
        Ok(svc)
    }
}

fn query(svc: &IWbemServices, wql: &str) -> AppResult<Vec<IWbemClassObject>> {
    let mut out = vec![];
    unsafe {
        let en = svc.ExecQuery(&BSTR::from("WQL"), &BSTR::from(wql), WBEM_FLAG_FORWARD_ONLY | WBEM_FLAG_RETURN_IMMEDIATELY, None)?;
        loop {
            let mut row = [None];
            let mut returned = 0u32;
            let hr = en.Next(WBEM_INFINITE, &mut row, &mut returned);
            if hr.is_err() || returned == 0 {
                break;
            }
            if let Some(obj) = row[0].take() {
                out.push(obj);
            }
        }
    }
    Ok(out)
}

fn get_prop(obj: &IWbemClassObject, name: &str) -> Option<VARIANT> {
    let w = wide(name);
    let mut v = VARIANT::default();
    unsafe { obj.Get(PCWSTR(w.as_ptr()), 0, &mut v, None, None).ok()? };
    Some(v)
}

fn wmi_list(svc: &IWbemServices) -> Vec<DisplayBrightness> {
    let rows = query(svc, "SELECT InstanceName, CurrentBrightness FROM WmiMonitorBrightness WHERE Active=TRUE").unwrap_or_default();
    rows.iter()
        .enumerate()
        .filter_map(|(i, r)| {
            let value = get_prop(r, "CurrentBrightness").and_then(|v| i32::try_from(&v).ok())?;
            Some(DisplayBrightness {
                id: format!("wmi:{i}"),
                name: "Встроенный дисплей".into(),
                value: value.clamp(0, 100) as u32,
                method: "WMI".into(),
            })
        })
        .collect()
}

fn wmi_set(index: usize, value: u32) -> AppResult<()> {
    let svc = wmi()?;
    let methods = query(&svc, "SELECT * FROM WmiMonitorBrightnessMethods WHERE Active=TRUE")?;
    let inst = methods.get(index).ok_or_else(|| AppError::NotFound("дисплей не найден".into()))?;
    let path = get_prop(inst, "__PATH")
        .and_then(|v| BSTR::try_from(&v).ok())
        .ok_or_else(|| AppError::system("не удалось получить путь WMI-объекта"))?;
    unsafe {
        let mut class: Option<IWbemClassObject> = None;
        svc.GetObject(&BSTR::from("WmiMonitorBrightnessMethods"), Default::default(), None, Some(&mut class), None)?;
        let class = class.ok_or_else(|| AppError::system("класс WMI не найден"))?;
        let mut in_sig: Option<IWbemClassObject> = None;
        let name = wide("WmiSetBrightness");
        class.GetMethod(PCWSTR(name.as_ptr()), 0, &mut in_sig, std::ptr::null_mut())?;
        let in_sig = in_sig.ok_or_else(|| AppError::system("сигнатура метода WMI не найдена"))?;
        let params = in_sig.SpawnInstance(0)?;
        let t = wide("Timeout");
        params.Put(PCWSTR(t.as_ptr()), 0, &VARIANT::from(0i32), 0)?;
        let b = wide("Brightness");
        params.Put(PCWSTR(b.as_ptr()), 0, &VARIANT::from(value.min(100) as u8), 0)?;
        svc.ExecMethod(&path, &BSTR::from("WmiSetBrightness"), Default::default(), None, &params, None, None)?;
    }
    Ok(())
}

// ---------- DDC/CI ----------

unsafe extern "system" fn collect_monitor(h: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> windows::core::BOOL {
    let list = &mut *(data.0 as *mut Vec<HMONITOR>);
    list.push(h);
    true.into()
}

fn monitors() -> Vec<HMONITOR> {
    let mut list: Vec<HMONITOR> = vec![];
    unsafe {
        let _ = EnumDisplayMonitors(None, None, Some(collect_monitor), LPARAM(&mut list as *mut _ as isize));
    }
    list
}

fn with_physical<R>(f: impl FnMut(usize, usize, &PHYSICAL_MONITOR) -> Option<R>) -> Vec<R> {
    let mut f = f;
    let mut out = vec![];
    for (mi, hmon) in monitors().into_iter().enumerate() {
        unsafe {
            let mut count = 0u32;
            if GetNumberOfPhysicalMonitorsFromHMONITOR(hmon, &mut count).is_err() || count == 0 {
                continue;
            }
            let mut phys = vec![PHYSICAL_MONITOR::default(); count as usize];
            if GetPhysicalMonitorsFromHMONITOR(hmon, &mut phys).is_err() {
                continue;
            }
            for (pi, p) in phys.iter().enumerate() {
                if let Some(r) = f(mi, pi, p) {
                    out.push(r);
                }
            }
            let _ = DestroyPhysicalMonitors(&phys);
        }
    }
    out
}

fn ddc_list() -> Vec<DisplayBrightness> {
    with_physical(|mi, pi, p| unsafe {
        let (mut min, mut cur, mut max) = (0u32, 0u32, 0u32);
        if GetMonitorBrightness(p.hPhysicalMonitor, &mut min, &mut cur, &mut max) == 0 || max <= min {
            return None;
        }
        let pct = ((cur.saturating_sub(min)) as f64 / (max - min) as f64 * 100.0).round() as u32;
        let desc = p.szPhysicalMonitorDescription;
        let name = from_wide_buf(&desc);
        Some(DisplayBrightness {
            id: format!("ddc:{mi}:{pi}"),
            name: if name.is_empty() { format!("Монитор {}", mi + 1) } else { name },
            value: pct,
            method: "DDC/CI".into(),
        })
    })
}

fn ddc_set(mi: usize, pi: usize, value: u32) -> AppResult<()> {
    let res = with_physical(|m, p, phys| {
        if m != mi || p != pi {
            return None;
        }
        unsafe {
            let (mut min, mut cur, mut max) = (0u32, 0u32, 0u32);
            if GetMonitorBrightness(phys.hPhysicalMonitor, &mut min, &mut cur, &mut max) == 0 {
                return Some(false);
            }
            let raw = min + ((max - min) as f64 * value.min(100) as f64 / 100.0).round() as u32;
            Some(SetMonitorBrightness(phys.hPhysicalMonitor, raw) != 0)
        }
    });
    match res.first() {
        Some(true) => Ok(()),
        Some(false) => Err(AppError::system("монитор отклонил команду DDC/CI")),
        None => Err(AppError::NotFound("монитор не найден".into())),
    }
}

pub fn state() -> AppResult<BrightnessState> {
    let mut displays = match wmi() {
        Ok(svc) => wmi_list(&svc),
        Err(_) => vec![],
    };
    displays.extend(ddc_list());
    let error = if displays.is_empty() {
        Some("Программное управление яркостью недоступно: нет встроенной панели (WMI), а внешние мониторы не поддерживают DDC/CI или он отключён в меню монитора.".into())
    } else {
        None
    };
    Ok(BrightnessState { displays, error })
}

pub fn set(id: &str, value: u32) -> AppResult<()> {
    let parts: Vec<&str> = id.split(':').collect();
    match parts.as_slice() {
        ["wmi", i] => wmi_set(i.parse().map_err(|_| AppError::invalid("id"))?, value),
        ["ddc", m, p] => ddc_set(
            m.parse().map_err(|_| AppError::invalid("id"))?,
            p.parse().map_err(|_| AppError::invalid("id"))?,
            value,
        ),
        _ => Err(AppError::invalid("неизвестный идентификатор дисплея")),
    }
}
