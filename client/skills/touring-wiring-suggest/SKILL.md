---
name: touring-wiring-suggest
description: Attack orphan pub symbols via wiring suggestions. Use to reduce the 33k+ orphan symbols by finding consumers for unused public APIs.
---
# touring-wiring-suggest

## Workflow

1. Check orphan status: `touring wiring orphans -j | jq .count`
2. Get suggestions: `touring wiring suggest --top 20 -j`
3. Review each suggestion (orphan_symbol -> suggested_consumer)
4. Apply: `touring wiring suggest --apply <id>`
5. Reject: `touring wiring suggest --reject <id>`

## Commands

| Command | Purpose |
|---------|---------|
| `touring wiring suggest --top N` | Get top N wiring suggestions |
| `touring wiring suggest --apply <id>` | Apply a suggestion |
| `touring wiring orphans` | List all orphan pub symbols |
| `touring wiring modules` | Module integration scores |
| `touring wiring audit` | Full wiring audit |

## O número que esta skill move é uma cláusula do juiz

`touring wiring orphans -j` não é só um relatório: é a cláusula `orphans_base` do
`loop_converged.py`, que compara os órfãos do escopo contra a baseline e **falha**
se subiram. Duas consequências práticas: (a) criar símbolo `pub` novo sem consumidor
faz a contagem subir e trava a convergência de quem estiver em loop (REGRA #0 vira
verificável, não exortação); (b) aplicar sugestões em lote sem `cargo check` entre
elas pode derrubar `cargo_green` e mascarar qual lote quebrou. Ref: `Touring/references/skill-operating-principles.md` (P2).

## Strategy
- Target: reduce orphan rate from 96.8% to <90%
- Focus on high-similarity suggestions (score > 0.7)
- Batch apply in groups of 10-20, verify with `cargo check` between batches
