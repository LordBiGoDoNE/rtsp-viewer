//! Textos e estados exibidos, derivados do domínio. Funções puras, sem Slint,
//! para que o conteúdo da UI seja testável isoladamente (design/telas.md).

use rtsp_viewer_core::application::ports::{
    DiscoveryFailure, PlayerState, RepositoryWarning, ResolveError, StreamInfo,
};
use rtsp_viewer_core::application::{CatalogError, Field};
use rtsp_viewer_core::domain::{CAMERA_NAME_MAX, DeviceAddress};

/// Edição com usuário e sem senha digitada: a consulta ONVIF precisa dela.
pub const LOOKUP_NEEDS_PASSWORD: &str = "Informe a senha para consultar a câmera.";

/// Nome de uma câmera encontrada na busca.
pub fn found_name(suggested: Option<&str>) -> String {
    suggested
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .unwrap_or("Câmera ONVIF")
        .to_string()
}

/// Nome de uma câmera cadastrada só com o endereço, quando o usuário deixou o
/// nome vazio e a câmera não informou um.
pub fn fallback_name(address: &DeviceAddress) -> String {
    format!("Câmera {address}")
        .chars()
        .take(CAMERA_NAME_MAX)
        .collect()
}

/// Motivo exibido na escolha de fabricante; vazio quando o motivo é a própria
/// falta de ONVIF, que o texto da tela já diz.
pub fn vendor_reason(reason: &ResolveError) -> String {
    match reason {
        ResolveError::NotOnvif => String::new(),
        other => format!("Motivo: {other}"),
    }
}

pub fn discovery_error(error: &DiscoveryFailure) -> String {
    format!("Não foi possível procurar câmeras: {error}")
}

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
        PlayerState::Reconnecting { attempt, .. } => (
            View::Reconnecting,
            format!("Reconectando (tentativa {attempt})…"),
        ),
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
        assert_eq!(
            view_for(&PlayerState::Connecting),
            Some((View::Connecting, "Conectando…".into()))
        );
        assert_eq!(
            view_for(&PlayerState::Playing),
            Some((View::Playing, String::new()))
        );
        assert_eq!(
            view_for(&PlayerState::Reconnecting {
                attempt: 3,
                delay: Duration::from_secs(4)
            }),
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
            FieldError {
                field: Field::Name,
                message: "Informe um nome.".into(),
            },
            FieldError {
                field: Field::Url,
                message: "URL ruim".into(),
            },
        ]);
        assert_eq!(
            form_errors(&e),
            FormErrors {
                name: "Informe um nome.".into(),
                url: "URL ruim".into(),
                general: String::new()
            }
        );
        let s = form_errors(&CatalogError::Storage("disco cheio".into()));
        assert!(s.general.contains("disco cheio"));
    }

    #[test]
    fn textos_da_busca_e_da_consulta() {
        assert_eq!(found_name(Some(" TP-Link C200 ")), "TP-Link C200");
        assert_eq!(found_name(Some("  ")), "Câmera ONVIF");
        assert_eq!(found_name(None), "Câmera ONVIF");
        assert_eq!(
            fallback_name(&DeviceAddress::parse("192.168.1.120").unwrap()),
            "Câmera 192.168.1.120"
        );
        let long = format!("{}.local", "a".repeat(80));
        assert_eq!(
            fallback_name(&DeviceAddress::parse(&long).unwrap())
                .chars()
                .count(),
            CAMERA_NAME_MAX
        );
        assert_eq!(vendor_reason(&ResolveError::NotOnvif), "");
        assert_eq!(
            vendor_reason(&ResolveError::Unreachable("tempo esgotado".into())),
            "Motivo: A câmera não respondeu: tempo esgotado"
        );
        assert_eq!(
            discovery_error(&DiscoveryFailure("sem interface".into())),
            "Não foi possível procurar câmeras: sem interface"
        );
    }

    #[test]
    fn aviso_de_arquivo_corrompido() {
        let b = banner_for(&RepositoryWarning::CorruptedFileRecovered {
            backup: "C:/x.corrompido".into(),
        });
        assert!(b.contains("C:/x.corrompido"));
    }
}
