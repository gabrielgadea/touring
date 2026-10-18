---
name: checkpoint
description: Checkpoint do trabalho em curso — snapshot WIP em ref safety/ (sem tocar índice, HEAD nem branch) + touring memory store. Nunca git stash (REGRA #11).
argument-hint: "<tópico>"
---
# /checkpoint — snapshot de segurança + memória (REGRA #11)

Guarda o estado do trabalho em dois lugares independentes, sem tocar em nada do que está em andamento:

1. **SNAPSHOT** — um commit WIP da working tree inteira (rastreado modificado + não rastreado, menos o `.gitignore`)
   numa ref **nova** `safety/<AAAA-MM-DD>/<tópico>-<HHMMSS>`. É aditivo: o `scripts/snapshot_safety.py` usa um índice
   temporário + `commit-tree` + `update-ref`; o **índice, o HEAD, as branches e os arquivos ficam byte a byte como
   estavam**, ref existente nunca é sobrescrita, nada é empurrado. **Nunca `git stash`** (destruiu 162 módulos em
   06/04/2026; REGRA #11).
2. **MEMÓRIA** — `touring memory store --tier semantic` com o que o resumo da compactação perde: estado, decisões,
   hipóteses descartadas e o próximo comando. É o que a próxima sessão recupera com `touring memory recall`.

O `touring learning reward` só é registrado quando o snapshot saiu **medido** (`OK` ou `COBERTO-PELO-HEAD`).

## Uso

```
/checkpoint <tópico>
```

Exemplos: `/checkpoint onda1-hooks` · `/checkpoint antes do refactor do wiring`. Sem tópico, responda
`Uso: /checkpoint <tópico>` e pare.

## Execução

Faça nesta ordem, em UMA chamada Bash. Antes dela, escreva o resumo no lugar de `<RESUMO>` (≤ 12 linhas, em pt-BR,
com caminhos, comandos e ids **literais**: o pedido do Gabriel entre aspas, o que está feito por evidência, o que
falta, hipóteses descartadas com a evidência, o próximo comando exato, DAG/bundle/RETOMADA se houver). Não resuma
com adjetivos: o resumo é para quem retoma sem reler nada.

```bash
set -u
TOPIC=$(cat <<'__TOPIC__'
$ARGUMENTS
__TOPIC__
)
if [ -z "${TOPIC//[[:space:]]/}" ]; then echo "Uso: /checkpoint <tópico>"; exit 2; fi
SUMMARY=$(cat <<'__RESUMO__'
<RESUMO>
__RESUMO__
)
SLUG=$(printf '%s' "$TOPIC" | python3 -c 'import re,sys,unicodedata as u; t=u.normalize("NFKD",sys.stdin.read()).encode("ascii","ignore").decode().lower(); print(re.sub(r"[^a-z0-9]+","-",t).strip("-")[:48] or "checkpoint")')
DAY=$(date +%F); STAMP=$(date +%H%M%S)

# 1) SNAPSHOT — aditivo; falha é barulhenta (exit 1 no fim), nunca silenciosa
SNAP_STATUS="n/a"; SNAP_REF=""; SNAP_OK=0
ROOT=$(git rev-parse --show-toplevel 2>/dev/null) || ROOT=""
SNAP_SCRIPT=""
for candidate in "$HOME/projects/touring/scripts/snapshot_safety.py" \
                 "$HOME/projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts/snapshot_safety.py"; do
  if [ -f "$candidate" ]; then SNAP_SCRIPT=$candidate; break; fi
done
if [ -z "$ROOT" ]; then
  echo "SNAPSHOT: n/a — fora de um repositório git"
elif [ -z "$SNAP_SCRIPT" ]; then
  SNAP_STATUS="FALHOU"; echo "SNAPSHOT: FALHOU — snapshot_safety.py não encontrado (scripts/ nem o bundle vgp-motor-base)"
else
  SNAP_OUT=$(python3 "$SNAP_SCRIPT" --prefix "safety/$DAY" "$SLUG-$STAMP=$ROOT" 2>&1); SNAP_RC=$?
  SNAP_JSON=$(printf '%s\n' "$SNAP_OUT" | tail -n 1)
  echo "SNAPSHOT: $SNAP_JSON"
  SNAP_STATUS=$(printf '%s' "$SNAP_JSON" | python3 -c 'import json,sys; print(json.loads(sys.stdin.read() or "{}").get("status","FALHOU"))' 2>/dev/null || echo FALHOU)
  SNAP_REF=$(printf '%s' "$SNAP_JSON" | python3 -c 'import json,sys; print(json.loads(sys.stdin.read() or "{}").get("ref",""))' 2>/dev/null || true)
  case "$SNAP_STATUS" in OK|COBERTO-PELO-HEAD) [ "$SNAP_RC" -eq 0 ] && SNAP_OK=1 ;; esac
fi

# 2) MEMÓRIA — o estado vale mesmo quando o snapshot falhou
KEY="checkpoint:$SLUG:$DAY-$STAMP"
touring memory store "$KEY" "$SUMMARY | snapshot=${SNAP_REF:-$SNAP_STATUS} | root=${ROOT:-fora-de-repo}" \
  --tier semantic --tag "#kind:outcome"; MEM_RC=$?
echo "MEMÓRIA: $KEY (rc=$MEM_RC)"

# 3) REWARD — só sobre outcome medido
if [ "$SNAP_OK" -eq 1 ] && [ "$MEM_RC" -eq 0 ]; then
  touring learning reward checkpoint 1.0 "$SLUG" >/dev/null && echo "REWARD: checkpoint +1.0"
fi

# veredito pelo exit code: snapshot que falhou ou memória que não gravou NÃO é checkpoint feito
if [ "$SNAP_STATUS" = "FALHOU" ] || [ "$MEM_RC" -ne 0 ] || { [ -n "$ROOT" ] && [ "$SNAP_OK" -ne 1 ]; }; then exit 1; fi
```

## Ler o resultado

| Linha | Significa |
|---|---|
| `SNAPSHOT: {... "status": "OK", "ref": "refs/heads/safety/…"}` | working tree guardada; `git show <ref>:<arquivo>` recupera qualquer arquivo |
| `status: COBERTO-PELO-HEAD` | nada mudou desde o HEAD; não há ref a criar |
| `status: RECUSADO` | ref já existia, ou um arquivo passou do teto de 50 MiB (`acima_do_teto` o nomeia); repita o comando |
| `SNAPSHOT: n/a` | fora de repositório git: só a memória foi gravada, sem reward |
| `SNAPSHOT: FALHOU` / exit 1 | nada foi guardado no git — diga ao Gabriel antes de seguir com qualquer operação arriscada |

Restaurar do snapshot é operação **destrutiva** (`checkout -- <path>`/`restore`): só sob o ritual da REGRA #11
(medir → confirmar com o Gabriel → `GIT_DESTRUCTIVE_OK=1`). Para só LER um arquivo da ref, use
`git show <ref>:<caminho>` (leitura livre).

## Ver também

- `~/.claude/rules/git-safety-protocol.md` — o ritual MEDIR → SNAPSHOT → CONFIRMAR → EXECUTAR → VERIFICAR
- `~/projects/touring/scripts/snapshot_safety.py` — o snapshot (`--dry-run` mede sem gravar)
- `/plan` — o comando irmão para planos prospectivos
