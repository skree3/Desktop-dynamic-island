//! В Linux атомарного move+resize нет — возвращаем false, и вызывающий код
//! использует стандартные set_size/set_position Tauri.

pub fn prepare_overlay(_window: &tauri::WebviewWindow) {}

pub fn set_bounds(_window: &tauri::WebviewWindow, _x: i32, _y: i32, _w: u32, _h: u32) -> bool {
    false
}
