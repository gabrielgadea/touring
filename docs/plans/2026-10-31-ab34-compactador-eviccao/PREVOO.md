# AB34 — registro dos pré-voos (R0 do §8)

## P1 — o transcrito guarda a saída compactada? **PASSOU** (31/10/2026 22:03 BRT)

- `python3 ab34_arm.py arm --force-cell C- --yes` → daemon reiniciado com `TOURING_RESULT_COMPACTOR=on`
  (`event: preflight`, calendário com `day0: null`).
- Sessão nova headless (`claude -p --model haiku`, sessão `b85fd0d8-80f8-44c7-ba0a-9b507e93f117`)
  rodou `python3 -c "print('\n'.join(f'compiling crate {i} ... ok' for i in range(300)))"` (exit 0).
- Disco: arquivo novo `~/.claude/touring/compacted/fa2ad0eeb509abc0269b95c502d4c1ed.txt`, 7.989 B
  (o diretório estava vazio antes).
- Transcrito: o `tool_result` tem **1.396 B** e contém o rodapé `[touring result-compactor]`
  (e o `toolUseResult.stdout`, 1.397 B, idem). **O transcrito guarda a saída substituída**: o
  desfecho primário enxerga C. Não há redesenho.
- Volta ao zero (P5): `python3 ab34_arm.py reset --yes` → `status` com as duas flags `None` e
  `/proc/<pid>/environ` do daemon sem `TOURING_RESULT_COMPACTOR` (contagem 0).
- Suíte do bundle antes de armar: `python3 -m pytest -q -p no:cacheprovider` → 150 passed.

Limite: a sessão do pré-voo foi headless, não interativa; o caminho do hook PostToolUse é o mesmo
nos dois modos (o arquivo em `compacted/` e o rodapé vieram do daemon).
