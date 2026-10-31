# Portfólio de scripts — a rota "consultar antes de criar"

Origem: plano `docs/plans/2026-10-27-portfolio-scripts-harness/` (repo `analise`). Medido em 27/10/2026: 81,2% dos
scripts criados eram efêmeros/inline e a recriação semântica era massiva (`probe.py` em 14 sessões).

## O hook

`~/.claude/skills/Touring/scripts/hooks/script_portfolio_gate.py` (PreToolUse, `Write` e `Bash` com heredoc `cat`).
Dispara quando o destino é efêmero (`/tmp`, `.cache`, scratchpad, `tool-results`) ou o basename é genérico. Consulta
`portfolio/recuperar.py` (SimHash do corpo, τ=8, mais propósito) e emite `additionalContext` com os candidatos.
**Sombra**: nunca nega e nunca reescreve a entrada. Log (sem conteúdo): `~/.claude/touring/portfolio_gate/events.jsonl`.
Kill switch humano: `TOURING_PORTFOLIO_GATE_DISABLED=1`.

## Os 4 estados do veredito (`veredito_gate/1`)

| Estado | Exit | Significa |
|---|---|---|
| `reprovado` | 1 | já existe equivalente de CORPO (cópia editada): leia e estenda |
| `aprovado` | 0 | nada equivalente; candidatos só por propósito/nome |
| `nao_aplicavel` | 0 | a consulta não se aplica ao alvo |
| `inaferivel` | 2 | a consulta ficou INCOMPLETA (fonte fora do ar) — ausência não é "limpo" |

Consulta manual: `python3 ~/.claude/skills/Touring/scripts/portfolio/recuperar.py --arquivo <script>`.

## Índice do projeto (federação)

Cada projeto exporta o próprio índice em `<projeto>/.claude/portfolio/indice.json` (procurado subindo a partir do
`cwd`). No `analise`, reexportar (incremental, a partir do catálogo FACT gravado; sem catálogo = exit 2):
`.venv/bin/python3 scripts/lexhub/portfolio_export.py`.

## Medir: `--stats`

`python3 ~/.claude/skills/Touring/scripts/hooks/script_portfolio_gate.py --stats` resume o log: `eventos`,
`com_candidato`, `would_avoid`, `pct_evitaveis`, `por_estado` e `sinteticos`. Eventos de instrumento de medição
(`/tmp/medicao-latencia-hook/`, `/tmp/auditoria-portfolio/`) ficam FORA dos totais e só aparecem em `sinteticos`;
eventos anteriores à P1 entram como `legado`.

## Plano ativo: `.claude/plano_ativo.json`

Decisão de Gabriel (31/10/2026). Arquivo explícito no projeto, procurado subindo a partir do `cwd` do evento:
`{"versao": 1, "bundle": "docs/plans/<dir>"}`. Com ele, um script criado em destino **efêmero** recebe no contexto a
sugestão `<raiz>/docs/plans/<bundle>/diagnostics/<nome>` (campo opcional `destino_sugerido` no evento). Sem o arquivo
o hook só avisa. Arquivo ilegível, `versao` ≠ 1 ou `bundle` absoluto/com `..` → só avisa e registra
`plano_ativo_erro` (fail-open). Nunca se adivinha "o plano mais recente". Desligar a sugestão:
`TOURING_PORTFOLIO_REDIRECT_DISABLED=1`.

## Regra

**Script nasce em `docs/plans/<bundle>/diagnostics/<nome>`** — nunca em `/tmp`, scratchpad ou `.scratch/`.
Reutilizável → portfólio (`~/.claude/scripts-portfolio/`, contrato no MANIFEST) ou `touring run --harvest <slug>`.
