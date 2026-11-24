#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod boot;
mod infrastructure;
mod presentation;
mod ui;

use std::path::PathBuf;

use infrastructure::credentials::{KeyringBackend, SystemCredentialStore};
use infrastructure::json_repository::JsonCameraRepository;
use infrastructure::memory_repository::MemoryCameraRepository;
use rtsp_viewer_core::application::CameraCatalog;
use rtsp_viewer_media::FfmpegPlayer;
use slint::ComponentHandle;

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let trace = boot::BootTrace::from_env();
    let window = AppWindow::new()?;

    // Lê só o JSON da lista. Cofre de senhas e FFmpeg ficam fora do caminho
    // do boot: o cofre abre no primeiro uso; o FFmpeg inicia em segundo plano.
    let (catalog, banner) = load_catalog();
    std::thread::spawn(|| {
        if rtsp_viewer_media::init().is_ok() {
            rtsp_viewer_media::warm_up_hardware();
        }
    });

    let _controller =
        ui::Controller::attach(&window, catalog, Box::new(FfmpegPlayer::default()), banner);
    trace.attach(window.window());
    window.run()
}

/// Diretório da configuração; `RTSP_VIEWER_CONFIG_DIR` permite isolar testes
/// e medições sem tocar na lista real do usuário.
fn config_dir() -> Option<PathBuf> {
    std::env::var_os("RTSP_VIEWER_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(JsonCameraRepository::default_dir)
}

fn credentials() -> Box<SystemCredentialStore<KeyringBackend>> {
    Box::new(SystemCredentialStore::new(KeyringBackend))
}

fn load_catalog() -> (CameraCatalog, Option<String>) {
    let failure = match config_dir() {
        Some(dir) => {
            match CameraCatalog::load(Box::new(JsonCameraRepository::new(dir)), credentials()) {
                Ok((catalog, warning)) => {
                    return (catalog, warning.as_ref().map(presentation::banner_for));
                }
                Err(e) => e.to_string(),
            }
        }
        None => "diretório de configuração do usuário não encontrado".to_string(),
    };
    let (catalog, _) =
        CameraCatalog::load(Box::new(MemoryCameraRepository::default()), credentials())
            .expect("repositório em memória sempre carrega");
    (
        catalog,
        Some(format!(
            "Não foi possível abrir a lista de câmeras ({failure}). As alterações desta sessão não serão salvas."
        )),
    )
}
