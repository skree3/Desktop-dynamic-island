//! Громкость и яркость в острове: на Linux клавиши обрабатывает окружение рабочего
//! стола, перехватить их нельзя — функция работает только в Windows.

use std::sync::mpsc::Sender;

#[derive(Debug, Clone, Copy)]
pub enum VolumeKey {
    Up,
    Down,
    Mute,
}

pub fn start_volume_keys(_tx: Sender<VolumeKey>) {}
pub fn stop_volume_keys() {}
pub fn start_brightness(_tx: Sender<u32>) {}
pub fn stop_brightness() {}
