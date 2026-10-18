#!/usr/bin/env python3
"""Corpus SEMEADO do VGP para Rust (onda 2, tarefa A).

O oraculo e o `cargo check` de verdade, num crate MINIMO e ISOLADO em `/tmp`, montado por este
script. Nao uso o worktree do touring porque (a) cada `cargo check` num monorepo custa segundos de
`Checking <crate>` que nao tem a ver com a mutacao, e (b) o DELTA exigido pela brief e "exatamente
1 diagnostico NOVO", o que so e exato quando o resto do programa e identico nas duas rotas. Um
crate de um arquivo faz o DELTA ser exato por construcao.

As quatro classes sao as do brief:
  nome      E0425  cannot find value `X` in this scope
  import    E0432/E0433  unresolved import / use of undeclared crate or module
  atributo  E0609/E0599  no field `X` on type / no method `X` found
  kwarg     E0560  struct has no field named `X` (na inicializacao)

Cada item carrega no manifesto o `disk_file` do ORIGINAL, que e o delta -- o mesmo esquema que
`scripts/juiz_motor_vgp.py` ja le.

Nao ha motor aqui: o script nao roda o VGP, so produz corpus + prova que o oraculo distingue o
mutado do original.

Uso:
  python3 corpus_rust.py --n 20 --selftest
  python3 corpus_rust.py --n 20 --saida ~/.claude/touring/vgp/corpus_rust
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import shutil
import subprocess
import sys
import tempfile
import time
from collections import Counter
from pathlib import Path

CLASSES = ("nome", "import", "atributo", "kwarg")
CODIGO_ESPERADO = {
    "nome": ("E0425",),
    "import": ("E0432", "E0433"),
    "atributo": ("E0609", "E0599"),
    "kwarg": ("E0560",),
}
# o codigo EXATO que a receita produz (o resto fica como os alternativos aceitos)
CODIGO_PRIMARIO = {
    "nome": "E0425",
    "import": "E0432",
    "atributo": "E0609",
    "kwarg": "E0560",
}

ORIGINAL = """\
// Base do oraculo: compila sem nenhum diagnostico.
pub struct Ponto {
    pub x: i64,
    pub y: i64,
}

pub fn area(p: &Ponto) -> i64 {
    p.x * p.y
}

pub fn desloca(p: &mut Ponto, dx: i64, dy: i64) {
    p.x += dx;
    p.y += dy;
}
"""


def nome_mutante(classe: str, semente: str) -> str:
    """Identificador unico e inequivoco -- e o que o E0425/E0609 vai nomear."""
    return f"vgp_{classe}_ausente_{semente}"


def receita(classe: str, semente: str) -> str:
    """A mutacao: o ORIGINAL mais uma linha que produz o codigo esperado.

    O mutante fica numa funcao `pub` para nao morrer por `dead_code`, e a linha mutada e a
    ULTIMA do arquivo -- assim a checagem de "o diagnostico esta na linha da mutacao" e exata.
    """
    n = nome_mutante(classe, semente)
    if classe == "nome":
        mut = f"pub fn mutado_{classe}() -> i64 {{ {n} }}"
    elif classe == "import":
        # MEDIDO em 09/10: `std::collections::{n}::new()` dentro do corpo da funcao da E0433
        # ("failed to resolve: use of undeclared crate or module"), e o primario do brief e
        # E0432 ("unresolved import"), que sai de um `use` no topo do modulo. Um item aceito com
        # o codigo errado nao serve: o juiz classifica por codigo, e o corpus tem de produzir o
        # codigo que a VL0 nomeia.
        mut = f"use std::collections::{n};\npub fn mutado_{classe}() -> i64 {{ 0 }}"
    elif classe == "atributo":
        mut = f"pub fn mutado_{classe}(p: &Ponto) -> i64 {{ p.{n} }}"
    elif classe == "kwarg":
        mut = f"pub fn mutado_{classe}() -> Ponto {{ Ponto {{ x: 1, y: 2, {n}: 3 }} }}"
    else:
        raise ValueError(f"classe sem receita: {classe}")
    return ORIGINAL + "\n" + mut + "\n"


def linha_da_mutacao(original: str, mutado: str) -> int:
    """1-based: a linha do `pub fn mutado_...` no arquivo MUTADO.

    O oraculo NAO sabe onde a mutacao esta -- ele so sabe onde o erro saiu. Sem comparar a linha
    com a mutacao, um item cujo `E0425` veio de outra linha passaria: o nome `vgp_nome_ausente_NNN`
    ja prova que o erro E o da mutacao (unico no arquivo), mas a checagem de linha e a que pega
    um recipe que mutou a linha errada.
    """
    n_original = len(original.splitlines())
    for i, l in enumerate(mutado.splitlines()[n_original:], start=n_original):
        if l.strip():
            return i + 1
    raise RuntimeError("a mutacao nao produziu nenhuma linha")


# ------------------------------------------------------------------------------- o oraculo
LINHA_CARGO = re.compile(r"^error\[(E\d+)\]:\s*(.*)$")
LINHA_ARQUIVO = re.compile(r"^\s*-->\s*(.+?):(\d+):(\d+)\s*$")


def monta_crate(raiz: Path, fonte: str) -> None:
    (raiz / "src").mkdir(parents=True, exist_ok=True)
    (raiz / "Cargo.toml").write_text(
        "[package]\n"
        'name = "vgp_oraculo"\n'
        'version = "0.0.0"\n'
        'edition = "2021"\n'
        "\n[lib]\n"
        'path = "src/lib.rs"\n'
        "\n[dependencies]\n",
        encoding="utf-8",
    )
    (raiz / "src" / "lib.rs").write_text(fonte, encoding="utf-8")


def roda_cargo_check(raiz: Path, fonte: str, timeout: int = 600) -> dict:
    """`cargo check` num crate de 1 arquivo. Devolve os diagnosticos JÁ normalizados.

    Normalizar e o que torna o DELTA exato: o `cargo` imprime o mesmo erro em varias linhas
    (`error[E0425]: ...`, o caret, a nota), e comparar o texto cru daria falso-positivo.
    """
    monta_crate(raiz, fonte)
    r = subprocess.run(
        ["cargo", "check", "--message-format=short", "--offline", "-q"],
        cwd=raiz,
        capture_output=True,
        text=True,
        timeout=timeout,
        check=False,
    )
    diags: list[dict] = []
    for ln in (r.stdout or "").splitlines() + (r.stderr or "").splitlines():
        m = re.match(
            r"^src/lib\.rs:(\d+):(\d+):\s*error\[(E\d+)\]:\s*(.*)$", ln.strip()
        )
        if m:
            diags.append(
                {
                    "lin": int(m.group(1)),
                    "col": int(m.group(2)),
                    "cod": m.group(3),
                    "msg": m.group(4).strip(),
                }
            )
    return {
        "rc": r.returncode,
        "diags": diags,
        "stdout": (r.stdout or "")[-3000:],
        "stderr": (r.stderr or "")[-3000:],
    }


def confere_que_o_cargo_rodou(res: dict) -> None:
    """0 diagnostico e o resultado AMBIGUO: "compilou limpo" ou "o cargo nem compilou".

    A onda 2 ja pagou esse preco tres vezes em JS. O `cargo` aborta ANTES de analisar quando o
    manifesto esta invalido ou a toolchain nao responde, e devolve rc != 0 com stderr que nao tem
    nenhuma linha `error[E...]`. Sem esta guarda, um crate quebrado leria como "base limpa".
    """
    if res["rc"] != 0 and not res["diags"]:
        raise RuntimeError(
            "o cargo check FALHOU sem produzir nenhum diagnostico de codigo "
            f"(rc={res['rc']}). stderr={res['stderr'][-400:]}"
        )
    if re.search(
        r"error: could not find `Cargo.toml`|no such file|failed to run `cargo`",
        res["stderr"],
    ):
        raise RuntimeError(f"o cargo nao chegou a analisar: {res['stderr'][-400:]}")


def delta(raiz: Path, original: str, mutado: str) -> dict:
    """O DELTA: o mesmo arquivo antes e depois, no mesmo crate."""
    antes = roda_cargo_check(raiz, original)
    confere_que_o_cargo_rodou(antes)
    confere_que_a_base_esta_limpa(antes)
    depois = roda_cargo_check(raiz, mutado)
    confere_que_o_cargo_rodou(depois)
    chave = lambda d: (d["cod"], d["lin"], d["msg"])
    conta_antes = Counter(chave(d) for d in antes["diags"])
    conta_depois = Counter(chave(d) for d in depois["diags"])
    novos = [list(k) + [n] for k, n in (conta_depois - conta_antes).items()]
    perdidos = [list(k) + [n] for k, n in (conta_antes - conta_depois).items()]
    return {"antes": antes, "depois": depois, "novos": novos, "perdidos": perdidos}


def confere_que_a_base_esta_limpa(antes: dict) -> None:
    if antes["diags"]:
        raise RuntimeError(
            f"a base NAO esta limpa ({len(antes['diags'])} diagnosticos): "
            f"{antes['diags'][:3]}"
        )


def aceita(
    classe: str, nome: str, res: dict, linha_esperada: int | None = None
) -> tuple[bool, str]:
    """O item so entra se o DELTA for EXATAMENTE 1, do codigo esperado, na linha certa, nomeando."""
    novos = res["novos"]
    if res["perdidos"]:
        return False, f"perdeu {len(res['perdidos'])} diagnostico(s)"
    if len(novos) != 1:
        return (
            False,
            f"delta = {len(novos)} diagnosticos (exigido 1): {[n[:3] for n in novos]}",
        )
    cod, lin, msg, _ = novos[0]
    aceitos = CODIGO_ESPERADO[classe]
    if cod not in aceitos:
        return False, f"codigo {cod} fora de {aceitos}"
    if cod != CODIGO_PRIMARIO[classe]:
        return (
            False,
            f"codigo {cod} e aceito mas nao e o primario ({CODIGO_PRIMARIO[classe]})",
        )
    if nome not in msg:
        return False, f"a mensagem nao nomeia `{nome}`: {msg[:90]}"
    if linha_esperada is not None and lin != linha_esperada:
        return False, f"o erro saiu na linha {lin}, a mutacao esta na {linha_esperada}"
    # MEDIDO em 10/10 (a coordenadora, e confirmado aqui): o corpus NATURAL grava a PRIMEIRA
    # LINHA do rustc COM o cabecalho — `error[E0560]: struct `Ponto` has no field named `…`` — e o
    # juiz classifica a partir dela. O gerador descartava o cabecalho no parse
    # (`LINHA_CARGO` separa codigo e texto) e devolvia so o texto; remontado aqui, e a linha
    # integra. Sem isso o `nao_classificado` do kwarg era DO GERADOR, nao do juiz: "has no field
    # named" nao casa com nenhum ramo do regex, enquanto `error[E0560]` casa.
    return True, f"error[{cod}]: {msg}"


# ----------------------------------------------------------------------------------- o item
def gera_item(raiz: Path, classe: str, semente: str, base_rel: str) -> dict:
    nome = nome_mutante(classe, semente)
    mutado = receita(classe, semente)
    linha = linha_da_mutacao(ORIGINAL, mutado)
    res = delta(raiz, ORIGINAL, mutado)
    ok, motivo = aceita(classe, nome, res, linha)
    ident = hashlib.sha256(f"{classe}|{semente}".encode()).hexdigest()[:12]
    return {
        "id": ident,
        "lang": "rust",
        "classe": classe,
        "rotulo": "positivo" if ok else "candidato",
        "origem": "semeado",
        "nome_esperado": nome,
        "codigo_esperado": CODIGO_PRIMARIO[classe],
        "path": f"{base_rel}/src/lib.rs",
        "project_root": base_rel,
        "reproduz": "true" if ok else "false",
        # MEDIDO/consentido em 10/10: o juiz (L179 de juiz_motor_vgp.py) faz
        # `for c in entrada.get("contexto") or []` e indexa `c["path"]`/`c["snap"]`. Iterar uma
        # STRING devolve caracteres, e indexar caractere por "path" e TypeError — nenhuma medicao
        # rodava sobre corpus_rust/. Este item e um crate de UM arquivo, entao nao ha modulos
        # irmaos para sobrepor: a lista correta e VAZIA. O motivo ja tem casa propria, em
        # `mensagem` (abaixo) e em `motivo`.
        "contexto": [],
        # A `mensagem` e o texto que o CLASSIFICADOR do juiz le (`classe_erro`), entao tem de
        # trazer a linha do rustc na forma `error[E0425]: …`. O `motivo` ja e essa linha (ver
        # `aceita`), com o caminho e o delta em volta para o olho humano.
        "mensagem": f"{motivo} (delta={CODIGO_PRIMARIO[classe]} em {base_rel}/src/lib.rs)",
        "snap": f"snap/{ident}.rs",
        "disk_file": f"base_rel_{ident}.rs",
        "mutado": mutado,
        "aceito": ok,
        "motivo": motivo,
        "delta": res["novos"],
    }


def escreve(d: Path, itens: list[dict], negativos: list[dict]) -> None:
    (d / "snap").mkdir(parents=True, exist_ok=True)
    (d / "base").mkdir(parents=True, exist_ok=True)
    linhas = []
    for it in itens + negativos:
        snap = d / it["snap"]
        snap.parent.mkdir(parents=True, exist_ok=True)
        snap.write_text(
            it.pop("mutado") if "mutado" in it else ORIGINAL, encoding="utf-8"
        )
        (d / "base" / f"{it['id']}.rs").write_text(ORIGINAL, encoding="utf-8")
        linhas.append(json.dumps(it, ensure_ascii=False, sort_keys=True))
    (d / "entradas.jsonl").write_text("\n".join(linhas) + "\n", encoding="utf-8")


# --------------------------------------------------------------------------------- negativo
def negativo(ident: str, base_rel: str) -> dict:
    return {
        "id": ident,
        "lang": "rust",
        "classe": "",
        "rotulo": "negativo",
        "origem": "semeado",
        "nome_esperado": "",
        "codigo_esperado": "",
        "path": f"{base_rel}/src/lib.rs",
        "project_root": base_rel,
        "reproduz": "false",
        # lista vazia, nao a string "[]" — ver a nota em `gera_item`: e o que o juiz itera.
        "contexto": [],
        "mensagem": "linha de base do oraculo: 0 erros",
        "snap": f"snap/{ident}.rs",
        "disk_file": f"base_rel_{ident}.rs",
    }


# --------------------------------------------------------------------------------- o gerador
def gera(n: int, saida: Path, raiz_tmp: Path | None = None) -> dict:
    """n positivos por classe. Cada item = 2 `cargo check` (antes e depois)."""
    tmp = Path(tempfile.mkdtemp(prefix="vgp-rust-", dir=str(raiz_tmp or "/tmp")))
    crate = tmp / "crate"
    crate.mkdir(parents=True, exist_ok=True)
    itens, rejeitados = [], []
    t0 = time.time()
    try:
        for classe in CLASSES:
            for i in range(n):
                it = gera_item(crate, classe, f"{i:03d}", "crate")
                (itens if it["aceito"] else rejeitados).append(it)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    return {
        "itens": itens,
        "rejeitados": rejeitados,
        "segundos": round(time.time() - t0, 1),
    }


def main() -> int:
    ap = argparse.ArgumentParser(
        description="Corpus semeado de Rust (tarefa A da onda 2)"
    )
    ap.add_argument("--n", type=int, default=20, help="positivos por classe")
    ap.add_argument(
        "--saida", type=Path, default=Path.home() / ".claude/touring/vgp/corpus_rust"
    )
    ap.add_argument("--negativos", type=int, default=100)
    ap.add_argument("--selftest", action="store_true")
    ap.add_argument("--confere-determinismo", action="store_true")
    a = ap.parse_args()

    if a.selftest:
        return selftest()

    if a.confere_determinismo:
        return confere_determinismo(a.n)

    r = gera(a.n, a.saida)
    itens = r["itens"]
    negs = [
        negativo(hashlib.sha256(f"neg|{i}".encode()).hexdigest()[:12], "crate")
        for i in range(a.negativos)
    ]
    a.saida.mkdir(parents=True, exist_ok=True)
    escreve(a.saida, itens, negs)
    por_classe = Counter(i["classe"] for i in itens)
    (a.saida / "relatorio.json").write_text(
        json.dumps(
            {
                "n_por_classe": dict(sorted(por_classe.items())),
                "alvo": a.n,
                "rejeitados": [r_["motivo"] for r_ in r["rejeitados"]],
                "segundos": r["segundos"],
                "negativos": len(negs),
            },
            ensure_ascii=False,
            indent=1,
        )
        + "\n",
        encoding="utf-8",
    )
    print(
        f"positivos: {len(itens)} | negativos: {len(negs)} | por classe: {dict(sorted(por_classe.items()))}"
    )
    print(f"rejeitados: {len(r['rejeitados'])} | tempo: {r['segundos']}s")
    for mot in Counter(r_["motivo"] for r_ in r["rejeitados"]).most_common(6):
        print(f"   rejeitado x{mot[1]}: {mot[0][:110]}")
    print("selftest: nao rodado (--selftest e separado)")
    return 0 if all(por_classe.get(c, 0) >= a.n for c in CLASSES) else 1


# ------------------------------------------------------------------------------- os gates
def _um_item(raiz: Path, classe: str, semente: str) -> dict:
    return gera_item(raiz, classe, semente, "crate")


def selftest() -> int:
    """Cada receita precisa discriminated o mutado do original, e cada guarda precisa morser.

    Um selftest que so passa quando tudo esta certo nao prova nada: cada caso abaixo e um
    MUTANTE do proprio script, e um mutante que sobrevive reprova.
    """
    tmp = Path(tempfile.mkdtemp(prefix="vgp-rust-st-", dir="/tmp"))
    crate = tmp / "crate"
    crate.mkdir(parents=True, exist_ok=True)
    cenarios = []
    t0 = time.time()
    try:
        # 1. as 4 classes produzem exatamente 1 diagnostico novo
        for classe in CLASSES:
            it = _um_item(crate, classe, "st0")
            cenarios.append(
                (
                    f"{classe}: delta de 1 com o codigo certo",
                    bool(it["aceito"]),
                    it["motivo"],
                )
            )

        # 2. MUTANTE: a receita sem mutacao nao produz nada -> o oraculo tem de reprovar
        original_apenas = roda_cargo_check(crate, ORIGINAL)
        mut = roda_cargo_check(crate, ORIGINAL)
        ok, motivo = aceita("nome", "vgp_nome_ausente_x", {"novos": [], "perdidos": []})
        cenarios.append(("mutante: receita vazia e reprovada", not ok, motivo))

        # 3. MUTANTE: um codigo de classe errada tem de ser rejeitado
        res_falso = {
            "novos": [["E0308", 20, "unmatched `arms` pattern", 1]],
            "perdidos": [],
        }
        ok_falso, mot_falso = aceita("nome", "vgp_nome_ausente_x", res_falso)
        cenarios.append(
            ("mutante: codigo de outra classe e rejeitado", not ok_falso, mot_falso)
        )

        # 4. MUTANTE: 2 diagnosticos novos tem de ser rejeitado (exigido exatamente 1)
        res_dois = {
            "novos": [
                ["E0425", 20, "cannot find value `x` in this scope", 1],
                ["E0425", 21, "cannot find value `y` in this scope", 1],
            ],
            "perdidos": [],
        }
        ok_dois, mot_dois = aceita("nome", "x", res_dois)
        cenarios.append(
            ("mutante: 2 diagnosticos novos sao rejeitados", not ok_dois, mot_dois)
        )

        # 5. MUTANTE: um codigo aceito mas nao primario tem de ser rejeitado
        res_sec = {
            "novos": [["E0433", 20, "failed to resolve: use of undeclared crate", 1]],
            "perdidos": [],
        }
        ok_sec, mot_sec = aceita("import", "x", res_sec)
        cenarios.append(
            ("mutante: codigo aceito mas nao primario e rejeitado", not ok_sec, mot_sec)
        )

        # 5b. MUTANTE: um diagnostico na LINHA ERRADA tem de ser rejeitado. Sem este cenario, a
        # checagem de linha (`linha_esperada`) pode estar desligada e o selftest continua verde.
        res_linha = {
            "novos": [["E0425", 99, "cannot find value `x` in this scope", 1]],
            "perdidos": [],
        }
        ok_linha, mot_linha = aceita("nome", "x", res_linha, linha_esperada=20)
        cenarios.append(
            ("mutante: erro na linha errada e rejeitado", not ok_linha, mot_linha)
        )

        # 6. a base limpa e o oraculo rodando: os dois estados do DELTA
        cenarios.append(
            (
                "base limpa antes de mutar",
                not original_apenas["diags"],
                str(len(original_apenas["diags"])),
            )
        )
        cenarios.append(
            (
                "cargo de fato rodou (sem erro sem codigo)",
                mut["rc"] in (0, 101),
                f"rc={mut['rc']}",
            )
        )

        # 7. MUTANTE da guarda: um cargo que falha SEM diagnostico tem de ser erro alto, nao "limpo"
        quebrado = {
            "rc": 1,
            "diags": [],
            "stderr": "error: could not find `Cargo.toml`",
            "stdout": "",
        }
        abortou = False
        try:
            confere_que_o_cargo_rodou(quebrado)
        except RuntimeError:
            abortou = True
        cenarios.append(
            (
                "mutante: cargo quebrado aborta em vez de ler como limpo",
                abortou,
                "guarda",
            )
        )

        # 8. MUTANTE da guarda: base suja tem de abortar
        sujo = {
            "rc": 1,
            "diags": [{"cod": "E0425", "lin": 1, "msg": "x", "col": 1}],
            "stderr": "",
            "stdout": "",
        }
        abortou2 = False
        try:
            confere_que_a_base_esta_limpa(sujo)
        except RuntimeError:
            abortou2 = True
        cenarios.append(("mutante: base suja aborta", abortou2, "guarda"))
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

    falhas = [f"{nome}: {mot}" for nome, ok, mot in cenarios if not ok]
    print(
        f"selftest: {len(cenarios) - len(falhas)}/{len(cenarios)} em {round(time.time() - t0, 1)}s"
    )
    for f in falhas:
        print(f"  FALHOU {f}")
    return 0 if not falhas else 1


def confere_determinismo(n: int) -> int:
    """Duas geracoes com a mesma semente tem de dar o MESMO conjunto de ids.

    Nao comparo os snaps byte a byte: o que tem de ser identico e o PAR (id, classe, codigo,
    linha), que e o que o juiz consome.
    """

    def uma() -> list:
        tmp = Path(tempfile.mkdtemp(prefix="vgp-rust-det-", dir="/tmp"))
        try:
            return sorted(
                (i["id"], i["classe"], i["codigo_esperado"], i["delta"][0][1])
                for i in gera(n, Path("/tmp/vgp-det-descarte"))["itens"]
            )
        finally:
            shutil.rmtree(tmp, ignore_errors=True)

    um, dois = uma(), uma()
    igual = um == dois
    print(f"determinismo 1a: {len(um)} itens")
    print(f"determinismo 2a: {len(dois)} itens")
    print(f"determinismo: {'IDENTICO' if igual else 'DIVERGE'}")
    if not igual:
        so_um = [x for x in um if x not in dois][:5]
        so_dois = [x for x in dois if x not in um][:5]
        print(f"  so na 1a: {so_um}")
        print(f"  so na 2a: {so_dois}")
    return 0 if igual and um else 1


if __name__ == "__main__":
    sys.exit(main())
