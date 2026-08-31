---
name: briah
description: "Briah — a descoberta da criação (Mundos da Criação). Ative quando Gabriel descreve algo NOVO a criar (\"quero criar\", \"nova criação\", \"vamos construir\", \"tenho uma ideia\", \"rito de criação\", \"/briah\") ou quando o Painel de Atziluth oferta o rito. Conduz a intenção crua até a descrição completa e perfeita — o prompt perfeito — através das 7 operações formais nomeadas, com intensidade calibrada (I0-I3), fading por medição de antecipação e modo profundo estritamente opt-in com oferta. Sai com criacao.md validado por gate executável (ratio 1.0). NÃO ativar para correções, perguntas, retomadas de trabalho existente ou tarefas triviais."
---

# Briah — a descoberta da criação

> O segundo Mundo (gate humano de Gabriel, 30/08/2026). Recebe a **emanação**
> (a intenção crua) e a conduz até a **concepção definida** — um `criacao.md`
> que é, simultaneamente, a descrição perfeita da criação e o prompt que
> Yetzirah (a formação — estratégia e plano) recebe. O rito é o aparelho de
> treino: cada passagem pratica as 7 operações formais até o Sistema 1 de
> Gabriel operá-las sozinho (aí o rito desaparece — fading medido).

## Lei do rito (as 3 leis do rumo aprovado)

1. **Camada, não sistema**: Briah alimenta o loop-engineering existente — o
   `criacao.md` é a entrada do OUTER (o `topic` do strategy-loop nasce dele).
2. **Executor + medidor desde o dia 1**: o gate de saída é um exit code
   (abaixo), e toda passagem grava no journal cognitivo.
3. **O alvo é a FORMA**: as perguntas treinam as operações invariantes de
   domínio, nunca o domínio. Pratique em superfícies variadas — código,
   texto, negócio, vida — e o esquema se descola do conteúdo.

## Passo 0 — medir ANTES de perguntar (o fading é por construção)

Ao receber a descrição crua da criação, PRIMEIRO meça o que Gabriel já
antecipou — nunca pergunte o que ele já respondeu:

```bash
python3 ~/.claude/skills/loop-engineering/scripts/cognicao_formal.py \
  medir --gravar --dominio <codigo|texto|negocio|vida|harness> \
  --texto "<a descrição crua, verbatim>"
# → {presentes, ausentes, ratio}
```

O rito cobre **apenas as operações ausentes**. Ratio 1.0 de entrada ⇒ I0:
declare o rito satisfeito de saída e siga direto para Yetzirah — forçar rito
no caso já-completo taxa o caso comum (a falha DeepSeek documentada).

## Roteador de intensidade I0-I3

| Nível | Quando (default por heurística; Gabriel SEMPRE sobrescreve) | Rito |
|---|---|---|
| **I0** | trivial/correção/pergunta, ou ratio de entrada = 1.0 | nenhum — vá direto |
| **I1** | criação pequena (S), poucas ausências | 1 rodada `AskUserQuestion` só com as 2-3 ausências mais graves |
| **I2** | criação M/L, ou 4+ ausências | espelho invertido + protocolo executivo completo sobre as ausentes |
| **I3** | criação com peso existencial/filosófico declarado | executivo + lente profunda — **somente com o sim explícito** |

**A oferta do profundo (decisão d do gate)**: quando a criação tem cara de I3
(temas de identidade, propósito de vida, obra autoral), o roteador **nota e
pergunta em uma linha** — *"esta criação tem cara de I3 — quer o modo
profundo?"* — e jamais entra sem o sim. Um não vale para a criação inteira.

**Espelho invertido (I2+)**: antes de perguntar, peça que Gabriel enuncie —
*"das 7 operações, quais você ainda não respondeu?"* — e complete só o que
faltar. É o scaffolding com fading: o objetivo é este passo bastar.

## As 7 operações — forma executiva e lente profunda

Mecânica: `AskUserQuestion`, 1-2 operações por rodada, na ordem abaixo,
construindo o artefato progressivamente. A forma executiva é o default; a
lente profunda só em I3 consentido. Perguntas SEMPRE devolvem para a criação
— exploração criativa, nunca análise de psique.

| Operação | Forma executiva (coaching de elite) | Lente profunda (I3, opt-in) |
|---|---|---|
| **A Emanação** — o que quer nascer? | "Descreva em uma frase o que você quer que exista ao final." | "O que está pedindo para nascer através de você aqui — e há quanto tempo isso insiste?" |
| **O Telos** — para quê, para quem? | "Quem usa isso, e o que muda na vida dessa pessoa?" (5 whys se raso) | "Se esta criação falasse, a quem ela diria que serve? O que ela quer que você se torne ao fazê-la?" |
| **A Imagem** — como é o pronto? | "Feche os olhos: a criação está pronta e funcionando. Descreva a cena — o que se vê, o que se usa, o que se lê." | "Qual é a imagem que volta quando você não está pensando nisso? Descreva-a sem editar." |
| **A Fronteira** — o que fica fora? | "Nomeie 3 coisas que esta criação NÃO é e não fará (anti-goals)." | "Que versão desta criação você descartou sem admitir? O que dela você está evitando dizer?" (a sombra da criação) |
| **A Cadeia** — os elos até o objetivo | "Do estado atual à Imagem: enuncie ≥5 elos 'se X → então Y'." | "Onde a cadeia depende de você mudar — e não de o mundo mudar?" |
| **O Custo Invisível** — pré-mortem | "É daqui a 6 meses e a criação fracassou. Escreva a manchete: o que a matou?" | "O que você teme encontrar no meio do caminho? O incômodo é informação." |
| **O Pronto** — como saberemos, medido | "Qual comando, número ou evento declara 'pronto'? (nunca sensação)" | "Como você saberá que pode soltá-la — e o que em você resiste a soltar?" |

## Passo final — o artefato e o gate executável

Consolide TUDO em `criacao.md` no bundle da criação
(`docs/plans/<data>-<slug>/criacao.md`), OKF:

```markdown
---
type: Criacao
title: "<nome da criação>"
plan_id: <data>-<slug>
tags: [briah, criacao]
timestamp: <ISO>
---
# <nome> — a concepção (Briah)
## A Emanação       ← o que quer nascer, em uma frase
## O Telos          ← para quê / para quem
## A Imagem         ← o estado final, descrito como cena
## A Fronteira      ← anti-goals: o que esta criação NÃO é
## A Cadeia         ← ≥5 elos se→então até a Imagem
## O Custo Invisível ← pré-mortem + a alternativa descartada
## O Pronto         ← critérios medidos (comandos, números, gates)
## O Prompt Perfeito ← o parágrafo executável que Yetzirah recebe:
   contexto + contrato de saída + exemplos + critérios de aceitação +
   anti-goals (engenharia de prompt 2026: denso, específico, verificável)
```

**Gate de saída (exit code, nunca sensação)** — o mesmo instrumento que mede
Gabriel valida o artefato:

```bash
python3 ~/.claude/skills/loop-engineering/scripts/cognicao_formal.py \
  medir --arquivo docs/plans/<slug>/criacao.md
# ratio == 1.0 exigido; ausência nomeada = a seção a completar
```

Feche registrando o rito (a série do espelho):

```bash
python3 ~/.claude/skills/loop-engineering/scripts/cognicao_formal.py \
  registrar --tipo destravamento --dominio <d> --criacao <slug> \
  --pergunta-destravou "<a pergunta que destravou, se houve>" \
  --texto "briah fechado: ratio entrada <r0> → saída 1.0"
```

Se o rito roda dentro de um loop com DAG, o fechamento da fase leva o mundo:
`loop_phase_close.py … --mundo briah`.

## Transição para Yetzirah

O `criacao.md` validado É a entrada: `touring adw run strategy-loop --var
topic="<O Prompt Perfeito, verbatim>" --var scope=… --var bundle=<o mesmo
bundle>`. Nunca redigite a intenção — a externalização já foi feita e paga.

## Anti-padrões do rito

- Perguntar o que o `medir` mostrou presente (taxa o caso comum e insulta a
  antecipação — o fading anda para trás).
- Entrar no profundo sem o sim explícito (viola a decisão d do gate humano).
- Fechar sem o gate executável ("ficou ótimo" não é ratio 1.0).
- Rodar o rito completo para I0/I1 (rito banalizado é rito morto).
- Interpretar a psique em vez de devolver à criação (não sou analista; o
  material profundo vira INSUMO da obra, nunca diagnóstico).

## Referências

- Régua e journal: `~/.claude/skills/loop-engineering/scripts/cognicao_formal.py`
- Estratégia-mãe + gate humano: `~/projects/touring/docs/plans/2026-08-30-mundos-da-criacao/strategy-2026-08-30-mundos-da-criacao.md`
- Painel que oferta este rito: `scripts/hooks/painel_emanacao.py` (Atziluth)
