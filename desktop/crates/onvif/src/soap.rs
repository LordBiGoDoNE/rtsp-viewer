//! Envelopes SOAP 1.2 e leitura de respostas e falhas.

use crate::xml::{self, Element};

pub const SOAP_ENV: &str = "http://www.w3.org/2003/05/soap-envelope";
pub const DEVICE_NS: &str = "http://www.onvif.org/ver10/device/wsdl";
pub const MEDIA_NS: &str = "http://www.onvif.org/ver10/media/wsdl";
pub const SCHEMA_NS: &str = "http://www.onvif.org/ver10/schema";

/// Limite de tamanho de uma resposta HTTP (design D1).
pub const MAX_RESPONSE_BYTES: u64 = 256 * 1024;

pub fn envelope(security_header: Option<&str>, body: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><s:Envelope xmlns:s="{SOAP_ENV}"><s:Header>{}</s:Header><s:Body>{body}</s:Body></s:Envelope>"#,
        security_header.unwrap_or_default()
    )
}

/// Falha SOAP com os códigos (`Code/Value` e `Subcode/Value` aninhados) e o texto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fault {
    pub codes: Vec<String>,
    pub reason: String,
}

impl Fault {
    /// `ter:NotAuthorized` (ONVIF) ou texto equivalente de firmwares antigos.
    pub fn is_not_authorized(&self) -> bool {
        let reason = self.reason.to_ascii_lowercase();
        self.codes
            .iter()
            .any(|c| local_part(c).eq_ignore_ascii_case("NotAuthorized"))
            || reason.contains("authoriz")
            || reason.contains("authority failure")
    }
}

fn local_part(qname: &str) -> &str {
    qname.rsplit(':').next().unwrap_or(qname).trim()
}

pub enum Reply {
    Body(Element),
    Fault(Fault),
}

/// Lê um envelope SOAP e devolve o `Body` (ou a falha). `None` se o documento
/// não for um envelope SOAP.
pub fn read_reply(text: &str) -> Option<Reply> {
    let doc = xml::parse(text).ok()?;
    if doc.name != "Envelope" {
        return None;
    }
    let body = doc.child("Body")?.clone();
    if let Some(f) = body.child("Fault") {
        let mut codes = Vec::new();
        let mut code = f.child("Code");
        while let Some(c) = code {
            if let Some(v) = c.child("Value") {
                codes.push(v.text_trimmed().to_string());
            }
            code = c.child("Subcode");
        }
        // SOAP 1.1 (alguns firmwares): faultcode / faultstring.
        if let Some(fc) = f.child("faultcode") {
            codes.push(fc.text_trimmed().to_string());
        }
        let reason = f
            .find("Text")
            .or_else(|| f.child("faultstring"))
            .map(|t| t.text_trimmed().to_string())
            .unwrap_or_default();
        return Some(Reply::Fault(Fault { codes, reason }));
    }
    Some(Reply::Body(body))
}

pub fn get_system_date_and_time() -> String {
    format!(r#"<GetSystemDateAndTime xmlns="{DEVICE_NS}"/>"#)
}

pub fn get_device_information() -> String {
    format!(r#"<GetDeviceInformation xmlns="{DEVICE_NS}"/>"#)
}

pub fn get_capabilities_media() -> String {
    format!(r#"<GetCapabilities xmlns="{DEVICE_NS}"><Category>Media</Category></GetCapabilities>"#)
}

pub fn get_profiles() -> String {
    format!(r#"<GetProfiles xmlns="{MEDIA_NS}"/>"#)
}

pub fn get_stream_uri(profile_token: &str) -> String {
    format!(
        concat!(
            r#"<GetStreamUri xmlns="{media}"><StreamSetup>"#,
            r#"<Stream xmlns="{schema}">RTP-Unicast</Stream>"#,
            r#"<Transport xmlns="{schema}"><Protocol>RTSP</Protocol></Transport>"#,
            r#"</StreamSetup><ProfileToken>{token}</ProfileToken></GetStreamUri>"#
        ),
        media = MEDIA_NS,
        schema = SCHEMA_NS,
        token = xml::escape(profile_token)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_com_e_sem_seguranca() {
        let e = envelope(None, &get_profiles());
        assert!(e.contains("<s:Header></s:Header>"));
        assert!(e.contains(r#"<GetProfiles xmlns="http://www.onvif.org/ver10/media/wsdl"/>"#));
        assert!(envelope(Some("<x/>"), "").contains("<s:Header><x/></s:Header>"));
    }

    #[test]
    fn token_do_perfil_escapado() {
        assert!(get_stream_uri("a<b").contains("<ProfileToken>a&lt;b</ProfileToken>"));
    }

    #[test]
    fn le_falha_not_authorized_soap12() {
        let xml = r#"<env:Envelope xmlns:env="http://www.w3.org/2003/05/soap-envelope" xmlns:ter="http://www.onvif.org/ver10/error">
          <env:Body><env:Fault><env:Code><env:Value>env:Sender</env:Value>
          <env:Subcode><env:Value>ter:NotAuthorized</env:Value></env:Subcode></env:Code>
          <env:Reason><env:Text xml:lang="en">Sender not Authorized</env:Text></env:Reason></env:Fault></env:Body></env:Envelope>"#;
        let Some(Reply::Fault(f)) = read_reply(xml) else {
            panic!()
        };
        assert_eq!(f.codes, ["env:Sender", "ter:NotAuthorized"]);
        assert!(f.is_not_authorized());
    }

    #[test]
    fn le_falha_soap11_e_outras_falhas() {
        let xml = r#"<s:Envelope xmlns:s="x"><s:Body><s:Fault><faultcode>s:Client</faultcode>
          <faultstring>The action requested requires authorization</faultstring></s:Fault></s:Body></s:Envelope>"#;
        let Some(Reply::Fault(f)) = read_reply(xml) else {
            panic!()
        };
        assert!(f.is_not_authorized());
        assert_eq!(f.reason, "The action requested requires authorization");
        let outra = Fault {
            codes: vec!["env:Receiver".into()],
            reason: "Action not supported".into(),
        };
        assert!(!outra.is_not_authorized());
    }

    #[test]
    fn nao_envelope() {
        assert!(read_reply("<html><body>login</body></html>").is_none());
        assert!(read_reply("lixo").is_none());
    }
}
