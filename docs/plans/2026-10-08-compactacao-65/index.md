---
okf_version: 1
type: LoopBundle
title: "Compactação de contexto — 9 recomendações + autocompactação aos 65%"
description: "Ordem de Gabriel (08/10/2026): implementar as 9 recomendações consolidadas, com a única alteração de autocompactar aos 65%."
plan_id: task_1791479126715571926
timestamp: 2026-10-08T15:10:00-03:00
tags: [compactacao, contexto, hooks, skills, mods]
---

# Compactação aos 65% — bundle

- [RETOMADA](/RETOMADA.md) — estado, pendências e o comando do juiz
- [Sondas do mod (item 9)](/diagnostics/sondas-mod-2026-10-08.md) — ambas negativas, mod de corte não construído
- Linhas de base: `diagnostics/resumos-antes.txt` (129 compactações), `diagnostics/piso-antes.json`
- Juiz único: `scripts/verificar_compactacao.py` (C65, C1–C9; exit 0 = convergiu)
- Scripts: `aplicar_settings.py` · `mutar_registrador.py` · `linha_de_base_resumos.py` · `medir_piso.py` · `probe-mod/`
