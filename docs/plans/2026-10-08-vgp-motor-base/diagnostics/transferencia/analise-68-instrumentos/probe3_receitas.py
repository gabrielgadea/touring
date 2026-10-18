"""Sonda 3 — as RECEITAS de mutação, provadas uma a uma contra o tsc do projeto.

Nada aqui vai para o corpus: é a etapa que decide se um operador é viável. Uma receita só
vive se o tsc acusar O erro esperado no arquivo mutado, e NENHUM outro erro.
"""

import json
import shutil
import subprocess
from pathlib import Path

FRONTEND = Path.home() / "projects/analise/frontend"
# A sombra fica no WORKSPACE: de dentro do sandbox do `touring run`, /tmp é escrita-negada
# (medido: "cp: cannot create directory ... Permission denied"). Eu removo a pasta na mão.
SOMBRA = Path.home() / "projects/analise/.vgp-sombra-frontend"

TS = next(
    p
    for p in FRONTEND.rglob("*.ts")
    if "node_modules" not in p.parts and len(p.read_text(errors="replace")) > 200
)
TSX = next(
    p
    for p in FRONTEND.rglob("*.tsx")
    if "node_modules" not in p.parts and len(p.read_text(errors="replace")) > 400
)

RECEITAS_TS = {
    "import": "import { vgpNomeAusente7f3a } from './__vgp_mod_ausente_7f3a';",
    "nome": "vgpNomeAusente7f3a;",
    "atributo": "export const _vgpProbe7f3a = new Date().vgpPropAusente7f3a;",
    "kwarg": "export const _vgpProbe7f3a: { vgpA: number } = { vgpA: 1, vgpPropAusente7f3a: 2 };",
}
RECEITAS_TSX = {
    "componente": "export const _vgpProbe7f3a = () => <VgpCompAusente7f3a />;",
    "prop": "export const _vgpProbe7f3a = () => <div vgpPropAusente7f3a={1} />;",
}


def sombra():
    """Cópia por hardlink: instantânea e barata. Sobrescrever = remove + write (nunca escrevo
    no inode original, que é a garantia de que o fonte fica intacto)."""
    if SOMBRA.exists():
        shutil.rmtree(SOMBRA)
    subprocess.run(["cp", "-al", str(FRONTEND), str(SOMBRA)], check=True)
    return SOMBRA


def escreve(destino: Path, texto: str) -> None:
    destino.unlink(missing_ok=True)  # quebra o hardlink ANTES de escrever
    destino.write_text(texto, encoding="utf-8")


def roda_tsc(sombra_dir: Path) -> list:
    r = subprocess.run(
        [str(sombra_dir / "node_modules/.bin/tsc"), "--noEmit", "-p", "."],
        cwd=sombra_dir,
        capture_output=True,
        text=True,
        timeout=900,
    )
    return [x for x in r.stdout.splitlines() if "error TS" in x]


def main() -> int:
    rel_ts = str(TS.relative_to(FRONTEND))
    rel_tsx = str(TSX.relative_to(FRONTEND))
    print("ts  :", rel_ts)
    print("tsx :", rel_tsx)
    # tsconfig é JSONC (aceita comentários) — `json.loads` não lê; mostro o texto.
    print("tsconfig.json:")
    print((FRONTEND / "tsconfig.json").read_text(encoding="utf-8")[:900])

    s = sombra()
    originais = {
        rel_ts: (SOMBRA / rel_ts).read_text(encoding="utf-8", errors="replace"),
        rel_tsx: (SOMBRA / rel_tsx).read_text(encoding="utf-8", errors="replace"),
    }

    base = roda_tsc(s)
    print("\n=== linha de base da sombra (tem de ser 0) ===")
    print(base or "0 erros")

    resultado = {}
    for rel, receitas in ((rel_ts, RECEITAS_TS), (rel_tsx, RECEITAS_TSX)):
        print("\n##########", rel, "##########")
        for nome, linha in receitas.items():
            escreve(s / rel, originais[rel] + "\n" + linha + "\n")
            erros = roda_tsc(s)
            escreve(s / rel, originais[rel])
            do_alvo = [e for e in erros if rel in e]
            outros = [e for e in erros if rel not in e]
            chave = rel + ":" + nome
            so_esperado = bool(do_alvo) and not outros
            resultado[chave] = {
                "receita": linha,
                "erros_no_alvo": do_alvo,
                "n_outros": len(outros),
                "so_o_esperado": so_esperado,
            }
            print("\n[" + nome + "]", "1 erro so, o esperado" if so_esperado else "FALHOU")
            for e in do_alvo:
                print("   ", e)
            for e in outros[:3]:
                print("    !outro:", e)

    print("\n=== o original continua intacto? (controle) ===")
    intacto = True
    for rel, orig in originais.items():
        igual = (s / rel).read_text(encoding="utf-8", errors="replace") == orig
        intacto = intacto and igual
        print("  ", rel, "intacto" if igual else "ALTERADO")
    real = FRONTEND / rel_ts
    print("  fonte real intocado:", real.read_text(encoding="utf-8", errors="replace") == originais[rel_ts])

    print("\n=== resumo ===")
    for k, v in resultado.items():
        print(f"  {'ok  ' if v['so_o_esperado'] else 'FALHA'} {k}")
    return 0 if all(v["so_o_esperado"] for v in resultado.values()) else 1


if __name__ == "__main__":
    raise SystemExit(main())