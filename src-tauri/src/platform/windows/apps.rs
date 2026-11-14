//! Обнаружение установленных приложений Windows через виртуальную папку
//! `shell:AppsFolder` — это тот же источник, что использует меню «Пуск»:
//! классические программы (ярлыки), приложения Microsoft Store (UWP/MSIX)
//! и PWA. Иконки извлекаются через IShellItemImageFactory.
//!
//! Объекты оболочки требуют STA, поэтому все вызовы выполняются в
//! выделенном долгоживущем потоке.

use std::sync::mpsc;
use std::sync::OnceLock;
use windows::core::{GUID, PCWSTR};
use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC, BITMAP, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HGDIOBJ,
};
use windows::Win32::System::Com::IBindCtx;
use windows::Win32::UI::Shell::{
    BHID_EnumItems, IEnumShellItems, IShellItem, IShellItemImageFactory, SHCreateItemFromParsingName, SHGetKnownFolderItem,
    SHGetKnownFolderPath, FOLDERID_AppsFolder, KF_FLAG_DEFAULT, SIGDN_NORMALDISPLAY, SIGDN_PARENTRELATIVEPARSING, SIIGBF_BIGGERSIZEOK,
    SIIGBF_ICONONLY,
};

use super::{ensure_com_sta, shell, take_pwstr, wide};
use crate::core::error::{AppError, AppResult};
use crate::platform::types::{InstalledApp, LaunchTarget};
use crate::util::image::{png_data_url, unpremultiply_bgra_to_rgba};

type Job = Box<dyn FnOnce() + Send>;

fn worker() -> &'static mpsc::Sender<Job> {
    static TX: OnceLock<mpsc::Sender<Job>> = OnceLock::new();
    TX.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::Builder::new()
            .name("shell-sta".into())
            .spawn(move || {
                ensure_com_sta();
                while let Ok(job) = rx.recv() {
                    job();
                }
            })
            .expect("shell worker thread");
        tx
    })
}

pub fn run_sta<R: Send + 'static>(f: impl FnOnce() -> R + Send + 'static) -> AppResult<R> {
    let (tx, rx) = mpsc::channel();
    worker()
        .send(Box::new(move || {
            let _ = tx.send(f());
        }))
        .map_err(|_| AppError::system("поток оболочки недоступен"))?;
    rx.recv().map_err(|_| AppError::system("поток оболочки не ответил"))
}

/// "{GUID}\rest\app.exe" → полный путь через SHGetKnownFolderPath.
fn resolve_exe(parsing: &str) -> Option<String> {
    if parsing.len() > 3 && parsing.as_bytes()[1] == b':' {
        return Some(parsing.to_string()).filter(|p| p.to_lowercase().ends_with(".exe"));
    }
    if parsing.starts_with('{') {
        let end = parsing.find('}')?;
        let guid = GUID::try_from(&parsing[1..end]).ok()?;
        let rest = parsing[end + 1..].trim_start_matches('\\');
        let base = unsafe { take_pwstr(SHGetKnownFolderPath(&guid, KF_FLAG_DEFAULT, None).ok()?) };
        let full = format!("{base}\\{rest}");
        return Some(full).filter(|p| p.to_lowercase().ends_with(".exe"));
    }
    None
}

fn scan_inner() -> AppResult<Vec<InstalledApp>> {
    let mut out = vec![];
    unsafe {
        let folder: IShellItem = SHGetKnownFolderItem(&FOLDERID_AppsFolder, KF_FLAG_DEFAULT, None)?;
        let en: IEnumShellItems = folder.BindToHandler(None::<&IBindCtx>, &BHID_EnumItems)?;
        loop {
            let mut items = [None];
            let mut fetched = 0u32;
            if en.Next(&mut items, Some(&mut fetched)).is_err() || fetched == 0 {
                break;
            }
            let Some(item) = items[0].take() else { break };
            let name = item.GetDisplayName(SIGDN_NORMALDISPLAY).map(|p| take_pwstr(p)).unwrap_or_default();
            let parsing = item.GetDisplayName(SIGDN_PARENTRELATIVEPARSING).map(|p| take_pwstr(p)).unwrap_or_default();
            if name.is_empty() || parsing.is_empty() {
                continue;
            }
            let lower = name.to_lowercase();
            // Служебные элементы (деинсталляторы, справка) не нужны в поиске.
            if lower.starts_with("uninstall") || lower.starts_with("удалить") || lower.contains("деинстал") {
                continue;
            }
            let exe = resolve_exe(&parsing);
            let mut keywords = vec![];
            if let Some(e) = &exe {
                if let Some(stem) = std::path::Path::new(e).file_stem() {
                    keywords.push(stem.to_string_lossy().to_lowercase());
                }
            }
            out.push(InstalledApp {
                id: format!("win:{parsing}"),
                name,
                target: LaunchTarget::AppsFolder { id: parsing },
                exe_path: exe,
                icon_hint: None,
                keywords,
            });
        }
    }
    out.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    out.dedup_by(|a, b| a.id == b.id);
    Ok(out)
}

pub fn scan() -> AppResult<Vec<InstalledApp>> {
    run_sta(scan_inner)?
}

unsafe fn hbitmap_to_data_url(hbmp: HBITMAP) -> Option<String> {
    let mut bmp = BITMAP::default();
    if GetObjectW(HGDIOBJ(hbmp.0), std::mem::size_of::<BITMAP>() as i32, Some(&mut bmp as *mut _ as *mut _)) == 0 {
        let _ = DeleteObject(HGDIOBJ(hbmp.0));
        return None;
    }
    let (w, h) = (bmp.bmWidth, bmp.bmHeight.abs());
    let mut bi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: w,
            biHeight: -h,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut buf = vec![0u8; (w * h * 4) as usize];
    let hdc = GetDC(None);
    let lines = GetDIBits(hdc, hbmp, 0, h as u32, Some(buf.as_mut_ptr() as *mut _), &mut bi, DIB_RGB_COLORS);
    ReleaseDC(None, hdc);
    let _ = DeleteObject(HGDIOBJ(hbmp.0));
    if lines == 0 {
        return None;
    }
    unpremultiply_bgra_to_rgba(&mut buf);
    png_data_url(w as u32, h as u32, &buf)
}

fn icon_inner(parsing_path: String, size: i32) -> Option<String> {
    unsafe {
        let w = wide(&parsing_path);
        let factory: IShellItemImageFactory = SHCreateItemFromParsingName(PCWSTR(w.as_ptr()), None::<&IBindCtx>).ok()?;
        let hbmp = factory.GetImage(SIZE { cx: size, cy: size }, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK).ok()?;
        hbitmap_to_data_url(hbmp)
    }
}

/// Иконка для элемента AppsFolder или произвольного пути к файлу.
pub fn icon(target: &LaunchTarget, _icon_hint: Option<&str>, size: u32) -> AppResult<Option<String>> {
    let path = match target {
        LaunchTarget::AppsFolder { id } => format!("shell:AppsFolder\\{id}"),
        LaunchTarget::Exe { path, .. } => path.clone(),
        LaunchTarget::Desktop { .. } | LaunchTarget::Uri { .. } => return Ok(None),
    };
    let size = size.clamp(16, 256) as i32;
    run_sta(move || icon_inner(path, size))
}

/// Запуск от имени администратора. Приложения из Microsoft Store так запустить нельзя.
pub fn launch_admin(target: &LaunchTarget, exe_path: Option<&str>) -> AppResult<()> {
    let exe = match (target, exe_path) {
        (LaunchTarget::Exe { path, args }, _) => Some((path.clone(), args.clone())),
        (_, Some(p)) if p.to_lowercase().ends_with(".exe") || p.to_lowercase().ends_with(".lnk") => Some((p.to_string(), vec![])),
        _ => None,
    };
    if let Some((path, args)) = exe {
        let p = std::path::Path::new(&path);
        if !p.is_file() {
            return Err(AppError::NotFound(format!("файл не найден: {path}")));
        }
        let dir = p.parent().map(|d| d.to_string_lossy().to_string());
        let joined = args.iter().map(|a| if a.contains(' ') { format!("\"{a}\"") } else { a.clone() }).collect::<Vec<_>>().join(" ");
        return shell::shell_exec("runas", &path, if joined.is_empty() { None } else { Some(&joined) }, dir.as_deref());
    }
    match target {
        // Ярлыки классических программ в меню «Пуск» принимают «runas»; приложения Store — нет.
        LaunchTarget::AppsFolder { id } if !id.contains('!') => shell::shell_exec("runas", &format!("shell:AppsFolder\\{id}"), None, None),
        _ => Err(AppError::unsupported("это приложение нельзя запустить от имени администратора")),
    }
}

pub fn launch(target: &LaunchTarget) -> AppResult<()> {
    match target {
        LaunchTarget::AppsFolder { id } => shell::shell_open(&format!("shell:AppsFolder\\{id}"), None, None),
        LaunchTarget::Exe { path, args } => {
            let p = std::path::Path::new(path);
            if !p.is_file() {
                return Err(AppError::NotFound(format!("исполняемый файл не найден: {path}")));
            }
            let dir = p.parent().map(|d| d.to_string_lossy().to_string());
            let joined = args.iter().map(|a| if a.contains(' ') { format!("\"{a}\"") } else { a.clone() }).collect::<Vec<_>>().join(" ");
            shell::shell_open(path, if joined.is_empty() { None } else { Some(&joined) }, dir.as_deref())
        }
        LaunchTarget::Uri { uri } => shell::open_uri(uri),
        LaunchTarget::Desktop { .. } => Err(AppError::unsupported(".desktop-файлы поддерживаются только в Linux")),
    }
}
