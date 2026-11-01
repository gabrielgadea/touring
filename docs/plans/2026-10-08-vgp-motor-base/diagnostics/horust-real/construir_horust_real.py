"""HORUST-REAL — orquestra a construção: inventario → rodar (≤3 rodadas) → reconfirmar → montar. Estado em `estado/` (resultados.jsonl é
append-only e retomável: ids são deterministas). NADA aqui roda o motor VGP. Pré-registro: PRE-REGISTRO.md (sha em pre-registro.hora).

Uso: python3 -I -B construir_horust_real.py inventario | rodar <k> | lotes <k> <n> (dentro de um hw-slot) | revalidar [n] (idem) | reconfirmar | montar
"""

from __future__ import annotations

import collections
import dataclasses
import hashlib
import json
import itertools
import math
import os
import subprocess
import sys
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import construtor_horust as ch  # noqa: E402
import operadores_rust as op  # noqa: E402
import oraculo_cargo_json as oc  # noqa: E402
import selecao_horust as sel  # noqa: E402

RAIZ = Path("/home/gabrielgadea/projects/touring-wt-horust-real")
TARGET = Path("/home/gabrielgadea/projects/touring/target/horust-real")
HEAD_ESPERADO = "c118aa524358d42b9bb2aa40b4794b2c5919a5ae"
SEMENTE = "horust-real-20261010"
TENTATIVAS_POR_CLASSE = 72
MIN_POR_CLASSE, NEG_MIN, NEG_ALVO, TETO = 20, 320, 340, 0.25
CORPUS = Path.home() / ".claude/touring/vgp/corpus_horust_real_20261010"
ESTADO = HERE / "estado"
VGP = Path.home() / ".claude/touring/vgp"
BASE_CHECK = HERE / "base_check.jsonl"
PREFIXOS_REPO = ("/home/gabrielgadea/projects/touring/", "/home/gabrielgadea/projects/touring-wt-semeado/")


def semente_da_rodada(k: int) -> str:
    return SEMENTE if k == 1 else f"{SEMENTE}-r{k}"


def git(*args: str) -> str:
    return subprocess.run(["git", "-C", str(RAIZ), *args], capture_output=True, text=True, check=True, timeout=300).stdout.strip()


def worktree_limpo() -> None:
    sujo = [x for x in git("status", "--porcelain").splitlines() if x.strip()]
    if sujo or git("rev-parse", "HEAD") != HEAD_ESPERADO:
        raise RuntimeError(f"worktree {RAIZ} fora do estado esperado: HEAD {git('rev-parse', 'HEAD')[:12]}, alterações {sujo[:3]}")


def sha_da_hora(chave: str) -> str:
    """Um campo de `pre-registro.hora` (gravado, sha256, tamanho_original, sha256_com_adendos…)."""
    return dict(x.split(": ", 1) for x in (HERE / "pre-registro.hora").read_text().splitlines())[chave]


def sha_arquivo(p: Path) -> str:
    return hashlib.sha256(p.read_bytes()).hexdigest()


# ------------------------------------------------------------------ inventário


def dev_e_controle() -> tuple[set[str], set[str]]:
    dev: set[str] = set()
    for linha in (VGP / "corpus_rust_natural/entradas.jsonl").read_text().splitlines():
        if linha.strip():
            p = json.loads(linha)["path"]
            dev.update(p[len(pre) :] for pre in PREFIXOS_REPO if p.startswith(pre))
    ls = git("ls-tree", "-r", "--name-only", "88c7cc8b", "--", "crates/touring-code", "crates/touring-identity", "crates/touring-pyright")
    return dev, {x for x in ls.splitlines() if x.endswith(".rs")}


def montar_inventario() -> tuple[dict[str, ch.Arquivo], dict]:
    meta = ch.metadados(RAIZ)
    jsonl = BASE_CHECK.read_text()
    limpos, fora = ch.pacotes_limpos(jsonl, meta)
    por_alvo = {k: v for k, v in ch.arquivos_por_alvo(jsonl, meta, RAIZ).items() if k[0] in limpos}
    ambiguos = sel.arquivos_ambiguos(por_alvo)
    dev, controle = dev_e_controle()
    contagem: collections.Counter = collections.Counter()
    arquivos: dict[str, ch.Arquivo] = {}
    for (pkg, alvo), fs in sorted(por_alvo.items()):
        pref = f"crates/{meta[pkg]['dir'].name}/"
        for rel in sorted(fs):
            if not rel.startswith(pref):
                contagem["fora_do_pacote"] += 1
            elif rel in ambiguos:
                contagem["ambiguo_mais_de_um_alvo"] += 1
            elif m := sel.motivo_exclusao(rel, dev, controle):
                contagem[f"excluido_{m}"] += 1
            else:
                try:
                    texto = (RAIZ / rel).read_bytes().decode("utf-8")
                except UnicodeDecodeError:
                    contagem["nao_utf8"] += 1
                    continue
                if sel.tamanho_elegivel(texto.count("\n") + 1, len(texto.encode("utf-8"))):
                    arquivos[rel] = ch.Arquivo(rel, pkg, alvo, texto)
                else:
                    contagem["tamanho"] += 1
    info = {
        "pacotes_limpos": sorted(limpos), "pacotes_fora": fora, "arquivos_compilados": sum(len(v) for v in por_alvo.values()),
        "descartes_do_inventario": dict(contagem), "elegiveis": len(arquivos),
        "elegiveis_por_pacote": dict(collections.Counter(a.pkg for a in arquivos.values()).most_common()),
        "vazamento": {"dev": sorted(sel.vazamento(set(arquivos), dev)), "controle": sorted(sel.vazamento(set(arquivos), controle)),
                      "fixture_vrs": sorted(r for r in arquivos if r.startswith(sel.PREFIXOS_FIXTURE_VRS))},
        "dev_n": len(dev), "controle_n": len(controle),
    }
    return arquivos, info


def tokens_por_pacote(arquivos: dict[str, ch.Arquivo]) -> dict[str, set[str]]:
    meta = ch.metadados(RAIZ)
    out: dict[str, set[str]] = {}
    for pkg in {a.pkg for a in arquivos.values()}:
        t: set[str] = set()
        for f in meta[pkg]["dir"].rglob("*.rs"):
            t |= op.tokens(f.read_text(encoding="utf-8", errors="replace"))
        out[pkg] = t
    return out


# ------------------------------------------------------------------ registros


def registro(t: ch.Tentativa, rodada: int, motivo: str, diag: oc.Diag | None, observado: list | None = None) -> dict:
    return {
        "rodada": rodada, "id": t.inj.id, "rel": t.inj.arq, "pkg": t.pkg, "alvo": t.alvo, "classe": t.inj.classe, "operador": t.inj.operador,
        "codigo": t.inj.codigo, "codigo_rustc": diag.codigo if diag else "", "linha": t.inj.linha, "coluna": t.coluna, "estilo": t.estilo, "nome_orig": t.nome_orig, "nome_novo": t.nome_novo,
        "nome_msg": t.inj.nome, "tipo": "", "motivo": motivo, "msg": diag.msg if diag else "", "coluna_rustc": diag.coluna if diag else 0,
        "sha_original": hashlib.sha256(t.inj.texto_original.encode()).hexdigest(), "sha_novo": hashlib.sha256(t.inj.texto_novo.encode()).hexdigest(),
        "observado": [f"{d.codigo}@{d.linha}: {d.msg[:90]}" for d in (observado or [])][:4],
    }


def ler_registros(arquivo: str = "resultados.jsonl") -> list[dict]:
    """Um registro por id; o ÚLTIMO vence (uma revalidação grava por cima sem apagar a história do arquivo append-only)."""
    p = ESTADO / arquivo
    todos = [json.loads(x) for x in p.read_text().splitlines() if x.strip()] if p.is_file() else []
    return list({r["id"]: r for r in todos}.values())


def regra_do_registro(r: dict) -> str:
    """'original' = registro da rodada 1 como o §3/§4 o definiam (gravado antes do campo `codigo_rustc` existir); 'adendo1' = o que o Adendo 1
    do PRE-REGISTRO acrescentou (a rodada 2 e os registros regravados pela revalidação, todos com `codigo_rustc`)."""
    return "original" if r["rodada"] == 1 and "codigo_rustc" not in r else "adendo1"


def ids_por_regra(registros: list[dict], ids: set[str]) -> dict[str, list[str]]:
    saida: dict[str, list[str]] = {"original": [], "adendo1": []}
    for r in registros:
        if r["id"] in ids:
            saida[regra_do_registro(r)].append(r["id"])
    return {k: sorted(v) for k, v in saida.items()}


def reconstruir(r: dict, texto: str) -> oc.Injecao:
    """A injeção a partir do registro e do texto original (a prova de que o registro basta para refazer a mutação)."""
    nome_msg = r["nome_msg"]
    s = op.Sitio(r["operador"], r["linha"], r["coluna"], r["coluna"] + len(r["nome_orig"]), r["nome_orig"], nome_msg if r["codigo"] == "E0063" else "")
    novo = op.mutar(texto, s, r["nome_novo"])
    return oc.Injecao(r["id"], r["rel"], r["classe"], r["operador"], r["codigo"], r["linha"], nome_msg, texto, novo, r["pkg"], op.ACEITOS.get(r["operador"], ()))


# ------------------------------------------------------------------ comandos


def cmd_inventario() -> int:
    worktree_limpo()
    ESTADO.mkdir(exist_ok=True)
    _, info = montar_inventario()
    (ESTADO / "inventario.json").write_text(json.dumps(info, ensure_ascii=False, indent=1, sort_keys=True))
    print(json.dumps({k: info[k] for k in ("arquivos_compilados", "descartes_do_inventario", "elegiveis", "pacotes_fora")}, ensure_ascii=False))
    print("vazamento:", {k: len(v) for k, v in info["vazamento"].items()})
    print("elegiveis por pacote:", info["elegiveis_por_pacote"])
    return 0 if not any(info["vazamento"].values()) else 1


# Rodada 2 = REPARO de dois operadores que a rodada 1 mediu quebrados (desvios D1/D2 do relatório): `nome_tipo` (o rustc deste worktree
# diz E0425 para tipo ausente) e `import_use` (mutar o NOME importado cascateia erros no resto do arquivo; o módulo, o penúltimo segmento, não).
REPARO_R2 = {"nome": {"nome_tipo": 40}, "import": {"import_use": 60}}


def reparo_da_rodada(k: int) -> dict | None:
    return REPARO_R2 if k == 2 else None


def filtro_da_rodada(k: int):
    return (lambda s: s.operador != "import_use" or s.posicao == "penultimo") if k == 2 else None


def classes_pendentes(k: int, registros: list[dict]) -> dict[str, int]:
    """Rodada 1: todas as classes. Da 3 em diante: só as com menos de 20 válidos nas rodadas ANTERIORES (o parcial da rodada em curso não
    entra, senão uma retomada mudaria o plano). A rodada 2 é de reparo (`REPARO_R2`)."""
    if k == 1:
        return dict.fromkeys(sel.PESOS, TENTATIVAS_POR_CLASSE)
    validos = collections.Counter(r["classe"] for r in registros if r["motivo"] == "" and r["rodada"] < k)
    return {c: TENTATIVAS_POR_CLASSE for c in sel.PESOS if validos[c] < MIN_POR_CLASSE}


def plano_da_rodada(k: int, arquivos: dict[str, ch.Arquivo], tokens: dict[str, set[str]], feitos: list[dict]) -> list[ch.Tentativa]:
    """O plano determinístico da rodada k, menos o que a própria rodada já gravou (retomável sem mudar nada do que falta)."""
    usados = {r["rel"] for r in feitos if r["rodada"] < k}
    totais = {} if reparo_da_rodada(k) else classes_pendentes(k, feitos)
    plano = ch.planejar(arquivos, tokens, semente_da_rodada(k), usados, totais, operadores=reparo_da_rodada(k), filtro_sitio=filtro_da_rodada(k))
    ja = {r["id"] for r in feitos if r["rodada"] == k}
    return [t for t in plano if t.inj.id not in ja]


def exigir_slot() -> None:
    """O modo `lotes` roda cargo SEM o hw-slot por chamada: só vale se este processo já está dentro de um (HW_SLOT_HELD)."""
    if not os.environ.get("HW_SLOT_HELD"):
        raise RuntimeError("`lotes` só roda dentro do hw-slot (HW_SLOT_HELD ausente): hw-slot --class rust --wait 7200 -- python3 … lotes <k> <n>")


def primeiros_lotes(grupos: dict, n: int | None):
    """Os primeiros `n` lotes, na ordem do plano (n vazio ou 0 = todos)."""
    return itertools.islice(grupos.items(), n or None)


def executar_plano(plano: list[ch.Tentativa], rodada_de, n_lotes: int | None, via_slot: bool, arquivo: str = "resultados.jsonl", gravar=None) -> None:
    """Roda os primeiros `n_lotes` lotes (camadas de dependência) do plano e grava cada lote assim que ele termina."""
    ESTADO.mkdir(exist_ok=True)
    camadas = ch.mapa_camadas(ch.metadados(RAIZ), {(t.pkg, t.alvo) for t in plano})
    grupos = ch.grupos_de_rodada(plano, camadas)
    print(f"  {len(grupos)} lotes (camadas); rodando {n_lotes or 'todos'}", flush=True)
    for (cam, alvo, fase), grupo in primeiros_lotes(grupos, n_lotes):
        diags: dict = {}
        observados: dict = {}
        inicio = time.time()
        res = ch.executar(grupo, RAIZ, TARGET, via_slot=via_slot, diags_out=diags, camadas=camadas, observados_out=observados)
        fazer = gravar or registro
        with (ESTADO / arquivo).open("a") as f:
            for t in grupo:
                f.write(json.dumps(fazer(t, rodada_de(t), res[t.inj.id], diags.get(t.inj.id), observados.get(t.inj.id)), ensure_ascii=False, sort_keys=True) + "\n")
        worktree_limpo()
        print(f"  lote {cam}/{alvo}/{fase} ({len({t.pkg for t in grupo})} pacotes): {len(grupo)} tentativas, {sum(m == '' for m in res.values())} válidas, "
              f"descartes {dict(ch.contar_motivos(res))}, {round(time.time() - inicio)} s", flush=True)


def cmd_rodar(k: int, n_lotes: int | None = None, via_slot: bool = True) -> int:
    """Roda a rodada k (ou os primeiros n lotes pendentes). via_slot=False = já dentro de um hw-slot (um slot para vários lotes)."""
    if not via_slot:
        exigir_slot()
    worktree_limpo()
    arquivos, _ = montar_inventario()
    plano = plano_da_rodada(k, arquivos, tokens_por_pacote(arquivos), ler_registros())
    print(f"rodada {k}: {len(plano)} tentativas pendentes", flush=True)
    executar_plano(plano, lambda t: k, n_lotes, via_slot)
    return 0


def tentativa_do_registro(r: dict) -> ch.Tentativa:
    texto = (RAIZ / r["rel"]).read_bytes().decode("utf-8")
    return ch.Tentativa(reconstruir(r, texto), r["pkg"], r["alvo"], r["estilo"], r["nome_novo"], r["nome_orig"], r["coluna"])


def candidatos_a_revalidar(registros: list[dict]) -> list[dict]:
    """`nome_tipo` descartado só por código: o rustc deste worktree diz E0425 para tipo ausente (D1)."""
    return [r for r in registros if r["operador"] == "nome_tipo" and r["motivo"] == "codigo_inesperado"]


def cmd_revalidar(n_lotes: int | None = None) -> int:
    """Refaz, com o critério de código corrigido, as MESMAS injeções de `nome_tipo` que a rodada 1 descartou só pelo código (sem sorteio novo)."""
    exigir_slot()
    worktree_limpo()
    alvo = candidatos_a_revalidar(ler_registros())
    print(f"revalidar: {len(alvo)} injeções de nome_tipo descartadas por código", flush=True)
    rodada = {r["id"]: r["rodada"] for r in alvo}
    executar_plano([tentativa_do_registro(r) for r in alvo], lambda t: rodada[t.inj.id], n_lotes, via_slot=False)
    return 0


def amostra(validos: list[dict], classe: str) -> list[dict]:
    n = max(3, math.ceil(0.10 * len(validos)))
    ordem = sel.ordem([r["id"] for r in validos], SEMENTE + "|reconfirma|" + classe)[:n]
    return [r for r in validos if r["id"] in ordem]


def cmd_reconfirmar() -> int:
    worktree_limpo()
    registros = ler_registros()
    saida = []
    for classe in sel.PESOS:
        validos = [r for r in registros if r["classe"] == classe and r["motivo"] == ""]
        for r in amostra(validos, classe):
            res = ch.executar([tentativa_do_registro(r)], RAIZ, TARGET, via_slot=True)
            saida.append({"id": r["id"], "classe": classe, "sozinha": res[r["id"]] or "ok"})
            worktree_limpo()
            print(f"  {classe} {r['id']}: {saida[-1]['sozinha']}", flush=True)
    (ESTADO / "reconfirmacao.json").write_text(json.dumps({"amostra": saida, "discordancias": [s for s in saida if s["sozinha"] != "ok"]}, indent=1))
    return 0 if all(s["sozinha"] == "ok" for s in saida) else 1


def escolher_ids(registros: list[dict]) -> set[str]:
    """Por classe: os ids válidos com o teto de 25% por crate (o maior N viável)."""
    escolhidos: set[str] = set()
    for classe in sel.PESOS:
        por_crate: dict[str, list[str]] = {}
        for r in registros:
            if r["classe"] == classe and r["motivo"] == "":
                por_crate.setdefault(r["pkg"], []).append(r["id"])
        escolhidos |= set(sel.selecao_com_teto(por_crate, TETO, SEMENTE + "|" + classe))
    return escolhidos


def construir_positivos(registros: list[dict], ids: set[str]) -> list[ch.Positivo]:
    saida = []
    for r in registros:
        if r["id"] in ids:
            texto = (RAIZ / r["rel"]).read_bytes().decode("utf-8")
            inj = reconstruir(r, texto)
            saida.append(ch.Positivo(r["id"], r["rel"], r["pkg"], r["classe"], r["operador"], r.get("codigo_rustc") or r["codigo"], r["linha"], r["coluna_rustc"],
                                     r["nome_msg"], r["nome_orig"], r["estilo"], r["msg"], texto, inj.texto_novo))
    return saida


def escolher_negativos(arquivos: dict[str, ch.Arquivo], tentados: set[str]) -> list[ch.Negativo]:
    """Arquivos intactos, disjuntos de TODO arquivo tentado, em round-robin entre pacotes."""
    grupos: dict[str, list[str]] = {}
    for rel, a in arquivos.items():
        if rel not in tentados:
            grupos.setdefault(a.pkg, []).append(rel)
    rels = sel.round_robin(grupos, NEG_ALVO, SEMENTE + "|negativos")
    return [ch.Negativo(hashlib.sha256(f"neg|{rel}|{SEMENTE}".encode()).hexdigest()[:12], rel, arquivos[rel].pkg, arquivos[rel].texto) for rel in rels]


def montar_meta(positivos: list[ch.Positivo], negativos: list[ch.Negativo], registros: list[dict], inv: dict) -> dict:
    regras = ids_por_regra(registros, {p.id for p in positivos})
    n_por_classe = collections.Counter(p.classe for p in positivos)
    neg_por_pkg = collections.Counter(n.pkg for n in negativos)
    return {
        "nome": "HORUST-REAL", "semente": SEMENTE, "worktree": str(RAIZ), "worktree_head": HEAD_ESPERADO, "worktree_tree": git("rev-parse", "HEAD^{tree}"),
        "pre_registro_sha256": sha_arquivo(HERE / "PRE-REGISTRO.md"), "pre_registro_sha256_original": sha_da_hora("sha256"),
        "ids_regra_original": regras["original"], "ids_regra_adendo": regras["adendo1"],
        "por_classe_regra_original": dict(collections.Counter(p.classe for p in positivos if p.id in set(regras["original"]))),
        "positivos": len(positivos), "negativos": len(negativos), "por_classe": dict(n_por_classe),
        "por_codigo": dict(collections.Counter(p.codigo for p in positivos)), "por_operador": dict(collections.Counter(p.operador for p in positivos)),
        "por_estilo": dict(collections.Counter(p.estilo for p in positivos)),
        "por_crate": {c: dict(collections.Counter(p.pkg for p in positivos if p.classe == c)) for c in n_por_classe},
        "negativos_por_crate": dict(neg_por_pkg), "teto": TETO, "minimo_por_classe": MIN_POR_CLASSE,
        "classes_abaixo_de_20": {c: n for c, n in n_por_classe.items() if n < MIN_POR_CLASSE},
        "descartes_por_motivo": dict(collections.Counter(r["motivo"] for r in registros if r["motivo"])),
        "tentativas": len(registros), "rodadas": sorted({r["rodada"] for r in registros}),
        "elegiveis": inv["elegiveis"], "descartes_do_inventario": inv["descartes_do_inventario"],
        "teto_negativos_observado": max(neg_por_pkg.values()) / len(negativos) if negativos else None,
        "limitacao": "recall sobre erro INJETADO em contexto real, não a distribuição natural; régua estrita, sem disk_file",
    }


def cmd_montar() -> int:
    worktree_limpo()
    arquivos, inv = montar_inventario()
    registros = ler_registros()
    positivos = construir_positivos(registros, escolher_ids(registros))
    negativos = escolher_negativos(arquivos, {r["rel"] for r in registros})
    meta = montar_meta(positivos, negativos, registros, inv)
    ch.escrever_corpus(CORPUS, RAIZ, positivos, negativos, meta)
    print(json.dumps({k: meta[k] for k in ("positivos", "negativos", "por_classe", "classes_abaixo_de_20", "teto_negativos_observado")}, ensure_ascii=False))
    return 0 if not meta["classes_abaixo_de_20"] and len(negativos) >= NEG_MIN else 1


def main() -> int:
    cmd = sys.argv[1] if len(sys.argv) > 1 else ""
    if cmd == "inventario":
        return cmd_inventario()
    if cmd == "rodar" and len(sys.argv) == 3:
        return cmd_rodar(int(sys.argv[2]))
    if cmd == "lotes" and len(sys.argv) == 4:
        return cmd_rodar(int(sys.argv[2]), int(sys.argv[3]), via_slot=False)
    if cmd == "revalidar":
        return cmd_revalidar(int(sys.argv[2]) if len(sys.argv) > 2 else None)
    if cmd == "reconfirmar":
        return cmd_reconfirmar()
    if cmd == "montar":
        return cmd_montar()
    print(__doc__)
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
