"""V0d — build the replay corpus the VGP motor is measured against (V0d-spec.md).

Each entry is a SNAPSHOT of a file in the state a session proposed, plus the project the invented
names resolve against. The motor judges that snapshot; today's code is only the control.

Three provenances, decided by where the file LIVES — not by the inventory bucket, which records
how the error was noticed:

  tmp/cache  the path is session work (`replay.session_work`: /tmp and <project>/.cache). Rebuilt
             from the session's own Write/Edit/MultiEdit fold up to `first`. 26 of the 49 "atribuido"
             rows are /tmp/claude-*/scratchpad files: bucketed as project edits, but never in git.
  atribuído  a real project file: base `git show <rev>:<rel>` with rev = the last commit before
             `first`, then the session's edits up to `first`, calls denied by a hook skipped.
  inline     the program IS the command (`python3 - <<EOF` / `-c`); the snapshot is that program.

An Edit that no longer matches, or a base that does not exist, makes the entry INCOMPLETE: it is
left out of the corpus and counted by reason in relatorio.json. It never enters with wrong content
— a corpus that measures a wrong file measures nothing.

Usage: python3 corpus_v0d.py [--jobs 4] [--timeout 300] [--selftest]
"""

from __future__ import annotations

import concurrent.futures
import hashlib
import json
import random
import re
import subprocess
import sys
import tempfile
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
import juiz_v0d  # noqa: E402  # pyright: ignore[reportMissingImports]
import replay  # noqa: E402  # pyright: ignore[reportMissingImports]
import vivos  # noqa: E402  # pyright: ignore[reportMissingImports]

C = replay.VGP / "corpus_v0d"
SEED, N_NEG, CAP = 20261008, 300, 300
INVENTARIO = replay.VGP / "erros_inventario.jsonl"


# --------------------------------------------------------------------------- util


def ident(rotulo: str, session: str, path: str, ts: str) -> str:
    """Deterministic id (REGRA #17): same provenance, same id, on any machine."""
    return hashlib.sha1(json.dumps([rotulo, session, path, ts]).encode()).hexdigest()[
        :12
    ]


def sha(text: str) -> str:
    return hashlib.sha256(text.encode()).hexdigest()


def gravar_snap(entry_id: str, texto: str) -> str:
    (C / "snap").mkdir(parents=True, exist_ok=True)
    rel = f"snap/{entry_id}.py"
    (C / rel).write_text(texto, encoding="utf-8")
    return rel


# --------------------------------------------------------------------------- a dobra do projeto


def _git(root: Path, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run(
        ["git", "-C", str(root), *args], capture_output=True, text=True
    )


def base_no_instante(root: Path, rel: str, first: str) -> str | None:
    """The committed content of `rel` as of `first`; None when the file did not exist yet."""
    rev = _git(root, "rev-list", "-1", f"--before={first}", "HEAD").stdout.strip()
    if not rev:
        return None
    p = _git(root, "show", f"{rev}:{rel}")
    return p.stdout if p.returncode == 0 else None


def _git_root(arq: str) -> Path | None:
    """The REAL git root containing the file (walk-up; `.git` may be a dir or a worktree file).
    `vivos.project_root` only maps the two known projects and DEFAULTS to analise — under it a hook
    in ~/.claude and the nested repo analise/.claude both fell to "reconstrução quebrada" (270 of the
    first 300 draws, 08/10)."""
    try:
        d = Path(arq).resolve().parent
    except OSError:
        return None
    for anc in (d, *d.parents):
        if (anc / ".git").exists():
            return anc
    return None


def _negadas(records: list[dict]) -> set:
    """Calls whose result is an error never changed the file: a hook-denied Edit, then its retry no
    longer matching. Applying them anyway desynchronised 52 of 72 rebuilds on 08/10/2026."""
    return {
        c.get("tool_use_id")
        for rec in records
        for c in replay._contents(rec)
        if c.get("type") == "tool_result" and c.get("is_error")
    }


def _heredocs_de(command: str, alvo: str) -> list[tuple[bool, str]]:
    """`(append, body)` of every `cat > alvo <<TAG` in the command — replay.HEREDOC/ASSIGN without
    replay's two disk guards (session_work, exists-today): the fold rebuilds the PAST; those guards
    protect today's tree, which is exactly what we do NOT want to see."""
    env = dict(replay.ASSIGN.findall(command))
    out = []
    for op, target, _tag, body in replay.HEREDOC.findall(command):
        path = re.sub(
            r"\$\{?([A-Za-z_]\w*)\}?", lambda m: env.get(m.group(1), m.group(0)), target
        )
        if path == alvo and "$" not in path:
            out.append((op == ">>", body))
    return out


def _aplica_livre(c: dict, alvo: str, files: dict[str, str], broken: set[str]) -> bool:
    """Apply ONE tool_use to the fold; True when it CREATED the base (Write or truncate heredoc)."""
    if c.get("name") == "Bash":
        criou = False
        for append, body in _heredocs_de(
            str((c.get("input") or {}).get("command", "")), alvo
        ):
            files[alvo] = (files.get(alvo, "") if append else "") + body + "\n"
            if not append:
                broken.discard(alvo)  # truncate = absolute write, like the Write tool
            criou = True
        return criou
    if str((c.get("input") or {}).get("file_path", "")) != alvo:
        return False
    if c.get("name") == "Write":
        files[alvo] = str(c["input"].get("content", ""))
        broken.discard(alvo)
        return True
    for e in replay._edits_of(c):
        feito = replay._apply_edit(
            files.get(alvo, ""),
            e.get("old_string", ""),
            e.get("new_string", ""),
            bool(e.get("replace_all")),
        )
        if feito is None:
            broken.add(alvo)
        else:
            files[alvo] = feito
    return False


def _dobra_livre(
    records: list[dict], alvo: str, base: str, until: str
) -> tuple[str, set[str], bool]:
    """Fold every recorded change to `alvo` over `base`, in timestamp order, up to `until` inclusive;
    calls denied by a hook are skipped. Returns (text, broken, created): `created` says a Write or
    truncate-heredoc gave the chain its base — without it and with an empty base the prior content
    is unknowable and the candidate must stay out."""
    failed = _negadas(records)
    files: dict[str, str] = {alvo: base}
    broken: set[str] = set()
    created = False
    for r in sorted(records, key=lambda r: r.get("timestamp", "")):
        if r.get("timestamp", "") > until:
            break
        for c in replay._contents(r):
            if c.get("type") == "tool_use" and c.get("id") not in failed:
                created |= _aplica_livre(c, alvo, files, broken)
    return files.get(alvo, ""), broken, created


def _dobra_projeto(
    records: list[dict], path: str, base: str, until: str
) -> tuple[str, set[str]]:
    """2-tuple wrapper of the free fold (the inherited selftest's contract)."""
    texto, broken, _ = _dobra_livre(records, path, base, until)
    return texto, broken


# --------------------------------------------------------------------------- o programa inline


HEREDOC_PROG = re.compile(r"<<-?\s*['\"]?(\w+)['\"]?[^\n]*\n(.*?)\n\1\s*$", re.S | re.M)
C_PROG = re.compile(r"python3?\s+-c\s+(?:'([^']*)'|\"([^\"]*)\"|'([^']*)'\s*;)", re.S)


def _arg_shell(command: str, flag: str) -> str | None:
    """The value of `--flag <value>` in a shell command line, parsed with shlex (quote-aware) —
    a regex over `--code '...'` breaks on the first inner quote; shlex honours the shell's rules."""
    import shlex

    try:
        tokens = shlex.split(command, posix=True)
    except ValueError:
        return None
    return (
        tokens[tokens.index(flag) + 1]
        if flag in tokens and tokens.index(flag) + 1 < len(tokens)
        else None
    )


def _reconstroi_arquivo_sessao(path: str, session: str, first: str) -> str | None:
    """The content of a session file (/tmp scratch etc.) at `first`, folded from that session's
    transcripts — for `touring run --file` the PROGRAM lives in the file, not in the command."""
    if not session:
        return None
    records = list(replay._records(replay.transcripts_of(session)))
    texto, quebrados, criado = _dobra_livre(records, path, "", first)
    return texto if criado and not quebrados else None


def programa_inline(command: str, session: str = "", first: str = "") -> str | None:
    """The program a command actually runs, as the file the traceback will name: heredoc, `-c`,
    or `touring run --code/--file` (whose frames the engine names `<string>`, like `-c` does)."""
    if "touring run" in command:
        code = _arg_shell(command, "--code")
        if code is not None:
            return code if code.endswith("\n") else code + "\n"
        alvo = _arg_shell(command, "--file")
        if alvo:
            return _reconstroi_arquivo_sessao(alvo, session, first)
        return None
    m = HEREDOC_PROG.search(command)
    if m:
        return m.group(2) + "\n"
    m = C_PROG.search(command)
    if m:
        return next(g for g in m.groups() if g is not None) + "\n"
    return None


def comando_replay(command: str, programa: str) -> str | None:
    """For a `touring run` invocation: the replay runs the extracted program directly — the engine
    inside the replay's bwrap would see hidden daemon sockets and no network. `--lang python` (the
    default) becomes `python3 -c`, whose frames are named `<string>` exactly as the engine's
    (measured on all 17 cases); `--lang bash` becomes `bash -c`, preserving the frames of whatever
    the bash itself calls (a real overlaid path, or `<stdin>` for an inner heredoc). `--timeout-ms`
    is preserved as `timeout N` because `_declared_timeout` reads the ceiling from the command text.
    Pipe suffixes (`| head`) are dropped: they truncated tracebacks and ate frames. Returns None for
    commands that are not touring run (they replay as-is)."""
    if "touring run" not in command:
        return None
    import shlex

    ms = _arg_shell(command, "--timeout-ms")
    prefixo = f"timeout {max(1, -(-int(ms) // 1000))} " if ms and ms.isdigit() else ""
    interp = "bash" if _arg_shell(command, "--lang") == "bash" else "python3"
    return f"{prefixo}{interp} -c {shlex.quote(programa)}"


def _programa_para_replay(
    comando: str, origem: str, texto: str, session: str, first: str
) -> str | None:
    """What the replay should RUN for a `touring run` command. Inline: the snapshot IS the program
    (extracted from --code/--file). tmp/atribuido: the snapshot is the FILE's content — the program
    is the command's own --code/--file (a bash wrapper calling `python3 /tmp/x.py`; the overlaid
    file gives the frame its real path). Feeding the python snapshot to `bash -c` was the measured
    bug behind 5 `bash: -c: syntax error` records (build 2, 09/10)."""
    if "touring run" not in comando:
        return None
    if origem == "inline":
        return texto
    return programa_inline(comando, session, first)


def comando_de_arquivo(command: str, path: str, texto: str) -> str | None:
    """When the stored command is `python3 -c '<the file's own source>'` (or the heredoc of it) for
    an entry whose path is a REAL file, the faithful replay runs the file: `-c` names frames
    `<string>` and has no `__file__` (measured: 289428033ddd died at `<string>:46` on `__file__`
    instead of the historical `arquivo.py:97`). None when the program is not the file's content."""
    if "touring run" in command or not path.startswith("/"):
        return None
    prog = programa_inline(command)
    if prog is None or not prog.strip()[:200] or prog.strip()[:200] not in texto:
        return None
    import shlex

    return f"python3 {shlex.quote(path)}"


def _expande_vars(command: str) -> str:
    """Expand `S=/tmp/x; python3 $S/a.py`'s simple VAR=literal assignments (replay.ASSIGN's subset):
    without it `$VAR` paths are invisible to reference extraction and vanish from the overlay —
    the replay then dies with 'No such file or directory' (measured: 429e18f3d11f)."""
    env = dict(replay.ASSIGN.findall(command))
    return re.sub(
        r"\$\{?([A-Za-z_]\w*)\}?", lambda m: env.get(m.group(1), m.group(0)), command
    )


# --------------------------------------------------------------------------- positivos


def candidatos() -> list[dict]:
    """The measured universe: python, bucket atribuído/inline, with an invented-API class.
    `classe_de` is IMPORTED — one definition of the class for builder, judge and report."""
    return [
        json.loads(ln)
        for ln in INVENTARIO.open(encoding="utf-8")
        if (r := json.loads(ln)).get("lang") == "python"
        and r.get("bucket") in ("atribuído", "inline")
        and juiz_v0d.classe_de(r)
    ]


def _snap_sessao(
    records: list[dict], arq: str, first: str
) -> tuple[str, str | None, str]:
    # a dobra livre substitui rebuild_tmp: este pulava o arquivo que AINDA EXISTE hoje (a guarda
    # protege o disco, não a história) — e o estado de first pode não ser o de hoje
    texto, quebrados, criado = _dobra_livre(records, arq, "", first)
    if quebrados:
        return ("tmp", None, "Edit que nao casa sobre arquivo de sessao")
    if criado:
        return ("tmp", texto, "")
    return ("tmp", None, "a sessao nao gravou esse arquivo antes de `first`")


def _snap_projeto(
    records: list[dict], arq: str, first: str
) -> tuple[str, str | None, str]:
    root = _git_root(arq)
    if not root:
        return ("atribuido", None, "sem raiz git (fora de todo repositório)")
    try:
        rel = str(Path(arq).resolve().relative_to(root.resolve()))
    except ValueError:
        return ("atribuido", None, "o arquivo nao esta sob a raiz do projeto")
    base = base_no_instante(root, rel, first)
    if base is None:
        # o arquivo não existia no commit anterior a first: se a sessão o CRIOU (Write/heredoc),
        # a base é vazia e a entrada é legítima — medido em 14 dos 18 casos de 08/10
        texto, quebrados, criado = _dobra_livre(records, arq, "", first)
        if criado and not quebrados:
            return ("atribuido", texto, "")
        return (
            "atribuido",
            None,
            f"o arquivo nao existia no commit anterior a {first} "
            "e nenhum Write da sessao o criou"
            if not criado
            else "o arquivo nao existia no commit anterior e a dobra quebrou",
        )
    texto, quebrados, _ = _dobra_livre(records, arq, base, first)
    if quebrados:
        return ("atribuido", None, "Edit que nao casa sobre a base do git")
    return ("atribuido", texto, "")


def _origem_e_base(
    row: dict, records: list[dict], comando: str = ""
) -> tuple[str, str | None, str]:
    """(origem, snapshot or None, motivo da incompletude). Where the file LIVES decides the
    provenance; the inventory bucket only records how the error was noticed. For inline the program
    is extracted from the FULL command (the inventory's copy may be cut at 600 chars)."""
    arq, first = row.get("file", ""), row.get("first", "")
    if row["bucket"] == "inline":
        cmd = comando or row.get("command", "")
        prog = programa_inline(cmd, row.get("session", ""), first)
        if prog is None and re.search(r"bash\s+\S*\.sh", cmd):
            return (
                "inline",
                None,
                "erro dentro de script .sh — programa indireto, nao extraivel",
            )
        return (
            "inline",
            prog,
            "" if prog else "sem heredoc, -c ou touring run extraivel no comando",
        )
    if replay.session_work(arq):
        return _snap_sessao(records, arq, first)
    return _snap_projeto(records, arq, first)


# --------------------------------------------------------------------------- contexto (mesmo rev)


def _refs_projeto(command: str, cwd: str) -> set[str]:
    """Project files the command NAMES: absolute paths under a git root (session_work is replay's),
    `python3 -m a.b.c` modules and relative paths, resolved against the session cwd."""
    refs = {
        r
        for r in re.findall(r"(/[^\s'\";|&)]+?\.(?:py|sh|json|txt))", command)
        if not replay.session_work(r) and _git_root(r)
    }
    base = Path(cwd) if cwd else None
    if base:
        for m in re.findall(r"(?:^|\s)-m\s+([\w.]+)", command):
            refs |= {
                str(base / (m.replace(".", "/") + ".py")),
                str(base / m.replace(".", "/") / "__init__.py"),
            }
        refs |= {
            str(base / r)
            for r in re.findall(
                r"(?<![\w/.-])((?:[\w.-]+/)*[\w.-]+\.(?:py|sh|json|txt))", command
            )
        }
    return {r for r in refs if Path(r).is_file() or _git_root(r)}


def _modulos_importados(snap_text: str) -> set[str]:
    """Top-level module names the snapshot imports — ast, not regex (comments and strings lie)."""
    import ast

    try:
        tree = ast.parse(snap_text or "")
    except (SyntaxError, ValueError):
        return set()
    mods = {
        a.name for n in ast.walk(tree) if isinstance(n, ast.Import) for a in n.names
    }
    return mods | {
        n.module
        for n in ast.walk(tree)
        if isinstance(n, ast.ImportFrom) and n.module and n.level == 0
    }


def _resolve_mod(mod: str, bases: list[Path], virtuais: set[str]) -> set[str]:
    """`a.b` → bases/a/b.py or bases/a/b/__init__.py, when the file exists today OR is one of the
    session's own rebuilt files (`virtuais`) — a hypothetical path under /tmp is not enough: the
    session must actually have written it (its content then comes from the fold, never a guess)."""
    rel = mod.replace(".", "/")
    out = set()
    for b in bases:
        for cand in (b / f"{rel}.py", b / rel / "__init__.py"):
            if cand.is_file() or str(cand) in virtuais:
                out.add(str(cand))
    return out


def _imports_do(
    snap_text: str,
    proprio: str,
    cwd: str,
    extra_bases: list[str] | None = None,
    virtuais: set[str] | None = None,
) -> set[str]:
    """Direct imports of the snapshot that resolve to a real file: base candidates are the
    snapshot's git root, the session cwd, the snapshot's own directory (a /tmp script importing
    its /tmp sibling) and `extra_bases` (the directory of a `touring run --file` program)."""
    bases = [
        b
        for b in (
            _git_root(proprio),
            Path(cwd) if cwd else None,
            Path(proprio).parent if proprio.startswith("/") else None,
        )
        if b
    ]
    bases += [Path(b) for b in (extra_bases or [])]
    return {
        r
        for mod in _modulos_importados(snap_text)
        for r in _resolve_mod(mod, bases, virtuais or set())
    }


def _contexto(
    command: str,
    cwd: str,
    proprio: str,
    first: str,
    files_sessao: dict[str, str],
    records: list[dict] | None = None,
    snap_text: str = "",
    prog_dir: str = "",
) -> list[dict]:
    """Files the replay must ALSO lay over the tree, each at its `first` state: session files the
    command names (rebuilt — today they are gone or changed), project files the command names or the
    snapshot imports (base git at `first` + the session fold — never today's, which is what the
    8 silent `rc 0` replays of 08/10 imported). What reconstructs identical to today is not laid."""
    alvo: dict[str, str] = {}
    comando_x = _expande_vars(command)
    refs = (
        replay._referenced(comando_x, cwd)
        | _refs_projeto(comando_x, cwd)
        | _imports_do(
            snap_text, proprio, cwd, [prog_dir] if prog_dir else None, set(files_sessao)
        )
    )
    for ref in sorted(refs - {proprio}):
        if ref in files_sessao:
            alvo[ref] = files_sessao[ref]
            continue
        recon = _conteudo_depois(ref, first, {ref: list(records or [])})
        if recon is None:
            continue
        hoje = Path(ref)
        if (
            not hoje.is_file()
            or hoje.read_text(encoding="utf-8", errors="replace") != recon
        ):
            alvo[ref] = recon
    out = []
    for ref, texto in sorted(alvo.items()):
        eid = ident("contexto", ref, ref, first)
        out.append({"path": ref, "snap": gravar_snap(eid, texto), "sha256": sha(texto)})
    return out


def montar_positivo(row: dict) -> tuple[dict | None, str]:
    """(entry, motivo do descarte)."""
    records = list(replay._records(replay.transcripts_of(row.get("session", ""))))
    if not records:
        return None, "transcript da sessao nao encontrado"
    comando, cwd, fonte = replay.full_command(row, records)
    origem, texto, motivo = _origem_e_base(row, records, comando)
    arq = row.get("file", "")
    if texto is None:
        return None, f"[{origem}] {motivo}"

    eid = ident("positivo", row.get("session", ""), arq, row.get("first", ""))
    rel = gravar_snap(eid, texto)
    files_sessao, _ = replay.rebuild_tmp(records, row.get("first", ""))
    prog = _programa_para_replay(
        comando, origem, texto, row.get("session", ""), row.get("first", "")
    )
    # touring run → python3 -c / bash -c; `-c` da própria fonte em arquivo real → python3 <path>
    cmd_replay = (
        (comando_replay(comando, prog) if prog else None)
        or comando_de_arquivo(comando, arq, texto)
        or comando
    )
    file_arg = _arg_shell(comando, "--file") if "touring run" in comando else None
    prog_dir = (
        str(Path(file_arg).parent) if file_arg else ""
    )  # imports do programa moram ao lado dele
    itens = {
        "id": eid,
        "rotulo": "positivo",
        "classe": juiz_v0d.classe_de(row),
        "origem": origem,
        "lang": "python",
        "path": arq,
        "project_root": str(vivos.project_root(Path(arq)) or ""),
        "snap": rel,
        "snap_sha256": sha(texto),
        "nome_esperado": vivos.static_name(row),
        "session": row.get("session", ""),
        "ts": row.get("first", ""),
        "tool_use_id": row.get("tool_use_id", ""),
        "item_v0a": row.get("id", ""),
        "kind": row.get("kind", ""),
        "linha": row.get("line"),
        "mensagem": row.get("message", ""),
        "command": cmd_replay,
        "cwd": cwd,
        "command_original": comando if cmd_replay != comando else "",
        "command_source": fonte if cmd_replay == comando else f"{fonte}+python-c",
        "contexto": _contexto(
            comando,
            cwd,
            arq,
            row.get("first", ""),
            files_sessao,
            records,
            texto,
            prog_dir,
        ),
    }
    return itens, ""


def reproduzir(item: dict, cap: int) -> dict:
    """Overlay the snapshot (and its declared contexto) and re-run the ORIGINAL command. The record,
    not this function's opinion, decides: `reproduz` is read back out of what was written. A record
    is reused only when the overlay signature (paths + sha256) is unchanged — a rebuilt snapshot or
    contexto re-runs, so a rebuild never inherits the verdict of an older snapshot."""
    laid = (
        item.get("contexto", [])
        if item["origem"] == "inline"
        else [
            {
                "path": item["path"],
                "snap": item["snap"],
                "sha256": item.get("snap_sha256", ""),
            },
            *item.get("contexto", []),
        ]
    )
    assinatura = sha(
        item["command"] + json.dumps([[f["path"], f.get("sha256", "")] for f in laid])
    )
    reg = C / "replay_v0d" / f"{item['id']}.json"
    if reg.is_file():
        r = json.loads(reg.read_text(encoding="utf-8"))
        if r.get("overlay_sha256") == assinatura:
            return r
    files = {f["path"]: (C / f["snap"]).read_text(encoding="utf-8") for f in laid}
    t0 = time.monotonic()
    rc, timed_out, out = replay.run_isolated(
        C / "work" / item["id"], item["command"], item["cwd"], files, cap
    )
    # `kind` e `mensagem` PRECISAM viajar: `_verdict_fields` le os dois, e sem `kind` o
    # `kind_in_output` sai False por construcao — foi o que zerou a primeira leva.
    ver = replay._verdict_fields(
        {
            "path": item["path"],
            "line": item["linha"],
            "kind": item.get("kind", ""),
            "message": item.get("mensagem", ""),
        },
        out,
    )
    rec = {
        "id": item["id"],
        "classe": item["classe"],
        "origem": item["origem"],
        "rc": rc,
        "timed_out": timed_out,
        "wall_s": round(time.monotonic() - t0, 1),
        "command_source": item.get("command_source", ""),
        "overlay_sha256": assinatura,
        **ver,
    }
    reg.parent.mkdir(parents=True, exist_ok=True)
    reg.write_text(json.dumps(rec, ensure_ascii=False, indent=1), encoding="utf-8")
    return rec


# --------------------------------------------------------------------------- negativos


def _ferrou(c: dict) -> bool:
    return (
        c.get("type") == "tool_use"
        and c.get("name") in ("Edit", "Write", "MultiEdit")
        and str((c.get("input") or {}).get("file_path", "")).endswith(".py")
    )


def _erro_nomeia(registros: list[dict], arq: str, ate: str) -> bool:
    nome = Path(arq).name
    for rec in registros:
        ts = rec.get("timestamp", "")
        if ts <= ate:
            for c in replay._contents(rec):
                if c.get("type") == "tool_result" and c.get("is_error"):
                    txt = " ".join(
                        str(x.get("text", ""))
                        for x in c.get("content") or []
                        if isinstance(x, dict)
                    )
                    if f'File "{nome}"' in txt or f"{nome}:" in txt:
                        return True
    return False


def _edicoes(registros: list[dict]) -> dict[str, list[str]]:
    """{arquivo .py: [timestamps das edições]} de um transcript, em ordem."""
    por_arq: dict[str, list[str]] = {}
    for rec in registros:
        for c in replay._contents(rec):
            if _ferrou(c):
                por_arq.setdefault(str(c["input"]["file_path"]), []).append(
                    rec.get("timestamp", "")
                )
    for tempos in por_arq.values():
        tempos.sort()
    return por_arq


def varrer_transcripts() -> dict:
    """ONE pass over every transcript: {arquivo .py editado: registros das sessoes que o editaram}.
    Rescanning per negative would be O(300 x transcripts)."""
    indice: dict[str, list[dict]] = {}
    for arquivo in sorted(replay.PROJECTS.glob("*/*.jsonl")):
        # alguns transcripts tem `message` como STRING; `replay._contents` assume dict e
        # levanta AttributeError. Filtro aqui — `replay.py` e da coordenadora, intocado.
        registros = [
            r for r in replay._records([arquivo]) if isinstance(r.get("message"), dict)
        ]
        if not registros:
            continue
        for arq in _edicoes(registros):
            indice.setdefault(arq, []).extend(registros)
    return indice


def universo_negativos(indice: dict) -> tuple[list[str], dict]:
    """Every clean `.py` edit, canonically ordered. Filtering happens BEFORE the draw, and the
    report says how many were seen, how many survived, and why the rest did not."""
    vistos: set[str] = set()
    filtrados: set[str] = set()
    for arq, registros in indice.items():
        vistos.add(arq)
        # `_edicoes` recebe a LISTA de registros: passar um registro dict faz o `for rec in
        # registros` iterar as CHAVES (strings), e `replay._contents` levanta AttributeError.
        tempos = sorted(_edicoes(registros).get(arq, []))
        for i, ts in enumerate(tempos):
            fim = tempos[i + 1] if i + 1 < len(tempos) else "9" * 30
            if not _erro_nomeia(registros, arq, fim):
                filtrados.add(f"{arq}@{ts}")
    return sorted(filtrados), {"vistos": len(vistos), "candidatos": len(filtrados)}


# --------------------------------------------------------------------------- main


def _montar() -> tuple[list[dict], dict[str, int], int]:
    entradas: list[dict] = []
    incompletos: dict[str, int] = {}
    linhas = candidatos()
    vistos: set[str] = set()
    for row in linhas:
        item, motivo = montar_positivo(row)
        if item is None:
            incompletos[motivo] = incompletos.get(motivo, 0) + 1
        elif item["id"] in vistos:
            # duas linhas V0a do MESMO evento (session, path, first): o id determinístico colide
            # por construção — entra uma só (medido: ea97d20c157a ×2, 09/10)
            incompletos["duplicata de inventario (mesmo evento)"] = (
                incompletos.get("duplicata de inventario (mesmo evento)", 0) + 1
            )
        else:
            vistos.add(item["id"])
            entradas.append(item)
    return entradas, incompletos, len(linhas)


def _replayar(entradas: list[dict], jobs: int, cap: int) -> list[dict]:
    with concurrent.futures.ThreadPoolExecutor(jobs) as pool:
        return list(pool.map(lambda i: reproduzir(i, cap), entradas))


def _conta(chave: str, entradas: list[dict], proved: set[str]) -> dict:
    valores = sorted({str(e.get(chave)) for e in entradas})
    return {
        v: sum(1 for e in entradas if str(e.get(chave)) == v and e["id"] in proved)
        for v in valores
    }


def _nao_reproduzem(
    entradas: list[dict], registros: list[dict], proved: set[str]
) -> list[dict]:
    return [
        {
            "id": e["id"],
            "classe": e["classe"],
            "origem": e["origem"],
            "kind_in_output": r.get("kind_in_output"),
            "frame_in_output": r.get("frame_in_output"),
            "rc": r.get("rc"),
            "timed_out": r.get("timed_out"),
        }
        for e, r in zip(entradas, registros)
        if e["id"] not in proved
    ]


def _sorteia_negativos(
    ids: list[str], seed: int, n: int, reconhece
) -> tuple[list[str], list[str], dict]:
    """Draw n ids from the canonical (sorted) pool; every drawn id must verify under `reconhece`.
    A drawn id that fails LEAVES the pool and the draw repeats — so the FINAL registered pool
    re-draws exactly the final sample under the seed, which is precisely what the judge replays.
    Deterministic: the pool sequence is a function of the seed and the (deterministic) verdicts."""
    pool = sorted(ids)
    tentados: dict[str, bool] = {}
    podados = rodadas = 0
    while True:
        amostra = random.Random(seed).sample(pool, min(n, len(pool)))
        for eid in (e for e in amostra if e not in tentados):
            tentados[eid] = bool(reconhece(eid))
        falhos = {e for e in amostra if not tentados[e]}
        if not falhos:
            return pool, amostra, {"podados": podados, "rodadas": rodadas + 1}
        pool = [e for e in pool if e not in falhos]
        podados += len(falhos)
        rodadas += 1


def construir_negativos() -> dict:
    """300 clean `.py` snapshots, drawn AFTER filtering and registered as IDS in canonical order —
    the judge re-draws `random.Random(SEED).sample(candidatos, 300)` and demands the same id set, so
    the pool holds only ids that verify (the spec DEFINES the candidate by its snapshot: it must
    reconstruct and compile). A candidate that fails is reported and leaves the pool, never the
    corpus with wrong content."""
    indice = varrer_transcripts()
    filtrados, contagens = universo_negativos(indice)
    pares: dict[str, tuple[str, str]] = {}
    for chave in filtrados:
        arq, ts = chave.rsplit("@", 1)
        pares[ident("negativo", "", arq, ts)] = (arq, ts)
    textos: dict[str, str | None] = {}

    def reconhece(eid: str) -> bool:
        if eid not in textos:
            arq, ts = pares[eid]
            texto = _conteudo_depois(arq, ts, indice)
            if texto is not None:
                try:
                    compile(texto, arq, "exec")
                except (SyntaxError, ValueError):
                    texto = None
            textos[eid] = texto
        return textos[eid] is not None

    pool_ids, amostra, stats = _sorteia_negativos(sorted(pares), SEED, N_NEG, reconhece)
    (C / "negativos_candidatos.json").write_text(
        json.dumps({"seed": SEED, "candidatos": pool_ids}, ensure_ascii=False),
        encoding="utf-8",
    )
    entradas = []
    for eid in amostra:
        arq, ts = pares[eid]
        texto = textos[eid]
        assert texto is not None  # a amostra final só fecha com todos verificados
        entradas.append(
            {
                "id": eid,
                "rotulo": "negativo",
                "classe": None,
                "origem": "atribuido",
                "lang": "python",
                "path": arq,
                "project_root": str(
                    _git_root(arq) or vivos.project_root(Path(arq)) or ""
                ),
                "snap": gravar_snap(eid, texto),
                "snap_sha256": sha(texto),
                "nome_esperado": None,
                "session": "",
                "ts": ts,
                "tool_use_id": "",
                "item_v0a": "",
                "kind": "",
                "linha": None,
                "mensagem": "",
                "command": "",
                "cwd": "",
                "contexto": [],
                "reproduz": None,
            }
        )
    return {
        "universo": contagens,
        "pool_registrado": len(pool_ids),
        "sorteados": len(amostra),
        "no_corpus": len(entradas),
        "podas": stats,
        "entradas": entradas,
    }


def _conteudo_depois(arq: str, ts: str, indice: dict) -> str | None:
    """The file's content right after the edit at `ts`, by where the file LIVES: a real git root
    (walk-up — `vivos.project_root` defaults to analise, and both ~/.claude hooks and the nested
    repo analise/.claude fell through it into "reconstrução quebrada") gives the committed base at
    `ts`; without a base, the session fold stands only when a Write/heredoc CREATED the file ≤ ts.
    Unknowable content → None — a wrong snapshot enters the corpus never."""
    registros = indice.get(arq)
    root = _git_root(arq)
    base: str | None = None
    if root:
        try:
            rel = str(Path(arq).resolve().relative_to(root.resolve()))
        except ValueError:
            return None
        base = base_no_instante(root, rel, ts)
    if not registros:
        # o contexto pergunta por arquivos que a sessão NÃO editou: o estado em ts é a base
        # commitada; sem base git o conteúdo é incognoscível (candidatos a negativo sempre têm
        # registros — a chave nasce de uma edição)
        return base
    texto, quebrados, criado = _dobra_livre(
        registros, arq, base if base is not None else "", ts
    )
    if quebrados or (base is None and not criado):
        return None
    return texto


def construir(jobs: int, cap: int) -> dict:
    t0 = time.monotonic()
    C.mkdir(parents=True, exist_ok=True)
    entradas, incompletos, n_v0a = _montar()

    registros = _replayar(entradas, jobs, cap)
    proved = {
        r["id"]
        for r in registros
        if r.get("kind_in_output") and r.get("frame_in_output")
    }
    for e in entradas:
        e["reproduz"] = e["id"] in proved

    neg = construir_negativos()
    (C / "entradas.jsonl").write_text(
        "".join(
            json.dumps(e, ensure_ascii=False) + "\n" for e in entradas + neg["entradas"]
        ),
        encoding="utf-8",
    )

    rel = {
        "gerado_em": time.strftime("%Y-%m-%dT%H:%M:%S%z"),
        "jobs": jobs,
        "candidatos_v0a": n_v0a,
        "positivos_no_corpus": len(entradas),
        "incompletos_por_motivo": incompletos,
        "reproduzem": len(proved),
        "reproduzem_por_classe": _conta("classe", entradas, proved),
        "reproduzem_por_origem": _conta("origem", entradas, proved),
        "taxa_de_reproducao": round(len(proved) / len(entradas), 4)
        if entradas
        else 0.0,
        "negativos": {k: v for k, v in neg.items() if k != "entradas"},
        "nao_reproduzem": _nao_reproduzem(entradas, registros, proved),
        "wall_s": round(time.monotonic() - t0, 1),
    }
    (C / "relatorio.json").write_text(
        json.dumps(rel, ensure_ascii=False, indent=1), encoding="utf-8"
    )
    return rel


def selftest() -> int:
    """Positive: a clean `.py` Write on a session-work path yields a compilable snapshot, and its id is
    the deterministic one. Negative: (a) a DENIED Edit never enters the fold — the snapshot keeps the
    pre-edit body; (b) an Edit whose old_string no longer matches yields INCOMPLETE, not wrong content."""
    ok = []
    with tempfile.TemporaryDirectory(dir="/tmp") as d:
        # o arquivo TEM de morar onde `session_work` diz que ha trabalho de sessao (`/tmp/…`):
        # `rebuild_tmp` filtra por esse predicado, e um tempfile fora dele prova NADA — o
        # controle leria 'limpo' medindo uma entrada que nunca entrou no codigo sob teste.
        tmp = str(Path(d) / "alvo.py")
        recs = [
            {
                "timestamp": "1",
                "message": {
                    "content": [
                        {
                            "type": "tool_use",
                            "id": "w",
                            "name": "Write",
                            "input": {"file_path": tmp, "content": "x = 1\n"},
                        }
                    ]
                },
            },
            {
                "timestamp": "2",
                "message": {
                    "content": [
                        {
                            "type": "tool_use",
                            "id": "e",
                            "name": "Edit",
                            "input": {
                                "file_path": tmp,
                                "old_string": "x = 1",
                                "new_string": "x = 2\n",
                            },
                        }
                    ]
                },
            },
            {
                "timestamp": "2",
                "message": {
                    "content": [
                        {
                            "type": "tool_use",
                            "id": "n",
                            "name": "Edit",
                            "input": {
                                "file_path": tmp,
                                "old_string": "x = 2",
                                "new_string": "NEGADA",
                            },
                        }
                    ]
                },
            },
            {
                "timestamp": "2",
                "message": {
                    "content": [
                        {
                            "type": "tool_result",
                            "tool_use_id": "n",
                            "is_error": True,
                            "content": [
                                {
                                    "type": "text",
                                    "text": "PreToolUse:Edit hook error: negado",
                                }
                            ],
                        }
                    ]
                },
            },
        ]
        files, quebrados = replay.rebuild_tmp(recs, "3")
        ok.append(("reconstrucao", files.get(tmp) == "x = 2\n\n" and not quebrados))
        # (a) a denied Edit must not have applied: "NEGADA" absent
        ok.append(("Edit negado fora do snapshot", "NEGADA" not in files.get(tmp, "")))
        # (b) an Edit that does not match ⇒ broken, and the caller drops the entry
        recs2 = [
            recs[0],
            {
                "timestamp": "3",
                "message": {
                    "content": [
                        {
                            "type": "tool_use",
                            "id": "e2",
                            "name": "Edit",
                            "input": {
                                "file_path": tmp,
                                "old_string": "NAO EXISTE",
                                "new_string": "z",
                            },
                        }
                    ]
                },
            },
        ]
        _, quebrados2 = replay.rebuild_tmp(recs2, "9")
        ok.append(("Edit que nao casa marca quebrado", tmp in quebrados2))
        # (c) a dobra de projeto nao tem guarda de existencia: o conteudo de hoje nao vence a base
        base_txt, quebrados3 = _dobra_projeto(recs, tmp, "base_do_git\n", "3")
        ok.append(
            (
                "dobra de projeto ignora o disco",
                base_txt == "x = 2\n\n" and not quebrados3,
            )
        )
        ok.append(
            (
                "id determinista",
                ident("positivo", "s", "/a.py", "t")
                == ident("positivo", "s", "/a.py", "t")
                != ident("positivo", "s", "/a.py", "u"),
            )
        )
        ok.append(
            (
                "programa inline de heredoc",
                programa_inline("python3 - <<'PY'\nprint(1)\nPY\n") == "print(1)\n",
            )
        )
        ok.append(
            (
                "programa inline de -c",
                programa_inline("python3 -c 'print(2)'") == "print(2)\n",
            )
        )
        ok.append(
            ("sem programa ⇒ None", programa_inline("python3 -c-file.py") is None)
        )
    for nome, v in ok:
        print(f"  {'ok  ' if v else 'FALHOU'} {nome}")
    return 0 if all(v for _, v in ok) else 1


def main() -> int:
    if "--selftest" in sys.argv:
        return selftest()
    args = dict(zip(sys.argv[1::2], sys.argv[2::2]))
    rel = construir(int(args.get("--jobs", 4)), int(args.get("--timeout", CAP)))
    print(
        json.dumps(
            {k: v for k, v in rel.items() if k != "nao_reproduzem"},
            ensure_ascii=False,
            indent=1,
        )
    )
    print(f"nao reproduzem: {len(rel['nao_reproduzem'])}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
