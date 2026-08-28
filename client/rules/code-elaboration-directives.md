# Diretrizes de Elaboração de Código — E/A/M (constitutional, auto-load)

> **Auto-load** | **Version**: v1.0 | **Date**: 2026-08-28 | **Authority**: Gabriel Gadea
> **Corpo canônico + fontes**: `~/projects/touring/docs/code-mode.md` §"Diretrizes de
> elaboração de código" e `~/projects/touring/docs/plans/2026-08-27-code-mode-aderencia-sandbox/strategy-2026-08-27-code-mode-aderencia-sandbox.md`
> **Enforcement**: gate `BestPracticesGate` (`crates/touring-quality/src/builtins/best_practices.rs`)
> verifica a DECLARAÇÃO (este arquivo ou o manual) e a MEDIÇÃO M1 (journal de runs).

Como TACO ESCOLHE a rota de execução, ACERTA o programa e MEDE a aderência ao code
mode. Fontes: Anthropic CodeAct/programmatic tool calling, Cloudflare Code Mode,
dsh (DeepSeek harness), TanStack AI — todas verificadas por leitura direta.

## E — ESCOLHER a rota (E1-E6)

| # | Diretriz |
|---|---|
| E1 | UMA ferramenta de execução + as demais como **API tipada (stub) no prompt** — nunca o catálogo inteiro |
| E2 | Regra de custo declarada: **loop/condicional/agregação/≥3 fatos → programa**; operação simples → tool direta (forçar programa no caso simples taxa o caso comum) |
| E3 | O colapso mora no **EXECUTOR** e o deny **nomeia a rota de volta** — omitir schema sem enforcement não enforça nada (D8) |
| E4 | A instrução do transport nomeia **TODOS os args obrigatórios** — sinal ausente vira erro opaco |
| E5 | Catálogo grande → **progressive disclosure** (descoberta por filesystem/busca, lazy loading) |
| E6 | Code é objetivamente mais barato: 150k→2k tokens (~98,7%); loops 11-15× — usar o número, não a fé |

## A — ACERTAR o programa (A1-A14)

| # | Diretriz |
|---|---|
| A1 | **SDK plana**: funções top-level autocontidas — nunca fluent/OO profundo (alucina parâmetro e this-binding) |
| A2 | **Um objeto de config nomeado**, nunca N posicionais do mesmo tipo primitivo |
| A3 | Stub tipado com **doc densa** (restrições, enums estritos, `@throws`) — o tipo é restrição de raciocínio |
| A4 | Bindings retornam **JSON canônico TIPADO**, nunca prosa para raspar |
| A5 | **Erros estruturados, taxonomia ortogonal** (exception/timeout/abort/proc-exit/invalid-output/output-limit); a mensagem ENSINA a correção |
| A6 | **output-limit explícito**, nunca truncamento silencioso — o modelo escolhe um resultado menor |
| A7 | **Concorrência declarada**: read-only independentes sobrepõem (`touring.parallel`, pool 10); mutantes correm sozinhas |
| A8 | **Exemplo canônico completo** no prompt (fan-out + reduce + return único) |
| A9 | **Prompt byte-estável** (ordem lexicográfica, OnceLock) — KV-cache do provider; retry append-only |
| A10 | **Retry com autocorreção**: lint → execução → diagnóstico com linha/coluna → re-inferência |
| A11 | **Trust paritário ao bash** (containment ≠ boundary); **segredos NUNCA entram no sandbox** (env_clear + whitelist) |
| A12 | **Fail-LOUD** em linguagem desconhecida — SDK errado por fallback silencioso é o pior mundo |
| A13 | **Scripts bons viram ativos**: escada medida (provisional ≥10 exec @≥90% → trusted ≥100 @≥95%) via `--harvest` |
| A14 | **Cap de 3 tentativas** no loop de auto-depuração — na 4ª muda de ESTRATÉGIA (outra rota, outra decomposição), jamais repete o mesmo corpo |

## M — MEDIR a aderência (M1-M3)

| # | Diretriz |
|---|---|
| M1 | Régua dedicada: `touring kpi -j` → `code_mode_adherence` (`success_rate`, `wasted_attempts_retry_pairs`, `by_failure_kind`, `by_language` — do `run_journal.jsonl`); piso 0.8 |
| M2 | Aderência é **modelo × apresentação** — abaixo do piso de capacidade do modelo, nenhum prompt salva |
| M3 | Contenção **determinística no substrato** (kernel: Landlock/seccomp/rlimit) — "não evitável por código malicioso"; nunca só no texto |

## Cross-references

| Tópico | Local |
|---|---|
| Manual do code mode (canônico) | `~/projects/touring/docs/code-mode.md` |
| Strategy com fontes e racional | `~/projects/touring/docs/plans/2026-08-27-code-mode-aderencia-sandbox/` |
| Gate de qualidade (enforcement) | `~/projects/touring/crates/touring-quality/src/builtins/best_practices.rs` |
| 4 Pilares (code mode é o guarda-chuva) | `~/.claude/rules/touring-4-pillars.md` |
| STR / padrões de combinação | `~/.claude/rules/tool-combination-patterns.md` |

---

_v1.0 — 2026-08-28 | Materializa as diretrizes E/A/M (roadmap code-mode-aderencia-sandbox) como rule constitucional auto-load._
