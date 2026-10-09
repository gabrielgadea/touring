"""V0e5 — o balde "G1 rajada-de-inspecao" e a JUNTA de gates distintos.

O padrao do balde e `rajada-de-inspe|inspection burst|RAJADA`, compilado com re.I (v0e_negacoes.py:25,
linha 53) e casado por `search` em qualquer ponto do texto. `gate_of` devolve o PRIMEIRO padrao que casa
na ordem de GATES, e G1 e o primeiro: por isso o texto do S3 ("[CODE MODE · rajada] ...") e o do G10
("[G10 exec-burst] ... a rajada desenrolada ...") caem no balde G1 sem nunca serem do G1.

Q1: quebra o balde da linha de base por TEXTO-FONTE (predicado positivo sobre o inicio literal).
Q2: desde o deploy 30.4.94, quantos S3 continuam negando e quantas reescritas do G1 existem.
    A reescrita pode nao deixar o texto da razao no transcript: o tool_use guarda o comando ORIGINAL e
    o tool_result volta como envelope JSON do `touring run`. Sem o envelope, o evento e INVISIVEL — e o
    controle positivo e o que diz se o detector enxerga.

Uso: python3 v0e5_por_origem.py [--since 2026-09-17]
"""

import json
import re
import sys
from collections import Counter
from datetime import datetime
from pathlib import Path

SCRIPTS = Path.home() / "projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts"
HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPTS))
from inventario_erros import ROOTS, _text  # noqa: E402
from v0e_negacoes import HOOK_DENIAL, COMPILED  # noqa: E402

DEPLOY_94 = "2026-10-09T14:16:43Z"
G1_NOME = COMPILED[0][0]                                   # o nome vem do instrumento, nao de memoria
G1_PAT = COMPILED[0][1]                                    # o padrao do balde, compilado pelo proprio instrumento
assert G1_NOME == "G1 rajada-de-inspeção", f"o primeiro gate mudou: {G1_NOME}"

# Predicados POSITIVOS sobre o inicio literal do texto. A ordem importa: cada um tem que ser
# VERIFICAVEL no texto, nunca inferido do nome do gate.
ORIGEM = [
    ("S3 rajada do code mode", re.compile(r"^\[?(?:PreToolUse:\w+ hook error: )?\[CODE MODE · rajada\]")),
    ("G10 exec-burst", re.compile(r"^\[?(?:PreToolUse:\w+ hook error: )?\[G10 exec-burst\]")),
    ("G1 proprio (rajada-de-inspecao)", re.compile(r"^\[?(?:PreToolUse:\w+ hook error: )?\[G1 rajada-de-inspe")),
]


def origem_de(texto: str) -> str:
    """Qual gate EMITIU o texto. Quem nao casa com nenhum predicado fica 'outro' (visivel)."""
    for nome, pat in ORIGEM:
        if pat.search(texto):
            return nome
    return "outro (casou o padrao sem ser G1/S3/G10)"


def envelope_run(texto: str) -> dict | None:
    """O envelope JSON do `touring run`, se o tool_result for um. None quando nao for."""
    i = texto.find("{")
    if i < 0 or '"duration_ms"' not in texto:
        return None
    try:
        env, _ = json.JSONDecoder().raw_decode(texto[i:])
    except json.JSONDecodeError:
        return None
    return env if isinstance(env, dict) else None


def varre(since: str, ate: str | None = None):
    """Todo tool_result que casa o balde G1, classificado por texto-fonte, e os envelopes de reescrita.

    A REESCRITA do G1 nao e uma negacao: desde o 30.4.94 o gate devolve `allow` com updatedInput, e o
    tool_result volta como envelope JSON do `touring run` com `is_error=False`. Exigir is_error aqui
    escondia toda a reescrita (medido: o controle positivo de e7c33fba existe no transcript e nao
    aparecia). Entao o evento de reescrita e reconhecido pelo ENVELOPE, nunca por is_error.
    """
    sub, envs = Counter(), []
    chamadas = Counter()
    for root in ROOTS:
        for f in root.rglob("*.jsonl"):
            if f.stat().st_mtime < datetime.fromisoformat(since.replace("Z", "+00:00")).timestamp():
                continue
            cmds = {}
            for line in f.open(encoding="utf-8", errors="replace"):
                if '"tool_use"' not in line and '"tool_result"' not in line:
                    continue
                try:
                    rec = json.loads(line)
                except json.JSONDecodeError:
                    continue
                ts = str(rec.get("timestamp", ""))
                if ts < since or (ate and ts > ate):
                    continue
                for c in (rec.get("message") or {}).get("content") or []:
                    if not isinstance(c, dict):
                        continue
                    if c.get("type") == "tool_use":
                        cmds[c.get("id")] = str((c.get("input") or {}).get("command", ""))
                    elif c.get("type") == "tool_result":
                        t = _text(c)
                        if c.get("is_error") and HOOK_DENIAL.search(t) and G1_PAT.search(t):
                            sub[origem_de(t)] += 1
                            chamadas[f.stem] += 1
                        else:
                            env = envelope_run(t)
                            if env is not None:
                                envs.append({"ts": ts, "sessao": f.stem,
                                             "is_error": bool(c.get("is_error")),
                                             "cmd": cmds.get(c.get("tool_use_id"), "")[:160],
                                             "exit_code": env.get("exit_code"),
                                             "duration_ms": env.get("duration_ms"),
                                             "advisory": str((env.get("ceg_advisory") or {}).get("reason") or "")[:170]})
    return sub, envs, chamadas


def main():
    since = sys.argv[sys.argv.index("--since") + 1] if "--since" in sys.argv else "2026-09-17"

    # ---------------- Q1: linha de base, ate o deploy do 30.4.94 (excluido)
    base_sub, _, _ = varre(since, ate=DEPLOY_94)
    total_base = sum(base_sub.values())

    # O MESMO balde sem o corte superior, para conferir que a soma bate com o instrumento.
    full_sub, _, _ = varre(since)

    print(f"=== Q1 · balde '{G1_NOME}', --since {since} (o padrao do proprio v0e_negacoes.py) ===\n")
    print(f" Conservation: soma dos subgrupos == balde do instrumento")
    print(f"  G1 proprio + S3 + G10 + outro = {total_base}  (o instrumento reporta "
          f"{sum(full_sub.values())} sem corte de data)\n")
    print(f"  {'origem':<40} {'n':>6} {'% do balde':>11}")
    for nome, n in base_sub.most_common():
        print(f"  {nome:<40} {n:>6} {n / total_base * 100:>10.1f}%")
    print(f"  {'TOTAL':<40} {total_base:>6}")

    # ---------------- Q2: desde o deploy do 30.4.94
    pos_sub, envs, _ = varre(DEPLOY_94)
    print(f"\n=== Q2 · desde o deploy 30.4.94 ({DEPLOY_94}) ===")
    print(f"  S3 rajada do code mode (AINDA NEGA): "
          f"{pos_sub.get('S3 rajada do code mode', 0)}")
    print(f"  G1 proprio (AINDA NEGA):             {pos_sub.get('G1 proprio (rajada-de-inspecao)', 0)}")
    print(f"  G10 exec-burst:                      {pos_sub.get('G10 exec-burst', 0)}")

    # Controle positivo declarado pela coordenadora: sessao e7c33fba, pgrep -x touring-daemon.
    cp = [e for e in envs if e["sessao"].startswith("e7c33fba") and "pgrep" in e["cmd"]]
    print(f"\n  CONTROLE POSITIVO (e7c33fba + pgrep, esperado >=1): "
          f"{'ACHADO' if cp else 'NAO ACHADO'}")
    for e in cp[:4]:
        print(f"    {e['ts']}  exit={e['exit_code']}  advisory={e['advisory'][:90]!r}")
    print(f"    cmd: {cp[0]['cmd'][:150]!r}" if cp else "    (nenhum)")
    print(f"\n  envelopes de `touring run` no periodo (assinatura de reescrita): {len(envs)}")
    for e in sorted(envs, key=lambda x: x["ts"])[-8:]:
        print(f"    {e['ts']}  {e['sessao'][:8]}  exit={e['exit_code']}  {e['cmd'][:78]!r}")

    saida = {"since": since, "deploy_94": DEPLOY_94,
             "q1_base_ate_deploy": {"subgrupos": dict(base_sub), "total": total_base,
                                    "instrumento_sem_corte": sum(full_sub.values())},
             "q1_base_por_origem": dict(full_sub),
             "q2_pos_deploy": {"subgrupos": dict(pos_sub), "total": sum(pos_sub.values()),
                               "envelopes_run": len(envs),
                               "controle_positivo": {"achado": bool(cp), "eventos": cp}}}
    (HERE / "v0e5_por_origem.json").write_text(json.dumps(saida, ensure_ascii=False, indent=1))
    print(f"\njson -> {HERE / 'v0e5_por_origem.json'}")
    return 0


if __name__ == "__main__":
    sys.exit(main())