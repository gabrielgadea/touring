"""HORUST-REAL — verifica a integridade do corpus montado e SELA (selo FORA do corpus). Nada de motor nem de juiz.

Uso: python3 -I -B selar_horust_real.py [--teto-negativos 0.30]   (o 0.30 só como DESVIO NOMEADO do pré-registro, seção 5)
"""

from __future__ import annotations

import argparse
import json
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import construir_horust_real as cr  # noqa: E402
import integridade_horust as ih  # noqa: E402

SELO = HERE / "selo.json"
GERADOR = ["operadores_rust.py", "oraculo_cargo_json.py", "selecao_horust.py", "construtor_horust.py", "construir_horust_real.py", "integridade_horust.py", "selar_horust_real.py"]


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--teto-negativos", type=float, default=None)
    a = ap.parse_args()
    dev, controle = cr.dev_e_controle()
    violacoes = ih.verificar(cr.CORPUS, cr.RAIZ, dev, controle, cr.MIN_POR_CLASSE, cr.NEG_MIN, cr.TETO, a.teto_negativos)
    (HERE / "integridade.json").write_text(json.dumps({"violacoes": violacoes, "dev_n": len(dev), "controle_n": len(controle)}, ensure_ascii=False, indent=1))
    for v in violacoes:
        print("VIOLACAO", v)
    if violacoes:
        return 1
    cr.worktree_limpo()
    reconf = HERE / "estado/reconfirmacao.json"
    if not reconf.is_file() or json.loads(reconf.read_text())["discordancias"]:
        print("VIOLACAO reconfirmação ausente ou com discordância: não selo")
        return 1
    hora = {k: v for k, v in (x.split(": ", 1) for x in (HERE / "pre-registro.hora").read_text().splitlines())}
    extra = {
        "worktree": str(cr.RAIZ), "worktree_head": cr.HEAD_ESPERADO, "worktree_tree": cr.git("rev-parse", "HEAD^{tree}"), "worktree_limpo": True,
        "pre_registro_gravado": hora["gravado"], "pre_registro_sha256_original": hora["sha256"], "pre_registro_tamanho_original": int(hora["tamanho_original"]),
        "adendos_gravado": hora["adendos_gravado"], "pre_registro_sha256_com_adendos_na_hora": hora["sha256_com_adendos"], "semente": cr.SEMENTE,
        "base_check_sha256": cr.sha_arquivo(cr.BASE_CHECK), "resultados_sha256": cr.sha_arquivo(cr.ESTADO / "resultados.jsonl"),
        "reconfirmacao_sha256": cr.sha_arquivo(reconf), "regime_do_juiz": "estrita: sem disk_file, sem estado_no_instante",
        "limitacao": "recall sobre erro INJETADO em contexto real; nao e a distribuicao natural",
    }
    selo = ih.montar_selo(cr.CORPUS, HERE / "PRE-REGISTRO.md", [HERE / g for g in GERADOR], extra)
    if not ih.verificar_prefixo(HERE / "PRE-REGISTRO.md", int(hora["tamanho_original"]), hora["sha256"]):
        print("VIOLACAO os bytes ORIGINAIS do PRE-REGISTRO.md mudaram (o prefixo não bate com o sha da hora registrada)")
        return 1
    if selo["pre_registro_sha256"] != hora["sha256_com_adendos"]:
        print("VIOLACAO o PRE-REGISTRO.md mudou depois dos adendos registrados")
        return 1
    SELO.write_text(json.dumps(selo, ensure_ascii=False, indent=1))
    print(json.dumps({k: selo[k] for k in ("sha_tree", "arquivos", "bytes", "pre_registro_sha256")}, ensure_ascii=False))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
