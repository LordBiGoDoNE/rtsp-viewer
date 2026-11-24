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
}

impl Default for PlayerConfig {
    fn default() -> Self {
        Self {
            hardware_decoding: true,
            socket_timeout: Duration::from_secs(5),
        }
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
}

fn run(request: &StreamRequest, sink: &PlayerSink, stop: &Arc<AtomicBool>, config: &PlayerConfig) {
    if let Err(e) = crate::init() {
        sink(PlayerEvent::State(PlayerState::Failed(StreamError::Other(
            format!("FFmpeg não inicializou: {e}"),
        ))));
        return;
    }
    let mut policy = ReconnectPolicy::new();
    sink(PlayerEvent::State(PlayerState::Connecting));
    loop {
        match session(request, sink, stop, config) {
            SessionEnd::Stopped => break,
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
    let device: Option<HwDevice> = if config.hardware_decoding {
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
    let mut decoder = match ctx.decoder().video() {
        Ok(d) => d,
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

    loop {
        if stop.load(Ordering::SeqCst) {
            return SessionEnd::Stopped;
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
        let mut decoded = ff::frame::Video::empty();
        while decoder.receive_frame(&mut decoded).is_ok() {
            let is_hw = device
                .as_ref()
                .is_some_and(|d| decoded.format() == d.pix_fmt);
            let frame = if is_hw {
                let mut sw = ff::frame::Video::empty();
                if hw::transfer(&decoded, &mut sw).is_err() {
                    continue;
                }
                sw
            } else {
                decoded.clone()
            };
            let Some(rgba) = converter.convert(&frame) else {
                continue;
            };
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
}
