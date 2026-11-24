//! Cliente ONVIF (design D1–D3): localiza o serviço do dispositivo, mede o
//! relógio da câmera e obtém as URLs RTSP dos streams principal e secundário.

use std::net::Ipv4Addr;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use rtsp_viewer_core::domain::{RtspUrl, redact_credentials};

use crate::auth::{
    ClockOffset, Credentials, DigestChallenge, UnixSeconds, random_bytes, ws_security_header,
};
use crate::soap::{self, MAX_RESPONSE_BYTES, Reply};
use crate::xml::Element;

/// Portas ONVIF usuais, na ordem de preferência (2020 é a da TP-Link Tapo).
pub const DEFAULT_PORTS: [u16; 5] = [80, 2020, 8000, 8080, 8899];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OnvifError {
    #[error("A câmera não respondeu: {0}")]
    Unreachable(String),
    #[error("O endereço não respondeu como câmera ONVIF.")]
    NotOnvif,
    #[error("Usuário ou senha inválidos.")]
    AuthenticationFailed,
    #[error("A câmera não informou nenhum perfil de vídeo.")]
    NoVideoProfiles,
    #[error("Resposta ONVIF inesperada: {0}")]
    Protocol(String),
}

#[derive(Debug, Clone)]
pub struct OnvifConfig {
    /// Timeout de conexão de cada chamada.
    pub connect_timeout: Duration,
    /// Prazo total de uma consulta inteira (`locate` ou `fetch_streams`, com
    /// todas as chamadas dela), não de cada chamada (design D1).
    pub total_timeout: Duration,
    /// Portas tentadas quando o endereço não traz porta.
    pub probe_ports: Vec<u16>,
    pub probe_connect_timeout: Duration,
}

impl Default for OnvifConfig {
    fn default() -> Self {
        Self {
            connect_timeout: Duration::from_secs(2),
            total_timeout: Duration::from_secs(10),
            probe_ports: DEFAULT_PORTS.to_vec(),
            probe_connect_timeout: Duration::from_secs(1),
        }
    }
}

/// Serviço de dispositivo ONVIF localizado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceService {
    /// `http://host:porta/caminho`.
    pub url: String,
    /// Host da câmera, usado para corrigir URLs RTSP com host inválido.
    pub host: String,
}

impl DeviceService {
    pub fn from_url(url: &str) -> Option<Self> {
        let rest = url.strip_prefix("http://")?;
        let authority = rest.split('/').next()?;
        let host = if let Some(v6) = authority.strip_prefix('[') {
            v6.split(']').next()?.to_string()
        } else {
            authority.split(':').next()?.to_string()
        };
        (!host.is_empty()).then(|| Self {
            url: url.to_string(),
            host,
        })
    }

    pub fn for_host(host: &str, port: u16) -> Self {
        let h = if host.contains(':') {
            format!("[{host}]")
        } else {
            host.to_string()
        };
        Self {
            url: format!("http://{h}:{port}/onvif/device_service"),
            host: host.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamProfile {
    pub url: RtspUrl,
    pub width: u32,
    pub height: u32,
    pub encoding: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamSet {
    pub manufacturer: Option<String>,
    pub model: Option<String>,
    pub main: StreamProfile,
    pub sub: Option<StreamProfile>,
}

#[derive(Clone)]
pub struct OnvifClient {
    agent: ureq::Agent,
    probe_agent: ureq::Agent,
    config: OnvifConfig,
}

impl Default for OnvifClient {
    fn default() -> Self {
        Self::new(OnvifConfig::default())
    }
}

struct HttpReply {
    status: u16,
    www_authenticate: Option<String>,
    body: String,
}

enum AuthMode {
    WsSecurity,
    HttpDigest(DigestChallenge),
}

/// Estado de autenticação de uma consulta (uma câmera).
struct Session<'a> {
    creds: Option<&'a Credentials>,
    clock: ClockOffset,
    mode: AuthMode,
    /// Fim do prazo total da consulta.
    deadline: Instant,
}

impl OnvifClient {
    pub fn new(config: OnvifConfig) -> Self {
        let agent = build_agent(config.connect_timeout);
        let probe_agent = build_agent(config.probe_connect_timeout);
        Self {
            agent,
            probe_agent,
            config,
        }
    }

    /// Localiza o serviço ONVIF do host. Com porta, só ela é tentada; sem
    /// porta, as portas usuais são tentadas em paralelo e vale a primeira que
    /// responder a `GetSystemDateAndTime` (design D3).
    pub fn locate(&self, host: &str, port: Option<u16>) -> Result<DeviceService, OnvifError> {
        let ports = match port {
            Some(p) => vec![p],
            None => self.config.probe_ports.clone(),
        };
        let deadline = Instant::now() + self.config.total_timeout;
        let (tx, rx) = mpsc::channel();
        for p in ports.iter().copied() {
            let tx = tx.clone();
            let agent = self.probe_agent.clone();
            let device = DeviceService::for_host(host, p);
            thread::spawn(move || {
                let ok = system_time(&agent, &device.url, deadline).is_ok();
                let _ = tx.send((device, ok));
            });
        }
        drop(tx);
        let mut answered = 0;
        while let Ok((device, ok)) =
            rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
        {
            answered += 1;
            if ok {
                return Ok(device);
            }
            if answered == ports.len() {
                break;
            }
        }
        Err(OnvifError::NotOnvif)
    }

    /// Obtém fabricante, modelo e os streams principal e secundário.
    pub fn fetch_streams(
        &self,
        device: &DeviceService,
        creds: Option<&Credentials>,
    ) -> Result<StreamSet, OnvifError> {
        // Prazo total da consulta inteira; cada chamada recebe o restante.
        let deadline = Instant::now() + self.config.total_timeout;
        // Relógio da câmera, sem autenticação; falha aqui não impede a
        // consulta (a câmera pode estar com a hora certa).
        let clock = system_time(&self.agent, &device.url, deadline)
            .map(|camera| ClockOffset::between(camera, now()))
            .unwrap_or_default();
        let mut session = Session {
            creds,
            clock,
            mode: AuthMode::WsSecurity,
            deadline,
        };

        let (manufacturer, model) =
            match self.call(&device.url, &soap::get_device_information(), &mut session) {
                Ok(body) => (
                    body.find("Manufacturer")
                        .map(|e| e.text_trimmed().to_string())
                        .filter(|s| !s.is_empty()),
                    body.find("Model")
                        .map(|e| e.text_trimmed().to_string())
                        .filter(|s| !s.is_empty()),
                ),
                Err(OnvifError::AuthenticationFailed) => {
                    return Err(OnvifError::AuthenticationFailed);
                }
                Err(_) => (None, None),
            };

        let media_url = match self.call(&device.url, &soap::get_capabilities_media(), &mut session)
        {
            Ok(body) => body
                .find("Media")
                .and_then(|m| m.child("XAddr"))
                .map(|x| x.text_trimmed().to_string())
                .and_then(|u| media_service_url(&u, &device.host))
                .unwrap_or_else(|| device.url.clone()),
            Err(OnvifError::AuthenticationFailed) => return Err(OnvifError::AuthenticationFailed),
            Err(_) => device.url.clone(),
        };

        let profiles_body = self.call(&media_url, &soap::get_profiles(), &mut session)?;
        let profiles = parse_profiles(&profiles_body);
        let (main, sub) = choose(&profiles).ok_or(OnvifError::NoVideoProfiles)?;

        let main = self.stream_profile(&media_url, main, &device.host, &mut session)?;
        let sub = match sub {
            Some(p) => self
                .stream_profile(&media_url, p, &device.host, &mut session)
                .ok(),
            None => None,
        };
        Ok(StreamSet {
            manufacturer,
            model,
            main,
            sub,
        })
    }

    fn stream_profile(
        &self,
        media_url: &str,
        p: &ProfileInfo,
        camera_host: &str,
        session: &mut Session<'_>,
    ) -> Result<StreamProfile, OnvifError> {
        let body = self.call(media_url, &soap::get_stream_uri(&p.token), session)?;
        let uri = body
            .find("MediaUri")
            .and_then(|m| m.child("Uri"))
            .map(|u| u.text_trimmed().to_string())
            .ok_or_else(|| OnvifError::Protocol("GetStreamUri sem Uri".into()))?;
        Ok(StreamProfile {
            url: sanitize_stream_url(&uri, camera_host)?,
            width: p.width,
            height: p.height,
            encoding: p.encoding.clone(),
        })
    }

    /// Chamada autenticada: WS-Security; se a câmera responder 401 com desafio
    /// Digest, repete uma vez com HTTP Digest e mantém esse modo na sessão.
    fn call(
        &self,
        url: &str,
        body: &str,
        session: &mut Session<'_>,
    ) -> Result<Element, OnvifError> {
        let mut digest_retry_done = false;
        loop {
            let (header, authorization) = match (&session.mode, session.creds) {
                (_, None) => (None, None),
                (AuthMode::WsSecurity, Some(c)) => {
                    let created = session.clock.camera_time(now());
                    (
                        Some(ws_security_header(c, &random_bytes::<16>(), created)),
                        None,
                    )
                }
                (AuthMode::HttpDigest(ch), Some(c)) => {
                    let cnonce: String = random_bytes::<8>()
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect();
                    (
                        None,
                        Some(ch.authorization(c, "POST", path_of(url), &cnonce)),
                    )
                }
            };
            let envelope = soap::envelope(header.as_deref(), body);
            let reply = post(
                &self.agent,
                url,
                &envelope,
                authorization.as_deref(),
                session.deadline,
            )?;

            if reply.status == 401 {
                let challenge = reply
                    .www_authenticate
                    .as_deref()
                    .and_then(|h| DigestChallenge::parse(h).ok());
                match (challenge, session.creds, digest_retry_done) {
                    (Some(ch), Some(_), false) => {
                        session.mode = AuthMode::HttpDigest(ch);
                        digest_retry_done = true;
                        continue;
                    }
                    _ => return Err(OnvifError::AuthenticationFailed),
                }
            }
            return match soap::read_reply(&reply.body) {
                Some(Reply::Body(b)) if (200..300).contains(&reply.status) => Ok(b),
                Some(Reply::Fault(f)) if f.is_not_authorized() => {
                    Err(OnvifError::AuthenticationFailed)
                }
                Some(Reply::Fault(f)) => Err(OnvifError::Protocol(redact_credentials(&f.reason))),
                Some(Reply::Body(_)) => Err(OnvifError::Protocol(format!("HTTP {}", reply.status))),
                None => Err(OnvifError::NotOnvif),
            };
        }
    }
}

/// O timeout total não fica no agente: cada chamada recebe o que resta do
/// prazo da consulta (ver `post`).
fn build_agent(connect: Duration) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(connect))
        .http_status_as_error(false)
        .max_redirects(0)
        .build()
        .into()
}

fn post(
    agent: &ureq::Agent,
    url: &str,
    envelope: &str,
    authorization: Option<&str>,
    deadline: Instant,
) -> Result<HttpReply, OnvifError> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(OnvifError::Unreachable("tempo esgotado".into()));
    }
    let mut req = agent
        .post(url)
        .config()
        .timeout_global(Some(remaining))
        .build()
        .header("Content-Type", "application/soap+xml; charset=utf-8");
    if let Some(a) = authorization {
        req = req.header("Authorization", a);
    }
    let mut resp = req.send(envelope).map_err(|e| match e {
        ureq::Error::BodyExceedsLimit(_) => OnvifError::Protocol("resposta grande demais".into()),
        other => OnvifError::Unreachable(redact_credentials(&other.to_string())),
    })?;
    let status = resp.status().as_u16();
    let www_authenticate = resp
        .headers()
        .get_all("www-authenticate")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find(|v| v.trim_start().to_ascii_lowercase().starts_with("digest"))
        .map(str::to_string);
    let body = resp
        .body_mut()
        .with_config()
        .limit(MAX_RESPONSE_BYTES)
        .read_to_string()
        .map_err(|e| match e {
            ureq::Error::BodyExceedsLimit(_) => {
                OnvifError::Protocol("resposta grande demais".into())
            }
            other => OnvifError::Unreachable(redact_credentials(&other.to_string())),
        })?;
    Ok(HttpReply {
        status,
        www_authenticate,
        body,
    })
}

/// `GetSystemDateAndTime` sem autenticação → hora UTC da câmera.
fn system_time(
    agent: &ureq::Agent,
    url: &str,
    deadline: Instant,
) -> Result<UnixSeconds, OnvifError> {
    let reply = post(
        agent,
        url,
        &soap::envelope(None, &soap::get_system_date_and_time()),
        None,
        deadline,
    )?;
    let Some(Reply::Body(body)) = soap::read_reply(&reply.body) else {
        return Err(OnvifError::NotOnvif);
    };
    let utc = body
        .find("UTCDateTime")
        .ok_or_else(|| OnvifError::Protocol("sem UTCDateTime".into()))?;
    let n = |path: &[&str]| -> Option<i64> { utc.path(path)?.text_trimmed().parse().ok() };
    let get = |path: &[&str]| n(path).ok_or_else(|| OnvifError::Protocol("data inválida".into()));
    let (y, mo, d) = (
        get(&["Date", "Year"])?,
        get(&["Date", "Month"])?,
        get(&["Date", "Day"])?,
    );
    let (h, mi, s) = (
        get(&["Time", "Hour"])?,
        get(&["Time", "Minute"])?,
        get(&["Time", "Second"])?,
    );
    let as_u32 =
        |v: i64| u32::try_from(v).map_err(|_| OnvifError::Protocol("data inválida".into()));
    crate::auth::utc_to_unix(
        y,
        as_u32(mo)?,
        as_u32(d)?,
        as_u32(h)?,
        as_u32(mi)?,
        as_u32(s)?,
    )
    .ok_or_else(|| OnvifError::Protocol("data inválida".into()))
}

fn now() -> UnixSeconds {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn path_of(url: &str) -> &str {
    url.strip_prefix("http://")
        .and_then(|r| r.find('/').map(|i| &r[i..]))
        .unwrap_or("/")
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProfileInfo {
    token: String,
    width: u32,
    height: u32,
    encoding: Option<String>,
}

fn parse_profiles(body: &Element) -> Vec<ProfileInfo> {
    let mut found = Vec::new();
    body.find_all("Profiles", &mut found);
    found
        .into_iter()
        .filter_map(|p| {
            let token = p.attr("token")?.to_string();
            let vec = p.child("VideoEncoderConfiguration")?;
            let res = vec.child("Resolution")?;
            let width = res.child("Width")?.text_trimmed().parse().ok()?;
            let height = res.child("Height")?.text_trimmed().parse().ok()?;
            let encoding = vec.child("Encoding").map(|e| e.text_trimmed().to_string());
            Some(ProfileInfo {
                token,
                width,
                height,
                encoding,
            })
        })
        .collect()
}

/// Principal = maior resolução; secundário = menor, se for diferente
/// (design D3). Empate: vale a ordem do dispositivo.
fn choose(profiles: &[ProfileInfo]) -> Option<(&ProfileInfo, Option<&ProfileInfo>)> {
    let area = |p: &ProfileInfo| u64::from(p.width) * u64::from(p.height);
    let mut main = profiles.first()?;
    let mut sub = main;
    for p in profiles {
        if area(p) > area(main) {
            main = p;
        }
        if area(p) < area(sub) {
            sub = p;
        }
    }
    Some((main, (area(sub) < area(main)).then_some(sub)))
}

/// Host que não identifica a câmera na rede: não especificado, loopback
/// (127.0.0.0/8, `::1`) ou `localhost`.
fn is_bad_host(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") || matches!(host, "::" | "::1") {
        return true;
    }
    host.parse::<Ipv4Addr>()
        .is_ok_and(|ip| ip.is_unspecified() || ip.is_loopback())
}

/// Valida a URL RTSP devolvida, descarta credenciais embutidas e troca host
/// inválido pelo host da câmera (design D3).
fn sanitize_stream_url(uri: &str, camera_host: &str) -> Result<RtspUrl, OnvifError> {
    let parsed = RtspUrl::parse(uri)
        .map_err(|e| OnvifError::Protocol(format!("URL RTSP inválida ({e})")))?;
    if !is_bad_host(parsed.url.host()) {
        return Ok(parsed.url);
    }
    let text = parsed.url.to_string();
    let old = parsed.url.host();
    let old_authority = if old.contains(':') {
        format!("//[{old}]")
    } else {
        format!("//{old}")
    };
    let new_authority = if camera_host.contains(':') {
        format!("//[{camera_host}]")
    } else {
        format!("//{camera_host}")
    };
    RtspUrl::parse(&text.replacen(&old_authority, &new_authority, 1))
        .map(|p| p.url)
        .map_err(|e| OnvifError::Protocol(format!("URL RTSP inválida ({e})")))
}

/// Endereço do serviço de mídia a partir do `XAddr` anunciado pela câmera.
///
/// O `XAddr` é entrada de rede não confiável (ADR 0005): as chamadas seguintes
/// levam o digest das credenciais, então nunca vão a outro host. A URL usada é
/// sempre `http://<host da câmera>:<porta do XAddr><caminho do XAddr>`, seja o
/// host anunciado igual, inválido (`0.0.0.0`, loopback) ou outro IP (firmware
/// atrás de NAT que anuncia o IP interno). Assim a câmera só escolhe porta e
/// caminho no próprio host, a que as credenciais já são enviadas. `XAddr` que
/// não é `http` ou que não se consegue ler → `None` (vale o serviço de
/// dispositivo).
fn media_service_url(xaddr: &str, camera_host: &str) -> Option<String> {
    let rest = xaddr.strip_prefix("http://")?;
    let (authority, path) = match rest.find(['/', '?', '#']) {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, "/"),
    };
    // Descarta credenciais embutidas (`user:pass@`).
    let hostport = authority.rsplit('@').next()?;
    let port_text = if let Some(v6) = hostport.strip_prefix('[') {
        v6.split_once(']')?.1.strip_prefix(':')
    } else {
        hostport.split_once(':').map(|(_, p)| p)
    };
    let port: u16 = match port_text {
        Some(p) => p.parse().ok().filter(|p| *p != 0)?,
        None => 80,
    };
    if !path.starts_with('/') || path.chars().any(|c| c.is_control() || c.is_whitespace()) {
        return None;
    }
    let host = if camera_host.contains(':') {
        format!("[{camera_host}]")
    } else {
        camera_host.to_string()
    };
    Some(format!("http://{host}:{port}{path}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml;

    fn p(token: &str, w: u32, h: u32) -> ProfileInfo {
        ProfileInfo {
            token: token.into(),
            width: w,
            height: h,
            encoding: None,
        }
    }

    #[test]
    fn escolhe_principal_e_secundario() {
        let ps = [
            p("sub", 640, 360),
            p("main", 2560, 1440),
            p("mid", 1280, 720),
        ];
        let (m, s) = choose(&ps).unwrap();
        assert_eq!(
            (m.token.as_str(), s.unwrap().token.as_str()),
            ("main", "sub")
        );
    }

    #[test]
    fn perfil_unico_ou_iguais_sem_secundario() {
        assert!(choose(&[p("a", 1920, 1080)]).unwrap().1.is_none());
        let iguais = [p("a", 1920, 1080), p("b", 1920, 1080)];
        let (m, s) = choose(&iguais).unwrap();
        assert_eq!(m.token, "a");
        assert!(s.is_none());
        assert!(choose(&[]).is_none());
    }

    #[test]
    fn le_perfis_e_ignora_os_sem_video() {
        let body = xml::parse(
            r#"<Body><GetProfilesResponse>
              <Profiles token="p1" fixed="true"><Name>main</Name><VideoEncoderConfiguration>
                <Encoding>H264</Encoding><Resolution><Width>1920</Width><Height>1080</Height></Resolution>
              </VideoEncoderConfiguration></Profiles>
              <Profiles token="audio"><Name>a</Name></Profiles>
              <Profiles token="p2"><VideoEncoderConfiguration><Encoding>H265</Encoding>
                <Resolution><Width>640</Width><Height>360</Height></Resolution></VideoEncoderConfiguration></Profiles>
            </GetProfilesResponse></Body>"#,
        )
        .unwrap();
        let ps = parse_profiles(&body);
        assert_eq!(ps.len(), 2);
        assert_eq!(
            ps[0],
            ProfileInfo {
                token: "p1".into(),
                width: 1920,
                height: 1080,
                encoding: Some("H264".into())
            }
        );
    }

    #[test]
    fn url_rtsp_com_host_invalido_ou_credenciais() {
        let u = sanitize_stream_url("rtsp://0.0.0.0:554/stream1", "192.168.1.120").unwrap();
        assert_eq!(u.to_string(), "rtsp://192.168.1.120:554/stream1");
        let u =
            sanitize_stream_url("rtsp://admin:segredo@192.168.1.120/s", "192.168.1.120").unwrap();
        assert_eq!(u.to_string(), "rtsp://192.168.1.120/s");
        let u = sanitize_stream_url("rtsp://localhost/s", "fe80::1").unwrap();
        assert_eq!(u.to_string(), "rtsp://[fe80::1]/s");
        assert!(matches!(
            sanitize_stream_url("http://x/s", "h"),
            Err(OnvifError::Protocol(_))
        ));
    }

    #[test]
    fn servicos_e_caminhos() {
        let d = DeviceService::for_host("192.168.1.120", 2020);
        assert_eq!(d.url, "http://192.168.1.120:2020/onvif/device_service");
        assert_eq!(DeviceService::from_url(&d.url), Some(d.clone()));
        assert_eq!(
            DeviceService::for_host("fe80::1", 80).url,
            "http://[fe80::1]:80/onvif/device_service"
        );
        assert_eq!(
            DeviceService::from_url("http://[fe80::1]:80/x")
                .unwrap()
                .host,
            "fe80::1"
        );
        assert_eq!(
            path_of("http://h:80/onvif/media_service"),
            "/onvif/media_service"
        );
    }

    #[test]
    fn servico_de_midia_fica_sempre_no_host_da_camera() {
        let m = |x: &str, h: &str| media_service_url(x, h);
        assert_eq!(
            m("http://0.0.0.0:8000/onvif/media", "10.0.0.5").as_deref(),
            Some("http://10.0.0.5:8000/onvif/media")
        );
        // Mesmo host: nada muda.
        assert_eq!(
            m("http://10.0.0.5:2020/onvif/service", "10.0.0.5").as_deref(),
            Some("http://10.0.0.5:2020/onvif/service")
        );
        // Outro host (NAT ou câmera maliciosa): host da câmera, porta e
        // caminho do XAddr.
        assert_eq!(
            m("http://203.0.113.7:8080/onvif/Media", "192.168.1.120").as_deref(),
            Some("http://192.168.1.120:8080/onvif/Media")
        );
        assert_eq!(
            m("http://evil.example/x?y=1", "192.168.1.120").as_deref(),
            Some("http://192.168.1.120:80/x?y=1")
        );
        assert_eq!(
            m("http://u:p@[2001:db8::9]:81/media", "fe80::1").as_deref(),
            Some("http://[fe80::1]:81/media")
        );
        assert_eq!(
            m("http://attacker.lan", "10.0.0.5").as_deref(),
            Some("http://10.0.0.5:80/")
        );
        // Não http, porta inválida ou caminho estranho: vale o dispositivo.
        assert_eq!(m("https://10.0.0.5/onvif/media", "10.0.0.5"), None);
        assert_eq!(m("http://10.0.0.5:99999/m", "10.0.0.5"), None);
        assert_eq!(m("http://10.0.0.5:0/m", "10.0.0.5"), None);
        assert_eq!(m("http://10.0.0.5/a b", "10.0.0.5"), None);
    }

    #[test]
    fn hosts_invalidos() {
        for h in [
            "0.0.0.0",
            "127.0.0.1",
            "127.1.2.3",
            "127.255.255.254",
            "localhost",
            "LocalHost",
            "::",
            "::1",
        ] {
            assert!(is_bad_host(h), "{h}");
        }
        for h in ["192.168.1.120", "128.0.0.1", "fe80::1", "camera.lan"] {
            assert!(!is_bad_host(h), "{h}");
        }
    }
}
