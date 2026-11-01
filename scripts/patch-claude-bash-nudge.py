#!/usr/bin/env python3
"""Neutraliza a instrução nativa do Claude Code que manda usar cat/grep/sed.

## O problema

Em `permissions.defaultMode` ∈ {`auto`, `bypassPermissions`}, o binário do
Claude Code injeta no contexto de cada PreToolUse:

    While auto mode is active:

    Do your work through the Bash tool wherever it can accomplish the job:
    read files with cat, head, or sed -n, search with grep and find, and make
    file changes with sed, heredocs, or short scripts, rather than using the
    dedicated Read, Edit tools. ...

Isso **contradiz** dois contratos deste ambiente:

1. **Code mode por escopo** — em workspaces com `[code_mode] mode = "code"`
   (ex.: `~/projects/touring`), inspeção `grep`/`cat`/`find` modelo-direta é
   NEGADA pelo hook, com a rota `touring run` devolvida no lugar.
2. **Edição com gate** — código muda por Edit/Write (17 stage gates: VGP,
   blast, TDG, snapshot atômico, gotcha, wiring delta, RL reward), nunca por
   `sed -i`, que atravessa todos eles.

A precedência já está correta no papel (instruções do usuário vencem defaults
do harness) e já é APLICADA pelo executor — provado ao vivo: com essa instrução
ativa mandando usar `grep`, o hook respondeu `permissionDecision: "deny"`. Mas
ela continua custando contexto em toda chamada e obrigando o modelo a arbitrar
um conflito que não deveria existir. Ordem de Gabriel (26/08/2026): eliminar na
origem.

## A correção

Os fragmentos são substituídos por espaços **do mesmo comprimento em bytes**.
Isso é seguro porque o formato embute o comprimento da string ANTES dela (Bun
serializa `<len:u32><bytes>`): preservar o tamanho preserva o layout inteiro —
nenhum offset se move, nenhum índice quebra.

## Idempotência e ciclo de vida

- `--check`  diz o estado atual (exit 0 = patchado, 1 = não, 2 = não pôde ver).
- `--apply`  aplica; roda de novo sem efeito se já patchado.
- `--restore` volta do backup `.orig` ao lado do binário.

**Um update do Claude Code (mise) reinstala o binário e DESFAZ o patch.** Não
há como evitar isso — o remédio é reaplicar: `--apply` depois de cada update.
`--check` no shell profile avisa quando voltou.
"""

from __future__ import annotations

import argparse
import os
import shutil
import subprocess
import sys
from pathlib import Path

# Os fragmentos que compõem a instrução, exatamente como vivem no binário.
# Cada um é substituído por espaços do MESMO tamanho. A busca é por conteúdo
# (nunca por offset fixo): o offset muda a cada versão do Claude Code, o
# conteúdo não — é isso que torna o script portátil entre versões.
FRAGMENTS: list[bytes] = [
    b"While bypass permissions mode is active:\n\n",
    b"While auto mode is active:\n\n",
    b"Do your work through the ",
    b" tool wherever it can accomplish the job: read files with cat, head, or "
    b"sed -n, search with grep and find, and make file changes with sed, "
    b"heredocs, or short scripts, rather than using the dedicated ",
    b" tools. Fall back to a dedicated tool only when ",
    b" genuinely cannot do the job.",
]


def resolve_binary(explicit: str | None) -> Path | None:
    if explicit:
        p = Path(explicit)
        return p if p.exists() else None
    which = shutil.which("claude")
    if not which:
        return None
    return Path(os.path.realpath(which))


def scan(data: bytes) -> tuple[list[tuple[bytes, int]], list[bytes]]:
    """Return (found, missing) for the fragment list."""
    found: list[tuple[bytes, int]] = []
    missing: list[bytes] = []
    for frag in FRAGMENTS:
        idx = data.find(frag)
        if idx >= 0:
            found.append((frag, idx))
        else:
            missing.append(frag)
    return found, missing


def cmd_check(binary: Path) -> int:
    data = binary.read_bytes()
    found, missing = scan(data)
    print(f"binário: {binary}")
    print(f"tamanho: {len(data):,} bytes")
    if not found:
        print("ESTADO: PATCHADO — nenhum fragmento da instrução presente.")
        return 0
    if not missing:
        print(f"ESTADO: NÃO PATCHADO — {len(found)}/{len(FRAGMENTS)} fragmentos presentes:")
        for frag, idx in found:
            print(f"  offset {idx:>12,}  {frag[:60]!r}...")
        return 1
    print(
        f"ESTADO: PARCIAL — {len(found)} presentes, {len(missing)} ausentes. "
        "Provável nova versão do Claude Code com texto alterado; rode --apply "
        "e confira se os ausentes ainda existem em outra forma."
    )
    for frag in missing:
        print(f"  AUSENTE: {frag[:60]!r}...")
    return 1


def cmd_apply(binary: Path, *, backup: bool = True, quiet: bool = False) -> int:
    data = bytearray(binary.read_bytes())
    found, missing = scan(bytes(data))
    if not found:
        if not quiet:
            print("já patchado — nada a fazer (idempotente).")
        return 0

    if backup:
        orig = binary.with_suffix(binary.suffix + ".orig")
        if not orig.exists():
            shutil.copy2(binary, orig)
            if not quiet:
                print(f"backup: {orig}")
        elif not quiet:
            print(f"backup já existe (preservado): {orig}")

    patched = 0
    for frag, _ in found:
        # Um fragmento pode ocorrer mais de uma vez; neutraliza todas.
        start = 0
        while True:
            idx = data.find(frag, start)
            if idx < 0:
                break
            data[idx : idx + len(frag)] = b" " * len(frag)
            patched += 1
            start = idx + len(frag)

    # A escrita preserva o modo (o binário precisa continuar executável).
    mode = binary.stat().st_mode
    tmp = binary.with_suffix(binary.suffix + ".patching")
    tmp.write_bytes(bytes(data))
    os.chmod(tmp, mode)
    os.replace(tmp, binary)
    if not quiet:
        print(f"patch aplicado: {patched} ocorrência(s) neutralizada(s).")
        if missing:
            print(f"AVISO: {len(missing)} fragmento(s) não encontrado(s) — texto pode ter mudado de versão.")
    return 0


STAMP = Path.home() / ".claude" / "tools" / ".claude-nudge-patch.stamp"


def binary_identity(binary: Path) -> str:
    """Identidade barata do binário: caminho + tamanho + mtime.

    Um update do mise instala em outro diretório de versão E/OU reescreve o
    arquivo, mudando mtime — qualquer um dos três muda a identidade. Isso
    evita varrer 392 MB a cada sessão só para descobrir que nada mudou.
    """
    st = binary.stat()
    return f"{binary}|{st.st_size}|{int(st.st_mtime_ns)}"


def cmd_ensure(binary: Path) -> int:
    """Garante o patch, barato o suficiente para rodar a cada sessão.

    Se a identidade do binário confere com o stamp, sai em O(1) sem ler o
    arquivo. Só quando o binário mudou (update) é que varre e reaplica.
    """
    ident = binary_identity(binary)
    try:
        if STAMP.read_text(encoding="utf-8").strip() == ident:
            return 0  # inalterado desde o último patch — nada a fazer.
    except OSError:
        pass  # sem stamp (primeira vez) ou ilegível → segue para o scan.

    data = binary.read_bytes()
    found, _ = scan(data)
    if found:
        rc = cmd_apply(binary, backup=True, quiet=True)
        if rc != 0:
            return rc
    # Re-derive a identidade DEPOIS do patch: o apply reescreve o arquivo e
    # muda mtime; gravar a identidade antiga faria o próximo run varrer de novo.
    try:
        STAMP.parent.mkdir(parents=True, exist_ok=True)
        STAMP.write_text(binary_identity(binary), encoding="utf-8")
    except OSError:
        return 0  # não conseguir gravar o stamp nunca falha a sessão.
    return 0


def cmd_restore(binary: Path) -> int:
    orig = binary.with_suffix(binary.suffix + ".orig")
    if not orig.exists():
        print(f"sem backup em {orig} — nada a restaurar.")
        return 1
    mode = binary.stat().st_mode
    shutil.copy2(orig, binary)
    os.chmod(binary, mode)
    print(f"restaurado de {orig}")
    return 0


def cmd_verify_runs(binary: Path) -> int:
    """Prova que o binário patchado ainda executa."""
    try:
        r = subprocess.run(
            [str(binary), "--version"], capture_output=True, text=True, timeout=60
        )
    except Exception as exc:  # noqa: BLE001
        print(f"FALHOU ao executar: {type(exc).__name__}: {exc}")
        return 1
    out = (r.stdout + r.stderr).strip()
    print(f"exit={r.returncode} saída={out[:120]!r}")
    return 0 if r.returncode == 0 and out else 1


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--binary", help="caminho explícito (default: resolve `claude` no PATH)")
    g = ap.add_mutually_exclusive_group(required=True)
    g.add_argument("--check", action="store_true", help="reporta o estado")
    g.add_argument("--apply", action="store_true", help="aplica (idempotente)")
    g.add_argument(
        "--ensure",
        action="store_true",
        help="garante o patch em O(1) via stamp; reaplica só se o binário mudou "
        "(uso: hook SessionStart, silencioso)",
    )
    g.add_argument("--restore", action="store_true", help="volta do backup .orig")
    g.add_argument("--verify-runs", action="store_true", help="executa --version")
    ap.add_argument("--no-backup", action="store_true")
    ap.add_argument("--quiet", action="store_true")
    args = ap.parse_args()

    binary = resolve_binary(args.binary)
    if binary is None:
        print("UNVERIFIED: binário do Claude Code não encontrado no PATH.")
        return 2

    if args.check:
        return cmd_check(binary)
    if args.apply:
        return cmd_apply(binary, backup=not args.no_backup, quiet=args.quiet)
    if args.ensure:
        return cmd_ensure(binary)
    if args.restore:
        return cmd_restore(binary)
    if args.verify_runs:
        return cmd_verify_runs(binary)
    return 2


if __name__ == "__main__":
    sys.exit(main())
