# Proposal

Issue: (a criar).
Épica: `epic/compat-yoosee`, criada da `main` `3c1a290` em 2026-11-27.

## Why

O usuário precisa que o app abra câmeras Yoosee (Gwell), comuns e baratas.
Hoje elas ficam de fora por três motivos:

- a Yoosee não está na lista de fabricantes, e os caminhos RTSP dela
  (`/onvif1` e `/onvif2`) não são óbvios;
- o ONVIF da Yoosee responde na porta 5000, que a consulta só com o IP não
  sonda (hoje: 80, 2020, 8000, 8080 e 8899);
- o app só pede RTSP sobre TCP, e câmeras desse tipo às vezes só aceitam RTP
  por UDP.

Decisões do usuário (2026-11-27):

- escopo: Yoosee na lista de fabricantes, porta 5000 na consulta ONVIF e
  reserva UDP quando a câmera recusar TCP, no desktop e no Android;
- o usuário ainda não tem uma Yoosee: a validação usa uma câmera falsa que
  imita a Yoosee (só UDP, caminhos `/onvif1` e `/onvif2`) e o simulador ONVIF
  na porta 5000. O teste com uma câmera real fica para quando ele tiver uma.

Fontes do padrão da Yoosee (comunitárias, não do fabricante): banco de câmeras
do iSpy e relatos do Home Assistant. Vídeo em `rtsp://<IP>:554/onvif1`
(principal) e `/onvif2` (secundário), usuário `admin`, ONVIF na porta 5000.

## What Changes

- **Fabricante Yoosee**, no desktop e no Android:
  - item "Yoosee" na escolha de fabricante, com principal `/onvif1` e
    secundário `/onvif2` na porta 554;
  - dica da busca e do formulário sobre a senha da Yoosee.
- **Porta 5000** na consulta ONVIF só com o IP, no desktop e no Android.
- **Reserva UDP:**
  - o player tenta RTSP sobre TCP, como hoje;
  - se a câmera recusar o TCP, ou aceitar e não mandar vídeo, o player tenta
    uma vez por UDP;
  - o transporte que funcionou fica lembrado por câmera enquanto o app estiver
    aberto, sem gravar nada em disco;
  - autenticação recusada, caminho inexistente e codec não suportado
    continuam sem nova tentativa.
- **Câmera falsa "Yoosee":** um segundo MediaMTX só UDP, com os caminhos
  `/onvif1` e `/onvif2` e credenciais fictícias. O simulador ONVIF já aceita
  `--port 5000` e as URLs `--main`/`--sub`, e o roteiro mostra o comando que
  o aponta para ela. Ela é usada nos testes, no CI e na ponta a ponta.
- **Documentação:** README, roteiros de teste (desktop e Android) e
  `design/telas.md` com a Yoosee; o ADR 0002 registra a reserva UDP do
  desktop e o ADR 0006 a do Android.

## Capabilities

### New Capabilities

Nenhuma.

### Modified Capabilities

- `live-view`: reprodução por TCP com reserva UDP.
- `camera-discovery`: porta 5000 na consulta ONVIF e Yoosee na lista de
  fabricantes.

## Fora do escopo

- **Acesso pela internet pela nuvem P2P da Yoosee:** o app Yoosee usa um
  protocolo proprietário da Gwell. O app continua só com RTSP e ONVIF na rede
  local (ou por VPN, como qualquer câmera).
- **PTZ, áudio, gravação e eventos.**
- **Preferência manual de transporte por câmera e mudança no `cameras.json`:**
  a reserva é automática.

## Impact

- **Desktop:** `core` (fabricante), `onvif` (portas), `media` (transporte e
  reserva) e `app` (dicas).
- **Android:** `core` (fabricante), `onvif` (portas), `app/player` (transporte
  e reserva) e `app/ui` (dicas).
- **Câmera falsa e CI:** `test-camera/compose.yaml` ganha o MediaMTX só UDP. Os
  jobs `desktop-linux` e `android-device` passam a testar a reserva.
- **Sem mudança** de schema, de formato de arquivo, de dependências nem de
  permissões. No Windows, a primeira conexão UDP pode abrir o aviso do
  firewall do sistema (design D3).
- **Boot:** nada muda, porque a rede continua só ao abrir uma câmera.
