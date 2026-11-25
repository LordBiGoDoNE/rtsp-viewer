# Proposal

Issue: [#21](https://github.com/LordBiGoDoNE/rtsp-viewer/issues/21).
Épica: `epic/app-android`, criada da `main` `cac44f0` em 2026-11-25.

## Why

O desktop (Windows e Linux) já cadastra câmeras, reproduz RTSP em tela cheia e
encontra câmeras ONVIF na rede. O projeto prevê o mesmo client no Android
(ADR 0001), com a mesma identidade visual (ADR 0004). O usuário quer ver as
câmeras no celular, com a mesma leveza: abrir rápido, sem rede no boot e sem
gastar bateria à toa.

Decisões do usuário (2026-11-25):

- escopo igual ao do desktop: lista, formulário, visualização em tela cheia e
  ONVIF (busca na rede, cadastro só com o IP e fabricantes);
- núcleo e ONVIF reescritos em Kotlin (sem UniFFI), como prevê o ADR 0001;
- testes num celular Android real do usuário (inclusive com a Tapo dele);
- APK de release assinado com uma chave local do usuário, fora do repositório.

## What Changes

- Projeto Gradle novo em `android/`, com três módulos:
  - `core` (Kotlin/JVM puro): domínio e aplicação, com as mesmas regras do
    `core` do desktop (nome, URL RTSP, endereço, fabricantes, stream
    secundário, casos de uso e portas);
  - `onvif` (Kotlin/JVM, só `java.net`): cliente ONVIF e WS-Discovery com as
    mesmas regras de segurança do ADR 0005;
  - `app` (Android): UI em Jetpack Compose, player Media3/ExoPlayer RTSP,
    persistência em `cameras.json`, senhas cifradas com chave do Android
    Keystore e wiring.
- Tema do Compose gerado no build a partir de `design/tokens.json`.
- `design/telas.md` ganha as adaptações do Android (toque, gesto de voltar,
  tela cheia imersiva, dicas da busca).
- CI ganha jobs Android: build, lint, testes JVM, APK de release sem
  assinatura e testes instrumentados num emulador contra a câmera falsa. O CI
  continua sem publicar nada.
- Assinatura local: script que cria a keystore do usuário fora do repositório e
  build de release assinado só na máquina dele.
- ADR 0006: Android (módulos, bibliotecas, versões, permissões, cofre,
  assinatura, `targetSdk`).

## Out of scope

- Publicação na Play Store ou em qualquer loja, e assinatura no CI.
- UniFFI ou qualquer código Rust no Android.
- Grade com várias câmeras, snapshot e gravação (specs próprias).
- `targetSdk` 37 e a permissão `ACCESS_LOCAL_NETWORK` do Android 17 (ver
  design, D9).
- Android TV, Wear OS, Auto e tablets com layout próprio (o app roda em
  tablets com o layout do celular).
- Reprodução em segundo plano ou picture-in-picture.

## Capabilities

### New Capabilities

- `android-distribution`: build, metas de boot e tamanho, permissões, CI e APK
  assinado localmente no Android.

### Modified Capabilities

- `camera-registry`: os requisitos passam a valer para o desktop e o Android;
  no Android as senhas ficam cifradas com chave do Keystore e a lista fica no
  armazenamento privado do app.
- `live-view`: reprodução também no Android (MediaCodec por hardware com
  fallback), tela cheia por botão ou duplo toque e saída pelo gesto de voltar.
- `camera-discovery`: busca também no Android, pelo Wi-Fi ou Ethernet do
  aparelho.
- `design-tokens`: tema do Compose gerado a partir dos tokens e verificação de
  cor literal no código da UI do Android.

## Impact

- Dependências novas (só no Android, versões fixas no catálogo do Gradle):
  Media3 (`exoplayer`, `exoplayer-rtsp`), Jetpack Compose (BOM, Material 3),
  AndroidX Activity e Lifecycle, `kotlinx-coroutines`,
  `kotlinx-serialization-json` e bibliotecas de teste. Todas sob Apache-2.0;
  avisos de licença no app e em `docs/licencas-terceiros.md`.
- Máquina do desenvolvedor: Android SDK (linha de comando), JDK 21 (já
  instalado) e o celular do usuário com depuração USB.
- Áreas críticas: segredos (senhas no Keystore, credenciais enviadas à câmera),
  entrada de rede não confiável (ONVIF e WS-Discovery), concorrência (player,
  busca e consulta fora da thread principal), CI e assinatura (sem publicação).
