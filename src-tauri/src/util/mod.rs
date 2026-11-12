pub mod clipboard;
pub mod image;

use crate::core::error::{AppError, AppResult};

/// Ограничивает длину строки и убирает управляющие символы.
pub fn clean_text(s: &str, max: usize) -> String {
    s.chars().filter(|c| !c.is_control() || *c == '\n').take(max).collect::<String>().trim().to_string()
}

/// Разрешены только https-URL (и http для localhost) — для веб-сервисов и плагинов.
pub fn validate_web_url(url: &str) -> AppResult<String> {
    let url = url.trim();
    if url.len() > 2048 {
        return Err(AppError::invalid("слишком длинный URL"));
    }
    let lower = url.to_lowercase();
    let ok = lower.starts_with("https://") || lower.starts_with("http://localhost") || lower.starts_with("http://127.0.0.1");
    if !ok {
        return Err(AppError::invalid("разрешены только адреса https://"));
    }
    let rest = &url[url.find("://").map(|i| i + 3).unwrap_or(0)..];
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    if host.is_empty() || host.contains('@') || host.contains(' ') {
        return Err(AppError::invalid("некорректный адрес сайта"));
    }
    Ok(url.to_string())
}

pub fn host_of(url: &str) -> Option<String> {
    let rest = &url[url.find("://")? + 3..];
    let host = rest.split(['/', '?', '#']).next()?;
    Some(host.split(':').next()?.to_lowercase())
}

/// Безопасный идентификатор: [a-z0-9._-], 3..64 символа.
pub fn validate_id(id: &str) -> AppResult<()> {
    let ok = (3..=64).contains(&id.len())
        && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '.' || c == '_' || c == '-')
        && id.chars().next().map(|c| c.is_ascii_alphanumeric()).unwrap_or(false);
    if ok {
        Ok(())
    } else {
        Err(AppError::invalid(format!("недопустимый идентификатор «{id}»")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls() {
        assert!(validate_web_url("https://claude.ai/new").is_ok());
        assert!(validate_web_url("http://evil.com").is_err());
        assert!(validate_web_url("javascript:alert(1)").is_err());
        assert!(validate_web_url("https://user@evil.com").is_err());
        assert_eq!(host_of("https://Chat.OpenAI.com:443/x").as_deref(), Some("chat.openai.com"));
    }

    #[test]
    fn ids() {
        assert!(validate_id("com.example.hello").is_ok());
        assert!(validate_id("../etc").is_err());
        assert!(validate_id("A").is_err());
    }
}
