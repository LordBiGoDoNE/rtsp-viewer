//! WS-Discovery (design D4): encontra câmeras ONVIF na rede local.
//!
//! Um Probe por interface IPv4 ativa, TTL 1, escuta por alguns segundos.
//! Respostas são entrada não confiável: só ProbeMatch da busca atual
//! (`RelatesTo`), de origem privada, link-local ou loopback, até 64 KiB.

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use socket2::{Domain, Protocol, Socket, Type};

use crate::auth::random_bytes;
use crate::xml::{self, Element};

/// Endereço multicast padrão do WS-Discovery.
pub const MULTICAST: SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::new(239, 255, 255, 250), 3702);

/// Tamanho máximo aceito de uma resposta UDP.
pub const MAX_DATAGRAM: usize = 64 * 1024;

#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    /// Quanto tempo esperar respostas.
    pub duration: Duration,
    /// Destino do Probe (multicast em produção; unicast nos testes).
    pub target: SocketAddrV4,
    /// Interfaces de onde enviar; `None` = todas as IPv4 ativas sem loopback.
    pub interfaces: Option<Vec<Ipv4Addr>>,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            duration: Duration::from_secs(3),
            target: MULTICAST,
            interfaces: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiscoveredDevice {
    /// Identidade estável do dispositivo (`EndpointReference/Address`).
    pub endpoint: String,
    /// Host da câmera (o do serviço de dispositivo escolhido).
    pub host: String,
    /// URL do serviço de dispositivo (http), escolhida entre os `XAddrs`.
    pub device_url: String,
    /// Nome sugerido (scope `name`, senão `hardware`).
    pub name: Option<String>,
    pub hardware: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DiscoveryError {
    #[error("Nenhuma interface de rede disponível para a busca.")]
    NoInterface,
    #[error("Falha de rede na busca: {0}")]
    Network(String),
}

/// Faz uma busca e devolve os dispositivos encontrados, sem repetição,
/// ordenados por IP.
pub fn discover(config: &DiscoveryConfig) -> Result<Vec<DiscoveredDevice>, DiscoveryError> {
    let interfaces = match &config.interfaces {
        Some(list) => list.clone(),
        None => active_ipv4_interfaces(),
    };
    if interfaces.is_empty() {
        return Err(DiscoveryError::NoInterface);
    }
    let message_id = new_message_id();
    let probe = probe_message(&message_id);
    let deadline = Instant::now() + config.duration;

    let (tx, rx) = mpsc::channel::<(Vec<u8>, SocketAddr)>();
    let mut sent_any = false;
    let mut last_error = None;
    for iface in interfaces {
        match open_socket(iface) {
            Ok(sock) => {
                if let Err(e) = sock.send_to(probe.as_bytes(), config.target) {
                    last_error = Some(e.to_string());
                    continue;
                }
                sent_any = true;
                let tx = tx.clone();
                thread::spawn(move || receive_until(sock, deadline, tx));
            }
            Err(e) => last_error = Some(e.to_string()),
        }
    }
    drop(tx);
    if !sent_any {
        return Err(DiscoveryError::Network(last_error.unwrap_or_default()));
    }

    let mut found: BTreeMap<String, DiscoveredDevice> = BTreeMap::new();
    while let Ok((data, from)) = rx.recv() {
        if let Some(dev) = parse_probe_match(&data, &message_id, from.ip()) {
            found.entry(dev.endpoint.clone()).or_insert(dev);
        }
    }
    let mut list: Vec<DiscoveredDevice> = found.into_values().collect();
    list.sort_by_key(|d| sort_key(&d.host));
    Ok(list)
}

fn sort_key(host: &str) -> (u32, String) {
    match host.parse::<Ipv4Addr>() {
        Ok(ip) => (u32::from(ip), String::new()),
        Err(_) => (u32::MAX, host.to_string()),
    }
}

fn active_ipv4_interfaces() -> Vec<Ipv4Addr> {
    let mut out: Vec<Ipv4Addr> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| !i.is_loopback())
        .filter_map(|i| match i.ip() {
            IpAddr::V4(v4) if !v4.is_unspecified() => Some(v4),
            _ => None,
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

fn open_socket(iface: Ipv4Addr) -> std::io::Result<UdpSocket> {
    let s = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    s.bind(&SocketAddr::from((iface, 0)).into())?;
    if !iface.is_loopback() {
        s.set_multicast_if_v4(&iface)?;
    }
    s.set_multicast_ttl_v4(1)?;
    s.set_read_timeout(Some(Duration::from_millis(100)))?;
    Ok(s.into())
}

fn receive_until(sock: UdpSocket, deadline: Instant, tx: mpsc::Sender<(Vec<u8>, SocketAddr)>) {
    // Um byte a mais que o limite: se ele for preenchido, a resposta é grande
    // demais e é descartada.
    let mut buf = vec![0u8; MAX_DATAGRAM + 1];
    while Instant::now() < deadline {
        match sock.recv_from(&mut buf) {
            Ok((n, from)) if n <= MAX_DATAGRAM => {
                if tx.send((buf[..n].to_vec(), from)).is_err() {
                    return;
                }
            }
            Ok(_) => {}
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) => {}
            // Windows devolve ConnectionReset quando um destino unicast
            // recusa; não encerra a escuta.
            Err(e) if e.kind() == std::io::ErrorKind::ConnectionReset => {}
            Err(_) => return,
        }
    }
}

fn new_message_id() -> String {
    let b = random_bytes::<16>();
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "uuid:{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    )
}

pub fn probe_message(message_id: &str) -> String {
    format!(
        concat!(
            r#"<?xml version="1.0" encoding="UTF-8"?>"#,
            r#"<s:Envelope xmlns:s="http://www.w3.org/2003/05/soap-envelope" "#,
            r#"xmlns:a="http://schemas.xmlsoap.org/ws/2004/08/addressing" "#,
            r#"xmlns:d="http://schemas.xmlsoap.org/ws/2005/04/discovery" "#,
            r#"xmlns:dn="http://www.onvif.org/ver10/network/wsdl">"#,
            "<s:Header><a:MessageID>{id}</a:MessageID>",
            r#"<a:To s:mustUnderstand="1">urn:schemas-xmlsoap-org:ws:2005:04:discovery</a:To>"#,
            r#"<a:Action s:mustUnderstand="1">http://schemas.xmlsoap.org/ws/2005/04/discovery/Probe</a:Action>"#,
            "</s:Header><s:Body><d:Probe><d:Types>dn:NetworkVideoTransmitter</d:Types></d:Probe></s:Body></s:Envelope>"
        ),
        id = xml::escape(message_id)
    )
}

/// Origem aceita: rede privada, link-local ou loopback (design D4).
fn trusted_origin(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_link_local() || v4.is_loopback(),
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || (v6.segments()[0] & 0xffc0) == 0xfe80
                || (v6.segments()[0] & 0xfe00) == 0xfc00
        }
    }
}

/// Lê um ProbeMatch. `None` para qualquer coisa que não seja uma resposta
/// válida desta busca.
pub fn parse_probe_match(data: &[u8], message_id: &str, from: IpAddr) -> Option<DiscoveredDevice> {
    if data.len() > MAX_DATAGRAM || !trusted_origin(from) {
        return None;
    }
    let text = std::str::from_utf8(data).ok()?;
    let doc = xml::parse(text).ok()?;
    if doc.name != "Envelope" {
        return None;
    }
    let relates = doc.path(&["Header", "RelatesTo"])?.text_trimmed();
    if relates != message_id {
        return None;
    }
    let m = doc.path(&["Body", "ProbeMatches", "ProbeMatch"])?;
    let endpoint = m
        .find("Address")
        .map(|a| a.text_trimmed().to_string())
        .filter(|s| !s.is_empty());
    let xaddrs: Vec<&str> = m
        .child("XAddrs")
        .map(Element::text_trimmed)
        .unwrap_or_default()
        .split_whitespace()
        .collect();
    let device_url = choose_xaddr(&xaddrs, &from.to_string())?;
    // Host da câmera = o do serviço escolhido (é o que se cadastra). Em
    // máquinas com várias interfaces, a origem do datagrama pode diferir.
    let host = crate::client::DeviceService::from_url(&device_url)?.host;
    let scopes: Vec<&str> = m
        .child("Scopes")
        .map(Element::text_trimmed)
        .unwrap_or_default()
        .split_whitespace()
        .collect();
    let scope = |kind: &str| {
        scopes
            .iter()
            .find_map(|s| s.strip_prefix(&format!("onvif://www.onvif.org/{kind}/")))
            .map(decode_scope)
            .filter(|s| !s.is_empty())
    };
    let name = scope("name");
    let hardware = scope("hardware");
    Some(DiscoveredDevice {
        endpoint: endpoint.unwrap_or_else(|| device_url.clone()),
        host,
        device_url,
        name: name.or_else(|| hardware.clone()),
        hardware,
    })
}

/// Só `http://`; prefere o XAddr com o mesmo IP de origem, depois o primeiro
/// com host IPv4, depois o primeiro http.
fn choose_xaddr(xaddrs: &[&str], from_host: &str) -> Option<String> {
    let http: Vec<&str> = xaddrs
        .iter()
        .copied()
        .filter(|x| x.starts_with("http://"))
        .collect();
    let host_of = |x: &str| crate::client::DeviceService::from_url(x).map(|d| d.host);
    http.iter()
        .find(|x| host_of(x).as_deref() == Some(from_host))
        .or_else(|| {
            http.iter()
                .find(|x| host_of(x).is_some_and(|h| h.parse::<Ipv4Addr>().is_ok()))
        })
        .or_else(|| http.first())
        .map(|x| x.to_string())
}

/// Scopes ONVIF usam codificação percentual (e às vezes `_` no lugar de espaço).
fn decode_scope(raw: &str) -> String {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(b) = raw
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(if bytes[i] == b'_' { b' ' } else { bytes[i] });
        i += 1;
    }
    String::from_utf8_lossy(&out)
        .trim()
        .chars()
        .take(60)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "uuid:11111111-2222-3333-4444-555555555555";

    fn reply(relates: &str, address: &str, xaddrs: &str, scopes: &str) -> String {
        format!(
            r#"<?xml version="1.0"?><SOAP-ENV:Envelope xmlns:SOAP-ENV="http://www.w3.org/2003/05/soap-envelope" xmlns:wsa="http://schemas.xmlsoap.org/ws/2004/08/addressing" xmlns:d="http://schemas.xmlsoap.org/ws/2005/04/discovery">
            <SOAP-ENV:Header><wsa:MessageID>uuid:x</wsa:MessageID><wsa:RelatesTo>{relates}</wsa:RelatesTo></SOAP-ENV:Header>
            <SOAP-ENV:Body><d:ProbeMatches><d:ProbeMatch><wsa:EndpointReference><wsa:Address>{address}</wsa:Address></wsa:EndpointReference>
            <d:Types>dn:NetworkVideoTransmitter</d:Types><d:Scopes>{scopes}</d:Scopes><d:XAddrs>{xaddrs}</d:XAddrs>
            <d:MetadataVersion>1</d:MetadataVersion></d:ProbeMatch></d:ProbeMatches></SOAP-ENV:Body></SOAP-ENV:Envelope>"#
        )
    }

    fn lan() -> IpAddr {
        "192.168.1.120".parse().unwrap()
    }

    #[test]
    fn le_probe_match_tapo() {
        let r = reply(
            ID,
            "urn:uuid:3fa1fe68-b915-4053-a3e1-c006c3afec0e",
            "http://192.168.1.120:2020/onvif/device_service",
            "onvif://www.onvif.org/name/TP-IPC onvif://www.onvif.org/hardware/C200 onvif://www.onvif.org/Profile/Streaming",
        );
        let d = parse_probe_match(r.as_bytes(), ID, lan()).unwrap();
        assert_eq!(d.endpoint, "urn:uuid:3fa1fe68-b915-4053-a3e1-c006c3afec0e");
        assert_eq!(
            d.device_url,
            "http://192.168.1.120:2020/onvif/device_service"
        );
        assert_eq!(d.host, "192.168.1.120");
        assert_eq!(d.name.as_deref(), Some("TP-IPC"));
        assert_eq!(d.hardware.as_deref(), Some("C200"));
    }

    #[test]
    fn prefere_xaddr_do_ip_de_origem_e_so_http() {
        let r = reply(
            ID,
            "urn:a",
            "https://192.168.1.120/onvif/device_service http://10.0.0.9/onvif/device_service http://[fe80::1]/x http://192.168.1.120/onvif/device_service",
            "",
        );
        assert_eq!(
            parse_probe_match(r.as_bytes(), ID, lan())
                .unwrap()
                .device_url,
            "http://192.168.1.120/onvif/device_service"
        );
        // Resposta chegou por outra interface: o host é o anunciado.
        let r = reply(
            ID,
            "urn:a",
            "http://192.168.240.1:8099/onvif/device_service",
            "",
        );
        let d = parse_probe_match(r.as_bytes(), ID, "172.23.0.1".parse().unwrap()).unwrap();
        assert_eq!(d.host, "192.168.240.1");
        let so_https = reply(
            ID,
            "urn:a",
            "https://192.168.1.120/onvif/device_service",
            "",
        );
        assert!(parse_probe_match(so_https.as_bytes(), ID, lan()).is_none());
    }

    #[test]
    fn nome_decodificado_e_hardware_como_reserva() {
        let r = reply(
            ID,
            "urn:a",
            "http://192.168.1.120/x",
            "onvif://www.onvif.org/name/Port%C3%A3o_Frente onvif://www.onvif.org/hardware/DS-2CD",
        );
        assert_eq!(
            parse_probe_match(r.as_bytes(), ID, lan())
                .unwrap()
                .name
                .as_deref(),
            Some("Portão Frente")
        );
        let r = reply(
            ID,
            "urn:a",
            "http://192.168.1.120/x",
            "onvif://www.onvif.org/hardware/DS-2CD",
        );
        assert_eq!(
            parse_probe_match(r.as_bytes(), ID, lan())
                .unwrap()
                .name
                .as_deref(),
            Some("DS-2CD")
        );
    }

    #[test]
    fn descarta_respostas_invalidas() {
        let boa = reply(ID, "urn:a", "http://192.168.1.120/x", "");
        let outra_busca = reply("uuid:outra", "urn:a", "http://192.168.1.120/x", "");
        assert!(parse_probe_match(outra_busca.as_bytes(), ID, lan()).is_none());
        assert!(parse_probe_match(b"<lixo", ID, lan()).is_none());
        assert!(parse_probe_match(&[0xff, 0xfe], ID, lan()).is_none());
        assert!(parse_probe_match(boa.as_bytes(), ID, "8.8.8.8".parse().unwrap()).is_none());
        let grande = format!("{boa}{}", " ".repeat(MAX_DATAGRAM));
        assert!(parse_probe_match(grande.as_bytes(), ID, lan()).is_none());
        let sem_xaddr = reply(ID, "urn:a", "", "");
        assert!(parse_probe_match(sem_xaddr.as_bytes(), ID, lan()).is_none());
    }

    #[test]
    fn origens_confiaveis() {
        for ok in [
            "10.1.2.3",
            "172.16.0.1",
            "192.168.0.1",
            "169.254.1.1",
            "127.0.0.1",
            "fe80::1",
        ] {
            assert!(trusted_origin(ok.parse().unwrap()), "{ok}");
        }
        for no in ["8.8.8.8", "100.64.0.1", "2001:db8::1"] {
            assert!(!trusted_origin(no.parse().unwrap()), "{no}");
        }
    }

    #[test]
    fn probe_tem_id_e_tipo() {
        let p = probe_message(ID);
        assert!(p.contains(&format!("<a:MessageID>{ID}</a:MessageID>")));
        assert!(p.contains("dn:NetworkVideoTransmitter"));
        assert!(xml::parse(&p).is_ok());
        let id = new_message_id();
        assert!(id.starts_with("uuid:") && id.len() == 41, "{id}");
        assert_ne!(id, new_message_id());
    }
}
