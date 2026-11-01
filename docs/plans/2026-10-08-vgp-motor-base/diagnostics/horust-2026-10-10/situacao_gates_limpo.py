"""HORUST — quantos dos positivos estao na situacao dos 4 `gates_limpo`: o rustc disse «nao achei», mas o ESTADO DO CRATE NO INSTANTE
(reconstruido) ja DEFINE o nome. Classifica os 26 positivos que ficam no corpus por 2 eixos textuais:

  no_instante  o nome tem DEFINICAO (fn/struct/enum/trait/type/const/static/mod/union/macro, `use .. as nome`) em algum .rs do crate no instante
  hoje         idem, nos .rs do crate no disco de hoje
  cfg          a definicao no instante vem logo depois de um atributo `#[cfg(..)]` (item condicional: some em outro alvo do `--all-targets`)

  (no_instante nao, hoje sim)   restauravel pelo estado do instante: a hipotese da coordenadora
  (no_instante sim, hoje sim)   o estado do instante tambem define: cfg/alvo, dobra divergente, ou caminho qualificado
  (nao, nao)                    o nome nunca esteve neste crate: outro crate, dependencia, macro, `Self`
  (sim, nao)                    definido no instante, sumiu hoje

Proxy textual; nao resolve modulo nem avalia `cfg`. Uso: python3 -I -B situacao_gates_limpo.py <saida.json>
"""

import collections
import json
import re
import sys
from pathlib import Path

D = Path(__file__).resolve().parent
V = Path.home() / ".claude/touring/vgp"
CORPUS = V / "horust_medicao/crate/corpus"
entradas = {e["id"]: e for e in (json.loads(x) for x in (CORPUS / "entradas.jsonl").read_text().splitlines() if x.strip())}
manifesto = json.loads((CORPUS / "estado_no_instante/manifesto.json").read_text())
positivos = sorted(i for i, e in entradas.items() if e["rotulo"] == "positivo")


def definicao(nome: str) -> re.Pattern:
    n = re.escape(nome)
    return re.compile(rf"(?:\b(?:fn|struct|enum|trait|type|const|static|mod|union)\s+{n}\b|\bmacro_rules!\s*{n}\b|\buse\b[^;]*\bas\s+{n}\b)")


def define(texto: str, padrao: re.Pattern) -> tuple[bool, bool]:
    """(ha definicao, a primeira definicao vem sob `#[cfg(..)]`)."""
    linhas = texto.splitlines()
    for n, linha in enumerate(linhas):
        if padrao.search(linha):
            antes = [x.strip() for x in linhas[max(0, n - 4):n]]
            return True, any(x.startswith("#[cfg(") for x in antes)
    return False, False


def crate_dir(path: str) -> Path:
    return Path(path.split("/src/")[0])


def main() -> int:
    linhas = []
    for i in positivos:
        e = entradas[i]
        pad = definicao(e["nome_esperado"])
        inst, inst_cfg = False, False
        for ov in manifesto[i]["overlays"]:
            ok, cfg = define((CORPUS / ov["file"]).read_text(encoding="utf-8", errors="replace"), pad)
            if ok:
                inst, inst_cfg = True, inst_cfg or cfg
        hoje = False
        for f in crate_dir(e["path"]).rglob("*.rs"):
            if "/target/" in str(f):
                continue
            if define(f.read_text(encoding="utf-8", errors="replace"), pad)[0]:
                hoje = True
                break
        linhas.append({"id": i, "classe": e["classe"], "nome": e["nome_esperado"], "no_instante": inst, "hoje": hoje, "cfg": inst_cfg})
    cel = collections.Counter(("sim" if x["no_instante"] else "nao", "sim" if x["hoje"] else "nao") for x in linhas)
    res = {"n": len(linhas), "celulas(no_instante,hoje)": {f"{a}/{b}": n for (a, b), n in cel.items()},
           "no_instante_sob_cfg": sum(x["cfg"] for x in linhas), "itens": linhas}
    Path(sys.argv[1]).write_text(json.dumps(res, ensure_ascii=False, indent=1), encoding="utf-8")
    print(json.dumps({k: v for k, v in res.items() if k != "itens"}, ensure_ascii=False))
    for x in sorted(linhas, key=lambda x: (not x["no_instante"], not x["hoje"], x["nome"])):
        print(f"{x['id']} {x['classe']:8} {x['nome'][:24]:24} instante={'S' if x['no_instante'] else 'n'} hoje={'S' if x['hoje'] else 'n'} cfg={'S' if x['cfg'] else '-'}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
