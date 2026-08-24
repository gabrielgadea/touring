# Generate & Evolution — comandos preservados de duas skills removidas

> **Origem** (20/08/2026): as skills `touring-generator` e `touring-evolve` foram
> removidas por decisão do Gabriel — ambas colidiam em ativação (a medição de
> propósito deu 5 usurpações entre elas) e ambas ensinavam comandos que não
> existem (`touring integration`, `touring evolve`, `touring autoevolve`).
>
> O que NÃO era defeito são os comandos abaixo: reais, verificados contra
> `touring --help`, e que **nenhuma outra fonte documentava**. Removê-los junto
> com as skills teria sido reduzir em vez de potencializar (REGRA #0), então
> ficam aqui. A skill que os hospedava sumiu; a informação não.


## Generation (`touring generate …`)

Pipeline typestate do `touring-generator` (o BINÁRIO, que continua existindo — é a skill homônima que foi removida, não a ferramenta).

| Comando | Contexto original |
|---|---|
| `touring decompose` | 1. The generator auto-creates subtasks in `touring decompose` for each pipeline stage |
| `touring generate plan-status` | touring generate plan-status |
| `touring generate plan-validate` | touring generate plan-validate --file plan.json |
| `touring generate schema-dump` | touring generate schema-dump -j |
| `touring generate template-list` | 1. Check `touring generate template-list` for available templates |
| `touring generate template-validate` | 3. Use `touring generate template-validate --kind <kind>` to check |
| `touring index` | 2. **Verified** — VGP checks all symbols_to_verify exist in the touring index |
| `touring memory` | "intent": "Pre-edit enrichment handler that injects context from touring memory", |

## Evolution & telemetria

Contadores e ciclo de evolução/flywheel. `touring evolve` e `touring autoevolve` NÃO existem — eram o defeito da skill removida.

| Comando | Contexto original |
|---|---|
| `touring cognitive engines` | touring cognitive engines -j    2>/dev/null > /tmp/te_engines.json    & |
| `touring evolution improvement` | touring memory recall "touring evolution improvement" -j 2>/dev/null > /tmp/te_memory.json & |
| `touring evolution tools` | touring evolution tools -j      2>/dev/null > /tmp/te_tools.json      & |
| `touring flywheel status` | touring flywheel status -j      2>/dev/null > /tmp/te_flywheel.json    & |
| `touring gotcha match` | 1. RECALL: touring memory recall + touring gotcha match |
| `touring incremental status` | touring incremental status -j   2>/dev/null > /tmp/te_incremental.json & |
| `touring memory stats` | touring memory stats -j         2>/dev/null > /tmp/te_memstats.json    & |
| `touring session checkpoint` | touring session checkpoint -j 2>/dev/null |
| `touring wiring modules` | touring wiring modules -j       2>/dev/null > /tmp/te_modules.json    & |
| `touring wiring status` | WIRING_BASELINE=$(touring wiring status -j 2>/dev/null \| jq '.orphan_count // 0') |

---

_Extraído por script no momento da remoção, não transcrito à mão._
