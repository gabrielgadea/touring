# C1 por família — onde está a queda de 898 → 537 chars/chamada

**Janelas** (UTC): BASE = 12 transcripts reconstruídos por atividade até `2026-10-08T17:00:00`
(≈17h de 08/10, quando a linha de base foi medida) · POST = `2026-10-09T01:11:50Z` (deploy_utc) →
`2026-10-09T13:53:47Z` (o instante em que esta sessão COMEÇOU a medir).

**Métrica B** (a do juiz C1, `verificar_compactacao._advice_since`): só anexos
`hook_additional_context`, chars = `len(json.dumps(content, ensure_ascii=False))`.

| | chamadas | chars | blocos | chars/chamada |
|---|---|---|---|---|
| BASE | 10.249 | 9.319.352 | 15.131 | **909,3** |
| POST | 549 | 294.718 | 432 | **536,8** |

Queda **40,96%**. Limiar C1 = 898 × 0,6 = **538,8** → **PASSA por 2,0 chars/chamada**.

⚠ **O veredito depende de quando se olha.** Mesma janela, sem limite superior (o que o juiz roda hoje):
**549,3/chamada (n=565) → REPROVA**. Os 592/n=323 relatados são a MESMA janela medida mais cedo: é uma
média sobre um denominador que cresce, e ela **dilui o próprio sinal** enquanto mais chamadas de baixo
custo se acumulam. Uma reprovação colhida às 16h pode virar aprovação às 18h sem que nada mude no código.

## O conjunto da linha de base é irrecuperável

Varri N=8..25 por mtime: **nenhum N reproduz as 8.441 chamadas** (máximo 8.251). Motivo: os 12
transcripts mais recentes em 08/10 **continuam sendo escritos** — o mtime deles já avançou além do
cutoff, e filtrar por `mtime ≤ cutoff` seleciona transcripts **mortos antes** da medição (conjunto
enviesado por omissão, não por sorte). Reconstruí por **atividade** (última mensagem até o cutoff mais
próxima dele), que devolve 909,3 — e **o 898 declarado não foi reproduzido por nenhum critério que eu
consiga nomear**. Trato 898 como constante herdada, não como número reproduzido.

⚠ **As duas métricas nunca foram a mesma.** 898 veio de `medir_conselho.py` (soma os **três** tipos de
advice, chars = `len(texto)`); 592/549 veio de `_advice_since` (só `hook_additional_context`, chars =
`len(json.dumps(...))`). A razão 898 → 549 **não é uma razão homogênea**. Na amostra de 12
transcripts: 11.773 `hook_additional_context` contra 36.114 `hook_success` (conteúdo vazio) e 6.909
`async_hook_response` (`'null'`). Medir antes e depois pela mesma definição daria outro número.

## Corte 1 — hook emissor (conservação EXATA: 9.319.352 e 294.718)

| hook | base/ch | pós/ch | Δ/ch | % do Δ |
|---|---|---|---|---|
| PreToolUse:Edit | 339,2 | 135,8 | −203,4 | **37,7%** |
| PreToolUse:Write | 147,7 | 51,9 | −95,8 | 16,4% |
| PreToolUse:Bash | 109,8 | 50,6 | −59,2 | 12,2% |
| PreToolUse:Read | 91,8 | 83,8 | −8,0 | 9,9% |
| PostToolBatch | 55,0 | 5,6 | −49,4 | 6,2% |
| PostToolUse:Edit | 56,1 | 25,3 | −30,8 | 6,2% |
| PostToolUse:Bash | 50,4 | 80,7 | **+30,2** | **−5,2%** |
| SessionStart | 25,2 | 63,0 | **+37,8** | **−2,5%** |
| UserPromptSubmit | 16,0 | 25,4 | **+9,4** | **−1,7%** |
| PostToolUse:Write | 13,8 | 8,4 | −5,5 | 1,5% |
| PostToolUseFailure:Bash | 4,1 | 6,5 | **+2,5** | **−0,4%** |
| PreToolUse:SendMessage | 0,3 | 0,0 | −0,3 | 0,0% |

**Um hook responde por 37,7% do Δ** (`PreToolUse:Edit`), e os quatro PreToolUse somam 76,2%. Nenhum
outro emissor chega perto.

## Corte 2 — prefixo do texto injetado (conservação EXATA: 9.319.352 e 294.718)

Cada família é um **predicado positivo** sobre o início literal do texto. A lista não veio de suposição:
abri o balde `não classificado` medindo (`abrir_nao_classificado.py`) e ele devolveu os prefixos reais.

| família | base/ch | pós/ch | Δ/ch | % do Δ |
|---|---|---|---|---|
| não classificado (resíduo: `cargo check`, falhas de teste) | 201,0 | 49,3 | −151,7 | 22,5% |
| MUST/SHOULD/MAY (diretivas) | 110,3 | 60,1 | −50,1 | 12,2% |
| Pre-edit prevention | 103,7 | 12,3 | −91,4 | 11,7% |
| HOTSPOT (wiring: quem chama) | 115,9 | 84,8 | −31,1 | 12,7% |
| code-workflow (complexidade/símbolos) | 73,6 | 25,5 | −48,1 | 8,2% |
| post-tool-batch (sugestão de consumidor) | 55,0 | 5,6 | −49,4 | 6,2% |
| TOURING SUGGEST (enriquecimento do índice) | 54,4 | 32,4 | −22,0 | 6,0% |
| post-edit verification | 47,8 | 10,7 | −37,1 | 5,4% |
| defs[] (mapa de definições) | 36,7 | 7,1 | −29,6 | 4,1% |
| medição sem relação verificada | 31,7 | 61,0 | **+29,3** | **−3,2%** |
| SCOUT PERPÉTUO (gerador de demanda) | 22,2 | 57,2 | **+35,0** | **−2,2%** |
| PROMPT ENHANCEMENT (general mode) | 13,6 | 23,6 | **+10,0** | **−1,4%** |
| EVIDENCE GUARD | 10,1 | 16,5 | **+6,3** | **−1,1%** |
| ast overview / leitura virtualizada | 5,0 | 34,7 | **+29,7** | **−0,3%** |
| env-context/navegador | 5,7 | 35,5 | **+29,8** | **−0,4%** |
| hook modify notice | 8,3 | 1,0 | −7,3 | 0,9% |
| post-write (health-delta) | 9,1 | 3,4 | −5,7 | 1,0% |
| CODE MODE (contrato da sessão) | 2,7 | 0,0 | −2,7 | 0,3% |
| CODE-FIRST / hook gate genérico `[generic]` | 2,2 | 0,2 | −2,1 | 0,2% |
| ATZILUTH (painel de emanação) | 0,0 | 5,8 | **+5,8** | −0,0% |
| SHADOW (scope shadowing) | 0,3 | 6,3 | **+5,9** | −0,0% |
| segurança de plugin | 0,0 | 4,0 | **+4,0** | −0,0% |

% negativo = a família **cresceu** e por isso devolve parte da queda. As sete famílias que cresceram
somam +120,7 chars/chamada, que é precisely o que as que caíram têm de sobra para entregar.

## Três achados

**1 · A queda é concentrada e não é difusa.** `PreToolUse:Edit` sozinho carrega 37,7% do Δ (corte 1),
e no corte 2 as três maiores quedas — `não classificado` / `Pre-edit prevention` / `MUST·SHOULD·MAY` —
são todas **o conselho de bloqueio de edição** repetido em 4 lugares (diretiva, prevention, verification,
code-workflow). O que caiu não foi "conselho", foi **o mesmo conselho emitido 4× por chamada de
edição**.

**2 · Sete famílias cresceram, e o saldo delas é material.** `SCOUT PERPÉTUO` (22,2 → 57,2, **+157%**),
`ast overview` (5,0 → 34,7, **+594%**), `env-context` (5,7 → 35,5), `medição sem relação` (31,7 → 61,0)
são conselhos **novos ou mais verbosos** que o deploy não encolheu. Se o próximo patch continuar
adicionando painéis, essas famílias **consomem por soma o ganho** das que caíram: quatro delas já
passam de 10 chars/chamada, e juntas são 7,1% dos chars do pós. Crescer não é detalhe — é o outro
lado da mesma métrica.

**3 · O C1 é um juiz instável por construção.** Mesma janela, dois vereditos (536,8 PASSA / 549,3
REPROVA), a 40 minutos de distância, sem uma linha de código alterada. E a linha de base (898) e o
pós (549) foram medidos por **definições diferentes**. Antes de tratar "C1 reprova" como informação,
o juiz precisa: (a) fixar o instante de corte da janela pós — ou congelar o conjunto de transcripts;
(b) medir antes e depois pela mesma função. Sem (b), a razão não tem denominador comum.

## Advertencia sobre o resíduo

`não classificado` (22,5% do Δ) **não é** uma classe homogênea nem pode ser somada a nenhuma das
famílias acima. São mensagens singletão de **`cargo check`, falhas de teste e avisos pontuais** — 27.046
chars no pós (9,2% dos chars do pós), as 16 maiores listadas em `nao_classificado.json`. Ele aparece
na tabela por honestidade do balde, não como resultado.

## Arquivos (rodáveis de novo)

| arquivo | o que faz |
|---|---|
| `c1_por_familia.py` | **o entregável**: reconstrói a base, varre as duas janelas, emite os 2 cortes + conservação + o JSON |
| `abrir_nao_classificado.py` | abre o balde por predicado positivo; **importa** a taxonomia do principal (divergiriam, senão) |
| `reconciliar.py` | busca o conjunto da base por N=8..25 e mostra as duas métricas lado a lado |
| `c1_por_familia.json` · `nao_classificado.json` · `reconciliacao.json` | os números |

```bash
cd ~/projects/touring/docs/plans/2026-10-08-compactacao-65/diagnostics/c1-por-familia-2026-10-09/
TOURING_CODE_MODE=native python3 c1_por_familia.py     # ~4 min
```

Nada fora deste diretório foi escrito. `medir_conselho.py`, `verificar_compactacao.py`, hooks,
`settings*.json` e `crates/` intocados; nenhum commit, nenhum restart de daemon, nenhum update-touring.