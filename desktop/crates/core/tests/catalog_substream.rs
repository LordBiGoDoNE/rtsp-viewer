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
        url: "rtsp://192.168.1.164:554/stream1".into(),
        substream: sub,
        ..Default::default()
    }
}

#[test]
fn cadastra_com_secundario_sem_credenciais() {
    let (mut c, mem) = catalog();
    let id = c
        .add(draft(SubstreamInput::Set(
            "rtsp://admin:segredo@192.168.1.164:554/stream2".into(),
        )))
        .unwrap();
    let cam = c.get(&id).unwrap();
    assert_eq!(
        cam.substream().unwrap().to_string(),
        "rtsp://192.168.1.164:554/stream2"
    );
    assert_eq!(mem.cameras.lock().unwrap()[0].substream(), cam.substream());
}

#[test]
fn editar_sem_mexer_mantem_e_clear_remove() {
    let (mut c, _) = catalog();
    let id = c
        .add(draft(SubstreamInput::Set(
            "rtsp://192.168.1.164/stream2".into(),
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

fn draft_url(url: &str, sub: SubstreamInput) -> CameraDraft {
    CameraDraft {
        url: url.into(),
        ..draft(sub)
    }
}

fn com_secundario() -> (CameraCatalog, &'static Memory, CameraId) {
    let (mut c, mem) = catalog();
    let id = c
        .add(draft(SubstreamInput::Set(
            "rtsp://192.168.1.164:554/stream2".into(),
        )))
        .unwrap();
    (c, mem, id)
}

#[test]
fn trocar_url_no_mesmo_host_sem_mexer_descarta_secundario() {
    let (mut c, mem, id) = com_secundario();
    c.edit(
        &id,
        draft_url("rtsp://192.168.1.164:554/h264", SubstreamInput::Unchanged),
    )
    .unwrap();
    assert!(c.get(&id).unwrap().substream().is_none());
    assert!(mem.cameras.lock().unwrap()[0].substream().is_none());
}

#[test]
fn trocar_host_sem_mexer_descarta_secundario() {
    let (mut c, mem, id) = com_secundario();
    c.edit(
        &id,
        draft_url(
            "rtsp://192.168.1.121:554/stream1",
            SubstreamInput::Unchanged,
        ),
    )
    .unwrap();
    assert!(c.get(&id).unwrap().substream().is_none());
    assert!(mem.cameras.lock().unwrap()[0].substream().is_none());
}

#[test]
fn trocar_url_com_set_explicito_grava_o_novo_secundario() {
    let (mut c, _, id) = com_secundario();
    c.edit(
        &id,
        draft_url(
            "rtsp://192.168.1.121:554/stream1",
            SubstreamInput::Set("rtsp://192.168.1.121:554/stream2".into()),
        ),
    )
    .unwrap();
    assert_eq!(
        c.get(&id).unwrap().substream().unwrap().to_string(),
        "rtsp://192.168.1.121:554/stream2"
    );
}

#[test]
fn secundario_de_outro_host_e_erro_de_campo_e_nada_muda() {
    let (mut c, mem, id) = com_secundario();
    let err = c
        .edit(
            &id,
            draft_url(
                "rtsp://192.168.1.121:554/stream1",
                SubstreamInput::Set("rtsp://192.168.1.164:554/stream2".into()),
            ),
        )
        .unwrap_err();
    let CatalogError::Invalid(fields) = err else {
        panic!("{err:?}")
    };
    assert!(
        fields[0].message.starts_with("Stream secundário:"),
        "{fields:?}"
    );
    let cam = c.get(&id).unwrap();
    assert_eq!(cam.url().to_string(), "rtsp://192.168.1.164:554/stream1");
    assert!(cam.substream().is_some());
    assert_eq!(
        mem.cameras.lock().unwrap()[0].url().to_string(),
        "rtsp://192.168.1.164:554/stream1"
    );

    let (mut c, mem) = catalog();
    assert!(matches!(
        c.add(draft(SubstreamInput::Set("rtsp://10.0.0.5/stream2".into()))),
        Err(CatalogError::Invalid(_))
    ));
    assert!(mem.cameras.lock().unwrap().is_empty());
}
