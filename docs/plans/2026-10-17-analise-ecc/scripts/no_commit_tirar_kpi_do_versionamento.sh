#!/usr/bin/env bash
# Decisão D4 (17/10/2026): rodar NO MOMENTO DO COMMIT autorizado pelo Gabriel (não antes: o índice é compartilhado).
# Tira do versionamento os snapshots datados de docs/kpi; os arquivos FICAM no disco (--cached).
set -euo pipefail
cd "$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
git ls-files -z 'docs/kpi/20[0-9][0-9]-[0-9][0-9]/*' | xargs -0 -r git rm --cached --quiet --
# Estado de runtime versionado (172 arquivos sob */.claude/{touring,data}/), mesma decisão.
git ls-files -z -ci --exclude-standard | grep -z -E '/\.claude/(touring|data)/' | xargs -0 -r git rm --cached --quiet --
# Arquivo criado por doctest dentro do crate (C13; o doctest passa a usar diretório temporário).
git ls-files -z crates/touring-intelligence/test.db | xargs -0 -r git rm --cached --quiet --
echo "retirados do índice: $(git diff --cached --name-only --diff-filter=D | wc -l) (arquivos continuam no disco)"
