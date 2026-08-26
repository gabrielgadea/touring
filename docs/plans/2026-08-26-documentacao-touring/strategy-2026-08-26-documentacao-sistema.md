---
type: Strategy
title: Documentação do sistema, repositório e infraestrutura do Touring — reconciliar com o código
description: Estratégia medida — 93 arquivos no escopo (não 1.112), 63 deles descrevem crates que não existem mais; correção guiada por mapa de fusão verificado e protegida por guard permanente.
plan_id: 2026-08-26-documentacao-touring
tags: [documentacao, drift, arquitetura, crates, diataxis]
timestamp: 2026-08-26T08:15:00-03:00
okf_version: "0.1"
---

# Documentação do sistema — reconciliar com o código

> **Escopo (Gabriel, 26/08)**: documentação do **repositório, infraestrutura e sistema**.
> **NÃO** documentação de planos nem de trabalho operacional.

## 1. O que o pedido significa, medido

"Toda a documentação" são **1.112 arquivos `.md` / 9,8 MB**. O seu recorte derruba isso
para **93 arquivos** — e a diferença não é detalhe, é a maior parte do trabalho:

| bloco | arqs | dentro do escopo? |
|---|---|---|
| `docs/plans/` (bundles) | 445 | ❌ trabalho operacional |
| `docs/` relatórios datados `YYYY-MM-DD-*` | 104 | ❌ trabalho operacional |
| `docs/` relatos + planos | 23 | ❌ trabalho operacional |
| `docs/audits/` | 15 | ❌ trabalho operacional |
| `client/` (espelho) | 157 | ❌ **gerado** — editar não muda nada que executa (regra 6) |
| **raiz** README/CLAUDE/ARCHITECTURE | **3** | ✅ (feito em 20/08, 4d) |
| **`crates/*/ARCHITECTURE*.md`** | **18** | ✅ **105d de idade média, 17 acima de 90d** |
| **`crates/*/README.md`** | **17** | ✅ 64d |
| **`crates/*/CLAUDE.md`** | **3** | ✅ 105d |
| **`docs/` sistema** (RFC-001..005, CONSTITUTION-v8) | **11** | ✅ 118d |
| **`docs/` a triar** (touring-pro 120KB, touring-system 52KB…) | **17** | ⚠️ triar: sistema ou operacional |
| `docs/reference/` (gerado) | 5 | ✅ validar, não editar (21d) |
| `docs/adr/` | 6 | ✅ 17d |

## 2. O achado que justifica a wave

Não é estética nem idade — é **afirmação falsa**. Ground truth executado hoje:
**44 crates**, versão **30.4.14**.

**63 dos 93 arquivos descrevem crates que não existem mais.** As menções:

| crate citado | menções | existe? |
|---|---|---|
| `touring-ast` | **113** | ❌ |
| `touring-learning` | **111** | ❌ |
| `touring-core` | 64 | ❌ |
| `touring-cognitive` | 59 | ❌ |
| `touring-index` | 50 | ❌ |
| `touring-wasm` | 26 | ❌ |
| + 10 outros (`activity`, `memory`, `search-fusion`, `graph-viz`, `telemetry`, `definitions`, `graph-core`, `rules`, `vfs`, `vector-store`) | ~120 | ❌ |

**Mais de 500 menções** a uma arquitetura que foi fundida e não existe. Quem lê
`touring-server-ARCHITECTURE.md` procura `touring-ast` e não encontra.

**E a correção NÃO é mecânica** — testei meus próprios palpites e errei:

| fantasma | meu palpite | destino REAL (verificado) |
|---|---|---|
| `touring-ast` | touring-analysis | **`touring-code/src/ast/`** |
| `touring-learning` | touring-intelligence | **`touring-analysis/src/learning/`** |
| `touring-index` | touring-analysis | **`touring-intelligence/src/index/`** |
| `touring-core` | touring-foundation | **`touring-generator/src/core/`** (a confirmar) |
| `touring-telemetry` | — | **dois destinos**: `foundation` E `server` |
| `touring-cognitive`, `touring-memory` | — | **não achados** como submódulo |

Três dos cinco palpites estavam errados. Um "find/replace" produziria documentação
*confiantemente errada* — pior que a desatualizada, porque parece atual.

## 3. Prior art — isto é continuação, não começo

`strategy:doc-rewriting:2026-08-20` (`outcome_reward 1.0`) já mediu e decidiu:

- Inventário de então: **954 `.md`** (hoje 1.112 — +158 em 6 dias).
- Tensão declarada: **"toda a documentação" = 79 h contínuas**.
- Solução: **10 ondas W1–W10**, MkDocs Material + mkdocstrings, preservação em `archive/`.
- **W1 executada e provada** (task_1787234572792401084, 8/8 done; preservação por SHA256).

Do plano original, o seu recorte de hoje mantém **W4** (crates), **W5** (taxonomia
Divio) e **W9** (MkDocs); descarta **W7/W8/W10** (auditorias, plans-archive, memória).

**Uma contradição do plano de 20/08 precisa ser corrigida**: W2 e W3 apontam para
`client/skills/` e `client/rules/`, mas a política do próprio documento diz "não editar
`client/`" — e a regra 6 do CLAUDE.md é explícita: `client/` é **gerado**, editá-lo é
desfeito pelo próximo sync. Se essas ondas voltarem, o alvo é o **live** (`~/.claude/`)
com sync depois.

## 4. Lente externa (Context7 — obrigatória)

**Diátaxis** (`/evildmp/diataxis-documentation-framework`) contradiz a leitura literal
do pedido: *"Work one step at a time — avoid completing large amounts of work before
publishing; small incremental changes over the big picture"* e *"Just do something —
pick a small existing piece, assess it against user need"*. Também prescreve
`MANIFEST.md` como inventário e define manutenção como **atualizar quando o código
muda** — o que aponta para um guard automático, não para uma varredura periódica.

## 5. Fases propostas

| # | fase | entregável | prova |
|---|---|---|---|
| **F1** | **Mapa de fusão verificado** | tabela `crate fantasma → destino real`, derivada por execução (não por palpite) + o script que a gera | cada linha citando o diretório/símbolo que a sustenta |
| **F2** | **Reconciliação guiada pelo mapa** | as ~500 menções corrigidas nos 63 arquivos | guard: 0 crates fantasma no escopo |
| **F3** | **Guard permanente** | teste que reprova se a doc citar crate inexistente | provado por mutação (introduzir `touring-ast` → exit 1) |
| **F4** | **`crates/*/ARCHITECTURE*.md`** (18, 105d) | decidir por crate: atualizar, ou consolidar em doc-comments e arquivar | `cargo doc` + gate de link |
| **F5** | **`docs/` sistema** (RFC-001..005, CONSTITUTION-v8, 11 arqs) | verificar se ainda descrevem o sistema; corrigir ou marcar como histórico | evidência por símbolo |
| **F6** | **Triagem dos 17 indefinidos** | classificar sistema vs operacional; arquivar o operacional | inventário com veredito |
| **F7** | **Infraestrutura (MkDocs)** — *opcional, depende de você* | `mkdocs.yml` + CI + deploy | `mkdocs build --strict` |

**F1→F2→F3 é o núcleo** e entrega o maior valor: a documentação deixa de mentir sobre a
arquitetura, e o guard impede que volte a mentir. F4–F7 são incrementais e podem ser
feitos em ondas separadas, como Diátaxis recomenda.

## 6. Decisões que preciso de você

1. **Escopo desta wave**: só o núcleo **F1–F3** (reconciliar + proteger), ou seguir até
   **F6** (todo o inventário do sistema)? — *recomendo F1–F3 agora*, pelo Diátaxis e
   porque é o que tem prova objetiva.
2. **`crates/*/ARCHITECTURE*.md` (F4)**: o plano de 20/08 previa **substituí-los por
   doc-comments Rust** (fonte de verdade = código). Confirma essa direção, ou prefere
   mantê-los como arquivos e só atualizar?
3. **MkDocs (F7)**: `mkdocs.yml` não existe. Entra nesta wave ou fica para depois?
4. **Os 17 indefinidos** — `touring-pro.md` (120 KB), `touring-system.md` (52 KB),
   `Manual-Avancado-de-Analise-de-Codigo.md` (115 KB), `01-suggestions.md` (146 KB):
   arquivar como histórico ou tratar como documentação de sistema a atualizar?

## 7. Política (herdada de 20/08, mantida)

Não apagar — o que sai vai para `archive/` com SHA256 de proveniência · não editar
`client/` (gerado) · OKF obrigatório em todo `.md` novo · linkagem relativa · validado
por `loop_doc_link_gate.py`.

---

_Estratégia medida em 26/08 — escopo por execução, não por estimativa. Aguardando
aprovação (HUMAN GATE)._
