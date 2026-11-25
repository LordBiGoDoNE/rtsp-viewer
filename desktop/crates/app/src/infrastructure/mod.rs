//! Adaptadores de disco e cofre de senhas.

pub mod credentials;
pub mod json_repository;
pub mod memory_repository;
// Ligados à interface no grupo 4 da spec descoberta-onvif.
#[allow(dead_code)]
pub mod onvif_adapters;
