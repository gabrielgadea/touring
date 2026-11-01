---
type: PreRegistration
title: Experimento de causa dos caracteres de alfabeto estranho na prosa — modelo × contexto
description: Escrito e commitado antes de qualquer chamada que produza dado. Fixa hipóteses, condições, N, semente, desfecho, regra de rótulo cego, testes, correção múltipla, teto de gasto e o que conta como inconclusivo.
plan_id: 2026-10-06-retomada
timestamp: 2026-10-06T09:55:00-03:00
tags: ["#kind:decision", "#domain:hooks", "#process:pre-registration"]
---

# Pré-registro — de onde vem o CJK no meio do português

**Autorização:** Gabriel, 06/10/2026: "Faça o experimento, gasto da API autorizada". Origem da pergunta:
canvas da sessão coordenadora em 05/10 (§9: "o experimento que separa causa de modelo e causa de contexto —
mesmo prompt, N repetições, 2 checkpoints"), sobre o defeito medido pela `analise-ef` em 04/10: caracteres
CJK/cirílicos dentro de prosa em português em 1,22% dos blocos de texto do `claude-opus-5-5` e 4,20% dos do
`MiniMax-M3.1`, contra ~0% nos demais modelos (pré-registro anterior:
`docs/plans/2026-10-04-integridade-prosa/pre-registro.md`).

Nada abaixo muda depois de ver dado. Mudança vira pré-registro novo, com data própria.

## 1. Pergunta

O defeito é propriedade do **checkpoint** (aparece mesmo com contexto curto e limpo), do **comprimento do
contexto** (aparece quando o contexto é longo), ou da **presença de CJK no contexto** (o modelo copia o alfabeto
que leu)?

## 2. Condições (2 × 3 = 6 células)

| Fator | Níveis |
|---|---|
| Checkpoint | `opus` = `claude-opus-5-5` (conta do Claude Code, OAuth) · `minimax` = `MiniMax-M3.1-Flash-Preview` (endpoint `https://api.minimax.io/anthropic`, como a função `minimax()` do `~/.bashrc`) |
| Contexto | `curto`: só a tarefa · `longo`: a tarefa depois de ~400 mil caracteres de arquivos do repositório **sem nenhum caractere do sinal** · `longo_cjk`: o mesmo `longo` com até 24 trechos curtos de arquivos de código e teste do repositório que contêm CJK legítimo, intercalados |

Os arquivos vêm do commit `e0b7bc1c988530ef8893e27422fa0c06235abf75` (`git show <commit>:<caminho>`), escolhidos
por regra determinística escrita no script (`cjk_experiment.py`, `build_prefix`): `docs/audits/*.md`,
`crates/touring-cli/src/**/*.rs` e `crates/touring-server/src/cli/*.rs`, em ordem de caminho, pulando arquivo
com sinal e arquivo acima de 60 mil caracteres, até o alvo. Os trechos do `longo_cjk` vêm só de caminhos fora de
`docs/` (testes, corpus, detectores), para que o contexto carregue CJK **legítimo**, não exemplos do defeito. O
sha256 de cada prefixo vai para o registro de cada chamada.

## 3. Constantes

- **Mesmo prompt** em todas as chamadas da mesma célula; mesmo `--system-prompt` e mesma tarefa nas seis: um
  relatório técnico em português do Brasil, de 700 a 900 palavras, em prosa contínua, sobre o trecho Rust fixo
  `parse_tolerating_json` + `rejects_global_json` (`crates/touring-server/src/cli/common.rs` no commit acima).
- **Harness idêntico** para os dois checkpoints: `claude -p --model <m> --system-prompt <S> --tools ""
  --setting-sources "" --strict-mcp-config --no-session-persistence --output-format json --effort medium`, num
  diretório temporário vazio, prompt pela entrada padrão. Só o modelo e o endpoint mudam.
- **N = 30** chamadas válidas por célula (180 no total). **Semente 20261006** para a ordem intercalada das 180
  chamadas (as células se misturam no tempo; nenhuma roda em bloco).
- Concorrência: 2 chamadas simultâneas no `opus`, 4 no `minimax`. Timeout de 600 s por chamada; até 2 novas
  tentativas em erro de transporte. Chamada que falha 3 vezes sai da amostra e é reportada por célula.

## 4. Piloto (fora da análise)

Antes da coleta, até 4 chamadas validam o harness: modelo efetivamente usado, `is_error`, e tokens de entrada
compatíveis com o tamanho do prompt (prova de que hooks, CLAUDE.md e memória do usuário **não** entraram). O
script grava do piloto **só metadados**: o texto da resposta é descartado sem ser lido, para que o piloto não
espie o desfecho. Se o piloto obrigar a trocar um flag do harness, a troca é registrada no relatório, e nada do
desenho científico (§1-§3, §5-§8) muda.

## 5. Desfecho

- **Ocorrência:** sequência contígua de caracteres do sinal (as faixas do `prose_integrity_gate.py`, o mesmo
  detector do hook, importado, não copiado) no texto final da resposta, com ±40 caracteres de contexto.
- **Rótulo** de cada ocorrência, pela regra do pré-registro de 04/10 (§4): CORRUPT (embutido em prosa ou
  identificador latino, sem intenção visível) · LEGIT (proposital: citação de dado do contexto, frase de exemplo)
  · UNCLEAR.
- **Dois avaliadores cegos**: a `touring-f9` (Opus 5.5) e a `touring-b4` (MiniMax-M3.1). Cada um recebe só
  `id` aleatório, caractere e contexto, nunca célula, modelo ou repetição. A chave fica num arquivo separado.
- **Desfecho primário por resposta:** ≥ 1 ocorrência rotulada CORRUPT pelos **dois** avaliadores. Discordância
  e UNCLEAR contam como não-CORRUPT, o lado conservador.
- **Secundários:** (a) CORRUPT por pelo menos um avaliador; (b) detector cru (≥ 1 caractere do sinal, sem
  rótulo); (c) ocorrências por 10 mil caracteres de resposta. Concordância: kappa de Cohen.

## 6. Testes (planejados, unilaterais)

Fisher exato unilateral sobre a proporção de respostas positivas no desfecho primário:

| # | Comparação | Pergunta |
|---|---|---|
| T1 | `minimax/curto` > `opus/curto` | checkpoint, contexto curto |
| T2 | `minimax/longo` > `opus/longo` | checkpoint, contexto longo |
| T3 | `opus/longo` > `opus/curto` | comprimento, Opus |
| T4 | `minimax/longo` > `minimax/curto` | comprimento, MiniMax |
| T5 | `opus/longo_cjk` > `opus/longo` | CJK no contexto, Opus |
| T6 | `minimax/longo_cjk` > `minimax/longo` | CJK no contexto, MiniMax |

Holm-Bonferroni sobre os seis, α = 0,05. Cada célula sai com a taxa e o intervalo de Wilson a 95%.

## 7. Veredito

- **Causa de checkpoint** sustentada se T1 ou T2 rejeitar. **Comprimento** sustentado para um modelo se T3/T4
  rejeitar. **CJK no contexto** sustentado se T5/T6 rejeitar.
- Comparação que não rejeita é **inconclusiva**, nunca "sem efeito": com N = 30 por célula, só diferenças
  grandes são detectáveis (exemplo: 30% contra 5% dá p ≈ 0,01; 5% contra 1% não se detecta).
- Se o total de respostas positivas no desfecho primário for **menor que 10** nas 180, o relatório declara
  **taxa-base insuficiente** para todas as comparações não rejeitadas, e a extensão do N exige pré-registro novo.
- Célula com menos de 25 chamadas válidas é reportada como subamostrada.

## 8. Gasto

Teto de **US$ 120** somando o `total_cost_usd` que o Claude Code reporta em cada chamada (no Opus é o valor
nocional da conta). Ao passar do teto, nenhuma chamada nova começa, e o relatório diz quantas faltaram. Os tokens
de entrada, de cache e de saída vão para o registro de cada chamada.
