//! Núcleo do rtsp-viewer: domínio, casos de uso e portas.
//!
//! Este crate não depende de UI, FFmpeg, cofre de senhas, disco nem rede
//! (ver `docs/adr/0001-stack.md`). Os adaptadores ficam em `media` e `app`.
