//! Встроенные плагины: поставляются внутри приложения и устанавливаются
//! при первом запуске (или обновляются при выходе новой версии). Включаются
//! пользователем в «Настройки → Плагины», как и любые другие.

use crate::core::state::Shared;

type Files = &'static [(&'static str, &'static str)];

const COMMON_CSS: &str = include_str!("../../builtin-plugins/common.css");

const BUILTIN: &[(&str, Files)] = &[
    (
        "builtin.pomodoro",
        &[
            ("manifest.json", include_str!("../../builtin-plugins/pomodoro/manifest.json")),
            ("index.html", include_str!("../../builtin-plugins/pomodoro/index.html")),
            ("main.js", include_str!("../../builtin-plugins/pomodoro/main.js")),
            ("common.css", COMMON_CSS),
        ],
    ),
    (
        "builtin.habits",
        &[
            ("manifest.json", include_str!("../../builtin-plugins/habits/manifest.json")),
            ("index.html", include_str!("../../builtin-plugins/habits/index.html")),
            ("main.js", include_str!("../../builtin-plugins/habits/main.js")),
            ("common.css", COMMON_CSS),
        ],
    ),
    (
        "builtin.system-pulse",
        &[
            ("manifest.json", include_str!("../../builtin-plugins/system-pulse/manifest.json")),
            ("index.html", include_str!("../../builtin-plugins/system-pulse/index.html")),
            ("main.js", include_str!("../../builtin-plugins/system-pulse/main.js")),
            ("style.css", include_str!("../../builtin-plugins/system-pulse/style.css")),
        ],
    ),
];

pub fn ensure(st: &Shared) {
    for (id, files) in BUILTIN {
        let Some(manifest) = files.iter().find(|(n, _)| *n == "manifest.json").map(|(_, c)| *c) else { continue };
        let Ok(m) = super::Manifest::parse(manifest) else {
            tracing::error!(%id, "встроенный плагин: некорректный манифест");
            continue;
        };
        let installed = st.plugins.get(id);
        let needs = match &installed {
            None => true,
            Some(p) => {
                let have = semver::Version::parse(&p.manifest.version).ok();
                let new = semver::Version::parse(&m.version).ok();
                new > have || p.missing
            }
        };
        if !needs {
            continue;
        }
        let tmp = std::env::temp_dir().join(format!("desktop-island-builtin-{id}"));
        let _ = std::fs::remove_dir_all(&tmp);
        if std::fs::create_dir_all(&tmp).is_err() {
            continue;
        }
        let ok = files.iter().all(|(name, content)| std::fs::write(tmp.join(name), content).is_ok());
        if ok {
            if let Err(e) = st.plugins.install(st, &tmp) {
                tracing::error!(%id, error = %e, "не удалось установить встроенный плагин");
            }
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }
}

pub fn is_builtin(id: &str) -> bool {
    BUILTIN.iter().any(|(b, _)| *b == id)
}
