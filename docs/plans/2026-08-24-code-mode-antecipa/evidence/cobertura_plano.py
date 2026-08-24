"""O plano cobre TODO o escopo do strategy doc? Programa decide, não impressão."""
import pathlib, re
B = pathlib.Path("/home/gabrielgadea/projects/touring/docs/plans/2026-08-24-code-mode-antecipa")
plano = (B/"plan.md").read_text()
estrategia = (B/"strategy-code-mode-antecipa-declaracao.md").read_text()

ITENS = {
 # da estratégia (Partes I-III)
 "E1 rajada→teeth": ["G1","rajada"], "E2 pipe-exit": ["G2","pipe"],
 "E3 contrafactual run_id": ["contrafactual","run_id"], "E4 veredito evidencia": ["_lint_verdict_needs_evidence"],
 "E5 harvest/--file": ["--harvest","--file"], "E6 stop hook prosa": ["Stop","loop_stop_guard"],
 "G3 edit-sem-read": ["G3"], "G4 telemetria": ["G4"], "G5 edit-sem-valid": ["G5"],
 "G6 redundante": ["G6"], "G7 re-inspecao": ["G7"],
 "R1-R8 repertorio": ["R1","R8","repertório","snippet"],
 "until_fixpoint": ["until_fixpoint"], "until_covered": ["until_covered"],
 "until_calibrated/control": ["control"], "retry_with_feedback": ["retry_with_feedback"],
 "probe/FACT": ["probe","FACT="], "4 lints": ["_lint_loop_marker_matches_type","_lint_gate_has_control","_lint_sweep_declares_floor"],
 "family-fix": ["family-fix"], "instrument-first": ["instrument-first"],
 "freshness-audit": ["freshness-audit"], "claim-ledger": ["claim-ledger"],
 "staleness 42%": ["staleness"], "instrumento 38%": ["instrumento"],
 "ausencia_como_zero": ["ausencia","paginação","paginacao"], "familia_parcial": ["família","familia"],
 "KPI adoption": ["adoption_ratio"], "baseline": ["baseline"],
 "promote/demote": ["demote"], "P9 verify-after": ["P9"],
 # das FONTES (origem 23/08) — o plano deve contemplá-las (não se limitar à estratégia)
 "dsh COLLAPSE (D8)": ["CODE_ONLY","collapse","colapso"],
 "dsh KV-cache hygiene": ["KV","kv-cache","kv_cache","prefixo estável","prefixo estavel"],
 "TanStack typings no prompt (P3)": ["sdk-stub","typings","tipos no prompt"],
 "TanStack skills/código-vira-tool (P5)": ["escada","trust","ladder"],
 "TanStack self-healing (P6)": ["self-healing","erro como contexto","feedback"],
 "contrafactual MEDIDO (overlay)": ["contrafactual","bytes_elided","overlay"],
 "aprovacao humana no code-tool": ["needsApproval","aprovação","aprovacao"],
 "modelo barato p/ codigo (P8)": ["Haiku","modelo barato","tier"],
 "allowed_callers (API)": ["allowed_callers"],
 "input rewrite (updatedInput)": ["updatedInput","reescrev","updated_input"],
}
faltam = []
for nome, chaves in ITENS.items():
    if not any(k.lower() in plano.lower() for k in chaves):
        faltam.append(nome)
print(f"ITENS={len(ITENS)}  COBERTOS={len(ITENS)-len(faltam)}  FALTAM={len(faltam)}")
for f in faltam: print("  MISS", f)
