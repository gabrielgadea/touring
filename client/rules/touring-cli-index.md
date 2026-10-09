# Touring CLI — Index & CLI Command Ranks (slim)

> **Auto-load** | **Version**: v5.3 (slim) | **Touring**: `touring --version` (o número muda a cada release; per-project: `.touring/bin/touring --version`, que sai em stderr)
> **TIER 4-9 (Session / Generate / Learning / Hooks / Search / Utility)**: `~/.claude/skills/Touring/references/touring-cli-tiers-4-9.md` (load on demand)
> **Per-module deep reference** (7 módulos): `~/.claude/skills/Touring/references/touring-cli-{overview,hooks,intelligence,tasks,rl-quality,generate,meta}.md`
> **Mirror full** (skill master): `~/.claude/skills/Touring/SKILL.md`

This rule keeps **TIER 1-3** (consulted every session) + Quick Cheatsheet (most-used commands) + Golden Rules. TIER 4-9 loaded on demand via the reference above.

---

## TIER 1 — CRÍTICOS (sempre usar, impactam diretamente qualidade)

| ★★★★★ | Comando | Porque | Como Usar |
|--------|---------|--------|-----------|
| `touring ast meta <file> --depth summary -j` | File metadata first | blast_radius/quality_score/cognitive_score/fan_in/fan_out **para arquivos indexados**; em cache-miss cai para `on_disk_fallback` (só metadata estrutural, esses campos vêm `null`). Para valores garantidos use `ast tdg` (quality), `ast blast` (blast radius), `file-knowledge extended` (cognitive) | Obrigatório antes de qualquer Edit |
| `touring pre-edit` | Pre-edit hook | Score composto 0-1 com CILA budget + rayon parallel signals | Corre antes de cada Edit, score deve ser >= 0.8 |
| `touring ast blast <file>` | Blast radius | Árvore completa de dependências antes de mudanças | Sempre antes de refactors L3+ |
| `touring index find <symbol>` | Symbol lookup (VGP) | Verifica se símbolo existe antes de criar/usar | Sempre antes de gerar código |
| `touring wiring orphans -j` | Orphan detection | Identifica pub symbols sem consumidores (potencialização) | After creating new pub fn/mod |
| `touring e2e -j` | E2E health | Score composto 0-1 de todo o sistema | Antes de mudanças arriscadas |
| `touring explore <topic>` | Loop-until-dry exploration (F1/CCE) | Ledger multi-lente persistente + contrato de convergência — "uma rodada nunca basta" institucionalizado | Antes de planejar/afirmar cobertura de um tema |
| `touring adw run <name> --var k=v` | ADW runner durável (F0) | Workflows declarativos (.touring/adw/*.toml) com journal fsync'd, `--resume-run`, Class-D, ZTE, racing; `lint`/`test`/`from-template`/`race` | Trabalho multi-nó com agentes headless gatados |
| `touring factory route\|start "<ticket>"` | Factory router (F4) | Ticket → ADW da library, determinístico-primeiro + RL feedback; KPIs `touring.adw.*` em `touring kpi` | Intake de tarefas para a esteira ADW |

## QUICK CHEATSHEET — Comandos mais usados

```bash
# PRE-EDIT (OBRIGATÓRIO)
touring ast meta <file> --depth summary -j   # file metadata first
touring ast blast <file>                     # blast radius
touring pre-edit                             # score >= 0.8
touring index find <symbol>                  # symbol exists? (VGP)

# DIAGNOSTICS
touring doctor -j                            # health check
touring status -j | jq '{index, orphans, rl, composite_health_score}'
touring index why <file|@companion/key>      # why a file is/isn't indexed: skipped_dir, oversized, unsupported_extension, companion_not_configured… + residue:true se o store ainda guarda linhas (I13)
# Memórias, rules, skills, agents, commands (~/.claude) e a auto-memory do projeto são INDEXADAS como raízes-companheiras
# (`@companion/<nome>/<rel>`; config `[index]` do `.touring/touring.toml`) — `search unified`/`tantivy search`/`find` as devolvem por essa chave.
touring tantivy search "<pergunta ou símbolo>" 10  # ranking medido (nomes+texto+caminho+proximidade): acha código E o que memórias/regras/skills DIZEM
touring search text "<frase>"                # só o texto (memórias, regras, docs de código); `search unified` = ranking do tantivy + BM25 de desempate
touring index status                         # index_generation.state: complete | building | partial (rebuild interrompido → `touring index rebuild`) (I16)
touring synergy -j                           # wired_pairs + opportunities
touring gate-metrics -j                     # L7-B metrics

# MASTERS (Layer-3)
touring portfolio "<intento>"                # prior-art por PROPÓSITO + lacunas + lente externa (veredito obrigatório)
touring explore <topic>                      # loop-until-dry exploration (CCE)
touring adw run <name>                       # durable agent workflow (spec+journal)

# WIRING
touring wiring audit -j                      # full audit
touring wiring orphans -j                    # orphans
touring wiring suggest --top 20 -j           # suggestions
touring wiring impact <symbol> --depth 2     # blast of symbol
touring wiring cycles --min-depth 2          # Tarjan SCC

# AST DEEP
touring ast rust-semantic <file.rs>          # syn semantics
touring ast workspace-info                   # cargo_metadata
touring ast grep <file> <pattern> --rewrite <r>  # polyglot rewrite

# MEMORY HASHTAGS (biblioteca facetada v30.4 — guide: docs/memory-hashtag-library.md)
touring memory store <k> <v> --tag "#kind:lesson" --tag "#domain:wiring"   # store c/ facetas (auto-derive sempre roda)
touring memory query "#kind:snippet #lang:python"      # conjuntivo exato por faceta
touring memory recall "diorama #artifact:map"          # recall c/ filtro de tag pré-RRF
touring memory sync-tags --file|--dir <path>           # codetags `#tags:` -> snippet memories (post_write/edit já sincroniza)
touring memory link <src> <dst> --rel extends          # aresta tipada (id determinístico) · links <key> · unlink <id>
touring memory moc <tópico> [--out path]               # Mapa de Conteúdo emergente (comunidades)
touring memory communities [--domain d]                # visão global das comunidades
touring memory backfill-tags [--dry-run]               # retroativo conservador
touring portfolio "<intento> #kind:script #lang:py"    # portfólio facetado (filtra corpus antes do BM25)
```

**TIER 4-9 commands** (session/decompose/memory/generate/tantivy/evolution/inferlets/jobs): see `~/.claude/skills/Touring/references/touring-cli-tiers-4-9.md`.

---

## REGRAS DE OURO

1. **FILE METADATA FIRST** — `touring ast meta <file> --depth summary` antes de qualquer Edit
2. **SEMPRE** executar `touring doctor -j` antes de atividades críticas
3. **SEMPRE** usar `touring index find` antes de criar novos símbolos
4. **SEMPRE** usar `touring shadow validate` (TIER 5) antes de Edit/Write
5. **SEMPRE** usar `touring wiring audit` após criar novos módulos pub
6. **NUNCA** ignorar orphan symbols — indicam código não utilizado
7. **SEMPRE** persistir lessons aprendidas via `touring memory store` (TIER 4)
8. **USAR** `touring evolution` (TIER 6) para atualizar padrões após erros
9. **VERIFICAR** sempre se o comando existe na tabela acima ou no reference TIER 4-9 antes de usar
10. **VGP** — verificar símbolos via `touring generate verify --symbol <name>` (TIER 5) antes de gerar código
11. **AGENTS** — usar touring-scouter/architect/engineer/auditor/scriber para tarefas especializadas

---

> **TIER 2, TIER 3, Context Windows e o catálogo por módulo:** `~/.claude/skills/Touring/references/touring-cli-index-tiers-2-3.md`
