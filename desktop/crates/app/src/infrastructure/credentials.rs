//! Senhas das câmeras: cofre do sistema (Windows Credential Manager, Secret
//! Service no Linux) ou, sem cofre, memória até o app fechar.
//!
//! O cofre é aberto na primeira utilização, não no boot, para que a janela
//! apareça sem esperar o D-Bus ou o Credential Manager.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use rtsp_viewer_core::application::ports::{CredentialError, CredentialStore};
use rtsp_viewer_core::domain::{CameraId, Password};

/// Nome do serviço no cofre; a conta é o `CameraId`.
pub const SERVICE: &str = "rtsp-viewer";

/// Senhas só em memória. Usado quando não há cofre e nos testes.
#[derive(Default)]
pub struct MemoryCredentialStore {
    secrets: Mutex<HashMap<CameraId, Password>>,
}

impl CredentialStore for MemoryCredentialStore {
    fn get(&self, id: &CameraId) -> Result<Option<Password>, CredentialError> {
        Ok(lock(&self.secrets).get(id).cloned())
    }

    fn set(&self, id: &CameraId, password: &Password) -> Result<(), CredentialError> {
        lock(&self.secrets).insert(id.clone(), password.clone());
        Ok(())
    }

    fn delete(&self, id: &CameraId) -> Result<(), CredentialError> {
        lock(&self.secrets).remove(id);
        Ok(())
    }

    fn is_persistent(&self) -> bool {
        false
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Backend real de cofre, abstraído para que a escolha entre cofre e memória
/// seja testável sem tocar no cofre do sistema.
pub trait VaultBackend: Send + Sync {
    /// Prepara o cofre. Erro significa "sem cofre neste sistema".
    fn open(&self) -> Result<(), String>;
    fn get(&self, account: &str) -> Result<Option<String>, String>;
    fn set(&self, account: &str, secret: &str) -> Result<(), String>;
    fn delete(&self, account: &str) -> Result<(), String>;
}

enum Resolved {
    Vault,
    Memory(MemoryCredentialStore),
}

/// Usa o cofre do sistema quando ele abre; caso contrário, memória.
pub struct SystemCredentialStore<B: VaultBackend> {
    backend: B,
    resolved: OnceLock<Resolved>,
}

impl<B: VaultBackend> SystemCredentialStore<B> {
    pub fn new(backend: B) -> Self {
        Self {
            backend,
            resolved: OnceLock::new(),
        }
    }

    fn resolved(&self) -> &Resolved {
        self.resolved.get_or_init(|| match self.backend.open() {
            Ok(()) => Resolved::Vault,
            Err(_) => Resolved::Memory(MemoryCredentialStore::default()),
        })
    }
}

impl<B: VaultBackend> CredentialStore for SystemCredentialStore<B> {
    fn get(&self, id: &CameraId) -> Result<Option<Password>, CredentialError> {
        match self.resolved() {
            Resolved::Memory(m) => m.get(id),
            Resolved::Vault => self
                .backend
                .get(id.as_str())
                .map(|o| o.map(Password::new))
                .map_err(vault_error),
        }
    }

    fn set(&self, id: &CameraId, password: &Password) -> Result<(), CredentialError> {
        match self.resolved() {
            Resolved::Memory(m) => m.set(id, password),
            Resolved::Vault => self
                .backend
                .set(id.as_str(), password.expose())
                .map_err(vault_error),
        }
    }

    fn delete(&self, id: &CameraId) -> Result<(), CredentialError> {
        match self.resolved() {
            Resolved::Memory(m) => m.delete(id),
            Resolved::Vault => self.backend.delete(id.as_str()).map_err(vault_error),
        }
    }

    fn is_persistent(&self) -> bool {
        matches!(self.resolved(), Resolved::Vault)
    }
}

fn vault_error(message: String) -> CredentialError {
    CredentialError(format!("cofre de senhas: {message}"))
}

/// Cofre do sistema via `keyring-core`.
pub struct KeyringBackend;

impl VaultBackend for KeyringBackend {
    fn open(&self) -> Result<(), String> {
        #[cfg(target_os = "windows")]
        {
            let store = windows_native_keyring_store::Store::new().map_err(|e| e.to_string())?;
            keyring_core::set_default_store(store);
        }
        #[cfg(target_os = "linux")]
        {
            let store =
                zbus_secret_service_keyring_store::Store::new().map_err(|e| e.to_string())?;
            keyring_core::set_default_store(store);
        }
        #[cfg(not(any(target_os = "windows", target_os = "linux")))]
        {
            return Err("plataforma sem cofre suportado".into());
        }
        // Confirma que o cofre responde: ausência de entrada é resposta válida.
        match self.get("rtsp-viewer-sonda") {
            Ok(_) => Ok(()),
            Err(e) => Err(e),
        }
    }

    fn get(&self, account: &str) -> Result<Option<String>, String> {
        match entry(account)?.get_password() {
            Ok(p) => Ok(Some(p)),
            Err(keyring_core::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn set(&self, account: &str, secret: &str) -> Result<(), String> {
        entry(account)?
            .set_password(secret)
            .map_err(|e| e.to_string())
    }

    fn delete(&self, account: &str) -> Result<(), String> {
        match entry(account)?.delete_credential() {
            Ok(()) | Err(keyring_core::Error::NoEntry) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

fn entry(account: &str) -> Result<keyring_core::Entry, String> {
    keyring_core::Entry::new(SERVICE, account).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    #[derive(Default)]
    struct FakeVault {
        available: bool,
        opens: AtomicUsize,
        data: Mutex<HashMap<String, String>>,
    }

    impl VaultBackend for &FakeVault {
        fn open(&self) -> Result<(), String> {
            self.opens.fetch_add(1, Ordering::SeqCst);
            if self.available {
                Ok(())
            } else {
                Err("sem Secret Service".into())
            }
        }
        fn get(&self, a: &str) -> Result<Option<String>, String> {
            Ok(lock(&self.data).get(a).cloned())
        }
        fn set(&self, a: &str, s: &str) -> Result<(), String> {
            lock(&self.data).insert(a.into(), s.into());
            Ok(())
        }
        fn delete(&self, a: &str) -> Result<(), String> {
            lock(&self.data).remove(a);
            Ok(())
        }
    }

    #[test]
    fn com_cofre_grava_no_cofre() {
        let vault = FakeVault {
            available: true,
            ..Default::default()
        };
        let store = SystemCredentialStore::new(&vault);
        let id = CameraId::generate();
        store.set(&id, &Password::new("segredo")).unwrap();
        assert!(store.is_persistent());
        assert_eq!(
            lock(&vault.data).get(id.as_str()).map(String::as_str),
            Some("segredo")
        );
        assert_eq!(store.get(&id).unwrap().unwrap().expose(), "segredo");
        store.delete(&id).unwrap();
        assert!(store.get(&id).unwrap().is_none());
    }

    #[test]
    fn sem_cofre_usa_memoria_e_avisa() {
        let vault = FakeVault::default();
        let store = SystemCredentialStore::new(&vault);
        let id = CameraId::generate();
        store.set(&id, &Password::new("segredo")).unwrap();
        assert!(!store.is_persistent());
        assert!(lock(&vault.data).is_empty());
        assert_eq!(store.get(&id).unwrap().unwrap().expose(), "segredo");
    }

    #[test]
    fn cofre_aberto_so_uma_vez_e_so_quando_usado() {
        let vault = FakeVault {
            available: true,
            ..Default::default()
        };
        let store = SystemCredentialStore::new(&vault);
        assert_eq!(vault.opens.load(Ordering::SeqCst), 0);
        let id = CameraId::generate();
        store.get(&id).unwrap();
        store.get(&id).unwrap();
        assert_eq!(vault.opens.load(Ordering::SeqCst), 1);
    }

    /// Usa o cofre real do sistema. Ignorado por padrão porque grava no
    /// Credential Manager/Secret Service do usuário (e apaga em seguida):
    /// `cargo test -p rtsp-viewer cofre_real -- --ignored`
    #[test]
    #[ignore = "usa o cofre real do sistema"]
    fn cofre_real_grava_le_e_apaga() {
        let store = SystemCredentialStore::new(KeyringBackend);
        let id = CameraId::generate();
        assert!(store.is_persistent(), "cofre do sistema indisponível");
        store.set(&id, &Password::new("senha-de-teste")).unwrap();
        assert_eq!(store.get(&id).unwrap().unwrap().expose(), "senha-de-teste");
        store.delete(&id).unwrap();
        assert!(store.get(&id).unwrap().is_none());
        store.delete(&id).unwrap();
    }

    #[test]
    fn memoria_apaga_senha() {
        let store = MemoryCredentialStore::default();
        let id = CameraId::generate();
        store.set(&id, &Password::new("x")).unwrap();
        store.delete(&id).unwrap();
        store.delete(&id).unwrap();
        assert!(store.get(&id).unwrap().is_none());
    }
}
