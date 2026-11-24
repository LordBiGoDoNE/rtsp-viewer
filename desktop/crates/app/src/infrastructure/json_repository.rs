//! Lista de câmeras em `cameras.json`, com escrita atômica e recuperação de
//! arquivo corrompido. Senhas nunca entram neste arquivo.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rtsp_viewer_core::application::ports::{
    CameraRepository, LoadedCameras, RepositoryError, RepositoryWarning,
};
use rtsp_viewer_core::domain::{Camera, CameraId, CameraName, RtspUrl};
use serde::{Deserialize, Serialize};

const FILE_NAME: &str = "cameras.json";
const CURRENT_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct FileDto {
    version: u32,
    cameras: Vec<CameraDto>,
}

#[derive(Serialize, Deserialize)]
struct CameraDto {
    id: String,
    name: String,
    url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    username: Option<String>,
}

pub struct JsonCameraRepository {
    dir: PathBuf,
}

impl JsonCameraRepository {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// Diretório de configuração do usuário (por exemplo
    /// `%APPDATA%\rtsp-viewer\config` ou `~/.config/rtsp-viewer`).
    pub fn default_dir() -> Option<PathBuf> {
        directories::ProjectDirs::from("", "", "rtsp-viewer").map(|d| d.config_dir().to_path_buf())
    }

    pub fn path(&self) -> PathBuf {
        self.dir.join(FILE_NAME)
    }

    fn preserve_corrupted(&self, path: &Path) -> Result<PathBuf, RepositoryError> {
        let backup = self.dir.join(format!(
            "{FILE_NAME}.corrompido-{}",
            timestamp_utc(SystemTime::now())
        ));
        fs::rename(path, &backup).map_err(|e| {
            RepositoryError(format!(
                "não foi possível preservar o arquivo corrompido: {e}"
            ))
        })?;
        Ok(backup)
    }
}

impl CameraRepository for JsonCameraRepository {
    fn load(&self) -> Result<LoadedCameras, RepositoryError> {
        let path = self.path();
        let bytes = match fs::read(&path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(LoadedCameras::default());
            }
            Err(e) => {
                return Err(RepositoryError(format!(
                    "falha ao ler {}: {e}",
                    path.display()
                )));
            }
        };
        match parse(&bytes) {
            Ok(cameras) => Ok(LoadedCameras {
                cameras,
                warning: None,
            }),
            Err(_) => {
                let backup = self.preserve_corrupted(&path)?;
                Ok(LoadedCameras {
                    cameras: Vec::new(),
                    warning: Some(RepositoryWarning::CorruptedFileRecovered {
                        backup: backup.display().to_string(),
                    }),
                })
            }
        }
    }

    fn save(&self, cameras: &[Camera]) -> Result<(), RepositoryError> {
        let dto = FileDto {
            version: CURRENT_VERSION,
            cameras: cameras
                .iter()
                .map(|c| CameraDto {
                    id: c.id().to_string(),
                    name: c.name().as_str().to_string(),
                    url: c.url().to_string(),
                    username: c.username().map(str::to_string),
                })
                .collect(),
        };
        let json = serde_json::to_vec_pretty(&dto)
            .map_err(|e| RepositoryError(format!("falha ao serializar: {e}")))?;
        write_atomic(&self.dir, &self.path(), &json)
            .map_err(|e| RepositoryError(format!("falha ao gravar {}: {e}", self.path().display())))
    }
}

fn parse(bytes: &[u8]) -> Result<Vec<Camera>, String> {
    let dto: FileDto = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if dto.version != CURRENT_VERSION {
        return Err(format!("versão {} não suportada", dto.version));
    }
    dto.cameras
        .into_iter()
        .map(|c| {
            let id = CameraId::parse(&c.id).ok_or("id inválido")?;
            let name = CameraName::parse(&c.name).map_err(|e| e.to_string())?;
            let parsed = RtspUrl::parse(&c.url).map_err(|e| e.to_string())?;
            if parsed.password.is_some() {
                return Err("URL com senha no arquivo".to_string());
            }
            Ok(Camera::restore(id, name, parsed.url, c.username))
        })
        .collect()
}

/// Grava num arquivo temporário no mesmo diretório e renomeia por cima do
/// destino: o arquivo anterior só é substituído depois da escrita completa.
fn write_atomic(dir: &Path, path: &Path, data: &[u8]) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    let tmp = dir.join(format!("{FILE_NAME}.tmp"));
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(data)?;
        f.sync_all()?;
    }
    fs::rename(&tmp, path)
}

/// `AAAAMMDD-HHMMSS` em UTC, sem depender de crate de datas.
fn timestamp_utc(now: SystemTime) -> String {
    let secs = now
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (days, rem) = ((secs / 86_400) as i64, secs % 86_400);
    // Algoritmo civil_from_days (Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}{m:02}{d:02}-{:02}{:02}{:02}",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rtsp-viewer-test-{}", uuid_like()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn uuid_like() -> String {
        CameraId::generate().to_string()
    }

    fn camera(name: &str, url: &str, user: Option<&str>) -> Camera {
        Camera::new(
            CameraName::parse(name).unwrap(),
            RtspUrl::parse(url).unwrap().url,
            user.map(str::to_string),
        )
    }

    #[test]
    fn sem_arquivo_lista_vazia() {
        let repo = JsonCameraRepository::new(temp_dir().join("nao-existe"));
        let loaded = repo.load().unwrap();
        assert!(loaded.cameras.is_empty());
        assert!(loaded.warning.is_none());
    }

    #[test]
    fn grava_e_le_de_volta() {
        let dir = temp_dir();
        let repo = JsonCameraRepository::new(&dir);
        let cams = vec![
            camera("Garagem", "rtsp://192.168.0.10:554/stream1", Some("admin")),
            camera("Portão", "rtsps://cam.local/live", None),
        ];
        repo.save(&cams).unwrap();
        assert_eq!(repo.load().unwrap().cameras, cams);
        assert!(!dir.join("cameras.json.tmp").exists());
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn arquivo_tem_versao_e_nao_tem_senha() {
        let dir = temp_dir();
        let repo = JsonCameraRepository::new(&dir);
        repo.save(&[camera("A", "rtsp://admin:segredo@h/s", Some("admin"))])
            .unwrap();
        let text = fs::read_to_string(repo.path()).unwrap();
        assert!(text.contains("\"version\": 1"));
        assert!(!text.contains("segredo"));
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn arquivo_corrompido_e_preservado() {
        let dir = temp_dir();
        let repo = JsonCameraRepository::new(&dir);
        fs::write(repo.path(), b"{ isto nao e json").unwrap();
        let loaded = repo.load().unwrap();
        assert!(loaded.cameras.is_empty());
        let Some(RepositoryWarning::CorruptedFileRecovered { backup }) = loaded.warning else {
            panic!("esperava aviso de arquivo corrompido");
        };
        assert!(backup.contains("cameras.json.corrompido-"));
        assert_eq!(fs::read(&backup).unwrap(), b"{ isto nao e json");
        assert!(!repo.path().exists());
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn dados_invalidos_contam_como_corrompido() {
        for conteudo in [
            r#"{"version":2,"cameras":[]}"#,
            r#"{"version":1,"cameras":[{"id":"x","name":"A","url":"rtsp://h"}]}"#,
            r#"{"version":1,"cameras":[{"id":"6f1c2a9e-1d55-4d3c-9b6c-0d0b9d7c2f11","name":"A","url":"rtsp://u:p@h"}]}"#,
        ] {
            let dir = temp_dir();
            let repo = JsonCameraRepository::new(&dir);
            fs::write(repo.path(), conteudo).unwrap();
            assert!(repo.load().unwrap().warning.is_some(), "{conteudo}");
            fs::remove_dir_all(dir).ok();
        }
    }

    #[test]
    fn temporario_abandonado_nao_afeta_o_arquivo() {
        // Simula gravação interrompida: o .tmp ficou pela metade.
        let dir = temp_dir();
        let repo = JsonCameraRepository::new(&dir);
        let cams = vec![camera("A", "rtsp://h/s", None)];
        repo.save(&cams).unwrap();
        fs::write(dir.join("cameras.json.tmp"), b"{ pela metade").unwrap();
        assert_eq!(repo.load().unwrap().cameras, cams);
        repo.save(&cams).unwrap();
        assert_eq!(repo.load().unwrap().cameras, cams);
        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn carimbo_de_data_utc() {
        let t = UNIX_EPOCH + Duration::from_secs(1_795_516_365); // 2026-11-24 10:32:45 UTC
        assert_eq!(timestamp_utc(t), "20261124-103245");
        assert_eq!(timestamp_utc(UNIX_EPOCH), "19700101-000000");
        let bissexto = UNIX_EPOCH + Duration::from_secs(951_782_400); // 2000-02-29
        assert_eq!(timestamp_utc(bissexto), "20000229-000000");
    }
}
