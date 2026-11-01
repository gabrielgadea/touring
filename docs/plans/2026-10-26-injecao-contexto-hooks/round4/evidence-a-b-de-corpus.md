---
type: Evidence
title: "O A/B de corpus: o que ele mede, e o que falta"
description: "A rodada 4 mediu o que o A/B de corpus mede — e o que falta para medir desfecho."
plan_id: 2026-10-26-injecao-contexto-hooks
okf_version: 1
timestamp: 2026-10-26
---

# O A/B de corpus: o que ele mede, e o que falta

> **O A/B de corpus existe** — `crates/touring-hooks-shared/tests/s8_corpus_ab.rs`
> (16.477 B, 09/10/2026). Ele mede a **deduplicação** de injeção sobre corpus real.
> **Não mede desfecho.**

## O que o A/B de corpus mede

| modo | o que é | o que mede |
|---|---|---|
| **A** | tudo pela dedupe | o quanto o dedup corta |
| **B** | deploy-fiel (só o que o daemon processa) | o quanto o deploy real corta |
| **C** | combinado (B + session-start:compact + turn_budget 2000) | o que o juiz mede ao vivo |

**O aceite A2** (o que o juiz mede):
- `N >= 200 payloads`;
- **queda >= 40%** (o dedup corta);
- **controle positivo**: nenhuma diretiva MUST perdida na 1ª emissão; o primeiro
  payload de cada sessão passa intacto.

## O que o A/B de corpus **não** mede

- **não invoca modelo nenhum** — sem `Command::new`, sem cliente de API;
- **mede payload, não desfecho** — compara bytes antes/depois;
- **não mede o efeito da injeção no comportamento** — só o efeito do dedup no
  volume.

## O que falta para medir desfecho

O architect já provou: **o harness tem de invocar um modelo**. O que falta:

1. **um harness que rode a mesma tarefa com e sem injeção** (o A/B de
   desfecho) — o `s8_corpus_ab.rs` mede o dedup, não o desfecho;
2. **medir a variância de reexecução** como covariante do controle (o Figure 11
   do harness2 mostra que remover uma entrada da biblioteca **subiu** o score
   (+1,9/+1,7) — o ganho online não é isolável do ruído de reexecução);
3. **medir o que o modelo faz com o contexto** — não o que o contexto pesa.

## O que a rodada 4 mediu em vez

| o que | o número | onde |
|---|---|---|
| a **conformidade** por família | FATO 52% · SERMÃO 0,3% · ALERTA 0% | `IMPACTO-POR-HOOK.md` |
| a **ordem de leitura** | 100% dos `PreToolUse` chegam depois da decisão | `evidence-v1-v2.md` |
| as **ordens inválidas** | 58,0% das ordens MUST chegam quebradas | `evidence-ordens-inexecutaveis.md` |
| a **redundância** | 69,8% dos bytes são repetição | `evidence-janela-e-redundancia.md` |

## O que a rodada 4 recomenda

1. **O A/B de desfecho exige harness novo** — com modelo no circuito, não com
   bytes.
2. **O dedup já existe; o que falta é alcance** (hooks Python, caminho
   standalone, campos da raiz).
3. **A regra operacional** (o "So what?" executável): injete só quando o bloco
   trouxer, em uma frase, (1) o estado medido, (2) a ação com verbo e alvo, (3) o
   porquê. **E cale** quando o fato já está na tela, é o conteúdo do arquivo, não
   tem consequência, ou não cabe numa frase.

---

_Medido por TACO, 26/10/2026. O A/B de corpus existe; o que falta é o A/B de
desfecho._