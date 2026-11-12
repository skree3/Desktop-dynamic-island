//! Один общий экземпляр arboard на всё приложение.
//!
//! `arboard::Clipboard::new()` в Linux при заданном `WAYLAND_DISPLAY` каждый раз
//! пробует протокол Wayland data-control и, если композитор его не поддерживает
//! (GNOME/Mutter), пишет предупреждение и переходит на X11. Фоновый опрос буфера
//! создавал экземпляр раз в 1,2 с — отсюда поток предупреждений и лишние
//! подключения к X-серверу. Экземпляр создаётся один раз и переиспользуется.

use std::sync::Mutex;

use crate::core::error::{AppError, AppResult};

static CLIPBOARD: Mutex<Option<arboard::Clipboard>> = Mutex::new(None);

/// Выполняет `f` с общим экземпляром буфера обмена (создаёт его при первом вызове).
pub fn with<R>(f: impl FnOnce(&mut arboard::Clipboard) -> R) -> AppResult<R> {
    let mut guard = CLIPBOARD.lock().unwrap_or_else(|p| p.into_inner());
    if guard.is_none() {
        let cb = arboard::Clipboard::new().map_err(|e| AppError::system(format!("буфер обмена недоступен: {e}")))?;
        *guard = Some(cb);
    }
    Ok(f(guard.as_mut().expect("экземпляр создан выше")))
}
