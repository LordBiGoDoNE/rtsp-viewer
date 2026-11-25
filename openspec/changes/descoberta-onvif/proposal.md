# Proposal

Issue: [#12](https://github.com/LordBiGoDoNE/rtsp-viewer/issues/12).
Épica: `epic/descoberta-onvif`, criada da `main` `a1c573e` em 2026-11-24.

## Why

No teste da `mvp-desktop-rtsp`, o usuário achou pouco intuitivo digitar a URL
RTSP completa (`rtsp://192.168.1.120:554/stream1`). Ele quer digitar só o IP e
que o app encontre as câmeras sozinho. Só o IP não basta: o caminho do stream
muda por fabricante. A solução é perguntar à própria câmera pelo ONVIF, que
devolve as URLs RTSP, e usar o WS-Discovery para encontrar as câmeras na rede.

Por decisão do usuário (2026-11-24), esta spec vem antes de `app-android`, cobre
só o desktop e usa uma lista de fabricantes quando a câmera não responder ao
ONVIF. Ela guarda também o stream secundário, para preparar a spec da grade, e
só procura câmeras quando o usuário pedir.

## What Changes

- Novo crate `onvif` no workspace `desktop/`, adaptador de rede, com:
  - cliente ONVIF: `GetSystemDateAndTime`, `GetDeviceInformation`,
    `GetCapabilities`, `GetProfiles` e `GetStreamUri`;
  - autenticação WS-UsernameToken (PasswordDigest), com compensação do relógio
    da câmera, e HTTP Digest quando a câmera exigir;
  - WS-Discovery: busca multicast em todas as interfaces de rede, mais o
    mesmo Probe por unicast aos hosts da sub-rede privada de cada uma (câmeras
    que ignoram multicast), por cerca de 3 s, só quando o usuário pede.
- `core`:
  - porta `CameraDiscovery` e porta `StreamResolver`;
  - lista de fabricantes com os caminhos RTSP conhecidos;
  - caso de uso que, a partir do endereço da câmera e das credenciais, resolve
    os streams principal e secundário (ONVIF primeiro, fabricante depois);
  - `Camera` ganha o stream secundário opcional, também no `cameras.json`
    (campo opcional, compatível com o arquivo atual).
- UI:
  - botão "Procurar câmeras" e tela com as câmeras encontradas;
  - formulário que aceita só o IP (ou `IP:porta`) além da URL completa;
  - consulta em segundo plano com estados visíveis;
  - escolha de fabricante quando o ONVIF falhar.
- Simulador ONVIF local (exemplo do crate `onvif`), que responde ao
  WS-Discovery e às consultas, para os testes e para o teste de ponta a ponta,
  junto com a câmera falsa.
- `AGENTS.md`: corrige a referência desatualizada ao ADR da stack.

## Out of scope

- Android (spec `app-android`, que reaproveita o comportamento descrito aqui).
- Grade com várias câmeras (spec `grade-multi-camera`); esta spec só guarda o
  stream secundário.
- PTZ, eventos, gravação na câmera, configuração da câmera pelo ONVIF, ONVIF
  sobre HTTPS e descoberta fora da rede local (outras sub-redes, nuvem do
  fabricante).
- Busca automática no boot ou em segundo plano.

## Capabilities

### New Capabilities

- `camera-discovery`: busca de câmeras na rede e consulta ONVIF dos streams,
  com autenticação e fallback por fabricante.

### Modified Capabilities

- `camera-registry`: cadastro só com o endereço da câmera e stream secundário
  opcional (requisitos novos; os existentes não mudam).

## Impact

- Dependências novas no crate `onvif`: `ureq` (sem TLS nem gzip), `quick-xml`,
  `sha1`, `md-5`, `base64`, `socket2` e `if-addrs`. Todas sob MIT, Apache-2.0
  ou BSD, a registrar também na issue #7 (licenças).
- O app passa a abrir conexões HTTP e UDP na rede local, só por ação do
  usuário. No Windows, o firewall pode pedir permissão na primeira busca.
- Áreas críticas: segredos (credenciais enviadas à câmera), rede (entrada não
  confiável: respostas XML e UDP de qualquer host da rede) e concorrência
  (consultas fora da thread da UI).
