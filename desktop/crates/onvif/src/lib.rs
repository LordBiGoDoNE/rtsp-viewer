//! Adaptador ONVIF do rtsp-viewer (spec `descoberta-onvif`): consulta dos
//! streams de uma câmera e, no grupo seguinte, busca na rede (WS-Discovery).
//!
//! Sem runtime async e sem TLS (ADR 0005). Entrada de rede é tratada como não
//! confiável: tamanho limitado, XML sem DOCTYPE, URLs validadas pelo `core`.

pub mod auth;
pub mod client;
pub mod soap;
pub mod xml;

pub use auth::Credentials;
pub use client::{DeviceService, OnvifClient, OnvifConfig, OnvifError, StreamProfile, StreamSet};
