//! Stream secundário no catálogo (spec `camera-registry`, "Stream secundário
//! opcional").

use std::collections::HashMap;
use std::sync::Mutex;

use rtsp_viewer_core::application::ports::{
    CameraRepository, CredentialError, CredentialStore, LoadedCameras, RepositoryError,
};
use rtsp_viewer_core::application::{CameraCatalog, CameraDraft, CatalogError, SubstreamInput};
use rtsp_viewer_core::domain::{Camera, CameraId, Password};

#[derive(Default)]
struct Memory {
    cameras: Mutex<Vec<Camera>>,
    secrets: Mutex<HashMap<CameraId, Password>>,
}

struct Repo<'a>(&'a Memory);
struct Vault<'a>(&'a Memory);

impl CameraRepository for Repo<'static> {
    fn load(&self) -> Result<LoadedCameras, RepositoryError> {
        Ok(LoadedCameras {
            cameras: self.0.cameras.lock().unwrap().clone(),
            warning: None,
        })
    }
    fn save(&self, cameras: &[Camera]) -> Result<(), RepositoryError> {
        *self.0.cameras.lock().unwrap() = cameras.to_vec();
        Ok(())
    }
}

impl CredentialStore for Vault<'static> {
    fn get(&self, id: &CameraId) -> Result<Option<Password>, CredentialError> {
        Ok(self.0.secrets.lock().unwrap().get(id).cloned())
    }
    fn set(&self, id: &CameraId, p: &Password) -> Result<(), CredentialError> {
        self.0.secrets.lock().unwrap().insert(id.clone(), p.clone());
        Ok(())
    }
    fn delete(&self, id: &CameraId) -> Result<(), CredentialError> {
        self.0.secrets.lock().unwrap().remove(id);
        Ok(())
    }
    fn is_persistent(&self) -> bool {
        true
    }
}

fn catalog() -> (CameraCatalog, &'static Memory) {
    let mem: &'static Memory = Box::leak(Box::default());
    let (c, _) = CameraCatalog::load(Box::new(Repo(mem)), Box::new(Vault(mem))).unwrap();
    (c, mem)
}

fn draft(sub: SubstreamInput) -> CameraDraft {
    CameraDraft {
        name: "Portão".into(),
        url: "rtsp://192.168.1.120:554/stream1".into(),
        substream: sub,
        ..Default::default()
    }
}

#[test]
fn cadastra_com_secundario_sem_credenciais() {
    let (mut c, mem) = catalog();
    let id = c
        .add(draft(SubstreamInput::Set(
            "rtsp://admin:segredo@192.168.1.120:554/stream2".into(),
        )))
        .unwrap();
    let cam = c.get(&id).unwrap();
    assert_eq!(
        cam.substream().unwrap().to_string(),
        "rtsp://192.168.1.120:554/stream2"
    );
    assert_eq!(mem.cameras.lock().unwrap()[0].substream(), cam.substream());
}

#[test]
fn editar_sem_mexer_mantem_e_clear_remove() {
    let (mut c, _) = catalog();
    let id = c
        .add(draft(SubstreamInput::Set(
            "rtsp://192.168.1.120/stream2".into(),
        )))
        .unwrap();
    c.edit(&id, draft(SubstreamInput::Unchanged)).unwrap();
    assert!(c.get(&id).unwrap().substream().is_some());
    c.edit(&id, draft(SubstreamInput::Clear)).unwrap();
    assert!(c.get(&id).unwrap().substream().is_none());
}

#[test]
fn secundario_invalido_e_erro_de_campo() {
    let (mut c, mem) = catalog();
    let err = c
        .add(draft(SubstreamInput::Set("http://x".into())))
        .unwrap_err();
    let CatalogError::Invalid(fields) = err else {
        panic!("{err:?}")
    };
    assert!(
        fields[0].message.starts_with("Stream secundário:"),
        "{fields:?}"
    );
    assert!(mem.cameras.lock().unwrap().is_empty());
}

#[test]
fn camera_nova_sem_secundario() {
    let (mut c, _) = catalog();
    let id = c.add(draft(SubstreamInput::Unchanged)).unwrap();
    assert!(c.get(&id).unwrap().substream().is_none());
}
