# RETOMADA — V-8 SEM2 (touring-65)

> O relatório do item é `docs/plans/2026-10-08-vgp-motor-base/RELATORIO-V8-SEM2.md`
> (main, commit `85e1142f` + `01989816`). O **V-4** (o item anterior) foi ACEITO
> pelo touring-00 — o branch `int/2026-10-31-intadapt` (14 commits) está pronto
> para o merge dele.

## O que está feito

1. **O selo EXTERNO do held-out web** (`scripts/selar_holdout_web.py`, novo):
   `~/.claude/touring/vgp/corpus_holdout_web_20261012.selo.json` — **fora** do
   corpus, no formato do `confere_selo.py` (`sha_tree`/`arquivos`/`bytes`), com
   os denominadores e a concentração **medida**. Conferido: **INTACTO** (rc=0,
   `sha_tree 42106741066f40f9`). Os denominadores por classe estão em
   `corpus_holdout_web_20261012.denominadores.json`.
2. **As classes do estrato `transcript` como NÃO PROVADA** (html n=83, javascript
   n=21; `n_para_cp95_1pct`=299) — com o motivo escrito, sem inflar o número.
   O alvo de 300 é **inalcançável** nessa população (medido: a população
   inteira do estrato é 83+21/23).
3. **A concentração OSS medida por camada**: `oss/tsx` 213 entradas (máx. 7 por
   chave = 3,3 %, igual ao teto declarado) ✓; `oss/typescript` 214 (máx. 7) ✓;
   a camada **podada** (`_podado_20261009-215331`) é a que viola (5,6 % e 6,3 %
   contra o teto de 3 %) — violação já medida e declarada pelo selo da própria
   camada.

## O que está em curso (os processos)

`/tmp/v8_oss_rota.sh` (rodando em background):

1. espera o lock dos clones (o `--dry-run` do `rotular_oss.py` que estava em
   curso);
2. **re-rotula a camada OSS com o teto de 3 % NA SELEÇÃO**:
   `rotular_oss.py --alvo 300 --teto-concentracao 0.03 --saida /tmp/oss_rotulado_20261031`
   (os 7 clones **já existem** em `~/.claude/touring/vgp/oss/` — excalidraw, zod,
   date-fns, html5-boilerplate, express, lodash, dom-examples — e o adjudicador
   congelado `oss/oraculo_ls.js` ✓; nenhuma clonagem foi necessária);
3. sela a camada nova (selo **externo**, no formato do validador atual) e o
   confere.

Logs: `/tmp/v8_oss_rota.log` (o encadeamento), `/tmp/oss_rotulado_20261031.log`
(o rotulador), `/tmp/oss_dryrun.log` (o dry-run anterior).

**O que o resultado tem de responder**: a camada re-rotulada com o teto
respeitado atinge 299 negativos por linguagem (tsx/typescript/javascript) para
sair do "não provada"? E a camada nova pode virar o **estrato OSS de `html`**?
(hmm: o `rotular_oss.py` hoje rotula `javascript`/`tsx`/`typescript` — o
**html** precisaria de um adjudicador que o script não tem hoje ✗ — é o
item em aberto para a coordenadora.)

## O critério (da coordenadora)

Classe que atingir **299 negativos com o teto respeitado** sai do "não provada";
a que não atingir segue declarada como tal, **com o n e o motivo**. Corte
temporal e disjunção contra os corpora anteriores, como sempre (o rotulador
faz isso ✓).

## Regras (do briefing, em vigor)

- Nada mergeado no branch principal; nada sobrescrito nem apagado (o corpus de
  disco e o `oss/selo.json` estão como estavam; o selo novo mora fora).
- hw-slot sempre; commit só dos meus paths.
- Clones de repositórios públicos: na rede, **fora** do workspace
  (`~/.claude/touring/vgp/oss/` ✓ — já existiam ✓).
