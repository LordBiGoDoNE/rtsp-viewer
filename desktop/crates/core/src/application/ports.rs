//! Portas implementadas pelos adaptadores (`media` e `app`).

use std::fmt;
use std::time::Duration;

use crate::domain::{Camera, CameraId, ConnectionUrl, Password};

// ---------------------------------------------------------------- persistência

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct RepositoryError(pub String);

/// Aviso não fatal da leitura da lista.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepositoryWarning {
    /// O arquivo não pôde ser lido; foi preservado em `backup` e a lista
    /// começou vazia.
    CorruptedFileRecovered { backup: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LoadedCameras {
    pub cameras: Vec<Camera>,
    pub warning: Option<RepositoryWarning>,
}

pub trait CameraRepository {
    fn load(&self) -> Result<LoadedCameras, RepositoryError>;
    /// Grava a lista inteira de forma atômica.
    fn save(&self, cameras: &[Camera]) -> Result<(), RepositoryError>;
}

// ---------------------------------------------------------------- credenciais

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct CredentialError(pub String);

pub trait CredentialStore {
    fn get(&self, id: &CameraId) -> Result<Option<Password>, CredentialError>;
    fn set(&self, id: &CameraId, password: &Password) -> Result<(), CredentialError>;
    /// Remove a senha; não é erro se ela não existir.
    fn delete(&self, id: &CameraId) -> Result<(), CredentialError>;
    /// `false` quando as senhas ficam só em memória até o app fechar.
    fn is_persistent(&self) -> bool;
}

// ---------------------------------------------------------------- reprodução

/// Pedido de reprodução. A URL com credenciais existe só aqui, em memória.
#[derive(Debug, Clone)]
pub struct StreamRequest {
    pub camera_id: CameraId,
    pub url: ConnectionUrl,
    /// URL sem credenciais, para logs e mensagens.
    pub display_url: String,
}

/// Quadro decodificado em RGBA8, linhas contíguas (`width * 4` bytes cada).
#[derive(Clone, PartialEq, Eq)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl fmt::Debug for VideoFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VideoFrame({}x{})", self.width, self.height)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeMode {
    /// Nome da API de aceleração (por exemplo `d3d11va`, `vaapi`).
    Hardware(String),
    Software,
}

impl fmt::Display for DecodeMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeMode::Hardware(api) => write!(f, "hardware ({api})"),
            DecodeMode::Software => f.write_str("software"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StreamInfo {
    pub codec: String,
    pub width: u32,
    pub height: u32,
    pub decode: DecodeMode,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum StreamError {
    #[error("Usuário ou senha inválidos.")]
    AuthenticationFailed,
    #[error("Câmera inacessível: {0}")]
    Unreachable(String),
    #[error("Formato de vídeo não suportado: {0}")]
    UnsupportedCodec(String),
    #[error("A câmera não envia vídeo.")]
    NoVideo,
    #[error("Falha no stream: {0}")]
    Other(String),
}

impl StreamError {
    /// Erros que não se resolvem tentando de novo exigem ação do usuário.
    pub fn is_retryable(&self) -> bool {
        matches!(self, StreamError::Unreachable(_) | StreamError::Other(_))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayerState {
    Connecting,
    Playing,
    Reconnecting { attempt: u32, delay: Duration },
    Failed(StreamError),
    Stopped,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlayerEvent {
    State(PlayerState),
    Info(StreamInfo),
    Frame(VideoFrame),
}

/// Recebe os eventos do player, a partir da thread de reprodução.
pub type PlayerSink = Box<dyn Fn(PlayerEvent) + Send + 'static>;

/// Reprodução em andamento. Parar (ou descartar o handle) encerra a conexão.
pub trait PlaybackHandle: Send {
    fn stop(&mut self);
}

pub trait StreamPlayer {
    fn start(&self, request: StreamRequest, sink: PlayerSink) -> Box<dyn PlaybackHandle>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn so_erros_transitorios_reconectam() {
        assert!(StreamError::Unreachable("x".into()).is_retryable());
        assert!(StreamError::Other("x".into()).is_retryable());
        assert!(!StreamError::AuthenticationFailed.is_retryable());
        assert!(!StreamError::UnsupportedCodec("mjpeg".into()).is_retryable());
        assert!(!StreamError::NoVideo.is_retryable());
    }

    #[test]
    fn mensagens_em_portugues() {
        assert_eq!(
            StreamError::AuthenticationFailed.to_string(),
            "Usuário ou senha inválidos."
        );
        assert_eq!(
            StreamError::UnsupportedCodec("mjpeg".into()).to_string(),
            "Formato de vídeo não suportado: mjpeg"
        );
    }

    #[test]
    fn debug_do_quadro_nao_despeja_pixels() {
        let f = VideoFrame {
            width: 2,
            height: 1,
            rgba: vec![0; 8],
        };
        assert_eq!(format!("{f:?}"), "VideoFrame(2x1)");
    }
}
