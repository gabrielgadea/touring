---
type: AuditReport
title: "Cross-audit 29/08/2026 — wave paralelização-agentes (30.4.22 + 30.4.23, M0–M5)"
description: "Auditoria de fidelidade a propósito sobre TUDO implementado na wave: fix ANN recall, aliases SDK, KPIs, tier journal, critic-panel, diamante ground, advisory M3, doutrina M4/M5. Toda prova executada."
tags: [cross-audit, paralelizacao-agentes, code-mode, adw, kpi]
timestamp: 2026-08-29T15:20:00-03:00
plan: /docs/plans/2026-08-29-paralelizacao-agentes/
---

# Cross-audit 29/08/2026 — wave paralelização-agentes (M0–M5 + fix ANN)

## VERDICT

**PASS com 2 findings corrigidos no próprio audit.** Todas as entregas M0–M5 e o
fix ANN 30.4.22 cumprem o propósito documentado, provado por execução (nunca por
leitura). O finding mais sério foi de **processo**: a prova do diamante M2 citada
no fechamento da wave não existia em disco — foi refeita ao vivo neste audit
(run `strategy-loop-1788014691`, artefato verificável). O segundo foi lacuna de
cobertura nos 2 KPIs novos, fechada com refactor de testabilidade + 2 testes
(39/39 verdes, clippy 0).

## Escopo auditado

Commits `0c6ce59` (fix ANN 30.4.22) + `2e7d7a5` (M0–M5 30.4.23) + artefatos
fora do repo (adw.py, adw-library — versionados via espelho `client/`, CLEAN
323/323 por `sync-client-skills.py --check`):

- Rust: `orchestrate_allowlist.rs` · `gate_metrics{,_snapshot}.rs` · `kpi.rs` ·
  `cli_suggester{,_tests}.rs` · `run.rs` (preludes) · `daemon.rs` ·
  `ann_memory/mod.rs`
- Specs ADW: `cross-audit.toml` + `strategy-loop.toml` (spec vivo ≡ library,
  `diff` = idênticos)
- Contratos: `docs/kpi/commitments.yaml` · regra M3 na decision matrix ·
  doutrina M4 · quadro M5

## FASE 2+6 — Propósito vs comportamento, tudo executado

| Entrega | Propósito | Prova executada | Veredito |
|---|---|---|---|
| M0 aliases (py) | nomes tipados valem como hook em `query`/`parallel` | `touring run --orchestrate`: `query("memory_recall")` ok; `parallel` 5/5 em **0,62s** | PASS |
| M0 aliases (js) | idem no SDK JS | `--lang js`: query + parallel 2/2 ok | PASS |
| M0 erro que ensina (A5) | alias inválido → erro com a allowlist | py e js: `"hook 'x' is not in the orchestrate read-only allowlist; available: cli-ast-blast, …"` | PASS |
| M0 allowlist fecha mutação | hook mutante nunca passa | `query("memory_store")` → **negado** nos 2 SDKs | PASS |
| M0 gating por origin | alias só resolve para origin sandbox | `daemon.rs:1119` `is_sandbox_origin(...)` guarda o resolve; testes cross-guard | PASS |
| M0 `:par` no origin | fan-out observável no journal | 16 origins `…:code:N:par` no `run_subcalls.jsonl` (frescos deste audit) | PASS |
| M0 KPI parallel_runs | distinct runs com `:par`; STUB sem sinal | vivo: `actual=3.0` advisory PASS (`.checks[29]`); predicado lido; teste novo | PASS |
| M0 KPI tiered_share | share de agents com tier DECLARADO | vivo: `actual=1.0` (`.checks[30]`); ver F2 abaixo — régua honesta | PASS |
| M0 tier no journal | `adw.py` journaliza tier/skill por agent | `_agent_journal_fields`: chave sempre, **valor null se não declarado** (E4 correto); 4 agents pós-M0, todos `tier=mid` | PASS |
| M1 critic-panel | juiz cego composto no cross-audit | lint **0 erros**; journal `cross-audit-1788011993`: `verdict_gate pass` → `panel.panel parallel_started` → 3 críticos (`correctness`/`security`/`reproducibility`, `tier=mid`) → verdicts `pass` | PASS |
| M2 diamante ground | `recall`‖`diagnose` por construção | **run fresco `strategy-loop-1788014691`** (ver F1): `ground type=parallel` → ramos partem no MESMO ms (…692.047) → `parallel_joined` → run pass. Serial 33,5s → paralelo **25,3s** | PASS |
| M3 delegação | advisory no 10º arquivo distinto, 1×/sessão | hook VIVO, sessão sintética: silêncio 1º–9º, **releitura não conta**, advisory exatamente no 10º, silêncio no 11º, counter `m3_delegation_advised_count` **0→1** | PASS |
| M4 doutrina + M5 quadro | presentes e consumíveis | `audit-plan-completion.sh` **9/9, exit 0** (fresco) | PASS |
| fix ANN 30.4.22 | recall sem dispatch wgpu por par | `EmbeddingIndex::new()` → `with_simd()` (doc carrega a medição p50 7,86s); fan-out 5 recalls **5/5 em 0,62s** | PASS |

Suites (exit real, sem pipe): foundation aliases 2/2 `EXIT=0` · server 14 testes
"alias" (incl. os 2 cross-guards `every_typed_method_is_a_faithful_alias…` e
`both_sdks_resolve_aliases_and_stamp_parallel_origin`) `EXIT=0` · kpi **39/39**
`EXIT=0` · suggester m3 2/2 · `clippy -p touring-cli -D warnings` `EXIT=0`.

## Findings

### F1 — prova do M2 não estava em disco (processo · MÉDIA · REMEDIADO)

Nenhum journal em `.touring/adw-runs/` continha eventos `parallel_*` do nó
`ground` — a prova "0ms skew, 0,44s overlap" citada no fechamento da wave não
apontava artefato. Arqueologia: o único run pós-commit (09:34) era ANTERIOR ao
mtime do spec vivo (10:41), então rodou serial legitimamente — mas o fechamento
citou uma prova cujo artefato não é localizável. **Remédio executado**: run
fresco `strategy-loop-1788014691` com o spec atual — diamante executa por
construção, evidência agora em disco. **Lição institucional**: prova viva em
fechamento DEVE citar o `run_id`/artefato; narrativa sem ponteiro é o modo de
falha que a Lei L3 existe para eliminar — desta vez do lado do relator.

### F2 — suspeita de régua tautológica (INVESTIGADO → NÃO-FINDING)

Hipótese: se o produtor sempre grava tier, `tiered_agent_share` nunca cai < 1.0
("adoção mede canal"). **Refutada por leitura dos dois lados**: `adw.py:2486`
grava `"tier": node.raw.get("tier")` — **null quando o spec não declara** — e o
predicado (`kpi.rs:751`) conta null no denominador e não no numerador. Um spec
sem `tier =` declarado derruba o share. A chave separa as eras (pré-M0 fora do
denominador), o valor mede a declaração. Régua correta; agora guardada por teste.

### F3 — 2 KPIs novos sem teste semântico (cobertura · BAIXA · CORRIGIDO)

Os 37 testes do kpi.rs cobriam só a existência braço↔contrato (guard
bidirecional), não a semântica dos predicados novos; e `code_mode_parallel_runs`
resolvia `$HOME` internamente (intestável sem mutar env global). **Fix (REGRA
#0)**: extraído `code_mode_parallel_runs_from(path)` + wrapper; 2 testes novos —
`parallel_runs_counts_distinct_stamped_runs_only` (distintos, sem-stamp não
conta, journal ausente = STUB nunca zero) e
`tiered_share_null_lowers_and_missing_key_is_excluded` (null abaixa o share;
chave ausente = STUB; non-agent nunca conta). 39/39 verdes, clippy 0.

### Instrumentos que mentiram neste audit (todos pegos antes de acusar o sistema)

1. Parser do `kpi -j` assumiu lista com `name` — o formato real é `checks[]` com
   `id`; re-medido. 2. `tail -3` de `cargo test` capturou só o ÚLTIMO binário
   (doc-tests "0 passed") — os testes reais rodaram noutro alvo; re-medido com
   log completo. 3. `EXIT` de bloco com pipes reportava o `tail`
   (`${PIPESTATUS}` — memória `exit-code-engolido-pelo-pipe`); re-medido sem pipe.

## FASE 3+4 — DEBT + HARMONY

- **Débito real: zero** nos 12 artefatos tocados (únicos hits = fixtures de
  teste e comentários que mencionam os padrões; 10/12 sem nenhum hit).
- **Órfãos: zero** — os 7 símbolos novos têm consumidor real fora do definidor
  (grep confirmatório, Cadeia 7): `resolve_hook`→daemon, `SDK_HOOK_ALIASES`→
  run.rs, `record_m3_delegation_advised`→suggester, KPI fns→braços do match,
  `_agent_journal_fields`→3 sítios de `node_started`.
- **50-dim (piso Gold 0.80)**: orchestrate_allowlist **0.961** · gate_metrics
  0.875 · snapshot 0.879 · kpi **0.936** · suggester 0.862 · run.rs **0.944** ·
  daemon 0.896 · ann_memory **0.915** — todos ≥ Gold, 4 ≥ Platinum.
- **6 P0 BLOCK (F2.1/F2.4/F2.5/F2.6/F4.3/F4.5)**: `P0_OK(6/6)` nos 8 arquivos.
- Contenção provada de graça: o sandbox Landlock **negou leitura de
  `~/.claude`** de dentro de um probe (FS-scope do workspace funcionando).

## Residuais (fora do escopo desta wave, já registrados)

Deps numéricas da DAG (cosmético) · docs de teste no índice tantivy global
(decisão humana) · débito release-TEST touring-server · rotação GEMINI_API_KEY
(só Gabriel) · `wiring_diagnostic` warning pré-existente (polyglot off).

## Ações fechadas neste audit

1. Run fresco do diamante (F1) — evidência em disco. 2. Refactor testabilidade +
2 testes semânticos dos KPIs (F3). 3. Este report como artefato do flow guard.

— *Toda linha "PASS" acima tem comando executado e saída neste audit; nada foi
aceito por leitura de código apenas, e nenhum claim de fechamento anterior foi
reutilizado sem re-execução.*
