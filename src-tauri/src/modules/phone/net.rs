//! Сеть протокола KDE Connect: обнаружение по UDP, TCP-соединения и TLS.
//!
//! Порядок установки связи (как в KDE Connect, протокол 7):
//! * устройство A рассылает по UDP (порт 1716) пакет `kdeconnect.identity`;
//! * устройство B, получив его, подключается по TCP к `tcpPort` из пакета,
//!   отправляет свой identity открытым текстом и начинает TLS **как сервер**;
//! * A читает identity и начинает TLS **как клиент**.
//!
//! Сертификаты самоподписанные; при первом сопряжении запоминается отпечаток
//! сертификата телефона, дальше соединение принимается только с ним.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::server::danger::{ClientCertVerified, ClientCertVerifier};
use rustls::{DigitallySignedStruct, DistinguishedName, SignatureScheme};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream, UdpSocket};

pub const UDP_PORT: u16 = 1716;
pub const TCP_PORTS: std::ops::RangeInclusive<u16> = 1716..=1764;
/// Защита от «бесконечной» строки: пакеты больше 4 МБ не принимаем.
const MAX_LINE: usize = 4 * 1024 * 1024;

fn provider() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

/// Проверка подписи рукопожатия — настоящая; доверие к самому сертификату
/// решается по отпечатку после соединения (как в KDE Connect).
#[derive(Debug)]
struct AnyCert(Arc<rustls::crypto::CryptoProvider>);

impl ServerCertVerifier for AnyCert {
    fn verify_server_cert(&self, _: &CertificateDer<'_>, _: &[CertificateDer<'_>], _: &ServerName<'_>, _: &[u8], _: UnixTime) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(&self, m: &[u8], c: &CertificateDer<'_>, d: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(m, c, d, &self.0.signature_verification_algorithms)
    }
    fn verify_tls13_signature(&self, m: &[u8], c: &CertificateDer<'_>, d: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(m, c, d, &self.0.signature_verification_algorithms)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

impl ClientCertVerifier for AnyCert {
    fn root_hint_subjects(&self) -> &[DistinguishedName] {
        &[]
    }
    fn verify_client_cert(&self, _: &CertificateDer<'_>, _: &[CertificateDer<'_>], _: UnixTime) -> Result<ClientCertVerified, rustls::Error> {
        Ok(ClientCertVerified::assertion())
    }
    fn client_auth_mandatory(&self) -> bool {
        true
    }
    fn verify_tls12_signature(&self, m: &[u8], c: &CertificateDer<'_>, d: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(m, c, d, &self.0.signature_verification_algorithms)
    }
    fn verify_tls13_signature(&self, m: &[u8], c: &CertificateDer<'_>, d: &DigitallySignedStruct) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(m, c, d, &self.0.signature_verification_algorithms)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}

/// Сертификат и ключ этого компьютера.
pub struct Identity {
    pub device_id: String,
    pub cert: CertificateDer<'static>,
    key_der: Vec<u8>,
}

impl Identity {
    /// Загружает или создаёт сертификат (CN = идентификатор устройства).
    pub fn load_or_create(dir: &std::path::Path) -> Result<Self, String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let id_path = dir.join("device_id");
        let cert_path = dir.join("certificate.der");
        let key_path = dir.join("private_key.der");
        if let (Ok(id), Ok(cert), Ok(key)) = (std::fs::read_to_string(&id_path), std::fs::read(&cert_path), std::fs::read(&key_path)) {
            let id = id.trim().to_string();
            if valid_device_id(&id) {
                return Ok(Self { device_id: id, cert: CertificateDer::from(cert), key_der: key });
            }
        }
        let device_id = uuid::Uuid::new_v4().simple().to_string();
        let mut params = rcgen::CertificateParams::new(Vec::<String>::new()).map_err(|e| e.to_string())?;
        let mut dn = rcgen::DistinguishedName::new();
        dn.push(rcgen::DnType::CommonName, device_id.clone());
        dn.push(rcgen::DnType::OrganizationName, "KDE");
        dn.push(rcgen::DnType::OrganizationalUnitName, "Kde connect");
        params.distinguished_name = dn;
        params.not_before = rcgen::date_time_ymd(2024, 1, 1);
        params.not_after = rcgen::date_time_ymd(2034, 1, 1);
        let key = rcgen::KeyPair::generate().map_err(|e| e.to_string())?;
        let cert = params.self_signed(&key).map_err(|e| e.to_string())?;
        let cert_der = cert.der().to_vec();
        let key_der = key.serialize_der();
        std::fs::write(&cert_path, &cert_der).map_err(|e| e.to_string())?;
        std::fs::write(&key_path, &key_der).map_err(|e| e.to_string())?;
        std::fs::write(&id_path, &device_id).map_err(|e| e.to_string())?;
        Ok(Self { device_id, cert: CertificateDer::from(cert_der), key_der })
    }

    fn key(&self) -> PrivateKeyDer<'static> {
        PrivateKeyDer::Pkcs8(self.key_der.clone().into())
    }

    pub fn server_config(&self) -> Result<Arc<rustls::ServerConfig>, String> {
        let p = provider();
        let cfg = rustls::ServerConfig::builder_with_provider(p.clone())
            .with_safe_default_protocol_versions()
            .map_err(|e| e.to_string())?
            .with_client_cert_verifier(Arc::new(AnyCert(p)))
            .with_single_cert(vec![self.cert.clone()], self.key())
            .map_err(|e| e.to_string())?;
        Ok(Arc::new(cfg))
    }

    pub fn client_config(&self) -> Result<Arc<rustls::ClientConfig>, String> {
        let p = provider();
        let cfg = rustls::ClientConfig::builder_with_provider(p.clone())
            .with_safe_default_protocol_versions()
            .map_err(|e| e.to_string())?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(AnyCert(p)))
            .with_client_auth_cert(vec![self.cert.clone()], self.key())
            .map_err(|e| e.to_string())?;
        Ok(Arc::new(cfg))
    }
}

/// Идентификатор устройства в KDE Connect: 32–38 символов из букв, цифр и `_`.
pub fn valid_device_id(id: &str) -> bool {
    (32..=38).contains(&id.len()) && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// SHA-256 отпечаток сертификата (hex).
pub fn fingerprint(der: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(der).iter().map(|b| format!("{b:02x}")).collect()
}

/// Читает одну строку пакета (до `\n`) без буферизации лишних байт —
/// после identity в том же потоке сразу начинается TLS.
pub async fn read_plain_line(stream: &mut TcpStream) -> std::io::Result<String> {
    let mut buf = Vec::with_capacity(512);
    let mut byte = [0u8; 1];
    loop {
        let n = stream.read(&mut byte).await?;
        if n == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        if byte[0] == b'\n' {
            break;
        }
        buf.push(byte[0]);
        if buf.len() > 64 * 1024 {
            return Err(std::io::ErrorKind::InvalidData.into());
        }
    }
    String::from_utf8(buf).map_err(|_| std::io::ErrorKind::InvalidData.into())
}

/// Чтение пакетов (строк) из зашифрованного потока.
pub struct LineReader<R> {
    inner: R,
    buf: Vec<u8>,
}

impl<R: AsyncRead + Unpin> LineReader<R> {
    pub fn new(inner: R) -> Self {
        Self { inner, buf: Vec::new() }
    }

    pub async fn next_line(&mut self) -> std::io::Result<Option<String>> {
        loop {
            if let Some(pos) = self.buf.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = self.buf.drain(..=pos).collect();
                return Ok(Some(String::from_utf8_lossy(&line[..line.len() - 1]).into_owned()));
            }
            if self.buf.len() > MAX_LINE {
                return Err(std::io::ErrorKind::InvalidData.into());
            }
            let mut chunk = [0u8; 8192];
            let n = self.inner.read(&mut chunk).await?;
            if n == 0 {
                return Ok(None);
            }
            self.buf.extend_from_slice(&chunk[..n]);
        }
    }
}

pub async fn write_line<W: AsyncWrite + Unpin>(w: &mut W, line: &str) -> std::io::Result<()> {
    w.write_all(line.as_bytes()).await?;
    w.write_all(b"\n").await?;
    w.flush().await
}

/// UDP-сокет на порту 1716 с SO_REUSEADDR (порт может делить установленный KDE Connect).
pub fn bind_udp() -> std::io::Result<UdpSocket> {
    use socket2::{Domain, Protocol, Socket, Type};
    let s = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    s.set_reuse_address(true)?;
    s.set_broadcast(true)?;
    s.set_nonblocking(true)?;
    s.bind(&SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), UDP_PORT).into())?;
    UdpSocket::from_std(s.into())
}

/// Первый свободный TCP-порт из диапазона KDE Connect.
pub async fn bind_tcp() -> std::io::Result<(TcpListener, u16)> {
    let mut last = None;
    for port in TCP_PORTS {
        match TcpListener::bind(SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port)).await {
            Ok(l) => return Ok((l, port)),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| std::io::ErrorKind::AddrInUse.into()))
}

/// Рассылка своего identity по сети.
pub async fn broadcast(udp: &UdpSocket, identity_line: &str) {
    let data = format!("{identity_line}\n");
    let _ = udp.send_to(data.as_bytes(), (Ipv4Addr::BROADCAST, UDP_PORT)).await;
}
