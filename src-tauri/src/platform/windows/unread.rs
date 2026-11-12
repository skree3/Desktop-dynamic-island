//! Запасной источник уведомлений для мессенджеров, которые показывают собственные
//! всплывающие окна вместо системных уведомлений Windows. Так по умолчанию ведёт себя
//! Telegram Desktop и его форки (AyuGram, Kotatogram, 64Gram): такие окна не попадают
//! в центр уведомлений, и UserNotificationListener их не видит.
//!
//! Зато Telegram пишет число непрочитанных в заголовок главного окна:
//! «Telegram (5)» или «Имя чата – (5)». Рост этого числа и есть новое сообщение.

use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::UI::WindowsAndMessaging::{EnumWindows, GetClassNameW, GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId};

use super::foreground::process_path;

/// Приложение, для которого нашёлся счётчик в заголовке окна.
#[derive(Debug, Clone)]
pub struct UnreadWindow {
    /// Имя, под которым приложение показывается в острове и в настройках.
    pub app: &'static str,
    pub pid: u32,
    pub exe_path: String,
    pub count: u32,
    /// Окно приложения сейчас активно: пользователь и так видит сообщения.
    pub foreground: bool,
}

struct Family {
    app: &'static str,
    exes: &'static [&'static str],
    /// Слова, по которым узнаём название этого клиента в заголовке («Telegram (5)»).
    names: &'static [&'static str],
}

const FAMILIES: &[Family] = &[Family {
    app: "Telegram",
    exes: &["telegram.exe", "ayugram.exe", "kotatogram.exe", "64gram.exe", "materialgram.exe", "telegramdesktop.exe"],
    names: &["telegram", "ayugram", "kotatogram", "64gram", "materialgram"],
}];

/// К какому семейству относится имя приложения из системного уведомления.
pub fn family_of(app_name: &str) -> Option<&'static str> {
    let lower = app_name.to_lowercase();
    FAMILIES.iter().find(|f| f.names.iter().any(|n| lower.contains(n))).map(|f| f.app)
}

/// Число непрочитанных из заголовка Telegram. Учитываем только два формата,
/// чтобы скобки в названии чата не принимались за счётчик.
pub fn parse_title(title: &str, names: &[&str]) -> Option<u32> {
    let t = title.trim_end();
    let open = t.strip_suffix(')')?.rfind('(')?;
    let count: u32 = t[open + 1..t.len() - 1].parse().ok()?;
    let before = t[..open].trim_end();
    let lower = before.to_lowercase();
    let after_dash = before.ends_with('–') || before.ends_with('—') || before.ends_with(" -");
    let after_name = names.iter().any(|n| lower == *n || lower.ends_with(&format!(" {n}")) || lower.ends_with(&format!("{n} desktop")));
    (after_dash || after_name).then_some(count)
}

struct Collect {
    found: Vec<(u32, String)>,
}

unsafe extern "system" fn enum_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let collect = &mut *(lparam.0 as *mut Collect);
    let mut cls = [0u16; 128];
    let n = GetClassNameW(hwnd, &mut cls);
    if n <= 0 {
        return true.into();
    }
    let class = String::from_utf16_lossy(&cls[..n as usize]);
    // Главное окно Qt-приложения: Qt6112QWindowIcon и т.п.
    if !(class.starts_with("Qt") && class.ends_with("QWindowIcon")) {
        return true.into();
    }
    let mut buf = [0u16; 512];
    let len = GetWindowTextW(hwnd, &mut buf);
    if len <= 0 {
        return true.into();
    }
    let mut pid = 0u32;
    GetWindowThreadProcessId(hwnd, Some(&mut pid));
    if pid != 0 {
        collect.found.push((pid, String::from_utf16_lossy(&buf[..len as usize])));
    }
    true.into()
}

pub fn scan() -> Vec<UnreadWindow> {
    let mut collect = Collect { found: vec![] };
    unsafe {
        let _ = EnumWindows(Some(enum_cb), LPARAM(&mut collect as *mut Collect as isize));
    }
    let fg_pid = unsafe {
        let mut pid = 0u32;
        let h = GetForegroundWindow();
        if !h.is_invalid() {
            GetWindowThreadProcessId(h, Some(&mut pid));
        }
        pid
    };
    let mut out: Vec<UnreadWindow> = vec![];
    for (pid, title) in collect.found {
        let Some(path) = process_path(pid) else { continue };
        let exe = std::path::Path::new(&path).file_name().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default();
        let Some(fam) = FAMILIES.iter().find(|f| f.exes.contains(&exe.as_str())) else { continue };
        // Нет счётчика в заголовке — значит, непрочитанных нет.
        let count = parse_title(&title, fam.names).unwrap_or(0);
        match out.iter_mut().find(|u| u.pid == pid) {
            Some(u) => {
                u.count = u.count.max(count);
                u.foreground |= pid == fg_pid;
            }
            None => out.push(UnreadWindow { app: fam.app, pid, exe_path: path, count, foreground: pid == fg_pid }),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    const N: &[&str] = &["telegram", "ayugram"];

    #[test]
    fn titles() {
        assert_eq!(parse_title("Telegram (5)", N), Some(5));
        assert_eq!(parse_title("AyuGram (12)", N), Some(12));
        assert_eq!(parse_title("Мама – (33)", N), Some(33));
        assert_eq!(parse_title("(2) Чат – (33)", N), Some(33));
        assert_eq!(parse_title("Telegram", N), None);
        assert_eq!(parse_title("Группа (5)", N), None);
        assert_eq!(parse_title("Чат", N), None);
        assert_eq!(family_of("Telegram Desktop"), Some("Telegram"));
        assert_eq!(family_of("AyuGram"), Some("Telegram"));
        assert_eq!(family_of("Discord"), None);
    }
}
