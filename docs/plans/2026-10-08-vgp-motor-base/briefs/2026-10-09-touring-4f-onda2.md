---
type: Brief
title: touring-4f (MiniMax) — onda 2 dos corpora: Rust semeado, negativos naturais selados, JS por arquivo
plan_id: 2026-10-08-vgp-motor-base
timestamp: 2026-10-09T19:55:00-03:00
autor: touring-09 (coordenadora)
tags: ["#kind:plan", "#status:active", "#domain:harness"]
---

# Brief touring-4f — onda 2

Ordem do Gabriel (/goal 09/10): «prossiga com a implementação completa de proposta.md e VL0-multilinguagem-spec.md e
ao final, me prove, na prática, que tudo funciona de forma completa e perfeita». B =
`/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base`.

## Juízo da onda 1 (coordenadora, 09/10 ~22:50Z): ACEITA

Conferido por mim, sem rodar o seu código: sha do `scripts/corpus_semeado.py` = `f2f0f33c6656b686a118` (1.581 linhas).
Contagem por classe em disco igual à do relatório: html 20/20/16/20/20 positivos e 200 negativos; typescript 80+135;
tsx 120+200. `classe_erro` do juiz `89a976164c291d9d` deu 296/296 `simbolo` nos positivos (o relatório §5 diz
250/250; corrija o número no relatório). O teto de `cdn_pin` (16) e o de JS foram aceitos como declarados. O achado
dos `touring-wt-*` (o mesmo repo em 5 diretórios) vale para todo construtor: deduplique por sha de conteúdo, nunca
por caminho.

## O problema que a onda 2 resolve

O juiz exige falso deny com limite superior **Clopper-Pearson 95% ≤ 1%** (`--exigir-limite-superior`). Com 0 falsos
isso pede **n ≥ 299 negativos por linguagem**. Hoje: TS 135 · TSX 200 · HTML 200 · JS 3, e nenhuma linguagem prova o
critério. Os negativos também têm de ser **HELD-OUT**: os autores dos adaptadores (VRS Kimi, VTS/VHTML sonnet)
desenvolvem contra o corpus semeado, e medir o falso deny no mesmo conjunto repete o erro que o held-out 20261010
pegou no Python (0/700 no ajuste, CP 1,74% fora dele; memória `juiz-taxa-pontual-aprova-o-que-o-limite-reprova`).

## Tarefas, por prioridade

**A. Rust semeado (primeira, porque a VRS da Kimi depende dela).** O corpus natural
`~/.claude/touring/vgp/corpus_rust/` tem n < 20 em `atributo` (3) e `kwarg` (1), e `import` está no limite (13).
Faça `--lang rust` no `corpus_semeado.py` com ≥ 20 positivos por classe: `nome` E0425 · `import` E0432/E0433 ·
`atributo` E0609/E0599 · `kwarg` E0560. A mutação é em arquivo REAL de crate pequeno, e o oráculo é o `cargo check`
do crate com `CARGO_TARGET_DIR` dedicado (`/home/gabrielgadea/projects/touring/target/semeado-rust`) e incremental,
por `scripts/hw-slot --class rust`. Escolha crates-folha (`touring-pyright`, `touring-foundation` ou crates de
`~/projects` com poucas dependências) para cada check custar segundos; meça e cite o custo. O código do rustc tem de
estar na `mensagem` (o `classe_erro` exige para Rust). Os negativos de Rust já existem (307 no natural).

**B. Negativos NATURAIS selados, ≥ 300 por linguagem onde a população permitir (TS, TSX, JS, HTML).** Fonte: edições
reais (`Write`/`Edit`/`MultiEdit`) em `.ts/.tsx/.js/.mjs/.html` nos transcripts `~/.claude/projects/*/*.jsonl`.
Rótulo pelo oráculo de DELTA que você já construiu: o conjunto de erros de símbolo do arquivo, antes e depois da
edição, aplicada sobre o projeto atual, é igual. A edição não introduz erro, então é negativo. A edição que
introduz erro vira positivo natural; separe esses e reporte a contagem como bônus. Regras do construtor, iguais às do
held-out Python (`B/diagnostics/instrumentos-2026-10-09/construir_disjunto.py`, só leitura): dedup por sha de
conteúdo; teto de concentração de 3% por arquivo e por sessão; corte em `2026-10-09T23:00:00Z`; nada que já esteja em
`corpus_semeado/`. Saída em `~/.claude/touring/vgp/corpus_holdout_web_20261012/<lang>/`, com `selo.json` (sha_tree no
método `sha256(relpath + sha256(conteúdo))` em ordem) e meta por linguagem: população, descartes por motivo com
denominador, concentração máxima. **Selado**: você não roda o motor nem o juiz nele, e não copia o conteúdo para
fora. Onde a população não chega a 300, reporte o teto com denominador; não complete com sintético.

**C. JS por arquivo.** Com 11 arquivos limpos na base, permita até 3 mutações por arquivo e por classe (símbolos
distintos) e reporte n e a concentração por arquivo. Se mesmo assim n < 20, declare o teto. Não mexa em config de
build nem em artefato de terceiro.

## Aceite (escrito antes)

1. A: ≥ 20 positivos por classe Rust (ou o teto com denominador), todos com o código do rustc na mensagem e
   `classe_erro` = `simbolo`; determinismo IDENTICO; selftest com mutante novo por receita Rust reprovado.
2. B: selo conferido duas vezes (início e fim); sobreposição zero com `corpus_semeado/` nas chaves id, sha de conteúdo
   e caminho@ts, com controle positivo > 0 (o corpus contra ele mesmo); n por linguagem com o CP95 de 0 falsos
   calculado; descartes por motivo.
3. C: n e concentração por classe.
4. `ruff format --check`, `pyright` 0 e `--selftest` com exit 0 depois de tudo; sha novo do script no relatório.

## Onde e como

Arquivos exclusivos: `B/scripts/corpus_semeado.py` (e testes ao lado), `~/.claude/touring/vgp/corpus_semeado/`,
`~/.claude/touring/vgp/corpus_holdout_web_20261012/`, `target/semeado-rust/` e o seu relatório. Trabalho pesado sempre por
`hw-slot` (rc 75 = sem vaga a tempo; tente de novo). A máquina está a 90 °C.

## Proibido

commit/push; `update-touring`/`propagate`; `kill`/`pkill` em processo touring; `git stash`; `rm -rf`;
`~/.claude/settings.json`; editar `crates/**` da árvore principal ou dos worktrees dos outros. A mutação Rust acontece
num worktree SEU: `git -C /home/gabrielgadea/projects/touring worktree add --detach
/home/gabrielgadea/projects/touring-wt-semeado 88c7cc8b` (aditivo). Entre uma mutação e outra, restaure regravando o
conteúdo original que você salvou; `git checkout --`/`restore` estão proibidos. O worktree termina limpo
(`git status --porcelain` vazio) e é o `project_root` das entradas Rust; os juízes `B/scripts/juiz_*.py`, `corpus_v0d.py` e `shell_inline.py` (só leitura); os
corpora `corpus_v0d*`, `corpus_rust` e `corpus_holdout_disjunto*` (só leitura, e o held-out Python nem isso).

## Relatório

Um só: `B/RELATORIO-corpus-onda2-touring-4f.md`, no roteiro de 8 seções, mais uma mensagem curta à touring-09. Se A
terminar antes, mande uma mensagem curta só com A (caminho + contagem), para a VRS começar a usá-lo.

> **Adendo 09/10 ~23:00Z (coordenadora) — B redesenhada pela medição da 4f.** Nos 1.251 transcripts há 19.656
> escritas: 18.293 em `.py` e 106 em arquivo web (html 86 em 10 arquivos, js 20 em 7, ts e tsx 0). Conferi com uma
> contagem independente: py 6.521 (controle positivo) · html 10 · js 7 · ts/tsx 0, com padrão mais estrito. Com o teto
> de 3%, HTML ficaria em 14 e JS em 0. **Decisão:** a fonte principal passa a ser o estrato `git`, com commits reais,
> só leitura, pares (blob antes, blob depois), dedup por repo e por (caminho, blob), corte pela data do commit e teto
> de 3% por arquivo e por commit. O estrato `transcript` entra sem teto, com a concentração reportada. Cada estrato
> sai em subpasta própria, com n e CP95 por estrato e no total; sobreposição zero com o semeado por sha (o blob do
> último commit costuma ser a base dos semeados). Oráculo recomendado: LanguageService persistente do `typescript` do
> projeto, com equivalência ao `tsc` provada em ≥ 20 pares. Continua selado.
