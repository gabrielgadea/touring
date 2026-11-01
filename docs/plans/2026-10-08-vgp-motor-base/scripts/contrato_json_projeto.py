#!/usr/bin/env python3
"""Sonda dos contratos JSON que um projeto consumidor lê do Touring — rodar ANTES e DEPOIS de um `touring update`.

Nasceu do release 30.4.96 (10/10/2026): a coordenação do analise lê `memory recall -j` → `entries`,
`memory query` → `count/shown/total/truncated`, `run` → `stdout/exit_code/success` (e `stored_path` no spill) e usa
`TOURING_TRUSTED_OK=1 touring run --allow-forbidden` para escrita intencional. Cada contrato é afirmado com um
CONTROLE POSITIVO (o campo existe E carrega o valor que a chamada produziu), nunca só pela presença da chave.

A escrita da rota confiável vai para DENTRO do projeto (o Landlock do `touring run` só concede o workspace — um
destino fora dele dá PermissionError e mediria o destino, não a rota): `<projeto>/.touring/contrato-probe-<marca>.txt`,
lido de volta e apagado (um arquivo, `os.unlink`), com a remoção conferida.

Uso: python3 contrato_json_projeto.py --projeto ~/projects/analise [--json <saida>]
Exit 0 = todos os contratos valem; 1 = algum quebrou (cada um impresso com a evidência).
"""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
import uuid
from pathlib import Path

LINHAS: list[dict] = []


def afirma(ok: bool, contrato: str, evidencia: str) -> None:
    LINHAS.append({"contrato": contrato, "ok": ok, "evidencia": evidencia[:300]})
    print(f"  {'OK   ' if ok else 'FALHA'} {contrato} — {evidencia[:160]}")


def chamar(
    binario: Path, args: list[str], cwd: Path, extra: dict[str, str] | None = None
) -> tuple[int, str, str]:
    env = {**os.environ, **(extra or {})}
    # The project's own daemon must answer, never the session's global socket (CLAUDE.md §5).
    env.pop("TOURING_DAEMON_SOCKET", None)
    env.pop("TOURING_DAEMON_SOCK", None)
    r = subprocess.run(
        [str(binario), *args],
        cwd=cwd,
        env=env,
        capture_output=True,
        text=True,
        timeout=120,
        check=False,
    )
    return r.returncode, r.stdout, r.stderr


def como_json(saida: str) -> dict:
    try:
        v = json.loads(saida)
    except json.JSONDecodeError:
        return {}
    return v if isinstance(v, dict) else {}


def contrato_recall(binario: Path, proj: Path) -> None:
    rc, out, err = chamar(binario, ["memory", "recall", "touring", "-j"], proj)
    v = como_json(out)
    afirma(
        rc == 0 and isinstance(v.get("entries"), list),
        "memory recall -j → entries (lista)",
        f"rc {rc}, chaves {sorted(v) or err[:80]}",
    )


def contrato_query(binario: Path, proj: Path) -> None:
    rc, out, _ = chamar(binario, ["memory", "query", "#kind:lesson", "-j"], proj)
    chaves = {"count", "shown", "total", "truncated"}
    tem = chaves & set(como_json(out))
    afirma(
        rc == 0 and tem == chaves,
        "memory query -j → count/shown/total/truncated",
        f"rc {rc}, presentes {sorted(tem)}",
    )


def contrato_run(binario: Path, proj: Path, marca: str) -> None:
    rc, out, _ = chamar(
        binario, ["run", "--lang", "python", "--code", f"print('{marca}')"], proj
    )
    v = como_json(out)
    ok = (
        marca in str(v.get("stdout", ""))
        and v.get("exit_code") == 0
        and v.get("success") is True
    )
    afirma(
        ok,
        "run → stdout/exit_code/success (controle: o marcador volta no stdout)",
        f"rc {rc}, chaves {sorted(v)}",
    )
    # `stored_path` só existe quando há spill; o que o leitor precisa para decidir é a chave `stdout_truncated`.
    afirma(
        "stdout_truncated" in v,
        "run → stdout_truncated presente (o leitor decide quando buscar stored_path)",
        f"chaves {sorted(v)}",
    )


def contrato_escrita_confiavel(binario: Path, proj: Path, marca: str) -> None:
    alvo = proj / ".touring" / f"contrato-probe-{marca}.txt"
    prog = f"from pathlib import Path; Path({str(alvo)!r}).write_text({marca!r}); print('escrevi')"
    rc, out, _ = chamar(
        binario,
        ["run", "--lang", "python", "--allow-forbidden", "--code", prog],
        proj,
        {"TOURING_TRUSTED_OK": "1"},
    )
    escrito = alvo.read_text() if alvo.exists() else None
    stderr_prog = str(como_json(out).get("stderr", ""))[-120:]
    afirma(
        escrito == marca,
        "TOURING_TRUSTED_OK=1 run --allow-forbidden escreve no projeto (controle: conteúdo lido de volta)",
        f"rc {rc}, arquivo {'com o marcador' if escrito == marca else repr(escrito)}; stderr do programa {stderr_prog!r}",
    )
    if alvo.exists():
        os.unlink(alvo)
    afirma(not alvo.exists(), "a sonda não deixa resíduo no projeto", str(alvo))


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--projeto", type=Path, required=True)
    ap.add_argument("--json", type=Path)
    a = ap.parse_args()
    proj = a.projeto.expanduser().resolve()
    binario = proj / ".touring/bin/touring"
    rc, _, err = chamar(
        binario, ["--version"], proj
    )  # a versão sai em STDERR (CLAUDE.md §2.1b)
    versao = err.strip().splitlines()[0] if err.strip() else "?"
    print(f"projeto {proj} · {versao} (rc {rc})")
    marca = f"contrato-{uuid.uuid4().hex[:8]}"
    contrato_recall(binario, proj)
    contrato_query(binario, proj)
    contrato_run(binario, proj, marca)
    contrato_escrita_confiavel(binario, proj, marca)
    falhas = [linha["contrato"] for linha in LINHAS if not linha["ok"]]
    if a.json:
        a.json.write_text(
            json.dumps(
                {
                    "projeto": str(proj),
                    "versao": versao,
                    "contratos": LINHAS,
                    "falhas": falhas,
                },
                ensure_ascii=False,
                indent=1,
            )
        )
    print(f"contratos: {len(LINHAS) - len(falhas)}/{len(LINHAS)} valem")
    return 1 if falhas else 0


if __name__ == "__main__":
    sys.exit(main())
