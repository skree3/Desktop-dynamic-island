//! Обнаружение приложений Linux по спецификации freedesktop Desktop Entry:
//! ~/.local/share/applications, $XDG_DATA_DIRS/applications, Flatpak и Snap.
//! Запуск — разбор строки Exec без использования shell.

use std::collections::HashMap;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::core::error::{AppError, AppResult};
use crate::platform::types::{InstalledApp, LaunchTarget};
use crate::util::image::bytes_data_url;

fn data_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![];
    let home = std::env::var("HOME").unwrap_or_default();
    let data_home = std::env::var("XDG_DATA_HOME").unwrap_or_else(|_| format!("{home}/.local/share"));
    dirs.push(PathBuf::from(&data_home));
    let sys = std::env::var("XDG_DATA_DIRS").unwrap_or_else(|_| "/usr/local/share:/usr/share".into());
    dirs.extend(sys.split(':').filter(|s| !s.is_empty()).map(PathBuf::from));
    dirs.push(PathBuf::from(format!("{data_home}/flatpak/exports/share")));
    dirs.push(PathBuf::from("/var/lib/flatpak/exports/share"));
    dirs.push(PathBuf::from("/var/lib/snapd/desktop"));
    dirs.dedup();
    dirs
}

fn locale_keys() -> Vec<String> {
    let lang = std::env::var("LC_MESSAGES").or_else(|_| std::env::var("LANG")).unwrap_or_default();
    let base = lang.split('.').next().unwrap_or("").to_string();
    let short = base.split('_').next().unwrap_or("").to_string();
    [base, short].into_iter().filter(|s| !s.is_empty()).collect()
}

pub fn parse_desktop(text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut in_main = false;
    for line in text.lines() {
        let l = line.trim();
        if l.starts_with('[') {
            in_main = l == "[Desktop Entry]";
            continue;
        }
        if !in_main || l.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = l.split_once('=') {
            map.entry(k.trim().to_string()).or_insert_with(|| v.trim().to_string());
        }
    }
    map
}

fn localized(map: &HashMap<String, String>, key: &str, locales: &[String]) -> Option<String> {
    for loc in locales {
        if let Some(v) = map.get(&format!("{key}[{loc}]")) {
            return Some(v.clone());
        }
    }
    map.get(key).cloned()
}

fn visible_here(map: &HashMap<String, String>) -> bool {
    let desktop = super::desktop_name();
    let current: Vec<&str> = desktop.split(':').collect();
    if let Some(only) = map.get("OnlyShowIn") {
        if !only.split(';').any(|d| current.contains(&d)) {
            return false;
        }
    }
    if let Some(not) = map.get("NotShowIn") {
        if not.split(';').any(|d| !d.is_empty() && current.contains(&d)) {
            return false;
        }
    }
    true
}

pub fn scan() -> AppResult<Vec<InstalledApp>> {
    let locales = locale_keys();
    let mut seen: HashMap<String, InstalledApp> = HashMap::new();
    for base in data_dirs() {
        let dir = base.join("applications");
        let mut stack = vec![dir.clone()];
        while let Some(d) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&d) else { continue };
            for e in entries.flatten() {
                let path = e.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|s| s.to_str()) != Some("desktop") {
                    continue;
                }
                let rel = path.strip_prefix(&dir).unwrap_or(&path).to_string_lossy().replace('/', "-");
                if seen.contains_key(&rel) {
                    continue; // приоритет у пользовательских каталогов (обрабатываются первыми)
                }
                let Ok(text) = std::fs::read_to_string(&path) else { continue };
                let map = parse_desktop(&text);
                let hidden = map.get("NoDisplay").map(|v| v == "true").unwrap_or(false)
                    || map.get("Hidden").map(|v| v == "true").unwrap_or(false);
                if map.get("Type").map(|t| t != "Application").unwrap_or(true) || hidden || !visible_here(&map) {
                    // Скрытая запись всё равно «занимает» id, чтобы перекрыть системную.
                    seen.insert(rel.clone(), InstalledApp { id: String::new(), name: String::new(), target: LaunchTarget::Desktop { path: String::new() }, exe_path: None, icon_hint: None, keywords: vec![] });
                    continue;
                }
                let Some(name) = localized(&map, "Name", &locales) else { continue };
                let exec = map.get("Exec").cloned().unwrap_or_default();
                let exe_token = split_exec(&exec).into_iter().next().unwrap_or_default();
                let mut keywords: Vec<String> = localized(&map, "Keywords", &locales)
                    .unwrap_or_default()
                    .split(';')
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_lowercase())
                    .collect();
                if let Some(g) = localized(&map, "GenericName", &locales) {
                    keywords.push(g.to_lowercase());
                }
                let exe_path = resolve_bin(&exe_token);
                if let Some(stem) = Path::new(&exe_token).file_name() {
                    keywords.push(stem.to_string_lossy().to_lowercase());
                }
                seen.insert(
                    rel.clone(),
                    InstalledApp {
                        id: format!("desktop:{rel}"),
                        name,
                        target: LaunchTarget::Desktop { path: path.to_string_lossy().to_string() },
                        exe_path,
                        icon_hint: map.get("Icon").cloned(),
                        keywords,
                    },
                );
            }
        }
    }
    let mut list: Vec<InstalledApp> = seen.into_values().filter(|a| !a.id.is_empty()).collect();
    list.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(list)
}

fn resolve_bin(token: &str) -> Option<String> {
    if token.is_empty() {
        return None;
    }
    if token.starts_with('/') {
        return Some(token.to_string());
    }
    let paths = std::env::var_os("PATH")?;
    std::env::split_paths(&paths).map(|p| p.join(token)).find(|p| p.is_file()).map(|p| p.to_string_lossy().to_string())
}

/// Разбор строки Exec по спецификации (кавычки, экранирование).
pub fn split_exec(exec: &str) -> Vec<String> {
    let mut args = vec![];
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut has = false;
    let mut chars = exec.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                in_quotes = !in_quotes;
                has = true;
            }
            '\\' if in_quotes => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            c if c.is_whitespace() && !in_quotes => {
                if has || !cur.is_empty() {
                    args.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            c => cur.push(c),
        }
    }
    if has || !cur.is_empty() {
        args.push(cur);
    }
    args
}

/// Удаляет коды полей (%f, %U и т.д.), подставляет %i/%c/%k.
pub fn expand_field_codes(args: Vec<String>, name: &str, icon: Option<&str>, path: &str) -> Vec<String> {
    let mut out = vec![];
    for a in args {
        match a.as_str() {
            "%f" | "%F" | "%u" | "%U" | "%d" | "%D" | "%n" | "%N" | "%v" | "%m" => continue,
            "%i" => {
                if let Some(i) = icon {
                    out.push("--icon".into());
                    out.push(i.into());
                }
            }
            "%c" => out.push(name.into()),
            "%k" => out.push(path.into()),
            _ => {
                let s = a.replace("%%", "\u{0}").replace("%f", "").replace("%F", "").replace("%u", "").replace("%U", "").replace('\u{0}', "%");
                if !s.is_empty() {
                    out.push(s);
                }
            }
        }
    }
    out
}

fn spawn_detached(program: &str, args: &[String], dir: Option<&str>) -> AppResult<()> {
    let mut cmd = Command::new(program);
    cmd.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    if let Some(d) = dir.filter(|d| Path::new(d).is_dir()) {
        cmd.current_dir(d);
    }
    // Новая группа процессов: приложение не завершится вместе с Desktop Island.
    cmd.process_group(0);
    cmd.spawn().map_err(|e| AppError::system(format!("не удалось запустить {program}: {e}")))?;
    Ok(())
}

pub fn launch(target: &LaunchTarget) -> AppResult<()> {
    match target {
        LaunchTarget::Desktop { path } => {
            let text = std::fs::read_to_string(path).map_err(|_| AppError::NotFound("desktop-файл не найден".into()))?;
            let map = parse_desktop(&text);
            let exec = map.get("Exec").ok_or_else(|| AppError::invalid("в desktop-файле нет Exec"))?;
            let name = map.get("Name").cloned().unwrap_or_default();
            let args = expand_field_codes(split_exec(exec), &name, map.get("Icon").map(|s| s.as_str()), path);
            let (program, rest) = args.split_first().ok_or_else(|| AppError::invalid("пустая команда Exec"))?;
            if map.get("Terminal").map(|v| v == "true").unwrap_or(false) {
                let term = ["x-terminal-emulator", "gnome-terminal", "konsole", "xterm"].into_iter().find(|t| super::command_exists(t));
                if let Some(t) = term {
                    let mut all = vec!["-e".to_string(), program.clone()];
                    all.extend(rest.iter().cloned());
                    return spawn_detached(t, &all, map.get("Path").map(|s| s.as_str()));
                }
            }
            spawn_detached(program, rest, map.get("Path").map(|s| s.as_str()))
        }
        LaunchTarget::Exe { path, args } => {
            if !Path::new(path).is_file() {
                return Err(AppError::NotFound(format!("файл не найден: {path}")));
            }
            spawn_detached(path, args, Path::new(path).parent().and_then(|p| p.to_str()))
        }
        LaunchTarget::Uri { uri } => super::shell::open_uri(uri),
        LaunchTarget::AppsFolder { .. } => Err(AppError::unsupported("shell:AppsFolder доступен только в Windows")),
    }
}

fn icon_dirs() -> Vec<PathBuf> {
    let mut out = vec![];
    let home = std::env::var("HOME").unwrap_or_default();
    out.push(PathBuf::from(format!("{home}/.local/share/icons")));
    out.push(PathBuf::from(format!("{home}/.icons")));
    for d in data_dirs() {
        out.push(d.join("icons"));
    }
    out
}

fn find_icon(name: &str) -> Option<PathBuf> {
    if name.starts_with('/') {
        return Some(PathBuf::from(name)).filter(|p| p.is_file());
    }
    if name.contains('/') || name.contains("..") {
        return None;
    }
    let sizes = ["48x48", "64x64", "128x128", "96x96", "256x256", "32x32", "scalable"];
    let themes = ["hicolor", "Adwaita", "breeze", "Papirus"];
    for base in icon_dirs() {
        for theme in themes {
            for size in sizes {
                for ext in ["png", "svg"] {
                    let p = base.join(theme).join(size).join("apps").join(format!("{name}.{ext}"));
                    if p.is_file() {
                        return Some(p);
                    }
                }
            }
        }
    }
    for ext in ["png", "svg", "xpm"] {
        let p = PathBuf::from(format!("/usr/share/pixmaps/{name}.{ext}"));
        if p.is_file() && ext != "xpm" {
            return Some(p);
        }
    }
    None
}

pub fn icon(target: &LaunchTarget, icon_hint: Option<&str>, _size: u32) -> AppResult<Option<String>> {
    let hint = match (icon_hint, target) {
        (Some(h), _) => Some(h.to_string()),
        (None, LaunchTarget::Desktop { path }) => std::fs::read_to_string(path).ok().and_then(|t| parse_desktop(&t).get("Icon").cloned()),
        _ => None,
    };
    let Some(hint) = hint else { return Ok(None) };
    let Some(path) = find_icon(&hint) else { return Ok(None) };
    let meta = std::fs::metadata(&path)?;
    if meta.len() > 512 * 1024 {
        return Ok(None);
    }
    let bytes = std::fs::read(&path)?;
    let mime = if path.extension().and_then(|e| e.to_str()) == Some("svg") { "image/svg+xml" } else { "image/png" };
    Ok(Some(bytes_data_url(mime, &bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_parsing() {
        assert_eq!(split_exec(r#"/usr/bin/foo --a "b c" %U"#), vec!["/usr/bin/foo", "--a", "b c", "%U"]);
        let a = expand_field_codes(split_exec("code --new-window %F"), "Code", None, "/x.desktop");
        assert_eq!(a, vec!["code", "--new-window"]);
    }

    #[test]
    fn desktop_parsing() {
        let m = parse_desktop("[Desktop Entry]\nName=Foo\nName[ru]=Фу\nExec=foo\n[Desktop Action x]\nName=Bar\n");
        assert_eq!(m.get("Name").unwrap(), "Foo");
        assert_eq!(localized(&m, "Name", &["ru".into()]).unwrap(), "Фу");
    }
}
