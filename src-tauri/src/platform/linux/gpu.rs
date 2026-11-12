//! GPU в Linux без NVML: драйвер amdgpu публикует загрузку и температуру в sysfs.
//! Для Intel iGPU стабильного интерфейса загрузки нет — показываем «Недоступно».

use std::path::PathBuf;

use super::read_trim;

pub struct SysfsGpu {
    busy: PathBuf,
    temp: Option<PathBuf>,
    pub name: String,
}

impl SysfsGpu {
    pub fn new() -> Option<Self> {
        let entries = std::fs::read_dir("/sys/class/drm").ok()?;
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if !name.starts_with("card") || name.contains('-') {
                continue;
            }
            let dev = e.path().join("device");
            let busy = dev.join("gpu_busy_percent");
            if !busy.is_file() {
                continue;
            }
            let temp = std::fs::read_dir(dev.join("hwmon")).ok().and_then(|mut it| it.next()).and_then(|h| h.ok()).map(|h| h.path().join("temp1_input")).filter(|p| p.is_file());
            let driver = std::fs::read_link(dev.join("driver")).ok().and_then(|p| p.file_name().map(|s| s.to_string_lossy().to_string())).unwrap_or_else(|| "GPU".into());
            return Some(Self { busy, temp, name: format!("{} ({name})", driver.to_uppercase()) });
        }
        None
    }

    pub fn usage(&self) -> Option<f32> {
        read_trim(&self.busy).and_then(|s| s.parse::<f32>().ok())
    }

    pub fn temperature(&self) -> Option<f32> {
        self.temp.as_ref().and_then(read_trim).and_then(|s| s.parse::<f32>().ok()).map(|m| m / 1000.0)
    }
}
