"""HORUST-REAL / estrato cfg — driver: plano, lotes, reconfirmação e montagem do corpus SEPARADO (adendo `PRE-REGISTRO-ADENDO-CFG.md`).

Reusa o oráculo, o executor de lotes e o escritor de corpus do HORUST-REAL principal; o que é só do estrato: o operador (`operador_cfg.py`),
o estado (`estado/resultados_cfg.jsonl`), a seleção (80 tentativas, 20-40 válidos, teto 25%) e os negativos PAREADOS.

Uso: python3 -I -B construir_cfg.py plano | lotes <n> (dentro de um hw-slot) | reconfirmar | montar
O plano só roda DEPOIS do `montar` do corpus principal (a disjunção é contra o `entradas.jsonl` final dele).
"""

from __future__ import annotations

import collections
import hashlib
import json
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import construir_horust_real as cr  # noqa: E402
import construtor_horust as ch  # noqa: E402
import operador_cfg as oc_  # noqa: E402
import operadores_rust as op  # noqa: E402
import oraculo_cargo_json as oc  # noqa: E402
import selecao_horust as sel  # noqa: E402

SEMENTE_CFG = "horust-real-cfg-20261010"
TENTATIVAS_CFG, MIN_CFG, MAX_CFG, TETO = 80, 20, 40, cr.TETO
CORPUS_CFG = cr.VGP / "corpus_horust_real_cfg_20261010"
ARQUIVO = "resultados_cfg.jsonl"
PLANO_JSON = cr.ESTADO / "plano_cfg.json"
RECONF_JSON = cr.ESTADO / "reconfirmacao_cfg.json"


# ------------------------------------------------------------------ plano


def palavras(texto: str) -> frozenset[str]:
    """Identificadores do texto MASCARADO (comentário e literal não contam)."""
    return frozenset(re.findall(r"[A-Za-z_][A-Za-z0-9_]*", op.mascarar(texto)))


def contagem_de(arquivos: dict[str, ch.Arquivo]) -> dict[str, collections.Counter]:
    """pacote → palavra → em quantos arquivos do pacote ela aparece."""
    out: dict[str, collections.Counter] = {}
    for a in arquivos.values():
        out.setdefault(a.pkg, collections.Counter()).update(palavras(a.texto))
    return out


def planejar_cfg(
    arquivos: dict[str, ch.Arquivo], excluidos: set[str], semente: str, n: int, contagem: dict[str, collections.Counter] | None = None,
) -> list[ch.Tentativa]:
    """Até `n` tentativas, 1 por arquivo, round-robin entre pacotes. `contagem` (pacote → palavra → nº de arquivos) pode cobrir MAIS arquivos
    do que `arquivos` (o driver passa todos os `.rs` do pacote); uma palavra em 2+ arquivos nunca é nome de sítio."""
    cont = contagem or contagem_de(arquivos)
    sitio_de: dict[str, oc_.SitioCfg] = {}
    por_pacote: dict[str, list[str]] = {}
    for rel, a in arquivos.items():
        if rel in excluidos:
            continue
        repetidas = frozenset(w for w in palavras(a.texto) if cont[a.pkg][w] > 1)
        if sitios := oc_.sitios_cfg(a.texto, repetidas):
            sitio_de[rel] = min(sitios, key=lambda s: sel._h(semente, f"{rel}|{s.linha_def}|{s.nome}"))
            por_pacote.setdefault(a.pkg, []).append(rel)
    saida: list[ch.Tentativa] = []
    for rel in sel.round_robin(por_pacote, n, semente + "|cfg"):
        a, s = arquivos[rel], sitio_de[rel]
        classe, codigo = oc_.CODIGO_CFG
        linha = oc_.linha_do_erro(s)
        inj = oc.Injecao(sel.id_injecao(rel, classe, linha, s.nome, semente), rel, classe, oc_.OPERADOR, codigo, linha, s.nome, a.texto,
                         oc_.mutar_cfg(a.texto, s), a.pkg, (codigo,))
        saida.append(ch.Tentativa(inj, a.pkg, a.alvo, "cfg", s.nome, s.nome, s.linha_def))
    return saida


# ------------------------------------------------------------------ registros


def registro_cfg(t: ch.Tentativa, rodada: int, motivo: str, diag: oc.Diag | None, observado: list | None = None) -> dict:
    return {**cr.registro(t, rodada, motivo, diag, observado), "linha_def": t.coluna, "linha_uso": oc_.linha_do_uso(t.inj.linha, t.coluna)}


def reconstruir_cfg(r: dict, texto: str) -> oc.Injecao:
    s = oc_.SitioCfg(r["nome_orig"], r["linha_def"], r["linha_uso"])
    return oc.Injecao(r["id"], r["rel"], r["classe"], r["operador"], r["codigo"], r["linha"], r["nome_msg"], texto, oc_.mutar_cfg(texto, s), r["pkg"], (r["codigo"],))


def tentativa_do_registro_cfg(r: dict, texto: str | None = None) -> ch.Tentativa:
    texto = texto if texto is not None else (cr.RAIZ / r["rel"]).read_bytes().decode("utf-8")
    return ch.Tentativa(reconstruir_cfg(r, texto), r["pkg"], r["alvo"], "cfg", r["nome_orig"], r["nome_orig"], r["linha_def"])


# ------------------------------------------------------------------ montagem


def escolher_ids_cfg(registros: list[dict]) -> set[str]:
    por_crate: dict[str, list[str]] = {}
    for r in registros:
        if r["motivo"] == "":
            por_crate.setdefault(r["pkg"], []).append(r["id"])
    return set(sel.selecao_ate(por_crate, TETO, SEMENTE_CFG + "|cfg", MAX_CFG))


def construir_positivos_cfg(registros: list[dict], ids: set[str], ler=None) -> list[ch.Positivo]:
    ler = ler or (lambda rel: (cr.RAIZ / rel).read_bytes().decode("utf-8"))
    saida = []
    for r in registros:
        if r["id"] in ids:
            texto = ler(r["rel"])
            saida.append(ch.Positivo(r["id"], r["rel"], r["pkg"], oc_.CODIGO_CFG[0], oc_.OPERADOR, r.get("codigo_rustc") or r["codigo"], r["linha"], r["coluna_rustc"],
                                     r["nome_msg"], r["nome_orig"], "cfg", r["msg"], texto, reconstruir_cfg(r, texto).texto_novo))
    return saida


def negativos_pareados(positivos: list[ch.Positivo]) -> list[ch.Negativo]:
    """O original de cada positivo, byte a byte: a única diferença entre o par é a linha `#[cfg(test)]`."""
    return [ch.Negativo(hashlib.sha256(f"par|{p.id}|{SEMENTE_CFG}".encode()).hexdigest()[:12], p.rel, p.pkg, p.texto_original, p.id) for p in positivos]


# ------------------------------------------------------------------ comandos (cargo / disco)


def entradas_do_principal() -> list[dict]:
    p = cr.CORPUS / "entradas.jsonl"
    if not p.is_file():
        raise RuntimeError(f"o corpus principal ({cr.CORPUS}) ainda não foi montado: o estrato cfg só planeja depois do `montar` dele")
    return [json.loads(x) for x in p.read_text().splitlines() if x.strip()]


def excluidos_do_principal() -> set[str]:
    """Todo arquivo que o corpus principal tentou OU selou (positivo e negativo)."""
    entradas = entradas_do_principal()
    prefixo = str(cr.RAIZ) + "/"
    return {r["rel"] for r in cr.ler_registros()} | {e["path"][len(prefixo):] for e in entradas}


def contagem_do_workspace(arquivos: dict[str, ch.Arquivo]) -> dict[str, collections.Counter]:
    """Todo `.rs` do diretório do pacote (inclui tests/, benches/, arquivos não elegíveis), não só os elegíveis."""
    meta = ch.metadados(cr.RAIZ)
    out: dict[str, collections.Counter] = {}
    for pkg in {a.pkg for a in arquivos.values()}:
        c: collections.Counter = collections.Counter()
        for f in meta[pkg]["dir"].rglob("*.rs"):
            c.update(palavras(f.read_text(encoding="utf-8", errors="replace")))
        out[pkg] = c
    return out


def plano_do_estrato() -> tuple[list[ch.Tentativa], dict]:
    cr.worktree_limpo()
    arquivos, _ = cr.montar_inventario()
    excluidos = excluidos_do_principal()
    plano = planejar_cfg(arquivos, excluidos, SEMENTE_CFG, TENTATIVAS_CFG, contagem_do_workspace(arquivos))
    info = {
        "semente": SEMENTE_CFG, "excluidos_sha256": hashlib.sha256("\n".join(sorted(excluidos)).encode()).hexdigest(), "excluidos_n": len(excluidos),
        "elegiveis": len(arquivos), "ids": [t.inj.id for t in plano], "por_pacote": dict(collections.Counter(t.pkg for t in plano)),
    }
    return plano, info


def congelar(info: dict) -> None:
    """O plano nasce uma vez: se o conjunto de excluídos mudar depois do primeiro lote, a retomada recusa em vez de mudar o plano."""
    if PLANO_JSON.is_file():
        antes = json.loads(PLANO_JSON.read_text())
        if antes["excluidos_sha256"] != info["excluidos_sha256"] or antes["ids"] != info["ids"]:
            raise RuntimeError("o plano do estrato cfg mudou desde o primeiro lote (excluídos ou ids diferentes): recuse e investigue")
    else:
        cr.ESTADO.mkdir(exist_ok=True)
        PLANO_JSON.write_text(json.dumps(info, indent=1))


def cmd_plano() -> int:
    plano, info = plano_do_estrato()
    print(json.dumps({k: info[k] for k in ("elegiveis", "excluidos_n", "por_pacote")} | {"tentativas": len(plano)}, ensure_ascii=False))
    return 0


def cmd_lotes(n_lotes: int | None) -> int:
    cr.exigir_slot()
    plano, info = plano_do_estrato()
    congelar(info)
    feitos = {r["id"] for r in cr.ler_registros(ARQUIVO)}
    pendentes = [t for t in plano if t.inj.id not in feitos]
    print(f"estrato cfg: {len(plano)} tentativas no plano, {len(pendentes)} pendentes", flush=True)
    cr.executar_plano(pendentes, lambda t: 1, n_lotes, via_slot=False, arquivo=ARQUIVO, gravar=registro_cfg)
    return 0


def cmd_reconfirmar() -> int:
    cr.worktree_limpo()
    saida = []
    validos = [r for r in cr.ler_registros(ARQUIVO) if r["motivo"] == ""]
    for r in cr.amostra(validos, oc_.CODIGO_CFG[0]):
        res = ch.executar([tentativa_do_registro_cfg(r)], cr.RAIZ, cr.TARGET, via_slot=True)
        saida.append({"id": r["id"], "classe": r["classe"], "sozinha": res[r["id"]] or "ok"})
        cr.worktree_limpo()
        print(f"  cfg {r['id']}: {saida[-1]['sozinha']}", flush=True)
    RECONF_JSON.write_text(json.dumps({"amostra": saida, "discordancias": [s for s in saida if s["sozinha"] != "ok"]}, indent=1))
    return 0 if all(s["sozinha"] == "ok" for s in saida) else 1


def montar_meta_cfg(positivos: list[ch.Positivo], negativos: list[ch.Negativo], registros: list[dict]) -> dict:
    plano = json.loads(PLANO_JSON.read_text()) if PLANO_JSON.is_file() else {}
    return {
        "nome": "HORUST-REAL-CFG", "estrato": "horust-real-cfg", "semente": SEMENTE_CFG, "worktree": str(cr.RAIZ), "worktree_head": cr.HEAD_ESPERADO,
        "worktree_tree": cr.git("rev-parse", "HEAD^{tree}"), "pre_registro_sha256": cr.sha_arquivo(HERE / "PRE-REGISTRO.md"), "pre_registro_sha256_original": cr.sha_da_hora("sha256"),
        "adendo_sha256": cr.sha_arquivo(HERE / "PRE-REGISTRO-ADENDO-CFG.md"), "positivos": len(positivos), "negativos": len(negativos),
        "piloto_n_abaixo_de_20": len(positivos) < MIN_CFG, "minimo": MIN_CFG, "maximo": MAX_CFG, "teto": TETO,
        "por_crate": dict(collections.Counter(p.pkg for p in positivos)), "tentativas": len(registros),
        "descartes_por_motivo": dict(collections.Counter(r["motivo"] for r in registros if r["motivo"])), "plano": {k: plano.get(k) for k in ("elegiveis", "excluidos_n", "por_pacote")},
        "limitacao": "recall sobre uma funcao movida para #[cfg(test)] e usada fora dele (E0425 no mesmo arquivo); nao e a distribuicao natural; "
                     "alvo de teste nao compilado; negativos pareados (mesmo arquivo)",
    }


def cmd_montar() -> int:
    cr.worktree_limpo()
    registros = cr.ler_registros(ARQUIVO)
    positivos = construir_positivos_cfg(registros, escolher_ids_cfg(registros))
    negativos = negativos_pareados(positivos)
    meta = montar_meta_cfg(positivos, negativos, registros)
    ch.escrever_corpus(CORPUS_CFG, cr.RAIZ, positivos, negativos, meta, estrato="horust-real-cfg")
    print(json.dumps({k: meta[k] for k in ("positivos", "negativos", "piloto_n_abaixo_de_20", "descartes_por_motivo")}, ensure_ascii=False))
    return 0 if positivos else 1


def main() -> int:
    cmd = sys.argv[1] if len(sys.argv) > 1 else ""
    if cmd == "plano":
        return cmd_plano()
    if cmd == "lotes":
        return cmd_lotes(int(sys.argv[2]) if len(sys.argv) > 2 else None)
    if cmd == "reconfirmar":
        return cmd_reconfirmar()
    if cmd == "montar":
        return cmd_montar()
    print(__doc__)
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
