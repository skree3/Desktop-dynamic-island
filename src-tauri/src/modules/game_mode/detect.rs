//! Распознавание игр. Панель скрывается ТОЛЬКО во время игр:
//! * игра из списка пользователя;
//! * путь установки игры (Steam, Epic, GOG, Xbox, Riot, Lesta, Gaijin…);
//! * процесс запущен игровым лаунчером (Steam, Epic, Lesta Game Center…);
//! * эксклюзивный полноэкранный режим Direct3D;
//! * полноэкранное окно приложения, которое не является браузером, плеером,
//!   офисом, мессенджером или средой разработки (borderless-игры без лаунчера).
//! Обычное полноэкранное видео в браузере, презентации и т.п. панель не скрывают.

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, UpdateKind};

use super::GameModeState;
use crate::core::settings::GameModeSettings;
use crate::platform::os;

/// Фрагменты путей, характерные для установленных игр.
const GAME_PATHS: &[&str] = &[
    "\\steamapps\\common\\",
    "/steamapps/common/",
    "\\epic games\\",
    "\\gog galaxy\\games\\",
    "\\gog games\\",
    "\\riot games\\",
    "\\xboxgames\\",
    "\\windowsapps\\microsoft.minecraft",
    "\\ea games\\",
    "\\ubisoft game launcher\\games\\",
    "\\battlestate games\\",
    "\\lesta\\",
    "\\wargaming.net\\",
    "\\gaijin\\",
    "\\war thunder\\",
    "\\.minecraft\\",
    "\\tlauncher\\",
    "/lutris/",
    "/games/heroic/",
    "/.wine/drive_c/",
    "/compatdata/",
];

/// Лаунчеры: процесс, запущенный ими, считается игрой.
const LAUNCHERS: &[&str] = &[
    "steam",
    "epicgameslauncher",
    "galaxyclient",
    "riotclientservices",
    "eadesktop",
    "origin",
    "upc",
    "ubisoftconnect",
    "battle.net",
    "lgc",
    "lestagamecenter",
    "wgc",
    "gaijin.net launcher",
    "launcher",
    "tlauncher",
    "minecraftlauncher",
    "heroic",
    "lutris",
    "gamescope",
    "faceit",
];

/// Приложения, полноэкранный режим которых — не игра.
const NOT_GAMES: &[&str] = &[
    "chrome", "msedge", "firefox", "opera", "opera_gx", "brave", "browser", "vivaldi", "yandex", "chromium", "arc", "zen",
    "vlc", "mpc-hc64", "mpc-hc", "mpc-be64", "potplayermini64", "wmplayer", "video.ui", "microsoft.media.player", "mpv", "kmplayer",
    "powerpnt", "winword", "excel", "onenote", "outlook", "acrord32", "acrobat", "sumatrapdf",
    "telegram", "discord", "whatsapp", "teams", "ms-teams", "skype", "zoom",
    "code", "cursor", "devenv", "idea64", "pycharm64", "obsidian", "notepad", "notepads",
    "explorer", "applicationframehost", "searchhost", "startmenuexperiencehost", "shellexperiencehost", "lockapp",
    "obs64", "obs", "photos", "microsoft.photos", "lively", "rainmeter", "desktop-island", "claude", "chatgpt",
    "totalcmd64", "freecommander", "mstsc", "vmware", "virtualbox", "bluestacks", "hd-player", "taskmgr",
];

#[derive(Default)]
pub struct Detector {
    sys: Option<sysinfo::System>,
    last_proc_game: Option<String>,
}

fn stem(name: &str) -> String {
    let n = name.to_lowercase();
    n.strip_suffix(".exe").map(|s| s.to_string()).unwrap_or(n)
}

pub fn is_game_path(path: &str) -> bool {
    let p = path.to_lowercase();
    GAME_PATHS.iter().any(|g| p.contains(g))
}

pub fn is_known_non_game(exe_name: &str) -> bool {
    let s = stem(exe_name);
    NOT_GAMES.iter().any(|n| s == *n)
}

fn matches_list(list: &[String], exe_name: &str, exe_path: &str) -> bool {
    let n = exe_name.to_lowercase();
    let p = exe_path.to_lowercase();
    list.iter().any(|e| {
        let e = e.to_lowercase();
        !e.is_empty() && (n == e || stem(&n) == stem(&e) || (e.contains(['\\', '/']) && p == e))
    })
}

impl Detector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Имена родительских процессов (до 3 уровней) — для распознавания лаунчеров.
    fn ancestors(&mut self, pid: u32) -> Vec<String> {
        let sys = self.sys.get_or_insert_with(sysinfo::System::new);
        let mut out = vec![];
        let mut cur = Pid::from_u32(pid);
        for _ in 0..3 {
            sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[cur]), false, ProcessRefreshKind::nothing());
            let Some(parent) = sys.process(cur).and_then(|p| p.parent()) else { break };
            sys.refresh_processes_specifics(ProcessesToUpdate::Some(&[parent]), false, ProcessRefreshKind::nothing());
            let Some(pp) = sys.process(parent) else { break };
            out.push(stem(&pp.name().to_string_lossy()));
            cur = parent;
        }
        out
    }

    /// Поиск игровых процессов (используется, когда активное окно недоступно — Wayland).
    fn process_scan(&mut self, cfg: &GameModeSettings) -> Option<String> {
        let sys = self.sys.get_or_insert_with(sysinfo::System::new);
        sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet));
        for p in sys.processes().values() {
            let name = p.name().to_string_lossy().to_lowercase();
            let path = p.exe().map(|e| e.to_string_lossy().to_string()).unwrap_or_default();
            if matches_list(&cfg.exclusions, &name, &path) {
                continue;
            }
            let linux_game = cfg!(target_os = "linux") && (["gamescope", "wine64-preloader", "wine-preloader"].contains(&name.as_str()) || is_game_path(&path));
            if matches_list(&cfg.extra_games, &name, &path) || linux_game {
                return Some(name);
            }
        }
        None
    }

    pub fn detect(&mut self, cfg: &GameModeSettings, tick: u64) -> GameModeState {
        let own = std::process::id();
        match os::foreground::current() {
            Some(fg) if fg.pid != own && !fg.is_shell => {
                if matches_list(&cfg.exclusions, &fg.exe_name, &fg.exe_path) || !cfg.detect_games {
                    return GameModeState::default();
                }
                let app = Some(fg.exe_name.clone()).filter(|s| !s.is_empty());
                let game = |reason: &str| GameModeState { active: true, reason: Some(reason.into()), app: app.clone() };
                if matches_list(&cfg.extra_games, &fg.exe_name, &fg.exe_path) || is_game_path(&fg.exe_path) {
                    return game("game");
                }
                if is_known_non_game(&fg.exe_name) {
                    return GameModeState::default();
                }
                if fg.exclusive_fullscreen {
                    return game("exclusive");
                }
                if fg.fullscreen {
                    let parents = self.ancestors(fg.pid);
                    if parents.iter().any(|p| LAUNCHERS.contains(&p.as_str())) {
                        return game("launcher");
                    }
                    if cfg.detect_fullscreen {
                        return game("fullscreen-game");
                    }
                }
                GameModeState::default()
            }
            Some(_) => GameModeState::default(),
            None => {
                if cfg.detect_games && (tick % 3 == 0 || self.last_proc_game.is_some()) {
                    self.last_proc_game = self.process_scan(cfg);
                }
                match &self.last_proc_game {
                    Some(name) => GameModeState { active: true, reason: Some("game".into()), app: Some(name.clone()) },
                    None => GameModeState::default(),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_paths() {
        assert!(is_game_path(r"D:\SteamLibrary\steamapps\common\Hades\Hades.exe"));
        assert!(is_game_path("/home/u/.local/share/Steam/steamapps/common/Game/game.x86_64"));
        assert!(!is_game_path(r"C:\Program Files\Mozilla Firefox\firefox.exe"));
        assert!(!is_game_path(r"D:\Games\Telegram Desktop\Telegram.exe"));
    }

    #[test]
    fn non_games_never_trigger() {
        for exe in ["chrome.exe", "msedge.exe", "vlc.exe", "POWERPNT.EXE", "Telegram.exe", "explorer.exe"] {
            assert!(is_known_non_game(exe), "{exe}");
        }
        assert!(!is_known_non_game("aces.exe"));
    }

    #[test]
    fn list_matching() {
        let list = vec!["Game.exe".to_string()];
        assert!(matches_list(&list, "game.exe", "C:/x/game.exe"));
        assert!(matches_list(&vec!["game".to_string()], "Game.exe", ""));
        assert!(!matches_list(&list, "other.exe", "C:/x/other.exe"));
    }
}
