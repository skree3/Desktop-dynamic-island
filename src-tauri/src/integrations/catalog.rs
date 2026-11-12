//! Каталог известных приложений с честным уровнем поддержки для каждой ОС.
//! Каталог не ограничивает список программ: любые приложения находятся
//! поиском и могут быть добавлены вручную.

use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SupportLevel {
    /// Полноценное управление.
    Full,
    /// Частичная интеграция.
    Partial,
    /// Только уведомления (+ запуск).
    NotificationsOnly,
    /// Только запуск приложения.
    LaunchOnly,
    /// Только веб-версия во встроенном окне.
    WebOnly,
}

impl SupportLevel {
    pub fn label(self) -> &'static str {
        match self {
            SupportLevel::Full => "Полноценное управление",
            SupportLevel::Partial => "Частичная интеграция",
            SupportLevel::NotificationsOnly => "Только уведомления",
            SupportLevel::LaunchOnly => "Только запуск приложения",
            SupportLevel::WebOnly => "Только веб-версия",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct KnownApp {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    /// Подстроки (нижний регистр) для поиска среди установленных приложений.
    pub match_names: &'static [&'static str],
    /// Имена процессов без расширения (нижний регистр).
    pub processes: &'static [&'static str],
    pub protocol: Option<&'static str>,
    pub web_url: Option<&'static str>,
    pub windows: (SupportLevel, &'static str),
    pub linux: (SupportLevel, &'static str),
    /// Уровень меняется на «уведомления», если ОС разрешает читать уведомления.
    pub notification_capable: bool,
    pub media_session: bool,
}

const BROWSER_MEDIA: &str = "Через системную медиасессию браузера: метаданные, обложка, пауза, переключение треков. Управление вкладками недоступно — браузер не предоставляет такого API.";

pub const CATALOG: &[KnownApp] = &[
    KnownApp {
        id: "spotify", name: "Spotify", category: "music", match_names: &["spotify"], processes: &["spotify"], protocol: Some("spotify:"), web_url: Some("https://open.spotify.com"),
        windows: (SupportLevel::Full, "SMTC: метаданные, обложка, прогресс, пауза, треки, перемотка."),
        linux: (SupportLevel::Full, "MPRIS: метаданные, обложка, прогресс, управление, громкость плеера."),
        notification_capable: false, media_session: true,
    },
    KnownApp {
        id: "youtube", name: "YouTube", category: "music", match_names: &[], processes: &[], protocol: None, web_url: Some("https://www.youtube.com"),
        windows: (SupportLevel::Partial, BROWSER_MEDIA), linux: (SupportLevel::Partial, BROWSER_MEDIA),
        notification_capable: false, media_session: true,
    },
    KnownApp {
        id: "youtube_music", name: "YouTube Music", category: "music", match_names: &["youtube music"], processes: &[], protocol: None, web_url: Some("https://music.youtube.com"),
        windows: (SupportLevel::Partial, BROWSER_MEDIA), linux: (SupportLevel::Partial, BROWSER_MEDIA),
        notification_capable: false, media_session: true,
    },
    KnownApp {
        id: "yandex_music", name: "Яндекс Музыка", category: "music", match_names: &["яндекс музыка", "yandex music", "яндекс.музыка"], processes: &["яндекс музыка", "yandexmusic", "yandex music"], protocol: None, web_url: Some("https://music.yandex.ru"),
        windows: (SupportLevel::Partial, "Приложение и веб-версия публикуют медиасессию SMTC: метаданные и управление воспроизведением."),
        linux: (SupportLevel::Partial, "Веб-версия через MPRIS браузера; официального клиента для Linux нет."),
        notification_capable: false, media_session: true,
    },
    KnownApp {
        id: "soundcloud", name: "SoundCloud", category: "music", match_names: &["soundcloud"], processes: &[], protocol: None, web_url: Some("https://soundcloud.com"),
        windows: (SupportLevel::Partial, BROWSER_MEDIA), linux: (SupportLevel::Partial, BROWSER_MEDIA),
        notification_capable: false, media_session: true,
    },
    KnownApp {
        id: "vlc", name: "VLC", category: "music", match_names: &["vlc media player", "vlc"], processes: &["vlc"], protocol: None, web_url: None,
        windows: (SupportLevel::LaunchOnly, "VLC 3.x для Windows не публикует медиасессию SMTC — доступен только запуск. Если версия VLC публикует SMTC, управление появится автоматически."),
        linux: (SupportLevel::Full, "MPRIS: метаданные, управление, перемотка, громкость."),
        notification_capable: false, media_session: true,
    },
    KnownApp {
        id: "chrome", name: "Google Chrome", category: "browser", match_names: &["google chrome"], processes: &["chrome"], protocol: None, web_url: None,
        windows: (SupportLevel::Partial, BROWSER_MEDIA), linux: (SupportLevel::Partial, BROWSER_MEDIA),
        notification_capable: false, media_session: true,
    },
    KnownApp {
        id: "edge", name: "Microsoft Edge", category: "browser", match_names: &["microsoft edge"], processes: &["msedge", "microsoft-edge"], protocol: Some("microsoft-edge:"), web_url: None,
        windows: (SupportLevel::Partial, BROWSER_MEDIA), linux: (SupportLevel::Partial, BROWSER_MEDIA),
        notification_capable: false, media_session: true,
    },
    KnownApp {
        id: "firefox", name: "Firefox", category: "browser", match_names: &["firefox"], processes: &["firefox"], protocol: None, web_url: None,
        windows: (SupportLevel::Partial, BROWSER_MEDIA), linux: (SupportLevel::Partial, BROWSER_MEDIA),
        notification_capable: false, media_session: true,
    },
    KnownApp {
        id: "telegram", name: "Telegram", category: "chat", match_names: &["telegram"], processes: &["telegram", "telegram-desktop", "ayugram"], protocol: Some("tg://"), web_url: Some("https://web.telegram.org"),
        windows: (SupportLevel::LaunchOnly, "Запуск и ссылки tg://. Уведомления — если Windows разрешает их чтение (UserNotificationListener; зависит от версии Windows и настроек конфиденциальности)."),
        linux: (SupportLevel::NotificationsOnly, "Уведомления через D-Bus, запуск, ссылки tg://."),
        notification_capable: true, media_session: false,
    },
    KnownApp {
        id: "discord", name: "Discord", category: "chat", match_names: &["discord"], processes: &["discord", "discordptb", "discordcanary"], protocol: Some("discord://"), web_url: Some("https://discord.com/app"),
        windows: (SupportLevel::LaunchOnly, "Запуск и ссылки discord://. Уведомления — при доступе к уведомлениям Windows (UserNotificationListener)."),
        linux: (SupportLevel::NotificationsOnly, "Уведомления через D-Bus, запуск, ссылки discord://."),
        notification_capable: true, media_session: false,
    },
    KnownApp {
        id: "whatsapp", name: "WhatsApp", category: "chat", match_names: &["whatsapp"], processes: &["whatsapp", "whatsapp.root"], protocol: Some("whatsapp://"), web_url: Some("https://web.whatsapp.com"),
        windows: (SupportLevel::LaunchOnly, "Приложение из Microsoft Store: запуск. Уведомления — при доступе к уведомлениям Windows (UserNotificationListener)."),
        linux: (SupportLevel::WebOnly, "Официального клиента для Linux нет — веб-версия во встроенном окне; уведомления браузера приходят через D-Bus."),
        notification_capable: true, media_session: false,
    },
    KnownApp {
        id: "steam", name: "Steam", category: "games", match_names: &["steam"], processes: &["steam", "steamwebhelper"], protocol: Some("steam://"), web_url: None,
        windows: (SupportLevel::Partial, "Запуск, ссылки steam://, распознавание игр Steam (steamapps\\common) для игрового режима."),
        linux: (SupportLevel::Partial, "Запуск, ссылки steam://, распознавание игр Steam/Proton для игрового режима."),
        notification_capable: false, media_session: false,
    },
    KnownApp {
        id: "happ", name: "Happ", category: "proxy", match_names: &["happ"], processes: &["happ"], protocol: None, web_url: Some("https://github.com/Happ-proxy"),
        windows: (SupportLevel::Partial, "Только чтение: запущено ли приложение и включён ли системный прокси. Документированного API управления подключением нет — управление не реализовано."),
        linux: (SupportLevel::Partial, "Только чтение: статус процесса и системного прокси. Управление подключением — в самом приложении."),
        notification_capable: false, media_session: false,
    },
    KnownApp {
        id: "incy", name: "INCY", category: "proxy", match_names: &["incy"], processes: &["incy"], protocol: None, web_url: Some("https://github.com/INCY-DEV/incy-platforms"),
        windows: (SupportLevel::Partial, "Только чтение: статус процесса и системного прокси. Документированного API управления подключением нет — управление не реализовано."),
        linux: (SupportLevel::Partial, "Только чтение: статус процесса и системного прокси. Управление подключением — в самом приложении."),
        notification_capable: false, media_session: false,
    },
    KnownApp {
        id: "claude", name: "Claude", category: "ai", match_names: &["claude"], processes: &["claude"], protocol: None, web_url: Some("https://claude.ai"),
        windows: (SupportLevel::Partial, "Запуск установленного приложения и веб-версия в изолированном мини-окне."),
        linux: (SupportLevel::WebOnly, "Веб-версия в изолированном мини-окне."),
        notification_capable: false, media_session: false,
    },
    KnownApp {
        id: "chatgpt", name: "ChatGPT", category: "ai", match_names: &["chatgpt"], processes: &["chatgpt"], protocol: None, web_url: Some("https://chatgpt.com"),
        windows: (SupportLevel::Partial, "Запуск установленного приложения и веб-версия в изолированном мини-окне."),
        linux: (SupportLevel::WebOnly, "Веб-версия в изолированном мини-окне."),
        notification_capable: false, media_session: false,
    },
    KnownApp {
        id: "deepseek", name: "DeepSeek", category: "ai", match_names: &["deepseek"], processes: &["deepseek"], protocol: None, web_url: Some("https://chat.deepseek.com"),
        windows: (SupportLevel::WebOnly, "Веб-версия в изолированном мини-окне."),
        linux: (SupportLevel::WebOnly, "Веб-версия в изолированном мини-окне."),
        notification_capable: false, media_session: false,
    },
    KnownApp {
        id: "gemini", name: "Gemini", category: "ai", match_names: &["gemini"], processes: &[], protocol: None, web_url: Some("https://gemini.google.com"),
        windows: (SupportLevel::WebOnly, "Веб-версия в изолированном мини-окне."),
        linux: (SupportLevel::WebOnly, "Веб-версия в изолированном мини-окне."),
        notification_capable: false, media_session: false,
    },
];

pub fn get(id: &str) -> Option<&'static KnownApp> {
    CATALOG.iter().find(|k| k.id == id)
}

pub fn platform_support(k: &KnownApp) -> (SupportLevel, &'static str) {
    if cfg!(windows) {
        k.windows
    } else {
        k.linux
    }
}

/// Сопоставление установленного приложения с каталогом.
pub fn match_app(name: &str, exe: Option<&str>) -> Option<&'static KnownApp> {
    let n = name.to_lowercase();
    let stem = exe
        .and_then(|e| std::path::Path::new(e).file_stem().map(|s| s.to_string_lossy().to_lowercase()))
        .unwrap_or_default();
    CATALOG.iter().find(|k| {
        (!stem.is_empty() && k.processes.contains(&stem.as_str()))
            || k.match_names.iter().any(|m| n == *m || n.starts_with(&format!("{m} ")) || (m.contains(' ') && n.contains(m)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching() {
        assert_eq!(match_app("Spotify", None).map(|k| k.id), Some("spotify"));
        #[cfg(windows)]
        let (tg, np) = (r"C:\x\Telegram.exe", r"C:\Windows\notepad.exe");
        #[cfg(not(windows))]
        let (tg, np) = ("/opt/x/Telegram", "/usr/bin/notepad");
        assert_eq!(match_app("Something", Some(tg)).map(|k| k.id), Some("telegram"));
        assert!(match_app("Notepad", Some(np)).is_none());
        // «steam» не должен совпадать со словом внутри другого названия
        assert!(match_app("GeoGuessr Steam Edition", None).is_none());
        assert!(match_app("Steamworks Common Redistributables", None).is_none());
        assert_eq!(match_app("Яндекс Музыка", None).map(|k| k.id), Some("yandex_music"));
    }

    #[test]
    fn every_entry_has_notes() {
        for k in CATALOG {
            assert!(!k.windows.1.is_empty() && !k.linux.1.is_empty(), "{}", k.id);
        }
    }
}
