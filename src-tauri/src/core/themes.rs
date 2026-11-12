//! Темы оформления. Тема содержит две палитры (тёмную и светлую); какая из
//! них используется — определяет `appearance.mode` (dark / light / system).
//! Пользовательские темы импортируются из JSON и строго валидируются:
//! допускаются только известные ключи и значения цвета (#hex / rgb[a]()).

use serde_json::{json, Value};

use crate::core::error::{AppError, AppResult};
use crate::core::state::Shared;
use crate::database::repo::ThemeRow;

pub const COLOR_KEYS: &[&str] = &["bg", "surface", "surfaceHover", "border", "text", "textMuted", "accent", "accentText", "danger", "success", "warning"];

fn palette(bg: &str, surface: &str, hover: &str, border: &str, text: &str, muted: &str, accent: &str) -> Value {
    json!({
        "bg": bg, "surface": surface, "surfaceHover": hover, "border": border,
        "text": text, "textMuted": muted, "accent": accent, "accentText": "#ffffff",
        "danger": "#ff5c5c", "success": "#3ccf91", "warning": "#ffb547"
    })
}

pub fn builtin() -> Vec<ThemeRow> {
    let light = |accent: &str| palette("#f7f7f9", "rgba(0,0,0,0.05)", "rgba(0,0,0,0.09)", "rgba(0,0,0,0.08)", "#1b1b1f", "#5f6068", accent);
    let t = |id: &str, name: &str, dark: Value, light: Value| ThemeRow {
        id: id.into(),
        name: name.into(),
        builtin: true,
        data: json!({ "dark": dark, "light": light }),
    };
    vec![
        t("midnight", "Полночь", palette("#000000", "rgba(255,255,255,0.07)", "rgba(255,255,255,0.12)", "rgba(255,255,255,0.08)", "#f5f5f7", "#9a9aa2", "#4c9aff"), light("#0a64d8")),
        t("graphite", "Графит", palette("#1c1c1e", "rgba(255,255,255,0.07)", "rgba(255,255,255,0.12)", "rgba(255,255,255,0.09)", "#ececf1", "#9b9ba5", "#a1a1aa"), light("#52525b")),
        t("ocean", "Океан", palette("#04131f", "rgba(120,200,255,0.08)", "rgba(120,200,255,0.14)", "rgba(120,200,255,0.12)", "#e6f4ff", "#8fb3c9", "#2fb4ff"), light("#0077b6")),
        t("forest", "Лес", palette("#06140d", "rgba(120,255,180,0.07)", "rgba(120,255,180,0.13)", "rgba(120,255,180,0.1)", "#e8fff1", "#8fbfa3", "#3ccf91"), light("#13865a")),
        t("sunset", "Закат", palette("#170a06", "rgba(255,170,120,0.08)", "rgba(255,170,120,0.14)", "rgba(255,170,120,0.11)", "#fff1e8", "#c9a08f", "#ff7a45"), light("#d9480f")),
        t("rose", "Роза", palette("#16060d", "rgba(255,130,180,0.08)", "rgba(255,130,180,0.14)", "rgba(255,130,180,0.11)", "#ffe8f1", "#c98fa8", "#ff5fa2"), light("#c2255c")),
        t("windows", "Windows 11", palette("#202020", "rgba(255,255,255,0.06)", "rgba(255,255,255,0.1)", "rgba(255,255,255,0.08)", "#ffffff", "#a0a0a0", "#60cdff"), light("#005fb8")),
    ]
}

pub fn ensure_builtin(st: &Shared) {
    // Импорт тем отключён — доступны только встроенные.
    let _ = st.db.conn().execute("DELETE FROM themes WHERE builtin = 0", []);
    let ids: Vec<String> = builtin().into_iter().map(|t| t.id).collect();
    if !ids.contains(&st.with_settings(|s| s.appearance.theme_id.clone())) {
        let _ = st.db.conn().execute(
            "UPDATE settings SET value = json_set(value, '$.themeId', 'midnight') WHERE key = 'appearance'",
            [],
        );
    }
    for t in builtin() {
        let _ = st.db.upsert_theme(&t);
    }
}

fn valid_color(s: &str) -> bool {
    let s = s.trim();
    if s.len() > 40 {
        return false;
    }
    if let Some(hex) = s.strip_prefix('#') {
        return matches!(hex.len(), 3 | 4 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit());
    }
    let inner = s.strip_prefix("rgba(").or_else(|| s.strip_prefix("rgb(")).and_then(|r| r.strip_suffix(')'));
    match inner {
        Some(i) => {
            let parts: Vec<&str> = i.split(',').map(|p| p.trim()).collect();
            (parts.len() == 3 || parts.len() == 4) && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit() || c == '.'))
        }
        None => false,
    }
}

fn clean_palette(v: &Value) -> AppResult<Value> {
    let obj = v.as_object().ok_or_else(|| AppError::invalid("палитра должна быть объектом"))?;
    let mut out = serde_json::Map::new();
    for key in COLOR_KEYS {
        if let Some(c) = obj.get(*key) {
            let s = c.as_str().ok_or_else(|| AppError::invalid(format!("цвет «{key}» должен быть строкой")))?;
            if !valid_color(s) {
                return Err(AppError::invalid(format!("недопустимый цвет «{key}»: {s}")));
            }
            out.insert(key.to_string(), Value::String(s.trim().to_string()));
        }
    }
    if !out.contains_key("bg") || !out.contains_key("text") || !out.contains_key("accent") {
        return Err(AppError::invalid("палитра должна содержать как минимум bg, text и accent"));
    }
    Ok(Value::Object(out))
}

/// Импорт темы из JSON: { "id", "name", "dark": {...}, "light": {...} }.
pub fn import(st: &Shared, json_text: &str) -> AppResult<ThemeRow> {
    if json_text.len() > 32 * 1024 {
        return Err(AppError::invalid("файл темы слишком большой"));
    }
    let v: Value = serde_json::from_str(json_text)?;
    let id = v.get("id").and_then(|x| x.as_str()).ok_or_else(|| AppError::invalid("нет поля id"))?.to_string();
    crate::util::validate_id(&id)?;
    if builtin().iter().any(|b| b.id == id) {
        return Err(AppError::invalid("нельзя перезаписать встроенную тему"));
    }
    let name = crate::util::clean_text(v.get("name").and_then(|x| x.as_str()).unwrap_or(&id), 64);
    let dark = v.get("dark").map(clean_palette).transpose()?;
    let light = v.get("light").map(clean_palette).transpose()?;
    if dark.is_none() && light.is_none() {
        return Err(AppError::invalid("тема должна содержать палитру dark и/или light"));
    }
    let data = json!({ "dark": dark.clone().or(light.clone()), "light": light.or(dark) });
    let row = ThemeRow { id, name, builtin: false, data };
    st.db.upsert_theme(&row)?;
    Ok(row)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_validation() {
        assert!(valid_color("#fff"));
        assert!(valid_color("rgba(255, 255, 255, 0.5)"));
        assert!(!valid_color("red;background:url(javascript:1)"));
        assert!(!valid_color("expression(alert(1))"));
    }

    #[test]
    fn palette_requires_core_keys() {
        assert!(clean_palette(&json!({"bg": "#000"})).is_err());
        let p = clean_palette(&json!({"bg": "#000", "text": "#fff", "accent": "#f00", "evil": "x"})).unwrap();
        assert!(p.get("evil").is_none());
    }
}
