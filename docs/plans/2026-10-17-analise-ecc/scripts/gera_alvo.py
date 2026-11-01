"""The consolidated target after round H (18/10/2026): every standing row where the ECC is better or alone, minus what is
already fixed or declared ignore/anti-lesson; plus the Touring defects the readers cited, with their confirmation state.

Usage: gera_alvo.py <achados-dir> <saida.md>
Reuses gera_matriz (same loading, same revisions), so the target always agrees with COMPARATIVO-matriz.md.
"""
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))  # the sibling gera_matriz, imported right below
import gera_matriz as gm

# Defects the orchestrator re-checked by command on 18/10/2026 (substring of the reader's text -> what was found).
CONFIRMADOS = {
    "config": "confirmado: nenhum hook protege ruff.toml, clippy.toml, rustfmt.toml ou eslintrc (grep em client/hooks e nos handlers)",
    "checkpoint": "confirmado: ~/.claude/commands/checkpoint.md aponta para workflows/checkpoint.sh, que não existe",
    "SECURITY": "confirmado: SECURITY.md:11 manda ao SUPPORT.md e SUPPORT.md:28 devolve ao SECURITY.md; não há canal privado",
    "budget-verify": "confirmado: `touring budget-verify` verifica o orçamento do DAG, não tokens",
    "cost-report": "confirmado: `touring cost-report` não existe",
    "tdd-enforcer": "confirmado e corrigido no texto (18/10): 0 registros de hook; SKILL.md e loop-engineering deixam de dizer que bloqueia; registrar = T1",
}


def feito(r: dict) -> bool:
    return r["estado"].startswith("FEITO")


def secao_alvo(alvo: list[dict]) -> list[str]:
    md = []
    for chave, nome in gm.DOMINIOS:
        rs = sorted((r for r in alvo if r["dominio"] == chave), key=lambda r: r["fonte"])
        if rs:
            md += [f"## {nome} ({len(rs)})", "", "| Fonte | Funcionalidade | Veredito | Ação | O que fazer |", "|---|---|---|---|---|"]
            md += [f"| {r['fonte']} | {gm.limpa(r['funcionalidade'], 120)} | {gm.ROTULO_V.get(r['veredito'], r['veredito'])} | "
                   f"{r['acao']} | {gm.limpa(r['proposta'], 220)} |" for r in rs]
            md += [""]
    return md


# Triage by command (haiku readers, 18/10/2026), judged by the orchestrator on a sample; ids map back to the
# defect text through triagem/defeitos-a-confirmar.json. CORRECOES_DA_TRIAGEM overrides a triage state the
# orchestrator re-checked and found wrong (the reader looked at main, the fix lives in int97).
TRIAGEM = Path(__file__).parent.parent / "triagem"
CORRECOES_DA_TRIAGEM = {
    "D28": "corrigido no int97 (C6: sanitize_memory_text redige segredos no store); chega com o R97",
    "D11": "refutado na referência (18/10): o gate de Stop existe (loop_stop_guard → loop_outer_gate); «regra 17 (work-outer)» não aparece em nenhum CLAUDE.md",
    "D04": "corrigido 18/10: o docstring do loop_converged dizia fail-open; diz o que o código faz (todas as cláusulas fail-closed)",
    "D07": "corrigido 18/10: a regra 3 do CLAUDE.md do workspace diz que `touring e2e` é análise composta, não teste ponta a ponta",
    "D13": "corrigido 18/10: /plan roda ground_truth_collector + plan_scaffolder reais; o gate plan-detector.sh, que nunca existiu, saiu do texto",
    "D17": "corrigido 18/10: touring-scouter grava num mktemp -d com umask 077 (vivo e espelho)",
    "D20": "corrigido 18/10: --criterion sem arquivo legível devolve uso e exit 2 (teste vermelho→verde, 49 passed)",
    "D24": "corrigido 18/10: a linha #13 do CLAUDE.md global aponta para os executores reais (test_skill_size_guard.py, package_skill.py)",
    "D25": "corrigido 18/10: o inventário declara que o par é por nome e conta os homônimos ambíguos; números inalterados",
    "D23": "corrigido (17/10): o gate confere modelo × tipo; a memória roteamento-de-modelo-so-exige-presenca foi atualizada em 18/10",
}


def carrega_triagem() -> dict[str, str]:
    """Defect text -> triage state, empty when the triage was not run (the target then says «a confirmar»)."""
    lista = TRIAGEM / "defeitos-a-confirmar.json"
    if not lista.exists():
        return {}
    texto_de = {d["id"]: str(d["defeito"]) for d in json.loads(lista.read_text(encoding="utf-8"))}
    estado = {}
    for arq in sorted(TRIAGEM.glob("resultado-*.json")):
        for r in json.loads(arq.read_text(encoding="utf-8")):
            nota = CORRECOES_DA_TRIAGEM.get(r["id"]) or f"{r['estado']} (triagem 18/10): {gm.limpa(r.get('nota', ''), 160)}"
            estado[texto_de[r["id"]]] = f"{r['id']} · {nota}"
    return estado


ESTADO_TRIAGEM = carrega_triagem()


def estado_do_defeito(texto: str) -> str:
    confirmado = next((v for k, v in CONFIRMADOS.items() if k.lower() in texto.lower()), None)
    return confirmado or ESTADO_TRIAGEM.get(texto, "a confirmar")


def secao_defeitos(defeitos: list[dict]) -> list[str]:
    md = ["## Defeitos do Touring citados pelos leitores", "",
          "Hipótese até confirmação por comando. Os marcados foram reconferidos pelo orquestrador.", "",
          "| Lote | Defeito | Evidência do leitor | Estado |", "|---|---|---|---|"]
    md += [f"| {d['fonte']} | {gm.limpa(str(d.get('defeito', '')), 220)} | `{gm.limpa(d.get('evidencia', ''), 140)}` | "
           f"{estado_do_defeito(str(d.get('defeito', '')))} |" for d in defeitos]
    return md


def main() -> int:
    pasta, saida = Path(sys.argv[1]), Path(sys.argv[2])
    linhas, _, defeitos = gm.carrega(pasta)
    gm.aplica_revisoes(linhas)
    gap = [r for r in gm.vigentes(linhas) if r["veredito"] in ("ecc_melhor", "so_ecc")]
    alvo = [r for r in gap if not feito(r) and r["acao"] not in ("ignorar", "anti_licao")]
    n_feito = sum(map(feito, gap))
    md = ["# Alvo consolidado depois da rodada H (gerado por `scripts/gera_alvo.py`)", "",
          (f"{len(gap)} linhas vigentes em que o ECC é melhor ou o único: {n_feito} já corrigidas, "
           f"{len(gap) - len(alvo) - n_feito} declaradas ignorar ou anti-lição, **{len(alvo)} acionáveis**."), ""]
    md += secao_alvo(alvo) + secao_defeitos(defeitos)
    saida.write_text("\n".join(md) + "\n", encoding="utf-8")
    estados = [estado_do_defeito(str(d.get("defeito", ""))) for d in defeitos]
    pendentes = sum(e == "a confirmar" for e in estados)
    print(f"alvo {len(alvo)} · defeitos {len(defeitos)} · triados ou confirmados {len(estados) - pendentes} · a confirmar {pendentes}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
