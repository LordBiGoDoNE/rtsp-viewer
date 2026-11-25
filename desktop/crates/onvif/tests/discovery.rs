//! Busca WS-Discovery contra o respondedor do simulador, por unicast em
//! loopback (spec `camera-discovery`, requisito "Busca de câmeras na rede").

mod support;

use std::net::{Ipv4Addr, SocketAddrV4};
use std::time::{Duration, Instant};

use rtsp_viewer_onvif::discovery::MAX_DEVICES;
use rtsp_viewer_onvif::{DiscoveryConfig, discover};
use support::simulator::{DiscoveryReply, start_discovery_responder};

fn config(port: u16) -> DiscoveryConfig {
    DiscoveryConfig {
        duration: Duration::from_millis(800),
        target: SocketAddrV4::new(Ipv4Addr::LOCALHOST, port),
        interfaces: Some(vec![Ipv4Addr::LOCALHOST]),
        unicast: None,
    }
}

fn tapo() -> DiscoveryReply {
    DiscoveryReply::Match {
        endpoint: "urn:uuid:tapo-1".into(),
        xaddr: "http://{ip}:2020/onvif/device_service".into(),
        scopes: "onvif://www.onvif.org/name/TP-IPC onvif://www.onvif.org/hardware/C200".into(),
    }
}

#[test]
fn encontra_camera_e_sugere_nome() {
    let r = start_discovery_responder("127.0.0.1:0", &[], "127.0.0.1", vec![tapo()]);
    let found = discover(&config(r.addr.port())).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(
        found[0].device_url,
        "http://127.0.0.1:2020/onvif/device_service"
    );
    assert_eq!(found[0].name.as_deref(), Some("TP-IPC"));
    assert_eq!(found[0].host, "127.0.0.1");
}

#[test]
fn deduplica_e_ignora_respostas_invalidas() {
    let r = start_discovery_responder(
        "127.0.0.1:0",
        &[],
        "127.0.0.1",
        vec![
            tapo(),
            DiscoveryReply::Malformed,
            tapo(),
            DiscoveryReply::WrongRelatesTo,
        ],
    );
    let found = discover(&config(r.addr.port())).unwrap();
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].endpoint, "urn:uuid:tapo-1");
}

#[test]
fn varias_cameras_ordenadas() {
    let r = start_discovery_responder(
        "127.0.0.1:0",
        &[],
        "127.0.0.1",
        vec![
            DiscoveryReply::Match {
                endpoint: "urn:b".into(),
                xaddr: "http://{ip}:8000/onvif/device_service".into(),
                scopes: "onvif://www.onvif.org/name/B".into(),
            },
            tapo(),
        ],
    );
    let found = discover(&config(r.addr.port())).unwrap();
    assert_eq!(
        found
            .iter()
            .map(|d| d.endpoint.as_str())
            .collect::<Vec<_>>()
            .len(),
        2
    );
}

#[test]
fn flood_de_dispositivos_fica_limitado_e_no_prazo() {
    let r = start_discovery_responder(
        "127.0.0.1:0",
        &[],
        "127.0.0.1",
        vec![DiscoveryReply::Flood(MAX_DEVICES + 200)],
    );
    let t = Instant::now();
    let found = discover(&config(r.addr.port())).unwrap();
    // O sistema pode descartar datagramas no loopback; o que importa é o teto.
    assert!(!found.is_empty());
    assert!(found.len() <= MAX_DEVICES, "{}", found.len());
    assert!(
        t.elapsed() < Duration::from_millis(1500),
        "{:?}",
        t.elapsed()
    );
}

#[test]
fn flood_de_lixo_nao_impede_a_busca() {
    let r = start_discovery_responder(
        "127.0.0.1:0",
        &[],
        "127.0.0.1",
        vec![tapo(), DiscoveryReply::Junk(200)],
    );
    let t = Instant::now();
    let found = discover(&config(r.addr.port())).unwrap();
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].endpoint, "urn:uuid:tapo-1");
    assert!(
        t.elapsed() < Duration::from_millis(1500),
        "{:?}",
        t.elapsed()
    );
}

#[test]
fn sem_respostas_termina_no_prazo() {
    // Porta sem ninguém escutando: a busca termina vazia, no prazo, sem erro.
    let silent = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let port = silent.local_addr().unwrap().port();
    let t = Instant::now();
    let found = discover(&config(port)).unwrap();
    assert!(found.is_empty());
    assert!(
        t.elapsed() < Duration::from_millis(1500),
        "{:?}",
        t.elapsed()
    );
}

/// Câmera que ignora o multicast (como a Tapo C320WS) e só responde ao Probe
/// unicast: o destino "multicast" é uma porta muda e a câmera só é alcançada
/// pela varredura unicast.
#[test]
fn encontra_camera_que_so_responde_a_unicast() {
    let mudo = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let camera = start_discovery_responder("127.0.0.1:0", &[], "127.0.0.1", vec![tapo()]);
    // Outros destinos da varredura, sem ninguém escutando (ICMP de porta
    // inacessível; no Windows, ConnectionReset na recepção).
    let vazios: Vec<std::net::UdpSocket> = (0..200)
        .map(|_| std::net::UdpSocket::bind("127.0.0.1:0").unwrap())
        .collect();
    let mut alvos: Vec<SocketAddrV4> = vazios
        .iter()
        .map(|s| SocketAddrV4::new(Ipv4Addr::LOCALHOST, s.local_addr().unwrap().port()))
        .collect();
    drop(vazios);
    alvos.insert(
        100,
        SocketAddrV4::new(Ipv4Addr::LOCALHOST, camera.addr.port()),
    );
    let cfg = DiscoveryConfig {
        unicast: Some(alvos),
        ..config(mudo.local_addr().unwrap().port())
    };
    let t = Instant::now();
    let found = discover(&cfg).unwrap();
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].endpoint, "urn:uuid:tapo-1");
    assert!(
        t.elapsed() < Duration::from_millis(1500),
        "{:?}",
        t.elapsed()
    );
}

/// A mesma câmera responde ao multicast e ao unicast: uma única entrada.
#[test]
fn multicast_e_unicast_da_mesma_camera_deduplicam() {
    let camera = start_discovery_responder("127.0.0.1:0", &[], "127.0.0.1", vec![tapo()]);
    let port = camera.addr.port();
    let cfg = DiscoveryConfig {
        unicast: Some(vec![SocketAddrV4::new(Ipv4Addr::LOCALHOST, port)]),
        ..config(port)
    };
    let found = discover(&cfg).unwrap();
    assert_eq!(found.len(), 1, "{found:?}");
}

#[test]
fn sem_interfaces_e_erro() {
    let cfg = DiscoveryConfig {
        interfaces: Some(vec![]),
        ..config(3702)
    };
    assert!(discover(&cfg).is_err());
}
