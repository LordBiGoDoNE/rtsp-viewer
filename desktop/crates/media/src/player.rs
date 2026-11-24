//! Player RTSP com FFmpeg: uma thread por stream, baixa latência, reconexão
//! com espera crescente e entrega do quadro mais recente em RGBA.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use ffmpeg_next as ff;
use rtsp_viewer_core::application::ports::{
    DecodeMode, PlaybackHandle, PlayerEvent, PlayerSink, PlayerState, StreamError, StreamInfo,
    StreamPlayer, StreamRequest, VideoFrame,
};
use rtsp_viewer_core::domain::{ReconnectPolicy, redact_credentials};

use crate::hw::{self, HwDevice};

#[derive(Debug, Clone)]
pub struct PlayerConfig {
    /// Tenta decodificar por hardware antes do software.
    pub hardware_decoding: bool,
    /// Timeout de socket (conexão e leitura).
    pub socket_timeout: Duration,
    /// Falha de hardware simulada, só para testes.
    #[cfg(test)]
    fault: Option<HwFault>,
}

impl Default for PlayerConfig {
    fn default() -> Self {
        Self {
            hardware_decoding: true,
            socket_timeout: Duration::from_secs(5),
            #[cfg(test)]
            fault: None,
        }
    }
}

/// Pontos do caminho de hardware em que os testes simulam falha.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HwFault {
    /// Abertura do decodificador com o dispositivo ligado.
    Open,
    /// Cópia do quadro da GPU para a memória.
    Transfer,
}

/// Indica se os testes pediram a falha `which`; fora dos testes, nunca.
fn injected(config: &PlayerConfig, which: HwFault) -> bool {
    #[cfg(test)]
    {
        config.fault == Some(which)
    }
    #[cfg(not(test))]
    {
        let _ = (config, which);
        false
    }
}

/// Falhas seguidas de cópia da GPU que fazem a câmera passar para software.
const MAX_TRANSFER_FAILURES: u32 = 10;
/// Tempo máximo, desde o primeiro quadro-chave aceito pelo decodificador,
/// para o hardware entregar o primeiro quadro antes de passar para software.
/// Conta a partir do quadro-chave, e não do primeiro pacote, porque antes
/// dele nenhum decodificador entrega quadro: câmeras com GOP longo ("smart
/// codec") não podem ser confundidas com falha de hardware.
const HW_FIRST_FRAME_TIMEOUT: Duration = Duration::from_secs(8);

/// Decide quando o caminho de hardware de uma sessão falhou. Não depende de
/// FFmpeg nem do relógio: o laço da sessão informa os eventos e o instante.
#[derive(Debug, Default)]
struct HwWatchdog {
    /// Primeiro quadro-chave aceito pelo decodificador.
    keyframe_at: Option<Instant>,
    /// Algum quadro já foi entregue.
    frame_seen: bool,
    /// Cópias da GPU que falharam em sequência.
    transfer_failures: u32,
}

impl HwWatchdog {
    /// O decodificador aceitou um pacote de quadro-chave.
    fn on_keyframe_sent(&mut self, now: Instant) {
        self.keyframe_at.get_or_insert(now);
    }

    /// Um quadro decodificado foi entregue.
    fn on_frame(&mut self) {
        self.frame_seen = true;
    }

    fn on_transfer_ok(&mut self) {
        self.transfer_failures = 0;
    }

    fn on_transfer_failed(&mut self) {
        self.transfer_failures += 1;
    }

    /// Verdadeiro quando a sessão deve ser reaberta por software.
    fn should_fall_back(&self, now: Instant) -> bool {
        if self.transfer_failures >= MAX_TRANSFER_FAILURES {
            return true;
        }
        !self.frame_seen
            && self
                .keyframe_at
                .is_some_and(|t| now.saturating_duration_since(t) > HW_FIRST_FRAME_TIMEOUT)
    }
}

#[derive(Debug, Clone, Default)]
pub struct FfmpegPlayer {
    config: PlayerConfig,
}

impl FfmpegPlayer {
    pub fn new(config: PlayerConfig) -> Self {
        Self { config }
    }
}

impl StreamPlayer for FfmpegPlayer {
    fn start(&self, request: StreamRequest, sink: PlayerSink) -> Box<dyn PlaybackHandle> {
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let config = self.config.clone();
        let sink = Arc::new(sink);
        let thread_sink = sink.clone();
        let spawned = thread::Builder::new()
            .name(format!("player-{}", request.camera_id))
            .spawn(move || run(&request, &thread_sink, &thread_stop, &config));
        if let Err(e) = spawned {
            sink(PlayerEvent::State(PlayerState::Failed(StreamError::Other(
                format!("não foi possível iniciar a reprodução: {e}"),
            ))));
        }
        Box::new(Handle { stop })
    }

    /// Cria o dispositivo de hardware numa thread, uma vez por processo, para
    /// que a primeira câmera não pague esse custo (design.md, D3). Não roda no
    /// boot: o dispositivo ocupa memória que o usuário só deve pagar quando
    /// mostra intenção de abrir uma câmera.
    fn prepare(&self) {
        static STARTED: AtomicBool = AtomicBool::new(false);
        if !self.config.hardware_decoding || STARTED.swap(true, Ordering::SeqCst) {
            return;
        }
        let _ = thread::Builder::new().name("hw-warm-up".into()).spawn(|| {
            if crate::init().is_ok() {
                hw::warm_up();
            }
        });
    }
}

struct Handle {
    stop: Arc<AtomicBool>,
}

impl PlaybackHandle for Handle {
    /// Sinaliza a parada sem bloquear: a thread sai na próxima checagem ou
    /// quando o FFmpeg consultar o callback de interrupção.
    fn stop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

/// Como uma sessão de conexão terminou.
enum SessionEnd {
    Stopped,
    /// Fim do stream ou erro, com o tempo de reprodução efetiva.
    Lost {
        error: StreamError,
        played_for: Duration,
    },
    /// O caminho de hardware falhou; a sessão deve ser reaberta por software.
    HardwareFailed,
}

fn run(request: &StreamRequest, sink: &PlayerSink, stop: &Arc<AtomicBool>, config: &PlayerConfig) {
    if let Err(e) = crate::init() {
        sink(PlayerEvent::State(PlayerState::Failed(StreamError::Other(
            format!("FFmpeg não inicializou: {e}"),
        ))));
        return;
    }
    let mut policy = ReconnectPolicy::new();
    // Decidido por câmera: depois de uma falha de hardware, o resto da
    // reprodução desta câmera usa software.
    let mut hardware = config.hardware_decoding;
    sink(PlayerEvent::State(PlayerState::Connecting));
    loop {
        match session(request, sink, stop, config, hardware) {
            SessionEnd::Stopped => break,
            SessionEnd::HardwareFailed => {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                // Reabre na hora: não é falha de rede e só acontece uma vez.
                hardware = false;
            }
            SessionEnd::Lost { error, played_for } => {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                if !error.is_retryable() {
                    sink(PlayerEvent::State(PlayerState::Failed(error)));
                    return;
                }
                policy.record_playback(played_for);
                let (attempt, delay) = policy.next_delay();
                sink(PlayerEvent::State(PlayerState::Reconnecting {
                    attempt,
                    delay,
                }));
                if !sleep_unless_stopped(delay, stop) {
                    break;
                }
            }
        }
    }
    sink(PlayerEvent::State(PlayerState::Stopped));
}

/// Dorme em fatias curtas; devolve `false` se a parada foi pedida.
fn sleep_unless_stopped(total: Duration, stop: &AtomicBool) -> bool {
    let deadline = Instant::now() + total;
    while Instant::now() < deadline {
        if stop.load(Ordering::SeqCst) {
            return false;
        }
        thread::sleep(Duration::from_millis(50));
    }
    !stop.load(Ordering::SeqCst)
}

fn open_options(config: &PlayerConfig) -> ff::Dictionary<'static> {
    let mut opts = ff::Dictionary::new();
    opts.set("rtsp_transport", "tcp");
    opts.set("fflags", "nobuffer");
    opts.set("flags", "low_delay");
    opts.set("probesize", "262144");
    opts.set("analyzeduration", "300000");
    opts.set("timeout", &config.socket_timeout.as_micros().to_string());
    opts
}

fn session(
    request: &StreamRequest,
    sink: &PlayerSink,
    stop: &Arc<AtomicBool>,
    config: &PlayerConfig,
    hardware: bool,
) -> SessionEnd {
    let lost = |error| SessionEnd::Lost {
        error,
        played_for: Duration::ZERO,
    };
    let interrupt_stop = stop.clone();
    let opened = ff::format::input_with_interrupt_and_dictionary(
        request.url.expose(),
        move || interrupt_stop.load(Ordering::SeqCst),
        open_options(config),
    );
    let mut input = match opened {
        Ok(i) => i,
        Err(_) if stop.load(Ordering::SeqCst) => return SessionEnd::Stopped,
        Err(e) => return lost(classify(&e)),
    };

    let Some(stream) = input.streams().best(ff::media::Type::Video) else {
        return lost(StreamError::NoVideo);
    };
    let stream_index = stream.index();
    let parameters = stream.parameters();
    let codec_id = parameters.id();
    if ff::decoder::find(codec_id).is_none() {
        return lost(StreamError::UnsupportedCodec(codec_name(codec_id)));
    }
    let mut ctx = match ff::codec::context::Context::from_parameters(parameters) {
        Ok(c) => c,
        Err(e) => return lost(StreamError::Other(redact(&e))),
    };
    let device: Option<HwDevice> = if hardware {
        hw::attach(&mut ctx, codec_id)
    } else {
        None
    };
    // Threads por fatia não acrescentam quadros de atraso, ao contrário das
    // threads por quadro.
    ctx.set_threading(ff::codec::threading::Config::kind(
        ff::codec::threading::Type::Slice,
    ));
    ctx.set_flags(ff::codec::Flags::LOW_DELAY);
    let opened = if device.is_some() && injected(config, HwFault::Open) {
        Err(ff::Error::Bug)
    } else {
        ctx.decoder().video()
    };
    let mut decoder = match opened {
        Ok(d) => d,
        // Com o dispositivo ligado, a falha pode ser do hardware: tenta de
        // novo por software antes de dar o codec como não suportado.
        Err(_) if device.is_some() => return SessionEnd::HardwareFailed,
        Err(e) => {
            return lost(StreamError::UnsupportedCodec(format!(
                "{} ({})",
                codec_name(codec_id),
                redact(&e)
            )));
        }
    };

    let mut converter = Converter::default();
    let mut playing_since: Option<Instant> = None;
    let played = |since: Option<Instant>| since.map(|s| s.elapsed()).unwrap_or_default();
    // Vigilância do hardware: sem quadro dentro do prazo ou com cópias da GPU
    // falhando em sequência, a sessão volta para software.
    let mut watchdog = HwWatchdog::default();

    loop {
        if stop.load(Ordering::SeqCst) {
            return SessionEnd::Stopped;
        }
        if device.is_some() && watchdog.should_fall_back(Instant::now()) {
            return SessionEnd::HardwareFailed;
        }
        let mut packet = ff::Packet::empty();
        match packet.read(&mut input) {
            Ok(()) => {}
            Err(ff::Error::Exit) => return SessionEnd::Stopped,
            Err(ff::Error::Eof) => {
                return SessionEnd::Lost {
                    error: StreamError::Unreachable("a câmera encerrou o stream".into()),
                    played_for: played(playing_since),
                };
            }
            Err(e) => {
                if stop.load(Ordering::SeqCst) {
                    return SessionEnd::Stopped;
                }
                return SessionEnd::Lost {
                    error: classify(&e),
                    played_for: played(playing_since),
                };
            }
        }
        if packet.stream() != stream_index {
            continue;
        }
        // Pacote corrompido não derruba a sessão: o decodificador se recupera
        // no próximo quadro-chave.
        if decoder.send_packet(&packet).is_err() {
            continue;
        }
        if packet.is_key() {
            watchdog.on_keyframe_sent(Instant::now());
        }
        let mut decoded = ff::frame::Video::empty();
        while decoder.receive_frame(&mut decoded).is_ok() {
            let is_hw = device
                .as_ref()
                .is_some_and(|d| decoded.format() == d.pix_fmt);
            let frame = if is_hw {
                let mut sw = ff::frame::Video::empty();
                let copied = if injected(config, HwFault::Transfer) {
                    Err(ff::Error::Bug)
                } else {
                    hw::transfer(&decoded, &mut sw)
                };
                if copied.is_err() {
                    watchdog.on_transfer_failed();
                    if watchdog.should_fall_back(Instant::now()) {
                        return SessionEnd::HardwareFailed;
                    }
                    continue;
                }
                watchdog.on_transfer_ok();
                sw
            } else {
                decoded.clone()
            };
            let Some(rgba) = converter.convert(&frame) else {
                continue;
            };
            watchdog.on_frame();
            if playing_since.is_none() {
                playing_since = Some(Instant::now());
                sink(PlayerEvent::Info(StreamInfo {
                    codec: codec_name(codec_id),
                    width: rgba.width,
                    height: rgba.height,
                    decode: match (&device, is_hw) {
                        (Some(d), true) => DecodeMode::Hardware(d.api.to_string()),
                        _ => DecodeMode::Software,
                    },
                }));
                sink(PlayerEvent::State(PlayerState::Playing));
            }
            sink(PlayerEvent::Frame(rgba));
        }
    }
}

/// Conversão para RGBA com `swscale`, reaproveitando o contexto enquanto
/// formato e tamanho não mudam.
#[derive(Default)]
struct Converter {
    scaler: Option<(ff::software::scaling::Context, ff::format::Pixel, u32, u32)>,
}

impl Converter {
    fn convert(&mut self, frame: &ff::frame::Video) -> Option<VideoFrame> {
        let (fmt, w, h) = (frame.format(), frame.width(), frame.height());
        if w == 0 || h == 0 {
            return None;
        }
        let reuse =
            matches!(&self.scaler, Some((_, f, sw, sh)) if *f == fmt && *sw == w && *sh == h);
        if !reuse {
            let ctx = ff::software::scaling::Context::get(
                fmt,
                w,
                h,
                ff::format::Pixel::RGBA,
                w,
                h,
                ff::software::scaling::Flags::BILINEAR,
            )
            .ok()?;
            self.scaler = Some((ctx, fmt, w, h));
        }
        let (scaler, ..) = self.scaler.as_mut()?;
        let mut out = ff::frame::Video::empty();
        scaler.run(frame, &mut out).ok()?;

        let stride = out.stride(0);
        let row = w as usize * 4;
        let data = out.data(0);
        let mut rgba = Vec::with_capacity(row * h as usize);
        for y in 0..h as usize {
            rgba.extend_from_slice(&data[y * stride..y * stride + row]);
        }
        Some(VideoFrame {
            width: w,
            height: h,
            rgba,
        })
    }
}

fn codec_name(id: ff::codec::Id) -> String {
    ff::decoder::find(id)
        .map(|c| c.name().to_string())
        .unwrap_or_else(|| format!("{id:?}").to_lowercase())
}

fn redact(e: &ff::Error) -> String {
    redact_credentials(&e.to_string())
}

/// Traduz erros do FFmpeg para erros do domínio. Mensagens passam pela
/// redação de credenciais.
fn classify(e: &ff::Error) -> StreamError {
    match e {
        ff::Error::HttpUnauthorized | ff::Error::HttpForbidden => StreamError::AuthenticationFailed,
        ff::Error::HttpNotFound => {
            StreamError::Unreachable("caminho não encontrado na câmera (404)".into())
        }
        ff::Error::StreamNotFound => StreamError::NoVideo,
        ff::Error::DecoderNotFound => StreamError::UnsupportedCodec("desconhecido".into()),
        other => StreamError::Unreachable(redact(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifica_erros_de_autenticacao() {
        assert_eq!(
            classify(&ff::Error::HttpUnauthorized),
            StreamError::AuthenticationFailed
        );
        assert_eq!(
            classify(&ff::Error::HttpForbidden),
            StreamError::AuthenticationFailed
        );
        assert!(classify(&ff::Error::HttpNotFound).is_retryable());
        assert!(classify(&ff::Error::Other { errno: 10061 }).is_retryable());
    }

    #[test]
    fn opcoes_de_baixa_latencia() {
        let opts = open_options(&PlayerConfig::default());
        assert_eq!(opts.get("rtsp_transport"), Some("tcp"));
        assert_eq!(opts.get("fflags"), Some("nobuffer"));
        assert_eq!(opts.get("flags"), Some("low_delay"));
        assert_eq!(opts.get("timeout"), Some("5000000"));
    }

    #[test]
    fn espera_interrompida_pela_parada() {
        let stop = AtomicBool::new(true);
        let t = Instant::now();
        assert!(!sleep_unless_stopped(Duration::from_secs(5), &stop));
        assert!(t.elapsed() < Duration::from_millis(200));
    }

    // Decisão de fallback, sem FFmpeg nem relógio real.

    const SEG: Duration = Duration::from_secs(1);

    #[test]
    fn sem_quadro_chave_o_prazo_nao_corre() {
        // GOP longo: pacotes chegam, mas sem quadro-chave nada é decodificável.
        let w = HwWatchdog::default();
        let t0 = Instant::now();
        assert!(!w.should_fall_back(t0 + 60 * SEG));
    }

    #[test]
    fn prazo_conta_a_partir_do_quadro_chave() {
        let mut w = HwWatchdog::default();
        let t0 = Instant::now();
        // Quadro-chave chega 20 s depois do primeiro pacote (GOP longo).
        let key = t0 + 20 * SEG;
        w.on_keyframe_sent(key);
        assert!(!w.should_fall_back(key + HW_FIRST_FRAME_TIMEOUT));
        assert!(w.should_fall_back(key + HW_FIRST_FRAME_TIMEOUT + SEG));
    }

    #[test]
    fn quadros_chave_seguintes_nao_reiniciam_o_prazo() {
        let mut w = HwWatchdog::default();
        let t0 = Instant::now();
        w.on_keyframe_sent(t0);
        w.on_keyframe_sent(t0 + 5 * SEG);
        assert!(w.should_fall_back(t0 + HW_FIRST_FRAME_TIMEOUT + SEG));
    }

    #[test]
    fn quadro_entregue_cancela_o_prazo() {
        let mut w = HwWatchdog::default();
        let t0 = Instant::now();
        w.on_keyframe_sent(t0);
        w.on_frame();
        assert!(!w.should_fall_back(t0 + 60 * SEG));
    }

    #[test]
    fn falhas_seguidas_de_copia_caem_para_software() {
        let mut w = HwWatchdog::default();
        let now = Instant::now();
        for _ in 1..MAX_TRANSFER_FAILURES {
            w.on_transfer_failed();
            assert!(!w.should_fall_back(now));
        }
        w.on_transfer_failed();
        assert!(w.should_fall_back(now));
    }

    #[test]
    fn copia_bem_sucedida_zera_as_falhas() {
        let mut w = HwWatchdog::default();
        let now = Instant::now();
        w.on_frame();
        for _ in 0..3 {
            for _ in 1..MAX_TRANSFER_FAILURES {
                w.on_transfer_failed();
            }
            w.on_transfer_ok();
        }
        assert!(!w.should_fall_back(now));
    }

    // Fallback para software com a câmera falsa (`RTSP_TEST_BASE`, veja
    // `tests/fake_camera.rs`). Ficam aqui porque a falha simulada só existe
    // nos testes do crate.

    fn camera_base() -> Option<String> {
        let base = std::env::var("RTSP_TEST_BASE").ok()?;
        crate::init().unwrap();
        hw::warm_up();
        Some(base.trim_end_matches('/').to_string())
    }

    /// Reproduz `/h264` e devolve o modo de decodificação informado e os
    /// eventos de estado vistos até o primeiro quadro.
    fn decode_mode(base: &str, config: PlayerConfig) -> (DecodeMode, Vec<PlayerState>) {
        use rtsp_viewer_core::domain::{CameraId, RtspUrl};
        use std::sync::{Mutex, mpsc};

        let url = RtspUrl::parse(&format!("{base}/h264")).unwrap().url;
        let request = StreamRequest {
            camera_id: CameraId::generate(),
            url: url.with_credentials(None, None),
            display_url: url.to_string(),
        };
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let mut handle = FfmpegPlayer::new(config).start(
            request,
            Box::new(move |e| {
                let _ = tx.lock().unwrap().send(e);
            }),
        );
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut states = vec![];
        let mut info = None;
        while let Some(left) = deadline.checked_duration_since(Instant::now()) {
            match rx.recv_timeout(left) {
                Ok(PlayerEvent::Info(i)) => info = Some(i),
                Ok(PlayerEvent::State(s)) => states.push(s),
                Ok(PlayerEvent::Frame(f)) => {
                    assert_eq!(f.rgba.len(), (f.width * f.height * 4) as usize);
                    break;
                }
                Err(_) => break,
            }
        }
        handle.stop();
        let info = info.unwrap_or_else(|| panic!("sem vídeo; estados: {states:?}"));
        (info.decode, states)
    }

    fn assert_falls_back(fault: HwFault) {
        let Some(base) = camera_base() else {
            eprintln!("RTSP_TEST_BASE não definida: teste ignorado");
            return;
        };
        let (normal, _) = decode_mode(&base, PlayerConfig::default());
        if normal == DecodeMode::Software {
            eprintln!("sem decodificação por hardware nesta máquina: fallback não exercitado");
            return;
        }
        let config = PlayerConfig {
            fault: Some(fault),
            ..Default::default()
        };
        let (mode, states) = decode_mode(&base, config);
        assert_eq!(mode, DecodeMode::Software, "estados: {states:?}");
        // A troca é transparente: sem erro nem reconexão para o usuário.
        assert!(
            states
                .iter()
                .all(|s| matches!(s, PlayerState::Connecting | PlayerState::Playing)),
            "estados: {states:?}"
        );
    }

    #[test]
    fn falha_ao_abrir_decodificador_de_hardware_cai_para_software() {
        assert_falls_back(HwFault::Open);
    }

    #[test]
    fn falhas_seguidas_de_copia_da_gpu_caem_para_software() {
        assert_falls_back(HwFault::Transfer);
    }
}
