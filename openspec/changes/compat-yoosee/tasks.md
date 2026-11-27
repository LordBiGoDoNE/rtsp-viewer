# Tasks

Issue: [#31](https://github.com/LordBiGoDoNE/rtsp-viewer/issues/31).
Épica: `epic/compat-yoosee`. Spec formalizada em 2026-11-27.
Implementação autorizada em 2026-11-27 (grupos 1–4). Teste local pelo usuário:
sim. Merges dependem da aprovação do pacote final.

## Como os grupos andam

- **Cadeia:** os grupos formam uma cadeia, e cada um parte da branch do
  anterior. Os grupos 1 e 2 mexem nas mesmas telas do desktop (dicas e
  "Informações"), e o fechamento precisa de todos.
- **Antes de cada PR:**
  - testes do projeto;
  - CI local via `act` (jobs `desktop-linux`, `android` e `openspec`; o
    `desktop-windows` e o `android-device` não rodam no `act`);
  - revisão por grupo (`group-reviewer`).
- **Áreas críticas:**
  - entrada de rede não confiável (grupos 2 e 3);
  - concorrência (reserva dentro da sessão do player, grupos 2 e 3);
  - CI (grupos 2 e 3).
- **Sem câmera real:** o usuário ainda não tem uma Yoosee. Tudo é validado com
  a câmera falsa só UDP e com o simulador ONVIF na porta 5000. O teste com
  uma Yoosee real fica registrado como não verificado.

## 1. Yoosee e porta 5000 (`task/compat-yoosee/fabricante`, base: épica)

- [ ] 1.1 `Vendor::Yoosee` no `core` do desktop e `Vendor.Yoosee` no `core`
  do Android (D1): rótulo, `/onvif1` e `/onvif2`, porta 554 ou a digitada.
  Os testes repetem os casos nos dois núcleos.
- [ ] 1.2 Porta 5000 em `DEFAULT_PORTS` no `onvif` do desktop e do Android
  (D2), com teste de integração: simulador na porta 5000 encontrado só com o
  IP, sem aumentar o prazo da consulta.
- [ ] 1.3 Escolha de fabricante com "Yoosee" no fim da lista e dica da senha
  da Yoosee nas telas de busca e de escolha de fabricante (Slint e Compose).
  Testes de UI dos dois apps.
- [ ] 1.4 `design/telas.md`: lista de fabricantes e dica da Yoosee.

## 2. Câmera falsa só UDP e reserva UDP no desktop (`task/compat-yoosee/udp-desktop`, base: grupo 1)

- [ ] 2.1 `test-camera/compose.yaml` (D5):
  - serviço `mediamtx-udp` em `:8556`, com `rtspTransports: [udp]` e RTP/RTCP
    em `8010`/`8011`;
  - caminhos `onvif1` e `onvif2` publicados por `ffmpeg`;
  - usuário `admin`, senha fictícia `yoosee123`;
  - `test-camera/README.md` atualizado.
- [ ] 2.2 Reserva UDP no `media` (D3):
  - decisão pura `should_try_udp` sobre o erro e a fase da tentativa, com
    testes unitários de cada caso, inclusive autenticação, 404, sem vídeo,
    codec e porta fechada;
  - reabertura por UDP na mesma sessão;
  - 5 s sem pacotes depois do `PLAY`;
  - opções UDP de baixa latência.
- [ ] 2.3 Memória do transporte por câmera, em memória e chaveada pela URL
  sem credenciais. "Informações" mostra "Transporte: TCP/UDP". Teste de que o
  `cameras.json` não muda.
- [ ] 2.4 Testes de integração contra a câmera falsa:
  - `/onvif1` só UDP abre por UDP;
  - `/h264` continua em TCP;
  - senha errada no só UDP dá erro de autenticação sem laço;
  - reabrir começa pelo transporte lembrado.
- [ ] 2.5 CI: `desktop-linux` sobe o `mediamtx-udp` e roda os testes do 2.4.
  ADR 0002 com a reserva UDP e o aviso do firewall do Windows.

## 3. Reserva UDP no Android (`task/compat-yoosee/udp-android`, base: grupo 2)

- [ ] 3.1 `shouldTryUdp` pura no `app/player`, com testes JVM dos mesmos casos
  do 2.2 (D4).
- [ ] 3.2 `Media3Engine` e `PlayerController`:
  - nova fonte com `setForceUseRtpTcp(false)` na reserva;
  - 5 s sem dados depois do `PLAY`;
  - prazo do primeiro quadro contado a partir da tentativa que conectou;
  - sessões antigas ignoradas;
  - parar em até 1 s, também durante a reserva.
  - Testes JVM com motor falso e tempo virtual.
- [ ] 3.3 Memória do transporte no `AppContainer` e linha "Transporte" em
  "Informações", com teste de UI.
- [ ] 3.4 Testes instrumentados contra a câmera falsa só UDP:
  - no celular;
  - no emulador do `android-device`, se o UDP atravessar o NAT do emulador
    (D4); senão, só no celular, com a decisão registrada no design.
  - O CI sobe o `mediamtx-udp`.
- [ ] 3.5 ADR 0006 com a reserva UDP do Android.

## 4. Fechamento (`task/compat-yoosee/fechamento`, base: grupo 3)

- [ ] 4.1 README:
  - Yoosee nas dicas de cadastro (porta 5000, `/onvif1`, usuário `admin`),
    marcada como não verificada numa câmera real;
  - reserva UDP e aviso do firewall do Windows;
  - fora do escopo: nuvem P2P da Yoosee.
- [ ] 4.2 Roteiros `test-camera/roteiro.md` e `test-camera/roteiro-android.md`:
  seção "Yoosee", com a câmera falsa só UDP, o simulador na porta 5000 (com o
  comando) e, depois, uma Yoosee real.
- [ ] 4.3 Medições com a câmera falsa só UDP, no desktop e no celular:
  - primeira imagem, contando a tentativa TCP recusada;
  - latência.
  - Metas: 2 s e 1 s.
- [ ] 4.4 Ponta a ponta do agente no desktop e no celular:
  - cadastro só com o IP (porta 5000);
  - fabricante Yoosee;
  - vídeo por UDP;
  - câmeras atuais ainda em TCP.
