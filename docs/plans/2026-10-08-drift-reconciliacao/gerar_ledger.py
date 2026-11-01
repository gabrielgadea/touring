#!/usr/bin/env python3
"""Gera ledger.jsonl — UMA linha por sinal do delta (111), com a disposição final.

Decisões da sessão (08/10/2026), cada uma medida contra os trechos reais
(ocorrencias.json) e contra o destino sob a régua final do scanner
(drift-scan-final-2026-10-08.json):

  (a) corrigido na fonte — NENHUM: o delta inteiro é registro datado legítimo
      ou falso positivo de predicado; nenhum doc vivo apresentava fato errado.
  (b) falso positivo do scanner — 45 sinais, três predicados corrigidos:
      C4-contexto (prosa), C4-path (".../projects/touring <cmd>"),
      C4-gt (help lido de stdout, sempre vazio desde 31/08), C6-gt (os.environ
      volátil → código-fonte estático). Prova: test_scanner_predicates.py.
  (c) legítimo e declarado — 66 sinais: citações em registros datados
      (docs/plans/<data>-*, docs/audits/, turns, changelog embutido no
      CLAUDE.md raiz) que NÃO se reescrevem.

Cada linha: categoria, item, arquivo/linha/trecho (ocorrência principal),
n_ocorrencias, disposicao, motivo (≥30 chars em c), predicado (em b).
"""
import json
import re
import subprocess
import sys
from pathlib import Path

BUNDLE = Path(__file__).resolve().parent
REPO = BUNDLE.parents[2]

MOTIVO_C1_DATADO = ("citação de versão histórica em registro datado (docs/plans|audits); "
                    "não se reescreve história")
MOTIVO_C4_B_PROSA = ("regex `touring <palavra>` sem âncora de contexto capturava prosa "
                     "portuguesa; predicado corrigido exige contexto de código")
MOTIVO_C4_B_PATH = ("falso positivo por path: '.../projects/touring <token>' casa o "
                    "diretório do workspace; predicado corrigido exclui 'touring' precedido de '/'")
MOTIVO_C4_B_PATH_BUILD = ("invocação por path de build ('./target/debug/touring <cmd>') em registro "
                          "datado; o predicado novo exclui 'touring' precedido de '/' por construção "
                          "— a citação canônica é o binário no PATH")
MOTIVO_C4_B_GT = ("o comando EXISTE no `touring --help`; o ground truth lia só stdout e o "
                  "help sai no stderr — gt vazio desde 31/08; corrigido para stdout+stderr")
MOTIVO_C6_B = ("var viva no código-fonte (.rs/.py/.sh/.toml); o ground truth antigo media "
               "os.environ de quem roda — volátil entre sessões; corrigido para código estático")

# Motivos (c) específicos por item, quando o genérico não basta.
MOTIVO_C_ESPECIFICO = {
    ("C1", "30.4.29"): ("heading de wave datada (01/09/2026) em crates/touring-server/.claude/CLAUDE.md "
                        "— registro histórico; arquivo estava sujo (outra sessão editando), não tocado"),
    ("C1", "30.4.75"): ("citação datada com hash de commit em docs/code-mode.md "
                        "('desde 26/09 (daa295cd, 30.4.75)') — proveniência de feature, não versão corrente"),
    ("C1", "30.4.93"): ("plano datado (docs/plans/2026-10-06-retomada) cita o release 30.4.93 planejado "
                        "— doc de planejamento, não afirmação de estado"),
    ("C4", "context7"): ("audit datado (docs/audits/causa-raiz-instrumento-2026-10-01.md) DOCUMENTA que "
                         "'touring context7' não existe — registro de verificação, não drift"),
    ("C4", "daemon_ctl"): ("turn log datado (14/09): 'daemon_ctl' é filtro de cargo test, não invocação "
                           "de subcomando; o real é daemon-ctl (hífen)"),
    ("C4", "find"): ("spec datada (31/08) documenta que 'touring find references' NÃO existia "
                     "(F1.5 o criaria) — registro de gap histórico"),
    ("C4", "module"): ("plano datado (02/09, touring-proxima-geracao) propõe 'touring module list|...' "
                       "— proposta registrada, não comando corrente"),
    ("C5", "touring-language"): ("mapa de migração em crates/touring-code/.claude/CLAUDE.md "
                                 "('use touring_language::X → use touring_code::languages::X') — cita o "
                                 "nome antigo para ensinar o novo; reescrever destruiria a instrução; "
                                 "arquivo estava sujo (outra sessão), não tocado"),
    ("C6", "TOURING_JEV_INTENT"): ("plano datado (30/09, jev-harness) cita var proposta que não consta "
                                   "no código atual — registro de decisão, não drift"),
    ("C6", "TOURING_CILA_BUDGET_TURN"): ("plano datado (02/09) cita a var pelo nome completo singular; "
                                         "o código implementa a família TOURING_CILA_BUDGET_*"),
    ("C6", "TOURING_SDK_SIGN"): ("description datada de fase (31/08, code-mode-sinal P2) — registro "
                                 "histórico de implementação"),
    ("C6", "TOURING_TANT"): ("turn log datado (14/09) — fragmento de comando Bash registrado"),
    ("C6", "TOURING_TANTIVY_COVERAGE_FIELDS"): ("turn log datado (14/09): var experimental usada numa "
                                                "sessão de medição (scratchpad), registrada no turno"),
    ("C6", "TOURING_TANTIVY_WORD_COVERA"): ("turn log datado (14/09): token truncado na linha do "
                                            "comando registrado — histórico, não se reescreve"),
    ("C6", "TOURING_TDD_ENFORCE"): ("explicação datada (01/09, skill-aprimoramento) cita a var da "
                                    "época — registro histórico"),
}

# Versões do C1 cuja ocorrência em doc vivo é entrada de CHANGELOG embutido
# (medido nos trechos: todas datadas "…(DD/MM/2026, 30.4.XX)…").
C1_CHANGELOG_CLAUDEMD = {"30.4.51", "30.4.56", "30.4.57", "30.4.58", "30.4.59", "30.4.60",
                         "30.4.61", "30.4.62", "30.4.63", "30.4.64", "30.4.65"}


def _motivo_c1(item: str, paths: list[str]) -> str:
    if ( "C1", item) in MOTIVO_C_ESPECIFICO:
        return MOTIVO_C_ESPECIFICO[("C1", item)]
    if item in C1_CHANGELOG_CLAUDEMD:
        return ("entrada datada de changelog na seção de histórico do CLAUDE.md raiz "
                "(ex.: '(15/09/2026, 30.4.51)') — registro de release; o arquivo está "
                "fora do escopo de edição desta delegação e a citação não é drift")
    return MOTIVO_C1_DATADO + f" — paths: {', '.join(paths[:3])}"


def _disposicao_c4(item: str, ficam: set[str], ocorrencias: list[dict], help_cmds: set[str]) -> tuple[str, str, str]:
    """Retorna (disposicao, motivo_ou_predicado, classe_b)."""
    if item in ficam:
        return "c", MOTIVO_C_ESPECIFICO[("C4", item)], ""
    # Sob a régua final o sinal sumiu — qual dos falsos positivos era?
    trechos = " ".join(h["trecho"] for h in ocorrencias)
    if f"debug/touring {item}" in trechos or f"release/touring {item}" in trechos:
        return "b", MOTIVO_C4_B_PATH_BUILD, "c4-path-build"
    if f"projects/touring {item}" in trechos:
        return "b", MOTIVO_C4_B_PATH, "c4-path-diretorio"
    # comando existe no help real?
    if item in help_cmds:
        return "b", MOTIVO_C4_B_GT, "c4-gt-help-stderr"
    return "b", MOTIVO_C4_B_PROSA, "c4-contexto"


def main() -> int:
    scan_path = sys.argv[sys.argv.index("--scan-json") + 1] if "--scan-json" in sys.argv \
        else "/tmp/drift-scan-final-2026-10-08.json"
    delta = json.loads((BUNDLE / "delta.json").read_text(encoding="utf-8"))["delta"]
    occ = json.loads((BUNDLE / "ocorrencias.json").read_text(encoding="utf-8"))
    scan_final = json.loads(Path(scan_path).read_text(encoding="utf-8"))
    cats = scan_final["categories"]
    now = {
        "C1": set(cats["C1-versions"]["stale_versions"]),
        "C3": set(cats["C3-fused-crates-cited"]["fused_crates_with_citations"].keys()),
        "C4": set(cats["C4-cli-cited"]["potential_stale"]),
        "C5": set(cats["C5-crates-in-code-examples"]["stale"]),
        "C6": set(cats["C6-envvars-cited"]["stale"]),
    }
    r = subprocess.run(["touring", "--help"], capture_output=True, text=True, cwd=REPO, timeout=30)
    help_cmds = set(re.findall(r"^\s+([a-z][a-z0-9_-]+)\s", r.stdout + "\n" + r.stderr, re.MULTILINE))

    linhas = []
    for cat in ("C1", "C3", "C4", "C5", "C6"):
        for item in sorted(delta[cat]):
            hits = occ[cat][item]
            first = hits[0]
            paths = sorted({h["arquivo"] for h in hits})
            base = {"categoria": cat, "item": item, "arquivo": first["arquivo"],
                    "linha": first["linha"], "trecho": first["trecho"],
                    "n_ocorrencias": len(hits)}
            if cat == "C1":
                base |= {"disposicao": "c", "motivo": _motivo_c1(item, paths)}
            elif cat == "C4":
                disp, motivo, classe_b = _disposicao_c4(item, now["C4"], hits, help_cmds)
                base |= {"disposicao": disp, "motivo": motivo}
                if classe_b:
                    base["predicado"] = classe_b
            elif cat == "C5":
                motivo = MOTIVO_C_ESPECIFICO.get(("C5", item),
                         f"nome de crate citado em code fence de registro datado ({paths[0]}); "
                         "planejamento/histórico, não código corrente")
                base |= {"disposicao": "c", "motivo": motivo}
            elif cat == "C6":
                if item in now["C6"]:
                    base |= {"disposicao": "c",
                             "motivo": MOTIVO_C_ESPECIFICO.get(("C6", item),
                                     "var citada em registro datado; não consta no código atual")}
                else:
                    base |= {"disposicao": "b", "motivo": MOTIVO_C6_B, "predicado": "c6-gt-codigo"}
            linhas.append(base)

    out = BUNDLE / "ledger.jsonl"
    with out.open("w", encoding="utf-8") as f:
        for row in linhas:
            f.write(json.dumps(row, ensure_ascii=False) + "\n")
    from collections import Counter
    dist = Counter((r["categoria"], r["disposicao"]) for r in linhas)
    print(f"ledger: {len(linhas)} linhas")
    for k in sorted(dist):
        print(f"  {k[0]} {k[1]}: {dist[k]}")
    print(f"gravado: {out}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
