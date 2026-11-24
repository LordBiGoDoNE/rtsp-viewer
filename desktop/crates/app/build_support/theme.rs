//! Geração do tema Slint a partir de design/tokens.json (ADR 0004).
//! Compartilhado pelo build.rs e pelos testes (tests/theme_tokens.rs).

use std::collections::BTreeMap;

/// Tokens obrigatórios por grupo e o tipo Slint de cada grupo.
const REQUIRED: &[(&str, Kind, &[&str])] = &[
    (
        "color",
        Kind::Color,
        &[
            "background",
            "surface",
            "surface-raised",
            "surface-hover",
            "border",
            "text",
            "text-muted",
            "primary",
            "primary-hover",
            "on-primary",
            "danger",
            "on-danger",
            "warning",
            "success",
            "video-background",
            "overlay",
            "focus",
        ],
    ),
    (
        "font",
        Kind::FontOrLength,
        &[
            "size-small",
            "size-body",
            "size-title",
            "size-display",
            "weight-regular",
            "weight-bold",
        ],
    ),
    ("space", Kind::Length, &["xs", "s", "m", "l", "xl"]),
    ("radius", Kind::Length, &["s", "m"]),
    (
        "size",
        Kind::Length,
        &[
            "control-height",
            "list-item-height",
            "toolbar-height",
            "form-width",
            "dialog-width",
            "status-width",
            "info-width",
            "about-width",
            "window-width",
            "window-height",
            "window-min-width",
            "window-min-height",
        ],
    ),
];

#[derive(Clone, Copy)]
enum Kind {
    Color,
    Length,
    /// `weight-*` vira `int`; o resto, `length`.
    FontOrLength,
}

pub fn generate_theme(text: &str) -> Result<String, String> {
    let json: serde_json::Value =
        serde_json::from_str(text).map_err(|e| format!("JSON malformado: {e}"))?;

    let mut out = String::from(
        "// Gerado pelo build.rs a partir de design/tokens.json. Não editar.\n\
         export global Theme {\n",
    );
    for (group, kind, names) in REQUIRED {
        let values: BTreeMap<String, serde_json::Value> = json
            .get(group)
            .and_then(|g| g.as_object())
            .ok_or_else(|| format!("grupo \"{group}\" ausente"))?
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for name in *names {
            let token = format!("{group}.{name}");
            let value = values
                .get(*name)
                .ok_or_else(|| format!("token \"{token}\" ausente"))?;
            let (ty, literal) = slint_value(&token, name, *kind, value)?;
            out.push_str(&format!(
                "    out property <{ty}> {group}-{name}: {literal};\n"
            ));
        }
    }
    out.push_str("}\n");
    Ok(out)
}

fn slint_value(
    token: &str,
    name: &str,
    kind: Kind,
    value: &serde_json::Value,
) -> Result<(&'static str, String), String> {
    match kind {
        Kind::Color => {
            let s = value
                .as_str()
                .ok_or_else(|| format!("token \"{token}\" deve ser texto"))?;
            let hex = s.strip_prefix('#').filter(|h| {
                (h.len() == 6 || h.len() == 8) && h.chars().all(|c| c.is_ascii_hexdigit())
            });
            match hex {
                Some(h) => Ok(("color", format!("#{h}"))),
                None => Err(format!(
                    "token \"{token}\" deve ser #RRGGBB ou #RRGGBBAA, veio {s:?}"
                )),
            }
        }
        Kind::Length | Kind::FontOrLength => {
            let n = value
                .as_f64()
                .filter(|n| *n > 0.0 && n.is_finite())
                .ok_or_else(|| format!("token \"{token}\" deve ser número positivo"))?;
            if matches!(kind, Kind::FontOrLength) && name.starts_with("weight-") {
                Ok(("int", format!("{}", n.round() as i64)))
            } else {
                Ok(("length", format!("{n}px")))
            }
        }
    }
}
