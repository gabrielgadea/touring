"""Medição V0d nº2 (09/10): taxa de reconstrução por rota×estratégia numa amostra, e anatomia
dos positivos incompletos. Uma passada de transcripts, só agregados na saída."""
import json
import random
import re
import subprocess
import sys
import time
from collections import Counter
from pathlib import Path

B = Path("/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts")
sys.path.insert(0, str(B))
import replay  # noqa: E402
import juiz_v0d  # noqa: E402
import corpus_v0d  # noqa: E402

VGP = replay.VGP
t0 = time.monotonic()


def git_root(arq: str) -> Path | None:
    d = Path(arq).parent
    for anc in [d, *d.parents]:
        if (anc / ".git").exists():
            return anc
        if str(anc) == "/":
            break
    return None


print("=== varrer_transcripts ===", flush=True)
indice = corpus_v0d.varrer_transcripts()
print(f"arquivos .py editados: {len(indice)} · {time.monotonic()-t0:.0f}s", flush=True)

# ---------------------------------------------------------------- negativos: amostra por rota
cand = json.loads((VGP / "corpus_v0d/negativos_candidatos.json").read_text())
pool = cand["candidatos"]


def rota(arq: str) -> str:
    if replay.session_work(arq):
        return "session_work"
    r = git_root(arq)
    return f"git:{r}" if r else "no-git"


por_rota: dict[str, list[str]] = {}
for c in pool:
    por_rota.setdefault(rota(c.rsplit("@", 1)[0]).split(":")[0], []).append(c)
rng = random.Random(9)
amostra = [c for k, v in sorted(por_rota.items()) for c in rng.sample(v, min(40, len(v)))]
print(f"amostra: {len(amostra)} ({ {k: len(v) for k, v in por_rota.items()} })", flush=True)


def base_git(root: Path, rel: str, ts: str) -> str | None:
    rev = subprocess.run(["git", "-C", str(root), "rev-list", "-1", f"--before={ts}", "HEAD"],
                         capture_output=True, text=True).stdout.strip()
    if not rev:
        return None
    p = subprocess.run(["git", "-C", str(root), "show", f"{rev}:{rel}"], capture_output=True, text=True)
    return p.stdout if p.returncode == 0 else None


def dobra_ord(registros: list[dict], arq: str, base: str, until: str) -> tuple[str, set[str]]:
    ordenados = sorted(registros, key=lambda r: r.get("timestamp", ""))
    return corpus_v0d._dobra_projeto(ordenados, arq, base, until)


def tenta(chave: str) -> tuple[str, str]:
    """(desfecho, detalhe) de reconstruir o candidato com a estratégia da rota dele, corte em ts."""
    arq, ts = chave.rsplit("@", 1)
    registros = indice.get(arq) or []
    r = rota(arq)
    if r.startswith("git:"):
        root = Path(r[4:])
        try:
            rel = str(Path(arq).resolve().relative_to(root.resolve()))
        except ValueError:
            return "falha", "fora da raiz"
        base = base_git(root, rel, ts)
        if base is None:
            tem_write = any(c.get("name") == "Write" and str((c.get("input") or {}).get("file_path")) == arq
                            for rec in registros for c in replay._contents(rec) if c.get("type") == "tool_use")
            if not tem_write:
                return "falha", "sem base git e sem Write"
            texto, quebrados = dobra_ord(registros, arq, "", ts)
            if quebrados:
                return "falha", "dobra de base vazia quebrou"
        else:
            texto, quebrados = dobra_ord(registros, arq, base, ts)
            if quebrados:
                return "falha", "Edit nao casa (git)"
    else:
        # session_work / no-git: dobra da sessão; válida só se houver Write <= ts
        tem_write = any(c.get("name") == "Write" and str((c.get("input") or {}).get("file_path")) == arq
                        and rec.get("timestamp", "") <= ts
                        for rec in registros for c in replay._contents(rec) if c.get("type") == "tool_use")
        if not tem_write:
            return "falha", "sem Write <= ts"
        texto, quebrados = dobra_ord(registros, arq, "", ts)
        if quebrados:
            return "falha", "dobra de sessao quebrou"
    try:
        compile(texto, arq, "exec")
    except (SyntaxError, ValueError):
        return "falha", "nao compila"
    return "ok", r


res = Counter()
det = Counter()
t1 = time.monotonic()
for i, c in enumerate(amostra):
    d, why = tenta(c)
    res[(rota(c.rsplit("@", 1)[0]).split(":")[0], d)] += 1
    if d == "falha":
        det[(rota(c.rsplit("@", 1)[0]).split(":")[0], why)] += 1
    if i % 20 == 19:
        print(f"  …{i+1}/{len(amostra)} {time.monotonic()-t1:.0f}s", flush=True)
print("\n=== reconstrução por (rota, desfecho) ===")
for k, v in sorted(res.items()):
    print(f"  {k}: {v}")
print("falhas por (rota, motivo):")
for k, v in det.most_common():
    print(f"  {k}: {v}")

# ---------------------------------------------------------------- positivos: anatomia
print("\n=== POSITIVOS ===", flush=True)
rows = [json.loads(ln) for ln in (VGP / "erros_inventario.jsonl").open(encoding="utf-8")]
cands = [r for r in rows if r.get("lang") == "python" and r.get("bucket") in ("atribuído", "inline")
         and juiz_v0d.classe_de(r)]

# (a) inline touring run: o --code usa o stub `touring.`? e o -c/heredoc dentro?
usa_stub, tem_prog, sh_indireto = 0, 0, 0
tour_cmds = []
for r in cands:
    if r["bucket"] == "inline" and not corpus_v0d.programa_inline(r.get("command", "")):
        cmd = r.get("command", "")
        if "touring run" in cmd:
            tour_cmds.append(r)
            m = re.search(r"--code\s+'", cmd)
            usa_stub += bool(re.search(r"\btouring\.\w", cmd))
            tem_prog += bool(m)
        else:
            sh_indireto += 1
print(f"inline não-extraídos: touring run {len(tour_cmds)} (com --code '…' {tem_prog}, usa stub touring. {usa_stub}) · sh indireto {sh_indireto}")

# (b) atribuído "não existia no commit anterior": a sessão tem Write do arquivo <= first?
criados, sem_write = 0, 0
for r in cands:
    if r["bucket"] != "atribuído":
        continue
    arq, first = r.get("file", ""), r.get("first", "")
    if replay.session_work(arq):
        continue
    root = git_root(arq)
    if not root:
        continue
    try:
        rel = str(Path(arq).resolve().relative_to(root.resolve()))
    except ValueError:
        continue
    if base_git(root, rel, first) is not None:
        continue
    recs = list(replay._records(replay.transcripts_of(r.get("session", ""))))
    tem = any(c.get("name") == "Write" and str((c.get("input") or {}).get("file_path")) == arq
              and rec.get("timestamp", "") <= first
              for rec in recs for c in replay._contents(rec) if c.get("type") == "tool_use")
    criados += bool(tem)
    sem_write += not tem
print(f"atribuído sem base git: {criados + sem_write} · com Write <= first na sessão {criados} · sem Write {sem_write}")

# (c) tmp "não gravado antes de first": o arquivo aparece em OUTRO transcript (índice global)?
tmp_salvos = 0
tmp_total = 0
for r in cands:
    if r["bucket"] != "atribuído":
        continue
    arq, first = r.get("file", ""), r.get("first", "")
    if not replay.session_work(arq):
        continue
    recs = list(replay._records(replay.transcripts_of(r.get("session", ""))))
    files, _ = replay.rebuild_tmp(recs, first)
    if arq in files:
        continue  # já reconstrói hoje
    tmp_total += 1
    # existe Write/Edit desse arquivo em QUALQUER transcript antes de first?
    outro = any(c.get("name") in ("Write", "Edit", "MultiEdit")
                and str((c.get("input") or {}).get("file_path")) == arq and rec.get("timestamp", "") <= first
                for rec in (indice.get(arq) or []) for c in replay._contents(rec) if c.get("type") == "tool_use")
    tmp_salvos += bool(outro)
print(f"tmp não gravado na própria sessão: {tmp_total} · gravado em OUTRA sessão <= first: {tmp_salvos}")

print(f"\ntotal {time.monotonic()-t0:.0f}s")
