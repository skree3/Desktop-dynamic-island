//! Голос → текст прямо на компьютере (Vosk). Библиотека и модель языка
//! (~45 МБ) скачиваются по кнопке в настройках с github.com/alphacep и
//! alphacephei.com; после этого распознавание работает без интернета.

use std::ffi::{c_char, c_float, c_int, c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;

use crate::core::error::{AppError, AppResult};
use crate::core::state::Shared;

const VOSK_VERSION: &str = "0.3.45";
pub const PROGRESS_EVENT: &str = "speech://progress";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeechStatus {
    pub library: bool,
    pub models: Vec<String>,
    pub installing: Option<String>,
}

#[derive(Default)]
pub struct SpeechModule {
    installing: Mutex<Option<String>>,
}

fn dir(st: &Shared) -> PathBuf {
    st.paths.data.join("speech")
}

fn lib_name() -> &'static str {
    if cfg!(windows) {
        "libvosk.dll"
    } else {
        "libvosk.so"
    }
}

fn model_url(lang: &str) -> Option<&'static str> {
    match lang {
        "ru" => Some("https://alphacephei.com/vosk/models/vosk-model-small-ru-0.22.zip"),
        "en" => Some("https://alphacephei.com/vosk/models/vosk-model-small-en-us-0.15.zip"),
        _ => None,
    }
}

fn lib_url() -> String {
    let arch = if cfg!(windows) { "win64" } else { "linux-x86_64" };
    format!("https://github.com/alphacep/vosk-api/releases/download/v{VOSK_VERSION}/vosk-{arch}-{VOSK_VERSION}.zip")
}

fn download(st: &Shared, url: &str, what: &str) -> Result<Vec<u8>, String> {
    let resp = ureq::get(url).timeout(std::time::Duration::from_secs(600)).call().map_err(|e| e.to_string())?;
    let total: u64 = resp.header("Content-Length").and_then(|v| v.parse().ok()).unwrap_or(0);
    let mut reader = resp.into_reader();
    let mut out = Vec::with_capacity(total as usize);
    let mut buf = vec![0u8; 256 * 1024];
    let mut last = 0u64;
    loop {
        let n = std::io::Read::read(&mut reader, &mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n]);
        if out.len() > 200 * 1024 * 1024 {
            return Err("файл слишком большой".into());
        }
        let done = out.len() as u64;
        if done - last > 1024 * 1024 || done == total {
            last = done;
            st.events.emit(PROGRESS_EVENT, serde_json::json!({ "what": what, "done": done, "total": total }));
        }
    }
    Ok(out)
}

/// Распаковывает zip, убирая верхнюю папку архива; `flat` — все файлы в одну папку.
fn unzip(bytes: &[u8], to: &Path, flat: bool) -> Result<(), String> {
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
        let Some(name) = f.enclosed_name() else { continue };
        let rel: PathBuf = if flat { name.file_name().map(PathBuf::from).unwrap_or_default() } else { name.components().skip(1).collect() };
        if rel.as_os_str().is_empty() {
            continue;
        }
        let path = to.join(rel);
        if f.is_dir() {
            std::fs::create_dir_all(&path).map_err(|e| e.to_string())?;
            continue;
        }
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let mut out = std::fs::File::create(&path).map_err(|e| e.to_string())?;
        std::io::copy(&mut f, &mut out).map_err(|e| e.to_string())?;
    }
    Ok(())
}

impl SpeechModule {
    pub fn status(&self, st: &Shared) -> SpeechStatus {
        let d = dir(st);
        let models = ["ru", "en"].iter().filter(|l| d.join(format!("model-{l}")).join("conf").exists()).map(|l| l.to_string()).collect();
        SpeechStatus { library: d.join("lib").join(lib_name()).exists(), models, installing: self.installing.lock().unwrap_or_else(|p| p.into_inner()).clone() }
    }

    /// Скачивает библиотеку (если нужно) и модель языка.
    pub fn install(&self, st: &Shared, lang: &str) -> AppResult<()> {
        let url = model_url(lang).ok_or_else(|| AppError::invalid("язык не поддерживается"))?;
        {
            let mut g = self.installing.lock().unwrap_or_else(|p| p.into_inner());
            if g.is_some() {
                return Err(AppError::invalid("загрузка уже идёт"));
            }
            *g = Some(lang.to_string());
        }
        let res = (|| -> Result<(), String> {
            let d = dir(st);
            if !d.join("lib").join(lib_name()).exists() {
                let bytes = download(st, &lib_url(), "library")?;
                unzip(&bytes, &d.join("lib"), true)?;
            }
            let bytes = download(st, url, "model")?;
            let target = d.join(format!("model-{lang}"));
            let _ = std::fs::remove_dir_all(&target);
            unzip(&bytes, &target, false)?;
            Ok(())
        })();
        *self.installing.lock().unwrap_or_else(|p| p.into_inner()) = None;
        st.events.emit(PROGRESS_EVENT, serde_json::json!({ "what": "done", "done": 0, "total": 0 }));
        res.map_err(|e| AppError::Other(format!("не удалось скачать распознавание речи: {e}")))
    }

    /// Распознаёт голосовую заметку (WAV) в текст.
    pub fn transcribe(&self, st: &Shared, name: &str, lang: &str) -> AppResult<String> {
        if !crate::modules::voice::valid_name(name) {
            return Err(AppError::invalid("неверное имя записи"));
        }
        let d = dir(st);
        let model = d.join(format!("model-{lang}"));
        if !model.join("conf").exists() {
            return Err(AppError::unsupported("модель языка не скачана"));
        }
        let samples = read_16k(&st.paths.data.join("voice").join(name))?;
        let lib_path = d.join("lib").join(lib_name());
        unsafe { run_vosk(&lib_path, &model, &samples) }.map_err(AppError::Other)
    }
}

/// WAV → моно 16 кГц, 16 бит (линейная интерполяция).
fn read_16k(path: &Path) -> AppResult<Vec<i16>> {
    let mut r = hound::WavReader::open(path).map_err(|e| AppError::Other(e.to_string()))?;
    let spec = r.spec();
    let ch = spec.channels.max(1) as usize;
    let raw: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => r.samples::<i32>().flatten().map(|v| v as f32 / (1i64 << (spec.bits_per_sample - 1)) as f32).collect(),
        hound::SampleFormat::Float => r.samples::<f32>().flatten().collect(),
    };
    let mono: Vec<f32> = raw.chunks(ch).map(|c| c.iter().sum::<f32>() / c.len() as f32).collect();
    let ratio = spec.sample_rate as f64 / 16_000.0;
    let n = (mono.len() as f64 / ratio) as usize;
    Ok((0..n)
        .map(|i| {
            let pos = i as f64 * ratio;
            let j = pos as usize;
            let t = (pos - j as f64) as f32;
            let a = mono.get(j).copied().unwrap_or(0.0);
            let b = mono.get(j + 1).copied().unwrap_or(a);
            ((a + (b - a) * t).clamp(-1.0, 1.0) * i16::MAX as f32) as i16
        })
        .collect())
}

type ModelNew = unsafe extern "C" fn(*const c_char) -> *mut c_void;
type ModelFree = unsafe extern "C" fn(*mut c_void);
type RecNew = unsafe extern "C" fn(*mut c_void, c_float) -> *mut c_void;
type RecAccept = unsafe extern "C" fn(*mut c_void, *const i16, c_int) -> c_int;
type RecFinal = unsafe extern "C" fn(*mut c_void) -> *const c_char;
type RecFree = unsafe extern "C" fn(*mut c_void);
type LogLevel = unsafe extern "C" fn(c_int);

unsafe fn run_vosk(lib_path: &Path, model: &Path, samples: &[i16]) -> Result<String, String> {
    #[cfg(windows)]
    let lib: libloading::Library = libloading::os::windows::Library::load_with_flags(lib_path, 0x8 /* LOAD_WITH_ALTERED_SEARCH_PATH */).map_err(|e| e.to_string())?.into();
    #[cfg(not(windows))]
    let lib = libloading::Library::new(lib_path).map_err(|e| e.to_string())?;
    let log: libloading::Symbol<LogLevel> = lib.get(b"vosk_set_log_level\0").map_err(|e| e.to_string())?;
    log(-1);
    let model_new: libloading::Symbol<ModelNew> = lib.get(b"vosk_model_new\0").map_err(|e| e.to_string())?;
    let model_free: libloading::Symbol<ModelFree> = lib.get(b"vosk_model_free\0").map_err(|e| e.to_string())?;
    let rec_new: libloading::Symbol<RecNew> = lib.get(b"vosk_recognizer_new\0").map_err(|e| e.to_string())?;
    let accept: libloading::Symbol<RecAccept> = lib.get(b"vosk_recognizer_accept_waveform_s\0").map_err(|e| e.to_string())?;
    let final_result: libloading::Symbol<RecFinal> = lib.get(b"vosk_recognizer_final_result\0").map_err(|e| e.to_string())?;
    let rec_free: libloading::Symbol<RecFree> = lib.get(b"vosk_recognizer_free\0").map_err(|e| e.to_string())?;
    let path = CString::new(model.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
    let m = model_new(path.as_ptr());
    if m.is_null() {
        return Err("модель не загрузилась".into());
    }
    let r = rec_new(m, 16_000.0);
    if r.is_null() {
        model_free(m);
        return Err("распознаватель не создан".into());
    }
    for chunk in samples.chunks(8000) {
        accept(r, chunk.as_ptr(), chunk.len() as c_int);
    }
    let json = CStr::from_ptr(final_result(r)).to_string_lossy().into_owned();
    rec_free(r);
    model_free(m);
    let v: serde_json::Value = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    Ok(v.get("text").and_then(|t| t.as_str()).unwrap_or("").trim().to_string())
}
