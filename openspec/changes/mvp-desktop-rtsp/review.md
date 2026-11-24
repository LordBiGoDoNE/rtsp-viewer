# Revisão: mvp-desktop-rtsp

Issue: [#1](https://github.com/LordBiGoDoNE/rtsp-viewer/issues/1). Épica: `epic/mvp-desktop-rtsp` (`89d3949`), criada da `main` `ccd5ce3`.
Implementação autorizada em 2026-11-24 (grupos 1–5). Teste local pelo usuário: sim (feito em 2026-11-24).

## Cadeia de PRs

| Grupo | PR | Base | Head |
| --- | --- | --- | --- |
| 1. Fundação | #2 | `epic/mvp-desktop-rtsp` | `task/mvp-desktop-rtsp/fundacao` |
| 2. Núcleo e cadastro | #3 | `task/mvp-desktop-rtsp/fundacao` | `task/mvp-desktop-rtsp/nucleo` |
| 3. Player FFmpeg | #4 | `task/mvp-desktop-rtsp/nucleo` | `task/mvp-desktop-rtsp/player` |
| 4. Interface | #5 | `task/mvp-desktop-rtsp/player` | `task/mvp-desktop-rtsp/interface` |
| 5. Fechamento | #8 | `task/mvp-desktop-rtsp/interface` | `task/mvp-desktop-rtsp/fechamento` |

## Revisões por grupo

Revisor: `group-reviewer` (Sonnet). Correções: `finding-fixer` (Opus). Uma rodada extra por grupo quando a correção tocou área crítica ou passou de ~100 linhas.

| Grupo | Achados | Triagem |
| --- | --- | --- |
| 1 (`89d3949..9646ec8`) | M: autobuild diário do BtbN fixado (some em 14 dias); B: registro de entregas casado por substring | corrigidos em `dda7efd`; rodada extra (CI) sem achados |
| 2 (`dda7efd..a79ecfd`) | M: `add` perdia o `CameraId` se só o cofre falhasse; M: senha antiga usada sem usuário (`rtsp://:senha@`) | corrigidos em `06d99e3`; rodada extra (segredos) sem achados |
| 3 (`06d99e3..467c7f9`) | M: fallback para software só em `hw::attach` | corrigido em `e298012`; a rodada extra (concorrência) achou 2 M (prazo de 8 s desde o primeiro pacote; testes de fallback sem efeito sem GPU), corrigidos em `816b543` sem nova rodada |
| 4 (`816b543..12809d4`) | M: faltavam testes do sink e de tela cheia, cofre e retry; M: cores nomeadas passavam na checagem; B: texto da tarefa 4.4 | corrigidos em `3c2f85f`; rodada extra (concorrência, mais de 100 linhas) sem achados |
| 5 (`3c2f85f..95155b4`) | M: checagem do pacote só no exe; M: avisos de licença das crates Rust nos pacotes | 1º corrigido em `b47e539` (a rodada extra achou 1 M, código de saída do `llvm-objdump`, corrigido em `605fb98`); 2º devolvido por exigir decisão de licença → issue #7 (`review-finding`) |

## Revisão final

- Modelo: `spec-reviewer` (Claude Opus 5.5, esforço medium), contexto novo, somente leitura.
- Base/head: cadeia `89d3949..605fb98` (tree `f50296b`) e épica contra `main` `ccd5ce3..89d3949`.
- Cobertura: diff completo da cadeia; áreas críticas (segredos, concorrência, FFI, CI/distribuição, domínio). Limitações declaradas: sem execução de testes, sem conferir as medições.
- Achados e triagem:
  - **A1 (alto):** senha com `/`, `?` ou `#` sem codificação na URL ia em texto puro para o `cameras.json`. Corrigido no grupo 2 (`2f3506e`): `UrlError::UnencodedCredentials` e redação ampliada. Propagado por merge.
  - **M1 (médio):** medidas literais nos `.slint`. Corrigido no grupo 4 (`9b2eea4`), com tokens novos e teste que acusa medida literal.
  - **M2 (médio):** dispositivo de hardware criado a cada boot sem justificativa. Corrigido no grupo 4 (`d6f5d4f`): a preparação acontece ao apontar para uma câmera. Decisão e medições no design.md (D3): +45 MB de private bytes, sem ganho na mediana da primeira imagem.
  - **B1 a B6 (baixos):** issue #9.
- Reverificação do delta `605fb98..cb6dde3` (`spec-reviewer`): A1, M1 e M2 resolvidos; 2 baixos novos (B7, `@` legítimo no caminho recusado; B8, exceção de borda não documentada), acrescentados à #9.

## Teste de ponta a ponta (agente, Windows, pacote .zip final)

UI Automation sobre o app extraído do `.zip`, com `PATH` mínimo (sem FFmpeg nem runtime do Visual C++ instalados), configuração isolada e câmera falsa. Foram 21 verificações, todas OK:

- lista vazia no primeiro boot;
- erros por campo;
- URL com senha sem codificação recusada e não gravada;
- cadastro;
- `cameras.json` sem senha e URL salva sem credenciais;
- tela cheia sem barra, Esc em dois passos;
- câmera com senha vinda da URL abre direto;
- pedido de senha;
- senha errada com "Tentar de novo";
- queda com "Reconectando" e retorno do vídeo;
- confirmação de remoção, cancelar e remover;
- "Sobre" com a atribuição do Slint.

## Medições (Windows)

- Boot a quente, 20 câmeras, mediana de 10 execuções: 320 ms (301 a 380 ms). Meta: < 500 ms.
- Memória ociosa: 78 MB de working set e 91 MB de private bytes.
- Latência com a câmera falsa: cerca de 0,1 s. Meta: até 1 s.
- Primeira imagem: H.264, 25 aberturas, mediana de 0,86 a 1,05 s. H.265 chega a 2,2 s, incluindo até 1 s de espera pelo quadro-chave. O requisito foi reescrito pelo usuário para não contar essa espera.
- Pacote Windows: 59,1 MB compactado. Meta: 30 MB → issue #6 (D4). Pacote Linux: `.deb` de 6,4 MB (na imagem do `act`).

## CI local (`act`) e remoto

- `act`, jobs `desktop-linux` e `openspec`: verdes em cada grupo antes do PR e no topo da cadeia (`cb6dde3`), inclusive os testes do player contra a câmera falsa e o `.deb`.
- `desktop-windows` não roda no `act` (runner Windows). Foi coberto pela execução nativa no Windows a cada grupo e pelo CI remoto.
- A imagem do `act` é mais nova que o Ubuntu 24.04, então o `Depends` exato do `.deb` vale para o runner real.
- CI remoto: todos os checks verdes nos PRs #2 (`dda7efd`), #3 (`2f3506e`), #4 (`310bbf8`), #5 (`a1b5e47`) e #8 (`cb6dde3`).

## Teste do usuário (2026-11-24)

- O usuário seguiu o roteiro (`test-camera/roteiro.md`) com o pacote final, a câmera falsa e uma câmera própria (vídeo exibido). Tudo funcionou como esperado, sem defeitos.
- **Pedido de melhoria:** cadastrar só com o IP e detectar câmeras automaticamente. Não é defeito desta spec: a URL RTSP depende do fabricante, e a solução é o ONVIF. Decisão do usuário: a spec `descoberta-onvif` passa a ser a próxima, antes de `app-android`. A ordem em `openspec/config.yaml` e no README é atualizada no PR de arquivamento.
- **Decisão de requisito:** o cenário "Abrir câmera disponível" (live-view) passa a medir os 2 s sem contar a espera pelo quadro-chave da câmera (`e31a06a`).
- Reverificação do delta `cb6dde3..e31a06a` pelo `spec-reviewer` (rodada do teste do usuário): ver abaixo.

### Reverificação da rodada do teste do usuário

- `spec-reviewer` (Claude Opus 5.5, esforço medium) sobre `cb6dde3..e31a06a` (1 commit, 3 arquivos de documentação, sem código): o delta reflete exatamente a decisão do usuário. O limite de 2 s em rede local continua, só que sem a espera pelo quadro-chave. É coerente com o design e com o restante da spec. Nenhum bloqueador, alto ou médio.
- **B9 (baixo):** a linha de medições do README não separa, no pior caso do H.265 (2,2 s), a parcela de espera pelo quadro-chave. A medição não comprova explicitamente o critério novo nesse pior caso. Registrado na #9.
- `openspec validate mvp-desktop-rtsp --strict` no head `e31a06a`: válido.

## Backlog vinculado

- #6: build mínimo do FFmpeg (pacote Windows de 59 MB contra a meta de 30 MB).
- #7 (`review-finding`, médio): avisos de licença das crates Rust nos pacotes. Resolver antes de qualquer distribuição.
- #9 (`review-finding`, baixos B1 a B8).

## Head revisado e tree SHA

- Head revisado: `e31a06a2d2c51978d1b1e2f4c2d02275e6e4b9ac` (`task/mvp-desktop-rtsp/fechamento`).
- Tree: `d3166f091222df7c4cb2106aefc22fb6b74c4968`.
- Entre o head revisado e o estado final da épica, o único arquivo alterado deve ser este `review.md`.
