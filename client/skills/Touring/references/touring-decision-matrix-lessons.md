# touring-decision-matrix — Lessons Learned (Touring Premium Refactor 2026-05-11)

Companion reference for `~/.claude/rules/touring-decision-matrix.md`. The rule keeps the C01-C12 categories + reflex triggers + pre-action checklist + post-mortem 2026-05-10 anti-patterns. This file holds the 5 lessons (L1-L5) from the Touring Premium Refactor wave (2026-05-11, 8+ hours, 55 Python scripts, 164 pytest tests, 26 plan docs, 8 iterações W8 v1→v5) — patterns aplicáveis a sessões futuras similares.

## L1 — Iterar versões com forensic measurement (W8 v1→v5)

Quando uma classificação algorítmica produz resultados subótimos (bucket fallback gigante, cycles inesperados), **não tente fix em uma única iteração**. Trate como ciclo CRC:

```
v1: heurística inicial (escopo amplo)
v2: forensic mensuração revela bottleneck (e.g., 75% num bucket fallback)
v3: refine regex/rules — bottleneck diminui mas cycles aumentam (informação MAIS honesta)
v4: extract shared bucket — pega 86% do residual mas reveals leaf violations
v5: enforce leaf invariant — cycles -56%, shared agora pure leaf
```

Cada versão deve ser **single hypothesis** (uma mudança mensurável). Quando v(N+1) move o ponteiro <10%, parar OR pivotar para semantic analysis profunda.

## L2 — Leaf invariant para bucket classification

Ao dividir um crate em sub-crates, o "shared types" bucket DEVE ser **leaf** (no outgoing `crate::` deps). Violação cria ciclos onde antes não havia.

**Detection**: para cada file em SHARED, grep `^use crate::` e flag se importa de qualquer bucket non-shared/non-facade.

**Auto-fix**: `LEAF_VIOLATORS: dict[str, str]` map que relocaliza para o bucket correto. Files que **importam** runtime/knowledge/branch_fs são consumers desses módulos — pertencem ao bucket que CONSOME (tools/lifecycle), não ao bucket leaf.

## L3 — `textwrap.dedent` gotcha (template generation)

**Sintoma**: stubs gerados com indentação residual (4-space prefix em cada linha).
**Root cause**: `textwrap.dedent` calcula o **mínimo** common leading whitespace. Se uma linha do f-string tem menos indent que o resto (ex: variable interpolation com prefix menor), dedent remove APENAS o menor — preservando indent residual.

**Fix**: garantir que TODAS as linhas substituídas tenham o mesmo prefix do common leading do template.

```python
# WRONG: cli_args usa 4-space, template usa 8-space common leading
cli_args = "\n".join(f"    parser.add(...)" for arg in args)
template = textwrap.dedent(f"""\
        def main():
            {cli_args}    # ← residual 4-space prefix no output

# RIGHT: 8-space prefix matching common leading
cli_args = "\n".join(f"        parser.add(...)" for arg in args)
```

## L4 — Cross-audit baseline distinguishes PENDING vs FAIL

Planos com 15 waves PENDING todas retornam composite=0.0 → look idêntico a "todas as waves falharam". Não distingue **estado inicial** de **estado falho**.

**Fix**: adicionar `--baseline` mode no cross-audit que:

1. Detecta PENDING via missing evidence files OR validate_WX status="PENDING"
2. Em baseline mode, exclui PENDING da dimension averaging
3. Status "BASELINE" (exit 0) quando todas são PENDING, "FAIL" só quando alguma executou e falhou

Apply em qualquer cross-audit script para plans multi-wave longos.

## L5 — Forensic discovery first, refactor second

Antes de qualquer refactor, **rodar auto-scripts que medem o estado atual**. Cada descoberta forense vale 10× o tempo de criação do script.

Exemplos desta sessão:

- W2: 220 rewrites em 32 crates contra 135 workspace deps (esperava ~50)
- W4: 77 consumer files (esperava ~38)
- W6: cortex JÁ tem 236% pub-ratio (premissa de "0.56%" estava errada)
- W3.2: anemic crates 100% overlap com W1 KNOWN_DEAD
- W13: 1.572 packages no Cargo.lock (43 workspace + 1.529 external)

**Heurística**: para cada wave do plano, criar um sub-script de medição ANTES de criar o sub-script de execução. Medir desbloqueia decisões de escopo.
