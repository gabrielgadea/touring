"""HORUST-REAL / estrato cfg — verifica a integridade do corpus cfg montado e SELA (selo FORA do corpus, `selo-cfg.json`). Sem motor, sem juiz.
O selo guarda o sha do PRE-REGISTRO.md original E o do adendo; o selo do corpus principal não é tocado.

Uso: python3 -I -B selar_cfg.py
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import construir_cfg as cc  # noqa: E402
import construir_horust_real as cr  # noqa: E402
import integridade_cfg as ic  # noqa: E402
import integridade_horust as ih  # noqa: E402

SELO = HERE / "selo-cfg.json"
GERADOR = [
    "operadores_rust.py", "oraculo_cargo_json.py", "selecao_horust.py", "construtor_horust.py", "construir_horust_real.py", "operador_cfg.py",
    "construir_cfg.py", "integridade_cfg.py", "integridade_horust.py", "selar_cfg.py",
]


def hora(arquivo: str) -> dict[str, str]:
    return dict(x.split(": ", 1) for x in (HERE / arquivo).read_text().splitlines())


def main() -> int:
    dev, controle = cr.dev_e_controle()
    principal = cc.excluidos_do_principal()
    violacoes = ic.verificar(cc.CORPUS_CFG, cr.RAIZ, dev, controle, principal, cc.MIN_CFG, cc.MAX_CFG, cc.TETO)
    (HERE / "integridade_cfg.json").write_text(json.dumps({"violacoes": violacoes, "principal_n": len(principal)}, ensure_ascii=False, indent=1))
    for v in violacoes:
        print("VIOLACAO", v)
    if violacoes:
        return 1
    cr.worktree_limpo()
    if not cc.RECONF_JSON.is_file() or json.loads(cc.RECONF_JSON.read_text())["discordancias"]:
        print("VIOLACAO reconfirmação do estrato ausente ou com discordância: não selo")
        return 1
    h_cfg, h_main = hora("pre-registro-cfg.hora"), hora("pre-registro.hora")
    n = len([x for x in (cc.CORPUS_CFG / "entradas.jsonl").read_text().splitlines() if '"rotulo": "positivo"' in x])
    extra = {
        "estrato": ic.ESTRATO, "worktree": str(cr.RAIZ), "worktree_head": cr.HEAD_ESPERADO, "worktree_tree": cr.git("rev-parse", "HEAD^{tree}"), "worktree_limpo": True,
        "pre_registro_gravado": h_main["gravado"], "pre_registro_sha256_original": h_main["sha256"], "pre_registro_tamanho_original": int(h_main["tamanho_original"]),
        "pre_registro_sha256_com_adendos": h_main["sha256_com_adendos"], "adendos_gravado": h_main["adendos_gravado"],
        "adendo_gravado": h_cfg["gravado"], "adendo_sha256_na_hora": h_cfg["sha256"], "semente": cc.SEMENTE_CFG,
        "resultados_sha256": cr.sha_arquivo(cr.ESTADO / cc.ARQUIVO), "plano_sha256": cr.sha_arquivo(cc.PLANO_JSON), "reconfirmacao_sha256": cr.sha_arquivo(cc.RECONF_JSON),
        "positivos": n, "piloto_n_abaixo_de_20": ic.piloto(n, cc.MIN_CFG), "regime_do_juiz": "estrita: sem disk_file, sem estado_no_instante",
        "conta_nas_4_classes": False,
        "limitacao": "recall sobre funcao movida para #[cfg(test)] e usada fora dele (E0425 no mesmo arquivo); nao e a distribuicao natural; alvo de teste nao compilado",
    }
    selo = ih.montar_selo(cc.CORPUS_CFG, HERE / "PRE-REGISTRO-ADENDO-CFG.md", [HERE / g for g in GERADOR], extra)
    if selo["pre_registro_sha256"] != h_cfg["sha256"]:
        print("VIOLACAO o PRE-REGISTRO-ADENDO-CFG.md mudou depois da hora registrada")
        return 1
    if not ih.verificar_prefixo(HERE / "PRE-REGISTRO.md", int(h_main["tamanho_original"]), h_main["sha256"]):
        print("VIOLACAO os bytes ORIGINAIS do PRE-REGISTRO.md mudaram (o prefixo não bate com o sha da hora registrada)")
        return 1
    if cr.sha_arquivo(HERE / "PRE-REGISTRO.md") != h_main["sha256_com_adendos"]:
        print("VIOLACAO o PRE-REGISTRO.md mudou depois dos adendos registrados")
        return 1
    selo["pre_registro_principal_sha256"] = h_main["sha256_com_adendos"]
    SELO.write_text(json.dumps(selo, ensure_ascii=False, indent=1))
    print(json.dumps({k: selo[k] for k in ("sha_tree", "arquivos", "bytes", "pre_registro_sha256", "pre_registro_principal_sha256", "positivos", "piloto_n_abaixo_de_20")}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
