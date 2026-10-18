---
type: Brief
title: A6-HTML — 2º leitor adversarial do adaptador HTML (recurso/ancora) contra a spec WHATWG e a de URL
plan_id: 2026-10-08-vgp-motor-base
timestamp: 2026-10-10T14:30:00Z
autor: touring-09 (coordenadora)
para: touring-a6 (Kimi)
tags: ["#kind:plan", "#status:blocked", "#domain:vgp", "#lang:html"]
---

# A6-HTML

> **PAUSADO em 17/10/2026** (decisão do Gabriel, canvas da sessão efe418c2). A touring-a6 nunca leu este brief: a
> sessão rodava noutro modo de permissão, a mensagem ficou retida à espera de aprovação NAQUELA sessão, e a sessão
> morreu no reboot de 17/10 07:57. **Reabrir só depois do INT-ADAPT** (DAG `A6HTML`, que bloqueia o `JHTML`), numa sessão
> Kimi aberta no MESMO modo de permissão do coordenador, conferindo a entrega pelo estado da sessão e por uma 1ª resposta.
> **Antes de reabrir, atualizar**: o binário congelado (`ec542c4b` → o do adapt integrado) e o código (`touring-wt-vhtml`
> → o worktree do adapt). O escopo continua o do §8 do `RELATORIO-VHTML.md`.

B = `/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base`.

## O que existe

O adaptador HTML do motor VGP (`vhtml`) verifica duas classes:
- `recurso`: `src`/`href` local inexistente;
- `ancora`: `#id`, `for=`, `aria-labelledby/describedby` e `getElementById`/`querySelector('#x')` literal sem o id no
  documento, mais id duplicado.

Binário congelado: `~/.claude/touring/vgp/binarios/ec542c4b/touring`. Uso:
`touring vgp check --path <arquivo.html>` (JSON com `findings`, cada um com `classe`, `nome`, `linha`, `severidade`,
`alvo`). Código (SÓ LEITURA): `/home/gabrielgadea/projects/touring-wt-vhtml/crates/touring-code/src/vgp/lang/html/`
(`recurso.rs`, `ancora.rs`, `script.rs`, `documento.rs`, `texto.rs`). Relatório do autor: `B/RELATORIO-VHTML.md`;
o §8 lista o que ELE não verificou.

## A tarefa: achar onde o adaptador erra, com prova

Para cada caso abaixo, leia a regra na spec (cite seção e trecho), escreva uma fixture HTML mínima no SEU diretório
`B/diagnostics/a6-html/fixtures/` (um arquivo por caso, com os arquivos irmãos que o caso pedir), rode o binário e
registre o observado.

- **Âncora (WHATWG HTML, «scroll to the fragment» / «find a potential indicated element»)**:
  - `href="#top"` sem `id=top` (alvo especial; ASCII case-insensitive: `#TOP`);
  - `href="#"` (fragmento vazio);
  - `<a name="x">` como alvo de `#x`;
  - fragmento com percent-encoding (`#caf%C3%A9` → `id="café"`);
  - text fragment `#:~:text=…`;
  - id dentro de `<template>`;
  - id em SVG inline (`<svg><g id="x">`) e `url(#x)`;
  - `for=` apontando para um id existente que não é rotulável;
  - `aria-labelledby` com LISTA de ids separados por espaço, um deles ausente.
- **Recurso (spec de URL, «URL parsing»; HTML `<base>`)**:
  - `<base href="sub/">` mudando a resolução;
  - caminho com `%20` e com espaço literal;
  - `?query` e `#frag` no `src`;
  - `href` para diretório (`docs/` com `docs/index.html`);
  - `srcset` com descritores `1x, 2x` e com vírgula dentro da URL;
  - `<link rel="preconnect|dns-prefetch">`, que não é recurso;
  - `<a href="arquivo.pdf" download>`;
  - `href="mailto:"`, `tel:`, `javascript:`, `data:`;
  - `//cdn.x/y.js` (protocolo-relativo);
  - `url()` em `style=` inline e em `<style>`;
  - `<object data>`, `<embed src>`, `<video poster>`, `<source src>`, `<track src>`;
  - caixa diferente (`Img.PNG` × `img.png`): o disco do Linux é case-sensitive, e diga o que o adaptador faz.
- **Livre**: qualquer outro caso que a spec ou a sua leitura do código sugiram. Cada um com a mesma evidência.

## Saída

`B/diagnostics/a6-html/RELATORIO.md`, com uma tabela, uma linha por caso:

| caso | regra (seção da spec + trecho) | esperado | observado (`classe`/`severidade`/`linha`, ou «sem achado», ou `unknown` com motivo) | veredito |
|---|---|---|---|---|

O veredito é um de quatro: **ok**, **falso positivo**, **falso negativo** ou **abstenção correta**. Depois da tabela:
1. os defeitos ordenados por gravidade, sendo falso `deny` > falso `warn` > falso negativo;
2. para cada defeito, o trecho do código responsável (arquivo:linha) e a correção sugerida em uma frase. Não edite o
   código: quem conserta é o `vhtml`, por ordem minha.

Abra o JSON cru de cada execução. Nada de «passou» sem o JSON, porque um rc 0 sem achado pode ser mudez (lição
`rc-zero-sem-sinal-e-a-assinatura`). Guarde os JSONs em `B/diagnostics/a6-html/saidas/`. **Controle positivo
obrigatório**: uma fixture com `<img src="nao-existe.png">` num diretório de projeto TEM de dar `recurso` `warn`. Sem
isso, nenhum «sem achado» vale.

Atenção ao caminho das fixtures: em `/tmp` a política muda (o `vhtml` está trocando `deny` por `warn` ali). Use o seu
diretório em `B/diagnostics/a6-html/fixtures/`, que é «projeto».

## Proibido

Editar qualquer arquivo fora de `B/diagnostics/a6-html/`; ler `~/.claude/touring/vgp/corpus_holdout_*` ou
`~/.claude/touring/vgp/oss/`; commit; `kill`/`pkill`; `rm -rf`. Nada de cargo, nada de build: só o binário congelado.

Ao terminar, mande uma mensagem à touring-09 com a contagem por veredito e os 3 defeitos mais graves.
