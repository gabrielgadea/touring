#!/usr/bin/env python3
"""Auditoria do code mode do Touring — estado REAL, por leitura do fonte.

Cada checagem responde uma pergunta verificável e devolve um veredito, nunca
uma impressão. A saída é o agregado; os detalhes ficam no JSON opcional.
"""
from __future__ import annotations

import json
import re
import subprocess
import sys
import pathlib
from pathlib import Path

RAIZ = Path("/home/gabrielgadea/projects/touring")
CRATES = RAIZ / "crates"


def rg(padrao: str, *, glob: str = "*.rs", excluir_teste: bool = True) -> list[str]:
    """Linhas `arquivo:linha:texto` que casam o padrão sob crates/."""
    try:
        out = subprocess.run(
            ["rg", "-n", "--no-heading", "-g", glob, padrao, str(CRATES)],
            capture_output=True, text=True, timeout=60,
        ).stdout
    except (subprocess.SubprocessError, FileNotFoundError):
        return []
    linhas = [l for l in out.splitlines() if l.strip()]
    if excluir_teste:
        linhas = [
            l for l in linhas
            if "/tests/" not in l and "_tests.rs" not in l and "#[test]" not in l
        ]
    return linhas


def curto(linha: str) -> str:
    """`caminho:linha` relativo, sem o texto casado."""
    partes = linha.split(":", 2)
    if len(partes) < 2:
        return linha[:80]
    try:
        return f"{Path(partes[0]).relative_to(RAIZ)}:{partes[1]}"
    except ValueError:
        return f"{partes[0]}:{partes[1]}"


achados: list[dict] = []


def registrar(id_: str, titulo: str, ok: bool, evidencia: str, impacto: str) -> None:
    achados.append(
        {"id": id_, "titulo": titulo, "ok": ok, "evidencia": evidencia, "impacto": impacto}
    )


# ── A. Instrumentação: os counters medem o canal principal? ─────────────────
for counter, fn in (
    ("code_mode_runs_count", "record_code_mode_run"),
    ("code_mode_bytes_elided_total", "record_code_mode_run"),
):
    chamadas = [l for l in rg(rf"\b{fn}\(") if f"fn {fn}" not in l]
    sitios = {curto(l) for l in chamadas}
    relay = rg(r'"cli-code-mode-run"', glob="run.rs")
    via_run = any("cli/run.rs" in s for s in sitios) or bool(relay)
    registrar(
        f"counter:{counter}",
        f"`{fn}` é chamado pelo `touring run`?",
        via_run,
        f"{len(sitios)} sítio(s) diretos + relay em cli/run.rs: {bool(relay)}",
        "o KPI de economia ignora o canal sem-MCP que o programa promove",
    )
    break  # os dois counters vêm da mesma função

# ── B. Sub-chamadas: instrumentadas? (C2-W0) ───────────────────────────────
sub = [l for l in rg(r"record_code_mode_subcall\(") if "fn record_code_mode_subcall" not in l]
registrar(
    "counter:subcalls",
    "sub-chamadas do --orchestrate são contadas?",
    bool(sub),
    f"{len(sub)} sítio(s): {sorted({curto(l) for l in sub}) or 'NENHUM'}",
    "sem isso o contrafactual (economia vs MCP) não existe",
)

# ── C. ADW: os nós `code` alcançam o code mode? ────────────────────────────
# As specs vivem em DOIS lugares reais (medido 24/08/2026): a biblioteca
# central instalada e o diretório do projeto. Apontar para um `adw-library/`
# na raiz do workspace — que não existe — devolvia "0/0 specs", um veredito
# vazio indistinguível de "nenhum fluxo usa code mode".
_DIRS = [
    pathlib.Path.home() / ".claude/skills/Touring/adw-library",
    RAIZ / ".touring/adw",
    pathlib.Path.home() / ".touring/adw",
]
adw_libs = [p for d in _DIRS if d.exists() for p in d.rglob("*.toml")]
# A afordância é `sandbox = true` (o runner é quem troca o comando por
# `touring run`), então procurar a string "touring run" na spec media a forma
# errada e reprovaria a adoção correta.
usa_run = [p for p in adw_libs if "sandbox = true" in p.read_text(errors="ignore")]
registrar(
    "adw:code-mode",
    "fluxos ADW ligam o sandbox (roteia para `touring run`)?",
    bool(usa_run),
    f"{len(usa_run)}/{len(adw_libs)} specs: {sorted(p.name for p in usa_run) or 'NENHUM'}",
    "os ADWs são o maior consumidor de execução — se não usam code mode, o ganho não escala",
)

# ── D. Snippets: a escada é alimentada pelo executor? ──────────────────────
harvest = [l for l in rg(r"settle_snippet_ladder\(") if "fn settle_snippet_ladder" not in l]
registrar(
    "snippet:ladder",
    "o executor matricula/mede snippets sozinho (afordância)?",
    bool(harvest),
    f"{len(harvest)} sítio(s): {sorted({curto(l) for l in harvest}) or 'NENHUM'}",
    "sem o executor, a biblioteca depende de o modelo lembrar — e ela ficou vazia por isso",
)

# ── E. CEG: todo run atravessa o gateway? ──────────────────────────────────
ceg = rg(r"run_gateway|RunGateway", glob="run.rs")
registrar(
    "ceg:run",
    "`touring run` roteia pelo Code Execution Gateway?",
    bool(ceg),
    f"{len(ceg)} referência(s) em cli/run.rs",
    "sem o gate, forbidden-calls e capabilities não valem para o canal principal",
)

# ── F. MCP code-first: o modo reduzido existe e é alcançável? ──────────────
mcp = rg(r"TOURING_MCP_CODE_MODE")
registrar(
    "mcp:code-first",
    "`TOURING_MCP_CODE_MODE` está implementado?",
    bool(mcp),
    f"{len(mcp)} sítio(s): {sorted({curto(l) for l in mcp})[:3]}",
    "handshake de 3 tools é o que torna o MCP barato quando ele é inevitável",
)

# ── G. Nudge: o suggester induz code mode com valor REAL? ──────────────────
nudge = rg(r"code_mode_command|detect_code_mode")
registrar(
    "nudge:induction",
    "o hook induz code mode com comando derivado?",
    bool(nudge),
    f"{len(nudge)} sítio(s)",
    "indução é o que converte disponibilidade em adoção (4-pillars)",
)

# ── H. W7 S-7.4 (plano code-mode-total): as dimensões do plano ─────────────
gates = rg(r"ExitCodeThroughPipe|RedundantExactCall")
registrar(
    "plano:gates-registrados",
    "G2/G6 existem como variantes da enum (executor, não nudge)?",
    len({curto(l).split(":")[0] for l in gates}) >= 3,
    f"{len(gates)} sítio(s) em {len({curto(l).split(':')[0] for l in gates})} arquivo(s)",
    "gate que só existe em prosa é persuasão — D8 exige o executor",
)
teeth = rg(r"burst_gate|G1_DENY_AT", glob="cli_suggester.rs")
registrar(
    "plano:g1-teeth",
    "o contador de rajada NEGA (teeth) com autodemote por dado?",
    bool(teeth) and bool(rg(r"g1_should_deny", glob="cli_suggester.rs")),
    f"{len(teeth)} sítio(s) + autodemote puro",
    "a 3ª sugestão ignorada era o teto da persuasão; o deny muda U(a)",
)
adw_py = Path.home() / ".claude/skills/Touring/scripts/adw.py"
adw_src = adw_py.read_text(encoding="utf-8") if adw_py.is_file() else ""
predicados = all(m in adw_src for m in ('"fixpoint"', '"covered"', '"calibrated"', "FACT_RE", "CONTROL_PASS_RE"))
registrar(
    "plano:loops-e-nos",
    "os 4 predicados de loop + nós probe/control existem no runner?",
    predicados,
    "fixpoint/covered/calibrated + FACT_RE/CONTROL_PASS_RE no adw.py" if predicados else "faltando no adw.py",
    "loop com predicado errado termina CONFIANTE — cada predicado ataca uma classe medida",
)
lib = Path.home() / ".claude/skills/Touring/adw-library"
fluxos_novos = [f for f in ("instrument-first", "family-fix", "freshness-audit") if (lib / f"{f}.toml").is_file()]
registrar(
    "plano:fluxos-novos",
    "instrument-first/family-fix/freshness-audit estão na biblioteca?",
    len(fluxos_novos) == 3,
    f"{len(fluxos_novos)}/3: {fluxos_novos}",
    "o remédio dos gates precisa de fluxo nomeado, não instrução",
)
kpi_yaml = Path("docs/kpi/commitments.yaml")
kpi_ok = kpi_yaml.is_file() and "touring.code_mode.adoption_ratio" in kpi_yaml.read_text(encoding="utf-8")
registrar(
    "plano:kpi-vivo",
    "touring.code_mode.adoption_ratio declarado nos commitments?",
    kpi_ok,
    "commitment presente" if kpi_ok else "ausente do commitments.yaml",
    "sem o denominador, nenhuma wave do plano é julgável",
)

falhas = [a for a in achados if not a["ok"]]
print(f"AUDIT code-mode: {len(achados) - len(falhas)}/{len(achados)} OK, {len(falhas)} defeito(s)\n")
for a in achados:
    marca = "OK  " if a["ok"] else "FALHA"
    print(f"[{marca}] {a['id']}: {a['titulo']}")
    print(f"        {a['evidencia']}")
    if not a["ok"]:
        print(f"        impacto: {a['impacto']}")
if "--json" in sys.argv:
    Path(sys.argv[sys.argv.index("--json") + 1]).write_text(json.dumps(achados, indent=2))
sys.exit(1 if falhas else 0)
