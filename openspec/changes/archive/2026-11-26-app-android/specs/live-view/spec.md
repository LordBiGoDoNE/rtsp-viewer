## MODIFIED Requirements

### Requirement: Reprodução de uma câmera

O app, no desktop e no Android, SHALL reproduzir o vídeo H.264 ou H.265 de uma
câmera RTSP selecionada, usando RTSP sobre TCP. Nenhuma conexão SHALL ser
aberta sem o usuário abrir uma câmera.

#### Scenario: Abrir câmera disponível

- **WHEN** o usuário abre uma câmera que publica H.264 ou H.265
- **THEN** o vídeo aparece em até 2 segundos numa rede local, sem contar a espera pelo primeiro quadro-chave enviado pela câmera (que depende do intervalo entre quadros-chave configurado nela)
- **AND** a proporção da imagem é preservada

#### Scenario: Iniciar o app não conecta

- **WHEN** o app é aberto
- **THEN** nenhuma conexão RTSP é iniciada até o usuário abrir uma câmera

#### Scenario: App Android vai para segundo plano

- **WHEN** o usuário sai do app Android durante a visualização
- **THEN** a conexão é encerrada em até 1 segundo
- **AND** ao voltar ao app a câmera conecta de novo, sem ação do usuário

### Requirement: Decodificação por hardware com fallback

O player SHALL tentar decodificar por hardware (D3D11VA no Windows, VAAPI no
Linux, `MediaCodec` por hardware no Android) e SHALL usar decodificação por
software quando o hardware não estiver disponível ou falhar, sem ação do
usuário.

#### Scenario: Sem aceleração disponível

- **WHEN** a abertura do decodificador por hardware falha
- **THEN** o vídeo é reproduzido por software
- **AND** as informações da câmera indicam o modo em uso

### Requirement: Tela cheia e navegação

No desktop, o app SHALL alternar a visualização em tela cheia com F11 ou duplo
clique e SHALL sair da tela cheia com Esc. No Android, o app SHALL alternar a
tela cheia imersiva pelo botão "Tela cheia" ou por duplo toque no vídeo, e o
gesto de voltar SHALL fazer o papel do Esc. Fechar a visualização SHALL
encerrar a conexão em até 1 segundo.

#### Scenario: Entrar e sair da tela cheia

- **WHEN** o usuário pressiona F11 durante a visualização
- **THEN** o vídeo ocupa a tela inteira
- **AND** Esc volta à janela normal e um segundo Esc volta à lista

#### Scenario: Tela cheia no Android

- **WHEN** o usuário toca duas vezes no vídeo
- **THEN** o vídeo ocupa a tela inteira, sem as barras do sistema
- **AND** o gesto de voltar sai da tela cheia e um segundo gesto volta à lista

#### Scenario: Fechar com a câmera inacessível

- **WHEN** o usuário volta à lista enquanto o app ainda tenta conectar a uma câmera que não responde
- **THEN** a tela volta à lista imediatamente e a conexão é encerrada em até 1 segundo
