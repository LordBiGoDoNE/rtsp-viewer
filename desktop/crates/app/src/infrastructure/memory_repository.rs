//! Lista de câmeras só em memória. Usada quando o arquivo de configuração não
//! pode ser lido nem preservado (as alterações da sessão não são salvas), e
//! nos testes da UI.

use std::sync::Mutex;

use rtsp_viewer_core::application::ports::{CameraRepository, LoadedCameras, RepositoryError};
use rtsp_viewer_core::domain::Camera;

#[derive(Default)]
pub struct MemoryCameraRepository {
    cameras: Mutex<Vec<Camera>>,
}

impl CameraRepository for MemoryCameraRepository {
    fn load(&self) -> Result<LoadedCameras, RepositoryError> {
        let cameras = self.cameras.lock().unwrap_or_else(|e| e.into_inner()).clone();
        Ok(LoadedCameras { cameras, warning: None })
    }

    fn save(&self, cameras: &[Camera]) -> Result<(), RepositoryError> {
        *self.cameras.lock().unwrap_or_else(|e| e.into_inner()) = cameras.to_vec();
        Ok(())
    }
}
