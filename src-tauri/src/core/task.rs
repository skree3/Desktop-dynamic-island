//! Управляемая фоновая задача: запускается/останавливается по требованию.
//! Используется модулями, чтобы не держать ненужные циклы опроса.

use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::async_runtime::JoinHandle;

#[derive(Default)]
pub struct TaskSlot {
    handle: Mutex<Option<JoinHandle<()>>>,
    alive: Arc<AtomicBool>,
}

struct AliveGuard(Arc<AtomicBool>);
impl Drop for AliveGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

impl TaskSlot {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_running(&self) -> bool {
        self.alive.load(Ordering::SeqCst)
    }

    /// Запускает задачу, если она ещё не запущена. Возвращает true, если запуск произошёл.
    pub fn start<F>(&self, fut: F) -> bool
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let mut g = self.handle.lock().unwrap_or_else(|p| p.into_inner());
        if self.alive.load(Ordering::SeqCst) {
            return false;
        }
        self.alive.store(true, Ordering::SeqCst);
        let guard = AliveGuard(Arc::clone(&self.alive));
        *g = Some(tauri::async_runtime::spawn(async move {
            let _guard = guard;
            fut.await;
        }));
        true
    }

    pub fn stop(&self) {
        let mut g = self.handle.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(h) = g.take() {
            h.abort();
        }
        self.alive.store(false, Ordering::SeqCst);
    }

    pub fn set_running(&self, run: bool, make: impl FnOnce() -> std::pin::Pin<Box<dyn Future<Output = ()> + Send>>) {
        if run {
            if !self.is_running() {
                self.start(make());
            }
        } else if self.is_running() {
            self.stop();
        }
    }
}
