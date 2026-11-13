//! Связь с телефоном по протоколу KDE Connect (на телефоне — бесплатное
//! приложение KDE Connect из Google Play / F-Droid, оно же есть для iOS).
//!
//! Что умеет: заряд телефона, уведомления телефона в острове (с группировкой),
//! входящие и пропущенные звонки (с кнопкой «Без звука»), общий буфер обмена,
//! «Найти телефон», пинг, ссылки и текст, присланные с телефона.
//! Всё работает только в локальной сети и только с сопряжёнными устройствами:
//! отпечаток сертификата телефона запоминается при сопряжении.

mod net;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::AppHandle;
use tokio::sync::mpsc;

use crate::core::error::{AppError, AppResult};
use crate::core::state::Shared;
use crate::core::task::TaskSlot;
use crate::events::{IslandNotification, NotificationAction, NotificationKind, Priority};
use crate::logging::category;

pub const EVENT: &str = "phone://status";

const INCOMING: &[&str] = &[
    "kdeconnect.ping",
    "kdeconnect.battery",
    "kdeconnect.notification",
    "kdeconnect.telephony",
    "kdeconnect.clipboard",
    "kdeconnect.clipboard.connect",
    "kdeconnect.share.request",
    "kdeconnect.findmyphone.request",
];
const OUTGOING: &[&str] = &[
    "kdeconnect.ping",
    "kdeconnect.battery.request",
    "kdeconnect.telephony.request_mute",
    "kdeconnect.clipboard",
    "kdeconnect.clipboard.connect",
    "kdeconnect.findmyphone.request",
    "kdeconnect.notification.reply",
    "kdeconnect.share.request",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Pairing {
    #[default]
    None,
    /// Мы попросили телефон о сопряжении и ждём ответа.
    Requested,
    /// Телефон просит о сопряжении — ждём решения пользователя.
    Incoming,
}

#[derive(Default)]
struct Device {
    name: String,
    kind: String,
    paired: bool,
    pairing: Pairing,
    battery: Option<(i32, bool)>,
    low_warned: bool,
    /// Открытое соединение: (номер поколения, отправитель строк).
    link: Option<(u64, mpsc::UnboundedSender<String>)>,
    fingerprint: Option<String>,
    /// Адрес телефона в сети (для передачи файлов).
    addr: Option<std::net::IpAddr>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Trusted {
    name: String,
    kind: String,
    fingerprint: String,
}

#[derive(Default)]
struct Inner {
    devices: HashMap<String, Device>,
    available: bool,
    message: String,
    gen: u64,
    /// Последний текст буфера, пришедший с телефона или ушедший на него (без «эха»).
    last_clip: String,
    my_id: String,
    /// Недавние попытки подключения по UDP-объявлению (чтобы не долбить телефон).
    attempts: HashMap<String, i64>,
    /// Уведомления телефона, на которые можно ответить: источник → requestReplyId.
    replies: HashMap<String, (String, String)>,
}

#[derive(Default)]
pub struct PhoneModule {
    task: TaskSlot,
    inner: Mutex<Inner>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhoneDevice {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub paired: bool,
    pub reachable: bool,
    /// none | requested | incoming
    pub pairing: String,
    pub battery: Option<i32>,
    pub charging: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhoneStatus {
    pub enabled: bool,
    pub available: bool,
    pub message: String,
    pub device_name: String,
    pub devices: Vec<PhoneDevice>,
}

fn now_ms() -> i64 {
    crate::database::now_ms()
}

fn packet(kind: &str, body: Value) -> String {
    json!({ "id": now_ms(), "type": kind, "body": body }).to_string()
}

fn host_name() -> String {
    let raw = std::env::var("COMPUTERNAME")
        .ok()
        .or_else(|| std::fs::read_to_string("/etc/hostname").ok())
        .or_else(|| std::env::var("HOSTNAME").ok())
        .unwrap_or_default();
    let name = raw.trim();
    if name.is_empty() {
        "Desktop Island".into()
    } else {
        name.to_string()
    }
}

fn device_name(st: &Shared) -> String {
    let custom = st.with_settings(|s| s.phone.device_name.clone());
    if custom.trim().is_empty() {
        host_name()
    } else {
        custom
    }
}

fn phone_dir(st: &Shared) -> std::path::PathBuf {
    st.paths.data.join("phone")
}

fn load_trusted(st: &Shared) -> HashMap<String, Trusted> {
    std::fs::read_to_string(phone_dir(st).join("trusted.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn save_trusted(st: &Shared, map: &HashMap<String, Trusted>) {
    let _ = std::fs::create_dir_all(phone_dir(st));
    if let Ok(s) = serde_json::to_string_pretty(map) {
        let _ = std::fs::write(phone_dir(st).join("trusted.json"), s);
    }
}

fn identity_line(st: &Shared, my_id: &str, tcp_port: u16) -> String {
    packet(
        "kdeconnect.identity",
        json!({
            "deviceId": my_id,
            "deviceName": device_name(st),
            "deviceType": "desktop",
            "protocolVersion": 7,
            "incomingCapabilities": INCOMING,
            "outgoingCapabilities": OUTGOING,
            "tcpPort": tcp_port,
        }),
    )
}

/// identity из пакета: (deviceId, deviceName, deviceType, tcpPort).
fn parse_identity(line: &str) -> Option<(String, String, String, u16)> {
    let v: Value = serde_json::from_str(line).ok()?;
    if v.get("type")?.as_str()? != "kdeconnect.identity" {
        return None;
    }
    let b = v.get("body")?;
    let id = b.get("deviceId")?.as_str()?.to_string();
    if !net::valid_device_id(&id) {
        return None;
    }
    let name: String = b.get("deviceName").and_then(|n| n.as_str()).unwrap_or("Телефон").chars().filter(|c| !c.is_control()).take(48).collect();
    let kind = b.get("deviceType").and_then(|n| n.as_str()).unwrap_or("phone").chars().take(16).collect();
    let port = b.get("tcpPort").and_then(|p| p.as_u64()).and_then(|p| u16::try_from(p).ok()).unwrap_or(net::UDP_PORT);
    Some((id, name, kind, port))
}

impl PhoneModule {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn status(&self, st: &Shared) -> PhoneStatus {
        let g = self.lock();
        let mut devices: Vec<PhoneDevice> = g
            .devices
            .iter()
            .filter(|(_, d)| d.paired || d.link.is_some())
            .map(|(id, d)| PhoneDevice {
                id: id.clone(),
                name: d.name.clone(),
                kind: d.kind.clone(),
                paired: d.paired,
                reachable: d.link.is_some(),
                pairing: match d.pairing {
                    Pairing::None => "none",
                    Pairing::Requested => "requested",
                    Pairing::Incoming => "incoming",
                }
                .into(),
                battery: d.battery.map(|b| b.0),
                charging: d.battery.map(|b| b.1).unwrap_or(false),
            })
            .collect();
        devices.sort_by(|a, b| b.paired.cmp(&a.paired).then(b.reachable.cmp(&a.reachable)).then(a.name.cmp(&b.name)));
        PhoneStatus {
            enabled: st.with_settings(|s| s.phone.enabled),
            available: g.available,
            message: g.message.clone(),
            device_name: device_name(st),
            devices,
        }
    }

    fn emit(&self, st: &Shared) {
        st.events.emit(EVENT, self.status(st));
    }

    fn send(&self, id: &str, line: String) -> bool {
        let g = self.lock();
        match g.devices.get(id).and_then(|d| d.link.as_ref()) {
            Some((_, tx)) => tx.send(line).is_ok(),
            None => false,
        }
    }

    pub fn set_running(&self, app: &AppHandle, st: &Shared, run: bool) {
        let _ = app;
        if !run {
            if self.task.is_running() {
                self.task.stop();
                let mut g = self.lock();
                for d in g.devices.values_mut() {
                    d.link = None;
                }
                g.available = false;
                g.message = "Связь с телефоном выключена".into();
                drop(g);
                self.emit(st);
            }
            return;
        }
        if self.task.is_running() {
            return;
        }
        {
            let trusted = load_trusted(st);
            let mut g = self.lock();
            for (id, t) in trusted {
                let d = g.devices.entry(id).or_default();
                d.name = t.name;
                d.kind = t.kind;
                d.paired = true;
                d.fingerprint = Some(t.fingerprint);
            }
        }
        let st = st.clone();
        self.task.start(async move {
            if let Err(e) = run_service(st.clone()).await {
                st.log.warn(category::INTEGRATION, "phone", format!("связь с телефоном недоступна: {e}"));
                let mut g = st.phone.lock();
                g.available = false;
                g.message = format!("Связь с телефоном недоступна: {e}");
                drop(g);
                st.phone.emit(&st);
            }
        });
    }

    /// Попросить телефон о сопряжении.
    pub fn pair(&self, st: &Shared, id: &str) -> AppResult<()> {
        if !self.send(id, packet("kdeconnect.pair", json!({ "pair": true, "timestamp": now_ms() / 1000 }))) {
            return Err(AppError::invalid("телефон сейчас не в сети"));
        }
        if let Some(d) = self.lock().devices.get_mut(id) {
            d.pairing = Pairing::Requested;
        }
        self.emit(st);
        Ok(())
    }

    pub fn unpair(&self, st: &Shared, id: &str) -> AppResult<()> {
        self.send(id, packet("kdeconnect.pair", json!({ "pair": false })));
        let mut trusted = load_trusted(st);
        trusted.remove(id);
        save_trusted(st, &trusted);
        {
            let mut g = self.lock();
            let remove = match g.devices.get_mut(id) {
                Some(d) => {
                    d.paired = false;
                    d.pairing = Pairing::None;
                    d.fingerprint = None;
                    d.link.is_none()
                }
                None => false,
            };
            if remove {
                g.devices.remove(id);
            }
        }
        self.emit(st);
        Ok(())
    }

    /// Ответ на просьбу телефона о сопряжении.
    pub fn accept(&self, st: &Shared, id: &str, yes: bool) -> AppResult<()> {
        let incoming = self.lock().devices.get(id).map(|d| d.pairing == Pairing::Incoming).unwrap_or(false);
        if !incoming {
            return Err(AppError::invalid("запрос на сопряжение уже неактуален"));
        }
        self.send(id, packet("kdeconnect.pair", json!({ "pair": yes, "timestamp": now_ms() / 1000 })));
        if yes {
            self.mark_paired(st, id);
        } else if let Some(d) = self.lock().devices.get_mut(id) {
            d.pairing = Pairing::None;
        }
        st.events.dismiss_source(&format!("phone-pair:{id}"));
        self.emit(st);
        Ok(())
    }

    /// Телефон зазвонит, даже если стоит беззвучный режим.
    pub fn ring(&self, id: &str) -> AppResult<()> {
        if self.send(id, packet("kdeconnect.findmyphone.request", json!({}))) {
            Ok(())
        } else {
            Err(AppError::invalid("телефон сейчас не в сети"))
        }
    }

    fn mark_paired(&self, st: &Shared, id: &str) {
        let mut trusted = load_trusted(st);
        let line = {
            let mut g = self.lock();
            let Some(d) = g.devices.get_mut(id) else { return };
            d.paired = true;
            d.pairing = Pairing::None;
            if let Some(fp) = d.fingerprint.clone() {
                trusted.insert(id.to_string(), Trusted { name: d.name.clone(), kind: d.kind.clone(), fingerprint: fp });
            }
            d.name.clone()
        };
        save_trusted(st, &trusted);
        st.log.info(category::INTEGRATION, "phone", format!("телефон «{line}» сопряжён"));
        st.events.push(crate::events::internal_notification("phone", "Телефон", format!("{line} подключён"), "Уведомления, звонки и буфер обмена теперь в острове", Priority::Normal));
        self.send(id, packet("kdeconnect.battery.request", json!({ "request": true })));
    }

    /// Текст скопирован на компьютере — отправляем на сопряжённые телефоны.
    pub fn clipboard_from_pc(&self, st: &Shared, text: &str) {
        if !st.with_settings(|s| s.phone.enabled && s.phone.clipboard) || text.is_empty() || text.len() > 1_000_000 {
            return;
        }
        let targets: Vec<mpsc::UnboundedSender<String>> = {
            let mut g = self.lock();
            if g.last_clip == text {
                return;
            }
            g.last_clip = text.to_string();
            g.devices.values().filter(|d| d.paired).filter_map(|d| d.link.as_ref().map(|l| l.1.clone())).collect()
        };
        let line = packet("kdeconnect.clipboard", json!({ "content": text }));
        for tx in targets {
            let _ = tx.send(line.clone());
        }
    }

    /// Кнопки уведомлений, пришедших с телефона.
    pub fn action(&self, st: &Shared, source: &str, action: &str) -> AppResult<()> {
        if let Some(id) = source.strip_prefix("phone-pair:") {
            return self.accept(st, id, action == "accept");
        }
        if let Some(text) = action.strip_prefix("reply:") {
            let target = self.lock().replies.get(source).cloned();
            let Some((dev, reply_id)) = target else { return Err(AppError::invalid("на это сообщение уже нельзя ответить")) };
            let text: String = text.trim().chars().take(2000).collect();
            if text.is_empty() {
                return Err(AppError::invalid("пустой ответ"));
            }
            if !self.send(&dev, packet("kdeconnect.notification.reply", json!({ "requestReplyId": reply_id, "message": text }))) {
                return Err(AppError::invalid("телефон сейчас не в сети"));
            }
            return Ok(());
        }
        if let Some(id) = source.strip_prefix("phone-call:") {
            if action == "mute" {
                self.send(id, packet("kdeconnect.telephony.request_mute", json!({})));
            }
            return Ok(());
        }
        Err(AppError::unsupported("действие не поддерживается"))
    }
}

/// Главный цикл: TCP-приёмник, UDP-обнаружение и периодическое объявление о себе.
async fn run_service(st: Shared) -> Result<(), String> {
    let dir = phone_dir(&st);
    let ident = tauri::async_runtime::spawn_blocking(move || net::Identity::load_or_create(&dir)).await.map_err(|e| e.to_string())??;
    let ident = Arc::new(ident);
    let server_cfg = ident.server_config()?;
    let client_cfg = ident.client_config()?;
    let (listener, tcp_port) = net::bind_tcp().await.map_err(|e| format!("порт TCP 1716–1764 занят ({e})"))?;
    let udp = net::bind_udp().map_err(|e| format!("порт UDP 1716 недоступен ({e})"));
    {
        let mut g = st.phone.lock();
        g.my_id = ident.device_id.clone();
        g.available = true;
        g.message = match &udp {
            Ok(_) => "Ожидание телефона в сети".into(),
            Err(e) => format!("Поиск телефона ограничен: {e}. Телефон всё равно может подключиться сам"),
        };
    }
    st.phone.emit(&st);
    st.log.info(category::INTEGRATION, "phone", format!("связь с телефоном: порт {tcp_port}"));
    let udp = udp.ok();
    let mut ticker = tokio::time::interval(Duration::from_secs(30));
    let mut buf = vec![0u8; 65_536];
    loop {
        tokio::select! {
            accepted = listener.accept() => {
                if let Ok((stream, addr)) = accepted {
                    let (st, cfg) = (st.clone(), client_cfg.clone());
                    tauri::async_runtime::spawn(async move { incoming(st, cfg, stream, addr).await });
                }
            }
            got = async {
                match &udp {
                    Some(u) => u.recv_from(&mut buf).await.map(|(n, a)| (buf[..n].to_vec(), a)),
                    None => std::future::pending().await,
                }
            } => {
                if let Ok((data, addr)) = got {
                    let line = String::from_utf8_lossy(&data);
                    if let Some((id, name, kind, port)) = parse_identity(line.trim()) {
                        if should_connect(&st, &id) {
                            let (st, cfg, line) = (st.clone(), server_cfg.clone(), identity_line(&st, &ident.device_id, tcp_port));
                            let target = SocketAddr::new(addr.ip(), port);
                            tauri::async_runtime::spawn(async move { outgoing(st, cfg, target, line, (id, name, kind)).await });
                        }
                    }
                }
            }
            _ = ticker.tick() => {
                if let Some(u) = &udp {
                    net::broadcast(u, &identity_line(&st, &ident.device_id, tcp_port)).await;
                }
            }
        }
    }
}

fn should_connect(st: &Shared, id: &str) -> bool {
    let mut g = st.phone.lock();
    if *id == g.my_id {
        return false;
    }
    if g.devices.get(id).map(|d| d.link.is_some()).unwrap_or(false) {
        return false;
    }
    let now = now_ms();
    if g.attempts.get(id).map(|t| now - t < 5000).unwrap_or(false) {
        return false;
    }
    g.attempts.insert(id.to_string(), now);
    true
}

/// Телефон подключился к нам: читаем его identity и начинаем TLS как клиент.
async fn incoming(st: Shared, cfg: Arc<rustls::ClientConfig>, mut stream: tokio::net::TcpStream, addr: SocketAddr) {
    let _ = stream.set_nodelay(true);
    let Ok(Ok(line)) = tokio::time::timeout(Duration::from_secs(10), net::read_plain_line(&mut stream)).await else { return };
    let Some((id, name, kind, _)) = parse_identity(&line) else { return };
    if id == st.phone.lock().my_id {
        return;
    }
    let server_name = rustls::pki_types::ServerName::IpAddress(addr.ip().into());
    let connector = tokio_rustls::TlsConnector::from(cfg);
    match tokio::time::timeout(Duration::from_secs(15), connector.connect(server_name, stream)).await {
        Ok(Ok(tls)) => {
            let cert = tls.get_ref().1.peer_certificates().and_then(|c| c.first()).map(|c| c.to_vec());
            run_link(st, (id, name, kind), tls, cert, addr.ip()).await;
        }
        Ok(Err(e)) => st.log.warn(category::INTEGRATION, "phone", format!("TLS с «{name}» не установлен: {e}")),
        Err(_) => {}
    }
}

/// Мы подключаемся к телефону, объявившему себя по UDP: TLS как сервер.
async fn outgoing(st: Shared, cfg: Arc<rustls::ServerConfig>, target: SocketAddr, my_identity: String, peer: (String, String, String)) {
    let Ok(Ok(mut stream)) = tokio::time::timeout(Duration::from_secs(5), tokio::net::TcpStream::connect(target)).await else { return };
    let _ = stream.set_nodelay(true);
    if net::write_line(&mut stream, &my_identity).await.is_err() {
        return;
    }
    let acceptor = tokio_rustls::TlsAcceptor::from(cfg);
    match tokio::time::timeout(Duration::from_secs(15), acceptor.accept(stream)).await {
        Ok(Ok(tls)) => {
            let cert = tls.get_ref().1.peer_certificates().and_then(|c| c.first()).map(|c| c.to_vec());
            run_link(st, peer, tls, cert, target.ip()).await;
        }
        Ok(Err(e)) => st.log.warn(category::INTEGRATION, "phone", format!("TLS с «{}» не установлен: {e}", peer.1)),
        Err(_) => {}
    }
}

/// Установленное соединение с телефоном: чтение пакетов и отправка из очереди.
async fn run_link<S>(st: Shared, peer: (String, String, String), stream: S, cert: Option<Vec<u8>>, addr: std::net::IpAddr)
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    let (id, name, kind) = peer;
    let Some(fp) = cert.as_deref().map(net::fingerprint) else { return };
    let (tx, mut rx) = mpsc::unbounded_channel::<String>();
    let (gen, paired) = {
        let mut g = st.phone.lock();
        g.gen += 1;
        let gen = g.gen;
        let d = g.devices.entry(id.clone()).or_default();
        if d.paired && d.fingerprint.as_deref().is_some_and(|known| known != fp) {
            drop(g);
            st.log.warn(category::PERMISSION, "phone", format!("«{name}»: сертификат не совпадает с сопряжённым — соединение отклонено"));
            return;
        }
        d.name = name.clone();
        d.kind = kind;
        d.fingerprint = Some(fp);
        d.link = Some((gen, tx.clone()));
        d.addr = Some(addr);
        let paired = d.paired;
        g.message = String::new();
        (gen, paired)
    };
    st.phone.emit(&st);
    if paired {
        let _ = tx.send(packet("kdeconnect.battery.request", json!({ "request": true })));
    }
    let (read, mut write) = tokio::io::split(stream);
    let writer = tauri::async_runtime::spawn(async move {
        while let Some(line) = rx.recv().await {
            if net::write_line(&mut write, &line).await.is_err() {
                break;
            }
        }
    });
    let mut reader = net::LineReader::new(read);
    while let Ok(Some(line)) = reader.next_line().await {
        if let Ok(v) = serde_json::from_str::<Value>(&line) {
            handle_packet(&st, &id, &v);
        }
    }
    writer.abort();
    {
        let mut g = st.phone.lock();
        if let Some(d) = g.devices.get_mut(&id) {
            if d.link.as_ref().map(|l| l.0) == Some(gen) {
                d.link = None;
                if d.pairing == Pairing::Requested {
                    d.pairing = Pairing::None;
                }
            }
        }
    }
    st.phone.emit(&st);
}

fn phone_notification(source: String, app: &str, title: String, body: String, priority: Priority, actions: Vec<NotificationAction>) -> IslandNotification {
    IslandNotification {
        stack: Vec::new(),
        count: 1,
        id: uuid::Uuid::new_v4().to_string(),
        kind: NotificationKind::Internal,
        source,
        app_name: app.chars().take(64).collect(),
        icon: None,
        title: crate::util::clean_text(&title, 200),
        body: crate::util::clean_text(&body, 1000),
        priority,
        timestamp: now_ms(),
        actions,
        open_target: None,
        duration_ms: 0,
    }
}

fn act(id: &str, label: &str) -> NotificationAction {
    NotificationAction { id: id.into(), label: label.into() }
}

fn handle_packet(st: &Shared, id: &str, v: &Value) {
    let kind = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
    let body = v.get("body").cloned().unwrap_or(Value::Null);
    let s = st.with_settings(|s| s.phone.clone());
    let (paired, name) = {
        let g = st.phone.lock();
        let d = g.devices.get(id);
        (d.map(|d| d.paired).unwrap_or(false), d.map(|d| d.name.clone()).unwrap_or_default())
    };

    if kind == "kdeconnect.pair" {
        let want = body.get("pair").and_then(|p| p.as_bool()).unwrap_or(false);
        let pairing = st.phone.lock().devices.get(id).map(|d| d.pairing).unwrap_or_default();
        if want && pairing == Pairing::Requested {
            st.phone.mark_paired(st, id);
        } else if want && !paired {
            if let Some(d) = st.phone.lock().devices.get_mut(id) {
                d.pairing = Pairing::Incoming;
            }
            st.events.push(phone_notification(
                format!("phone-pair:{id}"),
                "Телефон",
                format!("{name} хочет подключиться"),
                "Сравните, что это ваш телефон, и подтвердите".into(),
                Priority::High,
                vec![act("accept", "Подключить"), act("reject", "Отклонить")],
            ));
        } else if !want {
            let mut trusted = load_trusted(st);
            trusted.remove(id);
            save_trusted(st, &trusted);
            if let Some(d) = st.phone.lock().devices.get_mut(id) {
                d.paired = false;
                d.pairing = Pairing::None;
            }
        }
        st.phone.emit(st);
        return;
    }
    // Всё остальное — только от сопряжённых телефонов.
    if !paired {
        return;
    }
    let text = |k: &str| body.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    match kind {
        "kdeconnect.identity" => {
            // Протокол 8 присылает identity ещё раз уже внутри TLS — обновляем имя.
            if let Some(d) = st.phone.lock().devices.get_mut(id) {
                let n = text("deviceName");
                if !n.is_empty() {
                    d.name = n.chars().take(48).collect();
                }
            }
        }
        "kdeconnect.battery" => {
            let charge = body.get("currentCharge").and_then(|c| c.as_i64()).unwrap_or(-1) as i32;
            let charging = body.get("isCharging").and_then(|c| c.as_bool()).unwrap_or(false);
            let warn = {
                let mut g = st.phone.lock();
                let Some(d) = g.devices.get_mut(id) else { return };
                d.battery = (charge >= 0).then_some((charge, charging));
                let low = charge >= 0 && charge <= 15 && !charging;
                let warn = low && !d.low_warned;
                if low {
                    d.low_warned = true;
                } else if charging || charge > 20 {
                    d.low_warned = false;
                }
                warn
            };
            if warn && s.battery {
                st.events.push(phone_notification(format!("phone-battery:{id}"), "Телефон", format!("{name} разряжается"), format!("Осталось {charge}%"), Priority::High, vec![]));
            }
            st.phone.emit(st);
        }
        "kdeconnect.notification" => {
            if !s.notifications {
                return;
            }
            let nid = text("id");
            if body.get("isCancel").and_then(|c| c.as_bool()).unwrap_or(false) {
                st.events.dismiss_source(&format!("phone:{id}:{nid}"));
                return;
            }
            // «Тихие» — уже висевшие в шторке на момент подключения.
            if body.get("silent").and_then(|c| c.as_bool()).unwrap_or(false) {
                return;
            }
            let app = text("appName");
            let (title, body_text) = (text("title"), text("text"));
            let title = if title.is_empty() { text("ticker") } else { title };
            if title.is_empty() && body_text.is_empty() {
                return;
            }
            let app = if app.is_empty() { name.clone() } else { app };
            let source = format!("phone:{id}:{nid}");
            let reply = text("requestReplyId");
            let actions = if reply.is_empty() {
                vec![]
            } else {
                let mut g = st.phone.lock();
                if g.replies.len() > 200 {
                    g.replies.clear();
                }
                g.replies.insert(source.clone(), (id.to_string(), reply));
                vec![act("reply", "Ответить")]
            };
            st.events.push(phone_notification(source, &app, title, body_text, Priority::Normal, actions));
        }
        "kdeconnect.telephony" => {
            if !s.calls {
                return;
            }
            let event = text("event");
            let who = {
                let c = text("contactName");
                if c.is_empty() { text("phoneNumber") } else { c }
            };
            let who = if who.is_empty() { "Неизвестный номер".to_string() } else { who };
            let source = format!("phone-call:{id}");
            let cancel = body.get("isCancel").and_then(|c| c.as_bool()).unwrap_or(false);
            match (event.as_str(), cancel) {
                ("ringing", false) => {
                    let mut n = phone_notification(source, "Звонок", who, format!("Входящий звонок на {name}"), Priority::Critical, vec![act("mute", "Без звука")]);
                    n.duration_ms = 30_000;
                    st.events.push(n);
                }
                ("ringing", true) | ("talking", _) => st.events.dismiss_source(&source),
                ("missedCall", false) => {
                    st.events.dismiss_source(&source);
                    st.events.push(phone_notification(format!("phone:{id}:missed"), "Пропущенный звонок", who, name.clone(), Priority::High, vec![]));
                }
                _ => {}
            }
        }
        "kdeconnect.clipboard" | "kdeconnect.clipboard.connect" => {
            if !s.clipboard {
                return;
            }
            let content = text("content");
            if content.is_empty() {
                return;
            }
            {
                let mut g = st.phone.lock();
                if g.last_clip == content {
                    return;
                }
                g.last_clip = content.clone();
            }
            std::thread::spawn(move || {
                if let Ok(mut cb) = arboard::Clipboard::new() {
                    let _ = cb.set_text(content);
                }
            });
        }
        "kdeconnect.ping" => {
            let msg = text("message");
            st.events.push(phone_notification(format!("phone:{id}:ping"), &name, "Пинг".into(), msg, Priority::Normal, vec![]));
        }
        "kdeconnect.findmyphone.request" => {
            st.events.push(phone_notification(format!("phone:{id}:find"), &name, "Телефон ищет компьютер".into(), String::new(), Priority::Critical, vec![]));
        }
        "kdeconnect.share.request" => {
            let url = text("url");
            let shared = text("text");
            if !url.is_empty() && (url.starts_with("https://") || url.starts_with("http://")) {
                let _ = crate::platform::os::shell::open_uri(&url);
                st.events.push(phone_notification(format!("phone:{id}:share"), &name, "Открыта ссылка с телефона".into(), url, Priority::Normal, vec![]));
            } else if !shared.is_empty() {
                st.phone.lock().last_clip = shared.clone();
                let copy = shared.clone();
                std::thread::spawn(move || {
                    if let Ok(mut cb) = arboard::Clipboard::new() {
                        let _ = cb.set_text(copy);
                    }
                });
                st.events.push(phone_notification(format!("phone:{id}:share"), &name, "Текст с телефона скопирован".into(), shared, Priority::Normal, vec![]));
            } else if let Some(file) = body.get("filename").and_then(|f| f.as_str()) {
                let size = v.get("payloadSize").and_then(|s| s.as_i64()).unwrap_or(-1);
                let port = v.get("payloadTransferInfo").and_then(|p| p.get("port")).and_then(|p| p.as_u64()).and_then(|p| u16::try_from(p).ok());
                let addr = st.phone.lock().devices.get(id).and_then(|d| d.addr);
                match (port, addr) {
                    (Some(port), Some(ip)) => {
                        let (st2, file, name2) = (st.clone(), file.to_string(), name.clone());
                        tauri::async_runtime::spawn(async move { receive_file(st2, SocketAddr::new(ip, port), file, size, name2).await });
                    }
                    _ => {
                        st.events.push(phone_notification(format!("phone:{id}:share"), &name, "Файл не принят".into(), "Телефон не сообщил, откуда забрать файл".into(), Priority::Normal, vec![]));
                    }
                }
            }
        }
        _ => {}
    }
}

/// Безопасное имя файла с телефона (без путей и запрещённых символов).
fn safe_file_name(name: &str) -> String {
    let base = name.rsplit(['/', '\\']).next().unwrap_or("file");
    let clean: String = base.chars().filter(|c| !c.is_control() && !matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*')).take(120).collect();
    let clean = clean.trim().trim_start_matches('.').to_string();
    if clean.is_empty() { "файл с телефона".into() } else { clean }
}

/// Принимает файл с телефона: подключаемся к его порту, TLS как клиент,
/// сохраняем в «Загрузки» и кладём на полку.
async fn receive_file(st: Shared, target: SocketAddr, file: String, size: i64, device: String) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let dir = crate::modules::downloads::downloads_dir().unwrap_or_else(|| st.paths.data.join("phone-files"));
    let _ = std::fs::create_dir_all(&dir);
    let name = safe_file_name(&file);
    let mut path = dir.join(&name);
    let mut n = 1;
    while path.exists() {
        let (stem, ext) = match name.rsplit_once('.') {
            Some((s, e)) => (s.to_string(), format!(".{e}")),
            None => (name.clone(), String::new()),
        };
        path = dir.join(format!("{stem} ({n}){ext}"));
        n += 1;
    }
    let result: Result<u64, String> = async {
        let dir = phone_dir(&st);
        let ident = net::Identity::load_or_create(&dir)?;
        let cfg = ident.client_config()?;
        let stream = tokio::time::timeout(Duration::from_secs(10), tokio::net::TcpStream::connect(target)).await.map_err(|_| "телефон не отвечает".to_string())?.map_err(|e| e.to_string())?;
        let mut tls = tokio_rustls::TlsConnector::from(cfg).connect(rustls::pki_types::ServerName::IpAddress(target.ip().into()), stream).await.map_err(|e| e.to_string())?;
        let mut out = tokio::fs::File::create(&path).await.map_err(|e| e.to_string())?;
        let mut buf = vec![0u8; 64 * 1024];
        let mut total: u64 = 0;
        loop {
            let n = tls.read(&mut buf).await.unwrap_or(0);
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n]).await.map_err(|e| e.to_string())?;
            total += n as u64;
            if size >= 0 && total >= size as u64 {
                break;
            }
        }
        out.flush().await.map_err(|e| e.to_string())?;
        if size >= 0 && total < size as u64 {
            return Err("передача прервалась".into());
        }
        Ok(total)
    }
    .await;
    match result {
        Ok(bytes) => {
            let p = path.to_string_lossy().to_string();
            let _ = crate::modules::shelf::add(&st, &[p]);
            let size = crate::modules::downloads::format_size(bytes as f64);
            st.events.push(phone_notification(format!("phone:file:{}", now_ms()), &device, format!("Файл с телефона: {}", path.file_name().map(|f| f.to_string_lossy()).unwrap_or_default()), format!("{size} · лежит на полке и в «Загрузках»"), Priority::Normal, vec![]));
        }
        Err(e) => {
            let _ = std::fs::remove_file(&path);
            st.events.push(phone_notification(format!("phone:file:{}", now_ms()), &device, "Файл не получен".into(), e, Priority::Normal, vec![]));
        }
    }
}

impl PhoneModule {
    /// Отправляет файл на сопряжённый телефон (первый в сети): KDE Connect
    /// share.request с портом, к которому телефон подключится за содержимым.
    pub async fn send_file(st: Shared, path: std::path::PathBuf) -> AppResult<()> {
        use tokio::io::AsyncWriteExt;
        let meta = std::fs::metadata(&path)?;
        if !meta.is_file() {
            return Err(AppError::invalid("отправить можно только файл"));
        }
        let target = {
            let g = st.phone.lock();
            g.devices.iter().find(|(_, d)| d.paired && d.link.is_some()).map(|(id, d)| (id.clone(), d.name.clone()))
        };
        let Some((dev, dev_name)) = target else { return Err(AppError::invalid("нет сопряжённого телефона в сети")) };
        let ident = net::Identity::load_or_create(&phone_dir(&st)).map_err(AppError::Other)?;
        let cfg = ident.server_config().map_err(AppError::Other)?;
        let mut listener = None;
        for port in 1739u16..=1764 {
            if let Ok(l) = tokio::net::TcpListener::bind(("0.0.0.0", port)).await {
                listener = Some((l, port));
                break;
            }
        }
        let (listener, port) = listener.ok_or_else(|| AppError::Other("нет свободного порта для передачи".into()))?;
        let file_name = path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_else(|| "file".into());
        let line = json!({
            "id": now_ms(),
            "type": "kdeconnect.share.request",
            "body": { "filename": file_name, "open": false },
            "payloadSize": meta.len(),
            "payloadTransferInfo": { "port": port },
        })
        .to_string();
        if !st.phone.send(&dev, line) {
            return Err(AppError::invalid("телефон сейчас не в сети"));
        }
        let (stream, _) = tokio::time::timeout(Duration::from_secs(60), listener.accept())
            .await
            .map_err(|_| AppError::Other("телефон не забрал файл за минуту".into()))??;
        let mut tls = tokio_rustls::TlsAcceptor::from(cfg).accept(stream).await.map_err(|e| AppError::Other(e.to_string()))?;
        let mut file = tokio::fs::File::open(&path).await?;
        tokio::io::copy(&mut file, &mut tls).await?;
        tls.shutdown().await.ok();
        st.events.push(phone_notification(format!("phone:sent:{}", now_ms()), &dev_name, format!("Файл отправлен: {file_name}"), String::new(), Priority::Low, vec![]));
        Ok(())
    }
}
