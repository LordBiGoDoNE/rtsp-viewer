# Proposal

Issue: [#1](https://github.com/LordBiGoDoNE/rtsp-viewer/issues/1).
Épica: `epic/mvp-desktop-rtsp`, criada da `main` `ccd5ce3` em 2026-11-24.

## Why

O usuário precisa de um client RTSP leve, simples e de boot rápido para Windows,
Linux e Android. O repositório ainda não tem código. A stack foi decidida em
2026-11-24: Rust + Slint + FFmpeg no desktop e Kotlin + Compose + Media3 no
Android, com identidade visual compartilhada por tokens de design.

Esta é a primeira de cinco specs (`openspec/config.yaml`). Ela entrega o menor
produto útil no desktop: cadastrar câmeras e assistir a uma delas. As specs
seguintes (Android, grade, ONVIF, snapshot e gravação) dependem da fundação
criada aqui: workspace, núcleo com portas, player FFmpeg, tokens de design e CI.

## What Changes

- Workspace Cargo em `desktop/` com os crates `core`, `media` e `app`
  (`onvif` fica para a spec `descoberta-onvif`).
- Núcleo sem dependência de UI, FFmpeg, rede ou disco: `Camera`, `RtspUrl`,
  lista de câmeras, portas `CameraRepository`, `CredentialStore` e
  `StreamPlayer`.
- Cadastro de câmeras (adicionar, editar, remover) salvo localmente; senhas no
  cofre do sistema, nunca no arquivo de configuração.
- Player FFmpeg de baixa latência com decodificação por hardware quando houver,
  estados visíveis e reconexão automática.
- UI Slint: lista de câmeras, formulário e visualização com tela cheia.
- `design/tokens.json` e `design/telas.md`, com o tema do Slint gerado no build.
- CI no GitHub Actions: formatação, lint, testes (inclusive contra câmera falsa
  local) e build Linux/Windows, mais o job `openspec`.
- Pacotes: `.zip` para Windows e `.deb` para Ubuntu 24.04.
- Medição de boot, tamanho do pacote e latência, registrada no README.

## Out of scope

- Android (spec `app-android`).
- Grade com várias câmeras, substream (spec `grade-multi-camera`).
- Descoberta ONVIF (spec `descoberta-onvif`).
- Snapshot e gravação (spec `snapshot-e-gravacao`).
- Áudio, PTZ, transporte RTSP por UDP, instalador assinado, atualização
  automática, AppImage/Flatpak.
- Build próprio e mínimo do FFmpeg: só entra se a medição do grupo 5 mostrar
  que o pacote passou da meta (vira issue, ver design D4).
- Publicação de releases: deploy/publicação é workflow separado e manual,
  fora desta spec.

## Capabilities

### New Capabilities

- `camera-registry`: cadastro, validação e persistência de câmeras e credenciais.
- `live-view`: reprodução de um stream RTSP com estados, reconexão e tela cheia.
- `design-tokens`: identidade visual única gerada a partir de tokens.
- `desktop-distribution`: metas de boot, tamanho e latência, pacotes e CI.

### Modified Capabilities

Nenhuma.

## Impact

- Repositório novo; nenhum código existente é afetado.
- Dependências novas: Slint, ffmpeg-next (ou fork mantido, ver D3), keyring,
  serde/serde_json, directories, log.
- Ambiente de desenvolvimento: Rust via rustup, Build Tools do MSVC, FFmpeg
  compartilhado no Windows; Docker para a câmera falsa.
- Áreas críticas: segredos (senhas de câmeras) e CI.
