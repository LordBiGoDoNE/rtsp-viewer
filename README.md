# rtsp-viewer

Client leve para câmeras RTSP no Windows, Linux e Android: simples, rápido para abrir e econômico.

## Estado

Desktop (Windows e Linux): primeira versão entregue em 2026-11-24 (spec `mvp-desktop-rtsp`). Ela cobre a lista de câmeras e a visualização de uma câmera com tela cheia. Os pacotes saem só como artefatos do CI; nada foi publicado. Em 2026-11-25 entrou a descoberta ONVIF (spec `descoberta-onvif`): busca de câmeras na rede e cadastro só com IP, usuário e senha. Antes de distribuir, falta resolver a issue #7 (avisos de licença). Android: entregue em 2026-11-26 (spec `app-android`), com o mesmo escopo do desktop (lista de câmeras, visualização com tela cheia e descoberta ONVIF). O APK de release é assinado com a chave local do usuário; o CI gera só o APK sem assinatura, e nada foi publicado.

## Stack

| Plataforma | Tecnologia | Pasta |
| --- | --- | --- |
| Windows e Linux | Rust + Slint + FFmpeg | `desktop/` |
| Android | Kotlin + Jetpack Compose + Media3 | `android/` |

As duas interfaces compartilham a mesma identidade visual, definida em `design/` (ADR 0004).

## Funcionalidades previstas

1. Lista de câmeras e visualização em tela cheia (desktop). **Entregue.**
2. Descoberta de câmeras na rede e cadastro só com IP, usuário e senha (ONVIF, desktop). **Entregue.**
3. App Android com o mesmo escopo. **Entregue.**
4. Grade com várias câmeras.
5. Snapshot e gravação.

O desenvolvimento segue specs OpenSpec em `openspec/`. As entregas ficam registradas em `docs/entregas.md`.

## Usar (desktop)

- **Windows 10/11 x64:** extraia o `.zip` e rode `rtsp-viewer.exe`. Não precisa instalar nada.
- **Ubuntu 24.04 x64:** `sudo apt install ./rtsp-viewer_<versão>_amd64.deb`. O apt instala as bibliotecas do FFmpeg como dependência.

Os pacotes saem como artefatos do CI (workflow "CI"). Não há releases publicadas.

A lista de câmeras fica no diretório de configuração do usuário (`%APPDATA%\rtsp-viewer\config` ou `~/.config/rtsp-viewer`). As senhas ficam no cofre do sistema (Gerenciador de Credenciais no Windows, Secret Service no Linux), nunca nesse arquivo. Num Linux sem cofre, a senha é lembrada só até fechar o app.

Câmeras ONVIF: em "Procurar câmeras" o app lista as câmeras da rede (busca de cerca de 3 s, só quando pedida, por multicast e também direto a cada endereço da sub-rede privada, porque algumas câmeras só respondem assim). Também dá para cadastrar digitando só o IP, com usuário e senha: o app pergunta à câmera as URLs dos streams principal e secundário. Sem ONVIF, o app oferece a lista de fabricantes. Na TP-Link Tapo, o usuário e a senha são os da "Conta da câmera" (app Tapo, configurações avançadas) e o ONVIF responde na porta 2020.

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
| Busca de câmeras na rede (WS-Discovery, simulador ONVIF e câmera real na LAN) | 3,0 a 3,1 s | cerca de 3 s |
| Consulta ONVIF só com o IP, sondando as portas usuais | 0,12 a 0,15 s | |
| Pacote Linux `.deb` | ver o artefato do CI (o FFmpeg vem da distribuição) | |
| Decodificação | D3D11VA no Windows (hardware), com fallback para software | |

O pacote Windows passa da meta porque o build LGPL do FFmpeg traz todos os codecs: só o `avcodec` tem 87 MB. O build mínimo do FFmpeg é uma spec separada (ADR 0002, decisão D4; issue #6).

## Medições (Android, 2026-11-25)

Feitas na spec `app-android`. Aparelho: celular do usuário (Galaxy A56, Android 17), no Wi-Fi da LAN local; câmera falsa (`test-camera/`) e simulador ONVIF no PC, ligado por cabo. Build `e2e`: igual ao release (R8), com outro id e assinado com a chave de debug do SDK.

| Item | Resultado | Meta |
| --- | --- | --- |
| Partida a frio com 20 câmeras, `TotalTime` de `adb shell am start -W` após `am force-stop`, mediana de 10 execuções | 168 ms (161 a 218 ms) | até 1000 ms |
| Lista de câmeras visível na partida a frio (capturas de tela seguidas a partir do `am start`, 5 execuções) | 0,43 a 0,51 s, contando o próprio `am start` | até 1 s |
| APK de release sem assinatura (`app-release-unsigned.apk`) | 1,80 MB (1.798.024 bytes) | até 15 MB |
| Primeira imagem, do toque na câmera até o vídeo na tela, quadro-chave a cada 1 s (capturas a cada 0,35 s; valor = primeira captura com vídeo) | H.264: 0,35 a 1,11 s, mediana 0,83 s (6 aberturas); H.265: 0,29 a 0,74 s, mediana 0,71 s (6 aberturas) | até 2 s, sem contar a espera pelo quadro-chave da câmera |
| Latência com a câmera falsa (relógio na imagem contra o relógio do PC) | 0,39 a 0,49 s (6 amostras); 0,82 a 0,91 s numa segunda série (8 amostras), segundos antes de o celular trocar de rede Wi-Fi por sinal fraco | até 1 s |
| Busca de câmeras (WS-Discovery, câmera ONVIF real na LAN), do toque até a lista | 2,7 a 3,1 s (3 buscas) | até 4 s |
| Consulta ONVIF só com o endereço (simulador), de "Salvar" até a câmera na lista | menos de 2,5 s (limite da leitura da tela pelo `uiautomator`) | |
| Conexão encerrada ao ir para a tela inicial durante a visualização | até 0,54 s (socket RTSP fechado) | até 1 s |
| Decodificação | hardware: `c2.exynos.h264.decoder` (H.264) e `c2.exynos.hevc.decoder` (H.265) | |

Método da latência: o celular tira a captura de tela (`screencap`) e anota a própria hora antes dela; a diferença de relógio entre celular e PC é medida por `adb shell date` (menor ida e volta: 55 a 90 ms) e a do contêiner da câmera falsa por `docker exec date` (ida e volta de 210 ms; diferença de 3 a 5 ms). Incerteza: cerca de ±0,15 s (metade das idas e voltas, mais o instante exato da captura dentro do `screencap`, que tende a superestimar a latência). A busca a partir do celular não encontrou o simulador ONVIF do PC nesta rede (o multicast do Wi-Fi não chegou ao PC, e no Windows a porta UDP 3702 do pedido direto é do serviço de descoberta do sistema); por isso o tempo de busca foi medido com a câmera real.

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

## Desenvolver (Android)

Decisões no ADR 0006. Pré-requisitos no Windows: JDK 21 e o Android SDK de linha de comando (`platform-tools`, `platforms/android-37.2`, exigida pelo `compileSdk`, e `build-tools/36.1.0`) em `%LOCALAPPDATA%\Android\Sdk`. O wrapper baixa o Gradle 9.6.0.

```powershell
. ./scripts/env-android.ps1       # JAVA_HOME (JDK 21), ANDROID_HOME e adb no PATH
cd android
./gradlew -p build-logic test     # gerador do tema a partir de design/tokens.json
./gradlew :core:test :onvif:test :app:testDebugUnitTest :app:checkUiColors :app:checkCleartextHttp :app:lintRelease
./gradlew :app:checkReleasePermissions   # manifesto do release só com as quatro permissões
./gradlew :app:connectedDebugAndroidTest   # testes instrumentados (celular ou emulador)
```

O tema do Compose (`Tokens.kt`) é gerado no build a partir de `design/tokens.json`; não há arquivo gerado versionado.

APK de release assinado com a sua chave, fora do repositório:

1. Uma vez: `./scripts/android-keystore.ps1` cria `%USERPROFILE%\.android-keys\rtsp-viewer.jks` com a senha que você digitar. Guarde uma cópia da keystore e da senha; sem elas, novas versões não instalam por cima.
2. A cada versão: `./scripts/android-release.ps1` pede a senha e gera `android/app/build/outputs/apk/release/app-release.apk`. Instale com `adb install -r <apk>`.

O CI gera só o APK sem assinatura, como artefato; nada é publicado.

Teste manual no celular (câmera falsa, simulador ONVIF e câmera real), com a instalação do APK: `test-camera/roteiro-android.md`.

## Licenças

Veja `docs/licencas-terceiros.md`: FFmpeg sob LGPL, ligado dinamicamente; Slint sob a licença Royalty-free 2.0, com atribuição na tela "Sobre". No Android, Media3, Jetpack Compose, AndroidX e Kotlin, sob Apache-2.0.
