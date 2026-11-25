//! WS-Discovery (design D4): encontra câmeras ONVIF na rede local.
//!
//! Um Probe multicast por interface IPv4 ativa, TTL 1, e o mesmo Probe por
//! unicast a cada host da sub-rede privada da interface (câmeras que ignoram
//! o multicast, redes que não o repassam); escuta por alguns segundos.
//! Respostas são entrada não confiável: só ProbeMatch da busca atual
//! (`RelatesTo`), de origem privada, link-local ou loopback, até 64 KiB, com
//! XAddr de host IP também privado, link-local ou loopback. Fila de recepção
//! e número de dispositivos limitados.

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use socket2::{Domain, Protocol, Socket, Type};

use crate::auth::random_bytes;
use crate::xml::{self, Element};

/// Endereço multicast padrão do WS-Discovery.
pub const MULTICAST: SocketAddrV4 = SocketAddrV4::new(Ipv4Addr::new(239, 255, 255, 250), 3702);

/// Tamanho máximo aceito de uma resposta UDP.
pub const MAX_DATAGRAM: usize = 64 * 1024;

/// Respostas aguardando o parse. Com [`MAX_DATAGRAM`], limita a fila a
/// ~4 MiB mesmo sob flood; o excedente é descartado.
const QUEUE_CAPACITY: usize = 64;

/// Máximo de dispositivos distintos numa busca; os seguintes são ignorados.
pub const MAX_DEVICES: usize = 256;

type Datagram = (Vec<u8>, SocketAddr);

#[derive(Debug, Clone)]
pub struct DiscoveryConfig {
    /// Quanto tempo esperar respostas.
    pub duration: Duration,
    /// Destino do Probe (multicast em produção; unicast nos testes).
    pub target: SocketAddrV4,
    /// Interfaces de onde enviar; `None` = todas as IPv4 ativas sem loopback.
    pub interfaces: Option<Vec<Ipv4Addr>>,
    /// Destinos do Probe unicast, enviado de cada interface além do
    /// multicast; `None` = a sub-rede privada de cada interface, na porta
    /// 3702 (design D4). Injetável nos testes (loopback).
    pub unicast: Option<Vec<SocketAddrV4>>,
}

impl Default for DiscoveryConfig {
    fn default() -> Self {
        Self {
            duration: Duration::from_secs(3),
            target: MULTICAST,
            interfaces: None,
            unicast: None,
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
        Some(list) => list.iter().map(|ip| (*ip, None)).collect(),
        None => active_ipv4_interfaces(),
    };
    if interfaces.is_empty() {
        return Err(DiscoveryError::NoInterface);
    }
    let unicast = unicast_plan(&interfaces, config.unicast.as_deref());
    let message_id = new_message_id();
    let probe = probe_message(&message_id);
    let deadline = Instant::now() + config.duration;

    // Fila limitada: um host da LAN em flood não faz a memória crescer; o que
    // não cabe é descartado na thread de recepção.
    let (tx, rx) = mpsc::sync_channel::<Datagram>(QUEUE_CAPACITY);
    let mut sent_any = false;
    let mut last_error = None;
    let probe_shared: Arc<[u8]> = Arc::from(probe.as_bytes());
    for ((iface, _), targets) in interfaces.into_iter().zip(unicast) {
        let sock = match open_socket(iface) {
            Ok(sock) => sock,
            Err(e) => {
                last_error = Some(e.to_string());
                continue;
            }
        };
        let multicast_ok = match sock.send_to(probe.as_bytes(), config.target) {
            Ok(_) => true,
            Err(e) => {
                last_error = Some(e.to_string());
                false
            }
        };
        // Sem multicast nesta interface, a varredura unicast ainda vale.
        let sender = match sock.try_clone() {
            Ok(s) if !targets.is_empty() => Some(s),
            _ => None,
        };
        if !multicast_ok && sender.is_none() {
            continue;
        }
        sent_any = true;
        // A escuta começa antes; a varredura corre em paralelo, uma thread por
        // interface (uma sub-rede sem hosts, cujo envio espera o ARP, não
        // atrasa as outras), dentro do prazo.
        if let Some(s) = sender {
            let probe = probe_shared.clone();
            thread::spawn(move || send_unicast(&s, &targets, &probe, deadline));
        }
        let tx = tx.clone();
        let id = message_id.clone();
        thread::spawn(move || receive_until(sock, deadline, &id, tx));
    }
    drop(tx);
    if !sent_any {
        return Err(DiscoveryError::Network(last_error.unwrap_or_default()));
    }

    Ok(collect_devices(rx, &message_id))
}

/// Destinos da varredura enviados sem pausa.
const SWEEP_BATCH: usize = 32;
/// Pausa entre lotes, para não disparar centenas de resoluções ARP de uma vez.
const SWEEP_PAUSE: Duration = Duration::from_millis(2);
/// Teto de destinos unicast numa busca, somando todas as interfaces.
pub const MAX_UNICAST_TARGETS: usize = 1024;
/// Porta do WS-Discovery nas câmeras.
const DISCOVERY_PORT: u16 = 3702;
/// Buffer de envio do socket. Datagramas para hosts sem ARP resolvido ficam
/// retidos e ocupam o buffer; com o padrão do Windows (64 KiB) o envio
/// bloqueia por segundos depois de ~100 destinos. Cabe uma sub-rede inteira
/// (254 × ~0,6 KiB).
const SEND_BUFFER: usize = 256 * 1024;

/// Envia o Probe aos destinos unicast de uma interface, pelo socket dela
/// (mesmo `MessageID`, respostas pela mesma recepção). Erros de envio a um
/// destino são ignorados; para no prazo.
fn send_unicast(sock: &UdpSocket, targets: &[SocketAddrV4], probe: &[u8], deadline: Instant) {
    for (i, target) in targets.iter().enumerate() {
        if Instant::now() >= deadline {
            return;
        }
        let _ = sock.send_to(probe, target);
        if (i + 1) % SWEEP_BATCH == 0 {
            thread::sleep(SWEEP_PAUSE);
        }
    }
}

/// Destinos unicast por interface, na ordem de `interfaces`: os injetados
/// (testes) em todas, ou a varredura da sub-rede privada de cada uma, sem
/// os endereços da própria máquina, sem repetir destino entre interfaces e
/// com no máximo [`MAX_UNICAST_TARGETS`] no total.
fn unicast_plan(
    interfaces: &[(Ipv4Addr, Option<Ipv4Addr>)],
    injected: Option<&[SocketAddrV4]>,
) -> Vec<Vec<SocketAddrV4>> {
    if let Some(list) = injected {
        return interfaces.iter().map(|_| list.to_vec()).collect();
    }
    let mut seen: std::collections::BTreeSet<Ipv4Addr> =
        interfaces.iter().map(|(ip, _)| *ip).collect();
    let mut total = 0;
    let mut plan = Vec::with_capacity(interfaces.len());
    for (ip, mask) in interfaces {
        let mut targets = Vec::new();
        for host in mask.map(|m| sweep_hosts(*ip, m)).unwrap_or_default() {
            if total >= MAX_UNICAST_TARGETS {
                break;
            }
            if seen.insert(host) {
                targets.push(SocketAddrV4::new(host, DISCOVERY_PORT));
                total += 1;
            }
        }
        plan.push(targets);
    }
    plan
}

/// Hosts da sub-rede de uma interface para o Probe unicast (design D4). Só
/// endereço privado (RFC 1918): link-local, loopback e público dão lista
/// vazia. Prefixo /24 ou mais longo: a sub-rede real; mais curto (/20, /16):
/// só o /24 que contém o endereço. Sem rede, broadcast e o próprio endereço;
/// no máximo 254 hosts.
fn sweep_hosts(ip: Ipv4Addr, netmask: Ipv4Addr) -> Vec<Ipv4Addr> {
    if !ip.is_private() {
        return Vec::new();
    }
    let prefix = u32::from(netmask).leading_ones().max(24);
    if prefix > 30 {
        return Vec::new();
    }
    let mask = u32::MAX << (32 - prefix);
    let network = u32::from(ip) & mask;
    let broadcast = network | !mask;
    (network + 1..broadcast)
        .map(Ipv4Addr::from)
        .filter(|h| *h != ip)
        .collect()
}

/// Lê as respostas até as threads de recepção terminarem: dispositivos sem
/// repetição (por `EndpointReference`), no máximo [`MAX_DEVICES`], ordenados
/// por IP.
fn collect_devices(
    datagrams: impl IntoIterator<Item = Datagram>,
    message_id: &str,
) -> Vec<DiscoveredDevice> {
    let mut found: BTreeMap<String, DiscoveredDevice> = BTreeMap::new();
    for (data, from) in datagrams {
        if let Some(dev) = parse_probe_match(&data, message_id, from.ip())
            && (found.len() < MAX_DEVICES || found.contains_key(&dev.endpoint))
        {
            found.entry(dev.endpoint.clone()).or_insert(dev);
        }
    }
    let mut list: Vec<DiscoveredDevice> = found.into_values().collect();
    list.sort_by_key(|d| sort_key(&d.host));
    list
}

fn sort_key(host: &str) -> (u32, String) {
    match host.parse::<Ipv4Addr>() {
        Ok(ip) => (u32::from(ip), String::new()),
        Err(_) => (u32::MAX, host.to_string()),
    }
}

/// Interfaces IPv4 ativas sem loopback, com a máscara.
fn active_ipv4_interfaces() -> Vec<(Ipv4Addr, Option<Ipv4Addr>)> {
    let mut out: Vec<(Ipv4Addr, Option<Ipv4Addr>)> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| !i.is_loopback())
        .filter_map(|i| match i.addr {
            if_addrs::IfAddr::V4(v4) if !v4.ip.is_unspecified() => Some((v4.ip, Some(v4.netmask))),
            _ => None,
        })
        .collect();
    out.sort();
    out.dedup_by_key(|(ip, _)| *ip);
    out
}

fn open_socket(iface: Ipv4Addr) -> std::io::Result<UdpSocket> {
    let s = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
    s.bind(&SocketAddr::from((iface, 0)).into())?;
    if !iface.is_loopback() {
        s.set_multicast_if_v4(&iface)?;
    }
    s.set_multicast_ttl_v4(1)?;
    // Melhor esforço: sem ele, a varredura só fica mais lenta.
    let _ = s.set_send_buffer_size(SEND_BUFFER);
    s.set_read_timeout(Some(Duration::from_millis(100)))?;
    Ok(s.into())
}

/// Triagem barata antes de copiar e enfileirar: origem confiável, UTF-8 e o
/// `MessageID` da busca presente (o `RelatesTo` o contém). O parse completo
/// fica em [`parse_probe_match`].
fn worth_queueing(data: &[u8], from: IpAddr, message_id: &str) -> bool {
    data.len() <= MAX_DATAGRAM
        && trusted_origin(from)
        && std::str::from_utf8(data).is_ok_and(|t| t.contains(message_id))
}

fn receive_until(
    sock: UdpSocket,
    deadline: Instant,
    message_id: &str,
    tx: mpsc::SyncSender<Datagram>,
) {
    // Um byte a mais que o limite: se ele for preenchido, a resposta é grande
    // demais e é descartada.
    let mut buf = vec![0u8; MAX_DATAGRAM + 1];
    while Instant::now() < deadline {
        match sock.recv_from(&mut buf) {
            Ok((n, from)) if worth_queueing(&buf[..n], from.ip(), message_id) => {
                match tx.try_send((buf[..n].to_vec(), from)) {
                    // Fila cheia: descarta (flood); a busca segue.
                    Ok(()) | Err(mpsc::TrySendError::Full(_)) => {}
                    Err(mpsc::TrySendError::Disconnected(_)) => return,
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
    let device_url = choose_xaddr(&xaddrs, from)?;
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

/// Só `http://` com host IP literal confiável (privado, link-local ou
/// loopback), sem credenciais embutidas: o XAddr vira o destino das
/// credenciais no cadastro, então nome DNS, IP público ou `user@host` são
/// descartados (ADR 0005). Prefere o XAddr com o mesmo IP de origem, depois o
/// primeiro IPv4, depois o primeiro IPv6. Nenhum aceito → `None`.
fn choose_xaddr(xaddrs: &[&str], from: IpAddr) -> Option<String> {
    let accepted: Vec<(&str, IpAddr)> = xaddrs
        .iter()
        .filter_map(|x| {
            let ip = crate::client::DeviceService::from_url(x)?
                .host
                .parse::<IpAddr>()
                .ok()?;
            trusted_origin(ip).then_some((*x, ip))
        })
        .collect();
    accepted
        .iter()
        .find(|(_, ip)| *ip == from)
        .or_else(|| accepted.iter().find(|(_, ip)| ip.is_ipv4()))
        .or_else(|| accepted.first())
        .map(|(x, _)| x.to_string())
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
    fn xaddr_so_com_ip_confiavel() {
        let device = |xaddrs: &str| {
            parse_probe_match(reply(ID, "urn:a", xaddrs, "").as_bytes(), ID, lan())
                .map(|d| (d.host, d.device_url))
        };
        // Destinos que levariam as credenciais para fora da LAN ou para um
        // host que a origem não controla: o ProbeMatch é descartado.
        for bad in [
            "http://8.8.8.8/onvif/device_service",
            "http://evil.example/onvif/device_service",
            "http://camera.lan/onvif/device_service",
            "http://192.168.1.120@evil.com/onvif/device_service",
            "http://admin:x@192.168.1.120/onvif/device_service",
            "http://[2001:db8::1]/onvif/device_service",
            "http://100.64.0.1/onvif/device_service",
        ] {
            assert_eq!(device(bad), None, "{bad}");
        }
        // Os inválidos são pulados; vale o primeiro IP confiável.
        assert_eq!(
            device("http://evil.example/x http://8.8.8.8/x http://10.0.0.9/x"),
            Some(("10.0.0.9".into(), "http://10.0.0.9/x".into()))
        );
        // Só IPv6 link-local: aceito.
        assert_eq!(
            device("http://[fe80::1]:80/x"),
            Some(("fe80::1".into(), "http://[fe80::1]:80/x".into()))
        );
    }

    #[test]
    fn limita_dispositivos_distintos() {
        let from: SocketAddr = "192.168.1.120:3702".parse().unwrap();
        let datagrams = (0..MAX_DEVICES + 50)
            .map(|i| {
                let r = reply(ID, &format!("urn:d{i}"), "http://192.168.1.120/x", "");
                (r.into_bytes(), from)
            })
            // Um já visto depois do limite continua sem duplicar.
            .chain(std::iter::once((
                reply(ID, "urn:d0", "http://192.168.1.120/x", "").into_bytes(),
                from,
            )));
        let list = collect_devices(datagrams, ID);
        assert_eq!(list.len(), MAX_DEVICES);
        assert!(list.iter().any(|d| d.endpoint == "urn:d0"));
    }

    #[test]
    fn triagem_antes_da_fila() {
        let boa = reply(ID, "urn:a", "http://192.168.1.120/x", "");
        assert!(worth_queueing(boa.as_bytes(), lan(), ID));
        let outra = reply("uuid:outra", "urn:a", "http://192.168.1.120/x", "");
        assert!(!worth_queueing(outra.as_bytes(), lan(), ID));
        assert!(!worth_queueing(
            boa.as_bytes(),
            "8.8.8.8".parse().unwrap(),
            ID
        ));
        assert!(!worth_queueing(&[0xff, 0xfe], lan(), ID));
        let grande = format!("{boa}{}", " ".repeat(MAX_DATAGRAM));
        assert!(!worth_queueing(grande.as_bytes(), lan(), ID));
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

    /// Formato real de câmeras TP-Link Tapo (prefixos `SOAP-ENV`/`wsa`/`wsdd`/
    /// `tdn`, `MessageID` igual ao do Probe, `ReferenceProperties` vazio e
    /// `PortType`), com valores fictícios.
    #[test]
    fn le_probe_match_no_formato_da_tapo() {
        let r = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<SOAP-ENV:Envelope xmlns:SOAP-ENV="http://www.w3.org/2003/05/soap-envelope" xmlns:SOAP-ENC="http://www.w3.org/2003/05/soap-encoding" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance" xmlns:wsa="http://schemas.xmlsoap.org/ws/2004/08/addressing" xmlns:wsdd="http://schemas.xmlsoap.org/ws/2005/04/discovery" xmlns:chan="http://schemas.microsoft.com/ws/2005/02/duplex" xmlns:wsa5="http://www.w3.org/2005/08/addressing" xmlns:tt="http://www.onvif.org/ver10/schema" xmlns:tds="http://www.onvif.org/ver10/device/wsdl" xmlns:tdn="http://www.onvif.org/ver10/network/wsdl"><SOAP-ENV:Header><wsa:MessageID>{ID}</wsa:MessageID><wsa:RelatesTo>{ID}</wsa:RelatesTo><wsa:To SOAP-ENV:mustUnderstand="true">urn:schemas-xmlsoap-org:ws:2005:04:discovery</wsa:To><wsa:Action SOAP-ENV:mustUnderstand="true">http://schemas.xmlsoap.org/ws/2005/04/discovery/ProbeMatches</wsa:Action></SOAP-ENV:Header><SOAP-ENV:Body><wsdd:ProbeMatches><wsdd:ProbeMatch><wsa:EndpointReference><wsa:Address>uuid:00000000-1111-4222-8333-444444444444</wsa:Address><wsa:ReferenceProperties></wsa:ReferenceProperties><wsa:PortType>ttl</wsa:PortType></wsa:EndpointReference><wsdd:Types>tdn:NetworkVideoTransmitter</wsdd:Types><wsdd:Scopes>onvif://www.onvif.org/name/C320WS onvif://www.onvif.org/hardware/C320WS onvif://www.onvif.org/Profile/Streaming onvif://www.onvif.org/location/Sala%201 onvif://www.onvif.org/type/NetworkVideoTransmitter</wsdd:Scopes><wsdd:XAddrs>http://10.20.30.40:2020/onvif/device_service</wsdd:XAddrs><wsdd:MetadataVersion>1</wsdd:MetadataVersion></wsdd:ProbeMatch></wsdd:ProbeMatches></SOAP-ENV:Body></SOAP-ENV:Envelope>
"#
        );
        let from: IpAddr = "10.20.30.40".parse().unwrap();
        let d = parse_probe_match(r.as_bytes(), ID, from).unwrap();
        assert_eq!(d.endpoint, "uuid:00000000-1111-4222-8333-444444444444");
        assert_eq!(d.device_url, "http://10.20.30.40:2020/onvif/device_service");
        assert_eq!(d.host, "10.20.30.40");
        assert_eq!(d.name.as_deref(), Some("C320WS"));
        assert_eq!(d.hardware.as_deref(), Some("C320WS"));
        // Resposta de outra busca (MessageID/RelatesTo diferentes): descartada.
        assert!(parse_probe_match(r.as_bytes(), "uuid:outra", from).is_none());
    }

    fn ip(s: &str) -> Ipv4Addr {
        s.parse().unwrap()
    }

    #[test]
    fn varredura_da_sub_rede_24() {
        let hosts = sweep_hosts(ip("192.168.50.7"), ip("255.255.255.0"));
        assert_eq!(hosts.len(), 253);
        assert_eq!(hosts.first(), Some(&ip("192.168.50.1")));
        assert_eq!(hosts.last(), Some(&ip("192.168.50.254")));
        assert!(!hosts.contains(&ip("192.168.50.0")));
        assert!(!hosts.contains(&ip("192.168.50.255")));
        assert!(!hosts.contains(&ip("192.168.50.7")));
    }

    #[test]
    fn varredura_de_prefixos_menores_que_24_em_hosts() {
        // /25: só a metade de cima, sem rede, broadcast e o próprio endereço.
        let hosts = sweep_hosts(ip("10.0.0.200"), ip("255.255.255.128"));
        assert_eq!(hosts.len(), 125);
        assert_eq!(hosts.first(), Some(&ip("10.0.0.129")));
        assert_eq!(hosts.last(), Some(&ip("10.0.0.254")));
        // /30: um único vizinho.
        assert_eq!(
            sweep_hosts(ip("172.16.5.1"), ip("255.255.255.252")),
            vec![ip("172.16.5.2")]
        );
        // /31 e /32: nada a varrer.
        assert!(sweep_hosts(ip("10.0.0.1"), ip("255.255.255.254")).is_empty());
        assert!(sweep_hosts(ip("10.0.0.1"), ip("255.255.255.255")).is_empty());
    }

    #[test]
    fn varredura_de_rede_maior_fica_no_24_do_endereco() {
        // /20 e /16: só o /24 que contém o endereço da interface.
        let hosts = sweep_hosts(ip("172.20.3.7"), ip("255.255.240.0"));
        assert_eq!(hosts.len(), 253);
        assert_eq!(hosts.first(), Some(&ip("172.20.3.1")));
        assert_eq!(hosts.last(), Some(&ip("172.20.3.254")));
        let hosts = sweep_hosts(ip("192.168.7.10"), ip("255.255.0.0"));
        assert_eq!(hosts.len(), 253);
        assert!(hosts.iter().all(|h| h.octets()[..3] == [192, 168, 7]));
    }

    #[test]
    fn varredura_so_em_endereco_privado() {
        let m24 = ip("255.255.255.0");
        for no in ["169.254.10.20", "127.0.0.1", "8.8.8.8", "100.64.0.1"] {
            assert!(sweep_hosts(ip(no), m24).is_empty(), "{no}");
            assert!(sweep_hosts(ip(no), ip("255.255.0.0")).is_empty(), "{no}");
        }
    }

    #[test]
    fn plano_unicast_sem_repeticao_e_com_teto() {
        let m24 = Some(ip("255.255.255.0"));
        // Duas interfaces na mesma sub-rede: a segunda não repete destinos, e
        // nenhuma varre os endereços da própria máquina.
        let plan = unicast_plan(
            &[
                (ip("192.168.1.10"), m24),
                (ip("192.168.1.11"), m24),
                (ip("169.254.3.4"), Some(ip("255.255.0.0"))),
                (ip("10.9.9.9"), None),
            ],
            None,
        );
        assert_eq!(plan.len(), 4);
        assert_eq!(plan[0].len(), 252);
        assert!(plan[0].iter().all(|a| a.port() == 3702));
        assert!(!plan[0].contains(&SocketAddrV4::new(ip("192.168.1.11"), 3702)));
        assert!(plan[1].is_empty());
        // Link-local e interface sem máscara: nada.
        assert!(plan[2].is_empty() && plan[3].is_empty());
        // Muitas interfaces: no máximo MAX_UNICAST_TARGETS no total.
        let many: Vec<(Ipv4Addr, Option<Ipv4Addr>)> =
            (0..8).map(|i| (Ipv4Addr::new(10, 0, i, 1), m24)).collect();
        let plan = unicast_plan(&many, None);
        let total: usize = plan.iter().map(Vec::len).sum();
        assert_eq!(total, MAX_UNICAST_TARGETS);
        assert!(plan.iter().all(|p| p.len() <= 254));
        assert!(plan[7].is_empty());
        // Destinos injetados (testes): os mesmos em cada interface.
        let alvo = SocketAddrV4::new(Ipv4Addr::LOCALHOST, 9);
        assert_eq!(
            unicast_plan(&[(Ipv4Addr::LOCALHOST, None)], Some(&[alvo])),
            vec![vec![alvo]]
        );
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
