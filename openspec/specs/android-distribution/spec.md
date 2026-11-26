# android-distribution Specification

## Purpose

App Android em Kotlin + Jetpack Compose + Media3, com boot rápido, núcleo separado de Android e rede, só as permissões necessárias e APK de release assinado com a chave local do usuário, fora do repositório; o CI compila e testa, sem assinar nem publicar. Criada pela change app-android.

## Requirements

### Requirement: Boot rápido no Android

A lista de câmeras SHALL ficar visível em até 1 s numa partida a frio do APK
de release, no celular do usuário. O boot SHALL NOT abrir conexões de rede nem
decifrar senhas.

#### Scenario: Medição de partida a frio

- **WHEN** o app de release, com 20 câmeras cadastradas, é iniciado a frio 10 vezes por `adb shell am start -W`
- **THEN** a mediana do `TotalTime` é de até 1000 ms

#### Scenario: Boot sem rede

- **WHEN** o app é aberto
- **THEN** nenhuma conexão RTSP, consulta ONVIF ou busca é iniciada até o usuário pedir

### Requirement: Separação do núcleo no Android

Os módulos `core` e `onvif` do Android SHALL ser módulos Kotlin/JVM, sem o SDK
do Android. O `core` SHALL NOT depender do `onvif`, de rede, de sistema de
arquivos nem de bibliotecas de serialização. Um teste automatizado SHALL falhar
se essa regra for violada.

#### Scenario: Dependência proibida no core

- **WHEN** alguém usa `java.net` ou `java.io.File` em código do `core`
- **THEN** o teste de arquitetura falha no CI

### Requirement: APK assinado localmente

O build de release SHALL gerar um APK universal minificado. A assinatura SHALL
usar uma keystore do usuário fora do repositório, com caminho e senhas lidos
de variáveis de ambiente. Sem essas variáveis, o release SHALL sair sem
assinatura. O tamanho do APK SHALL ser registrado no README, com meta de até
15 MB.

#### Scenario: Release assinado na máquina do usuário

- **WHEN** o usuário define as variáveis da keystore e gera o release
- **THEN** o APK sai assinado e instala por cima da versão anterior assinada com a mesma chave

#### Scenario: Nada de chave no repositório

- **WHEN** a keystore e o `local.properties` existem na máquina
- **THEN** o `git status` não os mostra, porque estão fora do repositório ou no `.gitignore`

### Requirement: Permissões mínimas

O app SHALL declarar somente `INTERNET`, `ACCESS_NETWORK_STATE`,
`ACCESS_WIFI_STATE` e `CHANGE_WIFI_MULTICAST_STATE`, sem permissões de tempo de
execução. Tráfego HTTP sem TLS SHALL ser permitido somente para endereços da
rede local. O backup automático SHALL estar desligado.

#### Scenario: Manifesto do release

- **WHEN** o manifesto do APK de release é inspecionado
- **THEN** ele contém só as quatro permissões e `allowBackup` falso

### Requirement: Verificação contínua do Android

O CI SHALL executar, em todo pull request e push: lint do Android, testes JVM
dos módulos `core`, `onvif` e `app`, verificação de cor literal na UI, build de
release sem assinatura (APK como artefato do workflow) e testes instrumentados
num emulador contra a câmera falsa. O CI SHALL NOT assinar nem publicar o APK.

#### Scenario: Teste instrumentado falhando

- **WHEN** um pull request quebra o teste do player contra a câmera falsa
- **THEN** o job `android-device` falha

#### Scenario: Release sem segredos no CI

- **WHEN** o CI gera o APK de release
- **THEN** o APK sai sem assinatura e nenhum segredo de assinatura é usado
