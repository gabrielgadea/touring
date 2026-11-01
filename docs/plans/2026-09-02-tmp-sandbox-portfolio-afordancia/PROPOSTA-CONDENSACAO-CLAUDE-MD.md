---
type: Proposal
title: "Proposta de condensação do CLAUDE.md do projeto (02/09/2026)"
description: "Análise item a item dos quatro blocos de narrativa, com o texto condensado proposto e o destino de cada trecho removido."
tags: [claude-md, documentacao, higiene, proposta]
timestamp: 2026-09-02T15:30:00-03:00
plan_id: 2026-09-02-tmp-sandbox-portfolio-afordancia
---

# Proposta de condensação do `CLAUDE.md`

> **Não aplicada.** O conteúdo é de Gabriel; esta é a proposta para aprovação item a item.

## O critério

Três perguntas decidem cada parágrafo. Só as duas primeiras justificam custo em toda sessão.

1. **É invariante?** Muda o que alguém faz aqui hoje, sem depender de ter vivido a wave.
2. **É gotcha caro?** Ignorá-lo custa horas, e a informação não está no código.
3. **É narrativa?** Conta o que aconteceu, com datas, números de wave, nomes de fase e paths de bundle.

Narrativa não sai do projeto: vai para o arquivo canônico que já existe, e o invariante fica com um ponteiro.

## O quadro

| Item | Hoje | Proposto | Destino do texto removido |
|---|---|---|---|
| 8 Portfólio ADW | 53 | 12 | bundles `2026-08-18-graph-engineering-flow-portfolio`, `2026-08-28-adw-specs-expansao`, `2026-08-28-adw-potencializacao`, `2026-08-28-skills-adw-potencializacao` |
| 10 Code mode | 52 | 14 | `~/.claude/skills/Touring/references/code-mode-operational.md`, que o próprio item já declara como corpo canônico |
| 12 Code-mode-sinal | 26 | 4 | bundle `2026-08-31-code-mode-sinal` |
| 13 F0.3 post-bash | 66 | 11 | bundle `2026-08-31-complementacao-hooks`, cujo `log.md` já tem a narrativa |
| **Total** | **197** | **41** | |

Arquivo: **332 → cerca de 176 linhas**, abaixo das 200 recomendadas.

---

## Item 8 — Portfólio de fluxos ADW

**Fica** a lei de composição, o campo que permite descartar um fluxo, os três comandos de entrada, e os três gotchas que custaram estreias inteiras.

**Sai** a contagem de specs por data, a lista dos seis fluxos novos com descrição, o parágrafo inteiro de potencialização e o de skills como ADWs.

### Texto proposto

```markdown
8. **Portfólio de fluxos ADW**: fluxos são **compostos**, não copiados —
   `[[use]] module/as/with` faz inlining de um fragmento sob namespace. Todo fluxo
   publicado declara `[purpose]` com `when_not_to_use`: é esse campo que deixa o
   portfólio **descartar** um fluxo em vez de recomendar o mais próximo. Fan-out
   `parallel` tem forma estática e dinâmica; `max_branches` é verificado **antes**
   de rodar qualquer ramo. Criar: `touring adw new` (prior-art com veredito
   obrigatório) · inspecionar: `touring adw explain` (grafo plano) ·
   `touring adw fragments`.
   **Três gotchas pagos por estreias**: (a) agente headless herda os hooks da
   sessão — `_agent_claude` spawna com `TOURING_WORK_OUTER_DISABLED=1`; (b) gate
   mudo degrada o retry, então a REASON de um gate ENSINA a correção; (c) **texto
   de agente nunca entra no sandbox** — o X6 classifica o programa inteiro e prosa
   com cara de comando vira deny não-determinístico.
   Histórico das waves e a library completa: `docs/plans/2026-08-{18,28}-*/`.
```

12 linhas. Nada do que decide uma ação foi removido.

---

## Item 10 — Code mode ativo neste workspace

Este é o caso mais claro, porque o próprio item **já aponta** para o corpo canônico. A narrativa de calibração pertence a ele, não à constituição.

**Fica** a declaração de escopo, o efeito atual em uma tabela de limiares, a precedência de resolução e o kill switch, mais a linha do handshake MCP, que muda o que se vê numa sessão.

**Sai** todo o racional da calibração, o histórico do gate de turno, a evolução dos limiares e a repetição da regra de prova comportamental, que já vive na regra 2.

### Texto proposto

```markdown
10. **Code mode ATIVO neste workspace**: `.touring/touring.toml` declara
    `[code_mode] mode = "code"`. Inspeção **isolada passa**; o que colapsa é a
    **rajada**, com a chamada negada trazendo o programa fundido pronto.

    | Gate | Nega em | Janela |
    |---|---|---|
    | rajada de inspeção (`grep`/`cat`/`find`/`ls`/`wc`/`sed-n`/python-inline read-only) | 2ª | 300s |
    | G1 rajada · G7 re-inspeção | 3ª | 300s |
    | G10 exec-burst | 5ª | 600s |
    | par write→run · G3 edit-sem-read | 2º | 600s |

    Um `touring run` zera a janela. Precedência: prefixo `TOURING_CODE_MODE=<v>`
    no PRÓPRIO comando (exportar no shell não chega ao hook) → `touring.toml` →
    default `both`. Kill switch humano: `TOURING_CODE_GATES_DISABLED=1`. Um escopo
    que declara `code` também estreita o **anúncio** MCP para a fachada
    search+execute; todo tool escondido segue invocável por nome.
    Estratégia por contexto, calibração e histórico:
    `~/.claude/skills/Touring/references/code-mode-operational.md`.
```

14 linhas, e a tabela responde mais rápido que os cinco parágrafos atuais.

---

## Item 12 — Code-mode-sinal F1-F6

O item inteiro é changelog de entrega, com uma exceção: a lição do rebuild parcial, que custa uma sessão inteira quando ignorada.

**Fica** só o gotcha. **Sai** F2, F3, F4, F5, F6, o DAG, os paths e a nota de próxima wave.

### Texto proposto

```markdown
12. **Rebuild parcial não atualiza o binário**: `cargo build -p touring-cli` NÃO
    regenera `touring`/`touring-daemon`, que vêm de `touring-server`, e o daemon
    embute o CLI por linkagem estática carregando handlers uma vez no boot. Depois
    de editar um handler RPC: `cargo build -p touring-server --release` +
    `update-touring`. A superfície SDK do `--orchestrate` e sua entrega estão em
    `docs/plans/2026-08-31-code-mode-sinal/`.
```

4 linhas de conteúdo mais o ponteiro.

---

## Item 13 — F0.3, entrega viva do `post-bash`

O maior dos quatro, e o de maior densidade de narrativa. Contém quatro gotchas reais enterrados em três waves de história.

**Fica** o gotcha do `if` de hook, o das features do crate de handlers, o do reindex por script, o instrumento que prova que um hook roda, e o estado atual do `pre-bash`.

**Sai** a sonda, a fachada `all-hooks`, os itens de TDD, e as três waves de layers com seus paths.

### Texto proposto

```markdown
13. **Hooks: quatro coisas que custam horas**. (a) O campo `if` de um hook aceita
    **uma** regra de permissão, sem operadores lógicos, e falha ABERTO em comando
    não parseável: um valor com `|` nunca casa comando simples e dispara por acaso
    em heredoc. Um `if` por regra, e filtro por conteúdo dentro do handler.
    (b) `cargo test -p touring-hook-handlers` exige `--features pre-hooks,post-hooks`;
    sem elas os handlers nem compilam, e `clippy` com features parciais inventa
    dead code que `--all-features` não vê. (c) Edits aplicados por script, fora da
    ferramenta de edição, não passam pelo hook de reindex, e o juiz de convergência
    acusa órfãos falsos até `touring index rebuild`. (d) A prova de que um hook
    roda é o trace: `TOURING_HOOK_TRACE_FILE` grava uma linha JSON por invocação.
    Estado atual: `pre-bash` e `post-bash` sem `if`, logo o gate do `pre-bash`
    alcança todo Bash. Narrativa das waves: `docs/plans/2026-08-31-complementacao-hooks/`.
```

11 linhas para quatro gotchas que hoje ocupam 66.

---

## Como eu aplicaria

Item por item, na ordem 13, 8, 10, 12, do maior ganho para o menor. Para cada um:

1. Gravar o texto removido no destino da tabela, com data e a origem sendo o `CLAUDE.md`.
2. Substituir o item pelo texto proposto.
3. Medir o arquivo e mostrar o antes e depois.
4. Parar e esperar sua palavra antes do próximo.

Nada é apagado, tudo é movido. É reversível por git a qualquer momento.

## O que eu não proporia

Mover para `.claude/rules/`. A própria REGRA #16 registra que rules são auto-load, então o custo por sessão continua idêntico. Seria organização sem economia.
