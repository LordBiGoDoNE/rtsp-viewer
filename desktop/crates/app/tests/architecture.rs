//! Teste de arquitetura: o crate `core` não depende de UI, FFmpeg, cofre,
//! disco nem rede (docs/adr/0001-stack.md, spec `desktop-distribution`).

use std::path::Path;
use std::process::Command;

const CORE: &str = "rtsp-viewer-core";

/// Prefixos de crates proibidos no `core` (diretos ou transitivos).
const FORBIDDEN: &[&str] = &[
    "slint",
    "ffmpeg",
    "keyring",
    "windows-native-keyring-store",
    "zbus",
    "directories",
    "dirs",
    "serde_json",
    "tokio",
    "async-std",
    "reqwest",
    "hyper",
    "rtsp-viewer-media",
    "rtsp",
];

/// APIs de I/O proibidas no código-fonte do `core`.
const FORBIDDEN_APIS: &[&str] = &["std::fs", "std::net", "std::process", "std::env"];

fn metadata() -> serde_json::Value {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let out = Command::new(cargo)
        .args(["metadata", "--format-version", "1", "--locked"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo metadata");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("JSON do cargo metadata")
}

fn core_dependency_closure(meta: &serde_json::Value) -> Vec<String> {
    let packages = meta["packages"].as_array().unwrap();
    let nodes = meta["resolve"]["nodes"].as_array().unwrap();
    let name_of = |id: &str| -> String {
        packages
            .iter()
            .find(|p| p["id"] == id)
            .map(|p| p["name"].as_str().unwrap().to_string())
            .unwrap()
    };
    let core_id = packages
        .iter()
        .find(|p| p["name"] == CORE)
        .map(|p| p["id"].as_str().unwrap().to_string())
        .expect("pacote core");

    let mut seen = vec![];
    let mut stack = vec![core_id.clone()];
    while let Some(id) = stack.pop() {
        let node = nodes.iter().find(|n| n["id"] == id.as_str()).unwrap();
        for dep in node["deps"].as_array().unwrap() {
            // Só dependências normais (não dev/build) contam.
            let normal = dep["dep_kinds"]
                .as_array()
                .unwrap()
                .iter()
                .any(|k| k["kind"].is_null());
            let dep_id = dep["pkg"].as_str().unwrap().to_string();
            if normal && !seen.contains(&dep_id) {
                seen.push(dep_id.clone());
                stack.push(dep_id);
            }
        }
    }
    seen.iter().map(|id| name_of(id)).collect()
}

#[test]
fn core_nao_depende_de_adaptadores() {
    let deps = core_dependency_closure(&metadata());
    let violations: Vec<_> = deps
        .iter()
        .filter(|d| FORBIDDEN.iter().any(|f| d.starts_with(f)))
        .collect();
    assert!(violations.is_empty(), "core depende de: {violations:?}");
}

#[test]
fn core_nao_usa_apis_de_io() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../core/src");
    let mut violations = vec![];
    visit(&src, &mut |path, text| {
        for api in FORBIDDEN_APIS {
            if text.contains(api) {
                violations.push(format!("{}: {api}", path.display()));
            }
        }
    });
    assert!(violations.is_empty(), "core usa I/O: {violations:?}");
}

fn visit(dir: &Path, f: &mut dyn FnMut(&Path, &str)) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            visit(&path, f);
        } else if path.extension().is_some_and(|e| e == "rs") {
            f(&path, &std::fs::read_to_string(&path).unwrap());
        }
    }
}
