//! Adaptadores das portas `StreamResolver` e `CameraDiscovery` com o crate
//! `onvif` (spec `descoberta-onvif`). Bloqueiam: o app os chama fora da
//! thread da UI.

use rtsp_viewer_core::application::ports::{
    CameraDiscovery, DiscoveredCamera, DiscoveryFailure, ResolveError, ResolvedStreams,
    StreamResolver, StreamTarget,
};
use rtsp_viewer_core::domain::Password;
use rtsp_viewer_onvif::{
    Credentials, DeviceService, DiscoveryConfig, OnvifClient, OnvifError, StreamSet, discover,
};

#[derive(Default)]
pub struct OnvifStreamResolver {
    client: OnvifClient,
}

impl StreamResolver for OnvifStreamResolver {
    fn resolve(
        &self,
        target: &StreamTarget,
        username: Option<&str>,
        password: Option<&Password>,
    ) -> Result<ResolvedStreams, ResolveError> {
        let device = match target {
            StreamTarget::Address(a) => {
                self.client.locate(a.host(), a.port()).map_err(map_error)?
            }
            StreamTarget::ServiceUrl(url) => {
                DeviceService::from_url(url).ok_or(ResolveError::NotOnvif)?
            }
        };
        let creds = username.map(|u| Credentials {
            username: u.to_string(),
            password: password.cloned().unwrap_or_else(|| Password::new("")),
        });
        let set = self
            .client
            .fetch_streams(&device, creds.as_ref())
            .map_err(map_error)?;
        Ok(to_resolved(set))
    }
}

fn to_resolved(set: StreamSet) -> ResolvedStreams {
    let suggested_name = match (set.manufacturer, set.model) {
        (Some(m), Some(model)) if model.to_lowercase().contains(&m.to_lowercase()) => Some(model),
        (Some(m), Some(model)) => Some(format!("{m} {model}")),
        (m, model) => model.or(m),
    }
    .map(|n| n.chars().take(60).collect::<String>().trim().to_string())
    .filter(|n| !n.is_empty());
    ResolvedStreams {
        main: set.main.url,
        sub: set.sub.map(|s| s.url),
        suggested_name,
    }
}

fn map_error(e: OnvifError) -> ResolveError {
    match e {
        OnvifError::NotOnvif => ResolveError::NotOnvif,
        OnvifError::AuthenticationFailed => ResolveError::AuthenticationFailed,
        OnvifError::NoVideoProfiles => ResolveError::NoVideoProfiles,
        OnvifError::Unreachable(m) => ResolveError::Unreachable(m),
        OnvifError::Protocol(m) => ResolveError::Protocol(m),
    }
}

/// Busca WS-Discovery com a configuração padrão (3 s, todas as interfaces).
#[derive(Default)]
pub struct OnvifCameraDiscovery {
    config: DiscoveryConfig,
}

impl CameraDiscovery for OnvifCameraDiscovery {
    fn discover(&self) -> Result<Vec<DiscoveredCamera>, DiscoveryFailure> {
        discover(&self.config)
            .map(|list| {
                list.into_iter()
                    .map(|d| DiscoveredCamera {
                        host: d.host,
                        device_url: d.device_url,
                        suggested_name: d.name,
                    })
                    .collect()
            })
            .map_err(|e| DiscoveryFailure(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use rtsp_viewer_core::domain::RtspUrl;
    use rtsp_viewer_onvif::StreamProfile;

    use super::*;

    fn set(manufacturer: Option<&str>, model: Option<&str>) -> StreamSet {
        StreamSet {
            manufacturer: manufacturer.map(str::to_string),
            model: model.map(str::to_string),
            main: StreamProfile {
                url: RtspUrl::parse("rtsp://h/s").unwrap().url,
                width: 1,
                height: 1,
                encoding: None,
            },
            sub: None,
        }
    }

    #[test]
    fn nome_sugerido() {
        assert_eq!(
            to_resolved(set(Some("TP-Link"), Some("C200")))
                .suggested_name
                .as_deref(),
            Some("TP-Link C200")
        );
        assert_eq!(
            to_resolved(set(Some("HIKVISION"), Some("Hikvision DS-2CD")))
                .suggested_name
                .as_deref(),
            Some("Hikvision DS-2CD")
        );
        assert_eq!(
            to_resolved(set(None, Some("C200")))
                .suggested_name
                .as_deref(),
            Some("C200")
        );
        assert_eq!(to_resolved(set(None, None)).suggested_name, None);
    }

    #[test]
    fn erros_mapeados() {
        assert_eq!(
            map_error(OnvifError::AuthenticationFailed),
            ResolveError::AuthenticationFailed
        );
        assert_eq!(map_error(OnvifError::NotOnvif), ResolveError::NotOnvif);
    }
}
