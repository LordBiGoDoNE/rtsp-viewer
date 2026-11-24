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

/// Cores só pelo `Theme`: nenhum `.slint` do app usa `#hex`, `rgb()`, `rgba()`
/// ou `hsv()` (a palavra `transparent` é permitida).
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
            let code = line.split("//").next().unwrap_or_default();
            if has_hex_color(code) || ["rgb(", "rgba(", "hsv("].iter().any(|f| code.contains(f)) {
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

fn has_hex_color(code: &str) -> bool {
    code.match_indices('#').any(|(i, _)| {
        let digits = code[i + 1..]
            .chars()
            .take_while(|c| c.is_ascii_hexdigit())
            .count();
        matches!(digits, 3 | 4 | 6 | 8)
    })
}

#[test]
fn detector_de_cor_literal() {
    assert!(has_hex_color("background: #ff0000;"));
    assert!(has_hex_color("color: #fff;"));
    assert!(!has_hex_color("background: Theme.color-surface;"));
}
