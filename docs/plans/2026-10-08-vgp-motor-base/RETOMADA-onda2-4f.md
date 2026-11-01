# RETOMADA — touring-4f, onda 2 dos corpora VGP

> **Sessão:** `touring-4f` (MiniMax-M3.1-Flash-Preview) · **Data:** 09→10/10/2026
> **Mandada por:** `touring-09` · **Onda 1:** ACEITA (sha `f2f0f33c6656b686a118`)

## O que está em disco AGORA (não depende de resumo)

| script | estado | gate |
|---|---|---|
| `scripts/corpus_semeado.py` | onda 1 + correções | selftest **37/37** |
| `scripts/corpus_rust.py` | estrato `sanidade` (crate sintético) | selftest **13/13**, 80 positivos, determinismo IDENTICO |
| `scripts/corpus_rust_real.py` | estrato `real` (projeto real) | gates verdes, **ainda não rodado** (precisa do `hw-slot`) |
| `scripts/rotular_oss.py` | estrato `oss` da tarefa B | selftest **40/40**, `pyright` 0 |
| `scripts/oraculo_ls.js` | LanguageService, campo `base` adicionado | `node --check` OK |
| `~/.claude/touring/vgp/scratch/*.py` | verificação de sobreposição, controle positivo, prova de equivalência | — |

## Tarefas

| tarefa | estado |
|---|---|
| **C (JS por arquivo)** | **FECHADA** — 80/80 `classe_erro → simbolo`, zero `nao_classificado` |
| **A (Rust)** | `sanidade` FECHADO (80, crate sintético). `real` aceito pela coordenadora, **gerador escrito, não rodado** |
| **B (holdout)** | selo local selado (83 html + 21 js). Estrato `oss` **em re-rotulagem** — os 278 anteriores foram invalidados |

## O INVALIDA e por que (leia antes de confiar em qualquer número de `oss`)

A prova de equivalência LS×`tsc` em JS deu 8 coincidiram / 35 divergiram com `LS=[]` onde o
`tsc` acusava. A causa **não** era o oráculo:

```
tsc: 166 erros | LS: 166 | so no tsc: 0 | so no LS: 0
```

O `rotular_oss.py` pedia `lsc.delta(caminho, depois)` com o arquivo do clone no estado **HEAD**,
e o HEAD de hoje é muito posterior ao commit do par — o `antes` do par é um conteúdo que aquele
disco nunca teve. O DELTA rotulado era `delta(HEAD → depois)`, não `delta(antes → depois)`.
**Os 278 pares gravados antes da correção são inválidos** e foram descartados sem contar.

## Correções já no código (cada uma tem cenário de selftest)

1. `delta_do_par` — o disco vai para o `antes` antes do `lsc.delta`; restauração **verificada por
   sha**; par A **apaga** o que criou e confirma que sumiu.
2. `exige_clone_limpo` — conta ` M`, ` D` e `??`; `??` fora da allowlist
   (`ARTEFATOS_DE_INSTALACAO`) **recusa**; restaura por `git show HEAD:<path>`, nunca
   `git checkout --`.
3. `m_ancora` (no `corpus_semeado.py`) — trocava o `id` em vez do `href`/`for`.
4. `delta` do LS não checava `ok` — oráculo recusado virava "nenhum diagnóstico novo" = negativo.
5. `delta_do_tsc` só parseiava `arquivo(linha,col)`; o zod emite `arquivo:linha:col - ` e as 598
   linhas dele eram lidas como zero. Dois formatos agora.
6. Teto de concentração: `3% do n da linguagem`, recalculado por par (era `3% do alvo`, depois
   `3% de len(pares)` — nenhum dos dois era o denominador certo).
7. `grava_entrada` é idempotente (com `append` reexecutar duplicava linha).
8. `snap_antes` + `operacao: "M"|"A"` em toda entrada; contrato no meta do selo.
9. Descarte por `TS2307`/`TS2305`/`TS7016` de dependência ausente na base (decisão da coordenadora).
10. `exclude` de sintaxe inválida de propósito no `dom-examples` — o `tsc` foi de **3 linhas
    (parado) para 1505 erros**.

## Jobs em curso (quando esta gravação foi feita, ~00:20)

| job | o quê |
|---|---|
| `byohqvy32` | determinismo das linguagens web — 41 min de timeout 50 |
| `/tmp/rotulador_v2.json` | re-rotulagem dos 7 repos com delta contra `antes` |

## Comandos

```bash
B=/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base
O=~/.claude/touring/vgp/oss

# selftests (todos rc=0)
python3 -I $B/scripts/corpus_semeado.py --selftest
python3 -I $B/scripts/corpus_rust.py --selftest
python3 -I $B/scripts/rotular_oss.py --selftest

# determinismo web (2 gerações × 4 linguagens, lento)
VGP_SEMEADO_TMP=$HOME/.cache/vgp-semeado python3 -I $B/scripts/corpus_semeado.py --confere-determinismo

# re-rotulagem OSS (o selo é gravado no fim e conferido na releitura)
python3 -I $B/scripts/rotular_oss.py --max-commits 200 --alvo 300

# auditoria: concentração, selo e sobreposição com o semeado
python3 -I ~/.claude/touring/vgp/scratch/auditoria_oss.py

# Rust real: PRECISA do hw-slot; não forçar
cd /home/gabrielgadea/projects/touring-wt-semeado
hw-slot --class rust -- python3 -I $B/scripts/corpus_rust_real.py --selftest
hw-slot --class rust -- python3 -I $B/scripts/corpus_rust_real.py --n 20

# provas de equivalência (as duas, com as rotas medidas igual)
python3 -I ~/.claude/touring/vgp/scratch/prova_equivalencia_js.py

# controle positivo (LanguageService, o mesmo do rotulador)
python3 -I ~/.claude/touring/vgp/scratch/controle_positivo.py
python3 -I ~/.claude/touring/vgp/scratch/controle_positivo_html.py
```

## Requisitos de proofa que a coordenadora fixou (ainda pendentes)

1. **Refazer as DUAS provas de equivalência** (TS/TSX e JS) com as duas rotas medidas de forma
   idêntica (antes no disco → delta do depois), exigindo **≥ 10 pares de delta não vazio e ≥ 10
   vazios por linguagem**, e reportando por rota quantos deram não vazio. Sem caso não vazio a
   prova não prova nada — o 20/20 antigo é suspeito pela mesma assimetria.
2. **Contagem M/A com denominador** na re-rotulagem.
3. **Concentração por arquivo** por estrato (`real` × `sanidade`).

## Não verificado / não fazer

- Não medi falso deny (o holdout é selado, não rodo motor nem juiz nele).
- Não toquei em `juiz_*.py` — é da coordenadora.
- Nenhum commit (proibido pelo brief).
- Nenhum `crates/**` da árvore principal tocado.
---

## Evidência G1 replay (dano real no gate, 09→10/10/2026)

O gate de rajada G1 reescreve a chamada num `touring run` que **reexecuta comandos antigos**. Duas
vezes isso me devolveu o conteúdo do arquivo errado e eu editei em cima da leitura errada.

**Caso 1 — o replay trouxe o corpo de outro arquivo numa leitura**

- hora: `2026-10-10T00:08:07.022Z`
- o que eu digitei: `sed -n '/^def main/,/^def /p' prova_equivalencia_js.py`
- o que o hook devolveu: o corpo de `rotular_oss.py` (apareceram `exige_clone_limpo`,
  `ARTEFATOS_DE_INSTALACAO`, `cenario_antes_e_operacao` — símbolos que **não existem** em
  `prova_equivalencia_js.py`), precedido de `[G7 re-inspeção] 4ª leitura ... numere a rajada`
- o que eu editei em cima disso: a função `main` da prova de equivalência, com o corpo do outro
  arquivo como contexto. Só percebi quando o `pyright` apontou linha 447 que não batia.

**Caso 2 — o replay executou comandos de outra sessão**

- hora: ~`2026-10-10T00:12Z`
- o que eu digitei: um `sed -n` no `prova_equivalencia_js.py`
- o que o hook devolveu: `[G10 exec-burst] 5ª chamada seriada ...` com
  `# já executado (não repetir): ... /home/gabrielgadea/.cache/vgp-semeado/mede_teto_holdout.py`,
  `scripts/construir_holdout_web.py`, `prova_equivalencia_ls.py` — comandos meus de **rodadas
  anteriores**, nunca executados nesta janela.
- contagem no transcript da sessão: **10 ocorrências** do padrão de replay.

**Efeito no trabalho:** a correção de `tuple`/`list` e de truncamento na prova de equivalência foi
aplicada duas vezes sobre uma leitura contaminada, e a prova rodou com código velho sem eu
perceber — o número que reportei ("11 coincidiram / 32 divergiram") era de um script que eu
achava ter corrigido.

**Regra que estou seguindo até o 30.4.96:** leitura que vai embasar uma edição sai por
`touring run --lang bash --code 'sed -n …'` explícito, ou pela ferramenta de leitura, **nunca** por
`sed`/`cat` em rajada.


---

## Protocolo do oraculo LS: framing por comprimento declarado (10/10)

**O bug**: com `text=True, bufsize=1`, o `readline()` do Python le no maximo **65.536 bytes**.
Uma resposta de 77 KB (um `delta` com 144 diagnosticos) chegava ao `json.loads` como linha sem
fim: `JSONDecodeError: Unterminated string starting at: line 1 column 65455`. O par era marcado
como divergente e o `LS=[]` aparecia onde o `tsc` acusava 145 -- eu nunca recebia a lista.

**O conserto**: o `oraculo_ls.js` prefixa `#<n bytes>` e o leitor consome **exatamente n bytes**,
sem depender de buffer nem de newline, e **confere o comprimento declarado** (erro nomeado
`truncado: esperava N, veio M`). Compatibilidade: resposta sem `#` continua valendo como linha.

**Selftest** (`~/.claude/touring/vgp/scratch/selftest_protocolo_ls.py`, **5/5**):
- (a) resposta de **1.411.923 bytes** lida INTEIRA, bytes identicos ao que o oraculo escreveu
- (b) MUTANTE: o `readline()` antigo REPROVA na resposta grande (`JSONDecodeError`)
- (c) comprimento declarado maior que o enviado vira erro NOMEADO, nunca lista vazia
  (`truncado: esperava 1411923, veio 65527`)

**Consumidores do protocolo (todos corrigidos)**: `scripts/rotular_oss.py` (classe `LS`),
`scratch/prova_equivalencia_js.py` (classe `LS`), `scratch/controle_positivo.py` (classe
`Oraculo`). O JS atualizado esta copiado em `~/.claude/touring/vgp/oss/oraculo_ls.js`.

**Geracoes do estrato `oss` que foram INVALIDADAS** (nao confiar nos numeros delas):
1. a geracao com `delta(HEAD -> depois)` (o `antes` do par nunca esteve no disco);
2. a geracao com o protocolo truncando em 65 KB;
3. o lote `rotulador_v2`, que **PAROU por divergencia no retestemunho** em
   `excalidraw/packages/element/tests/zindex.test.tsx` (comportamento exigido: parar o lote).

O `rotulador_v3` (protocolo corrigido) estava em curso quando esta gravacao foi feita.

## Os cinco "o sinal existiu e nao chegou"

| # | onde o sinal se perdeu | sintoma observado |
|---|---|---|
| 1 | `ok` do `delta` do LS nao checado | oraculo RECUSADO virava "nenhum diagnostico novo" = negativo |
| 2 | 2o formato de saida do `tsc` (`arquivo:linha:col - `) | 598 erros lidos como ZERO no zod |
| 3 | `stderr` do `cargo` (o `stdout` fica vazio) | 14 receitas Rust dando "delta de 0" |
| 4 | buffer de 65.536 do `readline` | `LS=[]` onde o `tsc` acusava 145 |
| 5 | denominador do teto (`alvo`, depois `len(pares)`) | 9,3% e depois 9,8% num teto de 3% |

Nenhum deles era o oraculo errado. Todos sao o sinal certo chegando por um caminho que o parser
nao olhava -- e cada um tem cenario de selftest que reprova se a protecao sair.

## Jobs em curso na gravacao

| job | arquivo de saida |
|---|---|
| determinismo por linguagem | `/tmp/determinismo_v2.txt` |
| rotulador v3 (protocolo corrigido) | `/tmp/rotulador_v3.json` / `.err` |
| selftest do Rust real (na fila do `hw-slot`) | `/tmp/rust_real_v2.txt` |
| prova de equivalencia JS -- precisa ser RELANCADA com o protocolo corrigido | `/tmp/prova_js_v6.txt` |


---

## CAUSA-RAIZ da parada do `rotulador_v3` (medida em 10/10)

**Sintoma**: o lote parou no retestemunho em
`excalidraw/packages/element/tests/zindex.test.tsx`, com `LS: [('TS2345', 119, ...)]` e
`tsc: []`.

**As quatro medições, na ordem em que foram feitas** (nenhuma conclusão veio antes da medição):

| # | pergunta | resposta medida |
|---|---|---|
| 1 | o `tsc` ABORTA? | nao: `tsc -p . --noEmit` → EXIT=0, **zero linhas** |
| 2 | o alvo esta no programa? | sim: `--listFiles` → **1430 arquivos**, alvo presente |
| 3 | e cache incremental? | nao: `incremental` nao declarado, **nenhum** `*.tsbuildinfo`, `--incremental false` da o mesmo zero |
| 4 | o que o LS ve no par exato? | `delta(depois vs disco=depois)` → novos=0, base=0 · `delta(antes vs disco=antes)` → **base=1 com TS2345 na 119** |

**ACHADO**: o `TS2345` da linha 119 e erro **PRE-EXISTENTE do `antes`**; a mutacao o **REMOVE**.
O par e um **NEGATIVO** (a edicao conserta o arquivo) — por isso o `tsc` acusa zero no `depois`.
Os dois oraculos concordam.

**Estabilidade medida**: 3 rodadas no mesmo processo `node`, incluindo ordem invertida →
`novos=[]` sempre, `base=[('TS2345',119)]` sempre. O LS nao tem estado nesta cena.

**CAUSA**: o `oraculo_ls.js` foi reescrito (framing `#<n bytes>` + campo `base`) **enquanto o v3
rodava**. O `node` le o script UMA vez no boot → o processo respondeu com a versao ANTIGA, e o
selo declarava um oraculo que nao foi o que respondeu.

**SELO v3 INVALIDADO** (`selo_v3_INVALIDADO-oraculo-trocado.json`). Os 99 rotulos do v3
(52 ts / 47 tsx) NAO contam como aceitos ate o v4 fechar.

**Correcao** (`scripts/rotular_oss.py`):
- `identidades_do_oraculo()` → sha256 + bytes do `oraculo_ls.js` e a versao do `tsc`, em
  `identidade_do_oraculo` dentro do selo.
- `main()` tira a identidade no inicio, reconfere no fim; se mudou, `ORACULO MUDOU NO MEIO DO
  LOTE` e o selo e INVALIDADO (entra em `paradas`).
- `confere_oraculo(antes, agora)` nomeia o que mudou, campo a campo.
- selftest: **47/47**, incluindo o **mutante** (`confere_oraculo` com corpo `return ""`, que
  precisa reprovar).
- `ruff format --check` OK · `ruff check` **All checks passed** (corrigi os 3 pre-existentes:
  BLE001 + S110 em `LS.fecha()`, ISC004 no cenario de formatos) · `pyright` **0 errors**.

**Jobs na gravacao**: rotulador v4 (PID 2059943, excalidraw,zod) · prova de equivalencia JS v6
(PID 1952862) · selftest do Rust `real` na fila do `hw-slot` (93 °C, nao forcado).

## Medidas que DESCARTARAM hipotese (registrar para nao repetir)

- "o `tsc` aborta no excalidraw" → falso, EXIT=0 com 1430 arquivos listados.
- "o `tsconfig` raiz nao cobre `packages/element/tests`" → falso, `include: ["packages",
  "excalidraw-app"]` cobre e `--listFiles` confirma.
- "o LS tem estado/cache entre pares" → falso, 3 rodadas com ordem invertida, resultado identico.
- "e o alvo nao e arquivo, e um diretorio de testes ignorado" → falso, e um arquivo `.tsx` de 48.6 KB.


---

## O v5 mediu um arquivo que OUTRO PROCESSO estava reescrevendo (10/10)

O `rotulador v5` parou em `linearElementEditor.ts` com `LS: [TS2305 51, TS2554 391, TS2554 594,
TS2554 2335, TS2554 2374]` e `tsc: [TS2554 2335, TS2554 2374]`. Reproduzi o par exato
(`a70417f23f`, pai `1c8e8bb0f3`) e **os dois oraculos concordam em tudo**:

| | `tsc` TS2554 | LS TS2554 base | LS TS2554 novos | TS2305 |
|---|---|---|---|---|
| `antes` | 374, 412, 552, 628, 882, 914 | **exatamente as 6** | 0 | nenhum |
| `depois` | 377, 557, 888, 920, 2253, 2292 | **exatamente as 6** | 0 | 52 nos DOIS |

`binding.ts` do clone NAO tem `snapToMid`, e o `tsc` acusa `TS2305` na linha 52 do `depois` e
nenhum no `antes`. O par e um NEGATIVO (a mutacao conserta) e o retestemunho deveria ter passado.

**CAUSA**: o v5 rodou **enquanto** o `probe_binding.py` / `probe_binding_tsc.py` reescreviam o
MESMO `linearElementEditor.ts` no clone. E a MESMA corrida que fez a prova JS devolver
`0 arquivo(s)` para express e lodash: o `rotular_oss.py` e os probes escrevem os mesmos arquivos
dos clones, e nenhum dos dois tem exclusao mutua.

**REGRA ADOTADA**: nada que escreva em `~/.claude/touring/vgp/oss/` roda junto com o rotulador.
O rotulador e sequencial por natureza (ele reescreve cada arquivo duas vezes por par); os probes
escolhem arquivos e nao podem conviver com ele.

## Terceiro denominador errado: o teto por LINGUAGEM (v5)

`concentra_dentro_do_teto` acusou `javascript packages/excalidraw/types.ts: 7 entradas (233.33%)`.
`javascript` tem **3** entradas no total, e `types.ts` tem **7** em **typescript**. Os contadores
eram GLOBAIS e o n era por linguagem. `estado_no_disco` agora devolve `cont_arq`/`cont_commit`
**por linguagem**, e o laco de `rotula_repo` troca os contadores quando muda de linguagem.

E o quarto denominador: no `v4` os contadores do teto nasciam em **ZERO** a cada execucao (a
idempotencia dos ids funcionava), entao um lote retomado media o teto contra um n que nao incluía o
ja gravado — `types.ts` fechou com 12 (6,28%) contra um teto final de 5.

**Sequencia dos quatro denominadores**, todos meus, todos medidos: `alvo` (300) -> `len(pares)`
(1217, todas as linguagens) -> n do lote **parcial** -> n por linguagem **sem** o ja gravado.
O certo e o n por linguagem **com** o ja gravado, e o selo agora **mede** a concentracao contra o
disco em vez de so declara-la (um selo que declara uma restricao sem medir nao restringe nada).

## Selo: concentracao medida + identidade do oraculo

`selo.json` ganhou `identidade_do_oraculo` (sha256 + bytes do `oraculo_ls.js`, versao do `tsc` de
cada repo) e `concentracao_medida` (`dentro_do_teto` + a lista de infratores com arquivo, n,
fracao e teto). `main` devolve **1** quando ha infrator, e `main` tambem invalidaria o selo se o
oraculo mudasse no meio do lote.

Portas: **selftest 60/60** · `ruff format --check` OK · `ruff check` **All checks passed** ·
`pyright` **0 errors**. Cenarios novos com mutante: `cenario_oraculo_congelado` (7),
`cenario_teto_entre_execucoes` (8), `cenario_concentracao_medida` (6).

## Poda do lote v4 (com backup)

`/tmp/vgp-podar-teto.py` removeu o excedente do teto em cada linguagem, iterando (o teto depende
do n, que muda quando a entrada sai): `tsx` n=180->170 (removeu 10), `typescript` n=191->184
(removeu 7). Backup em `oss/_podado_20261009-215331/`. Conferido: `typescript` max 5 (2,72%),
`tsx` max 5 (2,94%), `javascript` max 1.

## Selo v5 tambem INVALIDADO

`selo_v5_INVALIDADO` nao foi gravado com nome, mas o lote v5 parou por uma divergencia que nao
reproduz, entao seus numeros nao contam: `tsx` 243 / `typescript` 245 sao de um lote medido contra
um arquivo que outro processo mexia.


---

## ORDEM DA COORDENADORA (10/10, apos os achados da corrida) — ESTE E O CAMINHO

1. **SIGTERM no v6** (PID 2248984) e confirmacao de que saiu. **FEITO**: `v6 PARADO`.
2. Clone limpo (`git status --porcelain` so com a allowlist).
3. **Lock + selecao canonica** (commit date, caminho, id), cega ao rotulo, com selftest e mutantes.
4. **v7 do zero num diretorio novo — ESTE E O CORPUS.**
5. Concordancia de rotulo na intersecao **v6 ∩ v7**, por `id`: 100% (positivo/negativo + codigos
   novos). Qualquer divergencia e investigada ANTES do selo.
6. Selo.

O v6 nao serve como conjunto (a regra de selecao mudou, entao o `sha_tree` difere por
construcao). Serve como **referencia de ROTULO** para o passo (5).

## Aviso recebido: palavras corrompidas nas minhas mensagens

A coordenadora观察到 «naoPilotou», «loteLatinhas» e «Selection-on-the-trot» nas minhas mensagens
— contexto longo. **Regra desta sessao**: toda mensagem para a coordenadora vai por um `touring
run` que valida o texto antes de sair, e o estado em disco e gravado ANTES de qualquer trabalho
longo. Vou reler minhas mensagens anteriores antes de reaproveitar qualquer frase delas.

## O lock ja esta escrito (passo 3, parte a)

`scripts/vgp_lock.py`: `adquire()` / `devolve()` / `escreve_exclusivo()` / `esta_travado()`.
`fcntl.flock` **bloqueante**, fd com `O_CLOEXEC`.

Por que `O_CLOEXEC` e obrigatorio aqui (e nao por higiene): o `oraculo_ls.js` e um processo `node`
de **vida longa dentro do rotulador**. Um fd de lock herdado por ele continuaria segurando o
exclusive depois do rotulador morrer, e o proximo escritor esperaria por um processo que ja saiu.

Sem timeout, de proposito: um timeout no lock seria o **mesmo «zero sem avisar»** — o escritor mais
lento seria acusado de nao ter escrito.

`scripts/test_vgp_lock.py` cobre o exigido: (a) dois processos, o 2o ESPERA e a ordem no arquivo e
serializada; (b) **mutante sem lock** onde o 2o nao espera e a mesma comparacao com lock esperando;
(c) fd nao herdavel por um filho, lido de `/proc/self/fd`; mais o dono que MORRE e o proximo entrar.

**Falta para o passo 3**: a regra de selecao canonica dentro do `rotular_oss.py`.

## A poda ad hoc foi selection-on-the-trot (erro meu, confirmado)

`/tmp/vgp-podar-teto.py` mantinha as primeiras `teto` ocorrencias **na ordem do arquivo**, que e a
ordem de gravitacao do rotulador: se o lote parou cedo, a poda mantem exatamente o que o lote fez
primeiro. Nao e cega ao rotulo e nao e deterministica entre execucoes. Por isso o backup
`oss/_podado_20261009-215331/` fica, e a remocao dos backups e decisao do Gabriel.


---

## PASSOS 1-4 CONCLUIDOS (10/10, depois da ordem da coordenadora)

**(1) v6 PARADO**: SIGTERM no PID 2248984, confirmado.

**(2) Clone limpo**: havia residuo ` M packages/excalidraw/components/Stats/index.tsx` no
excalidraw (o v6 morreu no meio de um par). Restaurado por `git show HEAD:<path>` + conferido com
`git diff --quiet` — **nunca `git checkout --`**. Estado final: os 7 clones com apenas `M tsconfig.json`
e `?? .yarnrc`, que sao os artefatos de instalacao da allowlist. Os outros 6 LIMPOS.

**(3) Lock** — `scripts/vgp_lock.py` + `scripts/test_vgp_lock.py`, **6/6**:
| cenario | medido |
|---|---|
| o 2o processo ESPEROU | 2,12 s (o 1o dorme 2,0 s) |
| ordem de instante serializada | `tomei/soltei/tomei/soltei` |
| **MUTANTE sem lock: o 2o NAO espera** | 0,16 s |
| com lock, o mesmo par espera | 2,12 s |
| fd NAO herdavel por um filho (lido de `/proc/self/fd`) | ok |
| lock liberado quando o dono MORRE | ok |

`O_CLOEXEC` porque o `oraculo_ls.js` e um `node` de **vida longa dentro do rotulador**; um fd
herdado seguraria o exclusive depois do rotulador morrer. Sem timeout, de proposito: timeout no
lock seria o mesmo "zero sem avisar".

**Ligado no rotulador**: `main` faz `with vgp_lock.escreve_exclusivo(): _rotula_todos(...)` e a
conferencia de oraculo + selo + veredito de teto ficam DEPOIS do `with` (`_selo_e_veredito`), porque
a identidade do oraculo e a concentracao sao do estado do disco DEPOIS do lote.

**Selecao canonica** — `chave_canonica(par) = (data_commit, caminho, id)`, `ordem_canonica` ordena
totalmente. Cega ao rotulo por construcao: a chave nao tem `classe`, `rotulo` nem `novos`.
`pares_do_repo` agora le a data com `git show -s --format=%ct` por commit, cacheado em `datas`.
A baralha com `random` saiu; `SEMENTE` continua no selo como procedencia do retestemunho.

**Portas**: selftest do rotulador **60/60** · selftest do lock **6/6** · `ruff check` All checks
passed · `pyright` **0 errors**.

**(4) v7 DO ZERO**: `~/.claude/touring/vgp/corpus_holdout_v7_zero`, diretorio NOVO e vazio (o `rm -rf`
foi bloqueado pelo gate e eu nao insisti — nao havia nada a apagar). PID 2355301, `--repos
excalidraw,zod`. **Este e o corpus.**

## Dois erros meus nesta passagem, ambos de teste

1. O selftest do lock gravava nos **mesmos** dois arquivos pelos dois filhos, e o segundo sobrescrevia
   o primeiro: a ordem observada virava lixo e o teste reprovava com a ordem CORRETA. Agora cada
   filho tem seu arquivo e a ordem e lida pelo **instante**.
2. O mesmo teste comparava o verbo `"tomei"` contra a string escrita `"tomei o lock"` — um teste que
   reprova pelo motivo errado e pior que um teste que nao existe.

## Rust `real`: selftest em 2/16, causa nao medida ainda

As 14 falhas dizem `codigo 5`, `codigo 47`, `codigo 71`, `codigo 79`, `codigo 86` — numeros que nao
sao codigos `E0xxx`. O `motivo_da_recusa` compara `cod` com `CODIGO_PRIMARIO[classe]`, entao `cod`
esta sendo extraido do grupo errado, OU o `cargo` emite um formato que o regex nao casa. Probe
pronto em `scratch/rust_formato_real.py` (imprime a linha BRUTA e cada grupo). **Nao roda ainda**:
o `hw-slot` `rust-1` esta com outro processo e a maquina esta a 94-96 °C. Nao forço.


---

## `tsconfig` NA IDENTIDADE DO ORACULO (exigencia da coordenadora, 10/10) — IMPLEMENTADO

O rotulador reescreve o `tsconfig.json` RASTREADO de cada clone, e o programa que o
LanguageService enxerga depende disso. `tsconfigs_do_oraculo()` devolve por repo:
`sha_head` (`git show HEAD:tsconfig.json`), `sha_usado` (o arquivo em disco), `alterado` e o
`git diff` **inteiro** (sem truncamento — um selo que corta o diff no meio esconde a mudanca).

**Medido agora, 7 clones**:

| repo | sha_head | sha_usado | alterado | diff |
|---|---|---|---|---|
| date-fns | 29c2f88d21cd | 29c2f88d21cd | **nao** | 0 |
| dom-examples | (sem HEAD) | 04cfe3b9a954 | sim | 0 (criado) |
| excalidraw | f80d226b52cc | 222ee8316718 | **sim** | **3713 chars** |
| express | (sem HEAD) | d1368fc7febd | sim | 0 (criado) |
| html5-boilerplate | (sem HEAD) | d1368fc7febd | sim | 0 (criado) |
| lodash | (sem HEAD) | d1368fc7febd | sim | 0 (criado) |
| zod | 79ff8445180c | e4ce48251037 | **sim** | 1042 chars |

**6 de 7 tem o tsconfig alterado.** O `excalidraw` tem `strict: true` no HEAD e o diff reescreve
os 85 campos — muda o programa inteiro. Sem isso na identidade, o selo nomearia o `oraculo_ls.js` e
a versao do `tsc` e calaria sobre a **configuracao** com que os dois leram o repo.

`confere_oraculo` compara o `tsconfigs` campo a campo (um dicionario de dicionarios nao serve para
`!=`), entao um `tsconfig` reescrito no meio do lote tambem invalida o selo.

Portas: **60/60** · ruff All checks passed · pyright **0 errors**.

## REGRA DE HW-SLOT (nova, da coordenadora, 10/10)

Trabalho pesado vai SEMPRE por `hw-slot`. Dois medidos fora do slot nesta maquina: dois
`tsc --noEmit -p .` do analise/frontend a ~200% de CPU cada e o `oraculo_ls.js` a 112%, com a
maquina a 90 °C.

Da proxima vez:
- determinismo: `scripts/hw-slot --label 4f-determinismo --wait 3600 -- python3 -I scripts/corpus_semeado.py --confere-determinismo --lang <L>`
- rotulador: `scripts/hw-slot --label 4f-rotulador -- python3 -I scripts/rotular_oss.py ...`

Sao **2 slots heavy** no maximo da maquina. O v7 em curso pode terminar como esta; nao o matei.

## PASSO (5) pronto: `scratch/concordancia_v6_v7.py`

Compara, na intersecao por `id`, a assinatura `(rotulo, classe, codigos_novos)`. Imprime a
distribuicao de rotulos da intersecao (e avisa se ela nao tem NENHUM positivo, porque ai a
concordancia mede so os negativos), lista os divergentes com path/repo/commit, e lista os ids
so num dos lados — que e a diferenca de conjunto, nao divergencia de rotulo.


---

## PASSO (0) — DIFF SEMANTICO do `tsconfig` (10/10): o caso e ACEITO

**v7 PARADO** (PID 2355301, SIGTERM) por causa do diff de 3713 chars do excalidraw.

Metodo: `tsc -p . --showConfig` (o JSON EFETIVO, ja com `extends` resolvido), HEAD e USADO
medidos **NO MESMO DIRETORIO** trocando so o arquivo — `include` e relativo ao diretorio do
tsconfig, entao medir o HEAD num diretorio separado compararia dois programas diferentes.

### Resultado por repo

| repo | campos HEAD | campos USADO | so no USADO | valor diferente | `compilerOptions` fora de `exclude` |
|---|---|---|---|---|---|
| excalidraw | 720 | 722 | **2** | **0** | **0** |
| zod | 516 | 518 | **2** | **0** | **0** |
| date-fns | 5 | 5 | 0 | 0 | **0** |

Os 2 campos acrescentados, **identicos nos dois**, sao:
`exclude[4] = web-workers/worker-playground/bad-syntax-worker.js` e
`exclude[5] = **/bad-syntax*.js`.

**VEREDITO: so `exclude`, nenhum `compilerOptions` alterado. ACEITO** — o oraculo usou a
configuracao DO PROJETO mais o `exclude` minimo que o motivou.

Os 4 sem `tsconfig` (express, lodash, html5-boilerplate, dom-examples): o criado e necessario
(sem ele o `oraculo_ls.js` nao monta programa). Opcoes declaradas:
`target ES2022` · `module ESNext` · `moduleResolution bundler` · `allowJs` · `checkJs` ·
`noEmit` · `skipLibCheck` · `allowImportingTsExtensions` · `strict: false` ·
`include ["**/*.js", "**/*.mjs"]`.
O `exclude` acrescentado e **inofensivo** em 3 deles (nenhum tem arquivo `bad-syntax*`); so o
`dom-examples` tem `web-workers/worker-playground/bad-syntax-worker.js`.

## TRES erros meus nesta medicao (todos de probe, nenhum do produto)

1. **`subprocess.run` sem `text=True`** — `json.loads` recebia bytes e eu escrevi «nao produziu
   `--showConfig`». Ele produzia `rc=0` com **41.899 bytes**. Concluir «nao produziu» sem medir foi
   o MESMO defeito que cercou a onda inteira, uma vez mais.
2. **`achata` ignorava o retorno das chamadas recursivas** e devolvia `{}` — os tres repos deram
   «0 campos», que eu quase li como «as configs so diferem no exclude». Um achatador que engole a
   propria saida produz 0 campos e PARECE uma comparacao limpa: e a forma mais silenciosa de um
   medidor mentir.
3. **HEAD medido num diretorio separado** — `include` relativo ao diretorio do tsconfig, e o
   `tsc` respondeu `TS18003` (sem entradas) e `TS5083` (`extends` quebrado). Eu estava comparando
   dois programas diferentes, que e o defeito que ia medir.

## Proximo passo apos o aval da coordenadora

(a) gravar o `tsconfig` **minimo**: HEAD + so o `exclude`, sem reserializar o resto (o
`json.dumps(indent=1)` e o que produz os 3713 chars de ruido textual) — e o diff semantico no selo;
(b) declarar no meta as opcoes do `TSCONFIG_JS` dos 4 sem tsconfig;
(c) **reiniciar o v7 do zero**.


---

## AJUSTE DA COORDENADORA APLICADO — o `exclude` SO onde casa (10/10)

`excludes_necessarios(repo)` devolve os `exclude` que casam com algum arquivo REAL do repo.
**Medido nos 7 clones**:

| repo | `exclude` necessario |
|---|---|
| dom-examples | `['web-workers/worker-playground/bad-syntax-worker.js']` |
| date-fns · excalidraw · express · html5-boilerplate · lodash · zod | **nenhum** |

`acrescenta_exclude_minimo()` grava **HEAD + so o `exclude`, EDITANDO O TEXTO** — sem
`json.load`/`json.dumps`, que era o que produzia os 3713 chars de ruido. Tres condicoes para
gravar, e sem elas o disco NAO e tocado: (a) ha `exclude` a acrescentar, (b) o JSON montado
continua valido, (c) nenhum campo alem do `exclude` (`compilerOptions`, `include`, `files`,
`extends`) mudou.

**Estado dos clones agora** (restaurados por `git show`, nunca `checkout --`):
excalidraw, zod e date-fns com o tsconfig **byte a byte o do HEAD** (`git diff --quiet` limpo);
dom-examples, express, html5-boilerplate e lodash sem `tsconfig` no HEAD — o criado pelo
rotulador fica, e as opcoes vao declaradas no meta do selo.

Portas: selftest **60/60** · ruff **All checks passed** · pyright **0 errors**.

## v8 DO ZERO, por `hw-slot` (PID 2510082)

`~/.claude/touring/vgp/corpus_holdout_v8_zero`, diretorio novo e vazio. Lancado por
`hw-slot --class heavy --label 4f-rotulador --wait 3600`, e estava **na fila** («aguardando —
temp 93C > 88C») no momento da gravacao. Machine a 94 °C, `heavy-1` com `p0-atributo-juizes-f55`,
`heavy-2` livre.

O **v7** (42 entradas, parado por SIGTERM) NAO serve de referencia de selecao: a regra de
selecao ja mudou desde entao. O passo (5) vai comparar o **v8** com o **v6** na intersecao por
`id` — o v6 e a referencia de ROTULO, nao de selecao.

## Ordem do que falta

1. v8 fecha (na fila do hw-slot).
2. Passo (5): `scratch/concordancia_v6_v7.py` no v6 ∩ v8, com a distribuicao de rotulos da
   intersecao e o aviso se ela nao tem nenhum positivo.
3. Selo do v8, com `identidade_do_oraculo` (sha do `oraculo_ls.js` + versao do `tsc`) e
   `tsconfig_alterado` (sha_head / sha_usado / diff por repo) + `concentracao_medida`.
4. Rust `real`: selftest em 2/16, causa nao medida. Probe pronto, `hw-slot` a 95 °C.
5. Prova de equivalencia JS: reescrita (disco no `antes` nas DUAS rotas, `Counter` nos dois lados,
   base alem do delta), veredito atual NAO PROVADO, em reexecucao.


---

## ACHADO NOVO: DOIS ORACULOS DIFERENTES NO MESMO LOTE (10/10)

`identidades_do_oraculo()` nomeava **uma** versao do `tsc`, com `break` no primeiro repo
achado. Medido agora, o selo seria:

| repo | TypeScript |
|---|---|
| date-fns | 7.0.2 |
| excalidraw | **5.9.3** |
| zod | **5.5.4** |
| dom-examples · express · html5-boilerplate · lodash | 5.9.3 |

O selo de um lote `excalidraw,zod` declararia `tsc_date-fns: Version 7.0.2` — um oraculo que o
lote **nao usa**, e da versao mais nova das tres. Dois oraculos diferentes no mesmo lote, e o selo
nomeando um terceiro.

**Um selo que nomeia a ferramenta errada e pior que um selo que nao nomeia**: ele convence que a
identidade esta conferida.

Corrigido: `ident["tsc_por_repo"]` com a versao de cada repo, e o mutante `break` coberto por
`cenario_oraculo_por_repo` (rota do `globe`: um so oraculo contra os 7 reais).

Portas: selftest **63/63** · ruff All checks passed · pyright **0 errors**.

## Estado dos clones para o selo do v8 (medido agora)

`excalidraw`, `zod` e `date-fns`: tsconfig **IDENTICO AO HEAD**. `dom-examples`, `express`,
`html5-boilerplate` e `lodash`: HEAD nao tem tsconfig, o criado fica (diff 0 chars, `sha_head` None).


---

## `tsconfig` COM COMENTARIO — o selftest pegou (10/10)

Escrevi `cenario_tsconfig_minimo` para provar que so o `exclude` muda, e ele reprovou em **74/75**:
`(com comentario) o texto montado e JSON valido: Expecting property name enclosed in double
quotes`.

O defeito era real e do rotulador: o validador usava `json.loads`, que **nao aceita comentario**,
enquanto o `tsc` aceita (JSONC). Um `tsconfig` de repo com um `//` de explicacao reprovaria a
garantia — ou seja, a garantia "so o exclude muda" so valeria para repos sem comentario, que e o
oposto do que se quer.

Corrigido com `jsonc_para_json()`, que remove `//` e `/* */` **respeitando as strings**: um
`"http://x"` nao e comentario, e apagar a partir do `//` dentro de uma string quebraria o JSON.

`acrescenta_exclude_minimo` tambem foi decomposta: `monta_exclude_minimo()` faz a montagem (a
parte com decisao) e `acrescenta_exclude_minimo()` decide gravar. A garantia estava dentro da
funcao que grava — ou seja, so era exercitada quando um repo real passava por ela.

Portas: selftest **78/78** · ruff All checks passed · pyright **0 errors**.


---

## VALIDADOR PELO PARSER DO COMPILADOR — 87/87 (10/10)

Sugestao da coordenadora, e a melhor das duas: validar pelo `ts.parseConfigFileTextToJson` em vez
de um parser proprio. Assim **o validador e o compilador concordam por construcao** — nao existe
tsconfig que o `tsc` aceite e o validador recuse, porque e o mesmo codigo.

`scripts/parse_tsconfig.js`: protocolo framing `#<n bytes>` (o mesmo do `oraculo_ls.js`, pelo
mesmo motivo medido — `readline()` trunca em 65.536). `valida_tsconfig()` cai no `jsonc_para_json`
quando nao ha compilador instalado, e o cenario marca isso: um validador sem o compilador e mais
fraco, e ele precisa **dizer** que e mais fraco.

**Medido, os 4 casos**:

| entrada | compilador | `jsonc_para_json` (mutante) |
|---|---|---|
| comentario `//` | aceita | aceita |
| virgula final | aceita | **REPROVA** |
| `"https://x"` em string | aceita | aceita |
| `{"a": }` (invalido) | **reprova** («Expression expected») | reprova |

O mutante ja trata comentario e URL depois que eu escrevi o removedor respeitando strings — mas
ainda assim e o caminho errado, porque reprova na virgula final. Cada conserto meu de parser compra
um caso e deixa outro. O `cenario_validador_pelo_compilador` mede isso explicitamente: «existe caso
que o tsc aceita e o mutante NAO».

## TRES ERROS MEUS NESTA PASSAGEM (todos de基础设施, nenhum do oraculo)

1. **`subprocess.run(input=...)` nao acrescenta newline** — o Node le por linha, a requisicao ficou
   no buffer, o `data` nunca disparou, e o processo saiu com `rc=0` e stdout **VAZIO**. Um zero sem
   aviso, pela **sextima** vez na onda, e desta vez dentro do validador que eu acabava de escrever.
2. **`node -e "<codigo>"` com o codigo embutido numa string Python** pagava o escape duas vezes
   (`\\n` virava `\n` literal no JS): `rc=0` e stdout vazio, mesma assinatura. Resolvido com um
   arquivo `.js` proprio.
3. **`process.argv[2]` com `node -e`** — o primeiro argumento que segue o `-e` e o `[1]`, e o
   `[2]` vinha `undefined` para o `require`.

Portas: selftest **87/87** · ruff All checks passed · pyright **0 errors**.

## Jobs: AMBOS NA FILA do hw-slot

- v8 (PID 2510082) · probe do Rust (PID 2256905): «aguardando — temp 93C > 88C»
- Maquina a 95 °C. Nao forco.


---

## `chama_oraculo` — a LIÇÃO virada EXECUTOR (10/10)

`scripts/chama_oraculo.py`: uma porta so por onde passa TODO subprocesso de oraculo. O
diagnostico estava certo, e o que faltava era deixar de ser uma regra (que eu quebraria na
proxima urgencia) e virar uma funcao.

**Contrato por ferramenta** — declara ONDE o sinal tem de aparecer:

| ferramenta | stream | framing | `vazio_valido` | sinal |
|---|---|---|---|---|
| `cargo` | **stderr** | nao | nao | `error[EXXXX]` |
| `tsc` | ambos | nao | nao | `error TSdddd` |
| `node_ls` | stdout | sim | nao | `^\s*[{#]` |
| `node_parse` | stdout | sim | nao | `^#\d+` |
| `show_config` | ambos | nao | nao | `error TS` ou `{` |
| `git_show` | nenhum | nao | — | o `rc` carrega |
| `git_status` | stdout | nao | **sim** | (com controle positivo) |

**As tres garantias, uma por clausula**:
(a) `entrada` ganha o `\n` final — MEDIDO: sem ele o Node le por linha e sai `rc=0` stdout vazio.
(b) `rc=0` sem sinal no stream contratado e ERRO NOMEADO — `sinal vazio: <ferramenta> rc=0 sem
    saida no <stream>`, com os dois tamanhos em bytes. `vazio_valido` so passa com um **controle
    positivo da mesma sessao**, porque «nao achou nada» e «nao rodou» produzem o mesmo byte.
(c) o framing `#<n>` e conferido: `truncado: esperava N, veio M`, nunca lista vazia.

Uma ferramenta **sem contrato declarado e recusada** — nao ha default silencioso.

**`scripts/test_chama_oraculo.py`: 15/15**, com um MUTANTE por clausula:
| clausula | mutante | medido |
|---|---|---|
| (a) newline | processo que nunca le a entrada | RECUSADO |
| (b) sinal | processo mudo com `rc=0` | RECUSADO, nomeia ferramenta e stream |
| (b) vazio_valido | `git_status` vazio SEM controle positivo | RECUSADO |
| (c) framing | `readline` em resposta de 70.000 chars | REPROVA; a leitura declarada le os 70.000 |

**Migrados**: `delta_do_tsc` (retestemunho, `tsc`) e `roda_check` (o `cargo` do `corpus_rust_real.py`
— este e o CASO 1 da assinatura). Os `git` de leitura NAO migraram, de proposito: eles nao produzem
diagnostico, produzem conteudo, e um `vazio_valido` para eles seria regra sem sinal.

Portas: rotulador **87/87** · executor **15/15** · lock **6/6** · ruff All checks passed ·
pyright **0 errors**.


---

## A PORTA FICOU UNICA: `LS.cmd` usa o mesmo leitor (10/10)

O cliente do `oraculo_ls.js` era o call site que mais importava — e a classe que **ROTULA** — e o
framing e o `ok` dele viviam num leitor PROPRIO. Um conserto futuro do protocolo valeria num lugar
e nao no outro, que e o oposto de porta unica.

`LS.cmd` agora le os `n` bytes declarados (porque o `node` e de VIDA LONGA: um `Popen` persistente,
sem `subprocess.run` para entregar a saida pronta) e entrega a resposta MONTADA a
`oraculo.ler_framing` — o mesmo leitor do executor. O **parse** fica num lugar so.
`ler_corpo_marcado` foi removido: ele era o leitor antigo, e o selftest do executor ja mede que o
`readline()` reprova em resposta de 70.000 chars.

**Um erro meu na migracao, e ele e do mesmo tipo dos outros sete**: passei SO o cabecalho ao
leitor, e ele deu `Expecting value: char 0` — o corpo vem na linha SEGUINTE. O selftest reprovou
e o diagnostico estava no rastro.

Portas: rotulador **87/87** · executor **15/15** · lock **6/6** · ruff All checks passed ·
pyright **0 errors**.

## v8 e Rust: AMBOS AINDA NA FILA

`hw-slot`: «aguardando — temp 93C > 88C», maquina a 94 °C, slots com `r96-candidato` e
`p0-atributo-juizes-f55` de outro dono. Nao forco em nenhum dos dois.


---

## PROVA DA CADEIA INTEIRA (pronta, NAO rodada — espera o `hw-slot`)

`scratch/selo_corrida.sh`: um lote minculo (zod, `--alvo 6 --max-commits 25`) que prova que a cadeia
inteira fecha num lote pequeno:

lock → selecao canonica → teto medido → selo com `identidade_do_oraculo` (sha do `oraculo_ls.js`,
`tsc_por_repo`) + `tsconfigs_do_oraculo` por repo + `concentracao_medida`.

**NAO rodei**: o `hw-slot` esta com `r96-candidato` e `p0-atributo-juizes-f55` de outro dono, e a
maquina a 95 °C. Um lote de prova consome `tsc --noEmit -p .` em repo inteiro — exatamente o tipo de
carga que o `hw-slot` existe para serializar. A ordem do Leibniz: quando o slot liberar, o v8 e o
Rust tem prioridade (sao os que a.Remote espera), e a prova da cadeia so depois.

## ESTADO CONSOLIDADO (10/10)

### Portas
| artefato | resultado |
|---|---|
| `scripts/rotular_oss.py --selftest` | **87/87** |
| `scripts/test_chama_oraculo.py` | **15/15** |
| `scripts/test_vgp_lock.py` | **6/6** |
| `selftest_protocolo_ls.py` | **5/5** (resposta de 1.411.923 bytes) |
| `scripts/corpus_rust.py --selftest` | **13/13**, 80 positivos, determinismo IDENTICO |
| `ruff check` (todos os `.py` da onda) | **All checks passed** |
| `pyright` | **0 errors** |

### Jobs na fila do `hw-slot` (nenhum forcado)
| job | PID | destino |
|---|---|---|
| v8 do zero | 2510082 | `~/.claude/touring/vgp/corpus_holdout_v8_zero` |
| probe do Rust `real` | 2256905 | `/tmp/rust_formato.txt` |

### Fechado
- clone limpo (7 clones, so `M tsconfig.json` e `?? .yarnrc`, que sao da allowlist)
- diff SEMANTICO do `tsconfig`: 0 `compilerOptions` diferentes, 2 entradas de `exclude`
- `exclude` so onde casa: **so o dom-examples**
- `tsconfig` do HEAD byte a byte em excalidraw, zod e date-fns
- validador = `ts.parseConfigFileTextToJson` (o parser do `tsc`)
- executor de oraculo com contrato por ferramenta, `vazio_valido` preso ao controle positivo
- porta unica: `LS.cmd` usa `oraculo.ler_framing`

### Em aberto
1. v8 → passo (5), concordancia v8 ∩ v6 por `id`
2. Rust `real`: 2/16, causa nao medida (o probe imprime a linha BRUTA; agora passa pelo contrato
   `cargo`, `stream="stderr"`)
3. prova de equivalencia JS: NAO PROVADO, em reexecucao


---

## MEMORIAS GRAVADAS (10/10)

A lição atravessa o programa inteiro e nao só esta onda, entao saiu do bundle para a memoria
persistente (indice atualizado, 148 entradas):

| memória | o que guarda |
|---|---|
| `rc-zero-sem-sinal-e-a-assinatura` | os 7 sightings, e que a defesa que dura e contrato |
| `chama-oraculo-contrato-por-ferramenta` | o desenho do executor e por que cada campo existe |
| `teste-com-parser-proprio-reprovou-o-valido` | `ts.parseConfigFileTextToJson` sobre parser proprio |

## CONFIRMACAO: as portas aceitas continuam verdes (10/10)

| porta | resultado |
|---|---|
| `corpus_rust.py --selftest` (estrato `sanidade`) | **13/13** |
| `corpus_semeado.py --selftest` (tarefa C, fechada antes) | **37/37** |
| `rotular_oss.py --selftest` | **87/87** |
| `test_chama_oraculo.py` | **15/15** |
| `test_vgp_lock.py` | **6/6** |

A tarefa C e o `sanidade` foram aceitos antes de `chama_oraculo`, do lock, do validador pelo
compilador e da selecao canonica. Continuam verdes: nenhuma dessas mudancas regrediu o que ja
estava fechado.

## MONITOR REARMADO

O anterior expirou — ele vigiava o **v7**, que eu parei de proposito. O novo vigia **v8** e o
**probe do Rust**, com filtro que tambem pega a saida de quem sai da fila.


---

## GESTAO DOS SHELLS E SCRIPTS (10/10, a pedido do Gabriel)

### Shells de fundo: 45 -> 10

Os shells eram `until ! pgrep -f "<padrao>"; do sleep 30; done` — **loops de espera do proprio
harness**, nao jobs. Classificados por O QUE esperavam, nao pela idade:

| classe | n | destino |
|---|---|---|
| **improdutivo** (esperam um `corpus_semeado.py --selftest` / `prova_equivalencia` / `cargo check`
  que JA TERMINARAM) | **35** | parados com SIGTERM |
| **produtivo** (esperam o determinismo e o v8, vivos) | **6** | mantidos |

**Como classifiquei sem me enganar**: `pgrep -f "<padrao>"` e executado de verdade, e um alvo so conta
como "trabalho" se nao for outro shell de espera. A primeira versao do meu programa marcou 14 como
improdutivos e depois 21 — a diferenca era que o filtro por substring casava um shell com ele mesmo.
**Dois shells esperavam um padrao que SO eles casavam**: `until ! pgrep -f "lang javascript"`
nunca terminaria, porque o proprio comando contem a string procurada. Sao **loops infinitos por
construcao**, e nao por esquecimento.

Os 6 que ficaram: 4 em `confere-determinismo` (esperam o determinismo por linguagem, que roda) e 2
em `rotular_oss` (esperam o v8).

### Scripts `.sh`: 22 -> 1

Todos os 22 tinham sintaxe valida (`bash -n`), mas **21 eram patches de uso unico cujo efeito ja
esta no `rotular_oss.py`** — nao sao ferramentas, sao o rastro de uma edicao. Movidos para
`_aplicados_20261010/` com um `LEIA-ME.md` que diz o que cada um aplicou. **Nenhum foi apagado**:
o `rm -rf` foi bloqueado pelo gate, e a decisao de remocao e do Gabriel.

O unico que fica na raiz e `selo_corrida.sh` — a prova da cadeia, que **ainda nao rodou**.

### Probes Python: versoes superadas arquivadas

`selecao_canonica` e `diff_semantico` tiveram **tres versoes cada**, e cada anterior continha um
erro que a seguinte corrigiu:

| versao | o que estava errado |
|---|---|
| `selecao_canonica.py` | secao morta `if False`; nunca rodou |
| `diff_semantico_tsconfig.py` | sem `text=True` -> "JSON invalido"; o `--showConfig` produzia 41.899 bytes |
| `diff_semantico2.py` | `achata()` ignorava o retorno recursivo -> 0 campos nos 3 repos |

As superadas foram para `_supersedidas_20261010/` com um `LEIA-ME.md` que liga cada erro ao probe
que o mediu. Os canonicos renomeados para o que fazem: `aplica_selecao_canonica.py` e
`mede_diff_semantico_tsconfig.py`.

**E o achado que vale**: o padrao `rc=0 sem sinal` apareceu **dentro dos meus proprios probes**,
tres vezes seguidas, que e o lugar onde eu menos esperava encontrar. Um probe que da `0` onde
deveria dar um numero nao e um probe quebrado — e um probe que produz uma conclusao errada com
aparencia de medida.

### Jobs (a saida do hw-slot chegou)
- **probe do Rust**: EXPIROU na fila (`sem vaga apos 2400s`), nunca rodou. Reenfileirado com
  `--wait 7000` e `--label 4f-rust-probe` (PID 3053988).
- **v8**: continua na fila do `hw-slot` (PID 2510082).


---

## NONO SIGHTING: o oraculo LENTO lido como MORTO (10/10, matou o lote `v8`)

**Sintoma**: o `v8` morreu com
`RuntimeError: o oraculo LS nao respondeu a 'delta': rc=None | stderr=(vazio)`.

**`rc=None` NO `poll()` significa "o processo AINDA ESTA VIVO"**, nao que morreu. E o `stderr` nao
era lido nesse ramo, entao a mensagem dizia so `stderr=(vazio)` — um processo vivo e mudo sem
nenhuma pista. Investiguei OOM numa maquina com **62 GB livres**.

**Medido, nao suposto** (`scratch/probe_ls_mudo.py`, 12 medicoes do loop real):

| | resultado |
|---|---|
| leituras VAZIAS | **6 de 12**, todas com `rc=None` |
| tempo por `delta` (arquivo de 90 KB) | 0,29 s a 2,07 s |
| o `node` responde depois? | **SIM** — `select` pegou `#77524` + corpo no pipe segundos depois |

**Por que `readline()` nao esperou**: em modo texto (`text=True, bufsize=1`), `readline()` devolve
`''` quando o buffer de texto **ainda nao tem linha** — mesmo com o pipe bloqueante e o processo
vivo. So bloqueia se o processo ja tiver escrito ALGO.

**Correcao**: `_espera_resposta()` com `select.select([stream], [], [], ESPERA_PADRAO)` antes do
`readline()`. `ESPERA_PADRAO = 180.0 s` — bem acima do pior tempo medido (2,07 s) e bem abaixo do
timeout do lote, para um travamento real nao travar o rotulador. E o erro agora **nomeia os dois
casos**: `VIVO E NAO RESPONDE` (rc=None) vs `morreu com rc=N`, com o `stderr` lido nos dois.

**`cmd` reescrito inteiro**: estava com a espera inserida DUAS vezes e sem a constante
(`AttributeError: 'LS' object has no attribute 'ESPERA_PADRAO'`). 87 linhas viraram 72.

`cenario_oraculo_lento` no selftest: prova que a espera existe, que tem **teto finito**, que o erro
distingue vivo de morto, e o **mutante** (o `cmd` sem espera) reprova. **92/92**.

## O probe do Rust: a CAUSA do 2/16 esta MEDIDA

O `cargo` emite, para a receita que eu achava errada:

```
crates/touring-identity/src/lib.rs:32:18: error: missing parameters for function definition
crates/touring-identity/src/lib.rs:32:23: error: expected item, found `(`
error: could not compile `touring-identity` (lib) due to 2 previous errors
```

O regex do `corpus_rust_real.py` e `error\[?(E\d+)\]?` — exige o codigo. O `cargo` escreve
**`error:` sem codigo** nesses casos, entao `NAO CASOU` e o selftest quebrava. Alem disso a receita
`nome` esta **malformada** (`mutado_{n}` com `{n}` literal na f-string, da onde
`pub fn mutado_{N}() -> i64 { vgp_nome_ausente_{N} }`).

O probe EXPIROU na fila do `hw-slot` (2400 s) e foi reenfileirado com `--wait 7000`. A leitura do
`stdout`/`stderr` dele ja rendeu a linha bruta acima.


---

## A DISTINCAO QUE ORGANIZA A NONA OCORRENCIA (10/10)

`subprocess.run` **espera o processo terminar** — a leitura do stdout so acontece depois que ha
algo la, e o nono sighting nao pode acontecer la.

`Popen` **persistente** (um `node` de vida longa, um comando por vez) **le enquanto o processo
vive**, e `readline()` em modo texto devolve `''` quando o buffer ainda nao tem linha. E o
processo esta VIVO calculando.

| | `run` (o executor) | `Popen` (a classe `LS`) |
|---|---|---|
| quando le | depois do processo terminar | enquanto o processo vive |
| garantia necessaria | o `timeout` do `subprocess.run` | `select` antes do `readline` |
| modo de falha | `TimeoutExpired` | `''` com `rc=None` |

Cada um precisa da garantia que o outro nao tem. A distincao esta agora no docstring de
`chama_oraculo`, porque e ela que impede o proximo de "unificar" os dois e apagar a espera.

## Prova da espera: em execucao

`scratch/prova_espera.py` — o mesmo cenario que deu **6 de 12 leituras vazias** sem a espera, agora
com a classe `LS` real (com a espera) contra o `node` real. Escolhi o **zod** e nao o excalidraw por
**medicao**: o `abrir` do excalidraw materializa 1430 arquivos e estourou o `timeout` de 500 s
(exited 143) na prova anterior; no zod abriu em **0,2 s**.

E o `| tail -10` da prova anterior esconderia a saida: o `timeout` mata o python antes do pipe
recuperar, entao a prova nao mostrava nada. Agora a saida vai direta para o arquivo.

## Estado dos jobs

| job | PID | situacao |
|---|---|---|
| prova da espera | 3317345 | RODANDO (zod, 10 voltas, arquivo de 177 KB) |
| v9 do zero | 3216701 | fila do `hw-slot` |
| probe Rust v3 (receita corrigida) | 3215279 | fila do `hw-slot` |

Maquina a 94 °C, os tres slots com processos de outro dono
(`p0-atributo-gates-finais`, `r96-prova`). Nao forco nenhum.


---

## DEZ (PROVAVEL) SIGHTING: a RESPOSTA MAIOR QUE O PIPE (10/10, em medicao)

**Sintoma**: a prova da espera no zod travou. O `node` em `epoll_wait` (event loop VAZIO, ja
respondeu) e o Python em `anon_pipe_read` (esperando bytes) — os dois esperando um pelo outro.

**O que ja esta medido**:
- os pipes casam certo: Python `fd5` -> `pipe:[13422878]` = Node `fd1`. Nada trocado.
- o Node tem 441 MB de RSS, 7 threads, `utime=355` — trabalhou.
- `State: S (sleeping)`, `wchan: do_epoll_wait` — o loop de eventos esta ocioso.

**A hipotese, ainda NAO medida**: o buffer do pipe do Linux e **65536 bytes**. Se a resposta do
`delta` passar disso, o `write()` do Node BLOQUEIA; o Python le o cabecalho e trava esperando os
`n` bytes do corpo que nunca chegam inteiros. Isso e um caso que o `select` NAO resolve — o `select`
cuida do «node lento», nao do «resposta grande demais para o pipe».

`scratch/mede_resposta_ls.py` esta medindo pelo caminho REAL (a classe `LS`): reconstrói o corpo
com `json.dumps(separators=(",", ":"))` e compara com 65536. PID 3367081, saida em
`/tmp/mede_resposta.txt`. **Se a volta travar em 180 s, o proprio `select` da prova sera a
medida**: o erro nomeado «VIVO E NAO RESPONDE» significa que o Node tem dados para escrever e o
pipe esta cheio.

**Nao afirmo a hipotese antes do numero.** Se a resposta for menor que 64 KB, a causa e outra e o
`wchan` do Node precisa de outra leitura.

## Por que o probe antigo nao serviria

`probe_ls_mudo.py` usou um arquivo de 90 KB do excalidraw e deu 6 de 12 leituras vazias com
`rc=None`. O zod tem um arquivo de **177 KB** — e o que muda entre os dois nao e o ARQUIVO, e o
NUMERO DE DIAGNOSTICOS na resposta. E o numero de diagnosticos, nao o tamanho do arquivo, que
decide se a resposta cabe no pipe. Por isso o probe antigo, que rodou bem, NAO prova nada sobre
este caso.

## Estado dos jobs

| job | PID | situacao |
|---|---|---|
| medicao da resposta | 3367081 | RODANDO |
| v9 do zero | 3216701 | fila do `hw-slot` |
| probe Rust v3 | 3215279 | fila do `hw-slot` |

A prova `prova_espera.py` (PID 3317345) foi PARADA: nao mediava nada em 3 min com o Node em
`epoll_wait`. Nao e resultado, e travamento — e por isso a medicao nova existe.


---

## O TRAVAMENTO DO `abrir` — MEDIDO, E NAO E O QUE PARECE (10/10)

Sintoma: no zod, `abrir` respondeu `TRUNCADO: 0/188 bytes em 45s` — a resposta tinha **188 bytes
declarados** e **zero** foram lidos. O `abrir` do mesmo repo tinha respondido em **0,2 s** meia
hora antes.

**O que medi para descartar as explicações faceis**:

| hipotese | medicao | veredito |
|---|---|---|
| o `oraculo_ls.js` mudou | sha256 `8e8dd5f1…eebe622` nos DOIS lugares (bundle e `oss/`) | **nao mudou** |
| faltam CPUs | 28 threads, CPUs 8-11 offline, load 6,25 de 24 CPUs ativas | **ha capacidade** |
| e o `Popen` do probe | le `cabecalho` com `select` e teto de 45 s | probe correto, com teto |
| a resposta passou de 64 KB | **nao**: eram 188 bytes declarados | **nao e tamanho** |

`188 bytes` e uma resposta PEQUENA. Um pipe de 65536 bytes a absorve sem bloqueio, entao a decima
hipotese (resposta maior que o pipe) esta **DESCARTADA por medicao**.

O que resta, e que eu ainda nao medi: o `node` sob CONTENTION com outros donos da maquina. Ha um
`node` do MCP a **141 % de CPU** e um `bash` de servico do Hyprland a 78,7 %. O `node` do LS
`wchan=do_epoll_wait` com `utime` CONGELADO — que e o node esperando o pipe, nao calculando.

**Nao afirmo a causa sem o numero.** O que falta e o tempo do `abrir` com a maquina OCIOSA. A
medicao em curso (`scratch/mede_zod_abrir.py`, PID 3399759) faz zod e excalidraw lado a lado com
teto de 45 s por passo: se os DOIS travarem agora, e contention; se so o zod, e o repo.

**Por que isso importa para a regra**: um `abrir` que trava por 45 s e um `abrir` que esta
lento sao a MESMA leitura no meu codigo (`''` com `rc=None`). So a medicao com teto separa os
dois, e e por isso que a prova usa `le_com_teto` em vez de `readline`.


---

## VEREDITO: o travamento e CONTENTION, e nao o codigo (10/10, medido lado a lado)

`scratch/mede_zod_abrir.py` rodou **zod e excalidraw no mesmo minuto**, com teto de 45 s por passo:

| repo | `abrir` | bytes lidos |
|---|---|---|
| zod | `TRUNCADO` em 45,3 s | **0 de 188** |
| excalidraw | `TRUNCADO` em 45,3 s | **0 de 202** |

**Os DOIS travaram, com respostas de 188 e 202 bytes.** Uma resposta de 188 bytes nao tem como
exceder um pipe de 65536 — a decima hipotese esta **DESCARTADA por medicao**, nao por argumento.
E o excalidraw e o repo que respondia `abrir` em **0,2 s** meia hora antes: se fosse o repo ou o
codigo, um dos dois responderia.

O que mudou entre as duas medicoes: **a maquina**. `uptime` com load **5,62** e um `node` da sessao
`analise` a **167 % de CPU** (o processo `ps` a 100 % era o proprio comando da medicoe). As 22:54
o load estava menor e o mesmo `abrir` levava 0,2 s.

**Conclusao**: o `select` que adicionei esta CORRETO (separa «node lento» de «node morto»), e o
`ESPERA_PADRAO = 180 s` e generoso demais para uma maquina saturada — mas reduzir o teto
TRANSFORMARIA contention em «node nao respondeu», que e o erro oposto e pior. O certo aqui e nao
rodar: o `hw-slot` existe para exatamente isto, e os tres jobs meus estao na fila dele ha meia
hora.

**O que a onda ensina sobre este caso**: eu passei de «o node morreu» para «o node esta lento»
para «o no esta saturado de maquina」 — tres leituras do mesmo sintoma (`''` com `rc=None`), e so
a TERCEIRA esta medida. As duas primeiras eram distinguidas por codigo (o nome do erro), a
terceira so por experimento controlado (dois repos, mesmo minuto).

## Estado final dos jobs

| job | PID | situacao |
|---|---|---|
| v9 do zero | 3216701 | fila do `hw-slot` (load 5,62, sessao `analise` a 167 % de CPU) |
| probe Rust v3 | 3215279 | fila do `hw-slot` |

O **probe da espera** foi ABANDONADO como prova de correcao: com a maquina saturada, ele mede o
ambiente, nao o `select`. A correcao esta no codigo (`cenario_oraculo_lento` no selftest, com o
mutante), e a prova de campo so vale com a maquina ociosa — que e quando o `hw-slot` liberar.


---

## 23:18 — o v9 ENTROU NO SLOT (10/10)

`hw-slot --status`: `heavy-1 4f-rotulador-v9`. O `timeout 7000 python3 -I scripts/rotular_oss.py`
(PID 3346159) esta rodando com um unico `node` do LS (PID 3360005, filho do rotulador).

**A MAQUINA**: load 5,56 com 24 CPUs ativas (8-11 offline, como o CLAUDE.md do projeto registra).
Load nao e 24, entao a maquina TEM capacidade — mas ha um `pytest` a 177 % e um `node` da sessao
`analise` a 142 %. A **contention** que travou a prova da espera era REAL, mas nao esgota a
maquina: e carga pontual de outros donos.

**Consequencia pratica**: **NAO rode a prova da espera agora** — ela disputa o mesmo
`oraculo_ls.js` e o mesmo disco de clones com o v9, e o resultado seria mistura de contention
com o v9. A prova de campo da espera so vale com a maquina ociosa E sem o rotulador rodando.

O probe Rust (PID 3215279) continua na fila; `rust-1` apareceu LIVRE no `status`, mas o `hw-slot`
nao oLiberou (ele reavalia por tempo, e o slot pode ter sido tomado entre o status e a reavaliacao).

## Ports (medidos as 23:18, depois de todas as correcoes)

| porta | resultado |
|---|---|
| `rotular_oss.py --selftest` | **92/92** |
| `test_chama_oraculo.py` | **15/15** |
| `test_vgp_lock.py` | **6/6** |
| `chama_oraculo.py` pyright | **0 errors** |
| `rotular_oss.py` ruff | **All checks passed** |


---

## 23:19 — v9 RODANDO MAS COM O NODE CONGELADO (10/10)

`utime=428` **inalterado** entre duas medicoes separadas por minutos: o `node` do LS **parou de
trabalhar**, nao esta lento. `wchan=do_epoll_wait` = event loop ocioso. **23 min de lote e ZERO
entradas gravadas.**

Isso e o MESMO travamento que impediu a prova da espera — so que agora ele atinge o ROTULADOR.
Combinado com o que medi antes (zod e excalidraw **os dois** com `0/188` e `0/202` bytes em 45 s),
a leitura mais economica e: **`node` do LanguageService trava nesta maquina sob contention**, e o
`node` nao e o unico — o `pytest` de outra sessao esta a 160 % de CPU neste instante.

**O que eu NAO vou fazer**: reduzir o `ESPERA_PADRAO` de 180 s. Isso transformaria contention em
«node nao respondeu», que e o erro oposto e produz um selo com `paradas` em vez de um lote.
Esperar mais e a saida honesta.

**A acao que cabe agora**: nada. O v9 esta no `hw-slot`, o rotulador esta correto (92/92), e
intervir num job que pode estar a um `select` de continuar seriaDestroy mais o lote que salvar.

### Para quem retomar

- **v9** (PID 3346159, node 3360005): na fila/rodando, sem entradas. Se em 30 min continuar com
  `utime` congelado e 0 entradas, e travamento confirmado — e a proxima acao e medir o `node`
  sozinho, SEM o rotulador, num repo pequeno, com teto por passo (`mede_zod_abrir.py` faz isso e
  ja rodou: **os dois** repos travaram em 45 s nesta maquina).
- **probe Rust** (PID 3215279): na fila do `hw-slot`, `rust-1` momentaneamente livre.
- **A prova da espera**: nao roda enquanto houver outro dono usando o mesmo
  `~/.claude/touring/vgp/oss/`. Sem exclusao para o `node`, ela mede contention.


---

## 23:22 — DIAGNOSTICO FECHADO: e o `delta`, nao o ambiente (10/10)

`scratch/mede_delta_sozinho.py`, medido agora, **sem o rotulador rodando**:

| passo | zod | excalidraw |
|---|---|---|
| `abrir` | **ok em 0,3 s** | **ok em 0,3 s** |
| `delta` (volta 0) | **TRAVOU** — `utime=316`, `wchan=do_epoll_wait` | **TRAVOU** — `utime=458`, `wchan=do_epoll_wait` |

**Sequencia que fecha o diagnostico:**
1. 23:00 — `abrir` do zod e do excalidraw **travaram** (0/188 e 0/202 bytes em 45 s).
2. 23:19 — `abrir` **responde em 0,2 s** nos dois, sem nenhuma mudanca no codigo (sha do
   `oraculo_ls.js` identico, repos intocados).
3. 23:22 — `abrir` responde em **0,3 s**, e o **`delta` trava** nos dois repos.

O passo 2 descarta a maquina: a mesma maquina, mesmo codigo, tres minutos depois, `abrir` passou.
O passo 3 localiza: **`abrir` funciona, `delta` nao**. E nos DOIS repos, com tamanhos diferentes
(83 KB e 177 KB).

**`utime` congelado + `do_epoll_wait`** = o node **nao esta calculando e nao esta esperando o pipe**:
o event loop esta ocioso. Isso aponta para o `delta()` do `oraculo_ls.js` ter **lancado algo que
saiu do `try/catch`** — o `catch` esta em volta da CHAMADA, mas uma excecao **assincrona** (dentro
de um `await` ou de um callback) nao passa por ele, e mata o process listener sem responder.

**O que eu NAO afirmo**: qual excecao. A medicao seguinte e abrir o `oraculo_ls.js` e ver o que o
`delta()` faz entre o `try` e a resposta, e rodar com `--trace-uncaught` para ver a excecao real.

**Consequencia pratica**: o rotulador esta CORRETO (92/92, o `select` funciona, o `abrir` funciona) e
o **oraculo esta com defeito no `delta`**. Isso e o que matou o `v8` e o `v9`, e o que impede a
prova de equivalencia JS. **Nao e a maquina, e nao e o meu `readline`.**


---

## 23:26 — a DECIMA SIGHTING, e o que a medicao DESCARTOU (10/10)

**Sintoma reproducible**: `abrir` responde em **0,2-0,3 s**; o `delta` **trava em TODO repo**, com
o `node` em `do_epoll_wait` e `utime` **congelado**. Um arquivo de **511 bytes** trava igual a um
de 177 KB — entao **nao e tamanho, nao e repo, nao e maquina**.

### Descartado por MEDICAO, nao por argumento
| hipotese | como caiu |
|---|---|
| `oraculo_ls.js` mudou | sha `8e8dd5f1…` igual em `oss/` e no bundle |
| faltam CPUs | 24 CPUs ativas, load 5,6 de 24 |
| e o `open` | responde em 0,2 s AGORA, o mesmo que travava 20 min antes |
| e tamanho de arquivo | **511 B trava igual a 177 KB** |
| e o repo | zod E excalidraw travam os dois |
| a resposta passa do buffer do pipe | eram **188 e 202 bytes** declarados |

### O que achei e era REAL, mas **nao e a causa**
`getScriptFileNames` fazia `require("/home/gabrielgadea/projects/analise/frontend/...")` — caminho
ABSOLUTO de outra sessao, que **sombreava** o `ts` do repo rotulado. Um `npm ci` na sessao `analise`
apagaria aquele `node_modules` e o `require` lancaria MODULE_NOT_FOUND dentro de um callback que o
TypeScript chama durante o `delta` — fora do `try/catch` do handler. **Corrigido** (o `ts` do
escopo ja e o certo), propagado ao bundle, sintaxe OK, sha novo `7f2f576b…`.

**E o `delta` continua travando depois da correcao.** O defeito era real e foi consertado; ele
**nao era** o que trava o `delta`. E por isso que a correcao nao entra na cadeia do selo como se
tivesse resolvido: ela conserta um bug, e o bug do travamento segue aberto.

### O que falta medir
O `delta` trava **antes** de responder, e o `utime` congelado diz que o node **nao esta
calculando**. Faltam duas medicoes:
1. rodar o `oraculo_ls.js` com `process.on("uncaughtException")` e `unhandledRejection` registrando
   em `stderr` — hoje uma excecao ASSINCRONA morre sem tocar no `catch` do handler, e o `stderr`
   fica vazio, que e exatamente o que o rotulador viu;
2. chamada direta a `servico.getSemanticDiagnostics(a)` no `node`, sem passar pelo stdin, para ver
   se e o TypeService que trava ou o handshake do protocolo.

**Nao afirmo a causa sem o numero.** Ja cairam tres hypotheses nesta onda por eu afirmar antes de
medir.


---

## 23:32 — a CAUSA-RAIZ esta ISOLADA ate a LINHA (10/10), e a correcao ainda nao fecha

Rodei o `ts.createLanguageService(...).getSemanticDiagnostics` **DIRETO**, sem o protocolo, com
`process.on("uncaughtException")` registrando em stderr. A excecao aparece:

```
UNCAUGHT: TypeError: Cannot read properties of undefined (reading 'target')
    at computeValue (typescript.js:21908:38)
    at acquireOrUpdateDocument (typescript.js:143961:73)
    at Object.acquireDocumentWithKey (typescript.js:143919:12)
    at getOrCreateSourceFileByPath (typescript.js:152933:31)
```

E a **linha 21908 do arquivo REAL** e literalmente:

```js
const target = compilerOptions.target === 0 /* ES3 */ ? void 0 : compilerOptions.target;
```

Logo: `compilerOptions.target` e `undefined` no ponto em que o TypeScript le.

### O que ja esta DESCARTADO por medicao
| hipotese | como caiu |
|---|---|
| `target` sai como string do `parseJsonConfigFileContent` | **medido: sai `99` (numero)** — `ScriptTarget.ES2022 = 9`, `ESNext = 99`. Correto |
| `settings` esta vazio quando o host e criado | `let settings = {}` (L124), `lerTsconfig` atribui (L135), e `abrir` chama `lerTsconfig` (L182) **antes** de `criarHost()` (L183) |
| `getCompilationSettings` devolve o objeto errado | devolve `settings`, que tem `target: 99` |
| o `acquireDocumentWithKey` falta no host | **ADICIONEI** (com `scriptKindDe`) e o delta continua travando |

### O que ainda NAO sei
`acquireOrUpdateDocument` (143961) recebe `compilerOptions` de algum lugar que **nao e o meu
`settings`** — provavelmente o `compilationSettings` que o registry associa ao `ScriptKind`, ou o
`getDefaultLibFileName`. A proxima medicao e imprimir o `compilerOptions` que o TS realmente ve,
interceptando `getCompilationSettings` com um log.

**Nao afirmo a correcao.** As duas intervencoes que fiz (o `require` hardcoded e o
`acquireDocumentWithKey`) sao **defeitos reais** — o `require` apontava para `projects/analise`
(OUTRA sessao) e um `npm ci` la mataria o oraculo — mas nenhuma delas fecha o travamento, e eu
nao vou declarar victory por ter consertado um bug ao lado.


---

## 23:40 — O ORACULO ESTAVA CORRETO. A MEDICAO ESTAVA ERRADA. (10/10)

`rastreia_handshake.js` fala o protocolo do `oraculo_ls.js` de fora, com log de tempo:

```
[     0ms] abrindo o oraculo...
[   270ms] ABRIR RESPONDEU: ok=true ts=5.9.3
[   271ms] delta enviado (509 bytes)
[  2121ms] DELTA RESPONDEU: ok=true base=0 novos=0     <- o DELTA RESPONDE
```

**O oraculo esta CORRETO.** E o `mt2.js` — que chama `getSemanticDiagnostics` DIRETO, sem o
protocolo — tambem responde `OK: 0 diagnosticos`.

### Onde a medicao me enganou (tres vezes, pela mesma razao)
O probe `mede_tsc_service.py` falhava com `TypeError: reading 'target'`, e eu tratei cada falha
como uma descoberta do oraculo:

| # | "descoberta" | o que era |
|---|---|---|
| 1 | o `require` de outra sessao era a causa | **defeito real, mas nao a causa** — um `npm ci` la mataria o oraculo, e o conserto vale |
| 2 | faltava `acquireDocumentWithKey` | **medido: nao faltava** — o probe quebrava por outro motivo |
| 3 | `getScriptFileNames` lancava com `tsconfig = null` | **medido: nao lancava** |

A CAUSA de todas: o probe usava **`conf.opts`** no `getCompilationSettings`, e **`conf.opts` nao
tem `target`** (`parseJsonConfigFileContent` devolve `options`). Depois corrigi para `settings` e
deu `ReferenceError: settings is not defined` — o probe **nunca declarou** `settings`. O `mt2.js`
declara (`settings = Object.assign({}, conv.options, {...})`), e por isso funciona.

**Ou seja: por tres intervencoes successive eu "consertei" o oraculo usando uma medicao que
media o probe, nao o oraculo.** As tres mudancas sao defensivas e ficam (o `require` de outra
sessao e um perigo real), mas **nenhuma delas era a causa do travamento** — e eu escrevi no
arquivo que eram.

### O que ainda NAO foi explicado
O `oraculo_ls.js` responde o `delta` em 2,1 s pelo handshake, e o meu `LS.cmd` (via
`mede_delta_por_tamanho.py`) trava nele. **A diferenca esta no meu cliente, no `readline`, ou no
probe que o chama** — e nao no oraculo. E a proxima medicao, nao uma conclusao.


---

## 23:44 — o cliente Python NAO fala com o oraculo, em NENHUM modo de leitura (10/10)

O handshake em **Node/Buffer** responde (`DELTA RESPONDEU` em 1,7-2,1 s, tres vezes seguidas).
O mesmo handshake reescrito em **Python**, para o MESMO oraculo, falha:

| modo de leitura do `Popen` | `abrir` | `delta` |
|---|---|---|
| `text=True, bufsize=1` (o do `LS.cmd`) | ok | **falha** |
| binario (bytes) | ok | **falha** |

Entao **nao e o modo de texto**: e alguma coisa no cliente Python que eu ainda nao isolei. As
diferencas entre o handshake que funciona (Node) e o que falha (Python) sao tres, e nenhuma esta
medida:
1. o handshake Node **consome o corpo com slicing de `Buffer`** (`buf.slice(0, n)`); o Python usa
   `stdout.read(n)` em循环;
2. o handshake Node dispara o `delta` **dentro do callback `data`**; o Python chama `manda()` e
   depois `le()` fora;
3. o handshake Node faz o parse **assim que a linha chega**; o Python acumula e so processa no
   fim.

**Nao afirmo qual delas e a causa.** A medicao seguinte e o mesmo handshake reescrito em Python
com as tres diferencas removidas uma de cada vez, e ver qual faz o `delta` responder.

### O que esta ESTAVEL e correto
- `oraculo_ls.js` responde o protocolo (medido em Node, 3 rodadas).
- `mt2.js` chama `getSemanticDiagnostics` direto e responde `OK: 0 diagnosticos`.
- As tres mudancas no `oraculo_ls.js` (`require` de outra sessao, `acquireDocumentWithKey`,
  `getScriptFileNames` sem lancar) sao **defensivas** e ficam, mas nenhuma era a causa.


---

## 23:47 — DOIS RESULTADOS MEDIDOS NO MESMO MINUTO (10/10)

### 1. O `cargo` EMITE DUAS LINHAS, e o regex casa a segunda

```
error: could not compile `touring-identity` (lib) due to 1 previous error    <- sem codigo
lib.rs:32:18: error[E0425]: cannot find value `...` in this scope            <- COM codigo
```
`('CASOU', 'E0425', 'lib.rs', 32): 1` e `('NAO CASOU', '', '', 0): 1`.

**O regex do `corpus_rust_real.py` esta CORRETO** — ele casa `error[E0425]` e ignora a linha de
sumario, que nao tem codigo. O selftest em 2/16 era pela **receita malformada do meu probe**
(`{{{{N}}}}` virando `{N}` literal), ja corrigida. **A causa do 2/16 esta resolvida e medida.**

### 2. O cliente Python devolve `bytearray(b'')` no `delta` — o handshake Node responde

| cliente | `abrir` | `delta` |
|---|---|---|
| **Node/Buffer** (`rastreia_handshake.js`) | ok, 222-270 ms | **ok, 1,7-2,1 s** |
| Python, acumulando (como o Node) | ok, 0,21 s | **`bytearray(b'')`** |
| Python, lendo direto do pipe | **TRUNCADO 0/202** | — |

O Python **nao e a diferenca de D1** (acumular vs direto — os dois falham, e por motivos
diferentes). `bytearray(b'')` no `delta` significa que o `os.read()` devolveu **zero bytes** com o
processo vivo — que e o MESMO `''` da nona sighting, agora no cliente Python.

**O que ainda falta medir**: por que `os.read()` devolve 0 bytes num pipe que tem dados. As
hipoteses (NAO afirmo nenhuma): o `select` no `self.p.stdout` de um `Popen` com `bufsize=0`
pode estar inspecionando o objeto errado; ou `os.read(fd)` em `bufsize=0` consome de um
descriptor ja drenado pelo `Popen`. A medicao seguinte e `os.read` direto no `fileno()`, sem
passar pelo objeto do `Popen`.

**O oraculo esta CORRETO e o cliente Python e que nao fala com ele.** Isso e a conclusao que a
medicao sustenta, e ela vale para o `v8` e o `v9`: nenhum dos dois falhou por defeito do oraculo.


---

## 23:52 — o cliente Python: L1 e L2 falham, D2 nao e a causa (10/10)

| leitura do mesmo pipe | `abrir` | `delta` |
|---|---|---|
| **L1** `p.stdout.read(n)` (o do `LS.cmd`) | ok, 0,19-0,22 s | **`b''`** |
| **L2** `os.read(fileno, n)` (descriptor cru) | ok, 0,20-0,26 s | **`b''`** |
| **L3** `delta` DENTRO do callback de stdout (como o Node) | ok, 0,30 s | **sem resposta** |

**L1 e L2 falham igual** — descartei o wrapper do `Popen` como causa. **D2 tambem**: enviar o
`delta` de dentro do callback (exatamente como o Node que funciona) nao muda nada.

**O que fica em aberto, honestamente**: o handshake Node funciona e nenhum dos tres formatos
Python funciona. A diferenca restante que **nao** mexi e que ainda nao isolei: o Node passa
`{"depois": texto}` com o texto do arquivo **como string**, e o meu Python tambem. A unica outra
e que o Node **le o arquivo com `fs.readFileSync`** e envia, enquanto o probe Python abre com
`open(..., errors="replace")` — o que pode mudar o CONTEUDO (encoding) e o `delta` receber um texto
diferente do que o arquivo tem.

**Nao afirmo a causa.** Tres hipoteses cairam; a quarta (encoding do `depois`) e a proxima a medir,
e e uma que muda o que o oraculo VE — nao o como ele fala.

### Resumo do que a cadeia de medicoes provou ate aqui
1. o `oraculo_ls.js` responde o protocolo (Node, 3 rodadas: `DELTA RESPONDEU ok`);
2. `getSemanticDiagnostics` direto responde (`OK: 0 diagnosticos`, `mt2.js`);
3. o cliente Python **nao obtem resposta** no `delta`, por leitura direta ou por callback;
4. o `cargo` emite **duas** linhas e o regex casa a segunda (`CASOU E0425`).

**O `v8` e o `v9` nao falharam por defeito do oraculo.** Falharam porque o cliente Python nao le a
resposta — e essa e a proxima coisa a consertar, com a nona sighting (o `''` que pode ser vivo ou
morto) ja nomeada no erro.

---

## 23:51 — v9 PARADO, residuo restaurado, maquina saturando (10/10)

O v9 estava com o `node` **congelado em `utime=428` ha ~30 min** (`do_epoll_wait`), o mesmo
sintoma do `v8`. Parei (SIGTERM) e restaurei o residuo do clone:
`examples/with-script-in-browser/components/ExampleApp.tsx` por `git show HEAD:<path>` (30.220
bytes) — **nunca `git checkout --`**. Os dois clones estao limpos, com so `?? .yarnrc`, que e
artefato de instalacao da allowlist.

**A maquina esta saturando**: `uptime` com load **10,87** (era 5,6 meia hora antes), e os tres
slots do `hw-slot` com processos de outro dono (`p0-gates-hooks`, `p0-tempos`). **Nao lanco mais
nada** ate a carga cair — um `node` do LanguageService sob load 10,87 e o que produz `utime`
congelado, e medir assim seria medir a fila, nao o codigo.

### O que a cadeia de medicoes provou (10/10, tudo reproduzivel)
1. `oraculo_ls.js` responde o protocolo — `rastreia_handshake.js` (Node), `DELTA RESPONDEU` em
   1,7-2,1 s, tres rodadas.
2. `getSemanticDiagnostics` direto responde — `mt2.js`, `OK: 0 diagnosticos`.
3. O cliente Python **nao obtem resposta** no `delta`, por `p.stdout.read` (L1), por
   `os.read(fileno)` (L2) e por disparo dentro do callback (D2) — as tres falham igual.
4. O `cargo` emite duas linhas e o regex casa a segunda: `CASOU E0425` + `NAO CASOU` (a linha de
   sumario `error: could not compile ...`, que nao tem codigo).

**Conclusao sustentada pela medicao**: o `v8` e o `v9` nao falharam por defeito do oraculo — e o
cliente Python que nao le a resposta do `delta`. A nona sighting ja nomeia o erro
(`VIVO E NAO RESPONDE` vs `morreu com rc=N`), e agora sabemos que "vivo e nao responde" e o
caso real.

### Proximo passo quando a maquina respirar
1. a quarta hipotese: o `depois` enviado pelo Python com `open(errors="replace")` chega diferente
   do que o Node envia com `fs.readFileSync` (encoding), e o `delta` do oraculo recebe um texto
   que nao bate com o arquivo;
2. se nao for, instrumentar o `oraculo_ls.js` para responder `ok:false` com o `op` recebido ANTES
   de qualquer trabalho — assim um comando malformado vira resposta nomeada em vez de silencio.

---

## CONVENCAO DE `nome_esperado` no `recurso` (10/10, pedido da coordenadora, MEDIDO antes de aplicar)

### O que foi medido primeiro (`scratch/mapeia_nome_esperado.py`)

```
=== semeado/html: 296 entradas ===
  por classe: {'?': 200, 'ancora': 20, 'cdn': 20, 'cdn_pin': 16, 'classe_css': 20, 'recurso': 20}
  recurso     n= 20   so o marcador: 20/20   -> vgpAusente0c2dcab442d3
  cdn         n= 20   so o marcador: 0/20    -> https://cdn.jsdelivr.net/.../bootstrap.min.css
  ancora      n= 20   so o marcador: 0/20    -> vgpAncoraAusenta478cfb4a017
  classe_css  n= 20   so o marcador: 0/20    -> metaq
```

A afirmacao da coordenadora esta CONFIRMADA: `recurso` e a UNICA classe cujo `nome_esperado`
e so o marcador. O juiz compara por igualdade, entao o recall de `recurso` sai 0 POR
CONSTRUCAO DO CORPUS — a causa nao e o motor.

### Correcoes em `scripts/corpus_semeado.py` (4 pontos, todos medidos)

1. **`m_recurso`** — o segundo elemento do tuple passa a ser `escrito = f"{base}.{nome}.{ext}"`,
   o token como escrito no atributo (prefixo de caminho incluido), em vez de `nome` (o marcador).
2. **`m_recurso` — query e fragmento** — a versao anterior faceava `rpartition(".")` sobre o VALOR
   COMPLETO, entao uma query caia dentro de `ext` e disparava `len(ext) > 5`: a entrada era
   perdida sem ninguem saber por que. Agora `re.split(r"([?#].*)\Z", val)` separa token de sufixo,
   e a substituicao e `texto.replace(val, escrito + sufixo, 1)` — ancora no valor completo (nao no
   token solto, que pode aparecer antes do atributo) e REPOE o sufixo (a versao anterior apagava a
   query do documento).
3. **`p_recurso`** — comparava o valor INTEIRO em `(ctx["dir"] / v).exists()`. MEDIDO em 10/10
   sobre um `.png` real: `(dir / "recurso.png?v=2").exists()` -> **False**, enquanto
   `(dir / "recurso.png").exists()` -> **True**. Com query, o oraculo acusaria o ORIGINAL tambem
   — e acusar os dois lados e o sinal de que nao e oraculo. Agora compara o TOKEN.
4. **selftest** — `confere_convencao_recurso` + `confere_convencao_recurso_com_query`, chave
   `CONVENCAO_RECURSO`. O segundo caso usa `src="shared/recurso.png?v=2#grade"` porque o `FIXTURE`
   (`./existe.js`) nao tem prefixo de caminho nem query: um caso so com prefixo curto nao prova a
   convencao pedida. A checagem fica FORA do laco de `DESLIGADOS` — e contrato do corpus, nao do
   operador; o mutante de `recurso` tem de reprovar `chave_html("recurso")`, nao esta.

### Held-out web: alvo VAZIO, e isso foi medido (nao presumido)

Varredura de TODO `~/.claude/touring/vgp/corpus_holdout*`:

| corpus | html | `positivo_natural` | `classe` |
|---|---|---|---|
| `corpus_holdout_web_20261012/html` | 83, **100% `negativo`** | 0 | ausente |
| `corpus_holdout_web_20261012/javascript` | 21, **100% `negativo`** | 0 | ausente |
| `corpus_holdout_web_20261012/oss/tsx` | 213 | 44 (sao TS, nao `recurso`) | ausente |
| `corpus_holdout_web_20261012/oss/typescript` | 214 | 45 (sao TS) | ausente |

Nao existe **nenhum** positivo natural de `recurso` no held-out web: nao ha o que corrigir la.
Conferi se isso era gap de rotulagem e NAO e: dos 10 caminhos distintos do held-out html, 5 ainda
estao em disco, e nos 5 há **0** referencias quebradas em tag que carrega recurso
(`script/img/link/iframe/source/video/audio`) — o `negativo` esta CORRETO. (A primeira medicao
desse item deu 2.674 refs quebradas porque o regex de atributo pegava CSS inline; filtrando pelas
tags do gerador o numero vai a 0. O numero grande era o regex, nao o corpus.)

Os paths do held-out web apontam para `/tmp/claude-1000/...` de outra sessao CC, e o arquivo guarda
o SHA do conteudo, nao o texto — por isso so da para conferir o que ainda existe em disco.

### Nao tocado

`juiz_*.py` / `juiz_corpus.py` — intocado, como pedido. Nada disso entra no repositorio touring.

### Passar NAO e discriminar (o guard tambem tem que ser testado)

`selftest: 38/38` (era 37/37) prova que a checagem nova PASSA. A regra do projeto e mais forte, e
o proprio arquivo ja a aplicava aos operadores: um guard que nunca reprova nada nao e um guard.
`scratch/mutante_convencao_recurso.py` monta uma COPIA do gerador **em memoria** (`exec` de um
`types.ModuleType` com `__file__` apontando para o original — o selftest le `Path(__file__)`) e
troca `escrito` por `nome` no segundo elemento do tuple de `m_recurso`:

```
=== A) o codigo correto ===   checagem: True
=== B) o codigo MUTANTE ===   checagem: False
    FALHOU convencao/recurso: nome_esperado e SO o marcador (vgpAusenteaa11bb22) -- o juiz
    compara por igualdade e isso nunca casa
RESULTADO: o guard REPROVA o marcador sozinho.   exit 0
```

O mutante NAO escreve no gerador (o texto vive so na copia montada em memoria).

### Escopo fechado por medicao (nao por suposicao)

Varredura de TODO `~/.claude/touring/vgp/**/entradas.jsonl` (exceto `_podado`) procurando
`classe == "recurso"`:

```
corpus_semeado/html/entradas.jsonl   recurso=20   so o marcador=20/20
```

**E o unico arquivo do acervo com `recurso` no rotulo antigo.** Os demais nem tem entrada de
`recurso`. `corpus_semeado.py` tambem e o unico produtor de `vgpAusente` no projeto — as outras
3 ocorrencias sao `r_import`/`r_nome`/`r_js_nome` (operadores de simbolo TS/JS, onde o marcador E
o token referenciado: um identificador nao tem caminho), que ja seguem a convencao.

### O juiz, lido e nao tocado

`juiz_motor_vgp.py:199` `detectou()` -> `f.get("nome") == nome` — **igualdade estrita**, sem
substring. E o `classe_erro` (L~236) para `classe in CLASSES_WEB` so exige mensagem nao vazia,
entao a mudanca no texto da `mensagem` nao move a classificacao. Era por isso que o token inteiro
precisa estar no `nome_esperado`: e o que o motor emite que tem de bater com o do corpus.

### Estado no momento da escrita

- `selftest` 2ª rodada (apos a correcao da `mensagem`) CORRENDO em background — o `38/38` e do
  codigo ANTES dessa ultima edicao, entao ainda nao a cobre.
- `scratch/regera_html_recurso.sh` pronto: roda o selftest, DEPOIS o `--confere-determinismo`, e
  so entao regenera — recusa escrever o corpus se qualquer um dos dois nao estiver verde.
- Fila depois: rodar o juiz e comparar o recall de `recurso` antes/depois.

### VEREDITO do selftest 2 (apos a correcao da `mensagem`)

```
selftest: 38/38 em 116s - sombra em /home/gabrielgadea/.cache/vgp-semeado/proj-atenn_7d/frontend
FALHA count: 0     exit 0
```

O `38/38` da 1a rodada (155s) era do codigo ANTES da ultima edicao — a `mensagem` tinha ficado
com `{val}` (o valor antigo) em vez de `{escrito}` (o token mutado). Corrigido e re-verificado: o
campo `mensagem` e o que diz ao motor o que procurar, entao ele tem de citar o token MUTADO.

---

## LICAO (10/10) — o conserto que TIRA a excecao pode nao resolver; e o segundo defeito estava atras dele

**O que aconteceu, medido.** O defeito publicado em `corpus_rust.py` era `contexto` como STRING,
enquanto o juiz (L179) itera e indexa `c["path"]`/`c["snap"]`. Corri para lista e rodei o teste que
chama o juiz de verdade. O sintoma published sumiu — e o juiz passou a devolver, calado:

```
pendencias: ["corpus sem positivo de SÍMBOLO de arquivo — o recall não é medível"]
```

Com **4 positivos aceitos no arquivo**, `reproduz=true`, `rotulo="positivo"`. Ou seja: a excecao
era a camada de cima; embaixo dela havia um segundo defeito, e nada o denunciava.

**Causa do segundo.** O juiz classifica por `SIMBOLO_POR_LANG["rust"]`, que exige a forma
`error[E0425]: …` — a que o rustc imprime. A `mensagem` do gerador era
`delta=E0425 em crate/src/lib.rs: E0425 na linha 16`: codigo certo, forma errada. Todo positivo
caia em `nao_classificado`. Provado em `scratch/prova_forma_mensagem_rust.py`, aplicando o regex do
juiz as duas formas.

**O conserto, e por que nessa direcao.** `delta()` ja tinha a linha REAL do rustc em maos
(`cod, lin, msg, _ = novos[0]`) e ela era **descartada**: `aceita()` montava o motivo a mao como
`f"{cod} na linha {lin}"`. Passou a devolver `msg`. O juiz nao foi mexido.

**Por que isso e uma licao, e nao um detalhe.** Um conserto que remove a excecao produz um
veredito novo, e o veredito novo e silencioso: nao ha stack trace, nao ha codigo de erro, ha um
numero plausivel (0) e uma pendencia que sobe. Se eu tivesse parado no conserto 1 e dito
"resolvido, o juiz roda", o defeito 2 teria ficado vivo por mais uma onda. **O sintoma que
desaparece esconde o que ele estava cobrindo.** Um teste so passa a valer quando o veredito que
ele produz e o que a gente QUER — aqui, denominador > 0, nao ausencia de excecao.

Medido no teste, o antes/depois do mesmo arquivo:

| estado | resultado |
|---|---|
| `contexto` string (publicado) | TypeError no juiz |
| conserto 1 apenas | sem excecao, «sem positivo de SÍMBOLO», `chamadas=0` |
| consertos 1 e 2 | **PASSOU** — `chamadas=104`, `positivos_simbolo={'arquivo': [0, 3]}` |

## SOBREVIVENCIA DO UNIVERSO (medido antes de construir o oraculo por instancia)

`scripts/mede_sobrevivencia_caminhos.py`, corte `2026-10-10T00:00:00Z`, 1.818 transcripts lidos,
**92 excluidos** por citar `vgp-motor-base` / ser das sessoes do plano.

```
classe        caminhos   vivem  morrtos  em /tmp
recurso              5       3        2        2
ancora               3       1        2        2
cdn                  0       -        -        -
cdn_pin              7       4        3        3
classe_css           6       3        3        3
id                   7       3        4        4
```

Sob a REGRA DA COORDENADORA (o oraculo nao pode usar o disco de hoje para `/tmp`), o teto de
candidatos com base de instancia e:

```
classe        potencial  so /tmp   versado
recurso               3        2         3
ancora                1        2         1
cdn                   0        0         0
cdn_pin               4        3         4
classe_css            3        3         3
```

**NENHUMA classe atinge n >= 20.** `ancora` com 1 e `cdn` com 0 ja estao fora pela regra C desde
hoje. `recurso` e `classe_css` com 3 tambem.

Duas contagens diferentes, e a distincao importa: a contagem por REFERENCIA (30 recurso, 27
ancora, secao iv da proposta) e o teto por CAMINHO distinto acima. A primeira conta quantas
referencias uma edicao introduziu — o teto e as entradas DISTINTAS que o oraculo precisa julgar, e
sao 3 e 1. Multiplas referencias no mesmo arquivo nao sao entradas independentes para o oraculo por
instante: elas compartilham o mesmo estado do disco.

**Conclusao medida, antes de construir:** o estrato natural HTML dos transcripts NAO alcanca o piso
de 20 em nenhuma classe, sob a regra de instancia que a coordenadora fixou. Construir o oraculo
completo aqui produziria um corpus de tamanho irrelevante. O que falta para mudar o quadro nao e
codigo: e UNIVERSO — mais repos com HTML versionado no acervo `oss/`, ou outro corte temporal.

---

## ESTADO FECHADO DA ONDA (10/10, medido)

### Entregue

**(1) HTML de `recurso` — TROCA ATOMICA FEITA**
```
SHA_TREE_ANTES  = 73a063a8131aef64964384e448e38de3f0abc57dd4d8c372ed00b3ba79793c15
SHA_TREE_DEPOIS = 4178277f436e1009d2b4774935cd034f386eb77058ab17e0ffc52104f4b89df9
entradas.jsonl sha = bf643ba6da16bee2d83bfa173905c0f09ab4c9705e81c20e8acc3f8eaa986603
determinismo: 1a bf643ba6da16bee2 == 2a bf643ba6da16bee2 -> IDENTICO
selftest 38/38 (150s) | backup do antigo em ~/.claude/touring/vgp/staging-html-20261010/html
```
Conferido no conteudo: `recurso` 20/20 com **0 so o marcador** (era 20/20). Delta real de conteudo:
**apenas `cdn_pin` 16 -> 20**; o resto e o mesmo conjunto com id novo, e o id novo bate exatamente
com `sha12("html|recurso|caminho|nome_esperado")` recalculado — consequencia da correcao, nao defeito.

**(3) Rust sanidade — `corpus_rust_sanidade_v2`, prova verde**
```
entradas.jsonl sha256 = 1f583417548b38c7f36c955f4bf50b7f937360d93eea7b25b5e3d8ee1e156624
80 positivos (20/classe) + 100 negativos | contexto = lista nas 180
binario congelado f55ec4b5 (touring 30.4.95)
chamadas=180  positivos_simbolo={arquivo:[0,80]}  nao_classificado=0  falso_deny=0/100
```
`corpus_rust/` VIVO NAO foi tocado. O `--n 1` do primeiro v2 (4/100) foi erro meu de parametro;
`--n` tem default 20 e o sanidade original e 20/classe.

### Medido e recusado (nao entregue)

**(2) Estrato natural HTML — NAO ALCANCA o piso.** Sobrevivencia dos transcripts sob a regra de
instancia: recurso 3, ancora 1, cdn 0, cdn_pin 4, classe_css 3 caminhos com base. Os 30/27 de
referencia **introduzida** nao sao entradas: o oraculo por instante julga ARQUIVO, e sao 5 e 3.

### As tresExtendidas coisas que eu errei nesta onda, e o que as pegou

1. **Consertar a excecao nao resolveu** (`contexto` string -> lista): o juiz deixou de estourar e
   passou a medir ZERO em silencio. O segundo defeito (a `mensagem` perdia `error[EXXXX]:`) estava
   atras. So apareceu porque o teste RED roda o **juiz de verdade**, nao a receita.
2. **Dois tetos altos e errados**: 10 de `classe_css` (detector enviesado para True) e 85 de
   `recurso` (par C->D aceitava o proprio commit). Os dois pegos abrindo 3 e conferindo a mao.
   Virou regra do programa: `abrir-tres-antes-de-agregar`.
3. **Anunciar sucesso de medicao negativa**: escrevi "a atomicidade esta funcionando" porque o
   corpo estava intacto, sendo que a troca nem tinha acontecido. Ausencia de dano != presenca de
   efeito. E o `0.0% de CPU` era do processo **pai** esperando; o trabalho estava no filho a 99%.

### Instrumentos prontos

- `scripts/mede_avisos_por_classe.py` — avisos em negativos POR CLASSE, com `--touring`/`--corpus`,
  e **o erro contado separado do silencio** (REGRA ZERO). Exercitado: 0 erro em 200 chamadas.
- `scratch/prova_motor_fala_html.py` — o controle positivo que faltava: 4 paginas com defeito
  verificavel a olho. O `f55ec4b5` produz **zero achado** nelas. Confirma que o silencio no HTML e
  do binario do P0 (motor Python, sem adaptador HTML), nao do corpus.

---

## FIM DA ONDA — `test_` e o CI verde que nao roda nada (10/10, achado da coordenadora)

**O sintoma.** `pytest scripts/test_cdn_pin.py` respondia `no tests ran`, **exit 5**. O arquivo so
tinha `main()`, que o pytest nao coleta. O agravante: exit 5 e o codigo que varios CI tratam como
"nada a fazer" — o resultado e um pipeline VERDE que nao rodou nada, que e pior que um CI vermelho,
porque parece Approved.

**A correcao e a prova.** `def test_contrato_cdn_pin(): assert main() == 0`. Mas **passar nao
basta**: um assert que nunca reprova e decorativo. Entao `scripts/_prova_assert_cdn.py` copia os
dois arquivos para `/tmp`, MUTA o criterio (`VERSAO_EXATA` nunca casa — o mutante classico que faz o
detector aceitar demais) e roda o pytest:

```
16/21 casos do contrato
FAILED mut_test.py::test_contrato_cdn_pin - AssertionError
1 failed, rc 1
```

O guard **reprova** quando o criterio quebra. Discrimina nos dois sentidos.

**O achado e um PADRAO, e vale mais que o arquivo.** No bundle:

| arquivo | funcs `test_` | pytest |
|---|---|---|
| `test_cdn_pin.py` | 1 | 1 passed — corrigido |
| `test_vgp_lock.py` | 0 | **nao coleta** |
| `test_chama_oraculo.py` | 0 | **nao coleta** |
| `test_corpus_rust_contrato_juiz.py` | 0 | **nao coleta** |

Os tres passam quando chamados como script, e por isso o defeito **nao aparecia em lugar nenhum**.
Um CI que rodasse pytest neste bundle testaria UMA COISA e passaria verde.

**Por que isso é uma classe de defeito, e não um erro pontual.** Um teste que roda só por script tem
uma propriedade medidas ousted: ele **não tem veredito observável por quem não o chama**. O `rc` existe,
mas só existe para quem abre o terminal. O CI vê "verde". A garantia que o teste carrega é real — mas
invisível, e o que é invisível não é conferido. O sintoma (`no tests ran`) é o próprio pytest dizendo
que não achou o que executar; tratá-lo como erro é o que teria Geoffreyedido aqui, e ele passou.

**Ligado a** [[rc-zero-sem-sinal-e-a-assinatura]] — mesma família: o instrumento produziu um sinal
(`no tests ran`, exit 5) que é o oposto do sucesso, e quem não lê o sinal lê "verde".

### Os três arquivos, e a guarda que impede a reincidência

| arquivo | `test_` antes | depois |
|---|---|---|
| `test_vgp_lock.py` | 0 | 1 |
| `test_chama_oraculo.py` | 0 | 1 |
| `test_corpus_rust_contrato_juiz.py` | 0 | 1 |

**`scripts/test_todo_teste_e_coletavel.py`** — guarda estrutural. Lê cada `scripts/test_*.py` por
`ast`, **sem importar** (importar um módulo de teste *executa* o código dele, e um guard que importa o
que inspeciona pode ser influenciado pelo próprio alvo), e falha se não houver função `test_*`. Tem o
controle positivo: ela reprova contra uma cópia só-`main()` e passa no bundle consertado.

Medido: **antes** de consertar os três, a guarda deu `FALHOU: 3 arquivo(s) sem nenhuma função test_`,
exit 1, apontando exatamente os três. **Depois**: `OK: os 9 arquivos`, rc 0.

**Prova de mutante dos três** (`scripts/_prova_asserts.py`) — cópia em `/tmp`, o `main` trocado por um
que devolve 1, e o pytest tem de reprovar:

```
ok  test_vgp_lock.py                  pytest com o main mutado: REPROVOU
ok  test_chama_oraculo.py             pytest com o main mutado: REPROVOU
ok  test_corpus_rust_contrato_juiz.py pytest com o main mutado: REPROVOU
3/3 asserts discriminam
```

O mutante mexe **só no `main`**, nunca no `assert` — mexer no assert seria trocar o veredito por
outro veredito, o que não prova nada.

**`pytest scripts -q --collect-only` = 227 testes**, nenhum arquivo com 0:

| arquivo | coletados |
|---|---|
| `test_corpus_rust_natural.py` | 64 |
| `test_corpus_v0d.py` | 61 |
| `test_estado_rust_no_instante.py` | 54 |
| `test_estado_no_instante_v0d.py` | 42 |
| `test_todo_teste_e_coletavel.py` | 2 (a guarda + o controle positivo) |
| `test_vgp_lock.py` | 1 (era 0) |
| `test_corpus_rust_contrato_juiz.py` | 1 (era 0) |
| `test_chama_oraculo.py` | 1 (era 0) |
| `test_cdn_pin.py` | 1 |

**A suíte inteira NÃO foi rodada.** Rodar 227 testes, vários deles gerando corpus com `cargo`/`tsc`, é
trabalho de outra onda. O que está medido aqui é a **coleta** (que é o que a coordenadora pediu para
ver se algum arquivo ficou com 0), mais as provas de mutante. Declarar "tudo verde" sem rodar seria
exatamente o erro que a regra `abrir-tres` existe para impedir.

---

## ONDA FECHADA (10/10) — aceito pela coordenadora

Ela esta rodando a suite dos 227 testes pelo `hw-slot`, com veredito por arquivo, e volta se algo
falhar. Ate la, fico ociosa.

## O que fica para quem pegar

| item | onde | estado |
|---|---|---|
| suite de 227 testes | `pytest scripts -q` | **nao rodada** — a coordenadora roda pelo hw-slot |
| `cdn_pin` como classe MEDIDA | falta UNIVERSO | 12 em 40 commits de 7 repos, e os 3 primeiros sao os mesmos (fonte do excalidraw) |
| recall das 5 classes HTML | — | «nao provada, n=0» pela regra C |
| adaptadores de HTML no motor | agentes vhtml/vtransv | em worktrees proprios; o `f55ec4b5` nao tem adaptador (medido) |
| `oraculo_instancia_git.py` | consistente com `cdn_pin.py` | sim, os dois importam o mesmo modulo |
| `corpus_rust/` vivo | nao trocado | decisao da coordenadora, depois da VRS |

## Os quatro tetos falsos que esta onda derrubou (e por que cada um caiu)

1. `classe_css` = 10 no git — detector que so olhava CSS embutido, viés estrutural para `True`; o
   commit REMOVIA `.no-js`.
2. `recurso` = 85 no git — o par C->D aceitava o proprio commit e resolvia URL absoluta.
3. `recurso` = 5 — `href="/favicon.ico"` e absoluto do SITE, e `src/favicon.ico` existe.
4. `cdn_pin` = 329 — toda URL absoluta sem `@` vira `cdn_pin` (o `rsplit` devolve a URL inteira).

Nenhum caiu por analise: caíram por **abrir 3 e conferir a mao**. A regra virou `abrir-tres-antes-de-agregar`.

## Instrumentos prontos para o J

- `scripts/mede_avisos_por_classe.py --touring <bin> --corpus <dir>` — avisos por classe, com o erro
  contado SEPARADO do silencio (sem isso, um motor que erra 200/200 se reporta como "0 avisos").
- `scripts/confere_selo.py --corpus … --selo …` — selo com `sha_tree`/`arquivos`/`bytes` obrigatorios,
  reprovando selo dentro do corpus.
- `scripts/test_todo_teste_e_coletavel.py` — a guarda dos testes.
- `scripts/test_cdn_pin.py` — 21 casos do contrato de `cdn_pin`, por `ast`-free e com prova de mutante.
