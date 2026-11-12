//! Загрузка GPU через счётчики производительности Windows (PDH):
//! `\GPU Engine(*engtype_3D)\Utilization Percentage` — работает для любых
//! видеокарт с WDDM 2.x (Windows 10 1709+). Температура через PDH недоступна.

use windows::core::w;
use windows::Win32::System::Performance::{
    PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData, PdhGetFormattedCounterArrayW, PdhOpenQueryW, PDH_FMT_COUNTERVALUE_ITEM_W,
    PDH_FMT_DOUBLE, PDH_HCOUNTER, PDH_HQUERY, PDH_MORE_DATA,
};

pub struct PdhGpu {
    query: PDH_HQUERY,
    counter: PDH_HCOUNTER,
}

// Дескрипторы PDH используются только из одного потока опроса за раз.
unsafe impl Send for PdhGpu {}

impl PdhGpu {
    pub fn new() -> Option<Self> {
        unsafe {
            let mut query = PDH_HQUERY::default();
            if PdhOpenQueryW(windows::core::PCWSTR::null(), 0, &mut query) != 0 {
                return None;
            }
            let mut counter = PDH_HCOUNTER::default();
            if PdhAddEnglishCounterW(query, w!("\\GPU Engine(*engtype_3D)\\Utilization Percentage"), 0, &mut counter) != 0 {
                let _ = PdhCloseQuery(query);
                return None;
            }
            let _ = PdhCollectQueryData(query);
            Some(Self { query, counter })
        }
    }

    pub fn sample(&mut self) -> Option<f32> {
        unsafe {
            if PdhCollectQueryData(self.query) != 0 {
                return None;
            }
            let mut size = 0u32;
            let mut count = 0u32;
            let rc = PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut size, &mut count, None);
            if rc != PDH_MORE_DATA || size == 0 {
                return if count == 0 { Some(0.0) } else { None };
            }
            let items = (size as usize).div_ceil(std::mem::size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>());
            let mut buf: Vec<PDH_FMT_COUNTERVALUE_ITEM_W> = vec![std::mem::zeroed(); items + 1];
            if PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut size, &mut count, Some(buf.as_mut_ptr())) != 0 {
                return None;
            }
            let total: f64 = buf[..count as usize].iter().map(|i| i.FmtValue.Anonymous.doubleValue.max(0.0)).sum();
            Some(total.min(100.0) as f32)
        }
    }
}

impl Drop for PdhGpu {
    fn drop(&mut self) {
        unsafe {
            let _ = PdhCloseQuery(self.query);
        }
    }
}
