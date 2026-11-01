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

## P2 — mod instalado e congelado por cópia: **PASSOU** (22:04)

- `tools/mods/touring-compact-evict` sem mudança desde `394fc635` (`git diff --quiet`); cópia em
  `~/.claude/ab34-marketplace/touring-compact-evict` com os 4 hashes do pré-registro conferidos
  (`df8beebd6fb0e41c` · `bd8c0060c8a06389` · `c835c92d8e1159d2` · `1a39009232c4fae6`).
- CC **2.1.316**: `claude plugin validate` (marketplace e mod) → `Validation passed` (só o aviso
  de README sem linha de instalação); `claude plugin test` → **30 pass, 0 fail**.
- `marketplace add` + `install touring-compact-evict@ab34-local` → `Status: enabled`, scope user.

## P3 — a flag do settings chega ao mod, e o mod age: **PASSOU** (22:05)

- `arm --force-cell=-E --yes` (o runbook dizia `--force-cell -E`, que o argparse recusa:
  `expected one argument`; o §8 foi corrigido para a forma com `=`).
- Sessão headless nova (`122d07fb-eb8c-45d6-b996-aa1c5e6fb13f`): Read de um arquivo de 97.890 B
  no scratchpad, Edit, Read de novo; depois `claude -p --resume <id> "/compact"`.
- `~/.claude/touring/evicted/` estava vazio; ficou com 2 arquivos novos (56.206 B e 48.081 B), com
  800 e 700 linhas do arquivo lido: o `env` do settings chega ao `$.env.get` e o mod evicta.

## P4 — dry-run e estado: **PASSOU**

- `status` mostra as duas flags; `arm` sem `--yes` e com `day0: null` recusa com exit 4
  (`experimento não ativo (day0 não definido): nada a armar`), sem gravar.

## P5 — volta ao zero: **PASSOU**

- `reset --yes` depois do P1 e do P3; `TOURING_COMPACT_EVICT` ausente do settings (contagem 0) e
  `TOURING_RESULT_COMPACTOR` ausente do `/proc/<pid>/environ` do daemon (contagem 0).

**Veredito do R0: go.** Próximo passo (R1): escolher `day0`, gravar no calendário e ligar os timers.
