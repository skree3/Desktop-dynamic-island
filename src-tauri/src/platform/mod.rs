//! Системные адаптеры. Каждая платформа реализует один и тот же набор модулей
//! (`media`, `audio`, `brightness`, `network`, `bluetooth`, `power`, `foreground`,
//! `apps`, `shell`, `notifications`, `clipboard`, `gpu`, `proxy`, `window`) своим
//! способом — без попыток «унифицировать» реализацию.

pub mod types;

#[cfg(windows)]
pub mod windows;
#[cfg(windows)]
pub use self::windows as os;

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "linux")]
pub use self::linux as os;

#[cfg(not(any(windows, target_os = "linux")))]
compile_error!("Desktop Island поддерживает только Windows и Linux");
