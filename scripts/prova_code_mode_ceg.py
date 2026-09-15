#!/usr/bin/env python3
"""PROVA PRÁTICA — code mode + CEG sandbox, contra o binário DEPLOYADO.

Nada aqui lê o código-fonte nem consulta um teste unitário: toda linha executa o
`touring` instalado e julga o que ele DEVOLVEU. É a distinção que este workspace
já pagou para aprender — teste do componente não é teste do caminho, e rótulo de
versão não prova build.

Uso:  python3 scripts/prova_code_mode_ceg.py [-v]
Exit: 0 se toda asserção passar; 1 caso contrário.
"""
from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path

TOURING = os.environ.get("TOURING_BIN", "touring")
WS = Path("/home/gabrielgadea/projects/touring")
VERBOSE = "-v" in sys.argv

resultados: list[tuple[bool, str, str]] = []


def afirma(ok: bool, titulo: str, evidencia: str) -> None:
    resultados.append((bool(ok), titulo, evidencia))
    if VERBOSE:
        print(f"  {'PASS' if ok else 'FALHA'}  {titulo}\n         {evidencia}")


def run(args: list[str], stdin: str | None = None, timeout: int = 120):
    """Executa o touring instalado e devolve (rc, stdout, stderr)."""
    env = dict(os.environ)
    env.pop("TOURING_CODE_MODE", None)
    env.pop("TOURING_GATE_OK", None)
    p = subprocess.run([TOURING, *args], input=stdin, capture_output=True,
                       text=True, cwd=str(WS), env=env, timeout=timeout)
    return p.returncode, p.stdout, p.stderr


def run_json(args: list[str], **kw):
    """Como `run`, mas devolve o JSON do resultado (ou None se negado)."""
    rc, out, err = run(args, **kw)
    try:
        return json.loads(out)
    except json.JSONDecodeError:
        return None


def sandbox(code: str, lang: str = "bash", extra: list[str] | None = None):
    return run_json(["run", "--lang", lang, "--file", "/dev/stdin", *(extra or [])], stdin=code)


def negado(code: str, lang: str = "bash") -> tuple[bool, str]:
    """(foi negado?, mensagem do deny)"""
    rc, out, err = run(["run", "--lang", lang, "--file", "/dev/stdin"], stdin=code)
    return (rc != 0 and "denied this run" in err), err.strip()[-220:]


# ══ 1. CEG — o gate DISCRIMINA (o núcleo do conserto de hoje) ═══════════════

def prova_ceg_discrimina():
    print("\n[1] CEG — o gate discrimina benigno de perigoso")

    # 1a. builtins e leitura comum passam LIMPOS (sem advisory = sem ruído)
    limpos, sujos = [], []
    for c in ("echo oi", "cd /tmp && pwd", "ls -la", "pwd", "true",
              "grep -c fn Cargo.toml", "export A=1", "printf x",
              "test -f Cargo.toml", "for i in 1 2; do echo $i; done"):
        r = sandbox(c)
        (limpos if r and "ceg_advisory" not in r else sujos).append(c)
    afirma(len(sujos) == 0,
           "10 comandos benignos, ZERO advisory",
           f"limpos={len(limpos)}/10, com ruído={sujos}")

    # 1b. rede NEGA duro — em shell E em python (a assimetria que fechamos)
    for lang, code, rotulo in (("bash", "curl -s -m 5 https://example.com", "curl/shell"),
                               ("python", "import socket\nsocket.create_connection(('1.1.1.1',53),timeout=4)", "socket/python")):
        ok, msg = negado(code, lang)
        afirma(ok and "network" in msg, f"rede NEGA duro — {rotulo}", msg[-120:])

    # 1c. e a rede realmente NÃO acontece (o deny não é decorativo)
    rc, out, err = run(["run", "--lang", "bash", "--file", "/dev/stdin"],
                       stdin='curl -s -m 5 -o /dev/null -w "HTTP=%{http_code}" https://example.com')
    afirma("HTTP=200" not in out, "nenhuma resposta HTTP atravessou o gate",
           f"stdout={out.strip()[:80] or '(vazio)'}")

    # 1d. padrão destrutivo NEGA, mesmo com subprocess junto
    ok, msg = negado("rm -rf /tmp/prova-ceg-zz")
    afirma(ok, "padrão destrutivo (rm -rf) NEGA", msg[-120:])

    # 1e. escrita fora do escopo NEGA
    ok, msg = negado("echo x > /home/gabrielgadea/.ssh/prova_ceg")
    afirma(ok and "file-write" in msg, "escrita fora do workspace NEGA", msg[-120:])

    # 1e-bis. idiomas de shell que NÃO podem virar deny: `2>&1` e `2>/dev/null`
    # apareciam como subprocess `1` + fs-write e negavam duro um comando comum.
    for c, rot in (("ls 2>&1", "2>&1"), ("ls 2>/dev/null", "2>/dev/null"),
                   ("ls 2>&1 | head -3", "2>&1 | pipe")):
        r = sandbox(c)
        afirma(r is not None and r.get("exit_code") == 0,
               f"idioma de shell `{rot}` não vira deny", f"exit={r and r.get('exit_code')}")

    # 1f. spawn legítimo passa, sob advisory declarado (waiver consciente)
    r = sandbox('python3 -c "print(42)"')
    nota = (r or {}).get("ceg_advisory", {}).get("note", "")
    afirma(r is not None and r.get("exit_code") == 0 and "subprocess-only" in nota,
           "spawn real passa sob advisory que DECLARA ser seletivo",
           f"exit={r and r.get('exit_code')} nota={nota[:70]}")


# ══ 2. CEG — a contenção que o waiver pressupõe EXISTE ═════════════════════

def prova_contencao():
    print("\n[2] CEG — a contenção do filesystem é real")
    fora = Path("/home/gabrielgadea/.ssh/prova_landlock")
    dentro = WS / ".prova_landlock_ws"
    for p in (fora, dentro):
        p.unlink(missing_ok=True)

    r = sandbox(f"touch {fora}")
    afirma(not fora.exists(), "escrita FORA do workspace não chega ao host",
           f"exit={r and r.get('exit_code')} existe={fora.exists()}")

    r = sandbox(f"touch {dentro}")
    afirma(dentro.exists(), "escrita DENTRO do workspace funciona (não é bloqueio cego)",
           f"exit={r and r.get('exit_code')} existe={dentro.exists()}")
    dentro.unlink(missing_ok=True)


# ══ 2b. SEG-2 — superfície de instrução legível, segredos de ~/.claude não ══

def prova_instrucao():
    print("\n[2b] SEG-2 — skills/rules legíveis no sandbox; settings/credenciais não")
    home = Path.home()

    skill = home / ".claude/skills/Touring/SKILL.md"
    if skill.exists():
        r = sandbox(f"head -1 {skill}")
        afirma(r is not None and r.get("exit_code") == 0 and (r.get("stdout") or "").strip() != "",
               "estrutura de skills é LEGÍVEL no sandbox (o caso da sessão analise)",
               f"exit={r and r.get('exit_code')} stdout={(r or {}).get('stdout', '')[:60]!r}")

    r = sandbox(f"ls {home / '.claude/rules'}")
    afirma(r is not None and r.get("exit_code") == 0,
           "~/.claude/rules é legível no sandbox",
           f"exit={r and r.get('exit_code')}")

    settings = home / ".claude/settings.json"
    if settings.exists():
        r = sandbox(f"cat {settings}")
        afirma(r is not None and r.get("exit_code") != 0 and (r.get("stdout") or "") == "",
               "settings.json (env com chaves) CONTINUA inalcançável",
               f"exit={r and r.get('exit_code')} stdout_len={len((r or {}).get('stdout', ''))}")

    # Predicado POSITIVO, no idioma que enganou (sonda da sessão analise-94,
    # 30/08): `pathlib.rglob` ENGOLE PermissionError e devolve 0 sem exceção
    # enquanto `exists()` dá True — um aceite "não levantou" passa com o bug
    # presente. Só a contagem > 0 prova o grant.
    if (home / ".claude/skills").exists():
        r = sandbox(
            "from pathlib import Path\n"
            "print(len(list((Path.home() / '.claude/skills').rglob('*.md'))))",
            lang="python")
        n = int((r or {}).get("stdout", "0").strip() or 0) if r else 0
        afirma(n > 0,
               "rglob em ~/.claude/skills devolve > 0 (o vazio silencioso morreu)",
               f"exit={r and r.get('exit_code')} md_count={n}")


# ══ 3. Code mode — o transporte ════════════════════════════════════════════

def prova_transporte():
    print("\n[3] Code mode — transporte sem quoting (S1)")
    corpo = ("s = 'aspas simples'; d = \"aspas duplas\"\n"
             "print('OK', s, d, \"$HOME `date` \\\\ barra\")\n")
    with tempfile.NamedTemporaryFile("w", suffix=".py", delete=False) as f:
        f.write(corpo)
        caminho = f.name
    try:
        r = run_json(["run", "--lang", "python", "--file", caminho])
        afirma(r and r.get("exit_code") == 0 and "OK" in r.get("stdout", ""),
               "--file transporta corpo hostil a quoting", (r or {}).get("stdout", "").strip()[:80])
        r = run_json(["run", "--lang", "python", "--file", "/dev/stdin"], stdin=corpo)
        afirma(r and r.get("exit_code") == 0 and "OK" in r.get("stdout", ""),
               "--stdin idem", (r or {}).get("stdout", "").strip()[:80])
    finally:
        os.unlink(caminho)

    rc, out, err = run(["run", "--lang", "python"])
    afirma(rc != 0 and "--code, --file, or --stdin" in err,
           "sem corpo, o erro ENSINA a rota", err.strip()[-90:])


# ══ 4. Code mode — a superfície do SDK (S4) ════════════════════════════════

def prova_superficie():
    print("\n[4] Code mode — 71 hooks alcançáveis (S4)")
    programa = (
        "meta = touring.ast_meta('Cargo.toml')\n"
        "gm = touring.query('cli-gate-metrics', {})\n"
        "try:\n"
        "    touring.query('cli-index-rebuild', {}); mut = 'PASSOU'\n"
        "except RuntimeError:\n"
        "    mut = 'negado'\n"
        "print(json.dumps({'hooks': len(touring.READONLY_HOOKS),\n"
        "                  'ast_meta_ok': meta is not None,\n"
        "                  'query_ok': isinstance(gm, dict),\n"
        "                  'mutante': mut}))\n"
    )
    r = run_json(["run", "--lang", "python", "--orchestrate", "--file", "/dev/stdin"],
                 stdin="import json\n" + programa)
    try:
        d = json.loads((r or {}).get("stdout", "{}").strip().splitlines()[-1])
    except Exception:
        d = {}
    afirma(d.get("hooks", 0) >= 60, f"allowlist com {d.get('hooks')} hooks (eram 8)", str(d.get("hooks")))
    afirma(d.get("ast_meta_ok") is True, "atalho tipado NOVO (ast_meta) funciona", str(d.get("ast_meta_ok")))
    afirma(d.get("query_ok") is True, "query() alcança hook fora dos 8 originais", str(d.get("query_ok")))
    afirma(d.get("mutante") == "negado", "o guard do SDK NEGA hook mutante", str(d.get("mutante")))

    rc, out, _ = run(["run", "--lang", "python", "--sdk-stub"])
    metodos = out.count("    def ")
    afirma(rc == 0 and metodos >= 15, f"stub GERADO declara {metodos} métodos", f"linhas={len(out.splitlines())}")
    afirma("STATIC STUB" in out and str(d.get("hooks", 0)) in out,
           "o stub declara o número de hooks DERIVADO da allowlist", f"{d.get('hooks')} presente no texto")


# ══ 4b. A allowlist é FRONTEIRA, não só contrato ═══════════════════════════

def prova_fronteira():
    print("\n[4b] SDK — a allowlist é imposta pelo DAEMON, não pelo cliente")
    # `touring.READONLY_HOOKS` é um atributo Python mutável. Até 27/08/2026 um
    # programa no sandbox o reescrevia e chamava qualquer hook — provado ao
    # vivo gravando em `cli-memory-store`. A fronteira agora é server-side.
    programa = (
        "touring.READONLY_HOOKS = touring.READONLY_HOOKS + ('cli-memory-store',)\n"
        "try:\n"
        "    touring.query('cli-memory-store', {'key': 'prova-fronteira', 'value': 'x'})\n"
        "    print('BYPASS')\n"
        "except RuntimeError as e:\n"
        "    print('RECUSADO:', str(e)[:200])\n"
    )
    r = run_json(["run", "--lang", "python", "--orchestrate", "--file", "/dev/stdin"],
                 stdin=programa)
    saida = (r or {}).get("stdout", "").strip()
    afirma(saida.startswith("RECUSADO"),
           "mutar a allowlist do cliente NÃO abre o hook mutante", saida[:110])
    afirma("allowlist" in saida and "cli-memory-store" in saida,
           "a razão do daemon VIAJA até o programa (deny que ensina)", saida[:130])

    r = run_json(["run", "--lang", "python", "--orchestrate", "--file", "/dev/stdin"],
                 stdin="print('leitura:', touring.index_find('CodeModePresentation') is not None)\n")
    afirma("True" in (r or {}).get("stdout", ""),
           "a leitura legítima continua passando (não é bloqueio cego)",
           (r or {}).get("stdout", "").strip()[:60])


# ══ 5. Code mode — o piso do --brief (S7) ══════════════════════════════════

def prova_brief():
    print("\n[5] Code mode — piso do --brief (S7)")
    r = run_json(["run", "--lang", "bash", "--file", "/dev/stdin", "--brief"],
                 stdin='printf "a\\nb\\nc\\n"')
    afirma(r is not None and "brief_skipped" in r and r.get("stdout") == "a\nb\nc\n",
           "saída curta volta ÍNTEGRA e declara que não resumiu",
           json.dumps((r or {}).get("brief_skipped", {})))

    r = run_json(["run", "--lang", "bash", "--file", "/dev/stdin", "--brief"],
                 stdin='i=1; while [ $i -le 250 ]; do echo "linha $i"; i=$((i+1)); done')
    afirma(r is not None and r.get("elided_lines", 0) > 200,
           "saída longa resume e DECLARA o que elidiu",
           f"elided_lines={(r or {}).get('elided_lines')}")


# ══ 6. Code mode — observabilidade (S5) ════════════════════════════════════

def prova_journal():
    print("\n[6] Code mode — par start/settle no journal (S5)")
    j = Path.home() / ".claude/touring/run_subcalls.jsonl"
    antes = j.stat().st_size if j.exists() else 0
    r = run_json(["run", "--lang", "python", "--orchestrate", "--file", "/dev/stdin"],
                 stdin="for _ in range(3):\n    touring.index_find('CodeModePresentation')\nprint('ok')\n")
    run_id = (r or {}).get("run_id", "")
    novos = []
    if j.exists():
        with j.open() as f:
            f.seek(antes)
            for linha in f:
                try:
                    novos.append(json.loads(linha))
                except json.JSONDecodeError:
                    pass
    meus = [n for n in novos if run_id and run_id in str(n.get("origin", ""))]
    starts = [n for n in meus if n.get("phase") == "start"]
    settles = [n for n in meus if n.get("phase") == "settle"]
    afirma(len(starts) == 3 and len(settles) == 3,
           "3 sub-chamadas produzem 3 pares start/settle",
           f"start={len(starts)} settle={len(settles)} run_id={run_id[:24]}")
    afirma(all("duration_ms" in s and "ok" in s for s in settles),
           "cada settle carrega duração e desfecho",
           str([{k: s[k] for k in ("hook", "duration_ms", "ok")} for s in settles[:1]]))
    proibidos = {"output", "preview", "content", "body", "result"}
    vazou = [k for n in meus for k in n if k in proibidos]
    afirma(not vazou, "nenhum campo de CONTEÚDO no journal (política de segredo)",
           f"campos={sorted({k for n in meus for k in n})}")


# ══ 7. Code mode — o gate de rajada (S3) ═══════════════════════════════════

def prova_rajada():
    print("\n[7] Code mode — predicado de rajada (S3)")
    hook = Path.home() / ".local/bin/touring-hook"
    if not hook.exists():
        afirma(False, "touring-hook instalado", str(hook))
        return

    def pre(cmd: str, sessao: str):
        payload = json.dumps({"tool_name": "Bash", "session_id": sessao,
                              "cwd": str(WS), "tool_input": {"command": cmd}})
        p = subprocess.run([str(hook), "cli-suggest"], input=payload,
                           capture_output=True, text=True, cwd=str(WS), timeout=60)
        try:
            v = json.loads(p.stdout or "{}")
        except json.JSONDecodeError:
            return ""
        return v.get("hookSpecificOutput", {}).get("permissionDecisionReason", "")

    # O ledger de rajada é chaveado por (project_root, SESSÃO, classe), janela de
    # 300s (`inspect_burst_key`, cli_suggester.rs). Esta prova dizia "NÃO por
    # sessão" e zerava o ledger com um `touring run` na sessão "prova-reset" —
    # que não toca as sessões da prova. O `cat` que passava numa execução ficava
    # contado 300s, a execução seguinte o herdava, e o deny apagava a chave: as
    # execuções alternavam reprova/passa (medido 13/09/2026, na propagação da
    # 30.4.41, com retry reprovando pela mesma herança). Sessões com nonce por
    # execução: nenhuma herda de outra, por construção.
    nonce = f"{os.getpid()}-{time.monotonic_ns()}"

    s = f"prova-rajada-1-{nonce}"
    r1 = pre("grep -rn alfa src/", s)
    afirma("CODE MODE · rajada" not in r1, "1ª inspeção da classe PASSA (o caso comum não é taxado)",
           r1[:70] or "(sem deny)")
    r2 = pre("grep -rn beta src/", s)
    afirma("CODE MODE · rajada" in r2, "2ª da mesma classe é NEGADA", r2[:70])
    afirma("alfa" in r2 and "beta" in r2, "o deny entrega as DUAS chamadas fundidas num programa",
           "ambos os comandos presentes na rota" if ("alfa" in r2 and "beta" in r2) else r2[:90])
    r3 = pre("cat README.md", f"prova-rajada-2-{nonce}")
    afirma("CODE MODE · rajada" not in r3, "classe diferente não soma à rajada", r3[:70] or "(sem deny)")
    # A rota seguida zera a janela DAQUELA sessão: a próxima inspeção passa.
    pre("grep -rn gama src/", s)
    pre("touring run --lang bash --code 'true'", s)
    r4 = pre("grep -rn delta src/", s)
    afirma("CODE MODE · rajada" not in r4, "um `touring run` na mesma sessão zera a janela",
           r4[:70] or "(sem deny)")


# ══ 8. O T3-B foi enterrado (S10) ══════════════════════════════════════════

def prova_t3_enterrado():
    print("\n[8] T3-B enterrado (S10)")
    rc, out, _ = run(["gate-metrics", "-j"])
    try:
        m = json.loads(out)
    except json.JSONDecodeError:
        m = {}
    afirma(not any(k.startswith("t3_turn") for k in m),
           "nenhum counter t3_turn_* no binário deployado",
           f"counters g1_inspect presentes={[k for k in m if 'g1_inspect' in k]}")
    afirma(any(k.startswith("g1_inspect") for k in m),
           "os counters do S3 (que o substituiu) EXISTEM",
           str({k: v for k, v in m.items() if "g1_inspect" in k}))


def main() -> int:
    print("=" * 74)
    print("PROVA PRÁTICA — code mode + CEG sandbox contra o binário DEPLOYADO")
    rc, _, ver = run(["--version"])
    print(f"binário: {TOURING}  |  {ver.strip() or '(versão em stderr)'}")
    print("=" * 74)

    for f in (prova_ceg_discrimina, prova_contencao, prova_instrucao, prova_transporte,
              prova_superficie, prova_fronteira, prova_brief, prova_journal, prova_rajada,
              prova_t3_enterrado):
        try:
            f()
        except Exception as e:  # uma prova que explode é uma prova que falhou
            afirma(False, f"{f.__name__} lançou {type(e).__name__}", str(e)[:160])
        if not VERBOSE:
            print("    " + "".join("." if ok else "F" for ok, _, _ in resultados[-9:]))

    print("\n" + "=" * 74)
    falhas = [(t, e) for ok, t, e in resultados if not ok]
    for t, e in falhas:
        print(f"  FALHA: {t}\n         {e}")
    print(f"RESULTADO: {len(resultados) - len(falhas)}/{len(resultados)} asserções passaram")
    print("=" * 74)
    return 1 if falhas else 0


if __name__ == "__main__":
    sys.exit(main())
