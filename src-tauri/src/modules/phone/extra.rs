//! Дополнительные возможности связи с телефоном (протокол KDE Connect):
//! * SMS — переписка с телефона и отправка ответов (`kdeconnect.sms.*`), имена из контактов;
//! * музыка телефона в острове (`kdeconnect.mpris`) с обложкой и управлением;
//! * кнопки-команды на телефоне (`kdeconnect.runcommand`);
//! * тачпад, клавиатура и пульт для презентаций (`kdeconnect.mousepad.request`, `kdeconnect.presenter`);
//! * закрытие уведомления на компьютере убирает его и с телефона.

use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;

use super::{net, now_ms, packet, phone_dir};
use crate::core::error::{AppError, AppResult};
use crate::core::state::Shared;
use crate::platform::os;
use crate::platform::types::{MediaAction, MediaSessionInfo};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sms {
    pub id: i64,
    pub thread: String,
    pub address: String,
    pub body: String,
    pub date: i64,
    pub outgoing: bool,
    pub read: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SmsThread {
    pub thread: String,
    pub address: String,
    pub name: String,
    pub last: String,
    pub date: i64,
    pub unread: bool,
    pub outgoing: bool,
}

#[derive(Debug, Clone, Default)]
struct Player {
    device: String,
    device_name: String,
    name: String,
    title: String,
    artist: String,
    album: String,
    playing: bool,
    pos_ms: Option<u64>,
    pos_at: i64,
    length_ms: Option<u64>,
    can_play: bool,
    can_pause: bool,
    can_next: bool,
    can_prev: bool,
    can_seek: bool,
    volume: Option<f32>,
    art_url: String,
    /// Обложка уже пришла (data URL лежит в музыкальном модуле).
    art_ready: bool,
}

#[derive(Default)]
pub struct State {
    threads: HashMap<String, Vec<Sms>>,
    contacts: HashMap<String, String>,
    sms_requested_at: i64,
    players: HashMap<String, Player>,
    art_support: bool,
    /// Последний отправленный телефону список команд.
    commands_sent: String,
}

const MAX_THREADS: usize = 200;
const MAX_MESSAGES: usize = 300;

/// Последние 10 цифр номера — так совпадают «+7 999…» и «8 999…».
fn norm(number: &str) -> String {
    let d: String = number.chars().filter(|c| c.is_ascii_digit()).collect();
    d.chars().rev().take(10).collect::<Vec<_>>().into_iter().rev().collect()
}

fn session_id(device: &str, player: &str) -> String {
    format!("phone:{device}:{player}")
}

fn lock(st: &Shared) -> std::sync::MutexGuard<'_, State> {
    st.phone.extra.lock().unwrap_or_else(|p| p.into_inner())
}

/// Сразу после подключения сопряжённого телефона — запросы того, что включено.
pub fn on_linked(st: &Shared, id: &str) {
    let s = st.with_settings(|s| s.phone.clone());
    if s.music {
        st.phone.send(id, packet("kdeconnect.mpris.request", json!({ "requestPlayerList": true })));
    }
    if s.sms {
        st.phone.send(id, packet("kdeconnect.contacts.request_all_uids_timestamps", json!({})));
        st.phone.send(id, packet("kdeconnect.sms.request_conversations", json!({})));
        lock(st).sms_requested_at = now_ms();
    }
    if s.commands {
        send_command_list(st, id);
    }
}

/// Телефон отключился — его плееры пропадают из острова.
pub fn on_unlinked(st: &Shared, id: &str) {
    let removed = {
        let mut g = lock(st);
        let before = g.players.len();
        g.players.retain(|_, p| p.device != id);
        before != g.players.len()
    };
    if removed {
        push_sessions(st);
    }
}

/// Обработка пакетов. Возвращает true, если пакет относится к этим возможностям.
pub fn handle(st: &Shared, id: &str, name: &str, kind: &str, body: &Value, v: &Value) -> bool {
    let s = st.with_settings(|s| s.phone.clone());
    match kind {
        "kdeconnect.sms.messages" if s.sms => {
            sms_messages(st, body);
            true
        }
        "kdeconnect.contacts.response_uids_timestamps" if s.sms => {
            let uids: Vec<Value> = body.get("uids").and_then(|u| u.as_array()).cloned().unwrap_or_default();
            if !uids.is_empty() {
                st.phone.send(id, packet("kdeconnect.contacts.request_vcards_by_uid", json!({ "uids": uids })));
            }
            true
        }
        "kdeconnect.contacts.response_vcards" if s.sms => {
            contacts(st, body);
            true
        }
        "kdeconnect.mpris" if s.music => {
            mpris(st, id, name, body, v);
            true
        }
        "kdeconnect.runcommand.request" if s.commands => {
            if body.get("requestCommandList").and_then(|x| x.as_bool()).unwrap_or(false) {
                send_command_list(st, id);
            } else if let Some(key) = body.get("key").and_then(|k| k.as_str()) {
                run_command(st, key);
            }
            true
        }
        "kdeconnect.mousepad.request" if s.remote_input => {
            mousepad(body);
            true
        }
        "kdeconnect.presenter" if s.remote_input => {
            if !body.get("stop").and_then(|x| x.as_bool()).unwrap_or(false) {
                let f = |k: &str| body.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0);
                os::input::pointer_by_fraction(f("dx"), f("dy"));
            }
            true
        }
        _ => false,
    }
}

// ---- SMS ----

fn sms_messages(st: &Shared, body: &Value) {
    let Some(list) = body.get("messages").and_then(|m| m.as_array()) else { return };
    let mut g = lock(st);
    for m in list {
        let thread = match m.get("thread_id") {
            Some(Value::Number(n)) => n.to_string(),
            Some(Value::String(s)) => s.clone(),
            _ => continue,
        };
        let address = m
            .get("addresses")
            .and_then(|a| a.as_array())
            .and_then(|a| a.first())
            .and_then(|a| a.get("address"))
            .and_then(|a| a.as_str())
            .unwrap_or("")
            .to_string();
        let msg = Sms {
            id: m.get("_id").and_then(|x| x.as_i64()).unwrap_or(0),
            thread: thread.clone(),
            address,
            body: crate::util::clean_text(m.get("body").and_then(|b| b.as_str()).unwrap_or(""), 4000),
            date: m.get("date").and_then(|x| x.as_i64()).unwrap_or(0),
            outgoing: m.get("type").and_then(|x| x.as_i64()) == Some(2),
            read: m.get("read").and_then(|x| x.as_i64()).unwrap_or(1) != 0,
        };
        let t = g.threads.entry(thread).or_default();
        if let Some(old) = t.iter_mut().find(|x| x.id == msg.id && msg.id != 0) {
            *old = msg;
        } else {
            // Отправленное с компьютера показано сразу; когда телефон пришлёт настоящее — заменяем.
            t.retain(|x| !(x.id < 0 && x.outgoing && x.body == msg.body));
            t.push(msg);
        }
        t.sort_by_key(|x| x.date);
        if t.len() > MAX_MESSAGES {
            let cut = t.len() - MAX_MESSAGES;
            t.drain(..cut);
        }
    }
    if g.threads.len() > MAX_THREADS {
        let mut dates: Vec<(String, i64)> = g.threads.iter().map(|(k, v)| (k.clone(), v.last().map(|m| m.date).unwrap_or(0))).collect();
        dates.sort_by_key(|x| std::cmp::Reverse(x.1));
        for (k, _) in dates.into_iter().skip(MAX_THREADS) {
            g.threads.remove(&k);
        }
    }
    drop(g);
    st.events.emit("phone://sms", ());
}

fn contacts(st: &Shared, body: &Value) {
    let Some(obj) = body.as_object() else { return };
    let mut g = lock(st);
    for (k, card) in obj {
        if k == "uids" {
            continue;
        }
        let Some(text) = card.as_str() else { continue };
        let mut name = String::new();
        let mut numbers = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if let Some(v) = line.strip_prefix("FN:").or_else(|| line.strip_prefix("FN;CHARSET=UTF-8:")) {
                name = v.trim().to_string();
            } else if line.starts_with("TEL") {
                if let Some((_, num)) = line.split_once(':') {
                    numbers.push(norm(num));
                }
            }
        }
        if !name.is_empty() {
            for n in numbers.into_iter().filter(|n| n.len() >= 5) {
                g.contacts.insert(n, crate::util::clean_text(&name, 64));
            }
        }
    }
    drop(g);
    st.events.emit("phone://sms", ());
}

pub fn sms_threads(st: &Shared) -> Vec<SmsThread> {
    // Раз в минуту при открытии — свежий список с телефона.
    let device = paired_device(st);
    let stale = now_ms() - lock(st).sms_requested_at > 60_000;
    if let (Some(dev), true) = (device, stale) {
        st.phone.send(&dev, packet("kdeconnect.sms.request_conversations", json!({})));
        lock(st).sms_requested_at = now_ms();
    }
    let g = lock(st);
    let mut out: Vec<SmsThread> = g
        .threads
        .iter()
        .filter_map(|(k, v)| {
            let last = v.last()?;
            let address = v.iter().rev().map(|m| m.address.clone()).find(|a| !a.is_empty()).unwrap_or_default();
            let name = g.contacts.get(&norm(&address)).cloned().unwrap_or_else(|| address.clone());
            Some(SmsThread { thread: k.clone(), address, name, last: last.body.chars().take(140).collect(), date: last.date, unread: !last.read && !last.outgoing, outgoing: last.outgoing })
        })
        .collect();
    out.sort_by_key(|t| std::cmp::Reverse(t.date));
    out
}

pub fn sms_thread(st: &Shared, thread: &str) -> Vec<Sms> {
    if let (Some(dev), Ok(tid)) = (paired_device(st), thread.parse::<i64>()) {
        st.phone.send(&dev, packet("kdeconnect.sms.request_conversation", json!({ "threadID": tid, "numberToRequest": 60 })));
    }
    lock(st).threads.get(thread).cloned().unwrap_or_default()
}

pub fn sms_send(st: &Shared, address: &str, text: &str) -> AppResult<()> {
    let address = address.trim();
    let text = text.trim();
    if address.is_empty() || text.is_empty() {
        return Err(AppError::invalid("пустой номер или сообщение"));
    }
    if text.chars().count() > 2000 {
        return Err(AppError::invalid("сообщение слишком длинное"));
    }
    let dev = paired_device(st).ok_or_else(|| AppError::invalid("телефон сейчас не в сети"))?;
    let body = json!({ "version": 2, "addresses": [{ "address": address }], "phoneNumber": address, "messageBody": text });
    if !st.phone.send(&dev, packet("kdeconnect.sms.request", body)) {
        return Err(AppError::invalid("телефон сейчас не в сети"));
    }
    // Показываем сразу в переписке; телефон потом пришлёт настоящее сообщение.
    let mut g = lock(st);
    let thread = g.threads.iter().find(|(_, v)| v.iter().any(|m| norm(&m.address) == norm(address))).map(|(k, _)| k.clone()).unwrap_or_else(|| format!("new:{}", norm(address)));
    g.threads.entry(thread.clone()).or_default().push(Sms { id: -now_ms(), thread, address: address.into(), body: text.into(), date: now_ms(), outgoing: true, read: true });
    drop(g);
    st.events.emit("phone://sms", ());
    Ok(())
}

/// Кнопка «Отправить буфер»: работает, даже если автоматическая синхронизация выключена.
pub fn send_clipboard(st: &Shared, text: &str) -> AppResult<()> {
    let dev = paired_device(st).ok_or_else(|| AppError::invalid("телефон сейчас не в сети"))?;
    st.phone.lock().last_clip = text.to_string();
    if st.phone.send(&dev, packet("kdeconnect.clipboard", json!({ "content": text }))) {
        Ok(())
    } else {
        Err(AppError::invalid("телефон сейчас не в сети"))
    }
}

fn paired_device(st: &Shared) -> Option<String> {
    let g = st.phone.lock();
    g.devices.iter().find(|(_, d)| d.paired && d.link.is_some()).map(|(id, _)| id.clone())
}

// ---- музыка ----

fn mpris(st: &Shared, id: &str, device_name: &str, body: &Value, v: &Value) {
    if let Some(list) = body.get("playerList").and_then(|l| l.as_array()) {
        let names: Vec<String> = list.iter().filter_map(|x| x.as_str().map(str::to_string)).collect();
        let art = body.get("supportAlbumArtPayload").and_then(|x| x.as_bool()).unwrap_or(false);
        let new: Vec<String> = {
            let mut g = lock(st);
            g.art_support = art;
            g.players.retain(|_, p| p.device != id || names.contains(&p.name));
            names.iter().filter(|n| !g.players.contains_key(&session_id(id, n))).cloned().collect()
        };
        for n in new {
            lock(st).players.insert(session_id(id, &n), Player { device: id.into(), device_name: device_name.into(), name: n.clone(), ..Default::default() });
            st.phone.send(id, packet("kdeconnect.mpris.request", json!({ "player": n, "requestNowPlaying": true, "requestVolume": true })));
        }
        push_sessions(st);
        return;
    }
    let Some(player) = body.get("player").and_then(|p| p.as_str()) else { return };
    let sid = session_id(id, player);
    // Обложка пришла файлом.
    if body.get("transferringAlbumArt").and_then(|x| x.as_bool()).unwrap_or(false) {
        let port = v.get("payloadTransferInfo").and_then(|p| p.get("port")).and_then(|p| p.as_u64()).and_then(|p| u16::try_from(p).ok());
        let size = v.get("payloadSize").and_then(|s| s.as_i64()).unwrap_or(-1);
        let addr = st.phone.lock().devices.get(id).and_then(|d| d.addr);
        if let (Some(port), Some(ip)) = (port, addr) {
            let (st2, sid2) = (st.clone(), sid.clone());
            let url = body.get("albumArtUrl").and_then(|u| u.as_str()).unwrap_or("").to_string();
            tauri::async_runtime::spawn(async move {
                if let Ok(bytes) = receive_payload(&st2, SocketAddr::new(ip, port), size, 4 * 1024 * 1024).await {
                    use base64::Engine;
                    let mime = if bytes.starts_with(&[0x89, b'P', b'N', b'G']) { "image/png" } else { "image/jpeg" };
                    let data = format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(&bytes));
                    st2.music.set_phone_art(&sid2, data);
                    if let Some(p) = lock(&st2).players.get_mut(&sid2) {
                        if p.art_url == url || url.is_empty() {
                            p.art_ready = true;
                        }
                    }
                    push_sessions(&st2);
                }
            });
        }
        return;
    }
    let request_art = {
        let mut g = lock(st);
        let art_support = g.art_support;
        let p = g.players.entry(sid.clone()).or_insert_with(|| Player { device: id.into(), device_name: device_name.into(), name: player.into(), ..Default::default() });
        let s = |k: &str| body.get(k).and_then(|x| x.as_str()).map(|x| crate::util::clean_text(x, 300));
        let b = |k: &str| body.get(k).and_then(|x| x.as_bool());
        if let Some(t) = s("title") {
            p.title = t;
        }
        if let Some(a) = s("artist") {
            p.artist = a;
        }
        if let Some(a) = s("album") {
            p.album = a;
        }
        if p.title.is_empty() {
            if let Some(np) = s("nowPlaying") {
                p.title = np;
            }
        }
        if let Some(x) = b("isPlaying") {
            p.playing = x;
        }
        if let Some(x) = body.get("pos").and_then(|x| x.as_i64()) {
            p.pos_ms = Some(x.max(0) as u64);
            p.pos_at = now_ms();
        }
        if let Some(x) = body.get("length").and_then(|x| x.as_i64()) {
            p.length_ms = (x > 0).then_some(x as u64);
        }
        if let Some(x) = body.get("volume").and_then(|x| x.as_i64()) {
            p.volume = Some((x.clamp(0, 100) as f32) / 100.0);
        }
        for (k, f) in [("canPlay", &mut p.can_play), ("canPause", &mut p.can_pause), ("canGoNext", &mut p.can_next), ("canGoPrevious", &mut p.can_prev), ("canSeek", &mut p.can_seek)] {
            if let Some(x) = b(k) {
                *f = x;
            }
        }
        match s("albumArtUrl") {
            Some(url) if url != p.art_url => {
                p.art_url = url.clone();
                p.art_ready = false;
                (art_support && !url.is_empty()).then_some(url)
            }
            _ => None,
        }
    };
    if let Some(url) = request_art {
        st.phone.send(id, packet("kdeconnect.mpris.request", json!({ "player": player, "albumArtUrl": url })));
    }
    push_sessions(st);
}

fn push_sessions(st: &Shared) {
    let list: Vec<MediaSessionInfo> = {
        let g = lock(st);
        let mut v: Vec<MediaSessionInfo> = g
            .players
            .iter()
            .filter(|(_, p)| !p.title.is_empty())
            .map(|(sid, p)| MediaSessionInfo {
                id: sid.clone(),
                source_app: "phone".into(),
                app_name: format!("{} · {}", p.name, p.device_name),
                title: p.title.clone(),
                artist: p.artist.clone(),
                album: p.album.clone(),
                status: if p.playing { "playing".into() } else { "paused".into() },
                position_ms: p.pos_ms,
                duration_ms: p.length_ms,
                position_updated_at: p.pos_at,
                can_play_pause: p.can_play || p.can_pause,
                can_next: p.can_next,
                can_prev: p.can_prev,
                can_seek: p.can_seek && p.length_ms.is_some(),
                supports_volume: p.volume.is_some(),
                volume: p.volume,
                artwork_key: if p.art_ready { format!("{}#ready", p.art_url) } else { String::new() },
                artwork_url: None,
                is_current: false,
            })
            .collect();
        v.sort_by(|a, b| a.id.cmp(&b.id));
        v
    };
    st.music.set_phone_sessions(st, list);
}

/// Управление плеером телефона из острова.
pub fn media_control(st: &Shared, sid: &str, action: &MediaAction) -> AppResult<()> {
    let (device, player) = {
        let g = lock(st);
        let p = g.players.get(sid).ok_or_else(|| AppError::NotFound("плеер телефона".into()))?;
        (p.device.clone(), p.name.clone())
    };
    let body = match action {
        MediaAction::PlayPause => json!({ "player": player, "action": "PlayPause" }),
        MediaAction::Play => json!({ "player": player, "action": "Play" }),
        MediaAction::Pause => json!({ "player": player, "action": "Pause" }),
        MediaAction::Next => json!({ "player": player, "action": "Next" }),
        MediaAction::Previous => json!({ "player": player, "action": "Previous" }),
        MediaAction::Seek { position_ms } => json!({ "player": player, "SetPosition": position_ms }),
        MediaAction::SetVolume { volume } => json!({ "player": player, "setVolume": (volume.clamp(0.0, 1.0) * 100.0).round() as i64 }),
    };
    if st.phone.send(&device, packet("kdeconnect.mpris.request", body)) {
        Ok(())
    } else {
        Err(AppError::invalid("телефон сейчас не в сети"))
    }
}

/// Файл с телефона в память (обложка): TLS-клиент к порту, который назвал телефон.
async fn receive_payload(st: &Shared, target: SocketAddr, size: i64, limit: usize) -> Result<Vec<u8>, String> {
    use tokio::io::AsyncReadExt;
    if size > limit as i64 {
        return Err("слишком большой файл".into());
    }
    let ident = net::Identity::load_or_create(&phone_dir(st))?;
    let cfg = ident.client_config()?;
    let stream = tokio::time::timeout(Duration::from_secs(10), tokio::net::TcpStream::connect(target)).await.map_err(|_| "телефон не отвечает".to_string())?.map_err(|e| e.to_string())?;
    let mut tls = tokio_rustls::TlsConnector::from(cfg).connect(rustls::pki_types::ServerName::IpAddress(target.ip().into()), stream).await.map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    let mut buf = vec![0u8; 32 * 1024];
    loop {
        let n = tls.read(&mut buf).await.unwrap_or(0);
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n]);
        if out.len() > limit {
            return Err("слишком большой файл".into());
        }
        if size >= 0 && out.len() as i64 >= size {
            break;
        }
    }
    Ok(out)
}

// ---- команды ----

/// Встроенные команды: (ключ, название).
const BUILTIN: &[(&str, &str)] = &[
    ("lock", "Заблокировать компьютер"),
    ("sleep", "Спящий режим"),
    ("screen_off", "Выключить экран"),
    ("mute", "Выключить / включить звук"),
    ("play_pause", "Музыка: пауза / воспроизведение"),
    ("dnd", "Не беспокоить"),
    ("island", "Показать / скрыть остров"),
];

/// Настройки поменялись — если список команд другой, отправляем его телефонам заново.
pub fn sync_commands(st: &Shared) {
    if !st.with_settings(|s| s.phone.enabled && s.phone.commands) {
        return;
    }
    let list = command_list(st);
    if lock(st).commands_sent == list {
        return;
    }
    let ids: Vec<String> = {
        let g = st.phone.lock();
        g.devices.iter().filter(|(_, d)| d.paired && d.link.is_some()).map(|(id, _)| id.clone()).collect()
    };
    for id in ids {
        send_command_list(st, &id);
    }
}

fn send_command_list(st: &Shared, id: &str) {
    let list = command_list(st);
    lock(st).commands_sent = list.clone();
    st.phone.send(id, packet("kdeconnect.runcommand", json!({ "commandList": list, "canAddCommand": false })));
}

fn command_list(st: &Shared) -> String {
    let mut map = serde_json::Map::new();
    for (k, name) in BUILTIN {
        map.insert(k.to_string(), json!({ "name": name, "command": format!("desktop-island:{k}") }));
    }
    for (i, c) in st.with_settings(|s| s.phone.custom_commands.clone()).into_iter().enumerate() {
        map.insert(format!("custom:{i}"), json!({ "name": c.name, "command": c.target }));
    }
    Value::Object(map).to_string()
}

fn run_command(st: &Shared, key: &str) {
    let app = st.events.app_handle();
    let result: AppResult<()> = match key {
        "lock" => os::input::lock(),
        "sleep" => os::input::sleep(),
        "screen_off" => os::input::screen_off(),
        "mute" => os::audio::state().and_then(|v| os::audio::set_mute(!v.muted)),
        "play_pause" => {
            os::input::media_play_pause();
            Ok(())
        }
        "dnd" | "island" => {
            if let Some(app) = app {
                crate::core::hotkeys::run_action(&app, st, if key == "dnd" { "toggle_dnd" } else { "toggle_island" });
            }
            Ok(())
        }
        k if k.starts_with("custom:") => {
            let idx: usize = k[7..].parse().unwrap_or(usize::MAX);
            match st.with_settings(|s| s.phone.custom_commands.get(idx).cloned()) {
                Some(c) => os::shell::open_uri(&c.target),
                None => Err(AppError::NotFound("команда".into())),
            }
        }
        _ => Err(AppError::NotFound("команда".into())),
    };
    if let Err(e) = result {
        st.log.warn(crate::logging::category::INTEGRATION, "phone", format!("команда с телефона «{key}» не выполнена: {e}"));
    }
}

// ---- мышь и клавиатура ----

fn mousepad(body: &Value) {
    let b = |k: &str| body.get(k).and_then(|x| x.as_bool()).unwrap_or(false);
    let f = |k: &str| body.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0);
    let (ctrl, alt, shift, win) = (b("ctrl"), b("alt"), b("shift"), b("super"));
    if b("singleclick") {
        os::input::click("left", false);
    } else if b("doubleclick") {
        os::input::click("left", true);
    } else if b("middleclick") {
        os::input::click("middle", false);
    } else if b("rightclick") {
        os::input::click("right", false);
    } else if b("singlehold") {
        os::input::left_button(true);
    } else if b("singlerelease") {
        os::input::left_button(false);
    } else if b("scroll") {
        // Телефон присылает смещение пальцев в пикселях: ~20 px — один «щелчок» колеса.
        os::input::scroll(f("dx") / 20.0, f("dy") / 20.0);
    } else if let Some(code) = body.get("specialKey").and_then(|x| x.as_u64()).filter(|c| *c > 0) {
        os::input::special_key(code as u32, ctrl, alt, shift, win);
    } else if let Some(text) = body.get("key").and_then(|x| x.as_str()).filter(|t| !t.is_empty()) {
        let text: String = text.chars().take(200).collect();
        let mut chars = text.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) => os::input::key_with_mods(c, ctrl, alt, shift, win),
            _ => os::input::type_text(&text),
        }
    } else if body.get("dx").is_some() || body.get("dy").is_some() {
        os::input::mouse_move(f("dx"), f("dy"));
    }
}

// ---- уведомления ----

/// Пользователь закрыл на компьютере уведомление телефона — закрываем и там.
pub fn cancel_notification(st: &Shared, source: &str) {
    if !st.with_settings(|s| s.phone.sync_dismiss) {
        return;
    }
    // Источник уведомления телефона: phone:<устройство>:<id уведомления>.
    let Some(rest) = source.strip_prefix("phone:") else { return };
    let Some((dev, nid)) = rest.split_once(':') else { return };
    if nid.is_empty() || matches!(nid, "missed" | "ping" | "find" | "share") || nid.starts_with("file") {
        return;
    }
    st.phone.send(dev, packet("kdeconnect.notification.request", json!({ "cancel": nid })));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_match() {
        assert_eq!(norm("+7 (999) 123-45-67"), "9991234567");
        assert_eq!(norm("89991234567"), "9991234567");
        assert_eq!(norm("123"), "123");
    }
}
