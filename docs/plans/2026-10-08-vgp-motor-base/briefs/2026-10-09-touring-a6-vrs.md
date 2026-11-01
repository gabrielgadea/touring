---
type: Brief
title: touring-a6 (Kimi) — VRS camada 1, o adaptador Rust do motor VGP
plan_id: 2026-10-08-vgp-motor-base
timestamp: 2026-10-09T19:30:00-03:00
autor: touring-09 (coordenadora)
tags: ["#kind:plan", "#status:active", "#domain:harness"]
---

# Brief touring-a6 — VRS (Rust, camada 1)

Ordem do Gabriel (/goal 09/10): «prossiga com a implementação completa de proposta.md e VL0-multilinguagem-spec.md e
ao final, me prove, na prática, que tudo funciona de forma completa e perfeita» · «consulte o context7 sempre que for
preciso solucionar um problema complexo ou para compreender as melhores práticas». B =
`/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base`.

## Leia antes de qualquer comando

`B/HANDOFF-touring-2026-10-09.md` (§1 regras, §7 armadilhas) · `B/proposta.md` §5 (o motor) e §10 (VRS) ·
`B/VL0-multilinguagem-spec.md` (o contrato: §3 trait, §4 classes, §5 política, §7 corpus/juiz) ·
`B/RELATORIO-V1V2V5-2026-10-09.md` §2 (como o adaptador Python foi construído) · o código `crates/touring-code/src/vgp/`.

## Quem está com o quê (não toque)

P0 do Python → subagente `p0-atributo` (`~/projects/touring-wt-vgp-attr`) · núcleo da VL0 (despacho, `lang_do_caminho`,
delta/política no `check`, hooks) → subagente `vl0-nucleo` (`~/projects/touring-wt-vl0`) · camada 2 genérica (pool LSP
pyright/rust-analyzer/tsserver/deno, inclusive a sonda de custo do rust-analyzer) → subagente `vl0-camada2-generica`
(`~/projects/touring-wt-l2`). A árvore principal está congelada para o release 30.4.96.

## A tarefa

Adaptador Rust da **camada 1** (nativo, puro: sem daemon, sem rede, sem processo), contra o contrato da VL0 §3:

- `use crate::…` / `super::…` / `self::…` / `mod x;` (`x.rs` e `x/mod.rs`) resolvem para o que o módulo do PROJETO
  define, inclusive `pub use` (a declaração vence a inferência — lição registrada no Touring sobre reexportes) →
  classe `import` (E0432) e `nome` (E0425) quando o alvo é do projeto e não existe.
- Campo de literal de struct do projeto (`S { campo_inventado: … }`) → classe `kwarg` (E0560).
- Macro, `cfg` desconhecido, glob `use x::*` de crate externo, caminho de crate externo sem modelo, `impl` gerado por
  derive → **abstenção** (`unknown`/warn), nunca deny. Receptor de método de tipo não inferível → abstenção.
- Política §5: símbolo do PROJETO ainda inexistente em arquivo do projeto = **aviso** (o RED do TDD); deny só onde
  nenhuma edição seguinte cria o alvo. Meça antes de decidir; cite o número.

**Integração sem esperar a VL0**: escreva o adaptador como `vgp/lang/rust.rs` com uma função livre
`pub fn verificar_rust(input: &CheckInput) -> (String, Vec<Finding>)` e os testes no mesmo diretório. O `impl Adaptador`
de 3 linhas e o registro entram quando o `vl0-nucleo` entregar (a integração é minha). Para o juiz rodar ponta a ponta
no seu binário, ligue um despacho TEMPORÁRIO `lang == "rust"` no `check` do SEU worktree, marcado
`// TEMPORARIO-VRS: substituido pelo registro da VL0`, e liste-o no relatório.

## Onde e como

- Worktree: `git -C /home/gabrielgadea/projects/touring worktree add -b feat/2026-10-09-vgp-vrs /home/gabrielgadea/projects/touring-wt-vrs 88c7cc8b`
  (aditivo; sem commit).
- `CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/wt-vrs`; todo cargo por
  `/home/gabrielgadea/projects/touring/scripts/hw-slot --class rust -- …` (a máquina é compartilhada e quente — o slot
  espera a temperatura cair; rc 75 = sem vaga a tempo, tente de novo).
- Context7 para a referência de caminhos/`use`/visibilidade da linguagem (Rust Reference, edição 2021) e para o
  índice de erros do rustc; cite o que consultou.

## Aceite (escrito antes)

1. Testes por classe (RED→GREEN): E0432, E0425 e E0560 com alvo do projeto inexistente; controles que precisam
   passar limpos: o mesmo símbolo existindo; reexportado por `pub use`; atrás de macro; de crate externo.
2. Mutantes mortos e listados: ignorar `pub use` · tratar macro como inexistente · resolver `super::` como `crate::`.
3. Juiz `B/scripts/juiz_motor_vgp.py` no corpus natural `~/.claude/touring/vgp/corpus_rust/` (37 positivos · 307
   negativos), binário congelado em `~/.claude/touring/vgp/binarios/<sha8>/touring` com sha: recall de símbolo por
   classe (≥ 0,70 onde n ≥ 20; n < 20 reportado com denominador, nunca escondido) e falso deny com o **limite
   superior Clopper-Pearson 95%** ≤ 1% (a flag `--exigir-limite-superior` está sendo acrescentada ao juiz pelo
   subagente `instrumentos-p1`; se ainda não existir, calcule o limite e reporte). p95 ≤ 150 ms.
4. `cargo fmt --check`, `clippy -D warnings`, suítes de `touring-code` e `touring-server` com 0 falhas no worktree.

## Proibido

commit/push; `update-touring`/`propagate`; `kill`/`pkill` em processo touring (REGRA #19); `git stash`; `rm -rf`;
`~/.claude/settings.json`; editar fora do seu worktree (exceto o relatório); os juízes `B/scripts/juiz_*.py` e os
corpora (só leitura); `~/.claude/touring/vgp/corpus_holdout_disjunto*`.

## Relatório (um só, ao final ou num bloqueio)

`B/RELATORIO-VRS-touring-a6.md`, roteiro de 8 seções (identidade · feito com evidência · estado por item · processos ·
dados com sha · decisões/riscos · git · o que NÃO foi verificado) + mensagem curta à touring-09 com o caminho.
