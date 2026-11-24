use std::fmt;

/// Endereço de uma câmera sem esquema: IP, nome de host ou `host:porta`
/// (IPv6 entre colchetes). É o que o usuário digita para o app consultar a
/// câmera pelo ONVIF (spec `camera-registry`, "Cadastro só com o endereço").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceAddress {
    host: String,
    port: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AddressError {
    #[error("Informe o endereço da câmera.")]
    Empty,
    #[error("Use um IP, um nome de host ou uma URL rtsp:// completa.")]
    Invalid,
    #[error("Porta inválida: use um número entre 1 e 65535.")]
    InvalidPort,
}

impl DeviceAddress {
    pub fn parse(input: &str) -> Result<Self, AddressError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(AddressError::Empty);
        }
        if input.contains("://")
            || input.contains(['/', '?', '#', '@'])
            || input.chars().any(char::is_whitespace)
        {
            return Err(AddressError::Invalid);
        }
        let (host, port) = if let Some(rest) = input.strip_prefix('[') {
            let (h, after) = rest.split_once(']').ok_or(AddressError::Invalid)?;
            let addr_ok =
                h.contains(':') && h.chars().all(|c| c.is_ascii_hexdigit() || ":.".contains(c));
            if !addr_ok {
                return Err(AddressError::Invalid);
            }
            let port = match after {
                "" => None,
                p => Some(p.strip_prefix(':').ok_or(AddressError::InvalidPort)?),
            };
            (h.to_ascii_lowercase(), port)
        } else {
            let (h, p) = match input.split_once(':') {
                Some((h, p)) => (h, Some(p)),
                None => (input, None),
            };
            let valid = !h.is_empty()
                && h.chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-._".contains(c))
                && !h.starts_with(['.', '-'])
                && !h.ends_with(['.', '-']);
            if !valid {
                return Err(AddressError::Invalid);
            }
            (h.to_ascii_lowercase(), p)
        };
        let port = match port {
            None => None,
            Some(p) => match p.parse::<u16>() {
                Ok(n) if n > 0 && p.chars().all(|c| c.is_ascii_digit()) => Some(n),
                _ => return Err(AddressError::InvalidPort),
            },
        };
        Ok(Self { host, port })
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> Option<u16> {
        self.port
    }

    /// Host como aparece numa URL (IPv6 entre colchetes).
    pub fn host_for_url(&self) -> String {
        if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        }
    }
}

impl fmt::Display for DeviceAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.port {
            Some(p) => write!(f, "{}:{p}", self.host_for_url()),
            None => f.write_str(&self.host_for_url()),
        }
    }
}

/// Endereço da câmera apontado por uma URL de serviço ONVIF (`http://h:p/...`)
/// ou digitado. `None` para URLs com credenciais ou fora de `http://`.
pub fn host_of_service_url(url: &str) -> Option<String> {
    let rest = url.strip_prefix("http://")?;
    let authority = rest.split(['/', '?', '#']).next()?;
    if authority.contains('@') {
        return None;
    }
    let host = if let Some(v6) = authority.strip_prefix('[') {
        v6.split(']').next()?.to_string()
    } else {
        authority.split(':').next()?.to_string()
    };
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aceita_ip_nome_e_porta() {
        let a = DeviceAddress::parse(" 192.168.1.120 ").unwrap();
        assert_eq!((a.host(), a.port()), ("192.168.1.120", None));
        let a = DeviceAddress::parse("Cam-Portao.local:2020").unwrap();
        assert_eq!((a.host(), a.port()), ("cam-portao.local", Some(2020)));
        assert_eq!(a.to_string(), "cam-portao.local:2020");
        let a = DeviceAddress::parse("[FE80::1]:80").unwrap();
        assert_eq!((a.host(), a.port()), ("fe80::1", Some(80)));
        assert_eq!(a.to_string(), "[fe80::1]:80");
    }

    #[test]
    fn recusa_formatos_invalidos() {
        assert_eq!(DeviceAddress::parse(""), Err(AddressError::Empty));
        for s in [
            "http://x",
            "rtsp://h/s",
            "h/s",
            "user@h",
            "a b",
            "-h",
            "h.",
            "[zz]",
            "a:b:c",
        ] {
            assert!(DeviceAddress::parse(s).is_err(), "{s}");
        }
        for s in ["192.168.1.120:abc", "h:0", "h:70000", "h:", "[::1]x"] {
            assert_eq!(
                DeviceAddress::parse(s),
                Err(AddressError::InvalidPort),
                "{s}"
            );
        }
    }

    #[test]
    fn host_de_url_de_servico() {
        assert_eq!(
            host_of_service_url("http://192.168.1.120:2020/onvif/device_service").as_deref(),
            Some("192.168.1.120")
        );
        assert_eq!(
            host_of_service_url("http://[fe80::1]:80/x").as_deref(),
            Some("fe80::1")
        );
        assert_eq!(host_of_service_url("http://192.168.1.5@evil.com/x"), None);
        assert_eq!(host_of_service_url("https://h/x"), None);
    }
}
