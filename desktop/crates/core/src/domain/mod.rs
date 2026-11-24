//! Entidades e objetos de valor.

pub mod camera;
pub mod reconnect;
pub mod rtsp_url;
pub mod secret;

pub use camera::{CAMERA_NAME_MAX, Camera, CameraId, CameraName, NameError};
pub use reconnect::{ReconnectPolicy, STABLE_PLAYBACK};
pub use rtsp_url::{ParsedUrl, RtspUrl, UrlError};
pub use secret::{ConnectionUrl, Password, redact_credentials};
