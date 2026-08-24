---
okf_version: "1.0"
type: Strategy
title: "Code mode antes da declaração — apagar o quarto onde a inferência prematura mora"
description: "Medição da sessão 24/08 (4% de adoção, 33 rajadas, 135 round-trips colapsáveis) e seis estratégias cujo critério de projeto é reduzir estados de conhecimento parcial, não pedir melhor comportamento dentro deles"
tags: [code-mode, afordancia, medicao, antipadrao, protocolo]
timestamp: 2026-08-24T18:25:00-03:00
plan_id: 2026-08-24-code-mode-antecipa
scope: /home/gabrielgadea/projects/touring
---

# Code mode antes da declaração

## A medição (fatos, 1.0)

Sessão `2f2d716c`, medida pelo próprio transcript via `touring run`:

| métrica | valor |
| --- | --- |
| chamadas Bash | **674** |
| que usam `touring run` | **30 (4%)** |
| inspeção pura (grep/sed/cat/ls/find/sqlite3) | 270 (40%) |
| rajadas de ≥3 inspeções consecutivas | **33** (maior: **14**) |
| round-trips colapsáveis em 1 chamada | **135** |
| exit code lido após pipe sem `pipefail` | **3** |

4% numa sessão em que eu **construí e consertei o code mode**. E o nudge
`code-mode-loop conf=0.95` disparou dezenas de vezes — eu o li e segui adiante.
É a reprodução exata de `protocol-adherence-diagnosis`: MUST com confiança 0.95
ignorado na própria sessão que o emitiu.

## O mecanismo (por que code mode ANTECIPA, e não apenas acelera)

Uma rajada de N inspeções cria **N−1 estados de conhecimento parcial**. A rajada
de 14 desta sessão teve 13 momentos em que eu segurava evidência incompleta —
e foi de dentro de um desses momentos que saiu cada declaração prematura do dia:

- o comentário do `arm_marker` ("sandboxar quebraria a escrita") — nunca executado, **falso**;
- o `mem-vazio` classificado como "decisão de arquitetura entre dois candidatos" — ler o código mostrou **um outlier**;
- `EXIT=0` reportado três vezes a partir do `tail`, não do comando.

Um programa só produz **zero** estados intermediários.

> **Code mode não me disciplina na hora de falar. Ele apaga o quarto em que a
> fala prematura pode acontecer.**

Daí o critério que ordena tudo abaixo:

> **A estratégia REDUZ estados parciais, ou pede melhor comportamento dentro
> deles?** Só a primeira funciona — medido duas vezes, em mim, hoje.

Corolário duro: **nenhum nudge novo.** A camada de anúncio já foi medida e
reprovada. O que falta não é aviso, é `U(a)`.

## As seis estratégias

### E1 — A pergunta define a unidade, não o arquivo · *teeth no contador que já existe*

**Regra**: pergunta que atravessa ≥3 arquivos/fatos → a PRIMEIRA ação é um
programa. Não "considere usar": precondição.

O contador de rajada **já existe** (`CODE_MODE_WINDOW_SECS`, `scan_class_key`,
dispara na 3ª busca atômica da janela). Não falta detecção — falta consequência.
A escalada honesta: da 4ª inspeção da mesma classe na janela, o hook **nega** em
vez de sugerir, entregando o `touring run` já derivado como remédio.

É o único ponto onde o hook pode mudar `U(a)` em vez de conversar.
Risco: negar exploração legítima. Mitigação: só a 4ª+ da MESMA classe, sempre
com o comando pronto, bypass documentado. **Custo: baixo** (o contador existe).

### E2 — O instrumento antes da leitura · *lint determinístico*

`EXIT=$?` depois de um pipeline sem `set -o pipefail` lê o exit do `tail`.
Três vezes hoje; três vezes eu reportei "exit 0" de um comando que não medi.

Lint de string, PreToolUse, zero LLM: `AntipatternKind::ExitCodeThroughPipe`,
na enum que já existe (`workflow/baseline.rs`). Bloqueia a medição malformada
**antes** que a saída dela vire afirmação.

**Custo: baixo. Confiança: a mais alta do conjunto.** É por onde começar.

### E3 — A contrafactual precisa de um run_id · *alta precisão por construção*

Afirmação sobre o que ACONTECERIA ("seria", "quebraria", "faria", "impediria")
é a única classe **infalsificável por construção** — e foi exatamente onde eu
errei no `arm_marker`.

PreToolUse em Write/Edit varrendo as linhas ADICIONADAS por modais
contrafactuais em comentário, exigindo citação de `run_id`. Preciso porque
contrafactual é raro: pouco ruído, e mira o pior erro do dia.

**Custo: médio.**

### E4 — Veredito sem evidência é recusado pelo lint · *estrutural, no grafo*

Em `adw.py`: um nó que emite `VERDICT=`/`METRIC=` DEVE ler de ≥1 nó `code`
(`{{nodes.X.summary}}`). Espelho invertido de `_lint_fake_waiting`, e
`node_data_reads` já computa o grafo de dados.

Torna a medição **upstream por construção** em todo fluxo publicado — o
executor chama o medidor antes do sintetizador porque o grafo não compila de
outro jeito.

**Custo: baixo-médio.**

### E5 — A colheita torna a segunda medição mais barata que a asserção · *já existe*

`touring run --harvest <slug>` persiste o programa como `#kind:snippet` e o
matricula na escada de confiança. É a **única** estratégia que compõe: cada
medição colhida baixa `C(tokens)` da próxima, até medir custar menos que
inventar.

Exercitado agora: `snippet:auditoria-sessao-tooluse`.
E `touring run --file` **já existia** — a fricção de dois passos (heredoc para
o scratchpad, depois `cat`) que eu inventei hoje não estava lá. Parte do 4% não
é afordância faltando; é afordância existente e não lida.

**Custo: zero.** É uso, não construção.

### E6 — O Stop hook é o único executor que vê a minha prosa

E1–E4 pegam o artefato; nenhum deles impede uma frase. A superfície da mensagem
não tem executor — **exceto** o Stop hook, que hoje me pegou: eu rodei o juiz,
declarei convergência e nunca entreguei o veredito ao marcador.

Extensão barata: turno com ≥20 chamadas Bash e **zero** `touring run` recebe um
bloqueio com o diagnóstico da rajada. Grosseiro, mas é o único lugar onde a
declaração e um executor se encontram.

**Custo: baixo.**

## Ordem recomendada

**E2** (determinístico, 3 instâncias medidas) → **E5** (custo zero, compõe) →
**E4** (estrutural, fecha os fluxos) → **E1** (teeth; exige cuidado com
falso-bloqueio) → **E6** → **E3**.

## Nota de retratação — e o que ela ensina

Uma versão anterior desta página registrava, como possível defeito, que
`touring memory query '#kind:snippet #lang:python'` não devolvia o snippet que o
`harvest_hint` acabara de criar. **Era falso.** `touring memory query` tem
`--limit` com **default 10**; com `--limit 40` a query devolve 7 resultados,
entre eles os quatro snippets colhidos. O `harvest_hint` estava correto.

Registro em vez de apagar porque o erro é uma instância exata da classe que o §7
mede — **`ausencia_como_zero` (22%)**: li um resultado truncado como o conjunto
completo. E é reincidência: existe uma memória minha sobre precisamente isto
(*"quase reportei 10 snippets — era o limite default da query"*).

Duas conclusões que valem mais que a observação que morreu:

1. **Toda saída paginada precisa dizer que é paginada.** Um `count: 10` com
   default 10 é indistinguível de um universo de 10 elementos. É o mesmo defeito
   de forma que o piso anti-vácuo (§5 R1/R2) existe para cobrir — e do lado do
   *consumidor* a regra é: antes de concluir ausência a partir de uma listagem,
   **rode-a de novo com o limite dobrado**. Se o número muda, você estava lendo
   uma janela.
2. **Ter a lição na memória não a aplica.** Eu tinha o registro e repeti o erro
   duas vezes na mesma hora. É a evidência mais limpa desta página inteira de que
   conhecimento não é afordância — e de por que os §6.4 e §4.7 propõem
   executores, não lembretes.

---

# Parte II — Diagnóstico completo, repertório, fluxos e prevenção

> Tudo abaixo é medido sobre **55 sessões / 5.816 tool calls** e **703 memórias**
> (`diag_tools.py`, `diag_gate.py`, `diag_mem.py`, no scratchpad da sessão).
> Contagens de tool call: **fato (1.0)**. Classificação lexical das memórias:
> **inferência (~0.7)** — a ordem de grandeza é o achado, não o número exato.

## §4 — Diagnóstico de TODAS as tool calls

### 4.1 Distribuição

| tool | chamadas | % |
| --- | ---: | ---: |
| Bash | 4.035 | **69%** |
| Read | 747 | 12% |
| Edit | 704 | 12% |
| Write | 96 | 1% |
| ToolSearch · SendMessage · Agent | 99 | 2% |
| resto (Task*, Skill, Web*, MCP) | ~135 | 2% |

**Bash é 69% de tudo.** Qualquer gate que não olhe para Bash está olhando para
um terço do problema.

### 4.2 Taxonomia dos 4.037 comandos Bash

| classe | n | % |
| --- | ---: | ---: |
| **inspeção** (grep/rg/sed -n/cat/head/tail/ls/find/wc/sqlite3/jq) | **1.843** | **45%** |
| touring_cli | 613 | 15% |
| escrita (`>`/`rm`/`mv`/`mkdir`/`sed -i`) | 576 | 14% |
| outro | 509 | 12% |
| **code_mode** (`touring run`) | **173** | **4%** |
| build/test | 159 | 3% |
| git · processo | 164 | 4% |

45% inspeção contra 4% code mode. **A massa colapsável é 11× a adoção.**

### 4.3 Sequências (bigramas) — o que um gate pode prever

| A → B | n | P(B\|A) |
| --- | ---: | ---: |
| **Bash → Bash** | 3.447 | **86%** |
| Read → Read | 441 | 59% |
| Edit → Edit | 420 | 59% |
| Write → Bash | 67 | 69% |
| Edit → Bash | 203 | 28% |
| Read → Edit | 117 | 15% |

`Bash → Bash` a 86% não é uma transição: é um **estado absorvente**. O shell
virou REPL, e cada passo é um round-trip. O gate certo não prevê o próximo tool
— ele mede **quanto tempo já se está nesse estado**.

### 4.4 Distribuição do tamanho da rajada de Bash — onde pôr o limiar

| tamanho | n | acumulado |
| ---: | ---: | ---: |
| 1 | 169 | 28% |
| 2 | 108 | 47% |
| 3 | 60 | 57% |
| **4** | 57 | **66%** |
| 5 | 31 | 72% |
| 6–10 | 73 | 88% |
| 11–19 | 26 | 94% |
| **20+** | **31** | 100% |

Rajada máxima observada: **315 chamadas Bash consecutivas**.
Classe dominante nas rajadas ≥3: **inspeção (158 de 285)**.
**1.278 chamadas de inspeção vivem dentro de rajadas ≥3** — é essa a massa.

> **O joelho está em 4.** Um gate em `len ≥ 4` deixa 66% das rajadas passarem
> intocadas e alcança a cauda onde mora quase toda a inspeção repetida. Em
> `len ≥ 6` alcança 23% das rajadas — mais seguro, menos tração.

### 4.5 Conformidade P9 (Verify-After) — o achado mais duro

| | |
| --- | ---: |
| rajadas de Edit/Write | 351 |
| seguidas de build/test em ≤4 chamadas | **62 (17%)** |

**83% das rajadas de edição não são validadas de imediato.** A regra P9
("toda mutação seguida de validação") é constitucional e é honrada em 17%.

### 4.6 Antipadrões estruturais

| padrão | n |
| --- | ---: |
| `Read` sem localizar antes (janela 3) | **362** |
| `Edit` sem `Read` do MESMO arquivo (janela 12) | **335** |
| chamadas redundantes (mesmo tool+input na sessão) | 46 |

### 4.7 Candidatos a gate no PreToolUse (hook Rust)

Ordenados por (tração × baixo risco de falso-bloqueio). "Vê" = o que o hook
já tem em mãos no evento, sem I/O extra.

| # | Gate | Vê | Limiar | Tração medida | Risco FP | Custo |
| --- | --- | --- | --- | ---: | --- | --- |
| **G1** | **Rajada de inspeção** | classe do comando + contador de janela (já existe) | 4ª da mesma classe | 1.278 chamadas | médio — mitigar com bypass e comando pronto | baixo |
| **G2** | **Exit code através de pipe** | string do comando | `\|` + `$?` sem `pipefail` | 3 hoje, determinístico | ~zero | **baixo** |
| **G3** | **Edit sem Read do mesmo arquivo** | `file_path` + janela de tools | janela 12 | **335** | baixo | baixo |
| **G4** | **Read sem localizar** | janela de tools | janela 3 | **362** | médio (leitura dirigida é legítima) | baixo |
| **G5** | **Rajada de Edit sem validação** | contador de Edit + ausência de build/test | ≥3 edits, 0 build | **289 rajadas** | baixo — avisar no Stop, não bloquear o Edit | médio |
| **G6** | **Chamada redundante** | hash (tool, input) na sessão | repetição exata | 46 | ~zero | baixo |

G2 e G6 são **determinísticos e sem falso-positivo plausível** — são os
primeiros. G3 tem 335 instâncias e o hook já tem `EditWithoutRead` na enum.
G1 é o de maior tração e o único que exige *teeth*, não aviso.

## §5 — Repertório de estruturas de script para code mode

Oito formas. Cada uma existe porque **evita um modo de falha medido** (§7), não
porque é elegante. Todas obedecem ao reflexo D5: o contexto recebe o **agregado**,
nunca o dump.

### R1 · Varredura-e-agregado — *mata os 45% de inspeção*

**Quando**: a pergunta atravessa ≥3 arquivos/fatos.
**Evita**: rajada (1.278 chamadas), estados parciais.

```python
# touring run --lang python --file varredura.py --args '["<glob>","<padrão>"]'
import sys, glob, re, collections
alvo, padrao = sys.argv[1], re.compile(sys.argv[2])
cnt = collections.Counter(); ex = []
for f in glob.glob(alvo, recursive=True):
    for i, l in enumerate(open(f, encoding="utf-8", errors="ignore"), 1):
        if padrao.search(l):
            cnt[f] += 1
            if len(ex) < 5: ex.append(f"{f}:{i}")
print(f"ARQUIVOS={len(cnt)} HITS={sum(cnt.values())}")
for f, n in cnt.most_common(10): print(f"  {n:>4}  {f}")
print("ex:", *ex, sep="\n  ")
```

**Regra de ouro**: imprima **contagem primeiro, exemplos depois, teto sempre**.

### R2 · Guarda de família com piso anti-vácuo — *mata o conserto parcial*

**Quando**: um defeito pode ter irmãos (135 memórias dizem que tem).
**Evita**: `familia_parcial` (19%), `teste_vacuo` (1%).

```python
sitios = [s for s in varrer_fonte() if padrao_defeituoso(s)]
assert total_examinado >= PISO, f"só {total_examinado} examinados — a varredura não achou a família"
assert sitios == [], sitios      # a lista, não a contagem: diz QUAIS
```

Duas asserções, nunca uma. A primeira prova que a varredura **enxergou**; a
segunda prova que **não achou nada**. Sem a primeira, zero achados é
indistinguível de zero procurados.

⚠ **Guard auto-referente não carrega a própria agulha.** Se o script varre o
arquivo que o contém, monte o padrão em runtime (`f"let x = {'current_dir'}()"`).
Custou uma falha real hoje.

### R3 · Matriz cross-caller (C08) — *mata a assimetria silenciosa*

**Quando**: 2+ callsites que *deveriam* ser simétricos.
**Evita**: `comentario_falso` (10%) — "keep both sites in sync" é a evidência mais fraca.

```python
acoes = {}
for fn in FUNCOES:
    corpo = extrair_corpo(fn)
    acoes[fn] = {a for a in ACOES_CONHECIDAS if a in corpo}
universo = set().union(*acoes.values())
for a in sorted(universo):
    linha = {fn: ("X" if a in acoes[fn] else "·") for fn in FUNCOES}
    if "·" in linha.values() and "X" in linha.values():
        print("ASSIMETRIA", a, linha)     # célula vazia = bug em potencial
```

### R4 · Controle negativo — *mata o instrumento não calibrado*

**Quando**: antes de confiar em QUALQUER verificador. `instrumento_errado` é
**38%** do corpus — a classe mais cara depois de staleness.
**Evita**: gate que aprova tudo, teste que passa por acidente, detector vácuo.

```python
assert verificador(ENTRADA_BOA) is True,  "o verificador não aprova o que deveria"
assert verificador(ENTRADA_RUIM) is False, "o verificador NÃO SABE REPROVAR — é vácuo"
```

**A segunda asserção é a que importa.** Um verificador que nunca falhou não foi
provado: foi assumido. Hoje a guarda estrutural só virou prova quando **falhou de
verdade** numa ocorrência real.

### R5 · Delta antes/depois — *mata a correção não provada*

**Quando**: qualquer afirmação de "corrigi".
**Evita**: `inferencia_sem_medida` (11%).

```python
antes = medir()
aplicar()
depois = medir()
print(f"DELTA={depois-antes} antes={antes} depois={depois}")
assert depois != antes, "a métrica não se moveu — a correção não tocou o que se mediu"
```

O `assert` final é o que separa "rodei um fix" de "provei um fix".

### R6 · Descoberta de conjunto + asserção de cobertura — *mata a lista escrita à mão*

**Quando**: agir sobre "todos os X".
**Evita**: `familia_parcial`, e o clássico "corrigi 4 dos 5 sítios".

```python
descobertos = descobrir()                      # CÓDIGO descobre, não eu
assert len(descobertos) >= PISO
tratados = [x for x in descobertos if tratar(x)]
print(f"COBERTURA={len(tratados)}/{len(descobertos)}")
assert len(tratados) == len(descobertos), set(descobertos) - set(tratados)
```

Nunca escreva a lista de alvos: **descubra-a**. Uma lista escrita à mão nasce do
que eu lembrei, que é exatamente o que falha.

### R7 · Forense de transcript/journal — *mata a impressão sobre o processo*

**Quando**: qualquer afirmação sobre como a sessão/sistema se comportou.
**Evita**: `ausencia_como_zero` (22%) — foi assim que os números desta análise saíram.

```python
for linha in open(transcript, encoding="utf-8", errors="ignore"):
    try: d = json.loads(linha)
    except Exception: continue          # linha corrompida NÃO é fim do arquivo
    ...
print("AGREGADO=...")                    # nunca o dump
```

### R8 · `--orchestrate` — *o daemon dentro do sandbox*

**Quando**: a pergunta precisa do índice/wiring/memória, não só do disco.

```bash
touring run --sdk-stub                       # o contrato tipado, primeiro
touring run --lang python --orchestrate --code '
for s in ["A","B"]:
    d = touring.index_find(s)
    print(s, len(d.get("definitions", [])), touring.wiring_impact(s, 2).get("direct_consumers"))
'
```

Um programa, N consultas ao daemon, **zero** MCP.

### As quatro invariantes do repertório

1. **Agregado, nunca dump** — ≤~200 tokens; use `--brief`.
2. **Piso anti-vácuo** — toda varredura afirma quantos itens examinou.
3. **Controle negativo** — todo verificador prova que sabe reprovar.
4. **Colher** — `--harvest <slug>` na segunda vez que a forma aparecer. É a única
   coisa aqui que **compõe**: cada colheita baixa `C(tokens)` da próxima.

E use `--file`: escrever heredoc para o scratchpad e depois passar por `--code`
é fricção inventada — hoje eu inventei duas vezes.

## §6 — Expansão dos fluxos ADW e dos tipos de loop

### 6.1 O que existe hoje (verificado em `adw.py`)

**6 tipos de nó**: `code · agent · gate · loop · human · parallel`.

**Um único tipo de loop**: `body + max_iters + dry_rounds` (+ `stagnation_rounds`
opt-in). O predicado de terminação é **um só** — `NEW_FINDINGS=0` por K rodadas.

Isso é correto para **descoberta**. Mas os modos de falha medidos no §7 não são,
em maioria, falhas de descoberta — e um loop com o predicado errado não termina
errado: ele termina **confiante**.

### 6.2 Cinco tipos de loop, um por predicado de terminação

| tipo | termina quando | modo de falha que ataca | marcador |
| --- | --- | --- | --- |
| `until_dry` *(existe)* | `NEW_FINDINGS=0` por K rodadas | descoberta incompleta | `NEW_FINDINGS=` |
| **`until_fixpoint`** | `METRIC=` **não muda** por K rodadas | `staleness` (**42%**) | `METRIC=` |
| **`until_covered`** | conjunto descoberto **exaurido** | `familia_parcial` (19%) | `COVERAGE=n/m` |
| **`until_calibrated`** | instrumento provou que sabe **reprovar** | `instrumento_errado` (38%) | `CONTROL=pass\|fail` |
| **`retry_with_feedback`** | gate passa **ou** budget esgota | retry cego (memória registrada) | veredito do gate → prompt |

**Por que `until_fixpoint` é diferente de `until_dry`.** "Nada novo" e "mesma
resposta duas vezes" são predicados distintos. Staleness é 42% do corpus: o valor
está lá, só está **velho**. Um `until_dry` sobre isso converge de primeira e
declara sucesso sobre um número obsoleto. É o modo de falha "rótulo de versão não
prova build" em forma de loop.

**Por que `until_covered` não é um contador.** `max_iters` é um teto de esforço;
cobertura é uma **propriedade do conjunto**. O runner deve comparar `n/m` e
recusar `n < m`, jamais aceitar "acabaram as iterações" como conclusão. É a Lei
L2 aplicada a conjunto em vez de sinal.

**`until_calibrated` é uma pré-condição, não um loop de trabalho.** Roda o
verificador contra um controle negativo conhecido; se ele **passa** no que
deveria reprovar, o fluxo aborta antes de gastar um único agente. Ataca a
segunda maior classe do corpus por um preço trivial.

### 6.3 Dois tipos de nó novos

| nó | contrato | por quê |
| --- | --- | --- |
| **`probe`** | um `code` que DEVE emitir `FACT=<chave>=<valor>` e cujo `run_id` fica citável pelos nós seguintes | dá endereço à evidência: um veredito passa a poder **citar** o que o provou |
| **`control`** | roda o verificador contra entrada boa E ruim; falha o fluxo se ele não reprovar a ruim | materializa R4 como nó, não como disciplina |

### 6.4 Regras de lint que tornam a expansão honesta

Sem elas, os tipos novos são vocabulário — e vocabulário é anúncio.

1. **`_lint_verdict_needs_evidence`** — nó que emite `VERDICT=`/`METRIC=` DEVE
   ler de ≥1 nó `code`/`probe` (`{{nodes.X.summary}}`). Espelho invertido de
   `_lint_fake_waiting`; `node_data_reads` já computa o grafo de dados.
   **É a regra que põe a medição upstream por construção.**
2. **`_lint_loop_marker_matches_type`** — `until_fixpoint` sem `METRIC=` no corpo,
   ou `until_covered` sem `COVERAGE=`, é erro. Um loop cujo corpo não fala o
   marcador do seu predicado exaure `max_iters` em silêncio.
3. **`_lint_gate_has_control`** — todo `gate` com `verdict_contract = true` deve
   ter um `control` upstream **ou** declarar `control_waived = "<razão>"`. A
   dispensa é permitida; a **omissão** não.
4. **`_lint_sweep_declares_floor`** — nó com `COVERAGE=` deve declarar
   `min_discovered`. Cobertura `0/0` é o `0/0 specs` do auditor de ontem.

### 6.5 Fluxos novos que esses tipos habilitam

| fluxo | forma | ataca |
| --- | --- | --- |
| **`family-fix`** | `probe`(descobre sítios) → `until_covered`(trata cada) → `gate`(cobertura==total) | o conserto de 4 dos 5 sítios |
| **`freshness-audit`** | `until_fixpoint` sobre versão/hash/contagem, não sobre rótulo | staleness (42%) |
| **`instrument-first`** | `control` → *só então* o fluxo real | instrumento errado (38%) |
| **`claim-ledger`** | cada `probe` grava `FACT=`; o `phase-close` exige que toda afirmação do relatório cite um `run_id` | inferência sem medida |

`family-fix` e `instrument-first` são os dois de maior retorno: juntos cobrem
**57%** das marcações do corpus.

## §7 — Mapeamento de erros, bugs e gaps · o que code mode evitaria

### 7.1 O corpus

**703 memórias** em 9 projetos (analise 449 · global 145 · konverter 58 ·
touring 46 · resto 5). Classificação **lexical e conservadora** (~0.7): uma
memória cai em vários modos; **21% não casaram nenhum padrão** e ficaram fora.

| modo de falha | n | % do corpus | prevenível por programa? |
| --- | ---: | ---: | --- |
| **staleness** — valor lido é velho; rótulo ≠ realidade | **296** | 42% | ✅ `until_fixpoint` · R5 |
| **instrumento_errado** — o medidor é que falhava | **269** | 38% | ✅ `control` · R4 |
| **ausencia_como_zero** — silêncio lido como zero/sucesso | **159** | 22% | ✅ piso anti-vácuo · R1/R7 |
| **familia_parcial** — corrigido 1 de N sítios | **135** | 19% | ✅ `until_covered` · R2/R6 |
| inferencia_sem_medida — afirmei sem medir | 84 | 11% | ⚠ parcial — R5, mas a fala não tem executor |
| comentario_falso — doc/comentário afirma o que o código não faz | 76 | 10% | ⚠ parcial — R3, E3 |
| config_desligada — capacidade existe, ninguém ligou | 34 | 4% | ✅ R1 sobre config |
| teste_vacuo — passava por acidente | 14 | 1% | ✅ R4 |

**907 de 1.067 marcações (85%)** caem em classes que **um programa varrendo a
família inteira teria pego**.

### 7.2 A leitura que importa

As duas maiores — staleness (42%) e instrumento errado (38%) — **não são erros de
raciocínio**. São erros de **medição**: li um valor velho, ou confiei num medidor
que nunca provei. Nenhuma quantidade de cuidado meu os evita, porque o cuidado
opera sobre o valor que chegou, não sobre a sua procedência.

É por isso que a resposta é estrutural. Um `control` que roda em 200 ms teria
barrado 38% do corpus; um `until_fixpoint` que compara duas medições teria
barrado 42%. Nenhum dos dois exige que eu seja mais atento — exigem que o
executor rode antes.

### 7.3 Estratégias, por classe

| classe | estratégia | executor | § |
| --- | --- | --- | --- |
| staleness | toda leitura de estado é **duas** leituras, e a igualdade é o critério | `until_fixpoint` + R5 | 6.2 · 5 |
| instrumento errado | nenhum verificador é usado antes de reprovar um controle negativo | nó `control` + `_lint_gate_has_control` | 6.3 · 6.4 |
| ausência como zero | toda varredura declara **quantos itens examinou** | `_lint_sweep_declares_floor` + R1/R2 | 6.4 · 5 |
| família parcial | a lista de alvos é **descoberta**, nunca escrita | `until_covered` + R6 | 6.2 · 5 |
| inferência sem medida | contrafactual em artefato exige `run_id` (E3); prosa só tem o Stop hook (E6) | G2/G3 + Stop | 4.7 |
| comentário falso | comentário que afirma comportamento vira alvo de R3 na revisão | E3 | — |
| config desligada | inventário periódico de afordâncias com **adoção zero** | R1 sobre config + `touring kpi` | 5 |

### 7.4 O ponto cego que eu não vou fingir que fechei

**A fala não tem executor.** G1–G6 pegam a chamada; E3 pega o artefato; E4 pega
o grafo. Nenhum impede uma frase minha. O único executor que vê a minha prosa é o
Stop hook — e hoje ele me pegou uma vez, declarando convergência sem ter entregue
o veredito ao marcador.

Portanto a estimativa honesta: as afordâncias acima atacam **~85%** do corpus
histórico, e **quase nada** da classe "eu disse antes de medir" enquanto ela vive
só na mensagem. O que realmente encolhe essa classe não é um gate sobre a fala —
é o §4.4: **apagar os estados parciais de onde a fala prematura sai.** Um gate em
`len ≥ 4` não me proíbe de falar. Ele faz com que, quando eu falar, eu já tenha a
resposta inteira.

### 7.5 Ordem de execução consolidada

| passo | item | custo | tração |
| --- | --- | --- | ---: |
| 1 | **G2** exit-code-através-de-pipe | baixo | determinístico |
| 2 | **G6** chamada redundante | baixo | 46 |
| 3 | **E5/R-todos** usar `--file` + `--harvest` | zero | compõe |
| 4 | **G3** Edit sem Read | baixo | 335 |
| 5 | **E4/6.4-1** veredito exige evidência | baixo-médio | estrutural |
| 6 | **nó `control`** + lint 6.4-3 | médio | **38%** do corpus |
| 7 | **`until_covered`** + lint 6.4-4 | médio | **19%** |
| 8 | **`until_fixpoint`** | médio | **42%** |
| 9 | **G1** teeth na rajada | médio | 1.278 chamadas |
| 10 | **G4/G5 · E3 · E6** | médio-alto | cauda |

Passos 1–5 são baratos e não bloqueiam nada que hoje funcione. **6–8 são os que
movem o ponteiro**, porque atacam os dois modos que somam 80% do corpus e que
nenhuma dose de atenção resolve.
