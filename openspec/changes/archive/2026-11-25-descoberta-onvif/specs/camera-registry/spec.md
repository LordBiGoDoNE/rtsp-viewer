## ADDED Requirements

### Requirement: Cadastro só com o endereço da câmera

O formulário de câmera SHALL aceitar, no campo de endereço, uma URL RTSP
completa ou só o endereço da câmera (IP, nome de host ou `host:porta`, sem
esquema). Só o endereço SHALL disparar a consulta dos streams (capability
`camera-discovery`) ao salvar. O que é gravado continua sendo uma URL RTSP sem
credenciais.

#### Scenario: Endereço só com IP

- **WHEN** o usuário informa `192.168.1.164` no endereço e salva
- **THEN** o app consulta a câmera e grava a URL RTSP obtida, sem credenciais

#### Scenario: Endereço inválido

- **WHEN** o usuário informa um endereço que não é URL RTSP nem host válido (por exemplo `http://x` ou `192.168.1.164:abc`)
- **THEN** o app não salva e explica o formato aceito junto ao campo

### Requirement: Stream secundário opcional

Uma câmera SHALL poder ter um stream secundário opcional, com as mesmas
credenciais do principal. Ele é gravado no `cameras.json` como campo opcional,
sem senha, e um arquivo sem esse campo SHALL continuar carregando. A
visualização continua usando o stream principal.

#### Scenario: Secundário obtido pelo ONVIF

- **WHEN** a câmera tem dois perfis de vídeo com resoluções diferentes
- **THEN** o de menor resolução é gravado como stream secundário

#### Scenario: Arquivo da versão anterior

- **WHEN** o app abre um `cameras.json` gravado antes desta mudança
- **THEN** todas as câmeras carregam, sem stream secundário e sem aviso de arquivo corrompido
