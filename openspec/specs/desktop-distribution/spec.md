# desktop-distribution Specification

## Purpose

Metas de boot e separação do núcleo, pacotes para Windows e Linux e verificação contínua no CI do app desktop. O CI não publica nada. Criada pela change mvp-desktop-rtsp.

## Requirements

### Requirement: Boot rápido

A janela do app SHALL ficar visível e utilizável em menos de 500 ms após o
início do processo, em build release, na máquina de referência (Windows do
usuário), com inicialização a quente. O boot SHALL NOT depender de rede nem do
cofre de senhas.

#### Scenario: Medição de boot

- **WHEN** o app é iniciado com a medição de boot habilitada e 20 câmeras cadastradas
- **THEN** o tempo do início do processo até a primeira renderização da janela é menor que 500 ms na mediana de 10 execuções

### Requirement: Separação do núcleo

O crate `core` SHALL NOT depender de Slint, FFmpeg, cofre de senhas, sistema de
arquivos ou rede. Um teste automatizado SHALL falhar se essa regra for violada.

#### Scenario: Dependência proibida

- **WHEN** alguém adiciona ao `core` uma dependência de `slint` ou `ffmpeg`
- **THEN** o teste de arquitetura falha no CI

### Requirement: Pacotes para Windows e Linux

O CI SHALL produzir um `.zip` para Windows x64 (executável e somente as DLLs do
FFmpeg usadas) e um `.deb` para Ubuntu 24.04 x64, como artefatos do workflow,
sem publicá-los. O tamanho de cada pacote SHALL ser registrado no README, com
meta de até 30 MB compactado para o Windows.

#### Scenario: Pacote Windows executa sem instalação

- **WHEN** o `.zip` é extraído numa máquina Windows 10 ou 11 x64 sem FFmpeg instalado
- **THEN** o app abre e reproduz a câmera falsa

#### Scenario: Pacote Linux instala dependências

- **WHEN** o `.deb` é instalado num Ubuntu 24.04 limpo com `apt install ./pacote.deb`
- **THEN** as bibliotecas do FFmpeg são instaladas como dependência e o app abre

### Requirement: Verificação contínua

O CI SHALL executar, em todo pull request e push: formatação, lint sem avisos,
testes (inclusive testes do player contra a câmera falsa no Linux), build
Windows e Linux e o job `openspec`. O CI SHALL NOT publicar releases.

#### Scenario: PR com teste falhando

- **WHEN** um pull request quebra um teste do player contra a câmera falsa
- **THEN** o job `desktop-linux` falha

#### Scenario: Change entregue sem arquivamento

- **WHEN** um PR para `main` é aberto e a `main` tem uma change entregue fora de `openspec/changes/archive/`, sem ser o PR `[SPEC]` que a introduz nem o PR que a arquiva
- **THEN** o job `openspec` falha
