//! Tokens de design (spec `design-tokens`): o tema é gerado de
//! design/tokens.json, token inválido falha com o nome dele, e a UI não usa
//! cores literais.

use std::path::Path;

#[path = "../build_support/theme.rs"]
mod theme;

fn tokens_text() -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../design/tokens.json"),
    )
    .expect("design/tokens.json")
}

fn with_token(group: &str, name: &str, value: serde_json::Value) -> String {
    let mut json: serde_json::Value = serde_json::from_str(&tokens_text()).unwrap();
    json[group][name] = value;
    json.to_string()
}

fn without_token(group: &str, name: &str) -> String {
    let mut json: serde_json::Value = serde_json::from_str(&tokens_text()).unwrap();
    json[group].as_object_mut().unwrap().remove(name);
    json.to_string()
}

#[test]
fn tokens_atuais_geram_o_tema() {
    let theme = theme::generate_theme(&tokens_text()).expect("tokens válidos");
    assert!(theme.contains("export global Theme"));
    assert!(theme.contains("out property <color> color-primary: #3D8BFF;"));
    assert!(theme.contains("out property <length> space-l: 16px;"));
    assert!(theme.contains("out property <int> font-weight-bold: 600;"));
}

#[test]
fn alterar_cor_muda_o_tema() {
    let theme = theme::generate_theme(&with_token("color", "primary", "#112233".into())).unwrap();
    assert!(theme.contains("color-primary: #112233;"));
}

#[test]
fn cor_malformada_falha_com_o_nome_do_token() {
    let err = theme::generate_theme(&with_token("color", "danger", "vermelho".into())).unwrap_err();
    assert!(err.contains("color.danger"), "{err}");
    let err = theme::generate_theme(&with_token("color", "danger", "#12345".into())).unwrap_err();
    assert!(err.contains("color.danger"), "{err}");
}

#[test]
fn medida_invalida_falha_com_o_nome_do_token() {
    let err = theme::generate_theme(&with_token("space", "m", (-4).into())).unwrap_err();
    assert!(err.contains("space.m"), "{err}");
    let err = theme::generate_theme(&with_token("size", "form-width", "420".into())).unwrap_err();
    assert!(err.contains("size.form-width"), "{err}");
}

#[test]
fn token_obrigatorio_ausente_falha() {
    let err = theme::generate_theme(&without_token("radius", "m")).unwrap_err();
    assert!(err.contains("radius.m"), "{err}");
}

#[test]
fn json_malformado_falha() {
    assert!(
        theme::generate_theme("{ nao e json")
            .unwrap_err()
            .contains("JSON")
    );
}

/// Cores só pelo `Theme`: nenhum `.slint` do app usa `#hex`, `rgb()`, `rgba()`,
/// `hsv()`, cores nomeadas (`red`, `white`…) ou o namespace `Colors.` (a
/// palavra `transparent` é permitida).
#[test]
fn ui_nao_usa_cor_literal() {
    let ui = Path::new(env!("CARGO_MANIFEST_DIR")).join("ui");
    let mut violations = vec![];
    for entry in std::fs::read_dir(&ui).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|e| e != "slint") {
            continue;
        }
        let text = std::fs::read_to_string(&path).unwrap();
        for (n, line) in text.lines().enumerate() {
            if has_literal_color(line) {
                violations.push(format!("{}:{}: {}", path.display(), n + 1, line.trim()));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "cores literais na UI:\n{}",
        violations.join("\n")
    );
}

/// Linha de `.slint` com cor literal, ignorando textos entre aspas e comentários.
fn has_literal_color(line: &str) -> bool {
    let code = code_only(line);
    has_hex_color(&code)
        || ["rgb(", "rgba(", "hsv(", "Colors."]
            .iter()
            .any(|f| code.contains(f))
        || has_named_color(&code)
}

/// Remove o conteúdo de strings e o comentário de linha.
fn code_only(line: &str) -> String {
    let mut out = String::new();
    let mut in_string = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if in_string {
            match c {
                '\\' => {
                    chars.next();
                }
                '"' => {
                    in_string = false;
                    out.push('"');
                }
                _ => {}
            }
        } else if c == '"' {
            in_string = true;
            out.push('"');
        } else if c == '/' && chars.peek() == Some(&'/') {
            break;
        } else {
            out.push(c);
        }
    }
    out
}

fn has_hex_color(code: &str) -> bool {
    code.match_indices('#').any(|(i, _)| {
        let digits = code[i + 1..]
            .chars()
            .take_while(|c| c.is_ascii_hexdigit())
            .count();
        matches!(digits, 3 | 4 | 6 | 8)
    })
}

/// Identificador solto (não precedido de `.` nem parte de um nome com `-`)
/// igual a uma cor nomeada do CSS, que o Slint aceita como cor.
fn has_named_color(code: &str) -> bool {
    let is_ident = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '-';
    let mut start = None;
    for (i, c) in code
        .char_indices()
        .chain(std::iter::once((code.len(), ' ')))
    {
        match (start, is_ident(c)) {
            (None, true) => start = Some(i),
            (Some(s), false) => {
                start = None;
                let word = &code[s..i];
                let qualified = code[..s].trim_end().ends_with('.');
                if !qualified && NAMED_COLORS.contains(&word.to_ascii_lowercase().as_str()) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// Cores nomeadas do CSS (sem `transparent`, que é permitida).
const NAMED_COLORS: &[&str] = &[
    "aliceblue",
    "antiquewhite",
    "aqua",
    "aquamarine",
    "azure",
    "beige",
    "bisque",
    "black",
    "blanchedalmond",
    "blue",
    "blueviolet",
    "brown",
    "burlywood",
    "cadetblue",
    "chartreuse",
    "chocolate",
    "coral",
    "cornflowerblue",
    "cornsilk",
    "crimson",
    "cyan",
    "darkblue",
    "darkcyan",
    "darkgoldenrod",
    "darkgray",
    "darkgreen",
    "darkgrey",
    "darkkhaki",
    "darkmagenta",
    "darkolivegreen",
    "darkorange",
    "darkorchid",
    "darkred",
    "darksalmon",
    "darkseagreen",
    "darkslateblue",
    "darkslategray",
    "darkslategrey",
    "darkturquoise",
    "darkviolet",
    "deeppink",
    "deepskyblue",
    "dimgray",
    "dimgrey",
    "dodgerblue",
    "firebrick",
    "floralwhite",
    "forestgreen",
    "fuchsia",
    "gainsboro",
    "ghostwhite",
    "gold",
    "goldenrod",
    "gray",
    "green",
    "greenyellow",
    "grey",
    "honeydew",
    "hotpink",
    "indianred",
    "indigo",
    "ivory",
    "khaki",
    "lavender",
    "lavenderblush",
    "lawngreen",
    "lemonchiffon",
    "lightblue",
    "lightcoral",
    "lightcyan",
    "lightgoldenrodyellow",
    "lightgray",
    "lightgreen",
    "lightgrey",
    "lightpink",
    "lightsalmon",
    "lightseagreen",
    "lightskyblue",
    "lightslategray",
    "lightslategrey",
    "lightsteelblue",
    "lightyellow",
    "lime",
    "limegreen",
    "linen",
    "magenta",
    "maroon",
    "mediumaquamarine",
    "mediumblue",
    "mediumorchid",
    "mediumpurple",
    "mediumseagreen",
    "mediumslateblue",
    "mediumspringgreen",
    "mediumturquoise",
    "mediumvioletred",
    "midnightblue",
    "mintcream",
    "mistyrose",
    "moccasin",
    "navajowhite",
    "navy",
    "oldlace",
    "olive",
    "olivedrab",
    "orange",
    "orangered",
    "orchid",
    "palegoldenrod",
    "palegreen",
    "paleturquoise",
    "palevioletred",
    "papayawhip",
    "peachpuff",
    "peru",
    "pink",
    "plum",
    "powderblue",
    "purple",
    "rebeccapurple",
    "red",
    "rosybrown",
    "royalblue",
    "saddlebrown",
    "salmon",
    "sandybrown",
    "seagreen",
    "seashell",
    "sienna",
    "silver",
    "skyblue",
    "slateblue",
    "slategray",
    "slategrey",
    "snow",
    "springgreen",
    "steelblue",
    "tan",
    "teal",
    "thistle",
    "tomato",
    "turquoise",
    "violet",
    "wheat",
    "white",
    "whitesmoke",
    "yellow",
    "yellowgreen",
];

#[test]
fn detector_de_cor_literal() {
    for bad in [
        "background: #ff0000;",
        "color: #fff;",
        "color: rgb(255, 0, 0);",
        "background: red;",
        "border-color: White;",
        "fill: black; // contorno",
        "stroke: Colors.blue;",
        "in property <color> destaque: orange;",
        "background: cond ? red : Theme.color-surface;",
    ] {
        assert!(has_literal_color(bad), "deveria detectar: {bad}");
    }
    for ok in [
        "background: Theme.color-surface;",
        "background: transparent;",
        "color: Theme.color-text; // red é só comentário",
        "text: \"Fundo red e #fff no texto\";",
        "border-color: Theme.color-red;",
        "in property <bool> red-alert;",
        "url: \"rtsp://h/s\"; background: Theme.color-bg;",
    ] {
        assert!(!has_literal_color(ok), "não deveria detectar: {ok}");
    }
}
