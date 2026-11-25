//! Árvore XML mínima para respostas ONVIF.
//!
//! Firmwares usam prefixos de namespace diferentes (`tt:`, `trt:`, `ns2:`...);
//! por isso a árvore guarda só o nome local dos elementos e atributos, e as
//! buscas são por nome local. O tamanho da entrada é limitado por quem chama.

use quick_xml::Reader;
use quick_xml::events::{BytesStart, Event};

/// Profundidade máxima aceita, contra documentos patológicos.
const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Element>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("XML inválido: {0}")]
pub struct XmlError(pub String);

impl Element {
    /// Primeiro filho direto com o nome local dado.
    pub fn child(&self, name: &str) -> Option<&Element> {
        self.children.iter().find(|c| c.name == name)
    }

    /// Filhos diretos com o nome local dado.
    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }

    /// Caminho de filhos diretos, por nome local.
    pub fn path(&self, names: &[&str]) -> Option<&Element> {
        names.iter().try_fold(self, |el, n| el.child(n))
    }

    /// Primeiro descendente (busca em profundidade) com o nome local dado.
    pub fn find(&self, name: &str) -> Option<&Element> {
        for c in &self.children {
            if c.name == name {
                return Some(c);
            }
            if let Some(found) = c.find(name) {
                return Some(found);
            }
        }
        None
    }

    /// Todos os descendentes com o nome local dado, em ordem de documento.
    pub fn find_all<'a>(&'a self, name: &str, out: &mut Vec<&'a Element>) {
        for c in &self.children {
            if c.name == name {
                out.push(c);
            }
            c.find_all(name, out);
        }
    }

    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    pub fn text_trimmed(&self) -> &str {
        self.text.trim()
    }
}

/// Lê o documento inteiro e devolve o elemento raiz.
pub fn parse(xml: &str) -> Result<Element, XmlError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);
    let mut stack: Vec<Element> = Vec::new();
    let mut root: Option<Element> = None;

    loop {
        let event = reader.read_event().map_err(|e| XmlError(e.to_string()))?;
        match event {
            Event::Start(e) => {
                if stack.len() >= MAX_DEPTH {
                    return Err(XmlError("profundidade excessiva".into()));
                }
                stack.push(start_element(&e)?);
            }
            Event::Empty(e) => {
                let el = start_element(&e)?;
                attach(&mut stack, &mut root, el)?;
            }
            Event::End(_) => {
                let el = stack
                    .pop()
                    .ok_or_else(|| XmlError("fechamento sem abertura".into()))?;
                attach(&mut stack, &mut root, el)?;
            }
            Event::Text(t) => {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(t.as_ref());
                }
            }
            Event::CData(t) => {
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(t.as_ref());
                }
            }
            Event::GeneralRef(r) => {
                let resolved = if r.is_char_ref() {
                    r.resolve_char_ref()
                        .map_err(|e| XmlError(e.to_string()))?
                        .map(String::from)
                } else {
                    predefined_entity(r.as_ref()).map(String::from)
                };
                let text = resolved.ok_or_else(|| XmlError("entidade desconhecida".into()))?;
                if let Some(top) = stack.last_mut() {
                    top.text.push_str(&text);
                }
            }
            Event::DocType(_) => return Err(XmlError("DOCTYPE não é aceito".into())),
            Event::Eof => break,
            _ => {}
        }
    }
    if !stack.is_empty() {
        return Err(XmlError("documento incompleto".into()));
    }
    root.ok_or_else(|| XmlError("documento vazio".into()))
}

fn attach(stack: &mut [Element], root: &mut Option<Element>, el: Element) -> Result<(), XmlError> {
    match stack.last_mut() {
        Some(parent) => parent.children.push(el),
        None if root.is_none() => *root = Some(el),
        None => return Err(XmlError("mais de um elemento raiz".into())),
    }
    Ok(())
}

fn start_element(e: &BytesStart<'_>) -> Result<Element, XmlError> {
    let name = e.local_name().as_ref().to_string();
    let mut attrs = Vec::new();
    for a in e.attributes() {
        let a = a.map_err(|e| XmlError(e.to_string()))?;
        let key = a.key.local_name().as_ref().to_string();
        let value = a
            .normalized_value(quick_xml::XmlVersion::default())
            .map_err(|e| XmlError(e.to_string()))?
            .into_owned();
        attrs.push((key, value));
    }
    Ok(Element {
        name,
        attrs,
        ..Default::default()
    })
}

fn predefined_entity(name: &str) -> Option<&'static str> {
    Some(match name {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        _ => return None,
    })
}

/// Escapa texto para inclusão em conteúdo ou atributo XML.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ignora_prefixos_e_resolve_entidades() {
        let doc = parse(
            r#"<?xml version="1.0"?><s:Envelope xmlns:s="x"><s:Body><tds:Resp xmlns:tds="y">
               <tds:Model a:k="v&amp;w" xmlns:a="z">C200 &amp; &#65;</tds:Model></tds:Resp></s:Body></s:Envelope>"#,
        )
        .unwrap();
        assert_eq!(doc.name, "Envelope");
        let model = doc.find("Model").unwrap();
        assert_eq!(model.text_trimmed(), "C200 & A");
        assert_eq!(model.attr("k"), Some("v&w"));
        assert!(doc.path(&["Body", "Resp", "Model"]).is_some());
    }

    #[test]
    fn recusa_doctype_e_documentos_quebrados() {
        assert!(parse("<!DOCTYPE x [<!ENTITY a 'b'>]><x/>").is_err());
        assert!(parse("<a><b></a>").is_err());
        assert!(parse("<a/><b/>").is_err());
        assert!(parse("").is_err());
        assert!(parse("<a>&desconhecida;</a>").is_err());
    }

    #[test]
    fn recusa_profundidade_excessiva() {
        let xml = "<a>".repeat(100) + &"</a>".repeat(100);
        assert!(parse(&xml).is_err());
    }

    #[test]
    fn escapa_texto() {
        assert_eq!(escape(r#"a&b<c>"d'"#), "a&amp;b&lt;c&gt;&quot;d&apos;");
    }

    #[test]
    fn busca_todos_os_descendentes() {
        let doc = parse("<r><p t='1'/><x><p t='2'/></x></r>").unwrap();
        let mut out = vec![];
        doc.find_all("p", &mut out);
        assert_eq!(
            out.iter().map(|e| e.attr("t").unwrap()).collect::<Vec<_>>(),
            ["1", "2"]
        );
    }
}
