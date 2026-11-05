#!/usr/bin/env bash
_HOME_SLUG=$(printf '%s' "$HOME" | tr -c 'A-Za-z0-9' '-')  # Claude Code project-dir slug of $HOME
# Medida de uma VARIANTE (binario debug, so achados importam): por id nos dois corpora, o juiz no natural
# (recall/falso deny/CP95/avisos) e o controle negativo dos 130 .rs. Rodar sob `hw-slot` da classe heavy.
# Uso: medida_var.sh <V> <binario>
SC=/tmp/claude-1000/${_HOME_SLUG}-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/vrs
B=${HOME}/projects/touring/docs/plans/2026-10-08-vgp-motor-base
VGP=${HOME}/.claude/touring/vgp
v=$1
bin=$2
python3 -I "$SC/medir_ids.py" --touring "$bin" --corpus "$SC/corpus_rust_norm" --out "$SC/var_${v}_ids_sanidade.json" > "$SC/var_${v}.log" 2>&1
python3 -I "$SC/medir_ids.py" --touring "$bin" --corpus "$VGP/corpus_rust_natural" --out "$SC/var_${v}_ids_natural.json" >> "$SC/var_${v}.log" 2>&1
python3 -I "$B/scripts/juiz_motor_vgp.py" --touring "$bin" --corpus "$VGP/corpus_rust_natural" --exigir-limite-superior \
  --json "$SC/var_${v}_juiz_natural.json" > "$SC/var_${v}_juiz_natural.log" 2>&1
python3 -I "$SC/controle130.py" --manifesto "$SC/manifesto_130.json" --touring "$bin" --out "$SC/var_${v}_controle130.json" \
  > "$SC/var_${v}_controle130.txt" 2>&1
echo "var $v concluida" >> "$SC/var_${v}.log"
