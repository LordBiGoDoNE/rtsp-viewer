use std::fmt;

/// Senha de câmera. Não implementa `Display` e o `Debug` é mascarado, para que
/// a senha não vaze em logs ou mensagens por acidente.
#[derive(Clone, PartialEq, Eq)]
pub struct Password(String);

impl Password {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Acesso explícito ao valor, só para quem precisa entregá-lo ao cofre ou
    /// montar a URL de conexão.
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Password {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Password(***)")
    }
}

/// URL de conexão com credenciais embutidas. Existe só em memória, no momento
/// de abrir o stream. `Debug` mascarado e sem `Display`.
#[derive(Clone, PartialEq, Eq)]
pub struct ConnectionUrl(String);

impl ConnectionUrl {
    pub(crate) fn new(value: String) -> Self {
        Self(value)
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ConnectionUrl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ConnectionUrl({})", redact_credentials(&self.0))
    }
}

/// Remove credenciais de URLs `rtsp://`/`rtsps://` presentes num texto
/// qualquer (mensagens do FFmpeg, erros), trocando `usuario:senha@` por `***@`.
pub fn redact_credentials(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = find_scheme(rest) {
        let (before, from_scheme) = rest.split_at(pos);
        out.push_str(before);
        let sep = from_scheme.find("://").map(|i| i + 3).unwrap_or(0);
        let (scheme, after_scheme) = from_scheme.split_at(sep);
        out.push_str(scheme);
        let authority_end = after_scheme
            .find(|c: char| c == '/' || c == '?' || c == '#' || c.is_whitespace())
            .unwrap_or(after_scheme.len());
        let authority = &after_scheme[..authority_end];
        match authority.rfind('@') {
            Some(at) => {
                out.push_str("***");
                out.push_str(&authority[at..]);
            }
            None => out.push_str(authority),
        }
        rest = &after_scheme[authority_end..];
    }
    out.push_str(rest);
    out
}

fn find_scheme(text: &str) -> Option<usize> {
    let lower = text.to_ascii_lowercase();
    [lower.find("rtsp://"), lower.find("rtsps://")]
        .into_iter()
        .flatten()
        .min()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_da_senha_e_mascarado() {
        let p = Password::new("segredo");
        assert_eq!(format!("{p:?}"), "Password(***)");
    }

    #[test]
    fn redige_credenciais_em_texto_livre() {
        let msg = "falha ao abrir rtsp://admin:s3gr3do@10.0.0.2:554/stream1: 401";
        let r = redact_credentials(msg);
        assert_eq!(r, "falha ao abrir rtsp://***@10.0.0.2:554/stream1: 401");
        assert!(!r.contains("s3gr3do"));
    }

    #[test]
    fn redige_varias_urls_e_rtsps() {
        let msg = "a RTSP://u:p@h/x e rtsps://v:q@k:322/y?z";
        assert_eq!(
            redact_credentials(msg),
            "a RTSP://***@h/x e rtsps://***@k:322/y?z"
        );
    }

    #[test]
    fn arroba_na_senha_nao_vaza() {
        let msg = "rtsp://admin:a@b@host/s";
        assert_eq!(redact_credentials(msg), "rtsp://***@host/s");
    }

    #[test]
    fn texto_sem_credenciais_fica_igual() {
        let msg = "rtsp://host:554/s e texto";
        assert_eq!(redact_credentials(msg), msg);
    }

    #[test]
    fn debug_da_url_de_conexao_nao_mostra_senha() {
        let u = ConnectionUrl::new("rtsp://a:b@h/s".into());
        assert_eq!(format!("{u:?}"), "ConnectionUrl(rtsp://***@h/s)");
    }
}
