//! Simulador de câmera ONVIF para testes manuais e de ponta a ponta.
//!
//! ```text
//! cargo run -p rtsp-viewer-onvif --example fake_onvif -- [opções]
//!   --bind <ip>          endereço de escuta (padrão 0.0.0.0)
//!   --port <n>           porta HTTP do serviço ONVIF (padrão 8099)
//!   --user <u>           usuário (padrão admin)
//!   --pass <p>           senha (padrão admin123; fictícia)
//!   --digest             exige HTTP Digest em vez de WS-Security
//!   --clock-offset <s>   relógio da câmera deslocado em segundos
//!   --main <url>         URL RTSP do stream principal
//!   --sub <url>          URL RTSP do stream secundário
//!   --advertise <ip>     IP anunciado no WS-Discovery (padrão: o primeiro da máquina)
//! ```
//!
//! Também responde ao WS-Discovery (UDP 3702, multicast) em todas as
//! interfaces, para o app encontrar o simulador com "Procurar câmeras".
//!
//! Os valores padrão apontam para a câmera falsa (`test-camera/`).

#[path = "../tests/support/simulator.rs"]
mod simulator;

use simulator::{SimAuth, SimConfig, SimProfile, Simulator};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let opt = |name: &str| -> Option<String> {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let bind = opt("--bind").unwrap_or_else(|| "0.0.0.0".into());
    let port: u16 = opt("--port").and_then(|p| p.parse().ok()).unwrap_or(8099);
    let config = SimConfig {
        username: opt("--user").unwrap_or_else(|| "admin".into()),
        password: opt("--pass").unwrap_or_else(|| "admin123".into()),
        auth: if args.iter().any(|a| a == "--digest") {
            SimAuth::HttpDigest
        } else {
            SimAuth::WsSecurity
        },
        clock_offset: opt("--clock-offset")
            .and_then(|s| s.parse().ok())
            .unwrap_or(0),
        profiles: vec![
            SimProfile {
                token: "main".into(),
                width: 1280,
                height: 720,
                uri: opt("--main").unwrap_or_else(|| "rtsp://{host}:8554/h264".into()),
            },
            SimProfile {
                token: "sub".into(),
                width: 640,
                height: 360,
                uri: opt("--sub").unwrap_or_else(|| "rtsp://{host}:8554/queda".into()),
            },
        ],
        ..Default::default()
    };
    let sim = Simulator::start_on(&format!("{bind}:{port}"), config);
    println!(
        "Simulador ONVIF em http://{bind}:{}/onvif/device_service (Ctrl+C para sair)",
        sim.port
    );

    // WS-Discovery em multicast real, em todas as interfaces IPv4 ativas.
    let ifaces: Vec<std::net::Ipv4Addr> = if_addrs::get_if_addrs()
        .unwrap_or_default()
        .into_iter()
        .filter(|i| !i.is_loopback())
        .filter_map(|i| match i.ip() {
            std::net::IpAddr::V4(v4) => Some(v4),
            _ => None,
        })
        .collect();
    let advertise = opt("--advertise").unwrap_or_else(|| {
        ifaces
            .first()
            .map(|ip| ip.to_string())
            .unwrap_or_else(|| "127.0.0.1".into())
    });
    let _responder = simulator::start_discovery_responder(
        "0.0.0.0:3702",
        &ifaces,
        &advertise,
        vec![simulator::DiscoveryReply::Match {
            endpoint: "urn:uuid:simulador-rtsp-viewer".into(),
            xaddr: format!("http://{{ip}}:{}/onvif/device_service", sim.port),
            scopes:
                "onvif://www.onvif.org/name/Simulador_ONVIF onvif://www.onvif.org/hardware/SIM-1"
                    .into(),
        }],
    );
    println!(
        "WS-Discovery anunciando {advertise}:{} em {} interface(s)",
        sim.port,
        ifaces.len()
    );
    loop {
        std::thread::park();
    }
}
