//! Двухшаговое подтверждение опасных действий (выключение, перезагрузка,
//! очистка истории). UI сначала запрашивает одноразовый токен, затем —
//! после явного подтверждения пользователя — выполняет действие с этим токеном.
//! Токен живёт 60 секунд и привязан к конкретному действию.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::core::error::{AppError, AppResult};

const TTL: Duration = Duration::from_secs(60);

#[derive(Default)]
pub struct ConfirmStore {
    tokens: Mutex<HashMap<String, (String, Instant)>>,
}

impl ConfirmStore {
    pub fn issue(&self, action: &str) -> String {
        let token = uuid::Uuid::new_v4().to_string();
        let mut g = self.tokens.lock().unwrap_or_else(|p| p.into_inner());
        g.retain(|_, (_, t)| t.elapsed() < TTL);
        g.insert(token.clone(), (action.to_string(), Instant::now()));
        token
    }

    pub fn consume(&self, token: &str, action: &str) -> AppResult<()> {
        let mut g = self.tokens.lock().unwrap_or_else(|p| p.into_inner());
        match g.remove(token) {
            Some((a, t)) if a == action && t.elapsed() < TTL => Ok(()),
            Some(_) => Err(AppError::PermissionDenied("токен подтверждения недействителен или истёк".into())),
            None => Err(AppError::PermissionDenied("действие не было подтверждено пользователем".into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_is_single_use_and_bound_to_action() {
        let s = ConfirmStore::default();
        let t = s.issue("shutdown");
        assert!(s.consume(&t, "reboot").is_err());
        let t = s.issue("shutdown");
        assert!(s.consume(&t, "shutdown").is_ok());
        assert!(s.consume(&t, "shutdown").is_err());
    }
}
