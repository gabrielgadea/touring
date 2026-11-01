#!/usr/bin/env python3
"""Prova prática do motor VGP com o binário INSTALADO — cada célula da política P4, ponta a ponta.

Escrita pela coordenadora analise-89 em 09/10/2026, antes do deploy (nasce vermelha; fica verde com o release).
Política P4 (proposta §5): código que roda AGORA (inline, /tmp) nega símbolo inventado; arquivo do projeto nega só
alvo EXTERNO e avisa o resto (consumidor antes do provedor, RED de TDD); `forma` (membro de valor builtin) e
`dinamico` (dunder, nome sob `import *`) só avisam; o que já estava no disco nunca é culpa da edição.

[1] `touring vgp check --camadas 1,2` em cada célula  ·  [2] hooks reais (sombra e ativo)  ·  [3] os juízes do plano.
Exit 0 = tudo afirmado; 1 = alguma afirmação falhou (cada uma impressa com a evidência). Falha fechada: o que não
roda (binário sem `vgp`, hook ausente) é FALHA, nunca "pulado".
Uso: python3 prova_vgp_motor.py [--touring <bin>] [--so 1,2,3]
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

H = Path.home()
ANALISE = H / "projects/analise"
VGP = H / "projects/touring/docs/plans/2026-10-08-vgp-motor-base"
FALHAS: list[str] = []
CAMADAS = {"valor": "1,2"}  # `--camadas` da prova (1 = só a camada nativa, sem a flag)


def afirma(ok: bool, titulo: str, evidencia: str) -> None:
    print(f"  {'OK   ' if ok else 'FALHA'} {titulo} — {evidencia[:150]}")
    if not ok:
        FALHAS.append(titulo)


def vgp_check(
    touring: str, path: str, content: str, disk: str | None = None, camadas: str = "1,2"
) -> dict:
    """Uma chamada do contrato; saída que não é JSON vira {"_erro"} (falha fechada)."""
    with tempfile.TemporaryDirectory() as tmp:
        cf = Path(tmp) / "proposto.py"
        cf.write_text(content)
        cmd = [
            touring,
            "vgp",
            "check",
            "--path",
            path,
            "--content-file",
            str(cf),
            "--project-root",
            str(ANALISE),
            "--lang",
            "python",
            "-j",
        ]
        cmd += ["--camadas", camadas] if camadas != "1" else []
        if disk is not None:
            df = Path(tmp) / "disco.py"
            df.write_text(disk)
            cmd += ["--disk-file", str(df)]
        r = subprocess.run(
            cmd, capture_output=True, text=True, timeout=120, check=False
        )
    try:
        return json.loads(r.stdout)
    except json.JSONDecodeError:
        return {"_erro": f"rc {r.returncode} · {r.stdout[:80]!r} · {r.stderr[:160]!r}"}


def severidade_de(saida: dict, nome: str) -> str | None:
    """A severidade MAIS forte entre os achados com esse nome (None = não achou)."""
    sevs = {
        f.get("severidade") for f in saida.get("findings", []) if f.get("nome") == nome
    }
    return "deny" if "deny" in sevs else ("warn" if "warn" in sevs else None)


# (título, path, conteúdo, disco, nome esperado, severidade esperada; None = nenhum achado, veredito allow)
ARQ = str(ANALISE / "scripts/_prova_vgp_celula.py")
CELULAS = [
    (
        "inline · externo inventado → DENY",
        "<stdin>",
        "import os\nos.caminho_inventado_vgp\n",
        None,
        "caminho_inventado_vgp",
        "deny",
    ),
    (
        "inline · import inventado de módulo do projeto → DENY",
        "<string>",
        "from scripts.memoria.grafo_memoria import gravar_inventado_vgp\n",
        None,
        "gravar_inventado_vgp",
        "deny",
    ),
    (
        "arquivo · externo inventado → DENY",
        ARQ,
        "import os\nos.caminho_inventado_vgp\n",
        None,
        "caminho_inventado_vgp",
        "deny",
    ),
    (
        "arquivo · símbolo do PROJETO ainda inexistente (RED de TDD) → só AVISA",
        ARQ,
        "from scripts.memoria.grafo_memoria import gravar_inventado_vgp\n",
        None,
        "gravar_inventado_vgp",
        "warn",
    ),
    (
        "inline · membro de valor builtin (forma) → só AVISA",
        "<stdin>",
        'x = "texto"\nx.metodo_inventado_vgp()\n',
        None,
        "metodo_inventado_vgp",
        "warn",
    ),
    (
        "inline · nome sob `import *` (dinâmico) → só AVISA",
        "<stdin>",
        "from os.path import *\nprint(nome_inventado_vgp)\n",
        None,
        "nome_inventado_vgp",
        "warn",
    ),
    (
        "inline · programa limpo → ALLOW sem achado",
        "<stdin>",
        "import os\nprint(os.sep)\n",
        None,
        None,
        None,
    ),
    (
        "arquivo · defeito que JÁ estava no disco não é culpa da edição → ALLOW",
        ARQ,
        "import os\nos.caminho_inventado_vgp\nprint(1)\n",
        "import os\nos.caminho_inventado_vgp\n",
        None,
        None,
    ),
]


def prova_celulas(touring: str) -> None:
    print(
        f"\n[1] `touring vgp check --camadas {CAMADAS['valor']}` — as células da política P4 (raiz: analise)"
    )
    for titulo, path, content, disk, nome, esperada in CELULAS:
        s = vgp_check(touring, path, content, disk, CAMADAS["valor"])
        if "_erro" in s:
            afirma(False, titulo, s["_erro"])
            continue
        obtida = severidade_de(s, nome) if nome else None
        ok = obtida == esperada and (nome is not None or s.get("verdict") == "allow")
        ev = (
            f"verdict {s.get('verdict')} · layer {s.get('layer')} · {s.get('elapsed_ms')} ms · "
            + "; ".join(
                f"{f.get('nome')}={f.get('severidade')}"
                for f in s.get("findings", [])[:3]
            )
        )
        afirma(ok, titulo, ev)


HOOK = [str(H / ".claude/hooks/touring-hook")]  # o --selftest troca por um stub
SOMBRA = Path(
    os.environ.get("TOURING_VGP_SOMBRA_FILE") or H / ".claude/touring/vgp/sombra.jsonl"
)


def _env_sessao_analise() -> dict[str, str]:
    """O ambiente de uma sessão Claude Code REAL no analise: raiz = analise, socket por walk-up. Herdar o env
    deste processo mandava o hook ao daemon errado (TOURING_PROJECT_ROOT=touring → {} no stdout e o erro
    "daemon is pinned" no stderr — medido 09/10, invalidava a parte [2] inteira)."""
    env = {
        k: v
        for k, v in os.environ.items()
        if k not in ("TOURING_DAEMON_SOCKET", "TOURING_DAEMON_SOCK")
    }
    env.update(CLAUDE_PROJECT_DIR=str(ANALISE), TOURING_PROJECT_ROOT=str(ANALISE))
    return env


def _rodar_hook(sub: str, payload: str) -> str:
    """stdout do hook; erro do daemon no stderr é FALHA explícita, nunca um `{}` silencioso."""
    p = subprocess.run(
        [*HOOK, sub],
        input=payload,
        capture_output=True,
        text=True,
        cwd=ANALISE,
        timeout=60,
        env=_env_sessao_analise(),
        check=False,
    )
    if '{"error"' in p.stderr:
        raise SystemExit(f"hook {sub}: o daemon recusou — {p.stderr.strip()[:300]}")
    return p.stdout


def modo_do_projeto() -> str:
    """O modo que o daemon lê: `[vgp] modo` no `.touring/touring.toml` do analise (default sombra)."""
    em_vgp = False
    for linha in (ANALISE / ".touring/touring.toml").read_text().splitlines():
        s = linha.strip()
        if s.startswith("["):
            em_vgp = s == "[vgp]"
        elif em_vgp and s.split("=")[0].strip() == "modo":
            return s.split("=", 1)[1].split("#")[0].strip().strip('"')
    return "sombra"


def chamar_hook(sub: str, tool: str, tool_input: dict, nonce: str) -> tuple[str, str]:
    """(decisão, texto) da resposta real do hook instalado; a linha do registro se acha pelo tool_use_id."""
    payload = json.dumps(
        {
            "tool_name": tool,
            "tool_input": tool_input,
            "session_id": f"prova-vgp-{nonce}",
            "tool_use_id": nonce,
            "cwd": str(ANALISE),
            "hook_event_name": "PreToolUse",
        }
    )
    stdout = _rodar_hook(sub, payload)
    try:
        out = json.loads(stdout or "{}").get("hookSpecificOutput", {})
    except json.JSONDecodeError:
        return "", stdout[:200]
    texto = out.get("permissionDecisionReason", "") + out.get("additionalContext", "")
    return out.get("permissionDecision", ""), texto


def linha_do_registro(nonce: str) -> dict | None:
    if not SOMBRA.is_file():
        return None
    for linha in reversed(SOMBRA.read_text().splitlines()[-2000:]):
        if nonce in linha:
            return json.loads(linha)
    return None


def _resposta_ok(modo: str, esperado: str, decisao: str, texto: str) -> bool:
    """Sombra: o modelo não vê NADA do VGP. Ativo: deny vira recusa etiquetada; o resto nunca nega."""
    if modo == "sombra":
        return "[VGP" not in texto
    if esperado == "deny":
        return decisao == "deny" and "[VGP · deny]" in texto
    return decisao != "deny"


def casos_dos_hooks(alvo: Path) -> list[tuple[str, str, str, dict, str, str | None]]:
    arq = str(ANALISE / "scripts/_prova_vgp_celula.py")
    return [
        (
            "pre-write · externo inventado",
            "pre-write",
            "Write",
            {"file_path": arq, "content": "import os\nos.caminho_inventado_vgp\n"},
            "deny",
            "caminho_inventado_vgp",
        ),
        (
            "pre-write · símbolo do projeto (RED de TDD)",
            "pre-write",
            "Write",
            {
                "file_path": arq,
                "content": "from scripts.memoria.grafo_memoria import gravar_inventado_vgp\n",
            },
            "warn",
            "gravar_inventado_vgp",
        ),
        (
            "pre-write · limpo",
            "pre-write",
            "Write",
            {"file_path": arq, "content": "import os\nprint(os.sep)\n"},
            "allow",
            None,
        ),
        (
            "pre-edit-prevention · externo inventado no Edit",
            "pre-edit-prevention",
            "Edit",
            {
                "file_path": str(alvo),
                "old_string": "print(os.sep)",
                "new_string": "print(os.caminho_inventado_vgp)",
            },
            "deny",
            "caminho_inventado_vgp",
        ),
        (
            "pre-bash · inline `python3 -c` externo inventado",
            "pre-bash",
            "Bash",
            {"command": 'python3 -c "import os; os.caminho_inventado_vgp"'},
            "deny",
            "caminho_inventado_vgp",
        ),
    ]


def prova_hooks(_touring: str) -> None:
    """[2] Os hooks REAIS do binário instalado, no modo que o analise declara (sombra: silêncio + registro;
    ativo: `[VGP · deny]` como recusa). Cada caso tem um tool_use_id único reencontrado no registro."""
    modo = modo_do_projeto()
    print(
        f"\n[2] hooks reais (pre-write · pre-edit-prevention · pre-bash) — modo do analise: {modo}"
    )
    base = f"{os.getpid()}-{time.monotonic_ns()}"
    alvo = ANALISE / "scripts/_prova_vgp_alvo_edit.py"
    alvo.write_text("import os\nprint(os.sep)\n")
    try:
        for i, caso in enumerate(casos_dos_hooks(alvo)):
            julgar_caso(modo, f"prova-vgp-{base}-{i}", caso)
    finally:
        alvo.unlink(missing_ok=True)


def julgar_caso(
    modo: str, nonce: str, caso: tuple[str, str, str, dict, str, str | None]
) -> None:
    """Um caso = a resposta do hook (o que o modelo vê) E a linha do registro (o que o motor julgou)."""
    titulo, sub, tool, entrada, esperado, nome = caso
    decisao, texto = chamar_hook(sub, tool, entrada, nonce)
    reg = linha_do_registro(nonce)
    nomes = {f.get("nome") for f in (reg or {}).get("findings", [])}
    ok_reg = (
        reg is not None
        and reg.get("verdict") == esperado
        and (nome is None or nome in nomes)
    )
    afirma(
        ok_reg and _resposta_ok(modo, esperado, decisao, texto),
        titulo,
        f"registro {'ausente' if reg is None else reg.get('verdict')} {sorted(map(str, nomes))[:2]} · "
        f"resposta {decisao or 'sem decisão'} {texto[:60]!r}",
    )


def prova_juizes(touring: str) -> None:
    print("\n[3] juízes do plano (exit code)")
    j2 = VGP / "diagnostics/juiz_v2.json"
    for titulo, argv in (
        (
            "V2 — juiz do motor (camada 1)",
            ["scripts/juiz_motor_vgp.py", "--touring", touring, "--json", str(j2)],
        ),
        (
            "V3 — juiz comparativo (camadas 1,2)",
            [
                "scripts/juiz_motor_vgp.py",
                "--touring",
                touring,
                "--arg-motor=--camadas",
                "--arg-motor=1,2",
                "--p95-ms",
                "1000",
                "--base",
                str(j2),
            ],
        ),
    ):
        r = subprocess.run(
            [sys.executable, *argv],
            capture_output=True,
            text=True,
            cwd=VGP,
            timeout=3600,
            check=False,
        )
        ultima = [ln for ln in (r.stdout + r.stderr).splitlines() if ln.strip()]
        afirma(
            r.returncode == 0,
            titulo,
            f"rc {r.returncode} · {(ultima[-2:] or [''])[0][:140]}",
        )


def hook_json(sub: str, tool: str, tool_input: dict, session: str, nonce: str) -> dict:
    """A resposta JSON inteira do hook instalado (para ler updatedInput/additionalContext)."""
    payload = json.dumps(
        {
            "tool_name": tool,
            "tool_input": tool_input,
            "session_id": session,
            "tool_use_id": nonce,
            "cwd": str(ANALISE),
            "hook_event_name": "PreToolUse",
        }
    )
    stdout = _rodar_hook(sub, payload)
    try:
        return json.loads(stdout or "{}").get("hookSpecificOutput", {})
    except json.JSONDecodeError:
        return {}


def _executaveis(out: dict) -> list[str]:
    cmd = (out.get("updatedInput") or {}).get("command", "")
    ini = cmd.find("--code '")
    corpo = cmd[ini + len("--code '") : -1].replace("'\\''", "'") if ini >= 0 else ""
    return [
        ln
        for ln in corpo.splitlines()
        if ln.strip() and not ln.lstrip().startswith("#")
    ]


def _rajada(base: str, cmds: list[str]) -> list[dict]:
    return [
        hook_json("cli-suggest", "Bash", {"command": c}, base, f"{base}-{i}")
        for i, c in enumerate(cmds)
    ]


def prova_g1(_touring: str) -> None:
    """[4] G1 sem replay no hook cli-suggest REAL. No fluxo real o S3 avisa na 2ª inspeção e retorna antes do
    G1 — o índice do disparo não é fixo; a prova acha o 1º disparo (controle positivo) e exige: só o comando atual
    executa, o modelo recebe o additionalContext, a inspeção seguinte não dispara (lote consumido) e, depois de um
    `touring run`, as 2 inspeções seguintes não disparam (a janela do G1 zerou). RED no 30.4.95, verde no 30.4.96."""
    print(
        "\n[4] G1 sem replay — hook cli-suggest REAL (onde vivem os gates de code mode), sessão-sonda própria"
    )
    base = f"prova-g1-{os.getpid()}-{time.monotonic_ns()}"
    cmds = [
        f"grep -{f} {w} {a}"
        for f, w, a in (
            ("n", "alpha", "scripts/__init__.py"),
            ("rn", "beta", "docs/INDEX.md"),
            ("l", "gamma", "CLAUDE.md"),
            ("c", "delta", "AGENTS.md"),
            ("w", "epsilon", "pyproject.toml"),
            ("i", "zeta", "Makefile"),
        )
    ]
    outs = _rajada(base, cmds)
    disparos = [i for i, o in enumerate(outs) if "updatedInput" in o]
    afirma(
        bool(disparos),
        "controle: o G1 dispara numa rajada real",
        f"disparos nos índices {disparos}",
    )
    if not disparos:
        return
    i = disparos[0]
    exe = _executaveis(outs[i])
    afirma(
        exe == [cmds[i]],
        f"no 1º disparo ({i + 1}ª) só o comando atual executa",
        f"executáveis {exe!r}"[:140],
    )
    afirma(
        "NÃO rodaram de novo" in outs[i].get("additionalContext", ""),
        "o modelo recebe o additionalContext",
        repr(outs[i].get("additionalContext", ""))[:90],
    )
    if i + 1 < len(outs):
        afirma(
            "updatedInput" not in outs[i + 1],
            "a inspeção seguinte não dispara: o lote foi consumido",
            f"disparos {disparos}",
        )
    # Background (caso da analise-78, 09/10 ~21:58: um `pytest … > arquivo` com run_in_background voltou com o stdout
    # de dois comandos anteriores): a mesma rajada com `run_in_background` — o disparo roda só o comando atual E
    # preserva o flag na reescrita.
    s3 = base + "-bg"
    outs_bg = [
        hook_json(
            "cli-suggest",
            "Bash",
            {"command": c, "run_in_background": True},
            s3,
            f"{s3}-{k}",
        )
        for k, c in enumerate(cmds)
    ]
    disparos_bg = [k for k, o in enumerate(outs_bg) if "updatedInput" in o]
    if disparos_bg:
        k = disparos_bg[0]
        novo = outs_bg[k].get("updatedInput") or {}
        afirma(
            _executaveis(outs_bg[k]) == [cmds[k]]
            and novo.get("run_in_background") is True,
            "background: o disparo roda só o comando atual e preserva run_in_background",
            f"executáveis {_executaveis(outs_bg[k])!r} · run_in_background={novo.get('run_in_background')!r}"[
                :140
            ],
        )
    else:
        afirma(
            False,
            "background: controle — o G1 dispara na rajada em background",
            "nenhum disparo",
        )
    s2 = base + "-run"
    _rajada(s2, cmds[:3])
    hook_json(
        "cli-suggest",
        "Bash",
        {"command": "touring run --lang bash --code 'true'"},
        s2,
        f"{s2}-r",
    )
    depois = [
        hook_json("cli-suggest", "Bash", {"command": c}, s2, f"{s2}-d{k}")
        for k, c in enumerate(cmds[3:5])
    ]
    afirma(
        not any("updatedInput" in o for o in depois),
        "depois de um touring run, 2 inspeções não disparam o G1",
        str([sorted(o) for o in depois])[:120],
    )


STUB_HOOK = """
import json, os, sys
p = json.load(sys.stdin)
txt = json.dumps(p["tool_input"])
nome = next((n for n in ("caminho_inventado_vgp", "gravar_inventado_vgp") if n in txt), None)
v = {"caminho_inventado_vgp": "deny", "gravar_inventado_vgp": "warn"}.get(nome, "allow")
mut = os.environ.get("STUB_MUTANTE", "")
if mut == "cego":
    v, nome = "allow", None
if mut != "sem_registro":
    with open(os.environ["TOURING_VGP_SOMBRA_FILE"], "a") as f:
        achados = [{"nome": nome}] if nome else []
        f.write(json.dumps({"tool_use_id": p["tool_use_id"], "verdict": v, "findings": achados}) + "\\n")
vaza = mut == "vaza" and v != "allow"
if (os.environ.get("STUB_MODO") == "ativo" and v == "deny") or vaza:
    out = {"permissionDecision": "deny" if v == "deny" else "", "permissionDecisionReason": f"[VGP · {v}] x"}
    print(json.dumps({"hookSpecificOutput": out}))
"""


def selftest() -> int:
    """O arnês da parte [2] tem de APROVAR o motor certo e REPROVAR cada defeito — senão a prova não prova."""
    global HOOK, SOMBRA
    hook_real, sombra_real = HOOK, SOMBRA
    cenarios = (  # (modo, mutante, falhas esperadas entre os 5 casos)
        ("sombra", "", 0),
        ("ativo", "", 0),
        ("sombra", "cego", 4),  # motor que não vê nada: deny×3 e warn×1 caem
        (
            "sombra",
            "sem_registro",
            5,
        ),  # hook que não grava o registro: tudo cai, até o limpo
        ("sombra", "vaza", 4),  # sombra que vaza [VGP para o modelo
    )
    ruins = []
    with tempfile.TemporaryDirectory() as d:
        stub = Path(d) / "stub_hook.py"
        stub.write_text(STUB_HOOK)
        HOOK = [sys.executable, str(stub)]
        alvo = Path(d) / "alvo.py"
        try:
            for modo, mut, esperado in cenarios:
                SOMBRA = Path(d) / f"sombra_{modo}_{mut or 'bom'}.jsonl"
                os.environ.update(
                    TOURING_VGP_SOMBRA_FILE=str(SOMBRA),
                    STUB_MODO=modo,
                    STUB_MUTANTE=mut,
                )
                antes = len(FALHAS)
                for i, caso in enumerate(casos_dos_hooks(alvo)):
                    julgar_caso(modo, f"selftest-{modo}-{mut}-{i}", caso)
                obtido = len(FALHAS) - antes
                del FALHAS[antes:]
                marca = "ok" if obtido == esperado else "ERRADO"
                print(
                    f"  selftest {modo}/{mut or 'bom'}: {obtido} falha(s), esperado {esperado} — {marca}"
                )
                if obtido != esperado:
                    ruins.append((modo, mut, obtido, esperado))
        finally:
            HOOK, SOMBRA = hook_real, sombra_real
            for k in ("TOURING_VGP_SOMBRA_FILE", "STUB_MODO", "STUB_MUTANTE"):
                os.environ.pop(k, None)
    print(f"selftest: {len(cenarios) - len(ruins)}/{len(cenarios)}")
    return 0 if not ruins else 1


def main() -> int:
    ap = argparse.ArgumentParser(
        description="Prova prática do motor VGP (binário instalado)"
    )
    ap.add_argument("--touring", default=shutil.which("touring") or "touring")
    ap.add_argument("--so", default="1,2,3,4")
    ap.add_argument(
        "--selftest",
        action="store_true",
        help="o arnês da parte [2] aprova e reprova (stub + mutantes)",
    )
    ap.add_argument(
        "--camadas",
        default="1,2",
        help="1 = só a camada nativa; 1,2 = com o verificador",
    )
    a = ap.parse_args()
    if a.selftest:
        return selftest()
    CAMADAS["valor"] = a.camadas
    so = set(a.so.split(","))
    for k, fn in (
        ("1", prova_celulas),
        ("2", prova_hooks),
        ("3", prova_juizes),
        ("4", prova_g1),
    ):
        if k in so:
            fn(a.touring)
    print(f"\nveredito: {'TUDO AFIRMADO' if not FALHAS else f'{len(FALHAS)} FALHA(S)'}")
    return 0 if not FALHAS else 1


if __name__ == "__main__":
    sys.exit(main())
