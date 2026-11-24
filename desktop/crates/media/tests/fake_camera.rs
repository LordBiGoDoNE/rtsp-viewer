//! Testes do player contra a câmera falsa (`test-camera/`).
//!
//! Rodam quando `RTSP_TEST_BASE` está definida (por exemplo
//! `rtsp://localhost:8554`); sem ela, cada teste avisa e passa. O teste de
//! queda e retorno também precisa de `RTSP_TEST_COMPOSE` (caminho do
//! `compose.yaml`), porque para e religa um publicador com `docker compose`.

use std::process::Command;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use rtsp_viewer_core::application::ports::{
    DecodeMode, PlaybackHandle, PlayerEvent, PlayerState, StreamError, StreamPlayer, StreamRequest,
};
use rtsp_viewer_core::domain::{CameraId, Password, RtspUrl};
use rtsp_viewer_media::{FfmpegPlayer, PlayerConfig};

fn base() -> Option<String> {
    let base = std::env::var("RTSP_TEST_BASE").ok()?;
    rtsp_viewer_media::init().unwrap();
    // Como o app faz no boot: o dispositivo de hardware fica pronto antes da
    // primeira câmera.
    rtsp_viewer_media::warm_up_hardware();
    Some(base.trim_end_matches('/').to_string())
}

macro_rules! require_camera {
    () => {
        match base() {
            Some(b) => b,
            None => {
                eprintln!("RTSP_TEST_BASE não definida: teste ignorado");
                return;
            }
        }
    };
}

fn request(url: &str, user: Option<&str>, password: Option<&str>) -> StreamRequest {
    let parsed = RtspUrl::parse(url).unwrap();
    let password = password.map(Password::new);
    StreamRequest {
        camera_id: CameraId::generate(),
        url: parsed.url.with_credentials(user, password.as_ref()),
        display_url: parsed.url.to_string(),
    }
}

fn start(
    config: PlayerConfig,
    req: StreamRequest,
) -> (Box<dyn PlaybackHandle>, Receiver<PlayerEvent>) {
    let (tx, rx) = channel();
    let tx = std::sync::Mutex::new(tx);
    let handle = FfmpegPlayer::new(config).start(
        req,
        Box::new(move |e| {
            let _ = tx.lock().unwrap().send(e);
        }),
    );
    (handle, rx)
}

/// Espera um evento que satisfaça `pred`, até `timeout`.
fn wait_for(
    rx: &Receiver<PlayerEvent>,
    timeout: Duration,
    mut pred: impl FnMut(&PlayerEvent) -> bool,
) -> Option<PlayerEvent> {
    let deadline = Instant::now() + timeout;
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        match rx.recv_timeout(left) {
            Ok(e) if pred(&e) => return Some(e),
            Ok(_) => continue,
            Err(_) => return None,
        }
    }
    None
}

fn assert_plays(base: &str, path: &str, codec: &str, config: PlayerConfig) -> DecodeMode {
    let (mut handle, rx) = start(config, request(&format!("{base}/{path}"), None, None));
    let t = Instant::now();
    let info = wait_for(&rx, Duration::from_secs(10), |e| {
        matches!(e, PlayerEvent::Info(_))
    })
    .unwrap_or_else(|| panic!("sem vídeo em /{path}"));
    let PlayerEvent::Info(info) = info else {
        unreachable!()
    };
    assert_eq!(info.codec, codec);
    assert_eq!((info.width, info.height), (1280, 720));
    let frame = wait_for(&rx, Duration::from_secs(5), |e| {
        matches!(e, PlayerEvent::Frame(_))
    })
    .expect("quadro depois do Info");
    let PlayerEvent::Frame(frame) = frame else {
        unreachable!()
    };
    assert_eq!(frame.rgba.len(), 1280 * 720 * 4);
    eprintln!(
        "/{path}: primeira imagem em {:?}, decodificação {}",
        t.elapsed(),
        info.decode
    );
    handle.stop();
    wait_for(&rx, Duration::from_secs(2), |e| {
        *e == PlayerEvent::State(PlayerState::Stopped)
    })
    .expect("parada confirmada");
    info.decode
}

#[test]
fn reproduz_h264() {
    let base = require_camera!();
    assert_plays(&base, "h264", "h264", PlayerConfig::default());
}

#[test]
fn reproduz_h265() {
    let base = require_camera!();
    assert_plays(&base, "h265", "hevc", PlayerConfig::default());
}

#[test]
fn reproduz_por_software_quando_hardware_desligado() {
    let base = require_camera!();
    let config = PlayerConfig {
        hardware_decoding: false,
        ..Default::default()
    };
    assert_eq!(
        assert_plays(&base, "h264", "h264", config),
        DecodeMode::Software
    );
}

#[test]
fn autenticacao_recusada_nao_reconecta() {
    let base = require_camera!();
    let (_handle, rx) = start(
        PlayerConfig::default(),
        request(&format!("{base}/auth"), Some("teste"), Some("errada")),
    );
    let mut events = vec![];
    let failed = wait_for(&rx, Duration::from_secs(10), |e| {
        events.push(e.clone());
        matches!(e, PlayerEvent::State(PlayerState::Failed(_)))
    });
    assert_eq!(
        failed,
        Some(PlayerEvent::State(PlayerState::Failed(
            StreamError::AuthenticationFailed
        ))),
        "eventos: {events:?}"
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, PlayerEvent::State(PlayerState::Reconnecting { .. })))
    );
}

#[test]
fn autenticacao_correta_reproduz() {
    let base = require_camera!();
    let (mut handle, rx) = start(
        PlayerConfig::default(),
        request(&format!("{base}/auth"), Some("teste"), Some("teste123")),
    );
    assert!(
        wait_for(&rx, Duration::from_secs(10), |e| matches!(
            e,
            PlayerEvent::Frame(_)
        ))
        .is_some()
    );
    handle.stop();
}

#[test]
fn parar_durante_conexao_pendente_encerra_em_ate_1s() {
    // Endereço sem rota: a conexão fica pendente até o timeout de socket.
    let (mut handle, rx) = start(
        PlayerConfig::default(),
        request("rtsp://10.255.255.1:554/x", None, None),
    );
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(1)).ok(),
        Some(PlayerEvent::State(PlayerState::Connecting))
    );
    std::thread::sleep(Duration::from_millis(300));
    let t = Instant::now();
    handle.stop();
    let stopped = wait_for(&rx, Duration::from_secs(1), |e| {
        *e == PlayerEvent::State(PlayerState::Stopped)
    });
    assert!(stopped.is_some(), "não parou em 1 s");
    eprintln!("parada em {:?}", t.elapsed());
}

#[test]
fn descartar_o_handle_para_a_reproducao() {
    let (handle, rx) = start(
        PlayerConfig::default(),
        request("rtsp://10.255.255.1:554/x", None, None),
    );
    drop(handle);
    assert!(
        wait_for(&rx, Duration::from_secs(1), |e| *e
            == PlayerEvent::State(PlayerState::Stopped))
        .is_some()
    );
}

#[test]
fn queda_e_retorno_da_camera() {
    let base = require_camera!();
    let Ok(compose) = std::env::var("RTSP_TEST_COMPOSE") else {
        eprintln!("RTSP_TEST_COMPOSE não definida: teste ignorado");
        return;
    };
    let docker = |action: &str| {
        let ok = Command::new("docker")
            .args(["compose", "-f", &compose, action, "pub-queda"])
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        assert!(ok, "docker compose {action} pub-queda falhou");
    };
    docker("start");
    let (mut handle, rx) = start(
        PlayerConfig::default(),
        request(&format!("{base}/queda"), None, None),
    );
    let playing = |rx: &Receiver<PlayerEvent>, secs| {
        wait_for(rx, Duration::from_secs(secs), |e| {
            *e == PlayerEvent::State(PlayerState::Playing)
        })
        .is_some()
    };
    // O publicador pode estar subindo: a primeira reprodução pode passar por
    // reconexões.
    assert!(playing(&rx, 30), "não reproduziu antes da queda");

    docker("stop");
    let reconnecting = wait_for(&rx, Duration::from_secs(15), |e| {
        matches!(e, PlayerEvent::State(PlayerState::Reconnecting { .. }))
    });
    assert!(reconnecting.is_some(), "não indicou reconexão após a queda");

    docker("start");
    assert!(playing(&rx, 45), "não voltou a reproduzir após o retorno");
    handle.stop();
}
