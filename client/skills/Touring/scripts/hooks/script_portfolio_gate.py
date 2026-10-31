#!/usr/bin/env python3
# #tags: kind:script purpose:gate domain:code-mode process:measure lang:python
"""script_portfolio_gate.py — PreToolUse (Write): consulta-antes-de-criar para scripts. SOMBRA.

Origem: diagnóstico 27/10/2026 (docs/plans/2026-10-27-portfolio-scripts/) — 41.923 eventos de
criação de script em 2.420 transcripts, 81,2% efêmeros/inline, duplicata exata 1,0% mas recriação
SEMÂNTICA massiva (probe.py reescrito em 14 sessões, mutantes.py em 18). `--harvest` 0×. O gap é
G1: nenhum passo "procurar no portfólio antes de escrever". Este hook É esse passo (D8: o
enforcement mora no executor/hook, não no anúncio).

Dispara quando o Write cria um script (.py/.sh/.js/.mjs/.ts/.rb/.pl) E:
  (a) o destino é efêmero (/tmp, /var/tmp, .cache, scratchpad, tool-results), OU
  (b) o basename é genérico (medido: probe/dump/fetch/build/show/final/ver/verifica/recalc/…).

A consulta, em ordem de custo (fail-open em cada degrau, teto total ~3s):
  1. `~/.claude/scripts-portfolio/MANIFEST.json` (P1) — registro tipado local, <1ms;
  2. `touring tantivy search "<stem>" 3` — índice BM25 (memória companion inclui snippets).

Sombra = nunca bloqueia: emite additionalContext com os 3 candidatos mais próximos e grava
`~/.claude/touring/portfolio_gate/events.jsonl` (um JSON por evento) para medir quantas criações
seriam evitadas (`would_avoid`: candidato com mesmo basename OU mesma finalidade presente).

Kill switch humano: `TOURING_PORTFOLIO_GATE_DISABLED=1`.
CLI: `--stats` resume o events.jsonl (base para o critério de promoção, à la prose gate).
"""

from __future__ import annotations

import json
import os
import re
import subprocess
import sys
import time
from pathlib import Path

SCRIPT_EXTS = {".py", ".sh", ".js", ".mjs", ".ts", ".rb", ".pl", ".bash"}
EPHEMERAL_HINTS = (
    "/tmp/",
    "/var/tmp/",
    "/.cache/",
    "/scratchpad",
    "/tool-results/",
    "/pytest-of-",
)
# Basenames genéricos medidos como campeões de recriação (mineracao 27/10, >=3 sessões).
GENERIC_BASENAMES = {
    "probe.py",
    "probe2.py",
    "dump.py",
    "fetch.py",
    "build.py",
    "show.py",
    "final.py",
    "ver.py",
    "verifica.py",
    "recalc.py",
    "mutantes.py",
    "mutar.py",
    "mutacao.py",
    "extrai.py",
    "links.py",
    "run.sh",
    "rodar.sh",
    "final.sh",
    "s1.py",
    "b1.py",
    "estrutura.py",
    "ctx.py",
    "sonda2.py",
}
PORTFOLIO = Path.home() / ".claude" / "scripts-portfolio"
MANIFEST = PORTFOLIO / "MANIFEST.json"
EVENTS = Path.home() / ".claude" / "touring" / "portfolio_gate" / "events.jsonl"
#: Path prefixes of the measurement instruments (latencia_hook_portfolio.py, auditar_plano_por_execucao.py).
SINTETICOS = ("/tmp/medicao-latencia-hook/", "/tmp/auditoria-portfolio/")
STOPWORDS = {"the", "and", "for", "with", "def", "import", "from", "print", "return"}
#: Declared active plan (decision of Gabriel, 31/10/2026): ``<project>/.claude/plano_ativo.json`` =
#: ``{"versao": 1, "bundle": "docs/plans/<dir>"}``, searched upward from the event cwd (like ``federacao.achar_indice``).
PLANO_ATIVO_REL = Path(".claude") / "plano_ativo.json"
PLANO_ATIVO_VERSAO = 1
#: The bundle value reaches the model's context: one level under docs/plans, a closed alphabet, a length cap.
BUNDLE_RE = re.compile(r"docs/plans/[A-Za-z0-9._-]+")
BUNDLE_MAX = 120


def is_ephemeral(path: str) -> bool:
    """Destino efêmero: morre com a sessão (tmpfs, cache, scratchpad)."""
    low = path.lower()
    return low.startswith(("/tmp", "/var/tmp")) or any(h in low for h in EPHEMERAL_HINTS)


def keywords_of(basename: str, head: str) -> list[str]:
    """Palavras-chave da busca: stem do basename + tokens raros do cabeçalho/docstring."""
    stem = Path(basename).stem.replace("_", " ").replace("-", " ")
    toks = [t for t in stem.split() if len(t) > 2 and t not in STOPWORDS]
    for tok in head[:600].split():
        t = "".join(c for c in tok if c.isalnum())
        if len(t) > 5 and t.lower() not in STOPWORDS and not t.isdigit():
            toks.append(t)
        if len(toks) >= 6:
            break
    return toks[:6]


def manifest_candidates(keys: list[str]) -> list[dict]:
    """Candidatos do portfólio tipado (P1) por interseção de propósito/tags/nome."""
    try:
        entries = json.loads(MANIFEST.read_text()).get("scripts", [])
    except (OSError, json.JSONDecodeError):
        return []
    out = []
    for e in entries:
        hay = " ".join(str(e.get(k, "")) for k in ("slug", "purpose", "domain", "tags")).lower()
        hits = sum(1 for k in keys if k.lower() in hay)
        if hits:
            out.append(
                {
                    "path": str(PORTFOLIO / e.get("path", "")),
                    "why": e.get("purpose", ""),
                    "hits": hits,
                }
            )
    out.sort(key=lambda c: -c["hits"])
    return out[:3]


def tantivy_candidates(query: str) -> list[dict]:
    """Candidatos do índice BM25 (scripts de planos + snippets companion)."""
    try:
        p = subprocess.run(
            ["touring", "tantivy", "search", query, "3"],
            capture_output=True,
            text=True,
            timeout=2.5,
        )
        rows = json.loads(p.stdout) if p.returncode == 0 else []
    except (OSError, subprocess.TimeoutExpired, json.JSONDecodeError):
        return []
    out = []
    for r in rows if isinstance(rows, list) else []:
        fp = str(r.get("file_path", ""))
        if fp and Path(fp).suffix.lower() in SCRIPT_EXTS:
            out.append({"path": fp, "why": str(r.get("symbol_name", ""))[:80], "hits": 1})
    return out[:3]


def emit(payload: dict) -> None:
    """Resposta PreToolUse canônica; sombra nunca sai != 0."""
    print(
        json.dumps(
            {
                "hookSpecificOutput": {
                    "hookEventName": "PreToolUse",
                    "additionalContext": payload["context"],
                }
            }
        )
    )


def log_event(event: dict) -> None:
    """Um JSON por linha; falha de I/O nunca derruba o hook."""
    try:
        EVENTS.parent.mkdir(parents=True, exist_ok=True)
        with EVENTS.open("a") as fh:
            fh.write(json.dumps(event, ensure_ascii=False) + "\n")
    except OSError:
        pass


def stats() -> int:
    """Resumo do events.jsonl: volume, candidatos e, desde a P1 (28/10/2026), a contagem por estado do veredito.

    Eventos anteriores à P1 não têm ``estado``: entram como ``legado`` (visíveis, nunca somados a um estado).
    Eventos de medição (``SINTETICOS``: corpos do corpus disparados por instrumento) ficam fora dos totais e
    aparecem só como ``sinteticos`` — a sombra do P5 mede escrita real, não a própria medição.
    """
    try:
        todos = [json.loads(linha) for linha in EVENTS.read_text().splitlines() if linha.strip()]
    except (OSError, json.JSONDecodeError):
        print("sem eventos")
        return 0
    rows = [r for r in todos if not str(r.get("file", "")).startswith(SINTETICOS)]
    n = len(rows)
    com_cand = sum(1 for r in rows if r.get("n_candidates"))
    evit = sum(1 for r in rows if r.get("would_avoid") or r.get("estado") == "reprovado")
    por_estado: dict[str, int] = {}
    for r in rows:
        chave = r.get("estado", "legado")
        por_estado[chave] = por_estado.get(chave, 0) + 1
    print(
        json.dumps(
            {
                "eventos": n,
                "com_candidato": com_cand,
                "would_avoid": evit,
                "pct_evitaveis": round(100 * evit / n, 1) if n else 0.0,
                "por_estado": por_estado,
                "sinteticos": len(todos) - n,
            },
            ensure_ascii=False,
        )
    )
    return 0


def _recuperar():
    """The single portfolio decision (``../portfolio/recuperar.py``), the same code the CLI and evaluator run."""
    import importlib.util

    caminho = Path(__file__).resolve().parent.parent / "portfolio" / "recuperar.py"
    spec = importlib.util.spec_from_file_location("portfolio_recuperar", caminho)
    if spec is None or spec.loader is None:
        raise ImportError(f"recuperar.py ausente em {caminho}")
    mod = importlib.util.module_from_spec(spec)
    sys.modules["portfolio_recuperar"] = mod
    spec.loader.exec_module(mod)
    return mod


_PALAVRA_ESTADO = {
    "reprovado": "já existe equivalente de CORPO (cópia editada)",
    "aprovado": "nada equivalente; candidatos só por propósito/nome",
    "inaferivel": "a consulta ficou INCOMPLETA",
}


def _contexto(v, basename: str, ephemeral: bool) -> str:
    lista = " · ".join(f"{cid} ({sinal}{'' if d is None else f', {d} bits'})" for cid, sinal, d in v.candidatos)
    destino = "destino EFÊMERO (morre com a sessão)" if ephemeral else "basename genérico recriado entre sessões"
    return (
        f"[PORTFOLIO · consulta-antes-de-criar · {v.estado}] {basename}: {destino}. "
        f"{_PALAVRA_ESTADO.get(v.estado, v.estado)} — {v.motivo}. Candidatos: {lista}. "
        f"Antes de recriar: leia o candidato e estenda; se o script for reutilizável, grave em "
        f"~/.claude/scripts-portfolio/ (contrato no MANIFEST.json) ou `touring run --harvest <slug>`. "
        f"Consulta manual: python3 ~/.claude/skills/Touring/scripts/portfolio/recuperar.py --arquivo <script>."
    )


def injection_allowed() -> bool:
    """Interruptor de injeção de contexto (26/10/2026) — client/hooks/injection_policy.py.

    Fail-open (padrão evidence_guard.py): política ausente ou ilegível NUNCA cala o hook.
    """
    mod_path = Path.home() / ".claude/hooks/injection_policy.py"
    if not mod_path.exists():
        return True
    try:
        import importlib.util

        spec = importlib.util.spec_from_file_location("injection_policy", mod_path)
        if spec is None or spec.loader is None:
            return True
        mod = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(mod)
        return bool(mod.allows("plugin-hook"))
    except Exception:
        return True


def _vigiado(path: str, content: str) -> tuple[str, str, bool, bool] | None:
    """(path, content, ephemeral, generic) when the destination is a script the gate watches; else None."""
    if Path(path).suffix.lower() not in SCRIPT_EXTS:
        return None
    ephemeral = is_ephemeral(path)
    generic = Path(path).name in GENERIC_BASENAMES
    if not (ephemeral or generic):
        return None
    return path, content, ephemeral, generic


def _heredoc():
    """The single heredoc parser (``../portfolio/heredoc.py``, P3), shared with the analise inventory."""
    import importlib.util

    caminho = Path(__file__).resolve().parent.parent / "portfolio" / "heredoc.py"
    spec = importlib.util.spec_from_file_location("portfolio_heredoc", caminho)
    if spec is None or spec.loader is None:
        raise ImportError(f"heredoc.py ausente em {caminho}")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def _absoluto(path: str, cwd: str | None) -> str:
    """A heredoc target as written (``~/x.py``, ``x.py``) resolved against the command's cwd; ``$VAR`` stays."""
    p = os.path.expanduser(path)
    return p if os.path.isabs(p) or not cwd or p.startswith("$") else os.path.join(cwd, p)


def _gatilhos(payload: dict) -> list[tuple[str, str, bool, bool, str]]:
    """Every watched script this tool call creates, with its door (``Write`` or ``Bash.heredoc``).

    Bash (P3, 28/10/2026): the command is parsed by the shared heredoc parser; the substring check first keeps
    the common Bash call (no heredoc) away from any import or regex cost — the hook runs on every Bash call.
    """
    ti = payload.get("tool_input") or {}
    if payload.get("tool_name") == "Bash":
        comando = str(ti.get("command", ""))
        if "<<" not in comando or "cat" not in comando:
            return []
        cwd = payload.get("cwd")
        alvos = (_vigiado(_absoluto(p, cwd), c) for p, c in _heredoc().extrair(comando))
        return [(*a, "Bash.heredoc") for a in alvos if a is not None]
    alvo = _vigiado(str(ti.get("file_path", "")), str(ti.get("content", "")))
    return [] if alvo is None else [(*alvo, "Write")]


def plano_ativo(cwd: str | None) -> tuple[Path | None, str | None]:
    """The declared ``diagnostics/`` directory of the active plan, as ``(directory, error)``.

    Both ``None`` = no declaration (the hook only warns). The nearest ``plano_ativo.json`` decides: an
    unreadable one, a wrong ``versao`` or an unsafe ``bundle`` (absolute, ``..``) yields ``(None, reason)`` —
    never a guess at "the most recent plan", never an exception.
    """
    if not cwd:
        return None, None
    atual = Path(cwd).resolve()
    for pasta in (atual, *atual.parents):
        arq = pasta / PLANO_ATIVO_REL
        if not arq.is_file():
            continue
        try:
            dados = json.loads(arq.read_text(encoding="utf-8"))
        except (OSError, ValueError) as exc:
            return None, f"plano_ativo ilegivel ({type(exc).__name__})"
        if not isinstance(dados, dict) or dados.get("versao") != PLANO_ATIVO_VERSAO:
            return (
                None,
                f"plano_ativo versao inesperada (esperava {PLANO_ATIVO_VERSAO})",
            )
        bundle = dados.get("bundle")
        if not isinstance(bundle, str) or len(bundle) > BUNDLE_MAX or not BUNDLE_RE.fullmatch(bundle):
            return None, "plano_ativo.bundle invalido (esperava docs/plans/<nome>, um nivel, [A-Za-z0-9._-])"
        if set(bundle.rsplit("/", 1)[1]) == {"."}:
            return None, "plano_ativo.bundle invalido (nome so de pontos)"
        raiz = pasta.resolve()
        try:
            alvo = (raiz / bundle).resolve()
        except (OSError, RuntimeError):
            return None, "plano_ativo.bundle nao resolve"
        if not alvo.is_relative_to(raiz):
            return None, "plano_ativo.bundle escapa da raiz do projeto"
        return alvo / "diagnostics", None
    return None, None


def _destino(path: str, cwd: str | None) -> dict:
    """Extra event fields for an ephemeral target: ``destino_sugerido`` or ``plano_ativo_erro`` (both optional)."""
    if os.environ.get("TOURING_PORTFOLIO_REDIRECT_DISABLED"):
        return {}
    try:
        pasta, erro = plano_ativo(cwd)
    except Exception as exc:  # fail-open: the declaration never takes the hook down
        return {"plano_ativo_erro": f"plano_ativo: {type(exc).__name__}"}
    if pasta is not None:
        if not Path(path).name.isprintable():  # a basename with control characters never reaches the context
            return {"plano_ativo_erro": "basename com caracteres de controle"}
        return {"destino_sugerido": str(pasta / Path(path).name)}
    return {"plano_ativo_erro": erro} if erro else {}


def _avaliar(alvo: tuple[str, str, bool, bool, str], cwd: str | None) -> str | None:
    """Consult, record the event, and return the context line when there are candidates."""
    path, content, ephemeral, generic, origem = alvo
    v, erro = _consultar_seguro(path, content, cwd)
    extra = _destino(path, cwd) if ephemeral else {}
    log_event({**_evento(path, ephemeral, generic, v, erro, origem), **extra})
    linhas = [_contexto(v, Path(path).name, ephemeral)] if v is not None and v.candidatos else []
    if "destino_sugerido" in extra:
        linhas.append(
            f"[PORTFOLIO · plano ativo] grave o script em {extra['destino_sugerido']} (diagnostics/ do plano declarado "
            f"em .claude/plano_ativo.json) — o que nasce em destino efêmero morre com a sessão."
        )
    return "\n".join(linhas) or None


def main() -> int:
    """Hook PreToolUse (Write e Bash): consulta o portfólio e avisa — nunca bloqueia (sombra).

    Desde a P1 (28/10/2026) a decisão é ``recuperar.consultar`` — corpo (SimHash por conjunto de shingles,
    τ=8) + propósito, devolvendo ``veredito_gate/1``. O casamento antigo (``manifest_candidates`` /
    ``tantivy_candidates``) fica neste módulo só como linha de base do avaliador (``portfolio/avaliar.py``).

    Desde a P3 (28/10/2026) também o Bash: heredoc ``cat`` que cria script vigiado. No Bash o hook só emite
    contexto — nunca reescreve a entrada nem nega: o ``touring-hook`` já reescreve comandos Bash, e dois
    reescritores no mesmo PreToolUse se sobrescrevem (o último vence).
    """
    if os.environ.get("TOURING_PORTFOLIO_GATE_DISABLED"):
        return 0
    if "--stats" in sys.argv:
        return stats()
    try:
        payload = json.load(sys.stdin)
    except (json.JSONDecodeError, EOFError):
        return 0  # fail-open: payload ilegível nunca bloqueia
    alvos = _gatilhos(payload) if isinstance(payload, dict) else []
    if not alvos or not injection_allowed():
        return 0
    contextos = [c for a in alvos if (c := _avaliar(a, payload.get("cwd")))]
    if contextos:
        emit({"context": "\n".join(contextos)})
    return 0


def _consultar_seguro(path: str, content: str, cwd: str | None):
    """Sombra: qualquer falha da decisão vira registro inaferível, nunca bloqueio nem silêncio."""
    try:
        return _recuperar().consultar(path, content, cwd), None
    except Exception as exc:
        return None, f"recuperar: {type(exc).__name__}"


def _evento(
    path: str,
    ephemeral: bool,
    generic: bool,
    v,
    erro: str | None,
    origem: str = "Write",
) -> dict:
    base = {
        "ts": int(time.time()),
        "file": path,
        "basename": Path(path).name,
        "ephemeral": ephemeral,
        "generic_name": generic,
        "origem": origem,
    }
    if v is None:
        return {
            **base,
            "estado": "inaferivel",
            "motivo": erro,
            "n_candidates": 0,
            "would_avoid": False,
            "top": [],
        }
    return {
        **base,
        "estado": v.estado,
        "motivo": v.motivo,
        "fontes_falhas": list(v.fontes_falhas),
        "n_candidates": len(v.candidatos),
        "would_avoid": v.estado == "reprovado",
        "top": [f"{cid[-80:]}|{sinal}|{d}" for cid, sinal, d in v.candidatos],
    }


if __name__ == "__main__":
    sys.exit(main())
