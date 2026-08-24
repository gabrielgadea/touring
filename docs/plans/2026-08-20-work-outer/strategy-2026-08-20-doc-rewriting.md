---
okf_version: "1.0"
type: Strategy
title: "Estratégia para a reescrita da documentação do Touring"
description: "Plano para profissionalizar toda a documentação do Touring segundo práticas de sistemas premium — escopo, ferramentas, fases, decisões pendentes"
plan_id: 2026-08-20-work-outer
tags: [documentation, strategy, work-outer, premium]
timestamp: 2026-08-20T02:55:00-03:00
---

# Estratégia — Reescrita profissional da documentação do Touring

Ligado a [index](./index.md) · diagnostic em [diagnostics/touring-20260820T105429.md](./diagnostics/touring-20260820T105429.md) · ledger CCE em `.touring-explore/auditoria-documental-touring--estado-e-plano-de-.ledger.json`

## Estado medido

| medida | valor | origem |
|---|---:|---|
| Total de `.md` no workspace | **954** | `find crates docs client assets root -name '*.md'` (excluindo `.serena`, `.remember`, `.full-review`, `adw-runs`) |
| Com frontmatter OKF | **344 (36%)** | parser `---` na primeira linha |
| `ARCHITECTURE*.md` | **25 arquivos** (255 KB total) | grep `'ARCHITECTURE' in name` |
| `ARCHITECTURE.v29.5.0.md` | **138 KB**, sozinho | `stat` |
| `docs/internal/sessions/` | duplica ~30 sessões inteiras | `walk` em `docs/` |
| Duplicação `docs/` ↔ `docs/internal/sessions/` | **~1100 KB** | soma dos tamanhos dos pares |
| `client/` | **306 arquivos** (regra #8: **gerado**, não-editar-aqui) | filesystem |
| `docs/plans/` | **306 arquivos** | `walk` em `docs/plans/` |
| Crates com `README.md` próprio | **18** de 42 | `find crates/*/README.md` |
| Planos com fases fechadas (`P1.md`, `F1.md`, `W*.md`) | **~90** arquivos | `walk` em `docs/plans/*/phases/` |

## Tensão entre os pedidos

| pedido | consequência prática |
|---|---|
| **"TODA a documentação"** | 954 arquivos × ~5min = 79h de trabalho contínuo, com fadiga esperada a partir da 8ª |
| **"Profissional, padrão premium"** | exige gerador (MkDocs/Docusaurus/mdBook) + taxonomia coerente + DAG de navegação |
| **"Não destruir histórico"** | 306 arquivos em `docs/plans/` são trilhas de decisão; comprimir pode apagar contexto |

A **única solução que respeita os três** é: **reescrita em ondas priorizadas**, com **ferramenta única**, **preservando trilha** em arquivo morto.

## Ferramenta escolhida: MkDocs Material + mkdocstrings

**Por que MkDocs**:
1. É o padrão Rust (usado por tokio, hyper, diesel).
2. **mkdocstrings** lê doc-comments direto do código — os 660k LOC do Touring são a maior fonte de verdade, não os `.md`.
3. Sidebar dinâmica + nav-tabs + paleta native = "premium" sem trabalho manual.
4. Admonitions (note, warning, tip, example) cobrem o que OKF faz em 5 seções por manual.
5. Componentes nativos para diagrama, abas, código, copy.

**Alternativas rejeitadas**:
- Docusaurus: stack React pesado, build lento, sem geração a partir de código Rust.
- mdBook: livro puro, não serve para documentação técnica de sistema.
- GitHub Pages + Jekyll: default, mas Jekyll não lê doc-comments Rust.
- Apenas Markdown puro: não é "premium" — é o que já existe.

## Escopo em ondas

| onda | escopo | tam | por que |
|---|---|---|---|
| **W1** | `README.md`, `CLAUDE.md`, `ARCHITECTURE.md` (consolidação dos 25 em 1) | M | entrada de qualquer pessoa no repo |
| **W2** | `client/skills/*/SKILL.md` (12 arquivos) — `description`, exemplos, linkagem cruzada | M | superfície de uso mais tocada |
| **W3** | `client/rules/*.md` (15 arquivos) — auto-load, hierarquia clara | M | regras auto-carregadas determinam comportamento |
| **W4** | `crates/*/README.md` (18) + `crates/*/ARCHITECTURE.md` (25) — substituir pelos doc-comments | L | migração para fonte de verdade = Rust |
| **W5** | `docs/{reference,tutorial,guide,explanation,how-to,cookbook}/` — 26 arquivos, taxonomia Divio | M | separar "learning" de "reference" |
| **W6** | `docs/adr/` (6 arquivos) — ADR template + retroativo dos major decisions | S | rastreabilidade de decisão |
| **W7** | `docs/audits/` (12 arquivos) — formato elite-audit-report padrão | M | output cross-audit comparável |
| **W8** | `docs/plans-archive/` (3 + 100s sub) — comprimir trilha para índice navegável | M | histórico acessível, não dominante |
| **W9** | MkDocs config (`mkdocs.yml`), CI, deploy (GitHub Pages) | M | o que torna o conjunto "premium" |
| **W10** | `.full-review/` (30) + `.serena/` (21) + `.remember/` (15) — comprimir para 1 página de histórico | S | estado da memória legível |

## Política de modificação

1. **Não apagar**: todo arquivo que sai vai para `archive/2026-08-20-doc-rewriting/` com nome datado.
2. **Não editar `client/`**: a regra #8 da skill-loop-engineering o diz; é gerado por `sync-client-skills.py`.
3. **OKF obrigatório** em todo `.md` mantido: `type`, `title`, `description`, `plan_id`, `tags`, `timestamp`.
4. **Linkagem por `[link](path.md)`**: relativo, nunca absoluto.
5. **Reescritas validadas por `loop_doc_link_gate.py`** — exit 0 obrigatório, sem contradições, sem links quebrados.

## Decisões (Gabriel, 20/08/2026)

1. **Ferramenta de deploy**: **GitHub Pages** (aprovado).
2. **Taxonomia de ADRs**: decisão minha — **MADR 4.0** (lightweight, suporta status/supersedes/links, padrão do community). Nygard é datado; custom é trabalho extra sem ganho.
3. **`docs/internal/sessions/`**: **manter como está**. Trilhas de auditoria são insubstituíveis; compressão custa mais do que economiza.
4. **`ARCHITECTURE.v29.5.0.md`**: **consolidar em `ARCHITECTURE.md` único**. As ondas W1 fundem v29.5.0 + o arquivo atual em uma única fonte canônica, com a versão v29.5.0 mantida como apêndice histórico (`ARCHITECTURE-historical-v29.5.0.md`) para referência, não para leitura.

## Sequenciamento

```
W1 → W2 → W3 → W4 → W5 → W6 → W7 → W8
                                       ↓
                                       W9 (MkDocs + CI)
                                       ↓
                                       W10 (limpeza)
```

Acíclico, mas W9 e W10 dependem do conteúdo das ondas anteriores. **W1** é a mais barata e mais visível — sugiro começar por ela **antes** de gastar tempo em W4 (a maior).

## Riscos

| risco | prob | impacto | mitigação |
|---|---|---|---|
| MkDocs não gerar bem (tema, sintaxe) | BAIXA | ALTO | rodar `mkdocs serve` localmente em cada onda; revisar em browser antes de fazer merge |
| Links internos quebrados | MÉDIA | MÉDIO | gate `loop_doc_link_gate.py` no CI |
| Reescrita perder contexto histórico | MÉDIA | ALTO | arquivar antes de reescrever |
| `client/` drift após sync | MÉDIA | BAIXO | guarda `test_sync_client_skills.py` no CI |
| Fadiga a partir da onda 5 | ALTA | MÉDIO | pausar para consolidamento a cada 3 ondas; não ultrapassar 4 ondas por sessão |

## O que esta estratégia NÃO cobre

- Reescrita dos doc-comments Rust em si (W4 cobre metadata dos crates; os próprios `///` são outra frente).
- Migração para plataforma de blog/changelog externa — `CHANGELOG.md` permanece no repo.
- Internacionalização — atual é só PT-BR parcial em comentários; manter assim.

## Métricas de sucesso

- `loop_doc_link_gate.py` exit 0 em todos os bundles
- Frontmatter OKF em 100% dos `.md` mantidos
- MkDocs serve sem erros, com sidebar completa, em menos de 30s de build
- Documentação `crates/*/ARCHITECTURE.md` removida quando equivalente doc-comment estiver no código
- Zero regressões: cada doc reescrito passa pelo mesmo `touring check` (lint OKF) que já roda
