# Telas do rtsp-viewer

Descrição independente de plataforma, válida para o desktop (Slint) e o Android
(Compose). Cores, fontes e espaçamentos vêm de `design/tokens.json` (ADR 0004).
Toda mudança de tela altera este documento e as duas UIs.

Tema escuro único nesta versão. Textos em pt-BR, exatamente como abaixo.

## 1. Lista de câmeras (tela inicial)

**Barra superior** (`size.toolbar-height`, fundo `surface`): título "Câmeras" à
esquerda; à direita, botão primário "Adicionar câmera" e botão discreto "Sobre".

**Conteúdo:** uma linha por câmera (`size.list-item-height`), na ordem de
cadastro:

- nome (`font.size-body`, negrito, `text`);
- endereço sem credenciais (`font.size-small`, `text-muted`);
- à direita, ações "Editar" e "Remover" (discretas).

Clique (ou Enter) na linha abre a câmera (tela 3). A linha sob o cursor usa
`surface-hover`.

**Estado vazio:** centralizado, "Nenhuma câmera cadastrada." e, abaixo, o botão
"Adicionar câmera".

**Avisos** (faixa no topo do conteúdo, cor `warning`, com "Fechar"):

- arquivo corrompido: "A lista de câmeras não pôde ser lida e foi guardada em
  <caminho>. Começando com a lista vazia.";
- erro ao salvar ou no cofre: a mensagem do erro.

**Confirmação de remoção** (diálogo, `size.dialog-width`): "Remover
<nome>?" / "A câmera e a senha salva serão apagadas." / botões "Cancelar" e
"Remover" (`danger`).

## 2. Formulário de câmera (adicionar / editar)

Título "Nova câmera" ou "Editar câmera". Campos (largura `size.form-width`):

| Campo | Rótulo | Observação |
| --- | --- | --- |
| Nome | "Nome" | obrigatório, até 60 caracteres |
| Endereço | "Endereço RTSP" | exemplo no campo vazio: `rtsp://192.168.0.10:554/stream1` |
| Usuário | "Usuário (opcional)" | |
| Senha | "Senha (opcional)" | oculta; ao editar, vazio mantém a senha atual e o texto de apoio diz "Deixe vazio para manter a senha atual" |

Se a URL tiver `usuário:senha@`, as credenciais vão para os campos próprios ao
salvar (a URL fica limpa).

Erros aparecem abaixo do campo, em `danger`, com o texto vindo da validação
(por exemplo "Informe um nome.", "O endereço deve começar com rtsp:// ou
rtsps://.").

**Aviso sem cofre** (quando o sistema não tem cofre de senhas): "Este sistema
não tem cofre de senhas: a senha será lembrada só até fechar o app."

Botões: "Cancelar" e "Salvar" (primário). Enter salva; Esc cancela.

## 3. Visualização

Fundo `video-background`. O vídeo ocupa a área toda, mantendo a proporção.

**Barra superior** (escondida em tela cheia): "Voltar", nome da câmera, e à
direita "Informações" e "Tela cheia".

**Estados** (mensagem centralizada sobre o vídeo, fundo `overlay`):

| Estado | Texto |
| --- | --- |
| Conectando | "Conectando…" |
| Reproduzindo | (sem mensagem) |
| Reconectando | "Reconectando (tentativa N)…" |
| Erro | mensagem do erro, por exemplo "Usuário ou senha inválidos." ou "Formato de vídeo não suportado: mjpeg", e o botão "Tentar de novo" |

**Informações** (painel no canto, alterna com o botão): codec, resolução,
decodificação ("hardware (d3d11va)" ou "software") e endereço sem credenciais.

**Senha pedida** (câmera com usuário e sem senha disponível): diálogo "Senha
de <usuário>", campo oculto, botões "Cancelar" e "Conectar".

**Atalhos:** F11 ou duplo clique alternam tela cheia; Esc sai da tela cheia e,
fora dela, volta à lista.

## 4. Sobre

Nome "rtsp-viewer", versão, "Client leve para câmeras RTSP." e o componente de
atribuição do Slint (obrigatório, ADR 0003). Botão "Voltar".
