//! Caminhos RTSP conhecidos por fabricante (design D5), usados quando a câmera
//! não responde ao ONVIF.

use super::device_address::DeviceAddress;
use super::rtsp_url::RtspUrl;

/// Porta RTSP padrão quando o endereço não informa outra.
pub const DEFAULT_RTSP_PORT: u16 = 554;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vendor {
    Tapo,
    Hikvision,
    Dahua,
    Reolink,
    Uniview,
    Axis,
}

impl Vendor {
    pub const ALL: [Vendor; 6] = [
        Vendor::Tapo,
        Vendor::Hikvision,
        Vendor::Dahua,
        Vendor::Reolink,
        Vendor::Uniview,
        Vendor::Axis,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Vendor::Tapo => "TP-Link Tapo",
            Vendor::Hikvision => "Hikvision",
            Vendor::Dahua => "Dahua / Intelbras / Amcrest",
            Vendor::Reolink => "Reolink",
            Vendor::Uniview => "Uniview",
            Vendor::Axis => "Axis",
        }
    }

    /// Caminhos principal e secundário.
    pub fn paths(self) -> (&'static str, Option<&'static str>) {
        match self {
            Vendor::Tapo => ("/stream1", Some("/stream2")),
            Vendor::Hikvision => ("/Streaming/Channels/101", Some("/Streaming/Channels/102")),
            Vendor::Dahua => (
                "/cam/realmonitor?channel=1&subtype=0",
                Some("/cam/realmonitor?channel=1&subtype=1"),
            ),
            Vendor::Reolink => ("/h264Preview_01_main", Some("/h264Preview_01_sub")),
            Vendor::Uniview => ("/media/video1", Some("/media/video2")),
            Vendor::Axis => ("/axis-media/media.amp", None),
        }
    }

    /// URLs RTSP principal e secundária para o endereço. A porta do endereço,
    /// se houver, é tratada como porta RTSP; senão vale 554.
    pub fn stream_urls(self, address: &DeviceAddress) -> (RtspUrl, Option<RtspUrl>) {
        let port = address.port().unwrap_or(DEFAULT_RTSP_PORT);
        let build = |path: &str| {
            RtspUrl::parse(&format!("rtsp://{}:{port}{path}", address.host_for_url()))
                .expect("caminhos de fabricante são URLs RTSP válidas")
                .url
        };
        let (main, sub) = self.paths();
        (build(main), sub.map(build))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn addr(s: &str) -> DeviceAddress {
        DeviceAddress::parse(s).unwrap()
    }

    #[test]
    fn hikvision_monta_principal_e_secundario() {
        let (m, s) = Vendor::Hikvision.stream_urls(&addr("192.168.1.164"));
        assert_eq!(
            m.to_string(),
            "rtsp://192.168.1.164:554/Streaming/Channels/101"
        );
        assert_eq!(
            s.unwrap().to_string(),
            "rtsp://192.168.1.164:554/Streaming/Channels/102"
        );
    }

    #[test]
    fn todos_os_fabricantes_geram_urls_validas() {
        for v in Vendor::ALL {
            let (m, _) = v.stream_urls(&addr("[fe80::1]:8554"));
            assert!(
                m.to_string().starts_with("rtsp://[fe80::1]:8554/"),
                "{}",
                v.label()
            );
        }
    }

    #[test]
    fn tapo_e_axis() {
        let (m, s) = Vendor::Tapo.stream_urls(&addr("cam.local"));
        assert_eq!(m.to_string(), "rtsp://cam.local:554/stream1");
        assert_eq!(s.unwrap().to_string(), "rtsp://cam.local:554/stream2");
        assert!(Vendor::Axis.stream_urls(&addr("h")).1.is_none());
    }
}
