//! Состояние VPN-клиентов по их сетевым адаптерам (Happ создаёт «happ-tun»,
//! INCY — Wintun-адаптер «wwan99») и путь к программе по зарегистрированной схеме
//! happ:// / incy://.

use windows::Win32::NetworkManagement::IpHelper::{FreeMibTable, GetIfTable2, MIB_IF_TABLE2};
use windows::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

use super::proxy::read_reg_string;

/// Есть ли среди работающих адаптеров адаптер с одним из имён.
pub fn adapter_up(names: &[&str]) -> bool {
    unsafe {
        let mut table: *mut MIB_IF_TABLE2 = std::ptr::null_mut();
        if GetIfTable2(&mut table).is_err() || table.is_null() {
            return false;
        }
        let t = &*table;
        let rows = std::slice::from_raw_parts(t.Table.as_ptr(), t.NumEntries as usize);
        let found = rows.iter().any(|r| {
            if r.OperStatus != IfOperStatusUp {
                return false;
            }
            let end = r.Alias.iter().position(|&c| c == 0).unwrap_or(r.Alias.len());
            let alias = String::from_utf16_lossy(&r.Alias[..end]);
            names.iter().any(|n| alias.eq_ignore_ascii_case(n))
        });
        FreeMibTable(table as *const _);
        found
    }
}

/// Путь к программе, которая открывает ссылки `scheme://`.
pub fn protocol_handler(scheme: &str) -> Option<String> {
    let key = format!(r"Software\Classes\{scheme}\shell\open\command");
    let cmd = read_reg_string(HKEY_CURRENT_USER, &key, "")
        .or_else(|| read_reg_string(HKEY_LOCAL_MACHINE, &format!(r"SOFTWARE\Classes\{scheme}\shell\open\command"), ""))?;
    let cmd = cmd.trim();
    let exe = if let Some(rest) = cmd.strip_prefix('"') { rest.split('"').next()?.to_string() } else { cmd.split(' ').next()?.to_string() };
    std::path::Path::new(&exe).is_file().then_some(exe)
}
