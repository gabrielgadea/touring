---
type: LoopLog
title: "Economia de Contexto — registro de execução"
description: "O que foi executado do plano em 04/09/2026, com o que a execução corrigiu do próprio plano."
plan: 2026-09-04-economia-de-contexto
plan_id: 2026-09-04-economia-de-contexto
timestamp: 2026-09-04
okf_version: 1
---

# Registro de execução — 04/09/2026

## Entregue

| fase | subtarefa | estado | evidência |
|---|---|---|---|
| P0 | S-0.1 régua `context_budget` | **feito** | `crates/touring-cli/src/cli/context_budget.rs`, 10 testes + sonda viva |
| P0 | S-0.2 fonte sem instrumentação nova | **feito** | lê o transcript do próprio Claude Code; funciona retroativamente |
| P1 | S-1.1 dedup por conteúdo | **feito** | `repeat_reference` + `emitted_content`, 5 testes |
| P2 | S-2.1 hint por evidência | **feito** | `symbol_bearing_edits`, 13 testes |
| P2 | S-2.2 expurgo do rastro superado | **feito** | `unresolved_failures`, 5 testes |
| P3 | S-3.1 uma escala CILA | **feito** | `touring-foundation/src/cila.rs` + guard cruzado em 256 níveis |
| P3 | S-3.2 `LESSON_BUDGET` sob a escala | **feito** (escopo corrigido) | ver abaixo |
| P3 | S-3.3 orçamento por turno | **feito** | `touring-hooks-shared/src/turn_budget.rs`, 6 testes |
| P4 | S-4.1 classe de custo no manifesto | **feito** | `flow_manifests.json` + gate + 3 guards |
| P4 | S-4.2 default cobra só classe A | **feito** | `work-outer.enforced_classes = ["A"]` |
| P4 | S-4.3 classe B (interação como gate) | **feito** | `loop_turn_record.py` + gate materializa; 130 testes verdes; registro vivo de 10,8 KB escrito sem nenhuma escrita manual |
| P5 | S-5.1 medir os tool results | **feito** | volume E reuso: `tool_output` na régua, 4 testes + sonda viva |
| P6 | S-6.1 verificar o CUR | **feito — hipótese confirmada** | CUR = 0,971; nada a corrigir |

## O que a execução corrigiu do plano

1. **S-3.2 tinha escopo errado.** O plano listava três constantes do
   `cli_suggester` como "orçamentos próprios". A leitura mostrou que só
   `LESSON_BUDGET` é orçamento de INJEÇÃO; `BUDGET` (rajada python-inline) e
   `G1_BODY_BUDGET` dimensionam o **programa que o remédio entrega**, e
   encolhê-los cortaria a correção em vez do ruído. O grep os agrupou; a leitura
   os separou. Quem alcança os 61% da injeção é S-3.3.

2. **A referência do dedup podia custar mais que o bloco.** A primeira forma
   tinha 144 B; `past-lessons` — a família com 86,4% de repetição — tem bloco
   médio de **48 B**. Deduplicar ali teria piorado exatamente o pior caso. O
   teste pegou, e o remédio é uma guarda aritmética: só elide quando a
   referência é menor.

3. **Os dois localizadores do F2.4 não eram iguais.** Colapsar duplicação exige
   ler os DOIS produtores; colapsei para o mais fraco e o teste pegou.

4. **A régua reportava 12× a mais.** Nove testes sintéticos verdes e o número
   ainda confiantemente errado: mensagens de usuário com `content` STRING — a
   forma comum de um prompt real — não contavam como turno. Só a sonda contra
   dados REAIS pegou.

## REGRA #21 — falhas pré-existentes fechadas no caminho

| falha | causa-raiz | correção |
|---|---|---|
| `e2e_test.py::test_phase_close_and_gate` | fixture `bundle` inexistente: um ERRO de coleta que contava como cobertura sem nunca executar | fixture criada |
| `test_cognicao_formal::mais_ausentes` | `chaves - set(...)` itera um SET: ordem por `PYTHONHASHSEED`, ranking diferente para a mesma entrada | desempate estável pela ordem do ESQUEMA |
| `test_doc_link_gate::mundos` | estratégia do Landlock com 0 elos causais onde o rito exige ≥5 | cadeia derivada da própria prosa, 6 elos |

## Fechamento — 04/09/2026, 2ª rodada

### P5 · a fração usada, medida — e a ação que ela REFUTA

A pergunta "o modelo leu?" não é mensurável. A que decide a mesma coisa é:
**quantos bytes um digest teria de carregar para preservar todo fato que o
modelo de fato USOU?** Essa é medível — as linhas e os identificadores do
resultado que reaparecem no que o assistente escreveu, ou nos argumentos das
chamadas seguintes, até o próximo turno humano. É **piso** do uso e portanto
**teto** do que um digest pode descartar: o lado conservador da decisão.

> ⚠ **Os números desta seção foram CORRIGIDOS pela cross-audit de 04/09/2026**
> (`docs/audits/cross-audit-2026-09-04.md`). Os valores contaminados ficam
> registrados abaixo com a causa — o histórico do defeito vale mais que um
> documento limpo. Achados A-1 (a janela de reuso acumulava desde o início do
> turno, contando texto ANTERIOR ao resultado) e A-5 (20,3% dos "turnos
> humanos" eram do harness).

| grandeza | publicado | **corrigido** | causa |
|---|---:|---:|---|
| `user_turns` | 165 | **133** | A-5 |
| `injected_bytes_per_turn` | 11.669 | **15.200** (+30,3%) | A-5 |
| `reuse_ratio` | 0,067 | **0,040** (−40,3%) | A-1 |

A régua ganhou `tool_output` (atribuição por ferramenta + reuso), 4 testes e a
sonda viva. Medido em 3 transcripts, **133 turnos**, 46 MB (valores já
corrigidos; entre parênteses o que fora publicado):

| ferramenta | resultados | bytes | share | linha% | 0-reuso |
|---|---:|---:|---:|---:|---:|
| **Bash** | 1.515 | 2.224.671 | **78,8%** | 6,0% (11,4%) | **14,8%** (5,5%) |
| **Read** | 265 | 417.661 | 14,8% | 14,9% (17,8%) | **7,2%** (2,0%) |
| Edit | 433 | 91.340 | 3,2% | 1,1% (2,5%) | 19,4% (0,0%) |

Reuso global **0,040**. E a varredura de limiar (10 transcripts, 4.301
resultados, digest hipotético de 2 KB) põe o joelho em **2 KB, não 8 KB**:

| limiar | n | %vol | reuso% | economia% |
|---:|---:|---:|---:|---:|
| 1 KB | 1.269 | 84,1% | 3,25% | 38,4% |
| **2 KB** | **707** | **69,9%** | **2,57%** | **44,4%** |
| 4 KB | 329 | 51,1% | 1,91% | 39,3% |
| 8 KB | 122 | 31,1% | 1,17% | 26,7% |

**A ação esboçada no plano ("digest por padrão nas dominantes") está REFUTADA na
forma que ela sugeria.** Duas medições a derrubam:

1. **As linhas reusadas estão distribuídas por igual** — decis re-medidos com a
   semântica corrigida: 6,15% · 6,34% · 6,53% · 5,54% · 6,71% · 6,31% · 7,10% ·
   6,38% · 5,80% · 5,36%. Ainda mais planos que os publicados (9,6% · 5,7% ·
   6,3% · 6,6% · 7,4% · 6,8% · 6,0% · 6,3% · 6,8% · 5,9%): **o pico do primeiro
   decil era artefato da contaminação A-1** — texto escrito antes do resultado
   casava com o cabeçalho dele. Cabeça+cauda de 40 linhas cobre ≥90% do reuso em
   apenas **17%** dos resultados grandes (publicado: 45%). Cortar por POSIÇÃO
   perde ainda mais do que eu havia dito.
2. ~~**Zero-reuso é raro** — 2,0% no Read, 5,5% no Bash: não existe população
   descartável.~~ **ISTO ESTAVA ERRADO** (A-1). Com o instrumento corrigido:
   **14,8% dos resultados do Bash e 7,2% dos do Read** não são observavelmente
   usados por nada. A população descartável **existe** e é o alvo real que esta
   auditoria revelou — não do plano, da correção do instrumento.

O que a medição SUSTENTA é o que o workspace já faz: digest **derivado do
conteúdo** (o programa calcula o agregado — `touring run --brief`, reflexo #8) e
**spill com `retrieval_hint`**, onde nada se perde, só se adia. O corte por
posição fica descartado com número, não com opinião — um resultado negativo
registrado como resultado, como o CUR da P6.

### P4/S-4.3 · a classe B, construída

`loop_turn_record.py` (novo) + `_materialize_class_b` no gate + `turn-record`
como artefato classe B do `work-outer`, que passa a exigir `["A","B"]`.

O executor roda no **Stop hook** — quando o turno JÁ acabou. Nesse instante o
transcript contém a resposta inteira, então não há nada a interromper: o texto
de que o registro é feito já foi escrito, para o humano, pelo motivo do humano.
A Lei L3 fica intacta (o artefato continua obrigatório e continua em disco);
mudam o **autor** e o **momento**. É o defeito D1 corrigido no ponto exato — o
gate cobra do plano de EXECUÇÃO o que cobrava do plano de RACIOCÍNIO.

Prova viva nesta sessão: registro de **10.817 bytes** materializado (21 blocos
de resposta, 12 linhas de espinha, **64 ações**; `Bash` 29× · `Edit` 28× ·
`Read` 6× · `Write` 1×) — e **zero** bytes escritos à mão.

**Fail-open é a razão de ser da classe.** Se o executor falhar, o artefato sai
do caminho crítico e a falha aparece em `class_b_unmaterialized` (ausência
exibida, nunca sumida) — a alternativa seria devolver a escrita manual que a
classe existe para eliminar. Exercitado: com `materializer` inexistente,
`missing=[]`, `complete=True`, `class_b_unmaterialized=["turn-record"]`.

## O que esta rodada corrigiu do próprio trabalho

5. **A primeira medição de reuso deu 0,0% em TODA ferramenta** — e 0,0% parecia
   um achado. Não era: o `Read` devolve `cat -n`, então cada linha chega com
   `<n>\t` na frente e nunca casa com a citação dela. Provado contra caso
   conhecido: 0 acertos crus, 17 após remover o prefixo. Virou o teste
   `o_prefixo_de_numeracao_do_read_nao_zera_o_reuso`, com red→green por mutação.
   *Provar o instrumento antes de acusar o sistema* — de novo.

6. **A espinha do registro extraiu 2 linhas de um turno inteiro de trabalho.** A
   regra só reconhecia negrito no INÍCIO da linha; num turno de trabalho a
   decisão viaja em negrito no meio da frase e em identificadores entre crases.
   A regra estreita não errou o instrumento — errou o que conta como estrutura.
   E faltava o essencial: um turno assim decide **agindo**. Sem a seção "o que o
   turno fez", o artefato seria decoração.

7. **Um guard existente reprovou por sobre-especificação.** O teste chamava-se
   *"o flow default nunca cobra artefato classe C"* e afirmava
   `enforced_classes == ["A"]` — mais estreito que o próprio enunciado. A classe
   B custa zero por construção e não viola nada do que ele protege. A asserção
   passou a seguir a intenção (`"C" not in enforced`) e ficou **mais forte**:
   uma classe B só é cobrável se declarar o executor que a escreve — sem isso,
   "B" seria classe C com outro nome.

## Não feito, e por quê

- **Deploy**: nada propagado. `update-touring` muda o comportamento de toda
  sessão CC — aguarda ordem de Gabriel. **Consequência exata, para ninguém ler
  errado**: `context_budget` é chamado por `cli_kpi`, que roda DENTRO do daemon;
  enquanto o daemon carregar o binário antigo, `touring kpi -j` **não** mostra
  `tool_output`. Os números acima vêm do binário de teste (`cargo test`), que é
  a verdade sobre correção — não sobre o que uma chamada viva devolve hoje. É a
  lição `binario-precede-a-fonte`, dita antes de custar de novo.
- **O limiar de 2 KB não virou executor.** A medição diz onde cortaria mais
  (44,4%) e diz que cortar por posição PERDE. Um executor de digest só se
  justifica na forma derivada-do-conteúdo, que já existe (`--brief`); construir
  um segundo, por limiar, seria criar a segunda fonte que este plano inteiro
  diagnostica.

## 2026-09-04T12:10:58.007293-03:00 — S-4.3-classe-b done

Classe B construida: loop_turn_record.py escreve o registro do turno no Stop hook (turno ja concluido), _materialize_class_b o invoca antes de cobrar, work-outer exige [A,B]. Lei L3 intacta: muda o autor, nao a exigencia. Prova viva 10.817 B / 64 acoes / zero escritas manuais; fail-open exercitado.

## 2026-09-04T12:11:12.808611-03:00 — S-5.1b-fracao-usada done

Fracao USADA dos tool results medida na regua (tool_output: atribuicao por ferramenta + reuso linha/token). Reuso global 0,067; Bash 79% do volume; zero-reuso 2-5,5%. Joelho em 2 KB (44,4%), nao 8 KB. Decis uniformes refutam o digest por truncagem: cabeca+cauda cobre >=90% do reuso em so 45% dos grandes. Sustentado: agregado derivado do conteudo (--brief) + spill com retrieval_hint.
