#!/usr/bin/env bash
# Verifica o D07 sem atualizar toolchain, daemon ou auditoria OSV ao vivo.
set -euo pipefail

BUNDLE="$(realpath "$(dirname "$0")")"
ROOT="$(realpath "$BUNDLE/../../..")"
MAIN="/home/gabrielgadea/projects/analise"
CRATE="$ROOT/crates/touring-quality"
DIAG="$BUNDLE/diagnostics"
PY="$MAIN/.venv/bin/python3"
export TOURING_CODE_MODE=native

cargo fmt --manifest-path "$CRATE/Cargo.toml" -- --check > "$DIAG/fmt_final.log" 2>&1
systemd-run --user --scope --slice=cargo-build.slice --same-dir --collect \
  cargo test --manifest-path "$CRATE/Cargo.toml" > "$DIAG/testes_crate.log" 2>&1
systemd-run --user --scope --slice=cargo-build.slice --same-dir --collect \
  cargo clippy --manifest-path "$CRATE/Cargo.toml" --all-targets -- -D warnings \
  > "$DIAG/clippy_crate.log" 2>&1
systemd-run --user --scope --slice=cargo-build.slice --same-dir --collect \
  cargo build --manifest-path "$CRATE/Cargo.toml" --bin touring-quality \
  > "$DIAG/build_binario.log" 2>&1
"$ROOT/target/debug/touring-quality" check --gate F2.5 --target "$MAIN" \
  > "$DIAG/f2_5_analise.log" 2>&1
"$PY" -c 'import pathlib,sys; t=pathlib.Path(sys.argv[1]).read_text(); assert "PYSEC-2026-2447" in t; assert "accepted risk until 2026-12-05" in t; assert "0 known vulnerabilities" in t' \
  "$DIAG/f2_5_analise.log"
git -C "$ROOT" diff --check
"$MAIN/.venv/bin/ruff" check "$DIAG/registrar_licao.py" "$DIAG/registrar_licao_auditoria.py" > "$DIAG/ruff_final.log" 2>&1
"$MAIN/.venv/bin/pyright" "$DIAG/registrar_licao.py" "$DIAG/registrar_licao_auditoria.py" > "$DIAG/pyright_final.log" 2>&1

find "$BUNDLE" -type f -name '*.pyc' -exec unlink {} +
find "$BUNDLE" -type d -name '__pycache__' -exec rmdir {} +
"$PY" "$MAIN/scripts/lexhub/gate_pii_acervo.py" \
  "$CRATE/src/verifications/python_deps.rs" "$BUNDLE" > "$DIAG/gate_pii_final.log" 2>&1
"$PY" "$MAIN/scripts/utils/gate_prosa_cjk.py" --paths \
  "$CRATE/src/verifications/python_deps.rs" "$BUNDLE/RETOMADA.md" \
  "$BUNDLE/strategy-2026-11-05-d07-risco-aceito.md" \
  "$BUNDLE/docs/audits/cross-audit-2026-11-05.md" > "$DIAG/gate_cjk_final.log" 2>&1
"$PY" "$MAIN/scripts/design_elite/gate_artefato.py" \
  "$BUNDLE/RETOMADA.md" "$BUNDLE/strategy-2026-11-05-d07-risco-aceito.md" \
  "$BUNDLE/docs/audits/cross-audit-2026-11-05.md" \
  --out-dir "$BUNDLE/qa" --strict > "$DIAG/gate_md_final.log" 2>&1
"$PY" "$HOME/.claude/skills/loop-engineering/scripts/loop_doc_link_gate.py" \
  --bundle "$BUNDLE" --json > "$DIAG/doc_link_gate.json" 2>&1

printf '{"plano":"d07-risco-aceito","f2_5_analise":"PYSEC-2026-2447 aceito ate 2026-12-05","clippy":"0 warnings"}\n'
