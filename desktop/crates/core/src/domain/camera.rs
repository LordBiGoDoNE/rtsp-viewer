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

/// Câmera cadastrada. A senha nunca faz parte da entidade: fica no cofre,
/// indexada pelo `CameraId`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Camera {
    id: CameraId,
    name: CameraName,
    url: RtspUrl,
    username: Option<String>,
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

    pub fn rename(&mut self, name: CameraName) {
        self.name = name;
    }

    pub fn change_url(&mut self, url: RtspUrl) {
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
