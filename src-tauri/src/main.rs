// Скрывает консольное окно в release-сборке Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    desktop_island_lib::run()
}
