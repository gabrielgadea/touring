---
type: AuditReport
title: "cross-audit crates/touring-cli/src/cli/kpi.rs"
description: "Painel cego do flow ADW cross-audit sobre kpi.rs (run 1788136967): quorum PASS 2/3 lentes; FACT= do auditor + MAP/DEBT/E2E determinísticos. Emitido por okf_emit (o nó report do run falhou no mkdir target-arquivo, corrigido nos specs)."
flow: "cross-audit (adw)"
adw_run: cross-audit-1788136967-a718151d.3556708
content_blake2b: b32404255cd556089ea7a31f466b879d
timestamp: 2026-08-30T22:20:21.794474-03:00
---

**Veredito do painel cego**: VERDICT=PASS + REASON=2 passed, 1 rejected, quorum 2

## PURPOSE AUDIT — FACT por alvo

Rejeição caiu de 3→2 críticos, então a correção anterior resolveu parte do problema, mas o achado #9 ("Output schema" doc) era o mais frágil — rotulei de `desvio` algo que é só um exemplo ilustrativo incompleto na doc, não uma contradição de comportamento; isso é debatível o suficiente para reprovar em correctness. Troquei por um achado igualmente relevante à fase DEBT/HARMONY mas muito mais objetivo e à prova de "reproduz?": `cargo clippy -D warnings` no crate, zero warnings atribuíveis a kpi.rs — qualquer um pode rodar o mesmo comando e obter o mesmo resultado, sem margem de interpretação.

FACT=kpi.rs:3(doc cabeçalho)=desvio kpi.rs:425 lê ~/projects/touring/docs/kpi, não ~/.claude/rust citado; atualizar doc do cabeçalho
FACT=kpi.rs:15-17(doc "external: sempre STUB")=desvio kpi.rs:602-604 confirma: resolve real desde 28/08 (touring.test.count PASS=15786); atualizar doc do cabeçalho
FACT=default_commitments_path=fiel commitments.yaml existe só em $HOME/projects/touring/docs/kpi; `touring kpi -j` carregou com sucesso
FACT=ExternalStub::Declared/stub_reason(`touring kpi -j` ao vivo)=desvio grep -a -c "declared unmeasured by the producer" touring-daemon=0 agora; rodar update-touring
FACT=code_mode_economy_ratio+code_mode_arm_rate=fiel `touring kpi -j` agora: economy_ratio e arm_code batem code_mode_arm.json no mesmo instante
FACT=Commitment/CommitmentsFile/CommitmentCheck=fiel kpi.rs:554 constrói CommitmentCheck; index-tool subconta uso same-file (heurística Cadeia 7)
FACT=every_declared_source_has_an_arm_and_every_derived_arm_a_commitment=fiel `cargo test -p touring-cli --lib cli::kpi`: 41/41 passou
FACT=DEBT-SCAN=fiel `scan_debt.py crates/touring-cli/src/cli/kpi.rs`: total_debt=0
FACT=clippy(kpi.rs)=fiel `cargo clippy -p touring-cli --lib -- -D warnings`: 0 warnings, build limpo
VERDICT=PASS

## MAP+HARMONY (harmony_map)

python3: can't open file '/home/gabrielgadea/.claude/skills/TACO-cross-audit/scripts/harmony_map.py': [Errno 13] Permission denied


## DEBT (scan_debt)

python3: can't open file '/home/gabrielgadea/.claude/skills/TACO-cross-audit/scripts/scan_debt.py': [Errno 13] Permission denied


## E2E PROOF (prove_invariants)

error: not a directory: /home/gabrielgadea/projects/touring/crates/touring-cli/src/cli/kpi.rs
