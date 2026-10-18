---
type: Brief
title: touring-35 (Kimi) — P0 do held-out: coletor de atributos de módulo (bloco composto e ligação dinâmica)
plan_id: 2026-10-08-vgp-motor-base
timestamp: 2026-10-09T19:30:00-03:00
autor: touring-09 (coordenadora)
tags: ["#kind:plan", "#status:active", "#domain:harness"]
---

# Brief touring-35 — P0 do held-out (antes da VL0)

B = `/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base`. Insumo: `B/diagnostics/holdout/RELATORIO.md`.

## Veredito P1 da coordenadora (09/10)

O held-out disjunto (semente 20261010, sobreposição 0) deu, no `e935aaf1`, falso deny **5/600 em arquivo (limite
superior 95% 1,74%)** e **2/600 inline (1,05%)**: «≤ 1%» não está provado e a V2/V3 **não fecham**. Três dos sete são
defeito real e geral do coletor de atributos de módulo — reproduzidos pela coordenadora no binário congelado:

| sonda (`vgp check`, arquivo em /tmp, `--project-root ~/projects/analise`) | hoje | causa |
|---|---|---|
| `import numpy as np` · `np.array(...)` · `np.where(x)` | **deny** `array`, `where` | numpy liga em bloco composto no `__init__` |
| `from PIL import Image` · `Image.LANCZOS` | **deny** `LANCZOS` | Pillow liga por `setattr(module, …)` dinâmico |
| `import scripts.aco.rust_bridge as rb` · `rb.RUST_AVAILABLE` | **deny** `RUST_AVAILABLE` | nome ligado dentro de `try/except` |
| controle: `np.arrai` | deny | (tem de continuar deny) |
| controle: `os.path.join` | allow | (tem de continuar allow) |
| observação: `os.pathh` | allow | não investigado — explique a causa (desenho ou lacuna) |

O release 30.4.96 sai em **sombra** sem este conserto (os hooks só negam em `Ativo`); ele entra no 30.4.97, antes da V4.

## O conserto

1. O coletor de ligações de módulo percorre as instruções compostas do escopo de módulo (`if/elif/else`, `try/except/
   else/finally`, `with`, corpo de `for`/`while`) e une as ligações de todos os ramos.
2. Módulo EXTERNO com sinal de ligação dinâmica (`setattr` sobre o módulo, `globals()[…] =`, `__getattr__` de módulo,
   `from x import *` não resolvido) → atributo desconhecido vira **abstenção** (warn/unknown), nunca deny.
3. Mesma severidade em arquivo e inline para o mesmo alvo externo incerto (hoje o numpy sai warn em arquivo e deny inline).

## Onde e como

- **Worktree próprio**: `git worktree add -b fix/2026-10-09-vgp-atributo-dinamico /home/gabrielgadea/projects/touring-wt-vgp-attr 88c7cc8b`
  (aditivo; sem commit). A árvore principal segue CONGELADA para o 30.4.96. A VL0 espera este P0; depois você
  rebaseia o worktree da VL0 sobre ele.
- Build: `CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/vgp-test` (check/test/clippy) e `…/target/vgp-judge`
  (release do juiz); cargo pesado só por `scripts/hw-slot --class rust -- …`. Atenção: o `target/debug` padrão foi
  limpo às 19:20 (higiene de disco); os seus targets dedicados estão intactos.

## Aceite (escrito antes)

1. As 3 sondas viram teste de regressão (RED no código atual → GREEN), nos dois baldes (arquivo e /tmp/inline); os 2
   controles seguem iguais (`np.arrai` deny, `os.path.join` allow).
2. Mutantes mortos e listados: remover a travessia de compostos (numpy volta a deny) · remover o sinal dinâmico
   (Pillow volta a deny) · severidade divergente arquivo×inline.
3. Juiz v2 e V3 no corpus V0d VIVO, binário congelado com sha: **sem regressão por id** contra
   `B/diagnostics/juiz-e935/k_v2_vivo.json` e `k_v3_vivo.json` (mesmos positivos detectados; falso deny 0).
4. **Proibido ler** `~/.claude/touring/vgp/corpus_holdout_disjunto*` e `B/diagnostics/holdout/` além deste brief: um
   held-out NOVO (semente 20261011, disjunto também do 20261010) está sendo construído e só a coordenadora o abre,
   depois que você entregar o binário congelado. É ele que decide «≤ 1%» pelo limite superior.
5. `cargo fmt --check`, `clippy -D warnings`, suítes de `touring-code` e `touring-server` com 0 falhas.

Proibições do brief da VL0 valem integralmente (sem commit/push/deploy/kill/stash/`rm -rf`; juízes e corpora são da
coordenação). Relatório único: `B/RELATORIO-P0-atributo-touring-35.md` (8 seções) + mensagem curta à touring-09 com o
caminho do binário congelado e o sha.
