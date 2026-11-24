//! Casos de uso e portas.

pub mod catalog;
pub mod ports;

pub use catalog::{CameraCatalog, CameraDraft, CatalogError, Field, FieldError, PasswordInput};
