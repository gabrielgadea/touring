#!/usr/bin/env python3
"""Corpus SEMEADO de Rust no PROJETO REAL (onda 2, tarefa A, estrato `real`).

Decisao da coordenadora (09/10): o estrato `real` usa o `touring-wt-semeado` (88c7cc8b, limpo) e
as receitas tem de exercitar a RESOLUCAO DE MODULOS DO PROJETO, nunca `std`. Um crate sintetico de
um arquivo (o estrato `sanidade`) mede o caso facil: `std::collections::x` nunca e ambiguo. Aqui o
que importa e o `crate::`, o `super::`, o `self::` e o caminho de um `pub use` de reexport.

Crates: `touring-identity` (modulos error/registry/schema/types, com `pub use error::{Error,
Result}` e `pub use types::*`) e `touring-pyright` (modulos client/pool/pyright).

O oraculo e o `cargo check` de verdade, no worktree, com `CARGO_TARGET_DIR` dedicado. O DELTA e
exato por construcao: a base tem de sair com ZERO diagnostico no crate alvo, e o mutado com
EXATAMENTE 1 diagnostico novo, do codigo esperado, na linha da mutacao. O que nao fechar sai com
contagem -- nunca vira positivo por suposicao.

A restauracao e VERIFICADA POR SHA: os bytes originais sao salvos, e no fim o sha tem de voltar.
Nunca `git checkout --` (destrutivo, REGRA #11). O worktree tem de estar limpo E no sha esperado
antes de comecar, senao o script recusa.

Este script nao roda o motor VGP: so produz corpus e prova que o oraculo distingue o mutado do
original.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
import time
from collections import Counter
from pathlib import Path

_RAIZ = Path(__file__).resolve().parent
if str(_RAIZ) not in sys.path:
    sys.path.insert(0, str(_RAIZ))
import chama_oraculo

WORKTREE = Path("/home/gabrielgadea/projects/touring-wt-semeado")
TARGET = Path("/home/gabrielgadea/projects/touring/target/semeado-rust")
SHA_ESPERADO = "88c7cc8b"
CRATES = ("touring-identity", "touring-pyright")
CLASSES = (
    "import",
    "import_reexport",
    "import_e0433",
    "nome",
    "atributo",
    "atributo_metodo",
    "kwarg",
)
CODIGO_PRIMARIO = {
    "import": "E0432",
    "import_reexport": "E0432",
    "import_e0433": "E0433",
    "nome": "E0425",
    "atributo": "E0609",
    "atributo_metodo": "E0599",
    "kwarg": "E0560",
}
RECEITAS = {
    "touring-identity": {
        "import": "use crate::types::{n};\npub fn mutado_{n}() {{ let _ = {n}; }}\n",
        "import_reexport": "use crate::error::{n};\npub fn mutado_{n}() {{ let _ = {n}; }}\n",
        "import_e0433": "pub fn mutado_{n}() -> usize {{ crate::error::{n}::conta().len() }}\n",
        "nome": "pub fn mutado_{n}() -> i64 {{ {n} }}\n",
        "atributo": "pub fn mutado_{n}(t: &crate::types::EntityId) -> i64 {{ t.{n} }}\n",
        "atributo_metodo": "pub fn mutado_{n}(t: &crate::types::EntityId) -> i64 {{ t.{n}() }}\n",
        "kwarg": (
            "pub fn mutado_{n}() -> crate::types::EntityId {{\n"
            "    crate::types::EntityId {{ {n}: 0 }}\n"
            "}}\n"
        ),
    },
    "touring-pyright": {
        "import": "use crate::pool::{n};\npub fn mutado_{n}() {{ let _ = {n}; }}\n",
        "import_reexport": "use crate::client::{n};\npub fn mutado_{n}() {{ let _ = {n}; }}\n",
        "import_e0433": "pub fn mutado_{n}() -> usize {{ crate::pool::{n}::TAMANHO }}\n",
        "nome": "pub fn mutado_{n}() -> i64 {{ {n} }}\n",
        "atributo": "pub fn mutado_{n}(p: &crate::pool::PyrightPool) -> usize {{ p.{n} }}\n",
        "atributo_metodo": (
            "pub fn mutado_{n}(p: &crate::pool::PyrightPool) -> usize {{ p.{n}() }}\n"
        ),
        "kwarg": (
            "pub fn mutado_{n}() -> crate::pool::PoolConfig {{\n"
            "    crate::pool::PoolConfig {{ {n}: 0 }}\n"
            "}}\n"
        ),
    },
}


def sha_bytes(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def worktree_limpo() -> None:
    """O worktree tem de estar limpo E no sha esperado. Se nao, o script recusa."""
    p = subprocess.run(
        ["git", "-C", str(WORKTREE), "status", "--porcelain"],
        capture_output=True,
        text=True,
        timeout=300,
        check=False,
    )
    sujo = [l for l in (p.stdout or "").splitlines() if l.strip()]
    if sujo:
        raise RuntimeError(
            f"o worktree {WORKTREE} tem alteracao: {sujo[:3]}. Restaure por "
            "`git show HEAD:<path>` antes de rodar."
        )
    h = subprocess.run(
        ["git", "-C", str(WORKTREE), "rev-parse", "--short", "HEAD"],
        capture_output=True,
        text=True,
        timeout=120,
        check=False,
    ).stdout.strip()
    if h != SHA_ESPERADO:
        raise RuntimeError(f"o worktree esta em {h}, e o esperado e {SHA_ESPERADO}")


def roda_check(crate: str, timeout: int = 1200) -> tuple[int, list, str]:
    """`cargo check` do crate. Devolve (rc, diagnosticos normalizados, stderr)."""
    env = {**os.environ, "CARGO_TARGET_DIR": str(TARGET)}
    # MEDIDO em 09/10: o `cargo` escreve o diagnostico no **stderr** e o stdout fica VAZIO com
    # `--message-format=short`. O contrato `cargo` declara `stream="stderr"`, entao um `cargo` que
    # nao produziu nada e ERRO NOMEADO em vez de `diags=[]` com `rc=101` lido como «a mutacao nao
    # introduziu erro» — que e a leitura ERRADA, porque o erro estava la o tempo todo, no outro
    # stream.
    r = chama_oraculo.chama_oraculo(
        "cargo",
        ["cargo", "check", "-p", crate, "--message-format=short"],
        cwd=WORKTREE,
        timeout=timeout,
        env=env,
    )
    diags = []
    # MEDIDO em 09/10: o `cargo` escreve os diagnosticos no **stderr**, nao no stdout (o stdout
    # fica VAZIO com `--message-format=short`). Um parser que so le o stdout ve `diags = []` com
    # `rc = 101` -- e o gerador le isso como "a mutacao nao introduziu erro", que e a leitura
    # ERRADA: o erro estava la o tempo todo, no outro stream. Mesmo desenho do bug do `tsc` com
    # os dois formatos de saida.
    for ln in (r["stdout"] + "\n" + r["stderr"]).splitlines():
        m = re.match(r"^(.+?):(\d+):(\d+): error\[?(E\d+)\]?: (.*)$", ln.strip())
        if m:
            diags.append(
                {
                    "arq": m.group(1).split("/")[-1],
                    "lin": int(m.group(2)),
                    "cod": m.group(3),
                    "msg": m.group(4).strip(),
                }
            )
    return r["rc"], diags, r["stderr"][-1500:]


def base_limpa(crate: str) -> list:
    """A base tem de estar LIMPA. `diags` vazio por erro de configuracao do cargo nao e base limpa:
    absence of signal tem DUAS causas, e a segunda e o compilador nao rodou."""
    rc, diags, err = roda_check(crate)
    if rc != 0 and not diags:
        raise RuntimeError(
            f"[{crate}] o cargo nao produziu diagnostico nenhum (rc={rc}): {err[-400:]}"
        )
    if any(d["cod"] in ("E0463", "E0464") for d in diags):
        raise RuntimeError(f"[{crate}] o cargo nao conseguiu compilar: {err[-400:]}")
    return diags


def injeta_e_mede(crate: str, rel: str, antes: str, mutado: str) -> dict:
    """Base limpa, mutado, DELTA exato. Restauracao por SHA no fim."""
    alvo = WORKTREE / rel
    original = alvo.read_bytes()
    sha_original = sha_bytes(original)
    try:
        alvo.write_bytes(antes.encode("utf-8"))
        base = base_limpa(crate)
        alvo.write_bytes(mutado.encode("utf-8"))
        _rc, depois, _err = roda_check(crate)

        def chave(d: dict) -> tuple:
            return (d["cod"], d["arq"], d["lin"], d["msg"][:90])

        cb = Counter(chave(d) for d in base)
        cd = Counter(chave(d) for d in depois)
        return {
            "base": base,
            "novos": [list(k) + [n] for k, n in (cd - cb).items()],
            "perdidos": [list(k) + [n] for k, n in (cb - cd).items()],
        }
    finally:
        alvo.write_bytes(original)
        if sha_bytes(alvo.read_bytes()) != sha_original:
            raise RuntimeError(
                f"restauracao FALHOU em {alvo}: sha divergiu do original"
            )


def motivo_da_recusa(classe: str, nome: str, res: dict, linha_esperada: int) -> str:
    """Vazio = aceita. Qualquer coisa = o motivo, contavel."""
    if res["perdidos"]:
        return f"perdeu {len(res['perdidos'])} da base"
    if len(res["novos"]) != 1:
        return f"delta de {len(res['novos'])} (exigido 1)"
    cod, _arq, lin, msg, _n = res["novos"][0]
    if cod != CODIGO_PRIMARIO[classe]:
        return f"codigo {cod} (esperado {CODIGO_PRIMARIO[classe]})"
    if nome not in msg:
        return "a mensagem nao nomeia o simbolo"
    if lin != linha_esperada:
        return f"saida na linha {lin} e a mutacao esta na {linha_esperada}"
    return ""


def um_item(crate: str, classe: str, i: int) -> tuple[dict | None, str]:
    rel = f"crates/{crate}/src/lib.rs"
    antes = (WORKTREE / rel).read_text(errors="replace")
    nome = f"vgp_{classe}_ausente_{i:03d}"
    mutado = antes + RECEITAS[crate][classe].format(n=nome)
    linha = len(antes.splitlines()) + 1
    res = injeta_e_mede(crate, rel, antes, mutado)
    motivo = motivo_da_recusa(classe, nome, res, linha)
    ident = hashlib.sha256(f"{crate}|{classe}|{i}".encode()).hexdigest()[:12]
    if motivo:
        return None, f"{crate}/{classe}: {motivo}"
    return {
        "id": ident,
        "lang": "rust",
        "estrato": "real",
        "origem": "semeado",
        "classe": classe,
        "codigo_esperado": CODIGO_PRIMARIO[classe],
        "rotulo": "positivo",
        "reproduz": "true",
        "crate": crate,
        "path": rel,
        "project_root": str(WORKTREE),
        "nome_esperado": nome,
        "contexto": json.dumps({"linha": linha, "codigo": CODIGO_PRIMARIO[classe]}),
        "mensagem": f"delta={CODIGO_PRIMARIO[classe]} em {crate}/src/lib.rs",
        "snap": f"snap/{ident}.rs",
        "disk_file": f"base_rel_{ident}.rs",
        "mutado": mutado,
    }, ""


def escrever(saida: Path, itens: list, stats: Counter, segundos: float) -> None:
    (saida / "snap").mkdir(parents=True, exist_ok=True)
    (saida / "base").mkdir(parents=True, exist_ok=True)
    linhas = []
    for it in itens:
        (saida / it["snap"]).write_text(it.pop("mutado"), encoding="utf-8")
        (saida / "base" / f"{it['id']}.rs").write_text(
            (WORKTREE / it["path"]).read_text(errors="replace"), encoding="utf-8"
        )
        linhas.append(json.dumps(it, ensure_ascii=False, sort_keys=True))
    for crate in CRATES:
        ident = hashlib.sha256(f"neg|{crate}".encode()).hexdigest()[:12]
        rel = f"crates/{crate}/src/lib.rs"
        (saida / f"snap/{ident}.rs").write_text(
            (WORKTREE / rel).read_text(errors="replace"), encoding="utf-8"
        )
        (saida / f"base/{ident}.rs").write_text(
            (WORKTREE / rel).read_text(errors="replace"), encoding="utf-8"
        )
        linhas.append(
            json.dumps(
                {
                    "id": ident,
                    "lang": "rust",
                    "estrato": "real",
                    "origem": "semeado",
                    "classe": "",
                    "rotulo": "negativo",
                    "reproduz": "false",
                    "crate": crate,
                    "path": rel,
                    "project_root": str(WORKTREE),
                    "mensagem": "base do crate: cargo check limpo",
                    "snap": f"snap/{ident}.rs",
                    "disk_file": f"base_rel_{ident}.rs",
                    "nome_esperado": "",
                    "codigo_esperado": "",
                    "contexto": "[]",
                },
                ensure_ascii=False,
                sort_keys=True,
            )
        )
    (saida / "entradas.jsonl").write_text("\n".join(linhas) + "\n", encoding="utf-8")
    por_classe = Counter(i["classe"] for i in itens)
    (saida / "relatorio.json").write_text(
        json.dumps(
            {
                "estrato": "real",
                "n_por_classe": dict(sorted(por_classe.items())),
                "n_total": len(itens),
                "n_por_crate": dict(sorted(Counter(i["crate"] for i in itens).items())),
                "recusas": dict(sorted(stats.items())),
                "segundos": segundos,
                "worktree": str(WORKTREE),
                "sha": SHA_ESPERADO,
                "crates": list(CRATES),
            },
            ensure_ascii=False,
            indent=1,
        )
        + "\n",
        encoding="utf-8",
    )
    print(f"positivos: {len(itens)} | segundos: {segundos}")
    print(f"por classe: {dict(sorted(por_classe.items()))}")
    for k, v in stats.most_common(10):
        print(f"   RECUSA {k}: {v}")


def main() -> int:
    ap = argparse.ArgumentParser(description="Corpus semeado de Rust no projeto REAL")
    ap.add_argument("--n", type=int, default=20, help="positivos por classe")
    ap.add_argument(
        "--saida",
        type=Path,
        default=Path.home() / ".claude/touring/vgp/corpus_rust_real",
    )
    ap.add_argument("--selftest", action="store_true")
    ap.add_argument("--confere-determinismo", action="store_true")
    a = ap.parse_args()

    if a.selftest:
        return selftest()
    if a.confere_determinismo:
        return confere_determinismo()

    t0 = time.time()
    worktree_limpo()
    itens: list = []
    stats: Counter = Counter()
    for crate in CRATES:
        for classe in CLASSES:
            for i in range(a.n):
                it, motivo = um_item(crate, classe, i)
                if it is not None:
                    itens.append(it)
                elif motivo:
                    stats[motivo.split(": ", 1)[-1]] += 1
    escrever(a.saida, itens, stats, round(time.time() - t0, 1))
    return 0 if len(itens) >= a.n else 1


def selftest() -> int:
    """Cada receita produz EXATAMENTE 1 diagnostico novo no projeto real, e o worktree volta limpo."""
    worktree_limpo()
    cenarios = []
    t0 = time.time()
    for crate in CRATES:
        for classe in CLASSES:
            it, motivo = um_item(crate, classe, 0)
            cenarios.append(
                (f"{crate}/{classe}", it is not None, motivo or "delta exato")
            )
    # MUTANTE: sem mutacao o delta tem de ser 0 (o gate rejeita)
    rel = "crates/touring-identity/src/lib.rs"
    antes = (WORKTREE / rel).read_text(errors="replace")
    res = injeta_e_mede("touring-identity", rel, antes, antes)
    cenarios.append(
        (
            "mutante: sem mutacao o delta e 0 (reprovado)",
            len(res["novos"]) == 0,
            f"{len(res['novos'])} novo(s)",
        )
    )
    # e o worktree tem de ter voltado limpo depois de todas as injecoes
    worktree_limpo()
    cenarios.append(
        ("o worktree volta limpo depois de todas as injecoes", True, "git status vazio")
    )
    falhas = [f"{n}: {m}" for n, ok, m in cenarios if not ok]
    print(
        f"selftest: {len(cenarios) - len(falhas)}/{len(cenarios)} em {round(time.time() - t0, 1)}s"
    )
    for f in falhas:
        print(f"  FALHOU {f}")
    return 0 if not falhas else 1


def confere_determinismo() -> int:
    """Duas medicoes da MESMA receita tem de dar o mesmo (id, classe, codigo, linha)."""

    def uma() -> list:
        out = []
        for crate in CRATES:
            for classe in CLASSES:
                it, _m = um_item(crate, classe, 0)
                out.append(
                    (it["id"], classe, it["codigo_esperado"])
                    if it
                    else (classe, "REJEITADO", "")
                )
        return sorted(out)

    um, dois = uma(), uma()
    igual = um == dois
    print(
        f"determinismo 1a: {len(um)} | 2a: {len(dois)} | "
        f"{'IDENTICO' if igual else 'DIVERGE'}"
    )
    if not igual:
        print(f"  so na 1a: {[x for x in um if x not in dois][:3]}")
        print(f"  so na 2a: {[x for x in dois if x not in um][:3]}")
    return 0 if igual and um else 1


if __name__ == "__main__":
    sys.exit(main())
