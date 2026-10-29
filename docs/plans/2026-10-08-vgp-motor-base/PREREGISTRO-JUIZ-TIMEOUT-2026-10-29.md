---
type: PreRegistration
title: "JUIZ-TIMEOUT — o juiz declara INCONCLUSIVO quando a camada 2 estourou o prazo"
description: "Pré-registro da regra do timeout da camada 2 no juiz do motor VGP: o que se conta, de onde vem o patamar, quando o veredito é INCONCLUSIVO e quais exit codes. Escrito ANTES de tocar no juiz."
plan_id: 2026-10-08-vgp-motor-base
subtask: task_1791581548206685387::JUIZ-TIMEOUT
okf_version: 1
timestamp: 2026-10-29
---

# PRÉ-REGISTRO — JUIZ-TIMEOUT (29/10/2026)

> Instrumento congelado: o juiz (`scripts/juiz_motor_vgp.py`) só pode mudar depois deste
> pré-registro, e o que for implementado tem de ser **exatamente** o que está escrito aqui.
> Escrito antes da primeira linha de código.

## 1. O defeito

`RELATORIO-INT97I-FASE-D.md:49-59`: na rodada V3 do pipeline, a camada 2 (pyright) estourou o prazo
em **521/738** ids (ev) e **278/762** (bak), contra 46 e 50 do INT97. O `timeouts_v3_abba.sh`
(mesmo corpus, uma reserva só) deu ref 50, novo 34, novo 38, ref 37 — **os timeouts seguem a
máquina, não o binário**.

**Mecanismo**: quando a camada 2 estoura o prazo, o motor devolve `verdict: allow` com
`findings: []` e o estado `camada2: "unknown: prazo de 1000 ms estourado …"`. O juiz, que só lia
`verdict`/`findings`, contou essa chamada como **negativo sem deny**. ⇒ **denies perdidos** ⇒
**falso deny subestimado** ⇒ o critério de falso deny (o que o VGP quer proteger) é medido num
número que a máquina carregada **favorece**. Um run assim pode reprovar por p95 e parecer
"conservador" quando, na verdade, **não mediu o que diz medir**.

Evidência do estado (contagem pelo mesmo predicado dos scripts de run):

| run | timeouts/ids | taxa | regime |
|---|---|---|---|
| int97-integrado 18/10 ev | 521/738 | **70,60 %** | máquina carregada |
| int97-integrado 18/10 bak | 278/762 | **36,48 %** | máquina carregada |
| int97-integrado 17/10b ev | 206/738 | 27,91 % | máquina carregada |
| frag 17/10 base_vivo | 115/738 | 15,58 % | maior taxa observada em máquina quieta |
| p0-atributo 09/10 f55 bak | 96/762 | 12,60 % | quieta |
| **ABBA 18/10 1_A (ref×ref)** | **50/738** | **6,78 %** | **A/A** |
| ABBA 18/10 2_B (novo×novo) | 34/738 | 4,61 % | B/B |
| ABBA 18/10 3_B (novo×novo) | 38/738 | 5,15 % | B/B |
| **ABBA 18/10 4_A (ref×ref)** | **37/738** | **5,01 %** | **A/A** |
| J13 18/10 5c7b3d80 V3 | 86/1217 | 7,07 % | quieta (mesma reserva rust) |
| J13 18/10 8fd0efb0 V3 | 76/1217 | 6,24 % | quieta (mesma reserva rust) |
| INT97 17/10 ev / bak | 46/738 · 50/762 | 6,23 % · 6,56 % | quieta |
| todo run V2 (camada 1) | 0/N | 0 % | sem camada 2 |

## 2. O que se conta

**Conta-se, por chamada do motor, o estado `camada2` que o motor devolveu**, com o **predicado
exatamente igual** ao dos scripts de run (`rodar_j13.sh`, `timeouts_v3_abba.sh`):

```python
timeout  = isinstance(camada2, str) and camada2.startswith("unknown: prazo de")
```

- `timeouts_camada2` — número de chamadas com timeout da camada 2.
- `camada2_sem_veredito` — número de chamadas com `camada2` string `unknown: …` de **qualquer**
  causa (timeout **ou** `unknown: no pyright-langserver in the venv of …`). A segunda causa
  (ferramenta ausente) derruba denies do mesmo jeito; entra no **relatório** (e na conta de
  sem-veredito), mas **não** no gatilho do patamar — ver §5.
- **Denominador**: `chamadas` = número de chamadas do motor que o juiz fez (o mesmo denominador
  que o histórico usa: `len(por_id)` = `chamadas` do JSON do juiz — 738/762/1217).
- **Taxa** = `timeouts_camada2 / chamadas`.

`timeouts_camada2`, `camada2_sem_veredito`, `chamadas`, `taxa` e `patamar` vão para **toda**
execução: no relatório (stdout) e no JSON do juiz.

## 3. O sinal ausente (fail-closed)

**Sinal presente** = o juiz observou a camada 2 em pelo menos uma chamada: o motor reportou
`layer == 2` (o motor diz **qual camada respondeu**) **ou** reportou algum estado `camada2`.

**Sinal ausente** = a camada 2 foi pedida (`--camadas …,2`) **e** nenhuma chamada reportou `layer`
nem `camada2` — ou o juiz fez **zero** chamadas (sem denominador). ⇒ **INCONCLUSIVO**.
*Sinal ausente ≠ zero.*

**Por que não é "o campo `camada2` apareceu em alguma chamada"**: o motor **omite** o campo
`camada2` quando a camada 2 respondeu normalmente — no run J13 V3, **978 de 1217** registros não
têm o campo, e só os 176 restantes o têm (86 timeouts + 90 `no pyright-langserver`). Se a
presença do campo fosse o sinal, **todo run limpo seria INCONCLUSIVO** e o instrumento nunca
aceitaria nada. O que o motor **sempre** reporta no release corrente é `layer` (`layer == 2` em
1217/1217 do J13 V3).

## 4. O patamar

**`PATAMAR_TIMEOUTS_CAMADA2 = 0,20` (20 % da taxa).**

Como é calculado: é o **teto do cluster quieto**, com folga, e **abaixo** do regime de falha.

- Cluster quieto observado (runs dentro da reserva rust do `hw-slot`): **2,98 % – 15,58 %**
  (mínimo: p0-atributo 09/10 det_bak/base 22/738; máximo: frag 17/10 base_vivo 115/738).
- **A/A** (mesmo binário, mesmo corpus, mesma reserva rust): **50/738 (6,78 %)** e **37/738
  (5,01 %)** — o par A/A do `timeouts_v3_abba.sh` de 18/10. B/B do mesmo ABBA: 4,61 % e 5,15 %.
- J13 18/10, mesma reserva rust: 7,07 % e 6,24 %.
- Regime de falha: **27,91 %** (17/10b ev), **36,48 %** (18/10 bak), **70,60 %** (18/10 ev).

⇒ 20 % fica **4,4 p.p. acima** do maior caso quieto e **7,9 p.p. abaixo** do menor caso de falha.

**PROVISÓRIO.** O A/A da **fase D / P95C** (`fase-d-2026-10-18/v3_{ev,bak}/`) **não guardou
registro por-id** (`veredito_aa.json`/`pareado_aa.json` são o pareamento de p95, sem contagem de
timeout) ⇒ **não há A/A de timeout da fase D**; o patamar vem do ABBA de 18/10 + J13 + do cluster
quieto do bundle. Deverá ser **re-derivado** quando a nova classe de medição do `hw-slot`
produzir um A/A com por-id. O número 20 % é **conservador** (não reprova nenhum run quieto
conhecido) e fica bem abaixo do regime de falha.

## 5. A regra

Para um run que **pediu a camada 2** (`--camadas …,2`):

| condição | veredito |
|---|---|
| sinal **ausente** (§3) | **INCONCLUSIVO** |
| `taxa = timeouts_camada2 / chamadas` **> 0,20** | **INCONCLUSIVO** |
| sinal presente e taxa ≤ 0,20 | veredito normal (PRONTO / NÃO PRONTO), pelas pendências de sempre |

Para um run que **não pediu** a camada 2 (V2, `--camadas` ausente): **a regra não se aplica** — o
veredito é o de sempre, byte a byte. O relatório mostra mesmo assim
`timeouts/denominador/patamar` (sinal "não aplicável").

`camada2_sem_veredito` (timeout **ou** ferramenta ausente) é **reportado**, não gatilhado: a causa
"pyright ausente no venv" é **do ambiente/projeto**, não da máquina (o corpus `ev`/`bak` mistura
projetos, e `touring` não tem pyright no venv), então incluí-la no gatilho tornaria a taxa
incomparável entre runs de corpora diferentes. Fica registrada para o próximo pré-registro.

## 6. Exit codes

| code | veredito |
|---|---|
| **0** | PRONTO |
| **1** | NÃO PRONTO |
| **2** | MOTOR AUSENTE (pré-existente) |
| **3** | **INCONCLUSIVO** (novo, distinto dos outros) |

**INCONCLUSIVO nunca vira PRONTO**: `pronto` é forçado a `False` e o motivo entra em
`pendencias`. O relatório imprime `veredito: INCONCLUSIVO — <motivo>` e o processo sai **3**.

## 7. O que NÃO muda

- O veredito de quem **não** pede a camada 2 (V2): byte a byte igual.
- O registro por-id (`diagnostics/instrumentos-2026-10-09/rodar_juiz_instrumentado.py`): o juiz
  **não** adiciona nenhuma chave ao dicionário que o motor devolve (o wrapper grava tudo que o
  `rodar_motor` devolve, menos `elapsed_ms`) — a contagem é feita no **acumulador**, não na saída.
- O `--exigir-limite-superior` e o `--base` continuam como estão.
- O `--selftest` (que não pede camada 2) continua igual.

## 8. Mutantes previstos (o teste tem de MATAR estes)

1. **Inverter a comparação do patamar** (`>` por `<`) — um run acima do patamar passaria como
   normal.
2. **Tratar ausência como zero** (sinal ausente ⇒ taxa 0 ⇒ não INCONCLUSIVO) — o fail-closed
   morre.

## 9. Onde a regra é implementada e provada

- Juiz: `scripts/juiz_motor_vgp.py` (contagem no acumulador, `julgar` aplica a regra, `main`
  imprime e sai 3).
- Teste: `scripts/test_juiz_timeout.py` (ao lado do juiz; padrão do bundle —
  `test_cdn_pin.py`, `test_chama_oraculo.py`, `test_corpus_rust_contrato_juiz.py`).
- `hw-slot`: classe de medição exclusiva que toma `rust` + `heavy` juntos
  (`scripts/hw-slot`, `scripts/test_hw_slot.py`) — a máquina sem vizinho é o que o juiz de latência
  precisa; as classes existentes **não mudam de comportamento**.

## 10. Esclarecimento da revisão (touring-cb, 29/10/2026 ~15:20, antes de qualquer julgamento)

Os dois retornos antecipados que o juiz já tinha (corpus degenerado; corpus sem positivo de
símbolo de arquivo) saíam **sem** os campos do §2, e o ramo "zero chamadas" do §3 era inalcançável
pela CLI (um mutante que o trocava por "presente" sobrevivia aos cinco cenários). Agora os dois
retornos levam os mesmos campos, calculados pela mesma função (`campos_camada2`), e o motivo de
sinal ausente entra nas pendências. O veredito deles **continua NÃO PRONTO (exit 1)**, não
INCONCLUSIVO: o defeito é do corpus, e "corpus degenerado" é mais específico do que "não deu para
medir". O §3 vale para um corpus que deixa medir. Cenário 6 do teste: `cenario_zero_chamadas`.
