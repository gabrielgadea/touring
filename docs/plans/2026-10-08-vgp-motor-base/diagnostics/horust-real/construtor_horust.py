"""HORUST-REAL — construtor: lê a base (artefatos do cargo, dep-info, pacotes limpos), planeja as tentativas, executa as rodadas no oráculo
e escreve o corpus. As peças puras (plano, seleção, escrita) são determinísticas; só `executar` e `metadados` falam com o cargo.
"""

from __future__ import annotations

import dataclasses
import hashlib
import json
import re
import subprocess
from collections import Counter
from pathlib import Path

import operadores_rust as op
import oraculo_cargo_json as oc
import selecao_horust as sel

ORDEM_CLASSES = ("kwarg", "atributo", "import", "nome")  # as mais escassas primeiro: 1 injeção por arquivo, no total
ALVOS = {"lib": "lib", "rlib": "lib", "dylib": "lib", "cdylib": "lib", "staticlib": "lib", "proc-macro": "lib", "bin": "bins"}


# ------------------------------------------------------------------ base


def ler_dep_info(texto: str, raiz: Path) -> set[str]:
    """Os `.rs` de `crates/*/src/` que o rustc leu (dep-info do cargo), em caminho relativo à raiz do worktree."""
    achados: set[str] = set()
    prefixo = str(raiz).rstrip("/") + "/"
    for tok in re.findall(r"\S+\.rs", texto):
        rel = tok[len(prefixo) :] if tok.startswith(prefixo) else tok
        if re.fullmatch(r"crates/[^/]+/src/.+\.rs", rel):
            achados.add(rel)
    return achados


def alvo_de_kind(kinds: list[str]) -> str | None:
    return next((ALVOS[k] for k in kinds if k in ALVOS), None)


def _pacote_de(src_path: str, metadados: dict) -> str | None:
    donos = [(len(str(m["dir"])), n) for n, m in metadados.items() if src_path.startswith(str(m["dir"]) + "/")]
    return max(donos)[1] if donos else None


def _mensagens(jsonl: str):
    for linha in jsonl.splitlines():
        try:
            o = json.loads(linha)
        except json.JSONDecodeError:
            continue
        if isinstance(o, dict):
            yield o


def pacotes_limpos(jsonl: str, metadados: dict) -> tuple[set[str], dict[str, str]]:
    """(pacotes sem nenhum erro e com TODO alvo lib/bin compilado, motivo de cada um que ficou de fora)."""
    compilados: dict[str, set] = {n: set() for n in metadados}
    com_erro: set[str] = set()
    for o in _mensagens(jsonl):
        t = o.get("target") or {}
        pkg = _pacote_de(t.get("src_path", ""), metadados)
        if pkg is None:
            continue
        if o.get("reason") == "compiler-artifact" and (a := alvo_de_kind(t.get("kind", []))):
            compilados[pkg].add((a, t.get("name")))
        elif o.get("reason") == "compiler-message" and (o.get("message") or {}).get("level") == "error":
            com_erro.add(pkg)
    limpos, motivos = set(), {}
    for n, m in metadados.items():
        esperado = {(alvo_de_kind([k]), nome) for k, nome in m["alvos"] if alvo_de_kind([k])}
        if n in com_erro:
            motivos[n] = "erro"
        elif not esperado or not esperado <= compilados[n]:
            motivos[n] = "alvo_nao_compilado"
        else:
            limpos.add(n)
    return limpos, motivos


def arquivos_por_alvo(jsonl: str, metadados: dict, raiz: Path) -> dict[tuple, set[str]]:
    """(pacote, 'lib'|'bins') → os `.rs` que o rustc leu naquele alvo (dep-info ao lado do .rmeta). Falha alto se o `.d` não existe."""
    saida: dict[tuple, set[str]] = {}
    for o in _mensagens(jsonl):
        t = o.get("target") or {}
        if o.get("reason") != "compiler-artifact" or not (alvo := alvo_de_kind(t.get("kind", []))):
            continue
        pkg = _pacote_de(t.get("src_path", ""), metadados)
        if pkg is None:
            continue
        rmeta = Path(next((f for f in o.get("filenames", []) if f.endswith(".rmeta")), None) or o["filenames"][0])
        saida.setdefault((pkg, alvo), set()).update(ler_dep_info(achar_dep_info(rmeta).read_text(), raiz))
    return saida


def achar_dep_info(rmeta: Path) -> Path:
    """O `.d` do cargo check mora ao lado do .rmeta, mas sem o prefixo `lib` do nome (`beta-<hash>.d`). Falha alto se não existe."""
    candidatos = [rmeta.with_suffix(".d"), rmeta.with_name(rmeta.stem.removeprefix("lib") + ".d")]
    achado = next((c for c in candidatos if c.is_file()), None)
    if achado is None:
        raise FileNotFoundError(f"dep-info não encontrado para {rmeta}: tentei {[str(c) for c in candidatos]}")
    return achado


def metadados(raiz: Path, via_slot: bool = False) -> dict:
    """`cargo metadata --no-deps` (não compila): pacote → {dir, alvos[(kind, nome)]}."""
    cmd = ["cargo", "metadata", "--no-deps", "--format-version", "1", "--offline"]
    if via_slot:
        cmd = [oc.HWSLOT, "--class", "rust", "--wait", "7200", "--label", "horust-real-metadata", "--", *cmd]
    r = subprocess.run(cmd, cwd=raiz, capture_output=True, text=True, check=True, timeout=600)
    pacotes = json.loads(r.stdout)["packages"]
    membros = {p["name"] for p in pacotes}
    return {
        p["name"]: {
            "dir": Path(p["manifest_path"]).parent, "alvos": [(t["kind"][0], t["name"]) for t in p["targets"]],
            "deps": {d["name"] for d in p["dependencies"] if d.get("kind") != "dev"} & membros,
        }
        for p in pacotes
    }


def mapa_camadas(meta: dict, nos: set[tuple]) -> dict[tuple, int]:
    """(pacote, alvo) → número do grupo que cabe num `cargo check` só. Libs: antecadeias do grafo de dependência do workspace; bins não
    dependem uns dos outros e vão juntos."""
    deps = {n: set(m["deps"]) for n, m in meta.items()}
    libs = sel.antecadeias([p for p, a in nos if a == "lib"], deps)
    saida = {(p, "lib"): i for i, g in enumerate(libs) for p in g}
    saida.update({(p, a): 0 for p, a in nos if a != "lib"})
    return saida


# ------------------------------------------------------------------ plano


@dataclasses.dataclass(frozen=True)
class Arquivo:
    rel: str
    pkg: str
    alvo: str
    texto: str


@dataclasses.dataclass(frozen=True)
class Tentativa:
    inj: oc.Injecao
    pkg: str
    alvo: str
    estilo: str  # near_miss | plausivel | remocao
    nome_novo: str
    nome_orig: str
    coluna: int


def _chave_ordem(rel: str, s: op.Sitio) -> str:
    return f"{rel}|{s.linha}|{s.inicio}"


def _montar(arq: Arquivo, s: op.Sitio, estilo: str, semente: str, existentes: set[str]) -> Tentativa | None:
    if s.operador == "kwarg_remove":
        novo, nome_msg = "", s.tipo
    else:
        chave = f"{semente}|{_chave_ordem(arq.rel, s)}|{estilo}"
        novo = op.variante(s.nome, op.tipo_do_nome(s), estilo, chave, existentes)
        if novo is None:
            return None
        nome_msg = novo
    texto_novo = op.mutar(arq.texto, s, novo)
    classe, codigo = op.CODIGO[s.operador]
    ident = sel.id_injecao(arq.rel, classe, s.linha, novo or f"-{s.nome}", semente)
    inj = oc.Injecao(ident, arq.rel, classe, s.operador, codigo, s.linha, nome_msg, arq.texto, texto_novo, arq.pkg, s.aceitos)
    return Tentativa(inj, arq.pkg, arq.alvo, "remocao" if s.operador == "kwarg_remove" else estilo, novo, s.nome, s.inicio)


def _tentar(arq: Arquivo, sitios: list[op.Sitio], estilo: str, semente: str, tokens_pkg: set[str]) -> Tentativa | None:
    outro = "plausivel" if estilo == "near_miss" else "near_miss"
    for s in sorted(sitios, key=lambda x: sel._h(semente, _chave_ordem(arq.rel, x)))[:6]:
        for e in (estilo, outro):
            if t := _montar(arq, s, e, semente, tokens_pkg):
                return t
    return None


def planejar(
    arquivos: dict[str, Arquivo], tokens_por_pkg: dict[str, set[str]], semente: str, usados: set[str], totais: dict[str, int],
    operadores: dict[str, dict[str, int]] | None = None, filtro_sitio=None,
) -> list[Tentativa]:
    """Tentativas deterministas: por classe (mais escassa primeiro), por operador (cota pré-registrada, ou `operadores[classe]` numa rodada de
    reparo), arquivos em round-robin entre pacotes; no máximo 1 injeção por arquivo no total. `filtro_sitio` restringe os sítios (reparo).
    Arquivo sem sítio utilizável é pulado, e a cota segue até esgotar o pool."""
    usados = set(usados)
    aceita = filtro_sitio or (lambda s: True)
    cache = {rel: [s for s in op.sitios(a.texto) if aceita(s)] for rel, a in arquivos.items() if rel not in usados}
    saida: list[Tentativa] = []
    for classe in ORDEM_CLASSES:
        if operadores and classe in operadores:
            cotas = operadores[classe]
        elif classe in totais:
            cotas = sel.cotas(classe, totais[classe])
        else:
            continue
        for operador, cota in cotas.items():
            grupos: dict[str, list[str]] = {}
            for rel, ss in cache.items():
                if rel not in usados and any(s.operador == operador for s in ss):
                    grupos.setdefault(arquivos[rel].pkg, []).append(rel)
            ordem = sel.round_robin(grupos, sum(len(v) for v in grupos.values()), f"{semente}|{classe}|{operador}")
            feitas = 0
            for rel in ordem:
                if feitas >= cota:
                    break
                a = arquivos[rel]
                t = _tentar(a, [s for s in cache[rel] if s.operador == operador], sel.estilo_do(feitas), semente, tokens_por_pkg.get(a.pkg, set()))
                if t:
                    saida.append(t)
                    usados.add(rel)
                    feitas += 1
    return saida


def grupos_de_rodada(tentativas: list[Tentativa], camadas: dict[tuple, int] | None = None) -> dict[tuple, list[Tentativa]]:
    """(grupo, alvo, fase) → tentativas. Sem `camadas`, um grupo por pacote; com elas, pacotes independentes dividem o mesmo cargo."""
    g: dict[tuple, list[Tentativa]] = {}
    for t in tentativas:
        grupo = camadas[(t.pkg, t.alvo)] if camadas else t.pkg
        g.setdefault((grupo, t.alvo, sel.fase(t.inj.classe)), []).append(t)
    return g


def executar(
    tentativas: list[Tentativa], raiz: Path, target: Path, via_slot: bool = True, offline: bool = False, diags_out: dict | None = None,
    camadas: dict[tuple, int] | None = None, observados_out: dict | None = None,
) -> dict[str, str]:
    """id → '' (válido) ou o motivo do descarte. `diags_out`, se dado, recebe id → o diagnóstico do rustc das injeções válidas."""
    resultado: dict[str, str] = {}
    for (_grupo, alvo, _fase), lote in grupos_de_rodada(tentativas, camadas).items():
        base = oc.runner_cargo(raiz, target, alvo, via_slot=via_slot, offline=offline)
        vistos: dict[str, list[oc.Diag]] = {}

        def runner(injs, base=base, vistos=vistos):
            diags = base(injs)
            for i in injs:
                vistos[i.id] = [d for d in diags if d.arq == i.arq]
            return diags

        res = oc.rodada([t.inj for t in lote], runner)
        resultado.update(res)
        if diags_out is not None:
            diags_out.update({i: vistos[i][0] for i, m in res.items() if m == "" and vistos.get(i)})
        if observados_out is not None:
            observados_out.update({i: vistos.get(i, []) for i in res})
    return resultado


# ------------------------------------------------------------------ corpus


@dataclasses.dataclass(frozen=True)
class Positivo:
    id: str
    rel: str
    pkg: str
    classe: str
    operador: str
    codigo: str
    linha: int
    coluna: int
    nome_esperado: str
    nome_orig: str
    estilo: str
    mensagem: str
    texto_original: str
    texto_novo: str


@dataclasses.dataclass(frozen=True)
class Negativo:
    id: str
    rel: str
    pkg: str
    texto: str
    pareado_com: str = ""  # estrato cfg: o id do positivo cujo original este negativo é


def _sha(t: str) -> str:
    return hashlib.sha256(t.encode("utf-8")).hexdigest()


def escrever_corpus(saida: Path, raiz: Path, positivos: list[Positivo], negativos: list[Negativo], meta: dict, estrato: str = "horust-real") -> None:
    """Corpus no formato do juiz (régua estrita: sem `estado_no_instante/`). Recusa diretório existente e não vazio."""
    if saida.exists() and any(saida.iterdir()):
        raise FileExistsError(f"{saida} já existe e não está vazio")
    (saida / "snap").mkdir(parents=True, exist_ok=True)
    (saida / "base").mkdir(parents=True, exist_ok=True)
    comum = {"lang": "rust", "origem": "semeado", "estrato": estrato, "project_root": str(raiz), "contexto": "[]"}
    linhas = []
    for p in sorted(positivos, key=lambda x: x.id):
        (saida / "snap" / f"{p.id}.rs").write_text(p.texto_novo, encoding="utf-8")
        (saida / "base" / f"{p.id}.rs").write_text(p.texto_original, encoding="utf-8")
        linhas.append({
            **comum, "id": p.id, "rotulo": "positivo", "reproduz": "true", "classe": p.classe, "operador": p.operador, "codigo_esperado": p.codigo,
            "codigo": p.codigo, "linha": p.linha, "coluna": p.coluna, "nome_esperado": p.nome_esperado, "nome_original": p.nome_orig,
            "estilo": p.estilo, "crate": p.pkg, "path": f"{raiz}/{p.rel}", "snap": f"snap/{p.id}.rs", "snap_sha256": _sha(p.texto_novo),
            "base": f"base/{p.id}.rs", "sha_original": _sha(p.texto_original), "mensagem": f"error[{p.codigo}]: {p.mensagem}",
        })
    for n in sorted(negativos, key=lambda x: x.id):
        (saida / "snap" / f"{n.id}.rs").write_text(n.texto, encoding="utf-8")
        linhas.append({
            **comum, "id": n.id, "rotulo": "negativo", "reproduz": "false", "classe": "", "nome_esperado": "", "codigo_esperado": "", "crate": n.pkg,
            "path": f"{raiz}/{n.rel}", "snap": f"snap/{n.id}.rs", "snap_sha256": _sha(n.texto), "mensagem": "base: cargo check limpo",
            **({"pareado_com": n.pareado_com} if n.pareado_com else {}),
        })
    (saida / "entradas.jsonl").write_text("\n".join(json.dumps(x, ensure_ascii=False, sort_keys=True) for x in linhas) + "\n", encoding="utf-8")
    (saida / "holdout_meta.json").write_text(json.dumps(meta, ensure_ascii=False, indent=1, sort_keys=True), encoding="utf-8")


def contar_motivos(resultado: dict[str, str]) -> Counter:
    return Counter(m for m in resultado.values() if m)
