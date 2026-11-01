"""Medição V0d (09/10): composição do universo de negativos e anatomia dos positivos falhos.

Saída só agregada. Nada é escrito fora de /tmp/v0d/.
"""
import json
import sys
from collections import Counter
from pathlib import Path

B = Path("/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts")
sys.path.insert(0, str(B))
import replay  # noqa: E402
import juiz_v0d  # noqa: E402
import corpus_v0d  # noqa: E402

VGP = replay.VGP


def rota(arq: str) -> str:
    """Classifica o caminho: session_work (rebuild_tmp) · git:<raiz> · no-git."""
    if replay.session_work(arq):
        return "session_work"
    p = Path(arq)
    d = p.parent
    for anc in [d, *d.parents]:
        if (anc / ".git").exists():
            return f"git:{anc}"
        if str(anc) == "/":
            break
    return "no-git"


print("=== 1. UNIVERSO DE NEGATIVOS (pool registrado) ===")
cand = json.loads((VGP / "corpus_v0d/negativos_candidatos.json").read_text())
pool = cand["candidatos"]
print(f"seed={cand['seed']} · candidatos={len(pool)} · exemplo={pool[0][:80]!r}")
rotas = Counter()
raizes = Counter()
existe_hoje = Counter()
for c in pool:
    arq = c.rsplit("@", 1)[0]
    r = rota(arq)
    rotas[r.split(":")[0]] += 1
    if r.startswith("git:"):
        raizes[r] += 1
    existe_hoje[(r.split(":")[0], Path(arq).is_file())] += 1
print("rotas:", rotas.most_common())
print("raízes git:", raizes.most_common(6))
print("(rota, existe_hoje):", existe_hoje.most_common())

print("\n=== 2. POSITIVOS: os 100 candidatos V0a e o destino de cada um ===")
rows = [json.loads(ln) for ln in (VGP / "erros_inventario.jsonl").open(encoding="utf-8")]
cands = [r for r in rows if r.get("lang") == "python" and r.get("bucket") in ("atribuído", "inline")
         and juiz_v0d.classe_de(r)]
print("candidatos V0a:", len(cands))

# 2a. inline cujo programa NÃO se extrai hoje — qual a cara do comando?
padroes = Counter()
exemplos = {}
for r in cands:
    if r["bucket"] != "inline":
        continue
    cmd = r.get("command", "")
    if corpus_v0d.programa_inline(cmd):
        continue
    if "touring run" in cmd:
        k = "touring run"
    elif re := __import__("re"):
        m = re.search(r"python3?\s+(\S+\.py)", cmd)
        k = f"python3 arquivo.py ({'tmp' if m and replay.session_work(m.group(1)) else 'outro'})" if m else "outro"
    padroes[k] += 1
    exemplos.setdefault(k, []).append((r.get("file", "?")[:60], cmd[:160]))
print("inline sem extração, por padrão:", padroes.most_common())
for k, exs in exemplos.items():
    print(f"\n--- {k} ({len(exs)}) ---")
    for f, c in exs[:3]:
        print(f"  file={f!r}\n  cmd={c!r}")

# 2b. atribuído: o arquivo existe hoje? está em repo? existia no rev anterior?
print("\n=== 3. ATRIBUÍDO: anatomia ===")
atrib = [r for r in cands if r["bucket"] == "atribuído"]
rotas_a = Counter(rota(r.get("file", "")) .split(":")[0] for r in atrib)
print("atribuído por rota:", rotas_a.most_common())

# 2c. o relatório anterior: não-reproduzem por origem
rel = json.loads((VGP / "corpus_v0d/relatorio.json").read_text())
print("\n=== 4. RELATÓRIO ANTERIOR ===")
print("incompletos:", json.dumps(rel["incompletos_por_motivo"], ensure_ascii=False, indent=1))
print("reproduzem por origem:", rel["reproduzem_por_origem"])
print("reproduzem por classe:", rel["reproduzem_por_classe"])
nr = rel["nao_reproduzem"]
print(f"nao_reproduzem: {len(nr)}")
por = Counter((r["origem"], r["rc"], bool(r["kind_in_output"]), bool(r["frame_in_output"])) for r in nr)
for k, v in sorted(por.items(), key=lambda x: -x[1]):
    print(f"  origem={k[0]:<10} rc={str(k[1]):<5} kind={k[2]} frame={k[3]} → {v}")
