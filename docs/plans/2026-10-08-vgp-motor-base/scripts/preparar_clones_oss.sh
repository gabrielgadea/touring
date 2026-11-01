#!/usr/bin/env bash
# Deixa os clones OSS com os BLOBS que a janela de commits usa — de uma vez, e visivel.
#
# MEDIDO em 01/11 (V-8): os clones sao PARTIAIS (`remote.origin.promisor = true`): o
# `excalidraw` tem 21.169 blobs ausentes e o `zod` 13.576. O `git cat-file --batch` encontra
# um blob ausente e dispara um LAZY FETCH — **um round-trip HTTP por blob**. Medido: 24 min
# de coleta com o git em 1,8 % de CPU e um `git fetch` novo a cada 9 segundos. Um lote de
# 13.572 blobs precisaria de ~10.000 fetches: inviavel, e o lote nao termina.
#
# Este script faz o que o lazy fetch faria, mas em UMA negociacao por repo:
#   1. `git rev-list --objects --missing=print` na janela de commits -> os oids ausentes;
#   2. `git fetch origin <oids...>` -> um pack so.
# Depois disso a rotulagem corre OFFLINE e DETERMINISTA: o mesmo comando produz a mesma
# camada em qualquer maquina (com os blobs em cache).
#
# O leitor tem `GIT_NO_LAZY_FETCH=1`: se um blob ainda faltar, ele vem como "missing" e o
# par e descartado — nunca um travamento.
#
# Uso: preparar_clones_oss.sh [--max-commits N] [--repos a,b,c]
set -uo pipefail

OSS="$HOME/.claude/touring/vgp/oss"
MAX_COMMITS=1000
REPOS="excalidraw,zod,date-fns,html5-boilerplate,express,lodash,dom-examples"
CORTE=$(date -d '2026-10-09 23:00:00' +%s)

while [ $# -gt 0 ]; do
  case "$1" in
    --max-commits) MAX_COMMITS="$2"; shift 2;;
    --repos) REPOS="$2"; shift 2;;
    *) echo "uso desconhecido: $1" >&2; exit 2;;
  esac
done

total_faltando=0
for repo in ${REPOS//,/ }; do
  dir="$OSS/$repo"
  [ -d "$dir/.git" ] || { echo "  $repo: sem .git, pulado"; continue; }
  antes=$(du -sm "$dir/.git" 2>/dev/null | cut -f1)
  oids=$(git -C "$dir" rev-list --objects --missing=print --max-count="$MAX_COMMITS" \
           --before="@$CORTE" HEAD 2>/dev/null | awk '/^\?/ {print $2}')
  n=$(printf '%s\n' "$oids" | grep -c . )
  echo "$repo: $n blobs ausentes na janela de $MAX_COMMITS commits"
  if [ "$n" -gt 0 ]; then
    # o git aceita muitos oids numa negociacao so; em lotes, para nao estourar a linha de comando
    printf '%s\n' "$oids" | grep . | xargs -n 400 git -C "$dir" fetch --no-tags --quiet origin 2>&1 | tail -3
    total_faltando=$((total_faltando + n))
  fi
  depois=$(du -sm "$dir/.git" 2>/dev/null | cut -f1)
  echo "  $repo: .git ${antes}MB -> ${depois}MB"
done

echo "total de blobs ausentes na janela: $total_faltando (buscados)"
