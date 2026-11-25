//! Autenticação ONVIF (design D2): WS-UsernameToken PasswordDigest com o
//! relógio da câmera, e HTTP Digest (MD5) quando a câmera exige. A senha nunca
//! é enviada em texto puro.

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use md5::{Digest as _, Md5};
use rtsp_viewer_core::domain::Password;
use sha1::Sha1;

use crate::xml::escape;

/// Credenciais de uma consulta. O `Debug` não mostra a senha (vem de `Password`).
#[derive(Debug, Clone)]
pub struct Credentials {
    pub username: String,
    pub password: Password,
}

/// Instante em segundos desde a época Unix (UTC).
pub type UnixSeconds = i64;

/// Diferença entre o relógio da câmera e o local, em segundos.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClockOffset(pub i64);

impl ClockOffset {
    pub fn between(camera_now: UnixSeconds, local_now: UnixSeconds) -> Self {
        Self(camera_now - local_now)
    }

    pub fn camera_time(self, local_now: UnixSeconds) -> UnixSeconds {
        local_now + self.0
    }
}

/// Cabeçalho WS-Security com UsernameToken PasswordDigest.
pub fn ws_security_header(creds: &Credentials, nonce: &[u8], created_utc: UnixSeconds) -> String {
    let created = format_utc(created_utc);
    let digest = password_digest(nonce, &created, creds.password.expose());
    format!(
        concat!(
            r#"<wsse:Security s:mustUnderstand="1" "#,
            r#"xmlns:wsse="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd" "#,
            r#"xmlns:wsu="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-utility-1.0.xsd">"#,
            "<wsse:UsernameToken><wsse:Username>{user}</wsse:Username>",
            r#"<wsse:Password Type="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-username-token-profile-1.0#PasswordDigest">{digest}</wsse:Password>"#,
            r#"<wsse:Nonce EncodingType="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-soap-message-security-1.0#Base64Binary">{nonce}</wsse:Nonce>"#,
            "<wsu:Created>{created}</wsu:Created></wsse:UsernameToken></wsse:Security>"
        ),
        user = escape(&creds.username),
        digest = digest,
        nonce = B64.encode(nonce),
        created = created,
    )
}

/// `Base64(SHA1(nonce + created + senha))`.
pub fn password_digest(nonce: &[u8], created: &str, password: &str) -> String {
    let mut h = Sha1::new();
    h.update(nonce);
    h.update(created.as_bytes());
    h.update(password.as_bytes());
    B64.encode(h.finalize())
}

/// Bytes aleatórios para nonce (WS-Security e cnonce do HTTP Digest).
pub fn random_bytes<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    // Falha do gerador do sistema é irrecuperável e rara; sem nonce aleatório
    // não há autenticação segura.
    getrandom::fill(&mut b).expect("gerador aleatório do sistema indisponível");
    b
}

/// Desafio `WWW-Authenticate: Digest ...` de uma resposta 401.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DigestChallenge {
    pub realm: String,
    pub nonce: String,
    pub opaque: Option<String>,
    pub qop_auth: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChallengeError {
    #[error("a câmera não pediu HTTP Digest")]
    NotDigest,
    #[error("a câmera pede HTTP Digest com algoritmo não suportado: {0}")]
    UnsupportedAlgorithm(String),
    #[error("desafio HTTP Digest incompleto")]
    Incomplete,
}

impl DigestChallenge {
    pub fn parse(header: &str) -> Result<Self, ChallengeError> {
        let header = header.trim();
        let rest = header
            .get(..6)
            .filter(|s| s.eq_ignore_ascii_case("digest"))
            .map(|_| &header[6..])
            .ok_or(ChallengeError::NotDigest)?;
        let params = parse_params(rest);
        let get = |k: &str| {
            params
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(k))
                .map(|(_, v)| v.clone())
        };
        if let Some(alg) = get("algorithm")
            && !alg.eq_ignore_ascii_case("MD5")
        {
            return Err(ChallengeError::UnsupportedAlgorithm(alg));
        }
        let qop_auth = get("qop")
            .map(|q| q.split(',').any(|v| v.trim().eq_ignore_ascii_case("auth")))
            .unwrap_or(false);
        Ok(Self {
            realm: get("realm").ok_or(ChallengeError::Incomplete)?,
            nonce: get("nonce").ok_or(ChallengeError::Incomplete)?,
            opaque: get("opaque"),
            qop_auth,
        })
    }

    /// Valor do cabeçalho `Authorization` para um POST em `uri` (caminho).
    pub fn authorization(
        &self,
        creds: &Credentials,
        method: &str,
        uri: &str,
        cnonce: &str,
    ) -> String {
        let ha1 = md5_hex(&format!(
            "{}:{}:{}",
            creds.username,
            self.realm,
            creds.password.expose()
        ));
        let ha2 = md5_hex(&format!("{method}:{uri}"));
        let nc = "00000001";
        let response = if self.qop_auth {
            md5_hex(&format!("{ha1}:{}:{nc}:{cnonce}:auth:{ha2}", self.nonce))
        } else {
            md5_hex(&format!("{ha1}:{}:{ha2}", self.nonce))
        };
        let mut h = format!(
            r#"Digest username="{}", realm="{}", nonce="{}", uri="{}", response="{}", algorithm=MD5"#,
            quote_safe(&creds.username),
            quote_safe(&self.realm),
            quote_safe(&self.nonce),
            quote_safe(uri),
            response
        );
        if self.qop_auth {
            h.push_str(&format!(r#", qop=auth, nc={nc}, cnonce="{cnonce}""#));
        }
        if let Some(o) = &self.opaque {
            h.push_str(&format!(r#", opaque="{}""#, quote_safe(o)));
        }
        h
    }
}

fn quote_safe(s: &str) -> String {
    s.replace(['"', '\\'], "")
}

pub fn md5_hex(s: &str) -> String {
    let mut h = Md5::new();
    h.update(s.as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// `k1="v1", k2=v2` → pares, respeitando vírgulas dentro de aspas.
fn parse_params(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut chars = s.chars().peekable();
    loop {
        while matches!(chars.peek(), Some(c) if c.is_whitespace() || *c == ',') {
            chars.next();
        }
        let key: String = chars
            .by_ref()
            .take_while(|c| *c != '=')
            .collect::<String>()
            .trim()
            .to_string();
        if key.is_empty() {
            break;
        }
        let value = if chars.peek() == Some(&'"') {
            chars.next();
            let mut v = String::new();
            while let Some(c) = chars.next() {
                match c {
                    '\\' => {
                        if let Some(n) = chars.next() {
                            v.push(n);
                        }
                    }
                    '"' => break,
                    c => v.push(c),
                }
            }
            v
        } else {
            chars
                .by_ref()
                .take_while(|c| *c != ',')
                .collect::<String>()
                .trim()
                .to_string()
        };
        out.push((key, value));
    }
    out
}

/// `AAAA-MM-DDTHH:MM:SSZ`.
pub fn format_utc(t: UnixSeconds) -> String {
    let (days, rem) = (t.div_euclid(86_400), t.rem_euclid(86_400));
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// Data e hora UTC → segundos Unix. `None` se os campos forem inválidos.
pub fn utc_to_unix(y: i64, m: u32, d: u32, hh: u32, mm: u32, ss: u32) -> Option<UnixSeconds> {
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || hh > 23 || mm > 59 || ss > 60 {
        return None;
    }
    Some(
        days_from_civil(y, m, d) * 86_400
            + i64::from(hh) * 3600
            + i64::from(mm) * 60
            + i64::from(ss),
    )
}

// Algoritmos de Howard Hinnant (calendário gregoriano proléptico).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = i64::from(if m > 2 { m - 3 } else { m + 9 });
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    fn creds(user: &str, pass: &str) -> Credentials {
        Credentials {
            username: user.into(),
            password: Password::new(pass),
        }
    }

    #[test]
    fn password_digest_do_exemplo_da_norma() {
        // Exemplo clássico do WS-Security UsernameToken Profile 1.0.
        let nonce = B64.decode("LKqI6G/AikKCQrN0zqZFlg==").unwrap();
        assert_eq!(
            password_digest(&nonce, "2010-09-16T07:50:45Z", "userpassword"),
            "tuOSpGlFlIXsozq4HFNeeGeFLEI="
        );
    }

    #[test]
    fn cabecalho_nao_contem_a_senha_e_escapa_usuario() {
        let h = ws_security_header(&creds("ad<m>in", "s3gr3do-unico"), &[1; 16], 0);
        assert!(!h.contains("s3gr3do-unico"));
        assert!(h.contains("<wsse:Username>ad&lt;m&gt;in</wsse:Username>"));
        assert!(h.contains("<wsu:Created>1970-01-01T00:00:00Z</wsu:Created>"));
        assert!(h.contains("#PasswordDigest"));
        assert!(!h.contains("#PasswordText"));
    }

    #[test]
    fn datas_ida_e_volta() {
        for t in [0, 951_782_400, 1_795_516_365, 4_102_444_800] {
            let s = format_utc(t);
            let (y, rest) = s.split_at(4);
            let p: Vec<u32> = rest[1..15]
                .split(['-', 'T', ':'])
                .map(|x| x.parse().unwrap())
                .collect();
            assert_eq!(
                utc_to_unix(y.parse().unwrap(), p[0], p[1], p[2], p[3], p[4]),
                Some(t),
                "{s}"
            );
        }
        assert_eq!(format_utc(1_795_516_365), "2026-11-24T10:32:45Z");
        assert_eq!(utc_to_unix(2026, 13, 1, 0, 0, 0), None);
    }

    #[test]
    fn deslocamento_de_relogio() {
        let off = ClockOffset::between(10_800, 0);
        assert_eq!(off.camera_time(100), 10_900);
    }

    #[test]
    fn desafio_digest_com_qop_e_opaque() {
        let c = DigestChallenge::parse(
            r#"Digest realm="IP Camera(C1234)", qop="auth,auth-int", nonce="abc,def", opaque="xyz", stale="FALSE""#,
        )
        .unwrap();
        assert_eq!(c.realm, "IP Camera(C1234)");
        assert_eq!(c.nonce, "abc,def");
        assert!(c.qop_auth);
        assert_eq!(c.opaque.as_deref(), Some("xyz"));
    }

    #[test]
    fn resposta_digest_da_rfc_2617() {
        // Exemplo da RFC 2617, seção 3.5 (GET; aqui o método é parâmetro).
        let c = DigestChallenge {
            realm: "testrealm@host.com".into(),
            nonce: "dcd98b7102dd2f0e8b11d0f600bfb0c093".into(),
            opaque: Some("5ccc069c403ebaf9f0171e9517f40e41".into()),
            qop_auth: true,
        };
        let h = c.authorization(
            &creds("Mufasa", "Circle Of Life"),
            "GET",
            "/dir/index.html",
            "0a4f113b",
        );
        assert!(
            h.contains(r#"response="6629fae49393a05397450978507c4ef1""#),
            "{h}"
        );
        assert!(!h.contains("Circle Of Life"));
    }

    #[test]
    fn desafios_invalidos() {
        assert_eq!(
            DigestChallenge::parse(r#"Basic realm="x""#),
            Err(ChallengeError::NotDigest)
        );
        assert_eq!(
            DigestChallenge::parse(r#"Digest realm="x", nonce="y", algorithm=SHA-256"#),
            Err(ChallengeError::UnsupportedAlgorithm("SHA-256".into()))
        );
        assert_eq!(
            DigestChallenge::parse(r#"Digest realm="x""#),
            Err(ChallengeError::Incomplete)
        );
    }

    #[test]
    fn nonces_aleatorios_diferentes() {
        assert_ne!(random_bytes::<16>(), random_bytes::<16>());
    }
}
