# Análise — Aproveitamento do CLI Touring pelos scripts e hooks

> **Data**: 2026-10-27 | **Pedido**: Gabriel — "verifique todo o CLI do Touring e tudo que compõe os master CLIs e analise o que pode ser aproveitado por tudo de `~/.claude/scripts-portfolio/` e de `~/.claude/skills/Touring/scripts/hooks`"
> **Método**: inventário estático dos 2 diretórios + superfície viva do CLI (`touring --help`) + composição dos masters na fonte canônica (`references/touring-cli-e-contexto.md`). Scripts: `scripts/cross_touring_cli.py`, saída em `scripts/cross_analysis.json`.
> Tags de confiança: FACT [1.0] (verificado por execução) · INFERENCE [0.7-0.9] · SPECULATION [<0.7].

## 1. A superfície (FACT [1.0])

- **144 subcomandos** no binário instalado (`touring --help`, contado).
- **13 masters**: `scout · read · health · guard · map · blast · investigate · explore · adw · factory · audit · portfolio · run/exec`.
- **Composição dos masters** (inverte a direção esperada): `touring scout/read/map/blast/investigate/...` são **wrappers Rust** (`touring-server cli/master.rs`) que **despacham para os scripts Python da própria skill** (`~/.claude/skills/Touring/scripts/{analyze_blast,diagnose_health,investigate,...}.py`). Ou seja: os scripts da skill não são consumidores do CLI — **são o backend dos masters**. O ciclo virtuoso correto é: script bom vira master (já aconteceu), e o que o master faz nenhum script reimplementa.
- **ADW/factory**: fluxos compostos por `[[use]]` de fragmentos (`recall-pack`, `prior-art`, `diagnose-pack`, `fanout-lenses`, `gate-rust`…), journal fsync'd, `adw explain` para o grafo plano. `[purpose].when_not_to_use` é o campo que permite DESCARTAR um fluxo — o mesmo princípio que o MANIFEST do portfólio adota.

## 2. Análise por arquivo (6 scripts produtivos nos 2 diretórios)

| Arquivo | Usa Touring hoje | Veredito |
|---|---|---|
| `scripts-portfolio/sonda_evidencia.py` | ✅ `tantivy search` como 1º estágio (prova ao vivo: `via: tantivy`) | Correto. rg é **fallback** deliberado para corpus não indexado, não lacuna |
| `scripts-portfolio/recomputa_verifica.py` | — | Correto NÃO chamar: não existe master para recomputo de claims (E2: forçar rota no caso simples taxa o caso comum). Ganho ecossistêmico: roda via `touring run --file` (sandbox+journal) |
| `scripts-portfolio/fetch_proveniencia.py` | — | Idem. Nenhum master faz fetch com proveniência |
| `hooks/script_portfolio_gate.py` | ✅ MANIFEST + `tantivy search` | Oportunidade pequena abaixo (§3.a) |
| `hooks/prose_integrity_gate.py` | — | **Correto ser autônomo**: hook PreToolUse em Write/Edit não pode pagar RPC de daemon por evento; análise é texto puro. Não aproveitar nada [0.9] |
| `hooks/scout_perpetuo_reminder.py` | lê estado local do scout | Correto: o estado é do `scout_perpetuo.py` (que o master `explore` complementa, não substitui) |

## 3. Oportunidades concretas (ordenadas por valor/esforço)

**a. Gate: o 2º estágio pode ser o master `portfolio` em vez de tantivy cru** [INFERENCE 0.8]
`touring portfolio "<intento> #kind:script"` já faz BM25 com filtro de faceta + lacunas + veredito — um comando substitui a composição manual. **Ressalva medida**: a saída do master é prosa formatada ("── prior art ──"), não JSON — parsear prosa é frágil. Decisão: **manter tantivy** (JSON) no hook e registrar `portfolio` como o comando NIVEL-HUMANO da consulta-antes-de-criar (já no README). Se o master ganhar `--json`, o hook troca.

**b. `~/.claude/scripts-portfolio/` como raiz-companion do índice** [INFERENCE 0.85]
Hoje o tantivy global não indexa o diretório (companions são skills/rules/agents/commands/memory). Declarar no `[index]` do touring.toml torna os scripts acháveis por `tantivy search`/`portfolio` de qualquer projeto — o hook já cobre via MANIFEST, então é redundância barata, não correção.

**c. Scripts do portfólio rodam via `touring run --file`** [FACT 1.0 que é viável, adoção a medir]
Ganha sandbox (SEG-3 agora lê ~/.claude), journal com `code_hash` (P2 — recorrência), escada de trust e harvest. O P2 fecha o loop: o 3º run idêntico sugere `--harvest`.

**d. O KPI já mede a adoção** [FACT 1.0]
`touring kpi -j` → `code_mode_reuse` (piso 0,20) e `code_mode_adherence` leem o journal. O gate P0 gera os eventos; o KPI é onde a melhoria aparece. Nada a construir — a régua existe.

**e. Dois canais de snippet, um só índice de consulta** [FACT 1.0]
Codetag (`sync-tags`) cria snippet biblioteca-estática; `--harvest` enrola na escada medida. Ambos respondem a `memory query "#kind:snippet"`. O portfólio usa os dois: codetag no arquivo (estático) + harvest quando o script prova recorrência (dinâmico).

**f. ADW `portfolio-tidy` (semana)** [SPECULATION 0.5]
Um fluxo ADW com `adw from-template` que roda o `--stats` do gate + promove scripts L1→L3. Vale quando houver ≥30 eventos no events.jsonl (critério à la prose gate).

## 4. Anti-padrões a NÃO introduzir (FACT, de memórias medidas)

- Abrir `memory.db`/`symbols.db` via sqlite3 num script → usar `memory query`/`index find` (o schema é interno e já mordeu: "Loop de learning no banco errado").
- Reimplementar ranking BM25 → `tantivy search`/`portfolio`.
- Hook de PreToolUse chamando daemon por evento trivial → latência no caminho quente (a regra do prose gate).
- `touring run` dentro de hook que o próprio `run` dispara → recursão (o gate P0 só consulta leitura, nunca executa).

## 5. Resposta curta ao pedido

Os masters JÁ são os scripts da skill (wrapper Rust → Python); o que os 6 scripts dos diretórios-alvo podem aproveitar **já está aproveitado onde faz sentido** (sonda→tantivy, gate→MANIFEST+tantivy), e os demais não têm master correspondente — a fronteira está certa. As alavancas reais são as §3.a-c: companion root, `run --file` como executor padrão dos scripts do portfólio, e `--json` no master `portfolio` como evolução que destrava a troca no gate.
