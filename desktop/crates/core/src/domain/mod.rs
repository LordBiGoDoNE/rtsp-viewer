//! Entidades e objetos de valor.

pub mod camera;
pub mod device_address;
pub mod reconnect;
pub mod rtsp_url;
pub mod secret;
pub mod vendor;

pub use camera::{CAMERA_NAME_MAX, Camera, CameraId, CameraName, NameError, SubstreamHostMismatch};
pub use device_address::{AddressError, DeviceAddress, host_of_service_url};
pub use reconnect::{ReconnectPolicy, STABLE_PLAYBACK};
pub use rtsp_url::{ParsedUrl, RtspUrl, UrlError};
pub use secret::{ConnectionUrl, Password, redact_credentials};
pub use vendor::{DEFAULT_RTSP_PORT, Vendor};
