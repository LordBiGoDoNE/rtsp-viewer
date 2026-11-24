//! Gera o `global Theme` do Slint a partir de `design/tokens.json` (ADR 0004)
//! e compila a UI. Nada gerado é versionado: o tema sai no `OUT_DIR`.

use std::path::PathBuf;

#[path = "build_support/theme.rs"]
mod theme;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let tokens_path = manifest.join("../../../design/tokens.json");
    println!("cargo:rerun-if-changed={}", tokens_path.display());
    println!("cargo:rerun-if-changed=build_support/theme.rs");

    let text = std::fs::read_to_string(&tokens_path)
        .unwrap_or_else(|e| panic!("não foi possível ler design/tokens.json: {e}"));
    let theme =
        theme::generate_theme(&text).unwrap_or_else(|e| panic!("design/tokens.json inválido: {e}"));
    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    std::fs::write(out_dir.join("theme.slint"), theme).expect("gravar theme.slint");

    let config = slint_build::CompilerConfiguration::new().with_include_paths(vec![out_dir]);
    slint_build::compile_with_config("ui/app.slint", config).expect("falha ao compilar a UI Slint");
}
