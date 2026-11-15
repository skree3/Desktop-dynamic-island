//! Загрузки Steam в острове: название игры, прогресс, скорость и сколько осталось.
//!
//! Steam не даёт API для этого другим программам, но хранит состояние каждой
//! игры в `steamapps/appmanifest_<id>.acf` (StateFlags, BytesDownloaded,
//! BytesToDownload). Манифест переписывается раз в несколько секунд, поэтому
//! между записями прогресс досчитывается по скорости сети, а скорость — по
//! самому загруженному сетевому интерфейсу (VPN не удваивает цифру).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};
use sysinfo::Networks;

use crate::core::state::Shared;
use crate::core::task::TaskSlot;
use crate::database::now_ms;
use crate::events::{Activity, IslandNotification, NotificationAction, NotificationKind, Priority};
use crate::modules::downloads::format_size;

const FLAG_FULLY_INSTALLED: u64 = 4;
const FLAG_UPDATE_REQUIRED: u64 = 2;
const FLAG_UPDATE_RUNNING: u64 = 256;
const FLAG_UPDATE_PAUSED: u64 = 512;
const FLAG_DOWNLOADING: u64 = 1_048_576;

#[derive(Default)]
pub struct SteamModule {
    task: TaskSlot,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Manifest {
    pub appid: String,
    pub name: String,
    pub flags: u64,
    pub downloaded: u64,
    pub total: u64,
}

impl Manifest {
    pub fn active(&self) -> bool {
        (self.flags & (FLAG_UPDATE_RUNNING | FLAG_DOWNLOADING)) != 0 && self.total > 0 && self.downloaded < self.total
    }
    pub fn paused(&self) -> bool {
        (self.flags & FLAG_UPDATE_PAUSED) != 0
    }
    /// Установлено и обновлять нечего.
    pub fn done(&self) -> bool {
        (self.flags & FLAG_FULLY_INSTALLED) != 0 && (self.flags & FLAG_UPDATE_REQUIRED) == 0 && !self.paused() && !self.active()
    }
}

/// Значение ключа из VDF/ACF: `"key"\t\t"value"`.
fn vdf_value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\"");
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix(&needle) {
            let v = rest.trim();
            return v.strip_prefix('"').and_then(|v| v.strip_suffix('"'));
        }
    }
    None
}

pub fn parse_manifest(text: &str) -> Option<Manifest> {
    Some(Manifest {
        appid: vdf_value(text, "appid")?.to_string(),
        name: vdf_value(text, "name").unwrap_or("Steam").to_string(),
        flags: vdf_value(text, "StateFlags").and_then(|v| v.parse().ok()).unwrap_or(0),
        downloaded: vdf_value(text, "BytesDownloaded").and_then(|v| v.parse().ok()).unwrap_or(0),
        total: vdf_value(text, "BytesToDownload").and_then(|v| v.parse().ok()).unwrap_or(0),
    })
}

/// Все пути библиотек из libraryfolders.vdf (строки `"path" "..."`).
pub fn parse_libraries(text: &str) -> Vec<PathBuf> {
    text.lines()
        .filter_map(|l| {
            let t = l.trim();
            let rest = t.strip_prefix("\"path\"")?.trim();
            Some(PathBuf::from(rest.trim_matches('"').replace("\\\\", "\\")))
        })
        .collect()
}

fn steam_root() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        use windows::Win32::System::Registry::HKEY_CURRENT_USER;
        let p = crate::platform::os::proxy::read_reg_string(HKEY_CURRENT_USER, r"Software\Valve\Steam", "SteamPath")?;
        let p = PathBuf::from(p.replace('/', "\\"));
        p.is_dir().then_some(p)
    }
    #[cfg(not(windows))]
    {
        let home = PathBuf::from(std::env::var_os("HOME")?);
        [".steam/steam", ".local/share/Steam", ".var/app/com.valvesoftware.Steam/.local/share/Steam"]
            .iter()
            .map(|d| home.join(d))
            .find(|p| p.join("steamapps").is_dir())
    }
}

fn library_dirs(root: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![root.join("steamapps")];
    // Для проверки без настоящей загрузки: ещё одна папка steamapps.
    if let Some(extra) = std::env::var_os("DI_STEAM_LIBRARY") {
        dirs.push(PathBuf::from(extra));
    }
    if let Ok(text) = std::fs::read_to_string(root.join("steamapps").join("libraryfolders.vdf")) {
        for lib in parse_libraries(&text) {
            let d = lib.join("steamapps");
            if d.is_dir() && !dirs.iter().any(|x| x.to_string_lossy().eq_ignore_ascii_case(&d.to_string_lossy())) {
                dirs.push(d);
            }
        }
    }
    dirs
}

/// Состояние загрузки по журналу Steam (`logs/content_log.txt`).
/// Во время загрузки Steam почти не переписывает appmanifest (BytesDownloaded
/// и StateFlags там обновляются только в конце), зато сразу пишет в журнал:
///   AppID 4465480 update started : download 0/14783871008, ...
///   AppID 4465480 state changed : Update Required,Update Queued,Update Running,Update Started,
///   AppID 4465480 state changed : Update Required,Update Queued,Update Started, (Suspended)
///   AppID 4465480 finished update, ...
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LogApp {
    pub running: bool,
    pub suspended: bool,
    pub downloaded: u64,
    pub total: u64,
    /// Загрузка закончилась («finished update»).
    pub finished: bool,
}

pub fn apply_log_line(apps: &mut HashMap<String, LogApp>, line: &str) {
    let Some(i) = line.find("AppID ") else { return };
    let rest = &line[i + 6..];
    let id: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if id.is_empty() {
        return;
    }
    let rest = rest[id.len()..].trim_start();
    if let Some(s) = rest.strip_prefix("state changed :") {
        let suspended = s.contains("(Suspended)");
        let running = s.contains("Update Running") && !suspended;
        let started = s.contains("Update Started");
        let e = apps.entry(id).or_default();
        e.running = running;
        e.suspended = suspended && started;
        if !running && !e.suspended {
            e.downloaded = 0;
            e.total = 0;
        }
    } else if let Some(s) = rest.strip_prefix("update started :") {
        // "download 0/14783871008, store 0/0, ..."
        let d = s.trim().strip_prefix("download ").and_then(|d| d.split(',').next()).and_then(|d| d.split_once('/'));
        if let Some((a, b)) = d {
            let e = apps.entry(id).or_default();
            e.downloaded = a.trim().parse().unwrap_or(0);
            e.total = b.trim().parse().unwrap_or(0);
            e.finished = false;
        }
    } else if rest.starts_with("finished update") || rest.starts_with("update canceled") {
        let e = apps.entry(id).or_default();
        e.running = false;
        e.suspended = rest.contains("(Suspended)");
        e.finished = rest.starts_with("finished update");
    }
}

struct ContentLog {
    path: PathBuf,
    offset: u64,
    apps: HashMap<String, LogApp>,
    grew_at: Option<Instant>,
}

impl ContentLog {
    fn new(root: &Path) -> Self {
        // DI_STEAM_LOG — свой журнал для проверки без настоящей загрузки.
        let path = std::env::var_os("DI_STEAM_LOG").map(PathBuf::from).unwrap_or_else(|| root.join("logs").join("content_log.txt"));
        Self { path, offset: 0, apps: HashMap::new(), grew_at: None }
    }

    /// Дочитывает новые строки. При первом чтении берёт хвост файла,
    /// чтобы подхватить загрузку, начатую до запуска острова.
    fn poll(&mut self) {
        use std::io::{Read, Seek, SeekFrom};
        let Ok(mut f) = std::fs::File::open(&self.path) else { return };
        let meta = f.metadata().ok();
        let len = meta.as_ref().map(|m| m.len()).unwrap_or(0);
        let first = self.grew_at.is_none() && self.offset == 0;
        if len < self.offset {
            self.offset = 0; // журнал начат заново
        }
        if len == self.offset {
            return;
        }
        let start = if first { len.saturating_sub(2 * 1024 * 1024) } else { self.offset };
        if f.seek(SeekFrom::Start(start)).is_err() {
            return;
        }
        let mut buf = Vec::new();
        if f.take(8 * 1024 * 1024).read_to_end(&mut buf).is_err() {
            return;
        }
        // Обрабатываем только целые строки; хвост без \n дочитаем в следующий раз.
        let Some(end) = buf.iter().rposition(|&b| b == b'\n') else { return };
        let text = String::from_utf8_lossy(&buf[..end]);
        let mut lines = text.lines();
        if first && start > 0 {
            lines.next(); // обрезанная первая строка
        }
        for l in lines {
            apply_log_line(&mut self.apps, l);
        }
        self.offset = start + end as u64 + 1;
        // При первом чтении «свежесть» — по времени изменения файла: если Steam закрыт
        // давно, недокачанная загрузка из старого журнала не всплывёт.
        let age = if first { meta.and_then(|m| m.modified().ok()).and_then(|t| t.elapsed().ok()).unwrap_or_default() } else { Duration::ZERO };
        self.grew_at = Some(Instant::now().checked_sub(age).unwrap_or_else(Instant::now));
    }

    /// Журнал свежий: во время загрузки Steam раз в минуту пишет скорость.
    /// Если Steam закрыли посреди загрузки, через 15 минут перестаём её показывать.
    fn fresh(&self) -> bool {
        self.grew_at.map(|t| t.elapsed() < Duration::from_secs(15 * 60)).unwrap_or(false)
    }
}

struct Tracked {
    manifest: Manifest,
    /// Оценка скачанного между записями манифеста.
    estimate: f64,
    last_seen: Instant,
}

/// Самый загруженный интерфейс, байт/с.
struct NetMeter {
    nets: Networks,
    last: Option<(HashMap<String, u64>, Instant)>,
    speed: f64,
}

impl NetMeter {
    fn new() -> Self {
        Self { nets: Networks::new_with_refreshed_list(), last: None, speed: 0.0 }
    }
    fn sample(&mut self) -> f64 {
        self.nets.refresh(true);
        let now = Instant::now();
        let cur: HashMap<String, u64> = self.nets.iter().filter(|(n, _)| !n.to_lowercase().contains("loopback") && !n.starts_with("lo")).map(|(n, d)| (n.clone(), d.total_received())).collect();
        if let Some((prev, at)) = &self.last {
            let secs = now.duration_since(*at).as_secs_f64().max(0.05);
            let max = cur.iter().map(|(n, v)| v.saturating_sub(*prev.get(n).unwrap_or(v)) as f64 / secs).fold(0.0, f64::max);
            self.speed = if self.speed == 0.0 { max } else { self.speed * 0.5 + max * 0.5 };
        }
        self.last = Some((cur, now));
        self.speed
    }
}

fn format_eta(secs: f64) -> String {
    let s = secs.round() as u64;
    if s < 60 {
        format!("{s} с")
    } else if s < 3600 {
        format!("{} мин", s.div_ceil(60))
    } else {
        format!("{} ч {} мин", s / 3600, (s % 3600) / 60)
    }
}

impl SteamModule {
    pub fn set_running(&self, st: &Shared, run: bool) {
        if run == self.task.is_running() {
            return;
        }
        if !run {
            self.task.stop();
            st.events.clear_activity("steam");
            return;
        }
        let st = st.clone();
        self.task.start(async move {
            let Some(root) = tauri::async_runtime::spawn_blocking(steam_root).await.ok().flatten() else { return };
            let mut dirs = library_dirs(&root);
            let mut dirs_at = Instant::now();
            let mut mtimes: HashMap<PathBuf, SystemTime> = HashMap::new();
            let mut manifests: HashMap<PathBuf, Manifest> = HashMap::new();
            let mut tracked: HashMap<String, Tracked> = HashMap::new();
            let mut meter = NetMeter::new();
            let mut clog = Some(ContentLog::new(&root));
            let mut last_tick = Instant::now();
            loop {
                if dirs_at.elapsed() > Duration::from_secs(120) {
                    dirs = library_dirs(&root);
                    dirs_at = Instant::now();
                }
                // Перечитываем только изменившиеся манифесты.
                let d2 = dirs.clone();
                let known = mtimes.clone();
                let mut cl = clog.take().unwrap_or_else(|| ContentLog::new(&root));
                let polled = tauri::async_runtime::spawn_blocking(move || {
                    cl.poll();
                    let mut out = vec![];
                    let mut seen = vec![];
                    for d in d2 {
                        let Ok(rd) = std::fs::read_dir(&d) else { continue };
                        for e in rd.flatten() {
                            let p = e.path();
                            let name = e.file_name().to_string_lossy().to_string();
                            if !(name.starts_with("appmanifest_") && name.ends_with(".acf")) {
                                continue;
                            }
                            let Ok(m) = e.metadata().and_then(|m| m.modified()) else { continue };
                            seen.push(p.clone());
                            if known.get(&p) != Some(&m) {
                                if let Some(man) = std::fs::read_to_string(&p).ok().and_then(|t| parse_manifest(&t)) {
                                    out.push((p, m, man));
                                }
                            }
                        }
                    }
                    (cl, (out, seen))
                })
                .await;
                let Ok((cl, (updates, seen))) = polled else {
                    tokio::time::sleep(Duration::from_secs(3)).await;
                    continue;
                };
                let log = clog.insert(cl);
                mtimes.retain(|k, _| seen.contains(k));
                manifests.retain(|k, _| seen.contains(k));
                for (p, m, man) in updates {
                    mtimes.insert(p.clone(), m);
                    manifests.insert(p, man);
                }

                let speed = meter.sample();
                let dt = last_tick.elapsed().as_secs_f64();
                last_tick = Instant::now();

                // Активные загрузки.
                // Поставленные на паузу показываем, только если видели их загрузку в этом сеансе.
                // Пока журнал Steam свежий, верим ему: манифест во время загрузки не меняется.
                let log_live = log.fresh();
                let mut active: Vec<Manifest> = Vec::new();
                for m in manifests.values() {
                    if let Some(l) = log.apps.get(&m.appid).filter(|_| log_live) {
                        if (l.running || (l.suspended && tracked.contains_key(&m.appid))) && l.total > 0 {
                            let mut x = m.clone();
                            if x.total != l.total {
                                x.downloaded = 0; // в манифесте прошлое обновление
                            }
                            x.total = l.total;
                            x.downloaded = x.downloaded.max(l.downloaded).min(l.total);
                            x.flags = if l.running { (x.flags | FLAG_UPDATE_RUNNING) & !FLAG_UPDATE_PAUSED } else { x.flags | FLAG_UPDATE_PAUSED };
                            active.push(x);
                        }
                        continue;
                    }
                    if m.active() || (m.paused() && tracked.contains_key(&m.appid) && m.downloaded < m.total) {
                        active.push(m.clone());
                    }
                }
                // Новая игра, для которой Steam ещё не записал манифест.
                if log_live {
                    for (id, l) in &log.apps {
                        if l.running && l.total > 0 && !manifests.values().any(|m| &m.appid == id) {
                            let name = tracked.get(id).map(|t| t.manifest.name.clone()).unwrap_or_else(|| "Steam".into());
                            active.push(Manifest { appid: id.clone(), name, flags: FLAG_UPDATE_RUNNING, downloaded: l.downloaded, total: l.total });
                        }
                    }
                }
                for m in &active {
                    let t = tracked.entry(m.appid.clone()).or_insert_with(|| Tracked { manifest: m.clone(), estimate: m.downloaded as f64, last_seen: Instant::now() });
                    if t.manifest.downloaded != m.downloaded || t.manifest.total != m.total {
                        t.estimate = t.estimate.max(m.downloaded as f64).min(m.total as f64);
                        if (m.downloaded as f64) < t.estimate - (m.total as f64 * 0.05) {
                            t.estimate = m.downloaded as f64; // новая загрузка или пересчёт
                        }
                    } else if !m.paused() {
                        t.estimate = (t.estimate + speed * dt).min(m.total as f64 * 0.995).max(m.downloaded as f64);
                    }
                    t.manifest = m.clone();
                    t.last_seen = Instant::now();
                }
                // Завершённые: были в списке, а теперь не активны и скачано всё.
                let finished: Vec<String> = tracked.keys().filter(|id| !active.iter().any(|m| &m.appid == *id)).cloned().collect();
                for id in finished {
                    if let Some(t) = tracked.remove(&id) {
                        let by_log = log.apps.get(&id).map(|l| l.finished).unwrap_or(false);
                        let done = by_log || manifests.values().find(|m| m.appid == id).map(|m| m.done()).unwrap_or(false);
                        if done {
                            st.events.push(IslandNotification {
                                stack: Vec::new(),
                                count: 1,
                                id: uuid::Uuid::new_v4().to_string(),
                                kind: NotificationKind::Internal,
                                source: format!("steam:{id}"),
                                app_name: "Steam".into(),
                                icon: None,
                                title: format!("{} загружена", t.manifest.name),
                                body: format!("{} скачано", format_size(t.manifest.total as f64)),
                                priority: Priority::Normal,
                                timestamp: now_ms(),
                                actions: vec![
                                    NotificationAction { id: "play".into(), label: "Играть".into() },
                                    NotificationAction { id: "dismiss".into(), label: "Закрыть".into() },
                                ],
                                open_target: None,
                                duration_ms: 10_000,
                            });
                        }
                    }
                }

                if tracked.is_empty() {
                    st.events.clear_activity("steam");
                } else {
                    let list: Vec<&Tracked> = tracked.values().collect();
                    let total: f64 = list.iter().map(|t| t.manifest.total as f64).sum();
                    let got: f64 = list.iter().map(|t| t.estimate).sum();
                    let paused = list.iter().all(|t| t.manifest.paused());
                    let left = (total - got).max(0.0);
                    let title = if list.len() == 1 { list[0].manifest.name.clone() } else { format!("Steam: {} загрузки", list.len()) };
                    let mut parts = vec![if paused { "пауза".to_string() } else { format!("{}/с", format_size(speed)) }];
                    parts.push(format!("осталось {}", format_size(left)));
                    if !paused && speed > 64.0 * 1024.0 {
                        parts.push(format_eta(left / speed));
                    }
                    st.events.set_activity(Activity {
                        id: "steam".into(),
                        kind: "steam".into(),
                        priority: Priority::Normal,
                        title,
                        subtitle: Some(parts.join(" · ")),
                        icon: None,
                        progress: Some(if total > 0.0 { (got / total).clamp(0.0, 1.0) } else { 0.0 }),
                        updated_at: now_ms(),
                        started_at: None,
                    });
                }
                let wait = if tracked.is_empty() { 3000 } else { 1000 };
                tokio::time::sleep(Duration::from_millis(wait)).await;
            }
        });
    }
}

/// «Играть» в уведомлении о готовой загрузке.
pub fn action(source: &str, action: &str) -> crate::core::error::AppResult<()> {
    if action == "play" {
        if let Some(id) = source.strip_prefix("steam:") {
            if id.chars().all(|c| c.is_ascii_digit()) {
                crate::platform::os::shell::open_uri(&format!("steam://rungameid/{id}"))?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_manifest_and_libraries() {
        let acf = "\"AppState\"\n{\n\t\"appid\"\t\t\"730\"\n\t\"name\"\t\t\"Counter-Strike 2\"\n\t\"StateFlags\"\t\t\"1048838\"\n\t\"BytesToDownload\"\t\t\"1000\"\n\t\"BytesDownloaded\"\t\t\"250\"\n}";
        let m = parse_manifest(acf).unwrap();
        assert_eq!(m.appid, "730");
        assert_eq!(m.name, "Counter-Strike 2");
        assert!(m.active());
        let done = Manifest { flags: 4, downloaded: 1000, total: 1000, ..m.clone() };
        assert!(!done.active() && done.done());
        let paused = Manifest { flags: 4 | 2 | 512, ..m.clone() };
        assert!(paused.paused() && !paused.done());
        let vdf = "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"C:\\\\Program Files (x86)\\\\Steam\"\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"D:\\\\SteamLibrary\"\n\t}\n}";
        let libs = parse_libraries(vdf);
        assert_eq!(libs, vec![PathBuf::from("C:\\Program Files (x86)\\Steam"), PathBuf::from("D:\\SteamLibrary")]);
        assert_eq!(format_eta(30.0), "30 с");
        assert_eq!(format_eta(150.0), "3 мин");
    }

    #[test]
    fn parses_content_log() {
        let log = "[2026-11-15 16:07:10] AppID 4465480 state changed : Update Required,Update Queued,Update Running,
[2026-11-15 16:07:13] AppID 4465480 update started : download 0/14783871008, store 0/0, reuse 0/0, delta 0/0, stage 0/34423761374 
[2026-11-15 16:07:13] AppID 4465480 state changed : Update Required,Update Queued,Update Running,Update Started,
[2026-11-15 16:08:24] Current download rate: 86.345 Mbps";
        let mut apps = HashMap::new();
        for l in log.lines() {
            apply_log_line(&mut apps, l);
        }
        let a = &apps["4465480"];
        assert!(a.running && !a.suspended && !a.finished);
        assert_eq!((a.downloaded, a.total), (0, 14783871008));

        apply_log_line(&mut apps, "[x] AppID 4465480 state changed : Update Required,Update Queued,Update Started, (Suspended)");
        let a = &apps["4465480"];
        assert!(!a.running && a.suspended && a.total == 14783871008);

        apply_log_line(&mut apps, "[x] AppID 4465480 update started : download 455000000/14783871008, store 0/0");
        apply_log_line(&mut apps, "[x] AppID 4465480 state changed : Update Required,Update Queued,Update Running,Update Started,");
        assert_eq!(apps["4465480"].downloaded, 455000000);

        apply_log_line(&mut apps, "[x] AppID 4465480 finished update, 6 mounted depots (BuildID 22100934)");
        apply_log_line(&mut apps, "[x] AppID 4465480 state changed : Fully Installed,");
        let a = &apps["4465480"];
        assert!(!a.running && !a.suspended && a.finished);
    }
}
