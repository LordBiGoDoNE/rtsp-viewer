//! Textos e estados exibidos, derivados do domínio. Funções puras, sem Slint,
//! para que o conteúdo da UI seja testável isoladamente (design/telas.md).

use rtsp_viewer_core::application::ports::{PlayerState, RepositoryWarning, StreamInfo};
use rtsp_viewer_core::application::{CatalogError, Field};

/// Estado da visualização, espelho do enum `ViewState` do Slint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Connecting,
    Playing,
    Reconnecting,
    Failed,
}

/// Estado e mensagem exibidos sobre o vídeo. `None` para `Stopped`, que não
/// muda a tela.
pub fn view_for(state: &PlayerState) -> Option<(View, String)> {
    Some(match state {
        PlayerState::Connecting => (View::Connecting, "Conectando…".into()),
        PlayerState::Playing => (View::Playing, String::new()),
        PlayerState::Reconnecting { attempt, .. } => {
            (View::Reconnecting, format!("Reconectando (tentativa {attempt})…"))
        }
        PlayerState::Failed(e) => (View::Failed, e.to_string()),
        PlayerState::Stopped => return None,
    })
}

pub fn info_text(info: &StreamInfo, display_url: &str) -> String {
    format!(
        "Codec: {}\nResolução: {}x{}\nDecodificação: {}\nEndereço: {}",
        info.codec, info.width, info.height, info.decode, display_url
    )
}

pub fn banner_for(warning: &RepositoryWarning) -> String {
    match warning {
        RepositoryWarning::CorruptedFileRecovered { backup } => format!(
            "A lista de câmeras não pôde ser lida e foi guardada em {backup}. Começando com a lista vazia."
        ),
    }
}

/// Erros do formulário distribuídos por campo: (nome, endereço, geral).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct FormErrors {
    pub name: String,
    pub url: String,
    pub general: String,
}

pub fn form_errors(error: &CatalogError) -> FormErrors {
    let mut out = FormErrors::default();
    match error {
        CatalogError::Invalid(fields) => {
            for f in fields {
                match f.field {
                    Field::Name => out.name = f.message.clone(),
                    Field::Url => out.url = f.message.clone(),
                }
            }
        }
        other => out.general = other.to_string(),
    }
    out
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use rtsp_viewer_core::application::FieldError;
    use rtsp_viewer_core::application::ports::{DecodeMode, StreamError};

    use super::*;

    #[test]
    fn textos_dos_estados() {
        assert_eq!(view_for(&PlayerState::Connecting), Some((View::Connecting, "Conectando…".into())));
        assert_eq!(view_for(&PlayerState::Playing), Some((View::Playing, String::new())));
        assert_eq!(
            view_for(&PlayerState::Reconnecting { attempt: 3, delay: Duration::from_secs(4) }),
            Some((View::Reconnecting, "Reconectando (tentativa 3)…".into()))
        );
        assert_eq!(
            view_for(&PlayerState::Failed(StreamError::AuthenticationFailed)),
            Some((View::Failed, "Usuário ou senha inválidos.".into()))
        );
        assert_eq!(view_for(&PlayerState::Stopped), None);
    }

    #[test]
    fn informacoes_da_camera() {
        let info = StreamInfo {
            codec: "h264".into(),
            width: 1920,
            height: 1080,
            decode: DecodeMode::Hardware("d3d11va".into()),
        };
        assert_eq!(
            info_text(&info, "rtsp://h/s"),
            "Codec: h264\nResolução: 1920x1080\nDecodificação: hardware (d3d11va)\nEndereço: rtsp://h/s"
        );
    }

    #[test]
    fn erros_vao_para_o_campo_certo() {
        let e = CatalogError::Invalid(vec![
            FieldError { field: Field::Name, message: "Informe um nome.".into() },
            FieldError { field: Field::Url, message: "URL ruim".into() },
        ]);
        assert_eq!(
            form_errors(&e),
            FormErrors { name: "Informe um nome.".into(), url: "URL ruim".into(), general: String::new() }
        );
        let s = form_errors(&CatalogError::Storage("disco cheio".into()));
        assert!(s.general.contains("disco cheio"));
    }

    #[test]
    fn aviso_de_arquivo_corrompido() {
        let b = banner_for(&RepositoryWarning::CorruptedFileRecovered { backup: "C:/x.corrompido".into() });
        assert!(b.contains("C:/x.corrompido"));
    }
}
