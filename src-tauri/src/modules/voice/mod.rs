//! Голосовые заметки: запись с микрофона по умолчанию в WAV (cpal + hound).
//! Запись идёт в отдельном потоке (поток аудио на некоторых системах нельзя
//! передавать между потоками), уровень громкости ~10 раз в секунду уходит
//! в интерфейс для «волны». Максимальная длина записи — 2 минуты.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::core::error::{AppError, AppResult};
use crate::core::state::Shared;

pub const LEVEL_EVENT: &str = "voice://level";
const MAX_SECS: u64 = 120;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recorded {
    pub name: String,
    pub duration_ms: i64,
}

struct Active {
    stop: Arc<AtomicBool>,
    done: mpsc::Receiver<Result<Recorded, String>>,
}

#[derive(Default)]
pub struct VoiceModule {
    active: Mutex<Option<Active>>,
}

fn dir(st: &Shared) -> std::path::PathBuf {
    st.paths.data.join("voice")
}

/// Имя файла записи: только то, что создаёт сам остров.
pub fn valid_name(name: &str) -> bool {
    name.len() <= 64 && name.ends_with(".wav") && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
}

pub fn remove(st: &Shared, name: &str) {
    if valid_name(name) {
        let _ = std::fs::remove_file(dir(st).join(name));
    }
}

/// Запись в виде data-URL для проигрывания в интерфейсе.
pub fn data_url(st: &Shared, name: &str) -> AppResult<String> {
    use base64::Engine;
    if !valid_name(name) {
        return Err(AppError::invalid("неверное имя записи"));
    }
    let bytes = std::fs::read(dir(st).join(name)).map_err(|_| AppError::NotFound("запись не найдена".into()))?;
    Ok(format!("data:audio/wav;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes)))
}

impl VoiceModule {
    pub fn recording(&self) -> bool {
        self.active.lock().unwrap_or_else(|p| p.into_inner()).is_some()
    }

    pub fn start(&self, st: &Shared) -> AppResult<()> {
        let mut g = self.active.lock().unwrap_or_else(|p| p.into_inner());
        if g.is_some() {
            return Ok(());
        }
        std::fs::create_dir_all(dir(st))?;
        let name = format!("{}.wav", uuid::Uuid::new_v4().simple());
        let path = dir(st).join(&name);
        let stop = Arc::new(AtomicBool::new(false));
        let (done_tx, done_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();
        let (stop2, st2) = (stop.clone(), st.clone());
        std::thread::Builder::new()
            .name("voice-record".into())
            .spawn(move || {
                let res = record(&path, &stop2, &st2, ready_tx);
                let _ = done_tx.send(res.map(|ms| Recorded { name, duration_ms: ms }));
            })
            .map_err(|e| AppError::Other(e.to_string()))?;
        match ready_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(())) => {
                *g = Some(Active { stop, done: done_rx });
                Ok(())
            }
            Ok(Err(e)) => Err(AppError::unsupported(e)),
            Err(_) => {
                stop.store(true, Ordering::SeqCst);
                Err(AppError::unsupported("микрофон не отвечает"))
            }
        }
    }

    pub fn stop(&self) -> AppResult<Recorded> {
        let active = self.active.lock().unwrap_or_else(|p| p.into_inner()).take().ok_or_else(|| AppError::invalid("запись не идёт"))?;
        active.stop.store(true, Ordering::SeqCst);
        match active.done.recv_timeout(Duration::from_secs(10)) {
            Ok(Ok(r)) => Ok(r),
            Ok(Err(e)) => Err(AppError::Other(e)),
            Err(_) => Err(AppError::Other("запись не завершилась".into())),
        }
    }

    pub fn cancel(&self, st: &Shared) {
        if let Ok(r) = self.stop() {
            remove(st, &r.name);
        }
    }
}

fn record(path: &std::path::Path, stop: &AtomicBool, st: &Shared, ready: mpsc::Sender<Result<(), String>>) -> Result<i64, String> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
    let fail = |ready: &mpsc::Sender<Result<(), String>>, e: String| {
        let _ = ready.send(Err(e.clone()));
        e
    };
    let host = cpal::default_host();
    let Some(device) = host.default_input_device() else { return Err(fail(&ready, "микрофон не найден".into())) };
    let config = device.default_input_config().map_err(|e| fail(&ready, format!("микрофон недоступен: {e}")))?;
    let rate = config.sample_rate().0;
    let channels = config.channels() as usize;
    let samples: Arc<Mutex<Vec<i16>>> = Arc::new(Mutex::new(Vec::with_capacity(rate as usize * 30)));
    let level = Arc::new(Mutex::new(0f32));
    let (s2, l2) = (samples.clone(), level.clone());
    // Моно: среднее по каналам, сразу в 16 бит.
    let push = move |frame_iter: &mut dyn Iterator<Item = f32>| {
        let mut buf = s2.lock().unwrap_or_else(|p| p.into_inner());
        let mut peak = 0f32;
        let mut acc = 0f32;
        let mut n = 0usize;
        for v in frame_iter {
            acc += v;
            n += 1;
            if n == channels {
                let m = (acc / channels as f32).clamp(-1.0, 1.0);
                peak = peak.max(m.abs());
                buf.push((m * i16::MAX as f32) as i16);
                acc = 0.0;
                n = 0;
            }
        }
        let mut l = l2.lock().unwrap_or_else(|p| p.into_inner());
        *l = l.max(peak);
    };
    let err_fn = |e| tracing::warn!(error = %e, "ошибка потока микрофона");
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => {
            let push = push;
            device.build_input_stream(&config.into(), move |d: &[f32], _| push(&mut d.iter().copied()), err_fn, None)
        }
        cpal::SampleFormat::I16 => {
            let push = push;
            device.build_input_stream(&config.into(), move |d: &[i16], _| push(&mut d.iter().map(|v| *v as f32 / i16::MAX as f32)), err_fn, None)
        }
        cpal::SampleFormat::U16 => {
            let push = push;
            device.build_input_stream(&config.into(), move |d: &[u16], _| push(&mut d.iter().map(|v| (*v as f32 - 32768.0) / 32768.0)), err_fn, None)
        }
        other => return Err(fail(&ready, format!("формат микрофона {other:?} не поддерживается"))),
    }
    .map_err(|e| fail(&ready, format!("микрофон недоступен: {e}")))?;
    stream.play().map_err(|e| fail(&ready, format!("микрофон недоступен: {e}")))?;
    let _ = ready.send(Ok(()));
    let started = Instant::now();
    while !stop.load(Ordering::SeqCst) && started.elapsed() < Duration::from_secs(MAX_SECS) {
        std::thread::sleep(Duration::from_millis(100));
        let l = std::mem::take(&mut *level.lock().unwrap_or_else(|p| p.into_inner()));
        st.events.emit(LEVEL_EVENT, l);
    }
    drop(stream);
    let data = std::mem::take(&mut *samples.lock().unwrap_or_else(|p| p.into_inner()));
    let spec = hound::WavSpec { channels: 1, sample_rate: rate, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut w = hound::WavWriter::create(path, spec).map_err(|e| e.to_string())?;
    for s in &data {
        w.write_sample(*s).map_err(|e| e.to_string())?;
    }
    w.finalize().map_err(|e| e.to_string())?;
    Ok((data.len() as i64 * 1000) / rate.max(1) as i64)
}
