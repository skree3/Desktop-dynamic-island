//! Системный монитор: CPU, RAM, GPU, температуры, диски, сеть, батарея.
//! Источники: sysinfo, NVML (NVIDIA, загружается динамически), PDH (Windows),
//! sysfs (Linux amdgpu/hwmon). Недоступные датчики отдаются как `null`
//! и показываются в UI как «Недоступно» — значения никогда не выдумываются.
//!
//! Опрос работает только когда карточка открыта или включены компактные
//! показатели; в игровом режиме и при скрытой панели — приостанавливается.
//! Сэмплер (и NVML) создаётся при старте задачи и освобождается при остановке.

use serde::Serialize;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use sysinfo::{Components, Disks, Networks, System};
use tauri::AppHandle;

use crate::core::state::Shared;
use crate::core::task::TaskSlot;
use crate::database::now_ms;
use crate::events::names;
use crate::platform::types::PowerState;

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GpuInfo {
    pub available: bool,
    pub name: Option<String>,
    pub usage: Option<f32>,
    pub temperature: Option<f32>,
    pub memory_used: Option<u64>,
    pub memory_total: Option<u64>,
    pub backend: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskInfo {
    pub name: String,
    pub mount: String,
    pub total: u64,
    pub available: u64,
    pub removable: bool,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SysSnapshot {
    pub ts: i64,
    pub cpu: f32,
    pub cpu_name: String,
    pub cores: usize,
    pub cpu_temp: Option<f32>,
    pub ram_used: u64,
    pub ram_total: u64,
    pub swap_used: u64,
    pub swap_total: u64,
    pub gpu: GpuInfo,
    pub disks: Vec<DiskInfo>,
    pub net_rx_bps: Option<u64>,
    pub net_tx_bps: Option<u64>,
    pub power: Option<PowerState>,
}

struct Sampler {
    sys: System,
    networks: Networks,
    disks: Vec<DiskInfo>,
    disks_at: Option<Instant>,
    components: Option<Components>,
    last_net: Option<(u64, u64, Instant)>,
    nvml: Option<nvml_wrapper::Nvml>,
    #[cfg(windows)]
    pdh: Option<crate::platform::os::gpu::PdhGpu>,
    #[cfg(target_os = "linux")]
    sysfs_gpu: Option<crate::platform::os::gpu::SysfsGpu>,
    power: Option<PowerState>,
    tick: u64,
}

impl Sampler {
    fn new() -> Self {
        let mut sys = System::new();
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        let components = Components::new_with_refreshed_list();
        let has_temp = components.iter().any(|c| c.temperature().map(|t| t.is_finite() && t > 0.0).unwrap_or(false));
        let nvml = nvml_wrapper::Nvml::init().ok();
        Self {
            sys,
            networks: Networks::new_with_refreshed_list(),
            disks: vec![],
            disks_at: None,
            components: if has_temp { Some(components) } else { None },
            last_net: None,
            #[cfg(windows)]
            pdh: if nvml.is_none() { crate::platform::os::gpu::PdhGpu::new() } else { None },
            #[cfg(target_os = "linux")]
            sysfs_gpu: if nvml.is_none() { crate::platform::os::gpu::SysfsGpu::new() } else { None },
            nvml,
            power: None,
            tick: 0,
        }
    }

    fn cpu_temp(&mut self) -> Option<f32> {
        let comps = self.components.as_mut()?;
        comps.refresh(false);
        let keys = ["package", "tctl", "tdie", "cpu", "core", "k10temp", "coretemp", "zenpower", "acpitz"];
        let mut best: Option<f32> = None;
        for k in keys {
            for c in comps.iter() {
                let label = c.label().to_lowercase();
                if label.contains(k) {
                    if let Some(t) = c.temperature().filter(|t| t.is_finite() && *t > 0.0 && *t < 150.0) {
                        best = Some(best.map_or(t, |b: f32| b.max(t)));
                    }
                }
            }
            if best.is_some() {
                break;
            }
        }
        best
    }

    fn gpu(&mut self) -> GpuInfo {
        if let Some(nvml) = &self.nvml {
            if let Ok(dev) = nvml.device_by_index(0) {
                let usage = dev.utilization_rates().ok().map(|u| u.gpu as f32);
                let temp = dev.temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu).ok().map(|t| t as f32);
                let mem = dev.memory_info().ok();
                return GpuInfo {
                    available: true,
                    name: dev.name().ok(),
                    usage,
                    temperature: temp,
                    memory_used: mem.as_ref().map(|m| m.used),
                    memory_total: mem.as_ref().map(|m| m.total),
                    backend: Some("NVML".into()),
                };
            }
        }
        #[cfg(windows)]
        if let Some(pdh) = self.pdh.as_mut() {
            return GpuInfo { available: true, usage: pdh.sample(), backend: Some("Windows PDH".into()), ..Default::default() };
        }
        #[cfg(target_os = "linux")]
        if let Some(g) = &self.sysfs_gpu {
            return GpuInfo { available: true, name: Some(g.name.clone()), usage: g.usage(), temperature: g.temperature(), backend: Some("sysfs (amdgpu)".into()), ..Default::default() };
        }
        GpuInfo::default()
    }

    fn sample(&mut self) -> SysSnapshot {
        self.tick += 1;
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();

        self.networks.refresh(true);
        let (rx, tx) = self
            .networks
            .iter()
            .filter(|(name, _)| !name.starts_with("lo") && !name.to_lowercase().contains("loopback"))
            .fold((0u64, 0u64), |(r, t), (_, d)| (r + d.total_received(), t + d.total_transmitted()));
        let now = Instant::now();
        let (net_rx, net_tx) = match self.last_net {
            Some((pr, pt, at)) => {
                let secs = now.duration_since(at).as_secs_f64().max(0.001);
                (Some((rx.saturating_sub(pr) as f64 / secs) as u64), Some((tx.saturating_sub(pt) as f64 / secs) as u64))
            }
            None => (None, None),
        };
        self.last_net = Some((rx, tx, now));

        // Диски меняются редко — обновляем раз в 30 секунд.
        if self.disks_at.map(|t| t.elapsed() > Duration::from_secs(30)).unwrap_or(true) {
            let d = Disks::new_with_refreshed_list();
            let mut list: Vec<DiskInfo> = d
                .iter()
                .filter(|d| d.total_space() > 0)
                .map(|d| DiskInfo {
                    name: d.name().to_string_lossy().to_string(),
                    mount: d.mount_point().to_string_lossy().to_string(),
                    total: d.total_space(),
                    available: d.available_space(),
                    removable: d.is_removable(),
                })
                .filter(|d| !d.mount.starts_with("/snap") && !d.mount.starts_with("/boot"))
                .collect();
            list.dedup_by(|a, b| a.mount == b.mount);
            self.disks = list;
            self.disks_at = Some(Instant::now());
        }

        if self.tick % 5 == 1 {
            self.power = crate::platform::os::power::state().ok();
        }

        let cpu_name = self.sys.cpus().first().map(|c| c.brand().trim().to_string()).unwrap_or_default();
        SysSnapshot {
            ts: now_ms(),
            cpu: self.sys.global_cpu_usage(),
            cores: self.sys.cpus().len(),
            cpu_name,
            cpu_temp: self.cpu_temp(),
            ram_used: self.sys.used_memory(),
            ram_total: self.sys.total_memory(),
            swap_used: self.sys.used_swap(),
            swap_total: self.sys.total_swap(),
            gpu: self.gpu(),
            disks: self.disks.clone(),
            net_rx_bps: net_rx,
            net_tx_bps: net_tx,
            power: self.power.clone(),
        }
    }
}

#[derive(Default)]
pub struct SysMonModule {
    task: TaskSlot,
    last: Mutex<Option<SysSnapshot>>,
}

impl SysMonModule {
    pub fn set_running(&self, app: &AppHandle, st: &Shared, run: bool) {
        if run == self.task.is_running() {
            return;
        }
        if !run {
            self.task.stop();
            return;
        }
        let st = st.clone();
        let _ = app;
        self.task.start(async move {
            let sampler = tauri::async_runtime::spawn_blocking(Sampler::new).await;
            let Ok(sampler) = sampler else { return };
            let sampler = std::sync::Arc::new(Mutex::new(sampler));
            loop {
                let interval = st.with_settings(|s| s.system_monitor.interval_ms);
                let s2 = sampler.clone();
                let snap = tauri::async_runtime::spawn_blocking(move || s2.lock().unwrap_or_else(|p| p.into_inner()).sample()).await;
                if let Ok(snap) = snap {
                    *st.sysmon.last.lock().unwrap_or_else(|p| p.into_inner()) = Some(snap.clone());
                    st.events.emit(names::SYSMON_SNAPSHOT, snap);
                }
                tokio::time::sleep(Duration::from_millis(interval)).await;
            }
        });
    }

    pub fn last(&self) -> Option<SysSnapshot> {
        self.last.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// Разовый снимок, если фоновый опрос не запущен.
    pub async fn snapshot_now(&self) -> Option<SysSnapshot> {
        if let Some(s) = self.last().filter(|s| now_ms() - s.ts < 5000) {
            return Some(s);
        }
        tauri::async_runtime::spawn_blocking(|| {
            let mut s = Sampler::new();
            std::thread::sleep(Duration::from_millis(300));
            s.sample()
        })
        .await
        .ok()
    }
}
