//! Эвристика «чувствительных» строк: пароли, API-ключи, токены, номера карт.
//! Лучше не сохранить лишнее, чем сохранить пароль — фильтр консервативен
//! и отключается в настройках.

const TOKEN_PREFIXES: &[&str] = &[
    "sk-", "sk_live_", "sk_test_", "pk_live_", "rk_live_", "ghp_", "gho_", "ghu_", "ghs_", "github_pat_", "glpat-", "xoxb-", "xoxp-",
    "AKIA", "ASIA", "AIza", "ya29.", "eyJhbGciOi", "-----BEGIN", "npm_", "pypi-", "hf_",
];

fn luhn_valid(digits: &[u32]) -> bool {
    let mut sum = 0;
    for (i, d) in digits.iter().rev().enumerate() {
        let mut v = *d;
        if i % 2 == 1 {
            v *= 2;
            if v > 9 {
                v -= 9;
            }
        }
        sum += v;
    }
    sum % 10 == 0
}

pub fn looks_sensitive(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return false;
    }
    if TOKEN_PREFIXES.iter().any(|p| t.starts_with(p)) || t.contains("PRIVATE KEY-----") {
        return true;
    }
    // Номер банковской карты (13–19 цифр, допускаются пробелы/дефисы) с проверкой Луна.
    let compact: String = t.chars().filter(|c| !c.is_whitespace() && *c != '-').collect();
    if (13..=19).contains(&compact.len()) && compact.chars().all(|c| c.is_ascii_digit()) {
        let digits: Vec<u32> = compact.chars().filter_map(|c| c.to_digit(10)).collect();
        if luhn_valid(&digits) {
            return true;
        }
    }
    // Похоже на пароль: одно «слово» 8–64 символа, есть строчные, заглавные, цифры и спецсимволы.
    if !t.contains(char::is_whitespace) && (8..=64).contains(&t.chars().count()) && !t.contains("://") && !t.contains('@') {
        let lower = t.chars().any(|c| c.is_lowercase());
        let upper = t.chars().any(|c| c.is_uppercase());
        let digit = t.chars().any(|c| c.is_ascii_digit());
        let symbol = t.chars().any(|c| !c.is_alphanumeric());
        if lower && upper && digit && symbol {
            return true;
        }
        // Длинные случайные строки (ключи) — высокая энтропия, без гласных-паттернов.
        if t.len() >= 32 && t.chars().all(|c| c.is_ascii_alphanumeric() || "-_=+/".contains(c)) && digit && (lower || upper) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::looks_sensitive;

    #[test]
    fn detects_secrets() {
        assert!(looks_sensitive("sk-ant-api03-abcdef"));
        assert!(looks_sensitive("ghp_1234567890abcdefghijklmnopqrstuvwxyzAB"));
        assert!(looks_sensitive("4111 1111 1111 1111"));
        assert!(looks_sensitive("Tr0ub4dor&3x!"));
        assert!(looks_sensitive("a8F3kL0pQz7xY2mN4bV6cR1tS9wE5uJ0"));
    }

    #[test]
    fn keeps_normal_text() {
        assert!(!looks_sensitive("Привет, как дела?"));
        assert!(!looks_sensitive("https://example.com/Path?a=1"));
        assert!(!looks_sensitive("user@example.com"));
        assert!(!looks_sensitive("1234 5678"));
        assert!(!looks_sensitive("HelloWorld"));
    }
}
