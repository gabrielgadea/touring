"""HORUST — MEDIR (nao construir): quantos positivos Rust por classe sobram com (1) o crate inteiro sobreposto no instante e (2) a
classe A fora, em dois universos, e a proposta de divisao dev/held-out.

  (a) universo atual: corte 2026-10-09T20:40Z (o do `corpus_rust_natural`); controle: sem nenhuma regra nova o gerador tem de
      devolver EXATAMENTE os 37 ids positivos que ja estao no corpus.
  (b) universo ampliado: corte 2026-10-10T00:00Z.
  Cada universo em duas visoes: todas as sessoes | sem as sessoes do plano VGP (esta sessao e subagentes, touring-4f, touring-a6 e
  qualquer transcript que cite `vgp-motor-base`; lista em `dados/sessoes_vgp.json`). As edicoes de TODAS as sessoes seguem na dobra
  (a reconstrucao de um arquivo depende delas); a exclusao vale para a ORIGEM do positivo/negativo.

Nao grava corpus nenhum, nao toca `corpus_rust_natural/`, nao roda o motor nem o juiz. Saida: `dados/medicao.json` (+ `dados/itens.json`).
Uso (nativo, o sandbox nao le ~/.claude/projects): python3 -I -B medir_horust.py <sessoes_vgp.json> <saida_dir>
"""

import collections
import json
import os
import re
import statistics
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

B = Path("/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base")
sys.path.insert(0, str(B / "scripts"))
import corpus_rust_natural as crn  # noqa: E402
import corpus_v0d  # noqa: E402
import estado_rust_no_instante as ers  # noqa: E402
import reconhecedor_cargo_estendido as rec_est  # noqa: E402
import replay  # noqa: E402

CORTE_A, CORTE_B = "2026-10-09T20:40:00.000Z", "2026-10-10T00:00:00.000Z"
CORTE_C = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%S.000Z")  # EXTRA, nao pedido: ate agora (informativo)
CLASSES = ("nome", "import", "atributo", "kwarg")
VGP_DIR = Path.home() / ".claude/touring/vgp"
APLICA_A = re.compile(r"cannot find|not found in this scope|use of undeclared")


def memoizar_git() -> dict:
    """Memo das leituras de git (rev-list/show/ls-tree) por (raiz, args): o crate le dezenas de modulos por entrada."""
    conta = {"hits": 0, "miss": 0}
    originais = {"v0d": corpus_v0d._git, "crn": crn._git}
    caches: dict[str, dict] = {"v0d": {}, "crn": {}}

    def envolver(nome: str, original):
        def git(root, *args):
            if not args or args[0] not in ("rev-list", "show", "ls-tree"):
                return original(root, *args)
            chave = (str(root), args)
            if chave in caches[nome]:
                conta["hits"] += 1
            else:
                conta["miss"] += 1
                caches[nome][chave] = original(root, *args)
            return caches[nome][chave]

        return git

    corpus_v0d._git = envolver("v0d", originais["v0d"])
    crn._git = envolver("crn", originais["crn"])
    return conta


def membros_do_workspace(raiz: Path, cache: dict) -> dict[str, str]:
    """{nome do pacote com '-' -> '_': dir} dos membros do workspace em `raiz` (Cargo.toml de hoje)."""
    if str(raiz) in cache:
        return cache[str(raiz)]
    import tomllib

    out: dict[str, str] = {}
    try:
        m = tomllib.loads((raiz / "Cargo.toml").read_text(encoding="utf-8"))
        padroes = (m.get("workspace") or {}).get("members", [])
    except (OSError, tomllib.TOMLDecodeError):
        padroes = []
    for pad in padroes:
        for d in sorted(raiz.glob(pad)):
            try:
                pk = tomllib.loads((d / "Cargo.toml").read_text(encoding="utf-8")).get("package", {}).get("name")
            except (OSError, tomllib.TOMLDecodeError):
                pk = None
            if pk:
                out[pk.replace("-", "_")] = str(d)
    cache[str(raiz)] = out
    return out


def ultimo_write_ts(uni, caminho: str, ate: str) -> str:
    return max((r["timestamp"] for r in uni.idx.get(caminho, []) if r["message"]["content"][0].get("name") == "Write" and r["timestamp"] <= ate), default="")


def avaliar(e: dict, snap: str, ctx, uni, ws_cache: dict) -> dict:
    """Tudo o que a medicao quer saber de UM positivo: o crate no instante, a classe A, o Bash e o cross-crate."""
    alvo, ts = e["path"], e["ts"]
    crate = crn.crate_de(alvo, ctx.manifestos)
    r: dict = {"id": e["id"], "classe": e["classe"], "codigo": e["codigo"], "nome": e["nome_esperado"], "projeto": e["projeto"],
               "sid": e["session"], "ts": ts, "path": alvo, "mensagem": e["mensagem"]}
    if crate is None:
        return {**r, "crate": None, "completo": False, "motivo": "crate não encontrado", "motivos": ["crate_nao_encontrado"], "n_arquivos": 0,
                "classe_a": None, "bash_crate": [], "bash_proprio": False, "cross_crate": [], "completo_ws": False, "motivos_ws": [], "n_arquivos_ws": 0}
    prov = ers.ProvedorReconstrutor(ctx.rec, ts)
    est = ers.estado_do_crate(Path(crate.dir), prov, proprio=alvo, extras_raiz=ers.raizes_do_manifesto(Path(crate.dir)))
    completo, motivo = ers.motivo_incompleto(est)
    suspeitos = []
    for p in [a.path for a in est.arquivos]:
        lt = ctx.rec.linha_tempo(str(p), ts)
        de = max(lt.commit_ts if lt else "", ultimo_write_ts(uni, str(p), ts))
        if crn.reescrito_por_bash(str(p), de, ts, uni):
            suspeitos.append(str(p))
    classe_a = None
    if APLICA_A.search(e["mensagem"]) and e.get("linha") and e.get("coluna"):
        classe_a = ers.define_no_escopo(snap, e["linha"], e["coluna"], e["nome_esperado"])
    raiz_ws = Path(e["project_root"])
    membros = membros_do_workspace(raiz_ws, ws_cache)
    proprio_lib = crate.nome.replace("-", "_")
    outros = sorted(n for n in membros if n != proprio_lib and re.search(rf"(?<![\w]){re.escape(n)}::", ers.limpar(snap)))
    ws_motivos, ws_n = [], 0
    for nome_c in outros:  # os crates IRMAOS que o snapshot cita: o motor tambem os le (membros do workspace)
        d = Path(membros[nome_c])
        est_c = ers.estado_do_crate(d, prov, proprio=None, extras_raiz=ers.raizes_do_manifesto(d))
        ws_n += len(est_c.arquivos)
        ws_motivos += [f"{nome_c}:{m}" for m in sorted({f.motivo for f in est_c.faltas})]
    return {**r, "crate": crate.nome, "completo": completo, "motivo": motivo, "motivos": sorted({f.motivo for f in est.faltas}),
            "completo_ws": completo and not ws_motivos, "motivos_ws": ws_motivos, "n_arquivos_ws": len(est.arquivos) + ws_n,
            "n_arquivos": len(est.arquivos), "n_orfaos": len(est.orfaos), "faltas": [[f.motivo, f.path.replace(crate.dir + "/", "")] for f in est.faltas][:6],
            "classe_a": classe_a, "bash_crate": [s.replace(crate.dir + "/", "") for s in suspeitos if s != alvo], "bash_proprio": alvo in suspeitos,
            "cross_crate": outros}


def por_classe(linhas: list[dict]) -> dict[str, int]:
    c = collections.Counter(l["classe"] for l in linhas)
    return {k: c.get(k, 0) for k in CLASSES} | {"total": len(linhas)}


def funil(itens: list[dict]) -> dict:
    """O funil de UM conjunto de positivos (todos ja passaram pelo gerador atual)."""
    crate_ok = [i for i in itens if i["completo"]]
    sem_a = [i for i in crate_ok if i["classe_a"] is None]
    so_a = [i for i in itens if i["classe_a"] is None]
    return {
        "gerador_atual (como esta hoje)": por_classe(itens),
        "so_classe_A (sem o crate)": por_classe(so_a),
        "crate_completo": por_classe(crate_ok),
        "SOBRAM (crate completo e fora da classe A)": por_classe(sem_a),
        "sensibilidade 1: SOBRAM sem nenhum arquivo do crate reescrito por Bash": por_classe([i for i in sem_a if not i["bash_crate"] and not i["bash_proprio"]]),
        "sensibilidade 2: SOBRAM sem o ARQUIVO DA ENTRADA reescrito por Bash": por_classe([i for i in sem_a if not i["bash_proprio"]]),
        "sensibilidade 3: SOBRAM com os crates irmaos citados tambem completos no instante": por_classe([i for i in sem_a if i["completo_ws"]]),
        "sensibilidade 4: 1 + 3 juntas": por_classe([i for i in sem_a if i["completo_ws"] and not i["bash_crate"] and not i["bash_proprio"]]),
        "incompletos_com_crate_irmao (motivo)": dict(collections.Counter(m for i in crate_ok if not i["completo_ws"] for m in i["motivos_ws"]).most_common()),
        "incompletos_por_motivo (prioridade)": dict(collections.Counter(i["motivo"] for i in itens if not i["completo"]).most_common()),
        "incompletos_por_qualquer_motivo": dict(collections.Counter(m for i in itens if not i["completo"] for m in i["motivos"]).most_common()),
        "classe_A_por_razao": dict(collections.Counter(re.sub(r"`[^`]*`|\(L\d+\)", "", i["classe_a"]).strip() for i in itens if i["classe_a"]).most_common()),
        "entradas_com_bash_no_crate": sum(1 for i in itens if i["bash_crate"]),
        "entradas_com_arquivo_proprio_reescrito_por_bash": sum(1 for i in itens if i["bash_proprio"]),
        "entradas_que_citam_outro_crate_do_workspace": sum(1 for i in itens if i["cross_crate"]),
        "por_projeto (sobram)": dict(collections.Counter(i["projeto"] for i in sem_a)),
        "sessoes_distintas (sobram)": len({i["sid"] for i in sem_a}),
        "tamanho_do_crate (arquivos) das entradas completas": statistics.median([i["n_arquivos"] for i in crate_ok]) if crate_ok else 0,
        "agrupamento (sobram): arquivos distintos": len({i["path"] for i in sem_a}), "agrupamento (sobram): pares (arquivo, nome) distintos": len({(i["path"], i["nome"]) for i in sem_a}),
    }


def main() -> int:
    t0 = time.monotonic()
    estendido = os.environ.get("HORUST_ESTENDIDO") == "1"  # reconhecedor de cargo estendido (hw-slot --wait, env -u, bash -c ...)
    if estendido:
        rec_est.instalar()
    vgp = set(json.loads(Path(sys.argv[1]).read_text())["vgp"])
    saida = Path(sys.argv[2])
    saida.mkdir(parents=True, exist_ok=True)
    memo = memoizar_git()
    uni = crn.Universo(CORTE_C)
    for proj in crn.PROJETOS:
        uni.varrer(proj, replay.PROJECTS / f"-home-gabrielgadea-projects-{proj}")
    uni.fechar()
    cfg = crn.Config((), saida, (Path.home() / "projects",), corte=CORTE_C)
    ctx = crn.Ctx(cfg, uni, crn.Reconstrutor(uni.idx, uni.erros))
    todas = list(uni.ocorrencias)
    dev = [json.loads(l) for l in (VGP_DIR / "corpus_rust_natural/entradas.jsonl").read_text().splitlines() if l.strip()]
    dev_pos = {e["id"] for e in dev if e["rotulo"] == "positivo"}
    perdidos = set()
    ids_dep = json.loads((B / "diagnostics/vrs-porta-2026-10-10/dados/ids_depois_natural.json").read_text())["ids"]
    for e in dev:
        if e["rotulo"] == "positivo" and not any(f["nome"] == e["nome_esperado"] for f in ids_dep[e["id"]]["findings"]):
            perdidos.add(e["id"])
    variantes = {
        "A_todas_as_sessoes (universo atual, = DEV)": lambda o: o.ts_resultado <= CORTE_A,
        "A_sem_sessoes_VGP": lambda o: o.ts_resultado <= CORTE_A and o.chamada.sid not in vgp,
        "B_todas_as_sessoes (ampliado)": lambda o: o.ts_resultado <= CORTE_B,
        "B_sem_sessoes_VGP (ampliado, o pedido)": lambda o: o.ts_resultado <= CORTE_B and o.chamada.sid not in vgp,
        "C_todas (EXTRA: ate agora)": lambda o: True,
        "C_sem_sessoes_VGP (EXTRA: ate agora)": lambda o: o.chamada.sid not in vgp,
    }
    ws_cache: dict = {}
    todos_itens: dict[str, list[dict]] = {}
    gerador: dict[str, dict] = {}
    for nome, filtro in variantes.items():
        uni.ocorrencias = [o for o in todas if filtro(o)]
        itens, motivos, stats = crn.construir_positivos(ctx)
        linhas = [{**avaliar(i.entrada, i.snap, ctx, uni, ws_cache), "em_dev": i.entrada["id"] in dev_pos, "perdido_vrs": i.entrada["id"] in perdidos,
                   "introduzido": i.entrada.get("introduzido")} for i in itens]
        todos_itens[nome] = linhas
        gerador[nome] = {"ocorrencias": len(uni.ocorrencias), "positivos": len(itens), "descartados_por_motivo": dict(motivos.most_common()), **stats}
        print(nome, len(itens), por_classe(linhas), flush=True)
    uni.ocorrencias = todas
    ctl = {i["id"] for i in todos_itens["A_todas_as_sessoes (universo atual, = DEV)"]}
    controle = {"reconhecedor_estendido": estendido, "dev_contido_no_gerador": dev_pos <= ctl,
                "ids_do_gerador_hoje": len(ctl), "ids_positivos_do_corpus": len(dev_pos), "iguais": ctl == dev_pos,
                "so_no_gerador": sorted(ctl - dev_pos), "so_no_corpus": sorted(dev_pos - ctl)}
    medicao = {"cortes": {"A": CORTE_A, "B": CORTE_B, "C (extra)": CORTE_C}, "sessoes_vgp": len(vgp), "controle_dos_37": controle, "gerador": gerador,
               "funil": {n: funil(l) for n, l in todos_itens.items()}, "git_memo": memo, "tempo_s": round(time.monotonic() - t0, 1)}
    # dev x held-out: o que e NOVO desde o corte A, fora das sessoes VGP, depois das regras novas
    novos = [i for i in todos_itens["B_sem_sessoes_VGP (ampliado, o pedido)"] if i["ts"] > CORTE_A]
    medicao["candidatos_a_held_out (ts > corte A, sem sessoes VGP)"] = funil(novos)
    fora_dev = [i for i in todos_itens["C_sem_sessoes_VGP (EXTRA: ate agora)"] if not i["em_dev"]]
    medicao["FORA_DO_DEV, sem sessoes VGP (qualquer data <= agora)"] = funil(fora_dev)
    medicao["FORA_DO_DEV, sem sessoes VGP: por semana"] = dict(sorted(collections.Counter(i["ts"][:10] for i in fora_dev).items()))
    novos_c = [i for i in todos_itens["C_sem_sessoes_VGP (EXTRA: ate agora)"] if i["ts"] > CORTE_A]
    medicao["candidatos_a_held_out_EXTRA (ts > corte A, ate agora, sem sessoes VGP)"] = funil(novos_c)
    dev_sobram = [i for i in todos_itens["A_todas_as_sessoes (universo atual, = DEV)"] if i["completo"] and i["classe_a"] is None]
    medicao["DEV_apos_as_regras_novas"] = {"sobram": por_classe(dev_sobram), "nome_perdidos_pelo_vrs_e_o_que_acontece": [
        {k: i[k] for k in ("id", "nome", "completo", "motivos", "classe_a", "n_arquivos", "cross_crate", "bash_crate", "bash_proprio")}
        for i in todos_itens["A_todas_as_sessoes (universo atual, = DEV)"] if i["perdido_vrs"]]}
    (saida / "medicao.json").write_text(json.dumps(medicao, ensure_ascii=False, indent=1), encoding="utf-8")
    (saida / "itens.json").write_text(json.dumps(todos_itens, ensure_ascii=False, indent=1), encoding="utf-8")
    print(json.dumps(medicao["controle_dos_37"], ensure_ascii=False))
    print("tempo_s", medicao["tempo_s"])
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
