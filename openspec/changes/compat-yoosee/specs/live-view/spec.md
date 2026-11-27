## MODIFIED Requirements

### Requirement: Reprodução de uma câmera

O app, no desktop e no Android, SHALL reproduzir o vídeo H.264 ou H.265 de uma
câmera RTSP selecionada, usando RTSP sobre TCP e, quando a câmera não entregar
vídeo por TCP, RTP por UDP (requisitos "Reserva UDP" e "Transporte lembrado em
memória"). Nenhuma conexão SHALL ser aberta sem o usuário abrir uma câmera.

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

## ADDED Requirements

### Requirement: Reserva UDP

O player, no desktop e no Android, SHALL tentar primeiro RTSP sobre TCP e,
se a tentativa falhar antes do primeiro pacote de vídeo, SHALL tentar uma vez
por UDP na mesma abertura, sem esperar o intervalo de reconexão. O player
SHALL NOT tentar o outro transporte quando a falha não for de transporte.

#### Scenario: Câmera que só aceita UDP

- **WHEN** o usuário abre uma câmera que recusa RTP intercalado no TCP (erro de RTSP no `SETUP` ou conexão fechada depois do `DESCRIBE`) e aceita UDP
- **THEN** o vídeo aparece por UDP sem nenhuma configuração

#### Scenario: Câmera aceita TCP e não manda vídeo

- **WHEN** a câmera aceita o `PLAY` por TCP e nenhum pacote chega em 5 segundos
- **THEN** o player tenta por UDP

#### Scenario: Câmera que aceita TCP

- **WHEN** o usuário abre uma câmera que entrega vídeo por TCP
- **THEN** o vídeo aparece por TCP, sem nenhuma tentativa por UDP

#### Scenario: Senha errada numa câmera só UDP

- **WHEN** a câmera recusa as credenciais
- **THEN** o app mostra "Usuário ou senha inválidos." sem tentar o outro transporte e sem reconectar

#### Scenario: Falhas que não são de transporte

- **WHEN** o caminho não existe, não há vídeo, o codec não é suportado ou a porta RTSP não aceita conexão
- **THEN** o player trata o erro como hoje, sem tentar o outro transporte

### Requirement: Transporte lembrado em memória

O transporte que funcionou SHALL ser lembrado por câmera enquanto o app
estiver aberto, sem gravar nada em disco, e SHALL aparecer em "Informações".

#### Scenario: Informações mostram o transporte

- **WHEN** o vídeo está aparecendo
- **THEN** "Informações" mostra "Transporte: TCP" ou "Transporte: UDP", conforme a conexão

#### Scenario: Reabrir lembra o transporte

- **WHEN** o usuário fecha e abre de novo uma câmera que funcionou por UDP, com o app ainda aberto
- **THEN** a nova abertura começa por UDP

#### Scenario: Reiniciar o app esquece o transporte

- **WHEN** o app é fechado e aberto de novo
- **THEN** a primeira abertura de cada câmera começa por TCP e nada sobre o transporte foi gravado no arquivo de câmeras
