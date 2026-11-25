//! Caso de uso: resolver os streams de uma câmera a partir do endereço
//! (spec `camera-discovery`). ONVIF primeiro; se a câmera não responder ao
//! ONVIF, o usuário escolhe o fabricante (fallback explícito, design D5).

use crate::application::ports::{ResolveError, ResolvedStreams, StreamResolver, StreamTarget};
use crate::domain::{AddressError, DeviceAddress, Password, RtspUrl, Vendor};

/// O que o usuário digitou no campo de endereço.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddressInput {
    /// URL RTSP completa: cadastro como antes, sem consulta.
    Rtsp,
    /// Só o endereço da câmera: consultar.
    Device(DeviceAddress),
}

/// Classifica o texto do campo de endereço.
pub fn classify_address(input: &str) -> Result<AddressInput, AddressError> {
    let t = input.trim();
    if t.to_ascii_lowercase().starts_with("rtsp://")
        || t.to_ascii_lowercase().starts_with("rtsps://")
    {
        return Ok(AddressInput::Rtsp);
    }
    DeviceAddress::parse(t).map(AddressInput::Device)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LookupOutcome {
    Resolved(ResolvedStreams),
    /// A câmera não respondeu ao ONVIF; o usuário escolhe o fabricante.
    NeedsVendor {
        reason: ResolveError,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LookupError {
    /// Credenciais recusadas: não salvar e não oferecer fabricante.
    #[error("Usuário ou senha inválidos.")]
    AuthenticationFailed,
}

/// Consulta a câmera. Falhas de autenticação interrompem; as demais levam à
/// escolha de fabricante.
pub fn lookup(
    resolver: &dyn StreamResolver,
    target: &StreamTarget,
    username: Option<&str>,
    password: Option<&Password>,
) -> Result<LookupOutcome, LookupError> {
    let username = username.map(str::trim).filter(|u| !u.is_empty());
    let password = password.filter(|p| !p.is_empty() && username.is_some());
    match resolver.resolve(target, username, password) {
        Ok(streams) => Ok(LookupOutcome::Resolved(streams)),
        Err(ResolveError::AuthenticationFailed) => Err(LookupError::AuthenticationFailed),
        Err(reason) => Ok(LookupOutcome::NeedsVendor { reason }),
    }
}

/// Streams montados pelo caminho conhecido do fabricante.
pub fn vendor_streams(vendor: Vendor, address: &DeviceAddress) -> (RtspUrl, Option<RtspUrl>) {
    vendor.stream_urls(address)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    struct Fake {
        result: Result<ResolvedStreams, ResolveError>,
        seen: Mutex<Vec<(Option<String>, bool)>>,
    }

    impl StreamResolver for Fake {
        fn resolve(
            &self,
            _: &StreamTarget,
            u: Option<&str>,
            p: Option<&Password>,
        ) -> Result<ResolvedStreams, ResolveError> {
            self.seen
                .lock()
                .unwrap()
                .push((u.map(str::to_string), p.is_some()));
            self.result.clone()
        }
    }

    fn fake(result: Result<ResolvedStreams, ResolveError>) -> Fake {
        Fake {
            result,
            seen: Mutex::new(vec![]),
        }
    }

    fn streams() -> ResolvedStreams {
        ResolvedStreams {
            main: RtspUrl::parse("rtsp://192.168.1.164:554/stream1")
                .unwrap()
                .url,
            sub: Some(
                RtspUrl::parse("rtsp://192.168.1.164:554/stream2")
                    .unwrap()
                    .url,
            ),
            suggested_name: Some("TP-Link C200".into()),
        }
    }

    fn target() -> StreamTarget {
        StreamTarget::Address(DeviceAddress::parse("192.168.1.164").unwrap())
    }

    #[test]
    fn classifica_url_e_endereco() {
        assert_eq!(classify_address("rtsp://h/s"), Ok(AddressInput::Rtsp));
        assert_eq!(classify_address(" RTSPS://h/s"), Ok(AddressInput::Rtsp));
        assert!(matches!(
            classify_address("192.168.1.164"),
            Ok(AddressInput::Device(_))
        ));
        assert!(classify_address("http://x").is_err());
        assert!(classify_address("192.168.1.164:abc").is_err());
    }

    #[test]
    fn onvif_ok() {
        let f = fake(Ok(streams()));
        let out = lookup(&f, &target(), Some(" admin "), Some(&Password::new("x"))).unwrap();
        assert_eq!(out, LookupOutcome::Resolved(streams()));
        assert_eq!(f.seen.lock().unwrap()[0], (Some("admin".into()), true));
    }

    #[test]
    fn autenticacao_recusada_interrompe() {
        let f = fake(Err(ResolveError::AuthenticationFailed));
        assert_eq!(
            lookup(&f, &target(), Some("admin"), Some(&Password::new("x"))),
            Err(LookupError::AuthenticationFailed)
        );
    }

    #[test]
    fn sem_onvif_oferece_fabricante() {
        for reason in [
            ResolveError::NotOnvif,
            ResolveError::NoVideoProfiles,
            ResolveError::Unreachable("tempo".into()),
            ResolveError::Protocol("x".into()),
        ] {
            let f = fake(Err(reason.clone()));
            assert_eq!(
                lookup(&f, &target(), None, None),
                Ok(LookupOutcome::NeedsVendor { reason })
            );
        }
    }

    #[test]
    fn senha_sem_usuario_nao_e_enviada() {
        let f = fake(Ok(streams()));
        lookup(&f, &target(), Some("  "), Some(&Password::new("x"))).unwrap();
        assert_eq!(f.seen.lock().unwrap()[0], (None, false));
    }

    #[test]
    fn fabricante_monta_urls() {
        let a = DeviceAddress::parse("192.168.1.164").unwrap();
        let (m, s) = vendor_streams(Vendor::Hikvision, &a);
        assert_eq!(
            m.to_string(),
            "rtsp://192.168.1.164:554/Streaming/Channels/101"
        );
        assert!(s.is_some());
    }
}
