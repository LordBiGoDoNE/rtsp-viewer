use std::fmt;

use super::rtsp_url::RtspUrl;

/// Identidade estável de uma câmera (UUID v4 em texto).
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CameraId(String);

impl CameraId {
    pub fn generate() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }

    /// Reconstrói um id lido da persistência. Aceita só UUIDs.
    pub fn parse(value: &str) -> Option<Self> {
        uuid::Uuid::parse_str(value)
            .ok()
            .map(|u| Self(u.hyphenated().to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for CameraId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Nome exibido da câmera: 1 a 60 caracteres, não composto só de espaços.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CameraName(String);

pub const CAMERA_NAME_MAX: usize = 60;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NameError {
    #[error("Informe um nome.")]
    Empty,
    #[error("Use no máximo {CAMERA_NAME_MAX} caracteres.")]
    TooLong,
}

impl CameraName {
    pub fn parse(value: &str) -> Result<Self, NameError> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(NameError::Empty);
        }
        if trimmed.chars().count() > CAMERA_NAME_MAX {
            return Err(NameError::TooLong);
        }
        Ok(Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Stream secundário de outro dispositivo: o host difere do principal.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("O stream secundário precisa usar o mesmo endereço do principal.")]
pub struct SubstreamHostMismatch;

/// Câmera cadastrada. A senha nunca faz parte da entidade: fica no cofre,
/// indexada pelo `CameraId`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Camera {
    id: CameraId,
    name: CameraName,
    url: RtspUrl,
    username: Option<String>,
    /// Stream secundário (menor resolução), com as mesmas credenciais.
    /// Invariante: pertence ao mesmo dispositivo do principal, ou seja, tem o
    /// mesmo host. Trocar a URL principal descarta o secundário (ver
    /// `change_url`).
    substream: Option<RtspUrl>,
}

impl Camera {
    pub fn new(name: CameraName, url: RtspUrl, username: Option<String>) -> Self {
        Self::restore(CameraId::generate(), name, url, username)
    }

    /// Reconstrói uma câmera já persistida.
    pub fn restore(id: CameraId, name: CameraName, url: RtspUrl, username: Option<String>) -> Self {
        Self {
            id,
            name,
            url,
            username: normalize_username(username),
            substream: None,
        }
    }

    pub fn id(&self) -> &CameraId {
        &self.id
    }

    pub fn name(&self) -> &CameraName {
        &self.name
    }

    pub fn url(&self) -> &RtspUrl {
        &self.url
    }

    pub fn username(&self) -> Option<&str> {
        self.username.as_deref()
    }

    pub fn substream(&self) -> Option<&RtspUrl> {
        self.substream.as_ref()
    }

    /// Define (ou remove, com `None`) o stream secundário. Recusa um
    /// secundário com host diferente do principal: as credenciais do
    /// principal não devem ir para outro dispositivo.
    pub fn change_substream(
        &mut self,
        substream: Option<RtspUrl>,
    ) -> Result<(), SubstreamHostMismatch> {
        if let Some(sub) = &substream
            && !same_host(sub, &self.url)
        {
            return Err(SubstreamHostMismatch);
        }
        self.substream = substream;
        Ok(())
    }

    pub fn rename(&mut self, name: CameraName) {
        self.name = name;
    }

    /// Troca a URL principal. Se ela mudar, o secundário é descartado: ele foi
    /// derivado da URL antiga (fabricante, caminho, dispositivo) e não há como
    /// saber se continua válido. Quem tiver um novo secundário o informa
    /// depois, com `change_substream`.
    pub fn change_url(&mut self, url: RtspUrl) {
        if url != self.url {
            self.substream = None;
        }
        self.url = url;
    }

    pub fn change_username(&mut self, username: Option<String>) {
        self.username = normalize_username(username);
    }

    /// Uma câmera com usuário precisa de senha para conectar (mesmo que vazia,
    /// informada pelo usuário).
    pub fn requires_credentials(&self) -> bool {
        self.username.is_some()
    }
}

fn same_host(a: &RtspUrl, b: &RtspUrl) -> bool {
    a.host().eq_ignore_ascii_case(b.host())
}

fn normalize_username(username: Option<String>) -> Option<String> {
    username
        .map(|u| u.trim().to_string())
        .filter(|u| !u.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn url(s: &str) -> RtspUrl {
        RtspUrl::parse(s).unwrap().url
    }

    #[test]
    fn nome_valido_e_aparado() {
        assert_eq!(CameraName::parse("  Garagem ").unwrap().as_str(), "Garagem");
        assert_eq!(
            CameraName::parse(&"á".repeat(60))
                .unwrap()
                .as_str()
                .chars()
                .count(),
            60
        );
    }

    #[test]
    fn nome_invalido() {
        assert_eq!(CameraName::parse(""), Err(NameError::Empty));
        assert_eq!(CameraName::parse(" \t "), Err(NameError::Empty));
        assert_eq!(CameraName::parse(&"x".repeat(61)), Err(NameError::TooLong));
    }

    #[test]
    fn ids_sao_unicos_e_persistiveis() {
        let a = CameraId::generate();
        let b = CameraId::generate();
        assert_ne!(a, b);
        assert_eq!(CameraId::parse(a.as_str()), Some(a));
        assert_eq!(CameraId::parse("nao-e-uuid"), None);
    }

    #[test]
    fn edicao_preserva_identidade() {
        let mut c = Camera::new(CameraName::parse("A").unwrap(), url("rtsp://h/s"), None);
        let id = c.id().clone();
        c.rename(CameraName::parse("B").unwrap());
        c.change_url(url("rtsp://h2/s"));
        c.change_username(Some("  admin ".into()));
        assert_eq!(c.id(), &id);
        assert_eq!(c.name().as_str(), "B");
        assert_eq!(c.url().to_string(), "rtsp://h2/s");
        assert_eq!(c.username(), Some("admin"));
        assert!(c.requires_credentials());
    }

    fn cam_com_secundario() -> Camera {
        let mut c = Camera::new(
            CameraName::parse("A").unwrap(),
            url("rtsp://192.168.1.164:554/stream1"),
            None,
        );
        c.change_substream(Some(url("rtsp://192.168.1.164:554/stream2")))
            .unwrap();
        c
    }

    #[test]
    fn secundario_de_outro_host_e_recusado() {
        let mut c = cam_com_secundario();
        assert_eq!(
            c.change_substream(Some(url("rtsp://192.168.1.121/stream2"))),
            Err(SubstreamHostMismatch)
        );
        assert_eq!(
            c.substream().unwrap().to_string(),
            "rtsp://192.168.1.164:554/stream2"
        );
        assert!(c.change_substream(None).is_ok());
        assert!(c.substream().is_none());
    }

    #[test]
    fn host_e_comparado_sem_diferenciar_maiusculas() {
        let mut c = Camera::new(
            CameraName::parse("A").unwrap(),
            url("rtsp://Cam.local/s1"),
            None,
        );
        assert!(c.change_substream(Some(url("rtsp://cam.LOCAL/s2"))).is_ok());
    }

    #[test]
    fn trocar_url_principal_descarta_secundario() {
        // Mesmo host, outro caminho.
        let mut c = cam_com_secundario();
        c.change_url(url("rtsp://192.168.1.164:554/h264"));
        assert!(c.substream().is_none());
        // Outro host.
        let mut c = cam_com_secundario();
        c.change_url(url("rtsp://192.168.1.121:554/stream1"));
        assert!(c.substream().is_none());
    }

    #[test]
    fn mesma_url_principal_mantem_secundario() {
        let mut c = cam_com_secundario();
        c.change_url(url("rtsp://192.168.1.164:554/stream1"));
        assert!(c.substream().is_some());
    }

    #[test]
    fn usuario_vazio_vira_ausente() {
        let c = Camera::new(
            CameraName::parse("A").unwrap(),
            url("rtsp://h"),
            Some("  ".into()),
        );
        assert_eq!(c.username(), None);
        assert!(!c.requires_credentials());
    }
}
