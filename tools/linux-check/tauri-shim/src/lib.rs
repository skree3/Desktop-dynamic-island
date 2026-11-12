//! Минимальная заглушка API Tauri, которое используют Linux-адаптеры.
pub mod async_runtime {
    pub fn block_on<F: std::future::Future>(f: F) -> F::Output {
        tokio::runtime::Handle::current().block_on(f)
    }
}
pub struct WebviewWindow;
