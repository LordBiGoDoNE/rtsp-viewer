//! Cliente ONVIF contra o simulador em processo (spec `camera-discovery`,
//! requisitos "Consulta dos streams pela câmera" e "Autenticação ONVIF sem
//! senha em texto puro").

mod support;

use std::net::TcpListener;
use std::time::Duration;

use rtsp_viewer_core::domain::Password;
use rtsp_viewer_onvif::{Credentials, DeviceService, OnvifClient, OnvifConfig, OnvifError};
use support::simulator::{SimAuth, SimConfig, SimProfile, Simulator};

fn creds(cfg: &SimConfig) -> Credentials {
    Credentials {
        username: cfg.username.clone(),
        password: Password::new(cfg.password.clone()),
    }
}

fn client() -> OnvifClient {
    OnvifClient::new(OnvifConfig {
        connect_timeout: Duration::from_secs(2),
        total_timeout: Duration::from_secs(5),
        probe_ports: vec![],
        probe_connect_timeout: Duration::from_millis(500),
    })
}

fn device(sim: &Simulator) -> DeviceService {
    DeviceService::from_url(&sim.device_url()).unwrap()
}

#[test]
fn obtem_principal_e_secundario_com_ws_security() {
    let cfg = SimConfig::default();
    let sim = Simulator::start(cfg.clone());
    let set = client()
        .fetch_streams(&device(&sim), Some(&creds(&cfg)))
        .unwrap();
    assert_eq!(set.manufacturer.as_deref(), Some("Simulador"));
    assert_eq!(set.model.as_deref(), Some("Camera ONVIF de teste"));
    assert_eq!(set.main.url.to_string(), "rtsp://127.0.0.1:8554/main");
    assert_eq!((set.main.width, set.main.height), (1920, 1080));
    let sub = set.sub.unwrap();
    assert_eq!(sub.url.to_string(), "rtsp://127.0.0.1:8554/sub");
    assert_eq!((sub.width, sub.height), (640, 360));
}

#[test]
fn senha_nunca_trafega_em_texto_puro() {
    let cfg = SimConfig::default();
    let sim = Simulator::start(cfg.clone());
    client()
        .fetch_streams(&device(&sim), Some(&creds(&cfg)))
        .unwrap();
    let traffic = sim.all_requests();
    assert!(traffic.contains("PasswordDigest"));
    assert!(
        !traffic.contains(&cfg.password),
        "senha em texto puro na rede"
    );
    assert!(!traffic.contains("PasswordText"));
}

#[test]
fn relogio_da_camera_com_horas_de_diferenca() {
    for offset in [3 * 3600, -5 * 3600, 400 * 86_400] {
        let cfg = SimConfig {
            clock_offset: offset,
            ..Default::default()
        };
        let sim = Simulator::start(cfg.clone());
        let set = client().fetch_streams(&device(&sim), Some(&creds(&cfg)));
        assert!(set.is_ok(), "offset {offset}: {set:?}");
    }
}

#[test]
fn camera_que_exige_http_digest() {
    let cfg = SimConfig {
        auth: SimAuth::HttpDigest,
        ..Default::default()
    };
    let sim = Simulator::start(cfg.clone());
    let set = client()
        .fetch_streams(&device(&sim), Some(&creds(&cfg)))
        .unwrap();
    assert_eq!(set.main.url.to_string(), "rtsp://127.0.0.1:8554/main");
    let traffic = sim.all_requests();
    assert!(
        traffic
            .to_ascii_lowercase()
            .contains("authorization: digest")
    );
    assert!(!traffic.contains(&cfg.password));
}

#[test]
fn senha_errada_com_ws_security_e_com_digest() {
    for auth in [SimAuth::WsSecurity, SimAuth::HttpDigest] {
        let cfg = SimConfig {
            auth,
            ..Default::default()
        };
        let sim = Simulator::start(cfg.clone());
        let wrong = Credentials {
            username: cfg.username.clone(),
            password: Password::new("errada"),
        };
        assert_eq!(
            client().fetch_streams(&device(&sim), Some(&wrong)),
            Err(OnvifError::AuthenticationFailed),
            "{auth:?}"
        );
    }
}

#[test]
fn sem_credenciais_em_camera_que_exige() {
    let sim = Simulator::start(SimConfig::default());
    assert_eq!(
        client().fetch_streams(&device(&sim), None),
        Err(OnvifError::AuthenticationFailed)
    );
}

#[test]
fn camera_aberta_sem_credenciais() {
    let sim = Simulator::start(SimConfig {
        auth: SimAuth::Open,
        ..Default::default()
    });
    assert!(client().fetch_streams(&device(&sim), None).is_ok());
}

#[test]
fn hosts_invalidos_trocados_pelo_da_camera() {
    let cfg = SimConfig {
        media_host_zero: true,
        profiles: vec![SimProfile {
            token: "main".into(),
            width: 1280,
            height: 720,
            uri: "rtsp://0.0.0.0:554/stream1".into(),
        }],
        ..Default::default()
    };
    let sim = Simulator::start(cfg.clone());
    let set = client()
        .fetch_streams(&device(&sim), Some(&creds(&cfg)))
        .unwrap();
    assert_eq!(set.main.url.to_string(), "rtsp://127.0.0.1:554/stream1");
    assert!(set.sub.is_none());
}

/// XAddr de mídia apontando para outro host (câmera maliciosa ou NAT): as
/// chamadas autenticadas seguem para o host da câmera, nunca para o anunciado
/// (ADR 0005). 192.0.2.10 é TEST-NET-1: se o cliente o seguisse, a conexão
/// falharia e a consulta não chegaria aos perfis.
#[test]
fn xaddr_de_midia_em_outro_host_nao_recebe_credenciais() {
    for auth in [SimAuth::WsSecurity, SimAuth::HttpDigest] {
        let cfg = SimConfig {
            auth,
            media_host: Some("192.0.2.10".into()),
            ..Default::default()
        };
        let sim = Simulator::start(cfg.clone());
        let set = client()
            .fetch_streams(&device(&sim), Some(&creds(&cfg)))
            .unwrap_or_else(|e| panic!("{auth:?}: {e:?}"));
        assert_eq!(set.main.url.to_string(), "rtsp://127.0.0.1:8554/main");
        let traffic = sim.all_requests();
        assert!(traffic.contains("GetProfiles"), "{auth:?}");
        assert!(traffic.contains("GetStreamUri"), "{auth:?}");
    }
}

/// O prazo total vale para a consulta inteira, não para cada chamada
/// (design D1): seis chamadas de 400 ms passam de 1,5 s.
#[test]
fn prazo_total_vale_para_a_consulta_inteira() {
    let cfg = SimConfig {
        response_delay: Duration::from_millis(400),
        ..Default::default()
    };
    let sim = Simulator::start(cfg.clone());
    let c = OnvifClient::new(OnvifConfig {
        connect_timeout: Duration::from_secs(2),
        total_timeout: Duration::from_millis(1500),
        probe_ports: vec![],
        probe_connect_timeout: Duration::from_millis(500),
    });
    let start = std::time::Instant::now();
    let result = c.fetch_streams(&device(&sim), Some(&creds(&cfg)));
    let elapsed = start.elapsed();
    assert!(
        matches!(result, Err(OnvifError::Unreachable(_))),
        "{result:?}"
    );
    assert!(elapsed < Duration::from_millis(2500), "{elapsed:?}");
}

#[test]
fn credenciais_embutidas_na_url_devolvida_sao_descartadas() {
    let cfg = SimConfig {
        profiles: vec![SimProfile {
            token: "main".into(),
            width: 1280,
            height: 720,
            uri: "rtsp://admin:outra@{host}:554/s".into(),
        }],
        ..Default::default()
    };
    let sim = Simulator::start(cfg.clone());
    let set = client()
        .fetch_streams(&device(&sim), Some(&creds(&cfg)))
        .unwrap();
    assert_eq!(set.main.url.to_string(), "rtsp://127.0.0.1:554/s");
}

#[test]
fn sem_perfis_de_video() {
    let cfg = SimConfig {
        profiles: vec![],
        ..Default::default()
    };
    let sim = Simulator::start(cfg.clone());
    assert_eq!(
        client().fetch_streams(&device(&sim), Some(&creds(&cfg))),
        Err(OnvifError::NoVideoProfiles)
    );
}

#[test]
fn resposta_grande_demais_e_recusada() {
    let cfg = SimConfig {
        oversized_profiles: true,
        ..Default::default()
    };
    let sim = Simulator::start(cfg.clone());
    assert!(matches!(
        client().fetch_streams(&device(&sim), Some(&creds(&cfg))),
        Err(OnvifError::Protocol(_))
    ));
}

#[test]
fn servidor_que_nao_e_onvif() {
    let cfg = SimConfig {
        not_onvif: true,
        ..Default::default()
    };
    let sim = Simulator::start(cfg.clone());
    assert_eq!(
        client().locate("127.0.0.1", Some(sim.port)),
        Err(OnvifError::NotOnvif)
    );
}

/// Porta livre sem servidor (para simular porta fechada).
fn closed_port() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    l.local_addr().unwrap().port()
}

#[test]
fn localiza_a_porta_onvif_entre_as_usuais() {
    let sim = Simulator::start(SimConfig::default());
    let c = OnvifClient::new(OnvifConfig {
        probe_ports: vec![closed_port(), sim.port, closed_port()],
        probe_connect_timeout: Duration::from_millis(500),
        ..Default::default()
    });
    let found = c.locate("127.0.0.1", None).unwrap();
    assert_eq!(found.url, sim.device_url());
}

#[test]
fn nenhuma_porta_responde() {
    let c = OnvifClient::new(OnvifConfig {
        probe_ports: vec![closed_port(), closed_port()],
        probe_connect_timeout: Duration::from_millis(300),
        total_timeout: Duration::from_secs(2),
        ..Default::default()
    });
    assert_eq!(c.locate("127.0.0.1", None), Err(OnvifError::NotOnvif));
}
