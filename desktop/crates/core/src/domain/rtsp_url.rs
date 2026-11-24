use std::fmt;

use super::secret::{ConnectionUrl, Password};

/// Endereço RTSP validado, sempre **sem** credenciais.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RtspUrl {
    secure: bool,
    host: String,
    port: Option<u16>,
    /// Caminho e consulta, com a barra inicial; vazio quando não há caminho.
    path: String,
}

/// Resultado da leitura de uma URL digitada: o endereço limpo e as
/// credenciais que vieram embutidas nela, se houver.
#[derive(Debug, PartialEq, Eq)]
pub struct ParsedUrl {
    pub url: RtspUrl,
    pub username: Option<String>,
    pub password: Option<Password>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UrlError {
    #[error("Informe o endereço da câmera.")]
    Empty,
    #[error("O endereço deve começar com rtsp:// ou rtsps://.")]
    InvalidScheme,
    #[error("Endereço sem host válido.")]
    InvalidHost,
    #[error("Porta inválida: use um número entre 1 e 65535.")]
    InvalidPort,
    #[error("O endereço contém espaços ou caracteres inválidos.")]
    InvalidCharacters,
    #[error("Usuário ou senha na URL com codificação inválida.")]
    InvalidCredentials,
}

impl RtspUrl {
    pub fn parse(input: &str) -> Result<ParsedUrl, UrlError> {
        let input = input.trim();
        if input.is_empty() {
            return Err(UrlError::Empty);
        }
        let (scheme, rest) = input.split_once("://").ok_or(UrlError::InvalidScheme)?;
        let secure = match scheme.to_ascii_lowercase().as_str() {
            "rtsp" => false,
            "rtsps" => true,
            _ => return Err(UrlError::InvalidScheme),
        };
        if rest.chars().any(|c| c.is_whitespace() || c.is_control()) {
            return Err(UrlError::InvalidCharacters);
        }

        let authority_end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
        let (authority, path) = rest.split_at(authority_end);
        let path = path.split('#').next().unwrap_or_default().to_string();

        let (userinfo, hostport) = match authority.rfind('@') {
            Some(at) => (Some(&authority[..at]), &authority[at + 1..]),
            None => (None, authority),
        };

        let (username, password) = match userinfo {
            None => (None, None),
            Some(info) => {
                let (user, pass) = match info.split_once(':') {
                    Some((u, p)) => (u, Some(p)),
                    None => (info, None),
                };
                let user = percent_decode(user)?;
                let pass = pass.map(percent_decode).transpose()?;
                (
                    Some(user).filter(|u| !u.is_empty()),
                    pass.filter(|p| !p.is_empty()).map(Password::new),
                )
            }
        };

        let (host, port) = split_host_port(hostport)?;
        Ok(ParsedUrl {
            url: RtspUrl {
                secure,
                host,
                port,
                path,
            },
            username,
            password,
        })
    }

    pub fn host(&self) -> &str {
        &self.host
    }

    pub fn port(&self) -> Option<u16> {
        self.port
    }

    /// Monta a URL de conexão com as credenciais codificadas. O resultado só
    /// deve existir em memória, no momento de abrir o stream. Senha sem
    /// usuário é ignorada.
    pub fn with_credentials(
        &self,
        username: Option<&str>,
        password: Option<&Password>,
    ) -> ConnectionUrl {
        let mut s = String::from(self.scheme());
        if let Some(username) = username {
            s.push_str(&percent_encode(username));
            if let Some(p) = password {
                s.push(':');
                s.push_str(&percent_encode(p.expose()));
            }
            s.push('@');
        }
        self.push_host_port_path(&mut s);
        ConnectionUrl::new(s)
    }

    fn scheme(&self) -> &'static str {
        if self.secure { "rtsps://" } else { "rtsp://" }
    }

    fn push_host_port_path(&self, s: &mut String) {
        if self.host.contains(':') {
            s.push('[');
            s.push_str(&self.host);
            s.push(']');
        } else {
            s.push_str(&self.host);
        }
        if let Some(port) = self.port {
            s.push(':');
            s.push_str(&port.to_string());
        }
        s.push_str(&self.path);
    }
}

impl fmt::Display for RtspUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = String::from(self.scheme());
        self.push_host_port_path(&mut s);
        f.write_str(&s)
    }
}

fn split_host_port(hostport: &str) -> Result<(String, Option<u16>), UrlError> {
    let (host, port) = if let Some(rest) = hostport.strip_prefix('[') {
        let (h, after) = rest.split_once(']').ok_or(UrlError::InvalidHost)?;
        // Endereço IPv6 (hex, ':' e '.') com zona opcional depois de '%'.
        let (addr, zone) = h.split_once('%').unwrap_or((h, ""));
        let addr_ok = addr.contains(':')
            && addr
                .chars()
                .all(|c| c.is_ascii_hexdigit() || ":.".contains(c));
        let zone_ok = zone
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
        if !addr_ok || !zone_ok {
            return Err(UrlError::InvalidHost);
        }
        let port = match after {
            "" => None,
            p => Some(p.strip_prefix(':').ok_or(UrlError::InvalidPort)?),
        };
        (h.to_string(), port)
    } else {
        let (h, p) = match hostport.split_once(':') {
            Some((h, p)) => (h, Some(p)),
            None => (hostport, None),
        };
        let valid = !h.is_empty()
            && h.chars()
                .all(|c| c.is_ascii_alphanumeric() || "-._".contains(c))
            && !h.starts_with(['.', '-'])
            && !h.ends_with('-');
        if !valid {
            return Err(UrlError::InvalidHost);
        }
        (h.to_ascii_lowercase(), p)
    };
    let port = match port {
        None => None,
        Some(p) => match p.parse::<u16>() {
            Ok(n) if n > 0 && p.chars().all(|c| c.is_ascii_digit()) => Some(n),
            _ => return Err(UrlError::InvalidPort),
        },
    };
    Ok((host, port))
}

fn percent_decode(s: &str) -> Result<String, UrlError> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = s.get(i + 1..i + 3).ok_or(UrlError::InvalidCredentials)?;
            let b = u8::from_str_radix(hex, 16).map_err(|_| UrlError::InvalidCredentials)?;
            out.push(b);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| UrlError::InvalidCredentials)
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(s: &str) -> ParsedUrl {
        RtspUrl::parse(s).expect(s)
    }

    #[test]
    fn aceita_url_simples() {
        let p = ok("rtsp://192.168.0.10:554/stream1");
        assert_eq!(p.url.to_string(), "rtsp://192.168.0.10:554/stream1");
        assert_eq!(p.url.host(), "192.168.0.10");
        assert_eq!(p.url.port(), Some(554));
        assert_eq!(p.username, None);
        assert_eq!(p.password, None);
    }

    #[test]
    fn aceita_rtsps_host_sem_porta_e_sem_caminho() {
        assert_eq!(
            ok("rtsps://Camera.Local").url.to_string(),
            "rtsps://camera.local"
        );
        assert_eq!(
            ok("  RTSP://cam/live?ch=1  ").url.to_string(),
            "rtsp://cam/live?ch=1"
        );
    }

    #[test]
    fn aceita_ipv6() {
        let p = ok("rtsp://[fe80::1%25eth0]:8554/a");
        assert_eq!(p.url.host(), "fe80::1%25eth0");
        assert_eq!(p.url.to_string(), "rtsp://[fe80::1%25eth0]:8554/a");
    }

    #[test]
    fn extrai_credenciais_embutidas() {
        let p = ok("rtsp://admin:segredo@192.168.0.10/stream1");
        assert_eq!(p.url.to_string(), "rtsp://192.168.0.10/stream1");
        assert_eq!(p.username.as_deref(), Some("admin"));
        assert_eq!(p.password.as_ref().map(Password::expose), Some("segredo"));
    }

    #[test]
    fn decodifica_credenciais_e_aceita_arroba_na_senha() {
        let p = ok("rtsp://user%40x:p%3Aw@d@host/s");
        assert_eq!(p.username.as_deref(), Some("user@x"));
        assert_eq!(p.password.unwrap().expose(), "p:w@d");
    }

    #[test]
    fn usuario_sem_senha() {
        let p = ok("rtsp://admin@host/s");
        assert_eq!(p.username.as_deref(), Some("admin"));
        assert!(p.password.is_none());
    }

    #[test]
    fn descarta_fragmento() {
        assert_eq!(ok("rtsp://h/s#x").url.to_string(), "rtsp://h/s");
    }

    #[test]
    fn rejeita_entradas_invalidas() {
        let casos = [
            ("", UrlError::Empty),
            ("   ", UrlError::Empty),
            ("http://192.168.0.10/stream", UrlError::InvalidScheme),
            ("192.168.0.10/stream", UrlError::InvalidScheme),
            ("rtsp://", UrlError::InvalidHost),
            ("rtsp:///s", UrlError::InvalidHost),
            ("rtsp://user@/s", UrlError::InvalidHost),
            ("rtsp://ho st/s", UrlError::InvalidCharacters),
            ("rtsp://h/a b", UrlError::InvalidCharacters),
            ("rtsp://h:0/s", UrlError::InvalidPort),
            ("rtsp://h:70000/s", UrlError::InvalidPort),
            ("rtsp://h:+5/s", UrlError::InvalidPort),
            ("rtsp://h:/s", UrlError::InvalidPort),
            ("rtsp://a:b:c/s", UrlError::InvalidPort),
            ("rtsp://[::1/s", UrlError::InvalidHost),
            ("rtsp://[abc]/s", UrlError::InvalidHost),
            ("rtsp://[::1]x/s", UrlError::InvalidPort),
            ("rtsp://u:%zz@h/s", UrlError::InvalidCredentials),
            ("rtsp://u:%ff@h/s", UrlError::InvalidCredentials),
        ];
        for (entrada, erro) in casos {
            assert_eq!(RtspUrl::parse(entrada), Err(erro), "entrada: {entrada:?}");
        }
    }

    #[test]
    fn monta_url_de_conexao_codificada() {
        let p = ok("rtsp://h:554/s");
        let senha = Password::new("p:w@d/x");
        let c = p.url.with_credentials(Some("ad min"), Some(&senha));
        assert_eq!(c.expose(), "rtsp://ad%20min:p%3Aw%40d%2Fx@h:554/s");
        let ida_e_volta = RtspUrl::parse(c.expose()).unwrap();
        assert_eq!(ida_e_volta.username.as_deref(), Some("ad min"));
        assert_eq!(ida_e_volta.password.unwrap().expose(), "p:w@d/x");
    }

    #[test]
    fn url_de_conexao_sem_credenciais() {
        let p = ok("rtsp://h/s");
        assert_eq!(p.url.with_credentials(None, None).expose(), "rtsp://h/s");
    }

    #[test]
    fn senha_sem_usuario_e_ignorada() {
        let p = ok("rtsp://h/s");
        let senha = Password::new("antiga");
        assert_eq!(
            p.url.with_credentials(None, Some(&senha)).expose(),
            "rtsp://h/s"
        );
    }
}
