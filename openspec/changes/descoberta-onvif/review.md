# Revisão: descoberta-onvif

Issue: [#12](https://github.com/LordBiGoDoNE/rtsp-viewer/issues/12). Épica: `epic/descoberta-onvif` (`ce3804e`), criada da `main` `a1c573e`.
Implementação autorizada em 2026-11-24 (grupos 1–5). Teste local pelo usuário: sim.

## Cadeia de PRs

| Grupo | PR | Base | Head |
| --- | --- | --- | --- |
| 1. Cliente ONVIF e simulador | #13 | `epic/descoberta-onvif` | `task/descoberta-onvif/cliente` |
| 2. Busca (WS-Discovery) | #14 | `task/descoberta-onvif/cliente` | `task/descoberta-onvif/busca` |
| 3. Núcleo, fabricantes e cadastro | #15 | `task/descoberta-onvif/busca` | `task/descoberta-onvif/nucleo` |
| 4. Interface | #16 | `task/descoberta-onvif/nucleo` | `task/descoberta-onvif/interface` |
| 5. Fechamento | #18 | `task/descoberta-onvif/interface` | `task/descoberta-onvif/fechamento` |

## Revisões por grupo

Revisor: `group-reviewer` (Sonnet). Correções: `finding-fixer` (Opus). Uma rodada extra por grupo quando a correção tocou área crítica ou passou de ~100 linhas.

| Grupo | Achados | Triagem |
| --- | --- | --- |
| 1 (`ce3804e..27f0cb1`) | M1: o `XAddr` de mídia era seguido para qualquer host, que recebia os digests das credenciais | corrigido em `13ad742` (chamadas autenticadas sempre no host da câmera; prazo total de 10 s por consulta); rodada extra (autenticação) sem achados |
| 2 (`13ad742..634897d`) | A1 (alto): host do `XAddr` da busca não validado (DNS, IP público, `user@host`); A2 (médio): fila e lista sem limite diante de flood | corrigidos em `88df9d0`; rodada extra (destino das credenciais) sem achados; resíduo documentado no ADR 0005 |
| 3 (`88df9d0..444cb0a`) | M1: ao editar a URL principal, o secundário antigo era mantido | corrigido em `761cdeb`, com 9 testes; rodada extra (invariantes) sem achados |
| 4 (`761cdeb..1b40d00`, merge `33e9c19`) | nenhum bloqueador, alto ou médio; 3 baixos | baixos na #17 |
| 5 (`33e9c19..e2f5694`) | nenhum | — |

## Revisão final

- Modelo: `spec-reviewer` (Claude Opus 5.5, esforço medium), contexto novo, somente leitura.
- Base/head: cadeia `ce3804e..e2f5694` (tree `ee9e77a`) e épica contra `main` `a1c573e..ce3804e` (só os artefatos da spec).
- Cobertura: diff completo da cadeia (41 arquivos, ~6,2 mil linhas), com foco em destino das credenciais, entrada de rede não confiável, concorrência da busca e da consulta, compatibilidade do `cameras.json`, boot e arquitetura. Limitações declaradas: sem execução de testes; simulador, testes de busca e `design/telas.md` lidos por amostragem; Slint verificado por busca no diff.
- Achados e triagem:
  - **M1 (médio):** a URL RTSP de `GetStreamUri` era aceita com qualquer host; ao abrir a câmera, a senha ia embutida para esse host. Corrigido no grupo 1 (`48b8e61`): a URL usa sempre o host da câmera, mantendo esquema, porta e caminho; teste de integração com o simulador (IP público, DNS, IPv6) que falhava antes. ADR 0005 e design D3 atualizados. Propagado por merge (`8111b18`, `e6e928b`, `9130a20`, `b88ae7d`; conflito trivial no ADR, dois itens mantidos).
  - **B1 a B4 (baixos) e sugestões:** issue #17.
- Reverificação do delta `e2f5694..b88ae7d` (`spec-reviewer`): M1 resolvido; o topo contém só o delta; nenhum achado.

## Teste de ponta a ponta (agente, Windows, pacote .zip)

UI Automation sobre o app extraído do `.zip`, com configuração isolada (`RTSP_VIEWER_CONFIG_DIR`), câmera falsa e simulador `fake_onvif`:

- regressão da mvp: 21 verificações, todas OK (mensagem de endereço inválido atualizada);
- ONVIF: 10 verificações, todas OK — busca por multicast real encontra o simulador; "Adicionar" com credenciais cadastra principal e secundário sem digitar URL e sem senha no `cameras.json`; vídeo abre; "Já cadastrada"; senha errada não salva; endereço sem ONVIF leva à escolha de fabricante e monta a URL; cancelar a consulta volta ao formulário sem salvar.

## Medições (Windows)

- Busca: 3,0 a 3,1 s (escuta de 3 s); depois da varredura unicast, 3,06 a 3,11 s na LAN real, com a Tapo C320WS encontrada em 4 de 4 rodadas.
- Consulta só com o IP, sondando portas: 0,12 a 0,15 s com o simulador.
- Boot com 20 câmeras, mediana de 10 execuções: 360 ms (325 a 697 ms). Meta: < 500 ms. Nada de rede no boot (teste `boot_nao_procura_cameras`).
- Pacote Windows: 59,4 MB (+0,3 MB do ONVIF).

## CI local (`act`) e remoto

- `act`, jobs `desktop-linux` e `openspec`: verdes em cada grupo antes do PR, no topo depois da revisão final (`b88ae7d`) e no topo depois do teste do usuário (`3778505`).
- `desktop-windows` não roda no `act` (runner Windows): coberto pela execução nativa no Windows (`fmt`, `clippy -D warnings`, `cargo test --workspace` com a câmera falsa) e pelo CI remoto.
- CI remoto: verde em todos os jobs de #13 a #16 nos heads anteriores ao teste do usuário (#13 também em `48b8e61`). Nos heads depois do teste do usuário (#14 `0de6483`, #15 `c5e0f70`, #16 `50793c6`, #18 `3778505`) os jobs **não foram iniciados** pelo GitHub (pagamento ou limite de gastos da conta); sem resultado de código. Pendente até a cobrança ser regularizada.

## Teste do usuário (2026-11-24)

- Com o simulador e a câmera falsa, a seção 9 do roteiro passou sem defeitos.
- **Achado:** "Procurar câmeras" não encontrava a câmera real do usuário (TP-Link Tapo C320WS, ONVIF na porta 2020). Diagnóstico na LAN: a câmera responde ao Probe enviado por unicast a ela, mas nunca ao Probe multicast (3 rodadas, de porta efêmera e da 3702); o firewall do Windows estava desligado.
- **Decisão do usuário:** além do multicast, enviar o Probe por unicast a cada host da sub-rede privada de cada interface. O requisito não muda; mudam o design D4, a proposta e o ADR 0005.
- **Correção** no grupo 2 (`0de6483`, `finding-fixer`): varredura só em endereço RFC 1918, /24 do endereço quando a rede é maior, até 254 destinos por interface e 1024 no total, uma thread de envio por interface, buffer de envio de 256 KiB (no Windows o envio travava ~2,6 s após ~100 destinos sem ARP); mesma validação das respostas. Testes: 6 de unidade e 2 de integração (um respondedor só unicast, que falhava antes). Propagado por merge (`c5e0f70`, `50793c6`, `0cb97e9`).
- **Achado transversal** (apontado pelo `finding-fixer`): o IP real da câmera e um UUID parcialmente real estavam versionados como valores de exemplo. Trocados por valores fictícios no topo (`3778505`, só substituições). Os valores continuam no histórico dos commits anteriores (repositório privado; sem usuário nem senha); o histórico não foi reescrito.
- Reverificação do delta `b88ae7d..3778505` (`spec-reviewer`, Claude Opus 5.5, esforço medium): as duas correções resolvidas, o delta contém só elas; **B5 (baixo)**, envio unicast sem prazo de escrita, na #17.
- Ponta a ponta no pacote novo: ONVIF 10/10.
- **Reteste do usuário** com a Tapo no pacote novo: tudo certo.

## Backlog vinculado

- #6: build mínimo do FFmpeg.
- #7 (`review-finding`): avisos de licença das crates Rust nos pacotes; incluir as crates novas (`ureq`, `ureq-proto`, `http`, `httparse`, `utf8-zero`, `quick-xml`, `sha1`, `md-5`, `socket2`, `if-addrs`, `getrandom`).
- #17 (`review-finding`): baixos das revisões por grupo, da revisão final e da reverificação do teste do usuário (B5).

## Head revisado e tree SHA

- Head revisado: `3778505f39eab821f6b113276ad77ef79433f471` (`task/descoberta-onvif/fechamento`).
- Tree: `fd714d6072c727f4f7b54a648ac1793bf37b38e2`.
- Entre o head revisado e o estado final da épica, o único arquivo alterado deve ser este `review.md`.
