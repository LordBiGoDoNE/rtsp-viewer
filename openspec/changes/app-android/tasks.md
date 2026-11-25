# Tasks

Issue: [#21](https://github.com/LordBiGoDoNE/rtsp-viewer/issues/21).
Épica: `epic/app-android`. Spec formalizada em 2026-11-25.
Implementação autorizada em 2026-11-25 (grupos 1–6). Teste local pelo usuário:
sim (no celular). Merges dependem da aprovação do pacote final.

Os grupos formam uma cadeia (cada um parte da branch do anterior). Em cada
grupo: testes do projeto, CI local via `act` (job `android`; o
`android-device` não roda no `act`) e revisão por grupo (`group-reviewer`)
antes de abrir o PR. Áreas críticas: CI e assinatura (grupo 1), segredos
(grupos 2 e 3), entrada de rede não confiável (grupo 3), concorrência (grupos
4 e 5).

Pré-requisitos na máquina, com cada download confirmado com o usuário: Android
SDK de linha de comando (`cmdline-tools`, `platform-tools`, plataforma 36,
`build-tools`). Para os testes instrumentados e a ponta a ponta: o celular do
usuário com depuração USB.

## 1. Fundação Android (`task/app-android/fundacao`, base: épica)

- [ ] 1.1 Projeto Gradle em `android/` (wrapper, catálogo de versões fixas
  conferidas na documentação atual, módulos `core`, `onvif` e `app`, `minSdk`
  26, `targetSdk` 36, R8 no release); `.gitignore` para `local.properties`,
  `*.jks` e `*.keystore`.
- [ ] 1.2 `build-logic`: geração do tema do Compose a partir de
  `design/tokens.json` (falha com o nome do token inválido), com testes; e a
  verificação de cor literal na UI.
- [ ] 1.3 Manifesto com as permissões mínimas, backup desligado e
  `network_security_config` (HTTP só na rede local); app vazio que abre com o
  tema gerado.
- [ ] 1.4 Assinatura local: `scripts/android-keystore.ps1` (keystore fora do
  repositório) e release assinado por variáveis de ambiente.
- [ ] 1.5 CI: jobs `android` (lint, testes JVM, cor literal, release sem
  assinatura como artefato) e `android-device` (emulador com a câmera falsa
  em `10.0.2.2`).
- [ ] 1.6 ADR 0006 (Android: módulos, bibliotecas e versões, permissões,
  cofre, assinatura, `targetSdk` 36 e a rede local do Android 17); README
  (desenvolver no Android).

## 2. Núcleo e persistência (`task/app-android/nucleo`, base: grupo 1)

- [x] 2.1 `core`: `Camera`, `CameraName`, `RtspUrl`, endereço da câmera,
  fabricantes, stream secundário e redação de credenciais, com os mesmos
  casos de teste do `core` em Rust.
- [x] 2.2 `core`: portas e casos de uso (catálogo e cadastro por endereço com
  fallback de fabricante), testados com portas falsas; teste de arquitetura do
  `core`.
- [x] 2.3 `app/infrastructure`: `cameras.json` (formato versão 1, escrita atômica,
  arquivo corrompido preservado), com testes JVM e instrumentados.
- [x] 2.4 `app/infrastructure`: cofre com chave AES do Keystore (AES-GCM, IV
  por gravação, senha que não decifra tratada como ausente), com testes
  instrumentados; nenhuma senha em log.

## 3. ONVIF em Kotlin (`task/app-android/onvif`, base: grupo 2)

- [x] 3.1 `onvif`: SOAP 1.2, parser XML sem DTD e com limites (SAX; o StAX
  não existe no Android, ajuste do grupo 3 no D4), PasswordDigest com relógio
  da câmera e HTTP Digest.
- [x] 3.2 `onvif`: cliente (informações, capacidades, perfis, URL do stream,
  principal e secundário, host sempre o da câmera, sondagem de portas, prazo
  total de 10 s).
- [x] 3.3 `onvif`: WS-Discovery (multicast e unicast na sub-rede, validação,
  limites, deduplicação), com as interfaces injetadas.
- [x] 3.4 Simulador ONVIF em Kotlin nos testes e testes no JVM equivalentes
  aos do desktop (inclusive senha nunca em texto puro).
- [x] 3.5 `app/infrastructure`: adaptadores das portas `CameraDiscovery` e
  `StreamResolver`, interfaces pelo `ConnectivityManager` (só Wi-Fi e
  Ethernet) e `MulticastLock` durante a busca.

## 4. Player Media3 (`task/app-android/player`, base: grupo 3)

- [ ] 4.1 Adaptador Media3: RTSP sobre TCP, buffers de baixa latência,
  decoder por hardware com fallback, informações (codec, resolução, decoder).
- [ ] 4.2 Estados e reconexão (1–30 s, reinício após 10 s estáveis), 401 sem
  reconexão, codec não suportado, liberação em até 1 s ao sair ou ir para
  segundo plano.
- [ ] 4.3 Testes instrumentados contra a câmera falsa (abrir H.264 e H.265,
  latência com o relógio na imagem, queda e volta, 401, codec não suportado).

## 5. Interface Compose (`task/app-android/interface`, base: grupo 4)

- [ ] 5.1 `design/telas.md`: seção de adaptações do Android (D5).
- [ ] 5.2 Lista (vazia, avisos, menu da linha, remoção com confirmação),
  formulário (validação, consulta ONVIF com cancelar, fabricantes) e sobre
  (licenças).
- [ ] 5.3 Visualização (estados, informações, senha pedida, tela cheia
  imersiva por botão e duplo toque, gesto de voltar, tela ligada).
- [ ] 5.4 Procurar câmeras (estados, "Já cadastrada", dicas do Android).
- [ ] 5.5 `ViewModel` com sessões (resultado antigo ignorado), testes JVM com
  portas falsas e testes de UI do Compose.

## 6. Fechamento (`task/app-android/fechamento`, base: grupo 5)

- [ ] 6.1 Medições no celular do usuário: partida a frio com 20 câmeras,
  tamanho do APK, latência e tempo de busca; registro no README.
- [ ] 6.2 `docs/licencas-terceiros.md`: bibliotecas do Android.
- [ ] 6.3 Roteiro de teste no celular (câmera falsa, simulador ONVIF e a Tapo
  do usuário) e instruções de instalação do APK.
- [ ] 6.4 Teste de ponta a ponta do sistema local com a cadeia completa, no
  celular do usuário (`teste-local.md`).
