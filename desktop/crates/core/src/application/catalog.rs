//! Caso de uso: manter a lista de câmeras e preparar a conexão.

use crate::application::ports::{
    CameraRepository, CredentialStore, RepositoryWarning, StreamRequest,
};
use crate::domain::{Camera, CameraId, CameraName, Password, RtspUrl};

/// Dados do formulário de câmera, como digitados.
#[derive(Debug, Clone, Default)]
pub struct CameraDraft {
    pub name: String,
    pub url: String,
    pub username: String,
    pub password: PasswordInput,
}

/// O que fazer com a senha ao salvar.
#[derive(Debug, Clone, Default)]
pub enum PasswordInput {
    /// Manter a senha atual (ou nenhuma, numa câmera nova).
    #[default]
    Unchanged,
    /// Definir uma nova senha; vazia equivale a `Clear`.
    Set(Password),
    Clear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Name,
    Url,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldError {
    pub field: Field,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CatalogError {
    #[error("Corrija os campos destacados.")]
    Invalid(Vec<FieldError>),
    #[error("Câmera não encontrada.")]
    NotFound,
    #[error("Não foi possível salvar a lista de câmeras: {0}")]
    Storage(String),
    #[error("{0}")]
    Credentials(String),
    /// A câmera nova foi salva (com este `id`), mas a senha não chegou ao
    /// cofre. A UI deve tratar a câmera como criada e oferecer reaplicar a
    /// senha com `edit`, sem chamar `add` de novo.
    #[error("Câmera salva, mas a senha não pôde ser guardada no cofre: {reason}")]
    AddedWithoutPassword { id: CameraId, reason: String },
    #[error("Informe a senha da câmera.")]
    NeedsPassword,
}

/// Lista de câmeras em memória, sincronizada com o repositório e o cofre.
pub struct CameraCatalog {
    repository: Box<dyn CameraRepository>,
    credentials: Box<dyn CredentialStore>,
    cameras: Vec<Camera>,
}

struct ValidDraft {
    name: CameraName,
    url: RtspUrl,
    username: Option<String>,
    password: PasswordInput,
}

impl CameraCatalog {
    /// Carrega a lista. O aviso (arquivo corrompido recuperado) deve ser
    /// mostrado ao usuário.
    pub fn load(
        repository: Box<dyn CameraRepository>,
        credentials: Box<dyn CredentialStore>,
    ) -> Result<(Self, Option<RepositoryWarning>), CatalogError> {
        let loaded = repository.load().map_err(|e| CatalogError::Storage(e.0))?;
        Ok((
            Self {
                repository,
                credentials,
                cameras: loaded.cameras,
            },
            loaded.warning,
        ))
    }

    pub fn cameras(&self) -> &[Camera] {
        &self.cameras
    }

    pub fn get(&self, id: &CameraId) -> Option<&Camera> {
        self.cameras.iter().find(|c| c.id() == id)
    }

    /// `false` quando as senhas não sobrevivem ao fechamento do app.
    pub fn passwords_are_persistent(&self) -> bool {
        self.credentials.is_persistent()
    }

    pub fn add(&mut self, draft: CameraDraft) -> Result<CameraId, CatalogError> {
        let valid = validate(draft)?;
        let camera = Camera::new(valid.name, valid.url, valid.username);
        let id = camera.id().clone();
        // Sem usuário, a senha não é usada: não vai para o cofre.
        let password = if camera.requires_credentials() {
            valid.password
        } else {
            PasswordInput::Unchanged
        };
        let mut next = self.cameras.clone();
        next.push(camera);
        self.persist(next)?;
        self.apply_password(&id, password).map_err(|e| match e {
            CatalogError::Credentials(reason) => CatalogError::AddedWithoutPassword {
                id: id.clone(),
                reason,
            },
            other => other,
        })?;
        Ok(id)
    }

    pub fn edit(&mut self, id: &CameraId, draft: CameraDraft) -> Result<(), CatalogError> {
        let idx = self.index_of(id)?;
        let valid = validate(draft)?;
        let mut next = self.cameras.clone();
        let camera = &mut next[idx];
        camera.rename(valid.name);
        camera.change_url(valid.url);
        camera.change_username(valid.username);
        // Câmera que passa a não ter usuário não deve manter senha no cofre.
        let password = if camera.requires_credentials() {
            valid.password
        } else {
            PasswordInput::Clear
        };
        self.persist(next)?;
        self.apply_password(id, password).map_err(|e| match e {
            CatalogError::Credentials(reason) => CatalogError::Credentials(format!(
                "Câmera salva, mas a senha não pôde ser guardada no cofre: {reason}"
            )),
            other => other,
        })
    }

    /// Remove a câmera da lista e a senha dela do cofre. A lista é gravada
    /// primeiro; se só a senha falhar, a câmera já saiu da lista e o erro diz
    /// isso.
    pub fn remove(&mut self, id: &CameraId) -> Result<(), CatalogError> {
        let idx = self.index_of(id)?;
        let mut next = self.cameras.clone();
        next.remove(idx);
        self.persist(next)?;
        self.credentials.delete(id).map_err(|e| {
            CatalogError::Credentials(format!(
                "Câmera removida, mas a senha não pôde ser apagada do cofre: {e}"
            ))
        })
    }

    /// Monta o pedido de reprodução. `typed_password` é a senha digitada pelo
    /// usuário quando o app a pediu (`NeedsPassword`); ela fica guardada no
    /// cofre (ou na memória, sem cofre) para as próximas aberturas.
    pub fn connection_request(
        &self,
        id: &CameraId,
        typed_password: Option<Password>,
    ) -> Result<StreamRequest, CatalogError> {
        let camera = self.get(id).ok_or(CatalogError::NotFound)?;
        if !camera.requires_credentials() {
            // Sem usuário, conecta sem credenciais; senha antiga é ignorada.
            return Ok(StreamRequest {
                camera_id: id.clone(),
                url: camera.url().with_credentials(None, None),
                display_url: camera.url().to_string(),
            });
        }
        let password = match typed_password {
            Some(p) => {
                self.credentials
                    .set(id, &p)
                    .map_err(|e| CatalogError::Credentials(e.to_string()))?;
                Some(p)
            }
            None => self
                .credentials
                .get(id)
                .map_err(|e| CatalogError::Credentials(e.to_string()))?,
        };
        if password.is_none() {
            return Err(CatalogError::NeedsPassword);
        }
        Ok(StreamRequest {
            camera_id: id.clone(),
            url: camera
                .url()
                .with_credentials(camera.username(), password.as_ref()),
            display_url: camera.url().to_string(),
        })
    }

    fn index_of(&self, id: &CameraId) -> Result<usize, CatalogError> {
        self.cameras
            .iter()
            .position(|c| c.id() == id)
            .ok_or(CatalogError::NotFound)
    }

    fn persist(&mut self, next: Vec<Camera>) -> Result<(), CatalogError> {
        self.repository
            .save(&next)
            .map_err(|e| CatalogError::Storage(e.0))?;
        self.cameras = next;
        Ok(())
    }

    fn apply_password(&self, id: &CameraId, input: PasswordInput) -> Result<(), CatalogError> {
        let result = match input {
            PasswordInput::Unchanged => Ok(()),
            PasswordInput::Set(p) if !p.is_empty() => self.credentials.set(id, &p),
            PasswordInput::Set(_) | PasswordInput::Clear => self.credentials.delete(id),
        };
        result.map_err(|e| CatalogError::Credentials(e.to_string()))
    }
}

fn validate(draft: CameraDraft) -> Result<ValidDraft, CatalogError> {
    let mut errors = Vec::new();
    let name = CameraName::parse(&draft.name)
        .map_err(|e| {
            errors.push(FieldError {
                field: Field::Name,
                message: e.to_string(),
            })
        })
        .ok();
    let parsed = RtspUrl::parse(&draft.url)
        .map_err(|e| {
            errors.push(FieldError {
                field: Field::Url,
                message: e.to_string(),
            })
        })
        .ok();
    let (Some(name), Some(parsed)) = (name, parsed) else {
        return Err(CatalogError::Invalid(errors));
    };
    // Credenciais embutidas na URL têm precedência sobre os campos.
    let username = parsed.username.or(Some(draft.username));
    let password = match parsed.password {
        Some(p) => PasswordInput::Set(p),
        None => draft.password,
    };
    Ok(ValidDraft {
        name,
        url: parsed.url,
        username,
        password,
    })
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;

    use super::*;
    use crate::application::ports::{CredentialError, LoadedCameras, RepositoryError};

    #[derive(Default)]
    struct State {
        saved: Option<Vec<Camera>>,
        fail_save: bool,
        initial: Vec<Camera>,
        secrets: HashMap<CameraId, Password>,
        fail_secrets: bool,
    }

    #[derive(Clone, Default)]
    struct Fake(Rc<RefCell<State>>);

    impl CameraRepository for Fake {
        fn load(&self) -> Result<LoadedCameras, RepositoryError> {
            Ok(LoadedCameras {
                cameras: self.0.borrow().initial.clone(),
                warning: None,
            })
        }
        fn save(&self, cameras: &[Camera]) -> Result<(), RepositoryError> {
            let mut s = self.0.borrow_mut();
            if s.fail_save {
                return Err(RepositoryError("disco cheio".into()));
            }
            s.saved = Some(cameras.to_vec());
            Ok(())
        }
    }

    impl CredentialStore for Fake {
        fn get(&self, id: &CameraId) -> Result<Option<Password>, CredentialError> {
            Ok(self.0.borrow().secrets.get(id).cloned())
        }
        fn set(&self, id: &CameraId, p: &Password) -> Result<(), CredentialError> {
            let mut s = self.0.borrow_mut();
            if s.fail_secrets {
                return Err(CredentialError("cofre indisponível".into()));
            }
            s.secrets.insert(id.clone(), p.clone());
            Ok(())
        }
        fn delete(&self, id: &CameraId) -> Result<(), CredentialError> {
            let mut s = self.0.borrow_mut();
            if s.fail_secrets {
                return Err(CredentialError("cofre indisponível".into()));
            }
            s.secrets.remove(id);
            Ok(())
        }
        fn is_persistent(&self) -> bool {
            true
        }
    }

    fn catalog() -> (CameraCatalog, Fake) {
        let fake = Fake::default();
        let (c, w) = CameraCatalog::load(Box::new(fake.clone()), Box::new(fake.clone())).unwrap();
        assert!(w.is_none());
        (c, fake)
    }

    fn draft(name: &str, url: &str) -> CameraDraft {
        CameraDraft {
            name: name.into(),
            url: url.into(),
            ..Default::default()
        }
    }

    #[test]
    fn adiciona_e_persiste() {
        let (mut c, fake) = catalog();
        let id = c
            .add(draft("Garagem", "rtsp://192.168.0.10:554/stream1"))
            .unwrap();
        let saved = fake.0.borrow().saved.clone().unwrap();
        assert_eq!(saved.len(), 1);
        assert_eq!(saved[0].id(), &id);
        assert_eq!(saved[0].name().as_str(), "Garagem");
        assert_eq!(c.cameras().len(), 1);
    }

    #[test]
    fn credenciais_da_url_vao_para_o_cofre() {
        let (mut c, fake) = catalog();
        let id = c
            .add(draft("Cam", "rtsp://admin:segredo@192.168.0.10/stream1"))
            .unwrap();
        let cam = c.get(&id).unwrap();
        assert_eq!(cam.url().to_string(), "rtsp://192.168.0.10/stream1");
        assert_eq!(cam.username(), Some("admin"));
        assert_eq!(
            fake.0.borrow().secrets.get(&id).unwrap().expose(),
            "segredo"
        );
    }

    #[test]
    fn valida_todos_os_campos_de_uma_vez() {
        let (mut c, fake) = catalog();
        let err = c.add(draft("  ", "http://x")).unwrap_err();
        let CatalogError::Invalid(erros) = err else {
            panic!("{err:?}")
        };
        assert_eq!(
            erros.iter().map(|e| e.field).collect::<Vec<_>>(),
            [Field::Name, Field::Url]
        );
        assert!(fake.0.borrow().saved.is_none());
    }

    #[test]
    fn falha_ao_gravar_nao_altera_a_lista() {
        let (mut c, fake) = catalog();
        fake.0.borrow_mut().fail_save = true;
        let mut d = draft("A", "rtsp://h/s");
        d.username = "admin".into();
        d.password = PasswordInput::Set(Password::new("x"));
        assert!(matches!(c.add(d), Err(CatalogError::Storage(_))));
        assert!(c.cameras().is_empty());
        assert!(
            fake.0.borrow().secrets.is_empty(),
            "senha não deve ficar órfã"
        );
    }

    #[test]
    fn edita_mantendo_identidade_e_senha() {
        let (mut c, fake) = catalog();
        let mut d = draft("A", "rtsp://h/s");
        d.username = "admin".into();
        d.password = PasswordInput::Set(Password::new("p1"));
        let id = c.add(d).unwrap();

        let mut e = draft("B", "rtsp://h2/s");
        e.username = "admin".into();
        c.edit(&id, e).unwrap();
        let cam = c.get(&id).unwrap();
        assert_eq!(cam.name().as_str(), "B");
        assert_eq!(fake.0.borrow().secrets.get(&id).unwrap().expose(), "p1");
    }

    #[test]
    fn senha_vazia_ou_clear_apaga_do_cofre() {
        let (mut c, fake) = catalog();
        let mut d = draft("A", "rtsp://u:p@h/s");
        let id = c.add(d.clone()).unwrap();
        d.url = "rtsp://h/s".into();
        d.username = "u".into();
        d.password = PasswordInput::Set(Password::new(""));
        c.edit(&id, d).unwrap();
        assert!(!fake.0.borrow().secrets.contains_key(&id));
    }

    #[test]
    fn editar_ou_remover_inexistente() {
        let (mut c, _) = catalog();
        let id = CameraId::generate();
        assert_eq!(
            c.edit(&id, draft("A", "rtsp://h")),
            Err(CatalogError::NotFound)
        );
        assert_eq!(c.remove(&id), Err(CatalogError::NotFound));
    }

    #[test]
    fn remove_camera_e_senha() {
        let (mut c, fake) = catalog();
        let id = c.add(draft("A", "rtsp://u:p@h/s")).unwrap();
        c.remove(&id).unwrap();
        assert!(c.cameras().is_empty());
        assert!(fake.0.borrow().saved.as_ref().unwrap().is_empty());
        assert!(fake.0.borrow().secrets.is_empty());
    }

    #[test]
    fn remover_com_cofre_falhando_avisa_mas_remove() {
        let (mut c, fake) = catalog();
        let id = c.add(draft("A", "rtsp://u:p@h/s")).unwrap();
        fake.0.borrow_mut().fail_secrets = true;
        assert!(matches!(c.remove(&id), Err(CatalogError::Credentials(_))));
        assert!(c.cameras().is_empty());
    }

    #[test]
    fn pedido_de_conexao_inclui_credenciais_so_na_url_de_conexao() {
        let (mut c, _) = catalog();
        let id = c.add(draft("A", "rtsp://admin:s3gr3do@h:554/s")).unwrap();
        let req = c.connection_request(&id, None).unwrap();
        assert_eq!(req.url.expose(), "rtsp://admin:s3gr3do@h:554/s");
        assert_eq!(req.display_url, "rtsp://h:554/s");
        assert!(!format!("{req:?}").contains("s3gr3do"));
    }

    #[test]
    fn sem_senha_no_cofre_pede_ao_usuario_e_guarda() {
        let (mut c, fake) = catalog();
        let mut d = draft("A", "rtsp://h/s");
        d.username = "admin".into();
        let id = c.add(d).unwrap();
        assert_eq!(
            c.connection_request(&id, None).unwrap_err(),
            CatalogError::NeedsPassword
        );
        let req = c.connection_request(&id, Some(Password::new("x"))).unwrap();
        assert_eq!(req.url.expose(), "rtsp://admin:x@h/s");
        assert!(fake.0.borrow().secrets.contains_key(&id));
        assert!(c.connection_request(&id, None).is_ok());
    }

    #[test]
    fn camera_sem_usuario_conecta_sem_senha() {
        let (mut c, _) = catalog();
        let id = c.add(draft("A", "rtsp://h/s")).unwrap();
        assert_eq!(
            c.connection_request(&id, None).unwrap().url.expose(),
            "rtsp://h/s"
        );
    }

    #[test]
    fn falha_do_cofre_ao_adicionar_devolve_o_id_da_camera_salva() {
        let (mut c, fake) = catalog();
        fake.0.borrow_mut().fail_secrets = true;
        let mut d = draft("A", "rtsp://h/s");
        d.username = "admin".into();
        d.password = PasswordInput::Set(Password::new("p1"));
        let err = c.add(d).unwrap_err();
        let CatalogError::AddedWithoutPassword { id, .. } = err else {
            panic!("{err:?}")
        };
        assert_eq!(c.cameras().len(), 1);
        assert_eq!(fake.0.borrow().saved.as_ref().unwrap().len(), 1);
        assert!(c.get(&id).is_some());

        // Com o id, a UI reaplica a senha via `edit`, sem duplicar a câmera.
        fake.0.borrow_mut().fail_secrets = false;
        let mut e = draft("A", "rtsp://h/s");
        e.username = "admin".into();
        e.password = PasswordInput::Set(Password::new("p1"));
        c.edit(&id, e).unwrap();
        assert_eq!(c.cameras().len(), 1);
        assert_eq!(fake.0.borrow().secrets.get(&id).unwrap().expose(), "p1");
    }

    #[test]
    fn editar_removendo_usuario_apaga_senha_do_cofre() {
        let (mut c, fake) = catalog();
        let id = c.add(draft("A", "rtsp://u:p@h/s")).unwrap();
        assert!(fake.0.borrow().secrets.contains_key(&id));

        c.edit(&id, draft("A", "rtsp://h/s")).unwrap();
        assert_eq!(c.get(&id).unwrap().username(), None);
        assert!(!fake.0.borrow().secrets.contains_key(&id));
        assert_eq!(
            c.connection_request(&id, None).unwrap().url.expose(),
            "rtsp://h/s"
        );
    }

    #[test]
    fn camera_sem_usuario_ignora_senha_que_restou_no_cofre() {
        let fake = Fake::default();
        let cam = Camera::new(
            CameraName::parse("X").unwrap(),
            RtspUrl::parse("rtsp://h/s").unwrap().url,
            None,
        );
        let id = cam.id().clone();
        {
            let mut s = fake.0.borrow_mut();
            s.initial = vec![cam];
            s.secrets.insert(id.clone(), Password::new("antiga"));
        }
        let (c, _) = CameraCatalog::load(Box::new(fake.clone()), Box::new(fake)).unwrap();
        let req = c.connection_request(&id, None).unwrap();
        assert_eq!(req.url.expose(), "rtsp://h/s");
    }

    #[test]
    fn adicionar_sem_usuario_nao_guarda_senha() {
        let (mut c, fake) = catalog();
        let mut d = draft("A", "rtsp://h/s");
        d.password = PasswordInput::Set(Password::new("x"));
        let id = c.add(d).unwrap();
        assert!(fake.0.borrow().secrets.is_empty());
        assert_eq!(
            c.connection_request(&id, None).unwrap().url.expose(),
            "rtsp://h/s"
        );
    }

    #[test]
    fn carrega_lista_existente() {
        let fake = Fake::default();
        let cam = Camera::new(
            CameraName::parse("X").unwrap(),
            RtspUrl::parse("rtsp://h").unwrap().url,
            None,
        );
        fake.0.borrow_mut().initial = vec![cam.clone()];
        let (c, _) = CameraCatalog::load(Box::new(fake.clone()), Box::new(fake)).unwrap();
        assert_eq!(c.cameras(), [cam]);
    }
}
