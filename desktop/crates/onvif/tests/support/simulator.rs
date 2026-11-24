//! Simulador de câmera ONVIF para testes e para o teste de ponta a ponta
//! (design D8). Valida de verdade o PasswordDigest (com o relógio simulado) e
//! o HTTP Digest, e guarda as requisições recebidas para os testes conferirem
//! que a senha nunca trafega em texto puro.
//!
//! Usado por `tests/` e por `examples/fake_onvif.rs`; não faz parte do app.
#![allow(dead_code)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use rtsp_viewer_onvif::auth::{format_utc, md5_hex, password_digest, utc_to_unix};
use rtsp_viewer_onvif::xml;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimAuth {
    /// Aceita só WS-UsernameToken PasswordDigest.
    WsSecurity,
    /// Responde 401 com desafio Digest e aceita só HTTP Digest.
    HttpDigest,
    /// Não exige autenticação.
    Open,
}

#[derive(Debug, Clone)]
pub struct SimProfile {
    pub token: String,
    pub width: u32,
    pub height: u32,
    pub uri: String,
}

#[derive(Debug, Clone)]
pub struct SimConfig {
    pub username: String,
    pub password: String,
    pub auth: SimAuth,
    /// Relógio da câmera adiantado (ou atrasado) em segundos.
    pub clock_offset: i64,
    pub manufacturer: String,
    pub model: String,
    pub profiles: Vec<SimProfile>,
    /// Anuncia o serviço de mídia com host 0.0.0.0 (firmware com NAT errado).
    pub media_host_zero: bool,
    /// Host anunciado no XAddr de mídia (outro IP: NAT ou câmera maliciosa).
    /// Tem precedência sobre `media_host_zero`.
    pub media_host: Option<String>,
    /// Atraso antes de cada resposta, para testar o prazo total da consulta.
    pub response_delay: std::time::Duration,
    /// Responde HTML em vez de SOAP (não é câmera ONVIF).
    pub not_onvif: bool,
    /// Tamanho de uma resposta artificialmente grande, para testar limites.
    pub oversized_profiles: bool,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            username: "admin".into(),
            password: "s3nh@:/?#-ONVIF".into(),
            auth: SimAuth::WsSecurity,
            clock_offset: 0,
            manufacturer: "Simulador".into(),
            model: "Camera ONVIF de teste".into(),
            profiles: vec![
                SimProfile {
                    token: "sub".into(),
                    width: 640,
                    height: 360,
                    uri: "rtsp://{host}:8554/sub".into(),
                },
                SimProfile {
                    token: "main".into(),
                    width: 1920,
                    height: 1080,
                    uri: "rtsp://{host}:8554/main".into(),
                },
            ],
            media_host_zero: false,
            media_host: None,
            response_delay: std::time::Duration::ZERO,
            not_onvif: false,
            oversized_profiles: false,
        }
    }
}

pub struct Simulator {
    pub port: u16,
    pub requests: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
}

impl Simulator {
    /// Inicia em `127.0.0.1` numa porta livre.
    pub fn start(config: SimConfig) -> Self {
        Self::start_on("127.0.0.1:0", config)
    }

    pub fn start_on(addr: &str, config: SimConfig) -> Self {
        let listener = TcpListener::bind(addr).expect("bind do simulador");
        let port = listener.local_addr().unwrap().port();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (reqs, st, cfg) = (requests.clone(), stop.clone(), Arc::new(config));
        thread::spawn(move || {
            for conn in listener.incoming() {
                if st.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(conn) = conn else { continue };
                let (reqs, cfg) = (reqs.clone(), cfg.clone());
                thread::spawn(move || {
                    let _ = handle(conn, &cfg, &reqs, port);
                });
            }
        });
        Self {
            port,
            requests,
            stop,
        }
    }

    pub fn device_url(&self) -> String {
        format!("http://127.0.0.1:{}/onvif/device_service", self.port)
    }

    pub fn all_requests(&self) -> String {
        self.requests.lock().unwrap().join("\n")
    }
}

impl Drop for Simulator {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

struct Request {
    path: String,
    authorization: Option<String>,
    body: String,
}

fn read_request(stream: &mut TcpStream) -> std::io::Result<(Request, String)> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut head = String::new();
    let mut content_length = 0usize;
    let mut authorization = None;
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    head.push_str(&request_line);
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 || line == "\r\n" {
            break;
        }
        head.push_str(&line);
        if let Some((k, v)) = line.split_once(':') {
            let v = v.trim().to_string();
            match k.trim().to_ascii_lowercase().as_str() {
                "content-length" => content_length = v.parse().unwrap_or(0),
                "authorization" => authorization = Some(v),
                _ => {}
            }
        }
    }
    let mut body = vec![0; content_length.min(1 << 20)];
    reader.read_exact(&mut body)?;
    let body = String::from_utf8_lossy(&body).to_string();
    let path = request_line
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .to_string();
    let raw = format!("{head}\r\n{body}");
    Ok((
        Request {
            path,
            authorization,
            body,
        },
        raw,
    ))
}

fn respond(
    stream: &mut TcpStream,
    status: &str,
    extra: &str,
    content_type: &str,
    body: &str,
) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n{extra}\r\n{body}",
        body.len()
    )?;
    stream.flush()
}

fn envelope(body: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><env:Envelope xmlns:env="http://www.w3.org/2003/05/soap-envelope" xmlns:tds="http://www.onvif.org/ver10/device/wsdl" xmlns:trt="http://www.onvif.org/ver10/media/wsdl" xmlns:tt="http://www.onvif.org/ver10/schema" xmlns:ter="http://www.onvif.org/ver10/error"><env:Body>{body}</env:Body></env:Envelope>"#
    )
}

fn not_authorized_fault() -> String {
    envelope(
        "<env:Fault><env:Code><env:Value>env:Sender</env:Value><env:Subcode><env:Value>ter:NotAuthorized</env:Value></env:Subcode></env:Code><env:Reason><env:Text xml:lang=\"en\">Sender not Authorized</env:Text></env:Reason></env:Fault>",
    )
}

fn handle(
    mut stream: TcpStream,
    cfg: &SimConfig,
    log: &Mutex<Vec<String>>,
    port: u16,
) -> std::io::Result<()> {
    let (req, raw) = read_request(&mut stream)?;
    log.lock().unwrap().push(raw);
    if !cfg.response_delay.is_zero() {
        thread::sleep(cfg.response_delay);
    }
    const SOAP: &str = "application/soap+xml; charset=utf-8";

    if cfg.not_onvif {
        return respond(
            &mut stream,
            "200 OK",
            "",
            "text/html",
            "<html><body>Login</body></html>",
        );
    }
    let Ok(doc) = xml::parse(&req.body) else {
        return respond(&mut stream, "400 Bad Request", "", "text/plain", "xml");
    };
    let op = doc
        .child("Body")
        .and_then(|b| b.children.first())
        .map(|e| e.name.clone())
        .unwrap_or_default();
    let camera_now = now() + cfg.clock_offset;

    if op != "GetSystemDateAndTime" {
        match cfg.auth {
            SimAuth::Open => {}
            SimAuth::WsSecurity => {
                if !ws_security_ok(&doc, cfg, camera_now) {
                    return respond(
                        &mut stream,
                        "400 Bad Request",
                        "",
                        SOAP,
                        &not_authorized_fault(),
                    );
                }
            }
            SimAuth::HttpDigest => {
                let challenge = r#"WWW-Authenticate: Digest realm="simulador", qop="auth", nonce="n0nce-sim", opaque="op4que"\r\n"#
                    .replace("\\r\\n", "\r\n");
                let ok = req
                    .authorization
                    .as_deref()
                    .is_some_and(|a| http_digest_ok(a, cfg, &req.path));
                if !ok {
                    return respond(
                        &mut stream,
                        "401 Unauthorized",
                        &challenge,
                        "text/plain",
                        "unauthorized",
                    );
                }
            }
        }
    }

    let media_host = if let Some(h) = &cfg.media_host {
        h.clone()
    } else if cfg.media_host_zero {
        "0.0.0.0".to_string()
    } else {
        "127.0.0.1".to_string()
    };
    let body = match op.as_str() {
        "GetSystemDateAndTime" => {
            let s = format_utc(camera_now);
            let (date, time) = (&s[..10], &s[11..19]);
            let d: Vec<&str> = date.split('-').collect();
            let t: Vec<&str> = time.split(':').collect();
            format!(
                "<tds:GetSystemDateAndTimeResponse><tds:SystemDateAndTime><tt:DateTimeType>Manual</tt:DateTimeType><tt:UTCDateTime><tt:Time><tt:Hour>{}</tt:Hour><tt:Minute>{}</tt:Minute><tt:Second>{}</tt:Second></tt:Time><tt:Date><tt:Year>{}</tt:Year><tt:Month>{}</tt:Month><tt:Day>{}</tt:Day></tt:Date></tt:UTCDateTime></tds:SystemDateAndTime></tds:GetSystemDateAndTimeResponse>",
                t[0].trim_start_matches('0').parse::<u32>().unwrap_or(0),
                t[1].trim_start_matches('0').parse::<u32>().unwrap_or(0),
                t[2].trim_start_matches('0').parse::<u32>().unwrap_or(0),
                d[0],
                d[1].trim_start_matches('0'),
                d[2].trim_start_matches('0'),
            )
        }
        "GetDeviceInformation" => format!(
            "<tds:GetDeviceInformationResponse><tds:Manufacturer>{}</tds:Manufacturer><tds:Model>{}</tds:Model><tds:FirmwareVersion>1.0</tds:FirmwareVersion><tds:SerialNumber>SIM-1</tds:SerialNumber><tds:HardwareId>1</tds:HardwareId></tds:GetDeviceInformationResponse>",
            xml::escape(&cfg.manufacturer),
            xml::escape(&cfg.model)
        ),
        "GetCapabilities" => format!(
            "<tds:GetCapabilitiesResponse><tds:Capabilities><tt:Media><tt:XAddr>http://{media_host}:{port}/onvif/media_service</tt:XAddr></tt:Media></tds:Capabilities></tds:GetCapabilitiesResponse>"
        ),
        "GetProfiles" => {
            let mut out = String::from("<trt:GetProfilesResponse>");
            for p in &cfg.profiles {
                out.push_str(&format!(
                    "<trt:Profiles token=\"{}\" fixed=\"true\"><tt:Name>{}</tt:Name><tt:VideoEncoderConfiguration token=\"v{}\"><tt:Encoding>H264</tt:Encoding><tt:Resolution><tt:Width>{}</tt:Width><tt:Height>{}</tt:Height></tt:Resolution></tt:VideoEncoderConfiguration></trt:Profiles>",
                    p.token, p.token, p.token, p.width, p.height
                ));
            }
            if cfg.oversized_profiles {
                out.push_str(&format!("<!-- {} -->", "x".repeat(300 * 1024)));
            }
            out.push_str("</trt:GetProfilesResponse>");
            out
        }
        "GetStreamUri" => {
            let token = doc
                .find("ProfileToken")
                .map(|t| t.text_trimmed().to_string())
                .unwrap_or_default();
            let Some(p) = cfg.profiles.iter().find(|p| p.token == token) else {
                return respond(
                    &mut stream,
                    "400 Bad Request",
                    "",
                    SOAP,
                    &envelope(
                        "<env:Fault><env:Code><env:Value>env:Sender</env:Value></env:Code><env:Reason><env:Text>NoProfile</env:Text></env:Reason></env:Fault>",
                    ),
                );
            };
            format!(
                "<trt:GetStreamUriResponse><trt:MediaUri><tt:Uri>{}</tt:Uri><tt:InvalidAfterConnect>false</tt:InvalidAfterConnect></trt:MediaUri></trt:GetStreamUriResponse>",
                xml::escape(&p.uri.replace("{host}", "127.0.0.1"))
            )
        }
        _ => {
            return respond(
                &mut stream,
                "400 Bad Request",
                "",
                SOAP,
                &envelope(
                    "<env:Fault><env:Code><env:Value>env:Receiver</env:Value></env:Code><env:Reason><env:Text>Action not supported</env:Text></env:Reason></env:Fault>",
                ),
            );
        }
    };
    respond(&mut stream, "200 OK", "", SOAP, &envelope(&body))
}

fn ws_security_ok(doc: &xml::Element, cfg: &SimConfig, camera_now: i64) -> bool {
    let Some(token) = doc.find("UsernameToken") else {
        return false;
    };
    let text = |n: &str| token.child(n).map(|e| e.text_trimmed().to_string());
    let (Some(user), Some(digest), Some(nonce), Some(created)) = (
        text("Username"),
        text("Password"),
        text("Nonce"),
        text("Created"),
    ) else {
        return false;
    };
    let pass_el = token.child("Password").unwrap();
    if !pass_el
        .attr("Type")
        .unwrap_or_default()
        .ends_with("#PasswordDigest")
    {
        return false;
    }
    use base64::Engine;
    let Ok(nonce) = base64::engine::general_purpose::STANDARD.decode(nonce) else {
        return false;
    };
    let Some(created_at) = parse_created(&created) else {
        return false;
    };
    // Câmeras reais recusam horário fora de uma janela curta.
    if (created_at - camera_now).abs() > 300 {
        return false;
    }
    user == cfg.username && digest == password_digest(&nonce, &created, &cfg.password)
}

fn parse_created(s: &str) -> Option<i64> {
    let s = s.strip_suffix('Z')?;
    let (date, time) = s.split_once('T')?;
    let d: Vec<&str> = date.split('-').collect();
    let t: Vec<&str> = time.split(':').collect();
    if d.len() != 3 || t.len() != 3 {
        return None;
    }
    utc_to_unix(
        d[0].parse().ok()?,
        d[1].parse().ok()?,
        d[2].parse().ok()?,
        t[0].parse().ok()?,
        t[1].parse().ok()?,
        t[2].split('.').next()?.parse().ok()?,
    )
}

fn http_digest_ok(header: &str, cfg: &SimConfig, path: &str) -> bool {
    let Some(rest) = header.strip_prefix("Digest ") else {
        return false;
    };
    let get = |k: &str| -> Option<String> {
        rest.split(", ").find_map(|kv| {
            let (key, v) = kv.split_once('=')?;
            (key.trim() == k).then(|| v.trim().trim_matches('"').to_string())
        })
    };
    let (Some(user), Some(uri), Some(response), Some(nc), Some(cnonce)) = (
        get("username"),
        get("uri"),
        get("response"),
        get("nc"),
        get("cnonce"),
    ) else {
        return false;
    };
    if user != cfg.username || uri != path || get("opaque").as_deref() != Some("op4que") {
        return false;
    }
    let ha1 = md5_hex(&format!("{}:simulador:{}", cfg.username, cfg.password));
    let ha2 = md5_hex(&format!("POST:{uri}"));
    response == md5_hex(&format!("{ha1}:n0nce-sim:{nc}:{cnonce}:auth:{ha2}"))
}

// ---------------------------------------------------------------- WS-Discovery

/// Resposta que o respondedor envia para cada Probe recebido.
#[derive(Debug, Clone)]
pub enum DiscoveryReply {
    /// ProbeMatch válido. `{ip}` em `xaddr` vira o endereço anunciado.
    Match {
        endpoint: String,
        xaddr: String,
        scopes: String,
    },
    /// ProbeMatch com `RelatesTo` de outra busca.
    WrongRelatesTo,
    /// Datagrama que não é XML.
    Malformed,
}

pub struct DiscoveryResponder {
    pub addr: std::net::SocketAddr,
    stop: Arc<AtomicBool>,
}

impl Drop for DiscoveryResponder {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

fn probe_match(relates: &str, endpoint: &str, xaddr: &str, scopes: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><SOAP-ENV:Envelope xmlns:SOAP-ENV="http://www.w3.org/2003/05/soap-envelope" xmlns:wsa="http://schemas.xmlsoap.org/ws/2004/08/addressing" xmlns:d="http://schemas.xmlsoap.org/ws/2005/04/discovery" xmlns:dn="http://www.onvif.org/ver10/network/wsdl"><SOAP-ENV:Header><wsa:MessageID>uuid:resp-{n}</wsa:MessageID><wsa:RelatesTo>{relates}</wsa:RelatesTo><wsa:Action>http://schemas.xmlsoap.org/ws/2005/04/discovery/ProbeMatches</wsa:Action></SOAP-ENV:Header><SOAP-ENV:Body><d:ProbeMatches><d:ProbeMatch><wsa:EndpointReference><wsa:Address>{endpoint}</wsa:Address></wsa:EndpointReference><d:Types>dn:NetworkVideoTransmitter</d:Types><d:Scopes>{scopes}</d:Scopes><d:XAddrs>{xaddr}</d:XAddrs><d:MetadataVersion>1</d:MetadataVersion></d:ProbeMatch></d:ProbeMatches></SOAP-ENV:Body></SOAP-ENV:Envelope>"#,
        n = now(),
        relates = xml::escape(relates),
        endpoint = xml::escape(endpoint),
        xaddr = xml::escape(xaddr),
        scopes = xml::escape(scopes),
    )
}

/// Respondedor WS-Discovery. Com `multicast_ifaces` vazio, escuta só em
/// `bind` (unicast, para testes); senão, entra no grupo 239.255.255.250 em
/// cada interface dada (para o teste de ponta a ponta).
pub fn start_discovery_responder(
    bind: &str,
    multicast_ifaces: &[std::net::Ipv4Addr],
    advertise_ip: &str,
    replies: Vec<DiscoveryReply>,
) -> DiscoveryResponder {
    use socket2::{Domain, Protocol, Socket, Type};
    let addr: std::net::SocketAddr = bind.parse().expect("endereço do respondedor");
    let s = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP)).unwrap();
    s.set_reuse_address(true).unwrap();
    s.bind(&addr.into())
        .expect("bind do respondedor WS-Discovery");
    for iface in multicast_ifaces {
        let _ = s.join_multicast_v4(&std::net::Ipv4Addr::new(239, 255, 255, 250), iface);
    }
    s.set_read_timeout(Some(std::time::Duration::from_millis(200)))
        .unwrap();
    let sock: std::net::UdpSocket = s.into();
    let local = sock.local_addr().unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let st = stop.clone();
    let advertise = advertise_ip.to_string();
    thread::spawn(move || {
        let mut buf = vec![0u8; 65536];
        while !st.load(Ordering::SeqCst) {
            let Ok((n, from)) = sock.recv_from(&mut buf) else {
                continue;
            };
            let Ok(text) = std::str::from_utf8(&buf[..n]) else {
                continue;
            };
            let Ok(doc) = xml::parse(text) else { continue };
            if doc.find("Probe").is_none() {
                continue;
            }
            let id = doc
                .path(&["Header", "MessageID"])
                .map(|m| m.text_trimmed().to_string())
                .unwrap_or_default();
            for r in &replies {
                let datagram = match r {
                    DiscoveryReply::Match {
                        endpoint,
                        xaddr,
                        scopes,
                    } => probe_match(&id, endpoint, &xaddr.replace("{ip}", &advertise), scopes),
                    DiscoveryReply::WrongRelatesTo => probe_match(
                        "uuid:outra-busca",
                        "urn:uuid:intrusa",
                        "http://10.0.0.66/onvif/device_service",
                        "",
                    ),
                    DiscoveryReply::Malformed => "isto não é XML <".to_string(),
                };
                let _ = sock.send_to(datagram.as_bytes(), from);
            }
        }
    });
    DiscoveryResponder { addr: local, stop }
}
