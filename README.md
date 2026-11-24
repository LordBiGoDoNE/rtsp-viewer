# rtsp-viewer

Client leve para câmeras RTSP no Windows, Linux e Android: simples, rápido para abrir e econômico.

## Estado

Em planejamento. Ainda não há versão utilizável.

## Stack

| Plataforma | Tecnologia | Pasta |
| --- | --- | --- |
| Windows e Linux | Rust + Slint + FFmpeg | `desktop/` |
| Android | Kotlin + Jetpack Compose + Media3 | `android/` |

As duas interfaces compartilham a mesma identidade visual, definida em `design/`.

## Funcionalidades previstas

1. Lista de câmeras e visualização em tela cheia (desktop).
2. App Android com o mesmo escopo.
3. Grade com várias câmeras.
4. Descoberta de câmeras na rede (ONVIF).
5. Snapshot e gravação.

O desenvolvimento segue specs OpenSpec em `openspec/`. As entregas ficam registradas em `docs/entregas.md`.
