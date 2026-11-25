# Design

## Contexto

O desktop entregou cadastro, visualização e ONVIF (`mvp-desktop-rtsp` e
`descoberta-onvif`). Esta spec leva o mesmo escopo ao Android, em Kotlin, com
a mesma identidade visual e as mesmas regras de domínio e de segurança.
Requisitos permanentes: leve, simples, boot rápido e nada de rede no boot.

Versões conferidas em 2026-11-25. O grupo 1 fixa as versões exatas no catálogo
do Gradle e registra no ADR 0006 qualquer diferença desta tabela:

| Item | Versão |
| --- | --- |
| Media3 (`exoplayer`, `exoplayer-rtsp`) | 1.11.1 (estável; 1.12 em alfa) |
| Android Gradle Plugin | 9.x estável mais recente (Kotlin embutido no AGP 9) |
| Gradle | wrapper na versão exigida pelo AGP escolhido |
| Compose | BOM estável mais recente, Material 3 |
| `targetSdk` | 36 (Android 16) |
| `compileSdk` | 37.2 (exigido por Compose 1.13 e Lifecycle 2.11; ajuste do grupo 1) |
| `minSdk` | 26 (Android 8.0) |
| JDK | 21 (já instalado) |

## Objetivos

- O mesmo comportamento do desktop: cadastro (URL completa ou só o endereço),
  visualização com estados e reconexão, tela cheia, busca e consulta ONVIF,
  fabricantes.
- Lista de câmeras visível em até 1 s numa partida a frio, sem rede no boot.
- APK de release pequeno (meta: até 15 MB).
- Nenhuma senha em texto puro: nem em arquivo, nem em log, nem na rede.

## Não objetivos

Ver "Out of scope" da proposta.

## Estrutura

```
android/
  settings.gradle.kts, build.gradle.kts, gradle/libs.versions.toml, gradlew
  build-logic/          plugin de build: gera o tema do Compose dos tokens
  core/                 Kotlin/JVM: domínio, aplicação, portas (sem Android)
  onvif/                Kotlin/JVM: ONVIF e WS-Discovery (só java.net)
  app/                  Android: ui/, infrastructure/, wiring (Application)
```

Direção de dependências: `app` → `onvif` → `core`. `core` e `onvif` são
módulos JVM: não enxergam o SDK do Android, o que o compilador garante. Um
teste de arquitetura confere ainda que o `core` não depende de `onvif`, de
rede (`java.net`), de arquivos (`java.io.File`, `java.nio.file`) nem de
bibliotecas de serialização, como o `core` do desktop.

## Decisões

### D1. Núcleo em Kotlin espelhando o desktop

- Tipos e regras iguais aos do `core` em Rust: `Camera` (identidade, métodos de
  intenção), `CameraName` (1 a 60 caracteres, não só espaços), `RtspUrl`
  (`rtsp`/`rtsps`, host obrigatório, credenciais extraídas), endereço da câmera
  (`host` ou `host:porta`), fabricantes e caminhos (D5 da `descoberta-onvif`),
  stream secundário (mesmo host do principal; trocar a principal o descarta) e
  redação de credenciais em mensagens.
- Casos de uso: catálogo (listar, salvar, remover com o cofre) e cadastro por
  endereço (ONVIF primeiro, fabricante como fallback explícito).
- Portas: `CameraRepository`, `CredentialStore`, `CameraDiscovery`,
  `StreamResolver`. O player não é porta do `core`: é adaptador da UI (como no
  desktop).
- Os testes do `core` em Kotlin cobrem os mesmos casos dos testes em Rust
  (mesmas entradas e saídas esperadas), para as duas implementações não
  divergirem.
- Código síncrono e sem framework. A UI chama os casos de uso em
  `Dispatchers.IO`.

### D2. Persistência e cofre

- `cameras.json` no armazenamento privado do app (`filesDir`), mesmo formato
  versão 1 do desktop (campos `id`, `name`, `url`, `username`,
  `substream_url`), escrita atômica como no desktop (temporário, sync e
  rename; ajuste do grupo 2). Arquivo ilegível:
  lista vazia, aviso e original preservado com o sufixo
  `.corrompido-<data e hora>`.
- Senhas: chave AES-256 no Android Keystore (não exportável, alias
  `rtsp-viewer-credentials`), AES/GCM com IV aleatório por gravação. O texto
  cifrado e o IV ficam em `credentials.json` no `filesDir`, por id da câmera.
  Senha que não decifra (chave invalidada, arquivo restaurado de outro
  aparelho) é tratada como ausente: o app pede a senha ao abrir a câmera
  (diálogo "Senha de <usuário>").
- Backup desligado (`allowBackup="false"` e regras de extração vazias): a chave
  do Keystore não vai junto no backup, então dados cifrados restaurados seriam
  inúteis, e a lista de câmeras revela a rede do usuário.
- O Keystore existe em todo aparelho com API 26+; não há o caso "sem cofre" do
  Linux.

### D3. Player Media3 RTSP

- `ExoPlayer` com `RtspMediaSource.Factory().setForceUseRtpTcp(true)` (RTSP
  sobre TCP, como no desktop) e credenciais passadas pela URL só na memória do
  player, nunca em log.
- Baixa latência: `DefaultLoadControl` com buffers pequenos (valores fixados e
  medidos no grupo 4) e reprodução no ponto ao vivo; quadros atrasados são
  descartados pelo renderizador. Meta do requisito: até 1 s com a câmera
  falsa.
- Decodificação: `MediaCodec` (hardware) por padrão, com fallback para decoder
  de software habilitado (`DefaultRenderersFactory` com
  `setEnableDecoderFallback(true)`). O painel "Informações" mostra o nome do
  decoder em uso e se é hardware ou software
  (`MediaCodecInfo.isHardwareAccelerated`).
- Vídeo numa `SurfaceView` (sem cópia de quadros pela CPU).
- Estados e reconexão iguais ao desktop: conectando, reproduzindo,
  reconectando (1, 2, 4, 8, 16 e 30 s, reinício após 10 s estáveis) e erro.
  401 vira "Usuário ou senha inválidos." sem reconexão; codec sem decoder vira
  "Formato de vídeo não suportado: <codec>".
  Limitação do Media3 1.11: o RTSP dele descarta do SDP as trilhas sem
  depacketizador (MJPEG, por exemplo), e o codec não chega ao app. Sem
  nenhuma trilha reproduzível, a mensagem é "Formato de vídeo não suportado:
  desconhecido", sem reconexão; se sobrar só áudio, é "A câmera não envia
  vídeo.".
- Sair da visualização ou levar o app para segundo plano libera o player em
  até 1 s. Ao voltar, a câmera reconecta. O `release()` tem prazo de 300 ms e
  a conexão RTSP usa sockets próprios (`setSocketFactory`) com prazo de
  conexão de 5 s, abortados ao fechar: o `connect` do Media3 não tem prazo e
  prenderia a thread de reprodução com a câmera sem responder.
- Plano B, se o Media3 não cumprir a latência ou não abrir as câmeras reais:
  parar e decidir com o usuário (libVLC ou outra opção), sem trocar a
  biblioteca por conta própria.

### D4. ONVIF em Kotlin

Mesmo protocolo e mesmas regras do desktop (`descoberta-onvif` D1–D4 e ADR
0005), em Kotlin, só com `java.net`, `javax.xml` desabilitado para DTD e
`java.security`:

- SOAP 1.2 via `HttpURLConnection`, sem TLS; prazo total de 10 s por consulta,
  2 s de conexão por chamada, respostas até 256 KiB.
- XML com parser da plataforma sem DTD nem entidades externas, profundidade
  limitada, parse por nome local (SAX, não StAX: ver os ajustes do grupo 3
  abaixo).
- WS-UsernameToken PasswordDigest com compensação do relógio da câmera e HTTP
  Digest (MD5, `qop=auth`) na repetição após 401.
- Sondagem das portas 80, 2020, 8000, 8080 e 8899 em paralelo quando o
  endereço não tem porta.
- Destino das credenciais: chamadas autenticadas e a URL RTSP devolvida vão
  sempre para o host da câmera, mantendo esquema, porta e caminho.
- WS-Discovery: multicast `239.255.255.250:3702` com TTL 1 e o mesmo Probe por
  unicast aos hosts da sub-rede privada (RFC 1918, /24 do endereço quando a
  rede é maior, até 254 por interface e 1024 no total), escuta de 3 s,
  validação de `RelatesTo`, origem e `XAddrs` (só IP literal privado,
  link-local ou loopback, sem `user@`), fila de 64 respostas, até 256
  dispositivos, deduplicação por `EndpointReference`.
- As interfaces (endereço e prefixo) vêm de quem chama: no Android, o adaptador
  lê as redes ativas pelo `ConnectivityManager` (`LinkProperties`), só Wi-Fi e
  Ethernet; nos testes, a lista é injetada.
- A busca segura um `WifiManager.MulticastLock` só enquanto dura e o solta ao
  terminar ou cancelar.

Ajustes do grupo 3:

- XML pelo SAX de `javax.xml.parsers`, não pelo StAX: o `javax.xml.stream`
  não existe no Android (conferido no `android.jar` das plataformas 36 e
  37.2). Documento com `DOCTYPE` é recusado antes do parse (sem DTD não há
  entidade declarada nem externa); as features de entidade externa e
  `disallow-doctype-decl` são ligadas onde a plataforma as reconhece;
  profundidade até 64 e parse por nome local, sem exigir prefixo declarado.
- O host da câmera é resolvido uma vez por consulta por um `HostResolver`
  injetado no cliente, e todas as chamadas HTTP da consulta vão ao IP
  devolvido (sem nova consulta DNS); a URL RTSP continua com o host da câmera
  (o nome digitado). No app, o `LocalNetworkResolver` aplica a regra do D8 e
  do ADR 0006: nome ou IP que não resolve para endereço privado, link-local
  ou loopback vira "A câmera não respondeu: <host> não é um endereço da rede
  local..." sem nada enviado. Efeito colateral: o cabeçalho `Host` leva o IP.
- `HttpURLConnection` sem proxy (`Proxy.NO_PROXY`: o proxy do Wi-Fi não
  recebe o tráfego da câmera) e sem redirecionamento; o prazo total vale
  também na leitura (um vigia desconecta no prazo).
- Cada socket da busca é preso à rede dele (`Network.bindSocket`), para o
  envio não sair pela rede padrão (dados móveis ou VPN).

### D5. UI em Compose

- Uma `Activity`, navegação por estado num `ViewModel` (sem biblioteca de
  navegação): lista, formulário, fabricantes, visualização, procurar câmeras e
  sobre.
- Telas e textos de `design/telas.md`, com as adaptações do Android descritas
  numa seção própria:
  - "Voltar" e o gesto de voltar do sistema fazem o papel do Esc;
  - tela cheia por botão "Tela cheia" ou duplo toque no vídeo, em modo
    imersivo (barras do sistema escondidas), e a rotação segue o sensor;
  - ações da linha ("Editar", "Remover") num menu da própria linha;
  - dicas de "Nenhuma câmera encontrada" falam de Wi-Fi em vez do firewall do
    Windows;
  - "Sobre" lista as licenças de terceiros (sem o componente do Slint).
- Busca, consulta e abertura do player com sessão: resultado de uma operação
  cancelada ou antiga é ignorado (mesmo padrão do desktop).
- A tela fica ligada enquanto o vídeo está em reprodução (`keepScreenOn`).

### D6. Tema gerado dos tokens

- Plugin em `android/build-logic` com uma task que lê `design/tokens.json` e
  gera `Theme.kt` (cores, tamanhos de fonte, espaçamentos, raios e tamanhos)
  em `build/generated`, registrada como fonte do módulo `app`. Nada gerado é
  versionado.
- Token inválido ou ausente faz o build falhar com o nome do token.
- Px lógicos dos tokens viram `dp` (e `sp` para fontes). Tokens só do desktop
  (`window-*`) são ignorados no Android.
- Verificação no CI: código Kotlin da UI (`app/src/main/.../ui`) que use
  `Color(0x...)` ou `Color.<nome>` literal fora do tema gerado falha o job.

### D7. Build, assinatura e CI

- Build de release com R8 (minificação e recursos encolhidos), um APK
  universal.
- Assinatura local: `scripts/android-keystore.ps1` cria, uma vez, a keystore
  em `%USERPROFILE%\.android-keys\rtsp-viewer.jks` (fora do repositório), com a
  senha digitada pelo usuário. O Gradle lê caminho e senhas de variáveis de
  ambiente; sem elas, o release sai sem assinatura. Nada disso é versionado
  (`.gitignore` cobre `*.jks`, `*.keystore` e `local.properties`).
- CI (`.github/workflows/ci.yml`), sem publicar e sem assinar:
  - job `android`: lint do Android, testes JVM de `core`, `onvif` e `app`,
    verificação de cor literal e `assembleRelease` sem assinatura, com o
    tamanho do APK no log e o APK como artefato de 14 dias;
  - job `android-device`: emulador (API 34, x86_64, KVM do runner Linux) com a
    câmera falsa no host (`10.0.2.2:8554`), rodando os testes instrumentados
    (player, cofre, persistência e UI).
- `act`: o job `android` roda localmente; o `android-device` não (sem
  emulador no contêiner) e é coberto pelos testes instrumentados no celular do
  usuário e pelo CI remoto.

### D8. Permissões e rede

- Manifesto: `INTERNET`, `ACCESS_NETWORK_STATE`, `ACCESS_WIFI_STATE` e
  `CHANGE_WIFI_MULTICAST_STATE`. Nenhuma permissão perigosa (de tempo de
  execução).
- Tráfego sem TLS só para endereços da rede local, para o ONVIF em HTTP. A
  `network_security_config` do Android não aceita faixas de IP, então ela
  permite o texto puro e o adaptador ONVIF do app só envia para endereços
  privados, link-local ou loopback, conferidos depois da resolução do nome
  (ajuste do grupo 1, ADR 0006). RTSP não passa por essa configuração.
- Nada de rede no boot: o app só abre conexão ao abrir uma câmera, consultar
  ou procurar.

### D9. `targetSdk` 36 e a rede local do Android 17

O Android 17 bloqueia a rede local para apps com `targetSdk` 37 ou mais, que
precisam pedir `ACCESS_LOCAL_NETWORK` em tempo de execução. Esta spec usa
`targetSdk` 36 (exigido pela Play Store desde 31/08/2026), que mantém o acesso
à rede local sem pedido. Subir para 37 fica para uma spec futura, que trata a
permissão e o texto de pedido. O ADR 0006 registra isso.

### D10. Testes

- `core`: JUnit 5 no JVM, com os mesmos casos do `core` em Rust.
- `onvif`: JUnit 5 no JVM, contra um simulador ONVIF em Kotlin dentro dos
  testes (HTTP do JDK e UDP em loopback), com amostras equivalentes às do
  desktop (Profile S genérico, Tapo, Hikvision, Dahua; sem dados reais).
  Verifica PasswordDigest, relógio, HTTP Digest, troca de host, limites e
  busca por unicast.
- `app` no JVM: `ViewModel` com portas falsas (estados, sessões, cancelamento,
  fabricantes, "Já cadastrada", nada de rede no boot).
- `app` instrumentado (emulador no CI e celular do usuário): cofre no Keystore
  real, persistência e arquivo corrompido, player contra a câmera falsa
  (abertura, latência, queda e volta, 401, codec não suportado) e testes de UI
  do Compose nos fluxos principais.
- Ponta a ponta (agente): APK de release assinado instalado no celular do
  usuário por `adb`, câmera falsa e simulador ONVIF do desktop (`fake_onvif`)
  no PC, na mesma rede; testes instrumentados apontando para o IP do PC e
  medições por `adb`. O usuário testa com a Tapo dele.

## Riscos e mitigação

| Risco | Mitigação |
| --- | --- |
| Latência do Media3 RTSP acima de 1 s | Buffers pequenos e medição no grupo 4; se não bastar, parar e decidir com o usuário (D3) |
| Câmera real com RTSP que o Media3 não abre | Teste com a Tapo no roteiro; mesma decisão do item acima |
| Wi-Fi do celular descarta multicast | Unicast na sub-rede (D4), como no desktop |
| Lógica duplicada entre Rust e Kotlin diverge | Mesmos casos de teste nos dois `core` e no ONVIF; ADR 0006 registra o custo |
| Emulador no CI lento ou instável | Job separado (`android-device`), com tempo limite; testes JVM no job principal |
| Chave de assinatura perdida | Keystore fora do repositório, com orientação de cópia de segurança no README |

## Questões abertas

Nenhuma bloqueante.
