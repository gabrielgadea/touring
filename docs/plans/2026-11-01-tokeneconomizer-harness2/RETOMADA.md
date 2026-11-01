---
type: Handoff
title: "RETOMADA — execução do backlog TokenEconomizer × harness2 + exploração do elephant-goldfish-model"
description: "Estado em 01/11/2026 ~17h30: B8, B7, B9 e B10 feitos; o elephant-goldfish-model em extração para aperfeiçoar o ADW (campos da Spec e além); B5, B3, B2, B1, B6, B4 por fazer."
timestamp: 2026-11-01T17:30:00-03:00
---

# RETOMADA (01/11/2026)

Pedido do Gabriel: "prossiga com A ordem proposta é B8 → B7 → B9 → B5 → B3 → B2 → B1 → B10 → B6 → B4" e,
no meio, "aproveite e explore https://github.com/google/elephant-goldfish-model para aperfeiçoar ao máximo todo
modus operandis do adw, inclusive expandir, complementar e detalhar mais os campos da Spec, mas não se limite a
isso, vá muito mais além".

## Feito (commits na branch; os scripts vivos ficam em `~/.claude/skills/Touring/scripts/`, espelho em `client/`)

| Item | Commit | Prova |
|---|---|---|
| B8 retomada do ADW por identidade de conteúdo (`node_fingerprint`, `spec_digest`, `replayable`) | `a6958da5` | test_adw 250/250; ruff 39→0 |
| B7 explore não lê o próprio ledger (`_is_own_ledger`) | `1c9fe965` | test_explore 25/25 |
| B9 headless `mcp = none|inherit|<json>` + `session_persistence` | `b1fd113e` | test_adw 254/254; lint 23/23 specs |
| B10 `run_effect` helped/hurt/confounded + fixture que impede a suíte de depositar no aprendizado real | `6a1437e4` | test_adw 259/259 |
| B5+B3 `context_cost` e `surface_usage` no `touring kpi` (vão ao daemon no release de 04/11) | `8827db32` | touring-cli 768/768; clippy limpo; 36 sessões reais, hooks 7,2% / anúncio 2,1% / tools 9,4% da entrada |
| B11 onda EGM E1-E8 (goldfish, must_read por desafio de linha, findings, churn, triage, tools=none, purpose de design, output_schema) | `cabdd732` | test_adw 361/361; 23 specs válidas |

## Restrições vivas

AB34 armado até 08/11 00:10: nada de hook/settings/plugin/daemon/`update-touring`/`cargo build --release` no
`target/` principal. Release só 04/11 00:00. O experimento de tokens do B9 fica para depois de 08/11 (o braço
de controle gravaria transcrito no corpus do AB34). B6 (`ab34_arm.py`) só depois de 08/11.

## Em curso

- elephant-goldfish-model clonado em `~/.claude/touring/ext/elephant-goldfish-model@403d8881d620` (Apache-2.0).
  Extração do método pronta em scratchpad (`egm_metodo.md`); a de distribuição/testes em andamento.
  Achados para o ADW: elefante (contexto profundo, redige) × goldfish (revisor sem histórico, lê só alvo +
  referências); revisor que não leu não revisou; sintetizador devolve só contagem + link; falha de revisor =
  `not run`; convergência = 2 rodadas limpas, teto 5, churn; triagem CONTEST/ESCALATE/PROPOSE FIX/DECLINE;
  contradição não edita arquivo; schema de retorno de worker {path, summary, interface, invariants, side_effects}.
- Pendências da onda EGM: o minerador do portfólio (`miner.rs`) não indexa goals/verification; efeito de `omitClaudeMd` e `--tools ""` exige run vivo (depois de 08/11).
- Próximo: B2 e B1 só na fonte e desligados até 08/11; B4 fase 0 (amostra para rótulo humano); B6, B12, B13 depois de 08/11.
