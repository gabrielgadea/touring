---
okf_version: "0.1"
type: LoopBundle
title: "Migração rkyv 0.7 → 0.8 (RUSTSEC-2026-0235)"
description: "Bundle da investigação e do plano de migração do rkyv, motivado por RUSTSEC-2026-0235 (leitura fora de limites em arquivos com Rc/Arc)."
plan_id: 2026-08-07-rkyv-migration
tags: [rkyv, security, migration, plan]
timestamp: "2026-08-07T07:30:00-03:00"
---

# Bundle — migração rkyv 0.7.46 → 0.8.17

Objetivo: sair de `rkyv 0.7.46` (afetado por RUSTSEC-2026-0235, série EOL) para
`>= 0.8.17` **por correção, não por adiamento**, sem quebrar o formato de fio do
IPC nem os caches em disco.

## Documentos

| Doc | Conteúdo |
|---|---|
| [/strategy-2026-08-07-rkyv-0-8-migration.md](/strategy-2026-08-07-rkyv-0-8-migration.md) | Diagnóstico medido, mapeamento de API, as duas compatibilidades e o plano em 5 fases |
| [/diagnostics/touring-20260807T071934.md](/diagnostics/touring-20260807T071934.md) | Diagnóstico determinístico do workspace |
| [/phases/P0.md](/phases/P0.md) | SPIKE — o 0.8 **recusa** arquivos 0.7 (`invalid UTF-8`); nenhum caminho de corrupção silenciosa |
| [/phases/P1.md](/phases/P1.md) | Funil — 71 substituições; zero `rkyv::` direto fora da fachada |
| [/phases/P2.md](/phases/P2.md) | Salto de versão — adaptadores, 21 atributos de derive, 3 defeitos upstream |
| [/phases/P3.md](/phases/P3.md) | Fio versionado + assimetria C08 no handler saga |
| [/phases/P4.md](/phases/P4.md) | Fecho — `cargo deny` verde sem `ignore`, docs co-evoluídas, lição persistida |
| [/phases/P5.md](/phases/P5.md) | O modelo de wiring media a coisa errada — 3 defeitos no resolvedor, F2.6 cego, EAGAIN era timeout |
| [/knowledge/P5.json](/knowledge/P5.json) | Abstract tipado da P5 — as 4 classes de órfão que um modelo baseado em `use` não enxerga |
| [/log.md](/log.md) | Histórico cronológico |

## A alavanca

`crates/touring-rkyv/src/lib.rs` **já é uma fachada** sobre o rkyv. Só 11
arquivos a contornam. Canalizá-los primeiro (fase P1, ainda em 0.7) converte uma
migração de 88 call sites numa troca de **um crate** — é o que torna a migração
exitosa em vez de arriscada.

## Ledger CCE

`.touring-explore/migração-rkyv-0-7-46---0-8-17--rustsec-2026-0235.ledger.json`
— convergido, 2 rodadas secas. Lente externa marcada: Context7
`/websites/rs_rkyv` (API 0.8 confirmada).
