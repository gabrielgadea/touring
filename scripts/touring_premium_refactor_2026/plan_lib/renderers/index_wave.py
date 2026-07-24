"""index_wave — render_index_md + _slug + render_wave_md + render_cross_audit_md.

Extracted from renderers.py lines 64-271. Each module owns one logical
rendering concern (utility, index/wave/cross-audit, one of the 9 cross-cutting
docs). All public functions are re-exported by ``renderers/__init__.py``.
"""
from __future__ import annotations

from .utilities import yaml_frontmatter, md_table, write_atomic, sha256_hex
from ..dataclasses import Wave

def render_index_md() -> str:
    """Render 00-INDEX.md."""
    meta = {
        "plan": _PLAN_NAME,
        "version": _VERSION,
        "created": _TODAY,
        "type": "index",
        "files": [f"{w.id}-{w.name.lower().replace(' ', '-').replace('—', '-')[:40]}.md"
                  for w in WAVES] + ["CROSS-AUDIT.md"],
        "flags": ["--sequential-thinking", "--ultrathink", "--touring-cli",
                  "--code-generator"],
        "cila": "L4+",
        "checkpoint_format": ".toon",
        "total_days_min": sum(w.days_min for w in WAVES),
        "total_days_max": sum(w.days_max for w in WAVES),
    }
    fm = yaml_frontmatter(meta)
    body = textwrap.dedent(f"""\
        # {_PLAN_NAME} — Touring Premium Refactor 2026

        > **Objetivo**: Transformar Touring (46 crates fragmentados, 410k LOC, macrociclo
        > depth 618) em produto premium com 13 crates produtivos, per-project deployment
        > rustup-like, 4 tiers comerciais, e gates de qualidade não-negociáveis.
        > **Total**: {sum(w.days_min for w in WAVES)}-{sum(w.days_max for w in WAVES)} engineer-days.

        ## Flags de Execução

        `--sequential-thinking` `--ultrathink` `--touring-cli` `--code-generator`

        ## Waves
        """)
    rows = [[
        f"[{w.id}]({w.id}-{_slug(w.name)}.md)",
        w.name,
        ", ".join(w.depends_on) or "—",
        w.rust_changes,
        f"{w.days_min}-{w.days_max}",
        "PENDING",
    ] for w in WAVES]
    table = md_table(["Wave", "Nome", "Depende De", "Mudanças", "Dias", "Status"], rows)
    return fm + body + "\n" + table + "\n"


def _slug(name: str) -> str:
    """Convert wave name to URL-safe slug (matches sample plan convention)."""
    s = name.lower().replace(" ", "-").replace("—", "-").replace("/", "-")
    s = "".join(c for c in s if c.isalnum() or c in "-")
    return s[:40].rstrip("-")


def render_wave_md(wave: Wave) -> str:
    """Render a wave's markdown file."""
    meta = {
        "plan": _PLAN_NAME,
        "version": _VERSION,
        "wave": wave.id,
        "name": wave.name,
        "phase": wave.phase,
        "depends_on": wave.depends_on,
        "parallel_with": wave.parallel_with,
        "status": "PENDING",
        "created": _TODAY,
        "cila": wave.cila,
        "rust_changes": wave.rust_changes,
        "estimated_days": f"{wave.days_min}-{wave.days_max}",
        "checkpoint": f"touring_premium_{wave.id}_{_TODAY_COMPACT}.toon",
        "validation_script": f"scripts/touring_premium_refactor_2026/validate_{wave.id}.py",
        "cross_references": ["00-INDEX.md", "CROSS-AUDIT.md"] +
                             [f"{w.id}-*.md" for w in WAVES if w.id != wave.id][:5],
        "discover_protocol": {
            "tantivy": "touring tantivy search '<keyword>' -j",
            "wiring_impact": "touring wiring impact <symbol> --depth 2",
            "ast_blast": "touring ast blast <file>",
            "memory_recall": "touring memory recall '<query>'",
        },
    }
    fm = yaml_frontmatter(meta)
    body = textwrap.dedent(f"""\
        # {wave.id}: {wave.name}

        > **Plano**: `{_PLAN_NAME}` v{_VERSION}
        > **Fase**: {wave.phase}
        > **Contribuição para resultado final**: {wave.contribution}

        ---

        ## Contexto e Dependências

        - **Depende de**: {', '.join(wave.depends_on) if wave.depends_on else 'Nenhuma'}
        - **Paralelo com**: {', '.join(wave.parallel_with) if wave.parallel_with else 'Nenhuma'}
        - **CILA**: `{wave.cila}`
        - **Mudanças Rust**: `{wave.rust_changes}`
        - **Estimativa**: {wave.days_min}-{wave.days_max} dias
        - **Checkpoint**: `{meta['checkpoint']}`
        - **Script de validação**: `{meta['validation_script']}`

        ---

        ## Descrição

        {wave.description}

        ---

        ## Efeitos no Sistema

        """)
    effects = "\n".join(f"- {e}" for e in wave.effects)
    body += effects + "\n\n---\n\n## Subtarefas (CODE-FIRST — DISCOVER antes de cada)\n\n"
    body += "> **PROTOCOLO DISCOVER OBRIGATÓRIO antes de cada subtarefa**:\n"
    body += "> 1. `touring tantivy search '<keyword>' -j` (Tantivy BM25)\n"
    body += "> 2. `touring wiring impact <symbol> --depth 2` (transitive consumers)\n"
    body += "> 3. `touring ast blast <file>` (dependency tree)\n"
    body += "> 4. `touring memory recall '<query>'` (past lessons)\n"
    body += "> 5. `touring index find <symbol> -j` (VGP gate)\n\n"
    for st in wave.subtasks:
        body += f"### {st.id}: {st.name}\n\n"
        body += f"**Descrição**: {st.description}\n\n"
        body += f"**Dias estimados**: {st.days}\n\n"
        if st.discover:
            body += "**DISCOVER obrigatório**:\n"
            for d in st.discover:
                body += f"  - {d}\n"
            body += "\n"
        if st.tdd_red:
            body += f"**TDD RED** (escrever ANTES do código):\n```python\n{st.tdd_red}\n```\n\n"
        if st.validation:
            body += f"**Critério de validação**: {st.validation}\n\n"
        if st.blocking:
            body += "**🛑 BLOCKING**: Esta subtarefa bloqueia as posteriores se falhar.\n\n"
        body += "---\n\n"
    body += f"## Gate de Saída\n\n{wave.gate}\n\n"
    if wave.risks:
        body += "## Riscos Específicos\n\n"
        for r in wave.risks:
            body += f"- {r}\n"
        body += "\n"
    body += textwrap.dedent("""\
        ## Checklist de Conclusão

        - [ ] Todos os subtasks implementados
        - [ ] Todos os testes TDD GREEN
        - [ ] `cargo check --workspace` exit 0
        - [ ] `cargo test --workspace --no-fail-fast` pass
        - [ ] `cargo clippy --workspace -- -D warnings` clean
        - [ ] `touring wiring cycles --min-depth 2` no new cycles
        - [ ] `touring wiring orphans -j` no new orphans (REGRA #0)
        - [ ] Bench regression < 5%
        - [ ] Test ratio ≥ 20% per touched crate
        - [ ] Checkpoint `.toon` salvo
        - [ ] Memory lesson persistida (`touring memory store --tier semantic`)
        - [ ] RL reward injetado (`touring learning reward orchestrate <val>`)
        - [ ] Documentação atualizada (se necessário)
        """)
    return fm + body


def render_cross_audit_md() -> str:
    """Render CROSS-AUDIT.md — stub, expanded in Edit phase."""
    meta = {
        "plan": _PLAN_NAME,
        "version": _VERSION,
        "type": "cross-audit",
        "created": _TODAY,
        "depends_on": [w.id for w in WAVES],
        "script": "scripts/touring_premium_refactor_2026/cross_audit_e2e.py",
    }
    fm = yaml_frontmatter(meta)
    body = textwrap.dedent("""\
        # Cross-Audit E2E — touring-premium-refactor-2026

        > **Propósito**: Verificar que TODAS as 15 waves atingiram seus objetivos e que
        > o plano cumpriu sua finalidade: transformar Touring em produto premium.

        ## Script de Auditoria

        ```bash
        python3 scripts/touring_premium_refactor_2026/cross_audit_e2e.py --full
        ```

        ## 10 Dimensões de Avaliação

        | # | Dimensão | Peso | Verificação |
        |---|---|---|---|
        | D1 | Funcional — código executa, testes passam | 2.0 | cargo test workspace pass |
        | D2 | Wiring — zero ciclos, zero orphans | 1.5 | touring wiring cycles + orphans |
        | D3 | Performance — < 5% regressão vs baseline | 1.5 | Criterion benches |
        | D4 | Cobertura — ≥ 20% LOC ratio por crate | 1.5 | cargo llvm-cov per crate |
        | D5 | Mutation — kill rate ≥ 80% | 1.0 | cargo mutants |
        | D6 | API Stability — semver-check clean | 1.5 | cargo public-api + semver-check |
        | D7 | Supply Chain — deny+audit+vet clean | 1.0 | cargo deny check |
        | D8 | Documentation — docs.rs green | 1.0 | cargo doc warnings-as-errors |
        | D9 | Deployment — per-project funcional | 1.5 | touring init pilot OK |
        | D10 | Propósito — produto premium entregue | 2.0 | 1.0.0 GA + 4 tiers ativos |

        ## Critérios de Sucesso

        - **Composite score** ≥ 0.95 (média ponderada das 10 dimensões)
        - **Nenhuma dimensão** < 0.80 (VETO threshold)
        - **D10 Propósito** OBRIGATORIAMENTE ≥ 0.95 (plano só passa se entrega o produto)

        ## Verificação por Wave

        Veja `cross_audit_e2e.py` para a tabela completa de critérios por wave.
        """)
    return fm + body


