---
type: AcceptanceCriterion
title: S-1 — critério de aceitação do temperature scaling, escrito ANTES do resultado
description: Fica registado antes de o w44_support_fit existir, precisamente para não poder ser ajustado ao número que der. A validação é C — re-derivo a curva eu mesmo no split congelado — e a opção que escolho rejeitar.
plan_id: 2026-10-01-jev-harness-r2
dag: task_1790895761205640116::S-1
owner: touring-02
autor: Gabriel (aprovou a opção C em 01/10/2026)
timestamp: 2026-10-01T17:55:00-03:00
okf_version: "0.1"
---

# S-1 — critério de aceitação, escrito antes do número

## Porque este ficheiro existe antes do resultado

O canvas (01/10, §8) recomendeu **C** em vez de A: *validar o número* é a formulação
que deixa um número mau passar, porque confirma a aritmética e não a afirmação.
Aceitar C obriga a que o critério exista **antes** de ver o resultado — depois já
seria umarationalização com formato de critério.

**Data de registo: 01/10/2026, 17:55 BRT.** O `w44_support_fit` ainda não existe
(implementação é da touring-a7, S-1 dela).

## O problema medido

O aluno Laya está calibrado e serve **0,85 %** das decisões
(`keyword_fallback_rate = 0,9915`, 117 keyword / 1 laya em 118).
Medido no W4.3: **acurácia 0,608** no teste congelado, **ECE 0,206 → 0,174**,
curva **monotónica sub-confiante**, e a **τ = 0,6** (`STUDENT_TAU`,
`prompt_enhance.rs:293`) dá **acc 0,926 com suporte 22,5 %**.

A documentação atribui a sub-confiança a **logit drift pós-fine-tune**: o modelo
acerta mas devolve probabilidades achatadas (0,48–0,58) e falha o gate. O
remédio nomeado é **temperature scaling pós-hoc** — um escalar `T` no val split;
sub-confiança implica **T < 1**, que afia a distribuição **sem retreinar**.

## O que eu valido, e como

**C, não A.** Re-derivo a curva eu mesmo:

1. Confirmei o **sha256 do split congelado** antes de começar. Se não bater,
   paro — é o caso S-2.0, e o `--check` já me deu verde uma vez hoje.
2. Corro o modelo no **val split**, gero a curva de calibração com **o meu** código,
   e calculo acc/suporte em cada τ.
3. Comparo com a curva entregue. **Divergência que eu não consiga explicar = rejeição**,
   não "aproximadamente igual".
4. O τ em produção é `0,6`. Avalio a Operating Characteristic **nesse τ** e à volta dele.

## Critério de aceitação — escrito agora, vincula-me

O **w44_support_fit é aceite se e só se as três condições seguintes**:

| # | condição | limiar | porquê este número |
|---|---|---|---|
| **1** | Suporte a τ=0,6 sobe | **≥ 25 %** (de 22,5 %) | abaixo de +2,5 pontos é ruído sobre o tamanho do split; a melhoria tem de ser visível |
| **2** | Acurácia a τ=0,6 **não cai** | **≥ 0,90** | o que torna o τ seguro hoje é acc 0,926 com aquele suporte. Gains de suporte comprando precisão é a troca errada |
| **3** | ECE no val **não piora** | **≤ 0,174** | o ECE já foi de 0,206 a 0,174. Subir o suporte degradando a confiança troca um erro visível por um invisível |

**A recusa explícita, em advance:** aceito o resultado **e mantenho τ = 0,6 em
produção** se a condição 1 falhar isoladamente. Ou seja: um T que melhora a curva
mas não move o suporte **não entra**, porque o objectivo do W1 não é uma curva
bonita — é o aluno a servir mais de 0,85 %.

## O que falsificaria isto

1. **O sha256 do split não bater** → paro. É o teste de integridade, e é o mesmo
   mecanismo que me deu o `--check` verde do S-2.0.
2. **Ganho de suporte < 3 pontos** → provavelmente ruído; a resposta certa seria
   adiar para o S-2 (shadow-matrix) dar n suficiente, não afinar T até ao número
   sair certo.
3. **A curva ficar monotónica mas DESLOCADA** (não afiada) → T não é o botão. O
   problema seria do fine-tuning, não da calibração, e nenhuma quantidade de
   temperature scaling o resolve.
4. **O split de val ter mudado** desde o congelamento → invalida a comparação
   inteira, e a culpa é minha por não ter verificado primeiro.

## O que NÃO vou fazer

- Não vou aceitar o número "porque o cálculo está certo". É a opção A, e é
  exactamente a que C substitui.
- Não vou ajustar estes limiares depois de ver o resultado. Se ficarem errados,
  o registo de 17:55 é o que prova que estavam errados desde o início.
- Não vou mexer em τ em produção sem te pedir. Mudar o limiar muda o
  comportamento de decisões já tomadas, e essa parte não volta.

---

_Registado em 01/10/2026, antes de o número existir. Se este ficheiro divergir do
que eu fizer depois, é este que está certo — e a pergunta passa a ser porque é que
eu me desviei dele._

---

# Errata de métrica (02/10/2026 11:46 BRT — registrada ANTES da re-execução)

> **Correção do rótulo de horário (coordenadora, 11:50 BRT):** o título originalmente dizia
> "11:52", horário que era POSTERIOR ao relógio real (11:49 ao ser lido). A ordem, que é o que
> importa numa pré-registração, está provada pelos `mtime` dos arquivos: este documento
> 11:46:55 → `results-errata.json` 11:48:06 → `report-errata.md` 11:48:31. Nenhum critério
> foi alterado nesta correção.

**Pre-registro.** Esta seção existe antes de a corrida ser executada, pela mesma razão
que a de 01/10 17:55: depois de ver o número, um critério reescrito é racionalização com
formato de critério.

## O que estava errado

A **condição 2** media a **métrica errada**. Escrevia *"Acurácia a τ=0,6 não cai ≥ 0,90"*,
mas o 0,926 que a justificava era a **precisão@suportado**, não a acurácia global. E a
acurácia global é **invariante sob T** — o argmax não muda com a temperatura, logo ela
**nunca** desce abaixo de 0,90: a condição era **inatingível por construção**, e
`phases/S-1.md:19` mediu isso (acc global 0,625 em toda a grade). O resultado registrado
como `FITTED=None` é a condição errada reprovando, não o W1 falhando.

## A correção (o limiar NÃO muda — muda o que ele mede)

| | antes | **depois** |
|---|---|---|
| **condição 1** | suporte a τ=0,6 ≥ 25 % | **inalterada** |
| **condição 2** | acc global a τ=0,6 ≥ 0,90 | **precisão@suportado(τ=0,6) ≥ 0,90** — mesmo limiar, `suportado := max_prob ≥ τ` |
| **condição 3** | ECE no val ≤ 0,174 | **inalterada** |

Grade de T, split de val congelado e seed: **inalterados** (mesma grade 0,4→1,2 passo 0,02,
mesmo val, seed 20260922). **É uma errata de métrica, não um afrouxamento**: o limiar
continua 0,90 e a condição 1 continua ≥ 25 %.

**Regra de escolha, escrita agora:** `T* = argmax` suporte(τ) s.t. as DUAS condições.
Se nenhum T cumpre as duas → resultado declarado **"nenhum T satisfaz; config intocada;
W1 não adotado pelos dados"**, e `rl_agent_config.json` não é tocado.

**Nenhum limiar muda depois de ver o número.** A correção é de métrica, e a métrica errada
está nomeada com `file:line`.

## Estado do código no momento deste registo (verificado, não assumido)

O `scripts/w44_support_fit.py` **já implementa a versão corrigida**: `grid_search_t()`
reporta `precision_supported` e escolhe `T*` por `precision >= MIN_PRECISION (0.90)` e
suporte máximo (`w44_support_fit.py:34-76`); `test_accuracy_is_temperature_invariant`
(`test_w44_support_fit.py:43-47`) prova o invariante por asserção, e a suíte está
**4 passed**. Ou seja: **o RED→GREEN do código já ocorreu** em 01/10 — o que faltava era o
critério escrito, que continua divergindo do código. Este registo alinha o documento à
medida que o código já toma; a corrida abaixo é a **re-execução** sob o critério corrigido,
não a primeira aplicação dele.

**Não escrevo em `rl_agent_config.json`** — aplicar T\* é decisão do Gabriel, posterior.

