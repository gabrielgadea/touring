"""Mutantes do DETERMINISMO (P0 + pedido 2 da coordenadora): aplica cada mutação, roda os testes vgp::, registra rc + testes que falharam,
restaura o arquivo (cp do backup, nunca git). Uso: python3 mutantes.py <worktree> <saida.json> [M...]
Cada mutante e (arquivo, antigo, novo[, antigo2, novo2...]) sobre o texto do arquivo."""
import json
import os
import re
import shutil
import subprocess
import sys

WT = sys.argv[1]
SAIDA = sys.argv[2]
SO = {a for a in sys.argv[3:] if not a.startswith('--')}
V = f"{WT}/crates/touring-code/src/vgp"
ENV = dict(os.environ, CARGO_TARGET_DIR="/home/gabrielgadea/projects/touring/target/vgp-test")

MUTANTES = {
    # sem ordenar: o `take(8)` volta a cortar o conjunto na ordem aleatoria da instancia
    "D1-SEM-ORDENACAO": [
        ("checks.rs",
         "    v.sort_by(|a, b| {\n        a.starts_with('_')\n            .cmp(&b.starts_with('_'))\n            .then_with(|| a.cmp(b))\n    });\n",
         ""),
    ],
    # ordenar DEPOIS do corte: ordena 8 nomes ja sorteados — o conjunto continua variando
    "D2-CORTA-ANTES-DE-ORDENAR": [
        ("checks.rs",
         "    let mut v: Vec<&String> = membros.iter().collect();",
         "    let mut v: Vec<&String> = membros.iter().take(REAIS_MAX).collect();"),
    ],
    # sugestao sem desempate total: empate a distancia 1 cai na ordem de entrada
    "D3-SUGESTAO-DESEMPATE-PELA-ENTRADA": [
        ("checks.rs",
         ".min_by(|(da, ra), (db, rb)| da.cmp(db).then_with(|| ra.cmp(rb)))",
         ".min_by_key(|(d, _)| *d)"),
    ],
    # desempate pelo nome ANTES da distancia: `aaaa` (distancia 2) vence `asa` (distancia 1)
    "D4-SUGESTAO-NOME-ANTES-DA-DISTANCIA": [
        ("checks.rs",
         ".min_by(|(da, ra), (db, rb)| da.cmp(db).then_with(|| ra.cmp(rb)))",
         ".min_by(|(da, ra), (db, rb)| ra.cmp(rb).then_with(|| da.cmp(db)))"),
    ],
    # alfabetica pura: os dunders (`__init__`) entram antes dos nomes publicos no corte de 8
    "D5-SEM-PUBLICOS-PRIMEIRO": [
        ("checks.rs",
         "        a.starts_with('_')\n            .cmp(&b.starts_with('_'))\n            .then_with(|| a.cmp(b))\n",
         "        a.cmp(b)\n"),
    ],
    # ordem descendente
    "D6-ORDEM-INVERTIDA": [
        ("checks.rs",
         "            .then_with(|| a.cmp(b))\n    });\n    v.into_iter()",
         "            .then_with(|| b.cmp(a))\n    });\n    v.into_iter()"),
    ],
    # sem corte: a lista de `reais` cresce alem do teto
    "D7-SEM-CORTE": [
        ("checks.rs",
         "    v.into_iter().take(REAIS_MAX).cloned().collect()",
         "    v.into_iter().cloned().collect()"),
    ],
    # params do dataclass voltam a sair do HashSet
    "D8-PARAMS-DO-CONJUNTO": [
        ("modules.rs",
         "        ci.init.params = ci.field_order.clone();",
         "        ci.init.params = ci.fields.iter().cloned().collect();"),
    ],
    # a ordem de declaracao nunca e registrada: params vazios
    "D9-ORDEM-DE-CAMPO-NAO-REGISTRADA": [
        ("modules.rs",
         "                    if ci.fields.insert(name.clone()) {\n                        ci.field_order.push(name);\n                    }",
         "                    ci.fields.insert(name);"),
    ],
    # reals_of_module sem sort: o atributo de modulo volta a cortar o conjunto sem ordem
    "D10-REALS-DE-MODULO-SEM-ORDEM": [
        ("checks.rs",
         "    v.sort();\n    v.truncate(REAIS_MAX);",
         "    v.truncate(REAIS_MAX);"),
    ],
}


def rodar():
    r = subprocess.run(
        ["cargo", "test", "-p", "touring-code", "--lib", "vgp::"],
        cwd=WT, env=ENV, capture_output=True, text=True)
    txt = r.stdout + r.stderr
    falhos = sorted(set(re.findall(r"^test (\S+) \.\.\. FAILED", txt, re.M)))
    resumo = re.findall(r"test result: .*", txt)
    return r.returncode, falhos, resumo, txt[-1500:] if r.returncode and not falhos else ""


def checar():
    """Valida que toda ancora existe no codigo atual (sem compilar)."""
    ruim = 0
    for nome, edits in MUTANTES.items():
        for arq, antigo, _ in edits:
            if antigo not in open(f"{V}/{arq}").read():
                print("ANCORA AUSENTE", nome, arq, repr(antigo[:70]))
                ruim += 1
    print("ancoras:", "ok" if not ruim else f"{ruim} ausentes")


def main():
    res = {}
    if os.path.exists(SAIDA):
        res = json.load(open(SAIDA))
    for nome, edits in MUTANTES.items():
        if SO and nome not in SO:
            continue
        arqs = {}
        try:
            for arq, antigo, novo in edits:
                p = f"{V}/{arq}"
                if p not in arqs:
                    arqs[p] = open(p).read()
                    shutil.copy(p, p + ".orig_mut")
                txt = open(p).read()
                if antigo not in txt:
                    raise RuntimeError(f"ancora ausente em {arq}: {antigo[:60]!r}")
                open(p, "w").write(txt.replace(antigo, novo, 1))
            rc, falhos, resumo, cauda = rodar()
            res[nome] = {"rc": rc, "morto": rc != 0, "falhos": falhos, "resumo": resumo, "cauda": cauda}
        except Exception as e:  # noqa: BLE001
            res[nome] = {"erro": str(e)}
        finally:
            for p, orig in arqs.items():
                open(p, "w").write(orig)
                os.remove(p + ".orig_mut")
        json.dump(res, open(SAIDA, "w"), indent=1, ensure_ascii=False)
        print(nome, "->", {k: v for k, v in res[nome].items() if k != "cauda"}, flush=True)


if __name__ == "__main__":
    (checar if "--checar" in sys.argv else main)()
