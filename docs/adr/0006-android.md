# ADR 0006: App Android

- Estado: aceito
- Data: 2026-11-25
- Spec: `app-android`
- Aprovação: usuário, 2026-11-25 (escopo igual ao do desktop, núcleo e ONVIF
  em Kotlin, testes no celular dele, APK assinado com chave local)

## Contexto

O ADR 0001 escolheu Kotlin + Jetpack Compose + Media3 para o Android, com a
identidade visual dos tokens (ADR 0004). Esta spec leva ao Android o mesmo
escopo do desktop: cadastro, visualização e ONVIF.

## Decisão

- Projeto Gradle em `android/`, com três módulos:

  | Módulo | Tipo | Papel |
  | --- | --- | --- |
  | `core` | Kotlin/JVM | domínio e aplicação (portas e casos de uso), sem SDK do Android, rede, arquivos nem serialização |
  | `onvif` | Kotlin/JVM | ONVIF e WS-Discovery, só `java.net` e `javax.xml` (ADR 0005) |
  | `app` | Android | UI Compose, player Media3, persistência, cofre, wiring |

  `core` e `onvif` não enxergam o SDK do Android: o compilador garante a
  direção `app` → `onvif` → `core`.
- Núcleo e ONVIF são reescritos em Kotlin, sem UniFFI. A duplicação com o Rust
  é controlada por testes com os mesmos casos nos dois lados.
- Versões fixas em `android/gradle/libs.versions.toml`, conferidas em
  2026-11-25 nos repositórios Maven:

  | Item | Versão |
  | --- | --- |
  | Android Gradle Plugin | 9.5.2 (exige Gradle 9.6.0 e JDK 17+) |
  | Gradle (wrapper, com SHA-256) | 9.6.0 |
  | Kotlin | 2.4.21 |
  | Compose BOM | 2026.11.01 (Material 3) |
  | Media3 | 1.11.1 |
  | AndroidX Activity / Lifecycle / Core | 1.13.0 / 2.11.1 / 1.19.1 |
  | kotlinx-coroutines / kotlinx-serialization | 1.11.0 / 1.13.0 |
  | JUnit (JVM) | 6.1.3 |

  Todas sob Apache-2.0 (JUnit sob EPL-2.0, só em teste).
- `targetSdk` 36 e `minSdk` 26; `compileSdk` 37.2, porque Compose 1.13 e
  Lifecycle 2.11 exigem compilar contra a API 37.1 ou mais nova (compilar
  contra uma API nova não muda o comportamento em tempo de execução, que segue
  o `targetSdk`). Build de release com R8. O lint trata avisos como erro, com
  as exceções deliberadas em `android/app/lint.xml`.
- Permissões: `INTERNET`, `ACCESS_NETWORK_STATE`, `ACCESS_WIFI_STATE` e
  `CHANGE_WIFI_MULTICAST_STATE`. Nenhuma de tempo de execução. As que as
  bibliotecas acrescentam no merge sem uso pelo app (`WAKE_LOCK` do Media3,
  `DYNAMIC_RECEIVER_NOT_EXPORTED_PERMISSION` do AndroidX Core) são retiradas
  com `tools:node="remove"`, e a task `checkReleasePermissions` recusa o
  manifesto mesclado do release com qualquer outra permissão, permissão
  própria declarada ou backup ligado.
- HTTP sem TLS: a configuração de segurança de rede do Android não aceita
  faixas de IP. O tráfego em texto puro fica permitido no manifesto, e o
  adaptador ONVIF do app só envia para endereços privados, link-local ou
  loopback, conferidos depois da resolução do nome, sem seguir
  redirecionamento. O app não usa HTTP para mais nada: a task
  `checkCleartextHttp` recusa cliente HTTP em `app` e `core` (só o módulo
  `onvif` usa HTTP). Ela não prova o guarda de endereço, coberto pelos testes
  do adaptador. O RTSP não passa por essa configuração.
- Senhas cifradas com AES-GCM por chave não exportável do Android Keystore;
  backup e transferência de dados desligados.
- Tema do Compose gerado no build por `android/build-logic` a partir de
  `design/tokens.json` (`Tokens.kt` em `build/generated`, nada versionado),
  com a task `checkUiColors` recusando cor literal no código da UI.
- Assinatura: keystore do usuário criada por `scripts/android-keystore.ps1` em
  `%USERPROFILE%\.android-keys\` e release assinado por
  `scripts/android-release.ps1`, que passa a senha ao Gradle só pelo ambiente
  do processo. Sem as variáveis, o release sai sem assinatura. O tipo de build
  `e2e` (igual ao release, assinado com a chave de debug do SDK e com o id
  `.e2e`) serve às medições e ao teste de ponta a ponta do agente.
- CI: job `android` (lint, testes JVM, cor literal, release sem assinatura
  como artefato) e job `android-device` (emulador API 34 com a câmera falsa).
  O CI não assina nem publica.

## Consequências

- O `targetSdk` 36 mantém o acesso à rede local sem pedido de permissão. O
  Android 17 exige `ACCESS_LOCAL_NETWORK`, pedida em tempo de execução, para
  apps com `targetSdk` 37 ou mais. Subir o `targetSdk` exige spec própria, que
  trate essa permissão.
- Duas implementações do núcleo e do ONVIF: toda mudança de regra altera as
  duas, com os mesmos casos de teste.
- Perder a keystore impede atualizar o app instalado por cima; o README
  orienta guardar uma cópia.
- O emulador do CI não enxerga a rede local real; a busca com câmera real só
  é testada no celular.
