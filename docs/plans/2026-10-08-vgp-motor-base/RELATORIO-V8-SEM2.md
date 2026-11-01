# RELATORIO-V8-SEM2 — o held-out web: denominadores, concentração e selo

> V-8 SEM2 da lista consolidada §1a. A entrega é hipótese até o juiz da
> coordenadora (touring-00 revisa; nada foi mergeado no branch principal).

## 1. O que foi pedido

1. **Completar os denominadores** do held-out web (hoje 83 HTML + 21 JS de 300;
   `~/.claude/touring/vgp/corpus_holdout_web_20261012/meta.json` com
   `atinge_alvo=false`).
2. **Resolver a concentração OSS.**
3. **Gerar selos externos** que o validador atual aceite.
4. **Classe sem n suficiente sai como "não provada"**, sem inflar o número.

## 2. O estado medido (nada aqui é declarado sem medição)

### 2.1 O estrato `transcript` (o corpus web)

| classe | n | alvo | arquivos distintos | sessões distintas | concentração máx. por arquivo | por sessão | `atinge_alvo` |
|---|---:|---:|---:|---:|---:|---:|---|
| html | 83 | 300 | 10 | 8 | **46 (55,4 %)** | **47 (56,6 %)** | false |
| javascript | 21 | 300 | 8 | 4 | 8 (38,1 %) | **17 (81,0 %)** | false |

O topo do html é um arquivo só (46 das 83 entradas,
`…/harvest-eleitoral-rs/entregaveis/PECA-DOIS-VOTOS.html`); o topo do
javascript é uma sessão só (17 das 21).

### 2.2 O estrato `oss` (a camada de repositórios abertos)

| camada | n | chaves distintas | concentração máx. por chave | teto declarado |
|---|---:|---:|---:|---:|
| `oss/tsx` | 213 | 127 | 7 (3,3 %) | 7 ✓ |
| `oss/typescript` | 214 | 128 | 7 (3,3 %) | 7 ✓ |
| `oss/javascript` | 3 | 3 | 1 (33 %) | 1 ✓ |
| `oss/_podado_20261009-215331/tsx` | 180 | 112 | **10 (5,6 %)** | 3 % ✗ |
| `oss/_podado_20261009-215331/typescript` | 191 | 119 | **12 (6,3 %)** | 3 % ✗ |

O selo da camada (`oss/selo.json`) **declara** `teto_concentracao: 0.03` e
**mede** `concentracao_medida: {dentro_do_teto: false, infratores: [...]}` —
ou seja: a violação do teto pela camada *podada* já está registrada pelo
próprio selo (é a concentration que ele mede, não só a que ele declara).
O selo da camada principal (`oss/tsx`, `oss/typescript`) está dentro do teto
(7 = o `teto_por_chave_no_fim` que ele declara ✓).

## 3. O que foi feito

### 3.1 O selo EXTERNO que o validador atual aceita

Novo script: `scripts/selar_holdout_web.py` (no bundle). O selo anterior era
um `meta.json` com um `metodo_selo` **declarado** — nenhum dos campos que
`confere_selo.py` exige, e morava **dentro** do corpus (o `sha_tree` do corpus
incluiria o próprio selo, e o validador reprova um selo dentro).

O selo novo:

| | |
|---|---|
| onde | `~/.claude/touring/vgp/corpus_holdout_web_20261012.selo.json` — **fora** do corpus ✓ |
| formato | `sha_tree` + `arquivos` + `bytes` (os três que `confere_selo.py` exige) + os denominadores por classe + a concentração **medida** + a situação da prova |
| conferido | `python3 scripts/confere_selo.py --corpus ~/.claude/touring/vgp/corpus_holdout_web_20261012 --selo ~/.claude/touring/vgp/corpus_holdout_web_20261012.selo.json` → **INTACTO** (rc=0, `sha_tree 42106741066f40f9`) |
| denominadores | `~/.claude/touring/vgp/corpus_holdout_web_20261012.denominadores.json` (a medição completa, por classe) |

### 3.2 A situação da prova: **NÃO PROVADA**, sem inflar o número

O `n` para provar ≤ 1 % de falso deny com 0 falsos (Clopper-Pearson 95 %) é
**299**. As duas classes do web estão abaixo dele (83 e 21), e o
`atinge_alvo` é `false` nas duas. O selo e o arquivo de denominadores
declaram, por classe:

```
situacao: "NAO PROVADA"
motivo: "n=83 < 299: o limite superior de Clopper-Pearson 95% com 0 falsos
         nao fica em 1% — a classe sai como NAO PROVADA, sem inflar o numero"
```

**O alvo de 300 é inalcançável nesta população** — e isso é medido, não
suposto: o gerador (`construir_holdout_web.py`, estrato `transcript`, corte
`2026-10-09T23:00:00Z`) recolhe as edições REAIS de `.ts/.tsx/.js/.mjs/.html`
nos transcripts, e a população inteira é a que está no `meta.json`: **83 HTML
e 21–23 JS** (o `--dry-run` de hoje já dá 23 no javascript — a população
cresceu 2 com as edições novas, e continua duas ordens de grandeza abaixo do
alvo). Não há mais 217 entradas para escrever: elas não existem.

## 4. O que ficou para a coordenadora decidir

1. **A concentração do estrato `transcript`** (55,4 % por arquivo no html,
   81,0 % por sessão no javascript) **não é um bug de seleção**: o estrato
   `transcript` é por construção **sem teto** (o próprio gerador: "no estrato
   transcript nao se aplica teto, e a concentracao maxima e REPORTADA"), e a
   população vem das edições da própria máquina — 10 arquivos, 8 sessões. O
   teto de 3 % só existe no estrato `git` (`--teto-concentracao`).
   Aplicar o teto aqui reduziria o corpus para poucos itens **sem** melhorar a
   prova (o n já está abaixo de 299 por falta de população, não de teto).
   **A alternativa honesta**: declarar o estrato `transcript` como uma
   *amostra de concentration conhecida* (o rótulo e de DELTA, e o n é pequeno
   demais para provar 1 %), e deixar a prova de 1 % para o estrato `oss`.
2. **A concentração da camada `oss` podada** (5,6 % e 6,3 % contra o teto de
   3 %) está **medida e declarada** no selo da própria camada (com os
   infratores nomeados). A correção é re-rotular com o teto aplicado na
   seleção (`rotular_oss.py --teto-concentracao 0.03`), o que exige os
   clones dos repositórios — o caminho fica sendo o da coordenadora.
   **Em curso**: a re-rotulação com o teto de 3 % **na seleção** está rodando
   (`/tmp/v8_oss_rota.sh` → `rotular_oss.py --alvo 300 --teto-concentracao 0.03
   --saida /tmp/oss_rotulado_20261031`, depois o selo externo e a conferência
   pelo `confere_selo.py`). **Nenhuma clonagem foi necessária**: os 7 clones já
   existiam em `~/.claude/touring/vgp/oss/` (excalidraw, zod, date-fns,
   html5-boilerplate, express, lodash, dom-examples) e o adjudicador congelado
   (`oss/oraculo_ls.js`) está lá — nada do workspace foi tocado.

   **O item que falta para o `html`**: o `rotular_oss.py` rotula hoje
   `javascript`/`tsx`/`typescript` — o **html** precisaria de um adjudicador
   que o script não tem (o de DELTA do `construir_holdout_web.py` lê os
   transcripts, não um clone). É o que fica em aberto para a coordenadora: ou um
   adjudicador de html para o estrato OSS, ou o html sai do "não provada" por
   decisão explícita.

## 4.1 O `html`: **NÃO PROVADA por decisão explícita** nesta rodada — **SUPERADO pelo D-8 (01/11)**

> **ATUALIZADO em 01/11**: a decisão do **Gabriel (D-8)** mandou **construir o adjudicador
> html**, o que substitui o «NÃO PROVADA por decisão» abaixo. O **plano** do adjudicador está em
> `PLANO-ADJUDICADOR-HTML.md` (na raiz do bundle), escrito **antes de qualquer código**, como a
> coordenadora pediu. O texto original da decisão de 31/10 fica abaixo, como registro.

**Decisão da coordenadora (31/10)**: o `html` fica **NÃO PROVADA por decisão
explícita** nesta rodada. Um adjudicador próprio para html é **trabalho novo**,
**fora do escopo do V-8** — e o escopo não é ampliado sem o Gabriel.

Três coisas que a decisão fixa:

1. **O n = 83** do estrato `transcript` (a classe html: 83 entradas, 10
   arquivos distintos, 8 sessões distintas; `atinge_alvo = false`; `n = 83 <
   299` → o limite superior de Clopper-Pearson 95 % com 0 falsos não fica em
   1 %). O javascript do mesmo estrato: n = 21.
2. **O motivo**: falta um **adjudicador OSS para html**. O adjudicador de DELTA
   do `construir_holdout_web.py` lê os **transcripts** — não os clones — e o
   `rotular_oss.py` (o adjudicador congelado do `oraculo_ls.js`) rotula hoje
   `javascript`/`tsx`/`typescript`, **não html**. Sem adjudicador, não há
   estrato OSS para html.
3. **A rota para fechar**: um **adjudicador html sobre `html5-boilerplate` e
   `dom-examples`** (os dois clones da ORDEM que têm html), registrado como
   **item NOVO a decidir** — a coordenadora leva o item à lista de pendências.

`tsx`, `typescript` e `javascript` seguem pela **re-rotulação em curso** (§4),
com o **mesmo critério**: classe com **299 negativos ou mais**, dentro do teto
e com o selo conferido, sai do "não provada"; a que não atingir segue
declarada como tal, com o n e o motivo.

## 5. A re-rotulação (continuação de 01/11) — o que a camada nova é, e como foi medida

> Esta seção é da **continuação** da sessão (a primeira parou com o run vazio). Ela traz: a causa
> raiz do run anterior, o que foi corrigido no rotulador, a **linha de base medida** da camada
> antiga, e o resultado da camada nova com o selo externo conferido.

### 5.1 A CAUSA RAIZ: o run anterior voltou vazio por um TRAVAMENTO, não por lentidão

O run do V-8 anterior (que a coordenadora reemitiu) não produziu saída. **Não era lentidão: era
um travamento silencioso**, e a causa está no leitor de frames do adjudicador:

| | |
|---|---|
| o que o adjudicador escreve | `#<n>` com `Buffer.byteLength(corpo, "utf8")` — **bytes** — e o corpo em UTF-8 (`oraculo_ls.js:233-236`) |
| o que o rotulador lia | `Popen(..., text=True)` e `stdout.read(n)` — que pede **n CARACTERES** (`rotular_oss.py`, classe `LS`) |
| o que acontece num corpo com acento | os caracteres são **menos** que os bytes declarados ⇒ o pipe esvaziava e o `read` bloqueava **para sempre**: node **VIVO** e ocioso (`ep_poll`), Python em `anon_pipe_read`, CPU ~0, **zero erro e zero saída** |
| o segundo defeito no mesmo lugar | o `oraculo_ls.js` escreve `${corpo}\n`; o `\n` ficava no stream e o `readline()` seguinte lia o terminador como se fosse o **cabeçalho** do frame seguinte |

**Diagnóstico que fecha**: o `wchar` do node (106 756 bytes escritos) com o **pipe vazio** e o
Python bloqueado ⇒ o produtor já escreveu tudo e o comprimento declarado é maior do que o frame
tem. Reproduzido em 5 linhas: `read(n)` em modo texto, num frame de 3 bytes com 2 caracteres,
**não recebe nunca o terceiro** (`select` fica `pronto=False`).

**Correção** (`rotular_oss.py` e `chama_oraculo.py`): pipe **binário** (`text=False`), leitura por
**BYTES**, o terminador `\n` consumido explicitamente, e o `ler_framing` comparando
`len(corpo.encode("utf-8"))` contra `n` (o `len(corpo)` em caracteres acusava «truncado» num frame
inteiro com acento).

**Prova**: `scripts/test_rotular_oss_framing.py` — **6/6**, e o **mutante em modo texto TRAVA**
(sem travar, o teste passaria sem provar nada). `rotular_oss.py --selftest` **92/92**,
`test_chama_oraculo.py` **15/15**, suíte do bundle **259 passed / 0 failed**.

### 5.2 A DISJUNCAO entre corporas — não existia, e sem ela a camada repetiria a anterior

MEDIDO: `estado_no_disco` lê **só** o diretório de saída e o `grava_entrada` **reescreve** o id
em vez de recusar o par ⇒ um par que um corpus anterior já rotulou seria rotulado **de novo**, e a
camada nova repetiria a antiga par a par. Um held-out que repete o holdout anterior não prova
nada: o juiz pode ter sido calibrado nos mesmos pares.

**Correção** (opt-in; o comportamento padrão fica intacto): a flag `--excluir-corpus`, que tira da
**população** os pares que um corpus anterior já rotulou, **antes** da seleção — então o par
excluído não consome oracle e **não entra no teto** (o teto mede o que foi ROTULADO). O `id` da
comparação é o mesmo do `grava_entrada` (`ident_do_par`), e a **política de exclusão fica no selo**.

O conjunto medido: **7 527 ids** no histórico de corporas (`~/.claude/touring/vgp`), incluindo os
446 da camada OSS antiga e os 924 do `corpus_semeado`.

**Prova**: `scripts/test_rotular_oss_exclusao.py` — **11/11**, incluindo «o par EXCLUIDO **não
chega ao adjudicador**» (o único jeito de provar a disjunção) e «sem a flag, **nenhum** par é
descartado» (controle negativo).

### 5.3 A LINHA DE BASE da camada antiga, medida com o método do selo

Medida por `scripts/selar_camada_oss.py` (o mesmo método do `rotular_oss.py`: teto = `3 % do n da
linguagem`, infração = `entradas por chave > teto`), sobre
`~/.claude/touring/vgp/corpus_holdout_web_20261012/oss/`:

| camada | n | negativos | chaves | maior chave | fração | teto medido | infrações | dentro do teto? |
|---|---:|---:|---:|---:|---:|---:|---:|---|
| `tsx` | 213 | 169 | 127 | 7 | 3,29 % | 6 | **1** | **NÃO** |
| `typescript` | 214 | 169 | 128 | 7 | 3,27 % | 6 | **1** | **NÃO** |
| `javascript` | 3 | 3 | 3 | 1 | 33,3 % | 1 | 0 | sim |
| `_podado_20261009-215331/tsx` | 180 | 148 | 112 | 10 | 5,56 % | 5 | **3** | **NÃO** |
| `_podado_20261009-215331/typescript` | 191 | 151 | 119 | 12 | 6,28 % | 5 | **1** | **NÃO** |

Todos os números do §2.2 deste relatório foram **confirmados** por esta medição independente
(n, chaves, maior chave e fração batem). **Uma coisa que o §2.2 não dizia e esta medição diz**: a
**camada principal também está acima do teto medido** (1 infração em `tsx` e 1 em `typescript`).
A causa é a poda: o selo da camada declara `n_por_lang` = {tsx 243, typescript 245}, e o disco tem
213/214 — a poda tirou 30 e 31 entradas e, com isso, o **teto caiu de 7 para 6**, deixando uma chave
com 7. **Declarar um teto e medir são coisas diferentes, e só a medição restringe** — é o que a
re-rotulação corrige, aplicando o teto **na seleção** e sem poda depois.

A camada antiga também é **DISJUNTA** do `corpus_semeado` (446 ids, verificado por id).

### 5.4 O selo externo da camada nova

`scripts/selar_camada_oss.py` — selo **fora** do corpus, no formato que o `confere_selo.py` exige
(`sha_tree`/`arquivos`/`bytes`), com os denominadores por classe, a **concentração medida**, o
**teto medido** e a **disjunção medida**. Conferido com
`confere_selo.py --corpus <dir> --selo <selo.json>` → **INTACTO** (rc 0).

O **denominador da prova é o dos NEGATIVOS**: o falso deny só existe sobre um negativo (um
positivo é um erro que o motor **deve** negar), e o `n` total quase dobra a força da prova sem
acrescentar um único falso deny possível. A camada antiga tem 169 negativos por classe — o `n`
total (213/214) inflaria a prova sem motivo.

### 5.5 As correções no rotulador — cinco defeitos, cada um com a causa medida

| # | defeito | sintoma medido | correção | prova |
|---|---|---|---|---|
| 1 | **framing lido em caracteres** — o `oraculo_ls.js` declara `#<n>` em **bytes** (`Buffer.byteLength`) e o rotulador lia `read(n)` em modo **texto** | num corpo com acento o pipe esvaziava e o `read` bloqueava **para sempre** (node vivo e ocioso, CPU ~0, zero saída) | pipe **binário**, leitura por **bytes**, e o `\n` terminador consumido | `test_rotular_oss_framing.py` **6/6** (o mutante em modo texto **trava**) |
| 2 | **sem disjunção entre corporas** — `estado_no_disco` lê só a saída e o `grava_entrada` **reescreve** o id | a camada nova repetiria a antiga par a par | `--excluir-corpus`: os pares já usados saem da **população** antes da seleção | `test_rotular_oss_exclusao.py` **11/11** |
| 3 | **a divergência parava o LOTE INTEIRO** | o `excalidraw` diverge em `manager.tsx` (o LS acusa `TS2307` de módulos que o `tsc` resolve) e o lote morreu com **218 entradas** — nenhum corpus | a divergência **exclui o par** (o item suspeito) e o lote continua; a divergência fica no selo | `test_rotular_oss_divergencia.py` **4/4** |
| 4 | **coleta com 3 spawns por par** | 17 670 pares ⇒ ~50 000 spawns; **1,5 h dos 2,4 h** de coleta eram de repos cujas linguagens já estavam cheias | pai e **data** no mesmo log (`%P`, `%ct`); conteúdo em **um** `git cat-file --batch` | `test_rotular_oss_coleta.py` **12/12** (byte a byte igual ao `git show`) |
| 5 | **o `datas` nunca era preenchido** — o `data_commit` era sempre `0` | a «ordem canônica» era **alfabética por caminho**, não pela data (o docstring prometia a data) | a data vem no log (`%ct`) | o mesmo teste, 12/12 |

**Dois achados que só apareceram porque a corrida foi observada**:

- **o adjudicador NÃO é o congelado**: o vivo é byte a byte igual à cópia versionada do bundle
  (sha `80976ec9…`, 10 580 bytes, mtime **09/10 23:39**), e a camada **antiga** foi feita com
  `8e8dd5f1…` (7 884 bytes). **As duas camadas não foram feitas pelo mesmo adjudicador**, e por
  isso os números delas não são comparáveis sem declarar isso.
- **o teto era por linguagem mas recarregado do disco a cada troca de linguagem**: com a ordem
  cronológica os pares alternam de `.ts`/`.tsx` a cada commit, e cada alternância devolvia os
  contadores a um estado que não tinha as entradas já rotuladas — efeito medido: `tsx` com **25
  entradas num arquivo** contra um teto de 3. Agora o `Counter` é por linguagem e atualizado no
  lugar (`test_rotular_oss_teto.py` **3/3**, com o mutante que só aparece com alternância).

E uma lição de operação que vale mais que o código: **dois rotuladores no mesmo `--saida`
corrompem a camada** — o teto é contado por processo e o arquivo é compartilhado. O sinal foi a
camada ter os **primeiros 12 índices repetindo caminho** e do 12 em diante não repetindo: dois
comportamentos na mesma camada. As camadas parciais foram guardadas ( `-parcial1a-divergencia`,
`-parcial2a-teto`, `-parcial3a-2processos`, `-parcial4a-alvo450`) e o lote final roda com
**um** processo, verificado.

### 5.6 A CAMADA NOVA: n por classe, concentração medida e hash do selo

**Comando** (reproduzível, um passo): `scripts/selar_e_conferir_camada_oss_20261101.sh`.

**Selo externo**: `~/.claude/touring/vgp/oss_rotulado_20261101.selo.json` — **fora** do corpus.
**Conferido com o validador atual**: `confere_selo.py --corpus … --selo …` → **INTACTO** (rc=0).

| | |
|---|---|
| `sha_tree` | **`e298fe3b6fe48d58`** |
| arquivos | 2 150 |
| bytes | 40 449 756 |
| ids na camada | 1 119 |
| **disjunção** | **DISJUNTA** — 0 ids em comum com o histórico de corporas (8 901 ids medidos em `~/.claude/touring/vgp`) |

**O veredito por classe** (critério: **299 negativos ou mais**, **dentro do teto**, **selo
conferido**):

| classe | n | **negativos** | taxa | teto por chave | dentro do teto | **situação** |
|---|---:|---:|---:|---:|---|---|
| **`typescript`** | 604 | **394** | 65 % | 18 | **sim** | **PROVADA** |
| `tsx` | 488 | 152 | 31 % | 14 | sim | **NÃO PROVADA** |
| `javascript` | 24 | 23 | 96 % | 1 | sim | **NÃO PROVADA** |
| `html` | 3 | 3 | — | 1 | sim | **NÃO PROVADA** |

**A concentração medida**, por classe (maior fatia da classe numa chave: arquivo e commit):

| classe | por arquivo | por commit | teto por chave | infratores |
|---|---:|---:|---:|---:|
| `typescript` | 2,6 % | 2,8 % | 18 | 0 |
| `tsx` | 2,9 % | 2,9 % | 14 | 0 |
| `javascript` | 4,2 % | 4,2 % | 1 | 0 |
| `html` | 33,3 % | 33,3 % | 1 | 0 |

O teto é `max(1, int(n_da_classe × 0,03))` **entradas por chave**: a `html` tem 1 entrada num
arquivo e o teto é 1 — dentro, mas com n = 3 a concentração percentual não significa nada, e é
por isso que a classe entra declarada como sem adjudicador. Nenhuma classe tem infrator.

**O veredito, classe a classe, com o motivo:**

1. **`typescript` — PROVADA.** 394 negativos >= 299, dentro do teto, selo conferido, camada
   disjunta. É a primeira classe do estrato OSS a sair de «não provada».
2. **`tsx` — NÃO PROVADA.** n = 152 negativos < 299. **O motivo é medido, não suposto**: a taxa
   de negativos desta classe é de **31 %** (a da `typescript` é 65 %), e a população elegível de
   `tsx` que sobra depois da disjunção é curta. Para os 299 negativos ela precisaria de cerca de
   **965** entradas (299 dividido por 0,31), e o lote foi interrompido em 488.
3. **`javascript` — NÃO PROVADA.** n = 23 negativos < 299. O motivo também é medido: os
   repositórios com `.js`/`.mjs` (express, dom-examples, html5-boilerplate) são os **últimos**
   da ordem canônica, e o `excalidraw` — que vem primeiro — quase não tem js. A taxa de
   negativos do que existe é de 96 %; falta **população**, não método.
4. **`html` — NÃO PROVADA, por um motivo de outra natureza:** o adjudicador desta camada é o do
   **TypeScript** (`oraculo_ls.js`), e um arquivo `.html` **não produz diagnóstico nenhum** — o
   par sai «negativo» com o rótulo **vazio de conteúdo**, e um negativo vazio não mede falso
   deny. A classe é declarada assim no selo (`classes_sem_adjudicador: ["html"]`) e o **n dela
   não entra em nenhuma prova**. O html tem adjudicador próprio: é o **D-8**, com o plano em
   `PLANO-ADJUDICADOR-HTML.md` (escrito antes de qualquer código).

**O que a camada nova é, em uma frase**: 1 119 entradas rotuladas por DELTA (o adjudicador e o
`tsc` concordam em cada retestamento), **disjuntas** de 8 901 ids de corporas anteriores, com o
teto de 3 % aplicado **na seleção** e não só medido depois, e **uma classe provada**.

**O que ficou para continuar** (o comando está no disco, e a camada é selável a qualquer
momento): relançar o lote com um alvo maior para levar `tsx` e `javascript` aos 299 negativos.
O `tsx` precisa de mais **~480** entradas (488 → ~965) e o `javascript` dos repositórios de js,
que vêm no fim da ordem — os dois exigiriam um alvo total de **~1 700**, acima do `--alvo 1200`
que este lote usava. Nenhum dos dois depende de mudar o método: só de tempo de adjudicador e de
alvo.

## 6. Regras (do briefing, em vigor)

- Nada foi mergeado no branch principal; nada foi sobrescrito nem apagado: o
  corpus de disco (`corpus_holdout_web_20261012`) e o `oss/selo.json` continuam
  como estavam, e o selo novo **mora fora** do corpus.
- hw-slot sempre; commit só dos meus paths.
- Uma classe sem n suficiente sai como **não provada** — sem inflar o número.
