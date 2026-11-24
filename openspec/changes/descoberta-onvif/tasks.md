# Tasks

Issue: [#12](https://github.com/LordBiGoDoNE/rtsp-viewer/issues/12).
Épica: `epic/descoberta-onvif`. Spec formalizada em 2026-11-24.
Implementação autorizada em 2026-11-24 (grupos 1–5). Teste local pelo usuário:
sim. Merges dependem da aprovação do pacote final.

Os grupos formam uma cadeia (cada um parte da branch do anterior). Em cada
grupo: testes do projeto, CI local via `act` e revisão por grupo
(`group-reviewer`) antes de abrir o PR. Áreas críticas: segredos e entrada de
rede não confiável (grupos 1 e 2), concorrência (grupo 4).

## 1. Cliente ONVIF (`task/descoberta-onvif/cliente`, base: épica)

- [ ] 1.1 Crate `desktop/crates/onvif` com versões fixas (design D1), sem TLS
  nem gzip; ADR 0005 (ONVIF: bibliotecas, sem TLS, autenticação, limites).
- [ ] 1.2 `soap.rs`: envelopes SOAP 1.2, parse por nome local com
  `quick-xml`, limite de 256 KiB e erros SOAP (`Fault`, `NotAuthorized`).
- [ ] 1.3 `auth.rs`: PasswordDigest com nonce aleatório, compensação de relógio
  (`GetSystemDateAndTime`) e HTTP Digest (MD5, `qop=auth`) na repetição após
  401.
- [ ] 1.4 `client.rs`: `GetDeviceInformation`, `GetCapabilities`,
  `GetProfiles` e `GetStreamUri`; escolha de principal e secundário; troca de
  host `0.0.0.0`/`localhost`; sondagem de portas 80/2020/8000/8080/8899 em
  paralelo (D3).
- [ ] 1.5 `examples/fake_onvif.rs`: simulador ONVIF com perfis configuráveis,
  relógio deslocado e modo HTTP Digest; testes de integração em processo,
  inclusive senha errada e que a senha nunca trafega em texto puro.
- [ ] 1.6 Corrigir a referência desatualizada ao ADR da stack no `AGENTS.md`
  (com `rules-conflict-checker`).

## 2. Busca na rede (`task/descoberta-onvif/busca`, base: grupo 1)

- [ ] 2.1 `discovery.rs`: Probe WS-Discovery por interface IPv4 ativa
  (`if-addrs` + `socket2`), TTL 1, escuta de 3 s, destino injetável (D4).
- [ ] 2.2 Validação das respostas: `RelatesTo`, limite de 64 KiB, origem
  privada ou link-local, deduplicação por `EndpointReference`, `XAddrs` http e
  nome sugerido pelos scopes.
- [ ] 2.3 Simulador responde ao probe (multicast real e unicast em loopback);
  testes com respostas válidas, duplicadas, de outra busca e malformadas.

## 3. Núcleo e cadastro (`task/descoberta-onvif/nucleo`, base: grupo 2)

- [ ] 3.1 `core`: portas `CameraDiscovery` e `StreamResolver`, `DiscoveredCamera`
  e erros tipados (sem ONVIF, autenticação, sem perfis, rede).
- [ ] 3.2 `core`: fabricantes e caminhos (D5) e endereço da câmera como objeto
  de valor (host ou `host:porta`, sem esquema), com testes.
- [ ] 3.3 `core`: `Camera` com stream secundário e caso de uso de cadastro por
  endereço (ONVIF primeiro, fabricante como fallback explícito), com portas
  falsas nos testes.
- [ ] 3.4 `app/infrastructure`: `substream_url` opcional no `cameras.json`,
  compatível com o arquivo atual; adaptadores das portas usando o crate
  `onvif`.
- [ ] 3.5 Teste de arquitetura: o `core` não depende de `onvif` nem das crates
  de rede.

## 4. Interface (`task/descoberta-onvif/interface`, base: grupo 3)

- [ ] 4.1 `design/telas.md`: tela "Procurar câmeras", estados da consulta e
  escolha de fabricante; tokens novos, se houver.
- [ ] 4.2 Botão "Procurar câmeras" e tela de resultados (procurando,
  encontradas, "Já cadastrada", nenhuma encontrada com dicas, procurar de
  novo).
- [ ] 4.3 Formulário: endereço só com host, "Consultando a câmera…" com
  cancelar, erro de autenticação e lista de fabricantes quando o ONVIF falhar;
  nome sugerido pelo dispositivo.
- [ ] 4.4 Busca e consulta fora da thread da UI, com sessão (resultado antigo
  ignorado); testes de UI com o backend de testes do Slint.

## 5. Fechamento (`task/descoberta-onvif/fechamento`, base: grupo 4)

- [ ] 5.1 Medir o tempo de busca e de consulta (simulador e câmera falsa) e o
  efeito no tamanho do pacote e no boot (meta de boot mantida); registrar no
  README.
- [ ] 5.2 Roteiro de teste com o simulador e com a câmera real do usuário (Tapo:
  "Conta da câmera" e ONVIF na porta 2020).
- [ ] 5.3 Teste de ponta a ponta do sistema local com a cadeia completa
  (`teste-local.md`).
