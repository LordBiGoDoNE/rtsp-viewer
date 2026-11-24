# ADR 0003: Licença do Slint

- Estado: aceito
- Data: 2026-11-24
- Aprovação: usuário, 2026-11-24

## Contexto

O Slint 1.19 é distribuído sob três licenças à escolha: GPLv3, Royalty-free 2.0
e licença comercial (Software 3.0). O repositório é privado e o app pode ser
distribuído sem abrir o código.

## Decisão

Usar a licença **Slint Royalty-free 2.0**. Ela permite apps desktop e mobile,
inclusive de código fechado, sem custo, desde que haja atribuição. A
atribuição é feita na tela "Sobre", com o componente `AboutSlint`.

## Consequências

- A tela "Sobre" com `AboutSlint` é obrigatória e não pode ser removida.
- A licença Royalty-free não cobre sistemas embarcados; se o app for para esse
  tipo de dispositivo, é preciso rever a licença.
- Mudar para GPLv3 ou para a licença comercial exige decisão do usuário e novo
  ADR.
