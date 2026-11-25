# rtsp-viewer

Client leve para câmeras RTSP no Windows, Linux e Android: simples, rápido para abrir e econômico.

## Estado

Desktop (Windows e Linux): primeira versão entregue em 2026-11-24 (spec `mvp-desktop-rtsp`). Ela cobre a lista de câmeras e a visualização de uma câmera com tela cheia. Os pacotes saem só como artefatos do CI; nada foi publicado. Antes de distribuir, falta resolver a issue #7 (avisos de licença). O Android ainda não começou.

## Stack

| Plataforma | Tecnologia | Pasta |
| --- | --- | --- |
| Windows e Linux | Rust + Slint + FFmpeg | `desktop/` |
| Android | Kotlin + Jetpack Compose + Media3 | `android/` |

As duas interfaces compartilham a mesma identidade visual, definida em `design/` (ADR 0004).

## Funcionalidades previstas

1. Lista de câmeras e visualização em tela cheia (desktop). **Entregue.**
2. Descoberta de câmeras na rede e cadastro só com IP, usuário e senha (ONVIF).
3. App Android com o mesmo escopo.
4. Grade com várias câmeras.
5. Snapshot e gravação.

O desenvolvimento segue specs OpenSpec em `openspec/`. As entregas ficam registradas em `docs/entregas.md`.

## Usar (desktop)

- **Windows 10/11 x64:** extraia o `.zip` e rode `rtsp-viewer.exe`. Não precisa instalar nada.
- **Ubuntu 24.04 x64:** `sudo apt install ./rtsp-viewer_<versão>_amd64.deb`. O apt instala as bibliotecas do FFmpeg como dependência.

Os pacotes saem como artefatos do CI (workflow "CI"). Não há releases publicadas.

A lista de câmeras fica no diretório de configuração do usuário (`%APPDATA%\rtsp-viewer\config` ou `~/.config/rtsp-viewer`). As senhas ficam no cofre do sistema (Gerenciador de Credenciais no Windows, Secret Service no Linux), nunca nesse arquivo. Num Linux sem cofre, a senha é lembrada só até fechar o app.

Câmeras ONVIF: em "Procurar câmeras" o app lista as câmeras da rede (busca de cerca de 3 s, só quando pedida). Também dá para cadastrar digitando só o IP, com usuário e senha: o app pergunta à câmera as URLs dos streams principal e secundário. Sem ONVIF, o app oferece a lista de fabricantes. Na TP-Link Tapo, o usuário e a senha são os da "Conta da câmera" (app Tapo, configurações avançadas) e o ONVIF responde na porta 2020.

Atalhos: Enter abre a câmera selecionada; F11 ou duplo clique alternam a tela cheia; Esc sai da tela cheia e volta à lista.

## Medições (2026-11-24)

Atualizadas na spec `descoberta-onvif`: o boot continua em 360 ms de mediana com 20 câmeras (325 a 697 ms).

Máquina de referência: Windows 11 do desenvolvedor, build release do pacote `.zip`.

| Item | Resultado | Meta |
| --- | --- | --- |
| Boot até a janela, mediana de 10 execuções a quente, 20 câmeras (`scripts/measure-boot.ps1`) | 335 a 360 ms (máximo 458 ms) | < 500 ms |
| Memória ociosa (working set / private bytes), sem câmera aberta | 78 MB / 91 MB | |
| Primeira imagem, câmera falsa local, quadro-chave a cada 1 s | H.264, 25 aberturas: mediana de 0,86 a 1,05 s (mínimo 0,5 s); H.265: até 2,2 s nos piores casos, incluindo até 1 s de espera pelo quadro-chave | até 2 s, sem contar a espera pelo quadro-chave da câmera |
| Latência com a câmera falsa (relógio na imagem contra o relógio do sistema) | cerca de 0,1 s | até 1 s |
| Pacote Windows `.zip` | 59,4 MB compactado (135 MB extraído; o ONVIF somou 0,3 MB) | até 30 MB |
| Busca de câmeras na rede (WS-Discovery, simulador ONVIF) | 3,0 a 3,1 s | cerca de 3 s |
| Consulta ONVIF só com o IP, sondando as portas usuais | 0,12 a 0,15 s | |
| Pacote Linux `.deb` | ver o artefato do CI (o FFmpeg vem da distribuição) | |
| Decodificação | D3D11VA no Windows (hardware), com fallback para software | |

O pacote Windows passa da meta porque o build LGPL do FFmpeg traz todos os codecs: só o `avcodec` tem 87 MB. O build mínimo do FFmpeg é uma spec separada (ADR 0002, decisão D4; issue #6).

## Desenvolver (desktop)

Pré-requisitos no Windows: Rust (rustup; a versão vem de `desktop/rust-toolchain.toml`), Build Tools do Visual Studio, LLVM (libclang) e Docker para a câmera falsa.

```powershell
./scripts/fetch-ffmpeg.ps1        # FFmpeg LGPL fixado, em third_party/
. ./scripts/env.ps1               # FFMPEG_DIR, LIBCLANG_PATH e PATH das DLLs
cd desktop
cargo test --workspace
cargo run -p rtsp-viewer --release
```

No Linux (Ubuntu 24.04): `sudo apt install pkg-config clang libclang-dev libavcodec-dev libavformat-dev libavutil-dev libswscale-dev libfontconfig1-dev libxkbcommon-dev`, depois `cargo test --workspace` em `desktop/`.

Testes do player contra a câmera falsa: suba `test-camera/` (veja `test-camera/README.md`) e defina `RTSP_TEST_BASE=rtsp://localhost:8554` (e `RTSP_TEST_COMPOSE` com o caminho do `compose.yaml` para o teste de queda e retorno).

Simulador ONVIF (consulta e WS-Discovery multicast, apontando para a câmera falsa): `cargo run -p rtsp-viewer-onvif --example fake_onvif -- --port 8099` (usuário `admin`, senha `admin123`, fictícios). Diagnóstico de rede: `cargo run -p rtsp-viewer-onvif --example discover` (busca) ou `-- <ip[:porta]> <usuário>` com a senha na variável `ONVIF_PASSWORD`.

Pacotes locais: `./scripts/package-windows.ps1` (gera `dist/*.zip`) e `cargo deb -p rtsp-viewer` no Linux.

Variáveis úteis: `RTSP_VIEWER_CONFIG_DIR` (lista de câmeras isolada), `RTSP_VIEWER_BOOT_TRACE=<arquivo>` e `RTSP_VIEWER_BOOT_EXIT=1` (medição de boot).

## Licenças

Veja `docs/licencas-terceiros.md`: FFmpeg sob LGPL, ligado dinamicamente; Slint sob a licença Royalty-free 2.0, com atribuição na tela "Sobre".
