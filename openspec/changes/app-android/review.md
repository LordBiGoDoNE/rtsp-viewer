# Revisão: app-android

Issue: [#21](https://github.com/LordBiGoDoNE/rtsp-viewer/issues/21). Épica: `epic/app-android` (`bf2cba2`), criada da `main` `cac44f0`.
Implementação autorizada em 2026-11-25 (grupos 1–6). Teste local pelo usuário: sim, no celular.

## Cadeia de PRs

| Grupo | PR | Base | Head |
| --- | --- | --- | --- |
| 1. Fundação Android | #22 | `epic/app-android` | `task/app-android/fundacao` |
| 2. Núcleo e persistência | #23 | `task/app-android/fundacao` | `task/app-android/nucleo` |
| 3. ONVIF em Kotlin | #24 | `task/app-android/nucleo` | `task/app-android/onvif` |
| 4. Player Media3 | #25 | `task/app-android/onvif` | `task/app-android/player` |
| 5. Interface Compose | #26 | `task/app-android/player` | `task/app-android/interface` |
| 6. Fechamento | #27 | `task/app-android/interface` | `task/app-android/fechamento` |

## Revisões por grupo

Revisor: `group-reviewer` (Sonnet). Correções: `finding-fixer` (Opus). Uma rodada extra por grupo quando a correção tocou área crítica ou passou de ~100 linhas.

| Grupo | Achados | Triagem |
| --- | --- | --- |
| 1 | M1: HTTP sem TLS liberado para qualquer host no manifesto; na rodada extra, N1: a checagem `checkCleartextHttp` cortava a linha no `//` de `"http://"` | M1 corrigido em `58cc180` (guarda de rede local no adaptador ONVIF, tarefa 3.5, ADR 0006 e checagem `checkCleartextHttp`); N1 corrigido em `c26910e`, com testes de regressão; rodada extra feita (CI) |
| 2 | M1: `ProviderException` do Keystore escapava do cofre | corrigido em `66d4a03`, com 3 testes e dois baixos no mesmo commit; rodada extra (segredos) sem achados |
| 3 | A1 (alto): faltava o teste de que um redirecionamento HTTP não é seguido; A2 (médio): resolvedor irrestrito por padrão | corrigidos em `5fa1b71` (301/302/303/307/308 nos três modos de autenticação; resolvedor obrigatório); rodada extra sem achados |
| 4 | M1: câmera só MJPEG reconectava sem fim; M2: `release()` bloqueante, sem teste de parar em até 1 s com a câmera inacessível | corrigidos em `8914636`; rodada extra (concorrência, mais de 100 linhas) sem achados |
| 5 | nenhum bloqueador, alto ou médio | baixos na #28 |
| 6 | não houve revisão própria; os achados do grupo vieram da ponta a ponta (seção abaixo) | — |

## Teste de ponta a ponta (agente, celular do usuário, build `e2e`)

Galaxy A56 (Android 17), com a câmera falsa e o simulador ONVIF no PC. Passaram pela interface real:

- abertura sem rede;
- cadastro, erros de campo, edição e remoção;
- H.264 e H.265 com "Informações";
- tela cheia por duplo toque;
- senha errada sem reconexão;
- queda e volta da câmera, segundo plano, tela ligada;
- cadastro ONVIF só com o endereço, fabricante, "Já cadastrada" e "Sobre".

Testes instrumentados no celular: 21/21 no head `469bb14`.

Achados corrigidos pelo `finding-fixer`:

- H.265 no decoder Exynos (`d522bbd`): `csd-0` único; streams só com CRA terminam em "Formato de vídeo não suportado" após 15 s.
- Teste de UI (`ad00372`): defeito do teste, que agora rola até "Salvar".
- Permissões além das quatro da spec (médio, `bab37df`): removidas, com a nova checagem `checkReleasePermissions`.
- Senha exposta à acessibilidade (baixo, `469bb14`): a acessibilidade vê só pontos, com 2 testes.

Não verificado pelo agente: a busca não encontrou o simulador rodando no Windows, porque o serviço de descoberta do sistema ocupa a UDP 3702. Com uma câmera ONVIF real da rede, a busca funcionou.

## Revisão final

- **Modelo:** `spec-reviewer` (Claude Opus 5.5, esforço medium), contexto novo, somente leitura.
- **Base/head:**
  - cadeia `bf2cba2..469bb14` (110 arquivos, cerca de 12,3 mil linhas);
  - épica contra `main` `cac44f0..bf2cba2` (só os artefatos da spec).
- **Cobertura:** spec (proposta, design, tarefas e specs delta), ADR 0006, `design/telas.md` seção 6 e os trechos equivalentes do núcleo do desktop.
- **Limitação:** sem execução de testes.
- **Achados:** nenhum bloqueador nem alto. Triagem:
  - **M1 (médio):** a busca aceitava redes VPN que herdam o transporte Wi-Fi e varria a sub-rede remota. Corrigido em `4b3edf4` (grupo 3) com `isLocalSearchNetwork` e testes.
    - Limite: o filtro foi testado no JVM, sem uma VPN ativa no aparelho.
  - **M2 (médio):** com a chave do Keystore presente mas inutilizável, a senha digitada não era gravada e a câmera não abria. Corrigido em `afe4e1f` (grupo 2): a chave é trocada uma vez no `seal`, com 4 testes.
  - **Baixo (grupo 2):** o teste de arquitetura do `core` se contornava com `"rtsp://"` na linha ou com import curinga. Corrigido em `37fa247`.
  - **Demais baixos:** issue #28.
- **Propagação:** as correções subiram a cadeia por merge até o topo `684f15d`.
- **Reverificação:** pela triagem, achados médios não exigem nova revisão. Nenhuma foi feita.

## Medições (Galaxy A56, build `e2e`)

- Partida a frio com 20 câmeras: mediana de 168 ms (161 a 218). Meta: até 1000 ms.
- APK de release sem assinatura: 1,80 MB. Meta: até 15 MB.
- Primeira imagem: H.264 0,83 s, H.265 0,71 s. Meta: até 2 s.
- Latência com a câmera falsa: 0,39 a 0,91 s. Meta: até 1 s.
- Busca: 2,7 a 3,1 s. Meta: até 4 s.
- Decodificação por hardware (Exynos).

## CI local (`act`) e remoto

- **`act`:** jobs `android` e `openspec` verdes em cada grupo antes do PR e no topo `684f15d`, depois das correções da revisão final.
  - Testes JVM no topo: `core` 80, `onvif` 79, `app` 104 e `build-logic` 23.
  - `android-device` e `desktop-windows` não rodam no `act` (sem emulador nem runner Windows) e ficam cobertos pelo CI remoto.
- **CI remoto:** verde em todos os jobs (`android`, `android-device`, `desktop-linux`, `desktop-windows`, `openspec`) nos heads finais de #22 a #27.
  - Falhas transitórias do `android-device` por HTTP 429 (Docker Hub e Maven) foram resolvidas com nova execução, sem mudança de código.

## Teste do usuário (2026-11-26)

- **Build:** `e2e` do head `684f15d`, instalado no celular do usuário.
- **Roteiro:** `test-camera/roteiro-android.md`, inclusive a câmera real (seção 5).
- **Resultado:** o usuário testou tudo, sem defeitos.

## Head revisado

- Head: `684f15d` (tree `9a331d1`).
- Depois dele entra só este `review.md`.
