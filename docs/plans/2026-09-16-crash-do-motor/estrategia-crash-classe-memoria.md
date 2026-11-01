# Estratégia: da caçada a um crash para a capacidade de pegar a classe

> Ordem de Gabriel (16/09/2026): *"pesquise as melhores práticas no context7 e elabore uma
> estratégia para uma solução definitiva e que, além disso, permita expansão e escalabilidade"*.
> Nada aqui foi implementado — é o plano, com o que já foi medido e o que é hipótese.

## 1. O problema, no tamanho real

O motor `touring-quality` morreu duas vezes em 16/09 dentro de units do juiz: **SIGILL** às 01:00 e
**SIGSEGV** às 01:26, esta última com core (`SEGV_MAPERR`, thread worker, stack de **um** frame em
`0x4`). Fora do juiz: **0 crashes em 24 execuções** (5 workspace, 8 só-shell, 8 controle sem shell,
3 sob carga de cargo). A cronologia descarta "morreu junto com o cargo": o `cargo test --workspace`
começou 1 s DEPOIS do crash.

Dois sinais diferentes (SIGILL e SIGSEGV) no mesmo binário, com salto para endereço inválido, é
assinatura de **memória corrompida**, não de bug lógico. E o que o juiz mostrava era
`quality_gold tier=None` + "raise touring-quality to >= Gold" — a cláusula é fail-closed (certo), mas
a mensagem manda consertar o que não está quebrado.

**A lição que orienta a estratégia**: o custo desta investigação não foi achar a causa — foi não ter
**captura**. O juiz descarta o stderr do que julga; o wrapper não persiste nada; o core trouxe um
frame só. Foram precisas quatro passadas de juiz e um shim observador improvisado para chegar a
"rc=139". Uma estratégia que só cace este bug deixa o próximo igualmente caro.

## 2. O que a pesquisa (Context7) trouxe, e o que se aplica aqui

Consultado `/websites/rs_tree-sitter` e `/tree-sitter/tree-sitter`:

| Prática | Fonte | Situação nossa |
|---|---|---|
| Parser cancelado por progress callback **retoma de onde parou** salvo `reset()` explícito | docs.rs `Parser::reset` | **Já cumprido.** `parse_bounded_with` chama `parser.reset()` quando o parse não completa, e o comentário explica o porquê. A hipótese fácil está descartada por leitura. |
| Rodar a suíte sob **AddressSanitizer**: `CFLAGS=-fsanitize=address RUSTFLAGS="-lasan --cfg sanitizing" ASAN_OPTIONS=verify_asan_link_order=0 cargo test` | `docs/src/6-contributing.md` | **Não fazemos.** É a receita oficial do projeto cujo C nós vendorizamos. |
| Ao alternar entre modo sanitizer e normal, **limpar o target** | idem | Consequência de projeto: o perfil sanitizer precisa de `CARGO_TARGET_DIR` próprio, senão contamina o `target/` que o daemon e o juiz usam. |
| Scanner externo deve alocar com **`ts_malloc`/`ts_calloc`** (`tree_sitter/alloc.h`), nunca libc, para permitir override do alocador | `docs/src/creating-parsers/4-external-scanners.md` | **Violado**: `third_party/tree-sitter-bash/src/scanner.c` usa `calloc` (l. 1194) e `free` (l. 1222), zero `ts_*`, sem incluir `alloc.h`. |
| Estado dinâmico do scanner via `tree_sitter/array.h` | idem | Não auditado. |

**Honestidade sobre a violação do alocador**: ela é real, mas **não é causa provada** deste crash.
Medi: ninguém chama `ts_set_allocator` no workspace, e `touring-quality` não declara
`#[global_allocator]` — logo `ts_malloc` e o `calloc` do scanner resolvem para o mesmo alocador da
libc, e não há cruzamento hoje. O risco é **latente e condicional**: no dia em que alguém registrar
um alocador custom (o binário `touring` já compila com `jemalloc` nas features) ou trocar de versão
do scanner, o par alocar-aqui/liberar-ali vira corrupção de heap silenciosa — exatamente a assinatura
que estamos perseguindo. É dívida a pagar antes de doer, não o culpado de hoje.

## 3. Hipóteses vivas, e o que cada uma prevê

| # | Hipótese | Predição que a distingue | Custo de testar |
|---|---|---|---|
| H1 | Corrida de dados no motor (o score roda `par_iter` aninhado: dimensões × arquivos, `scope_report.rs`) | Com `RAYON_NUM_THREADS=1` o crash **some** num laço longo | ~8 min |
| H2 | FFI de tree-sitter: scanner vendorizado ou uso do parser sob concorrência | ASan/TSan sobre o corpus **nomeia** o sítio | ~1 h de setup |
| H3 | Memória do hardware | Crash reaparece em binários não relacionados; `memtest86+` acusa | uma janela ociosa |
| H4 | Reuso de parser por thread após corte de orçamento | Enfraquecida: o `reset()` está lá e é a prática correta | descartada por leitura |

Nenhuma está provada. A estratégia abaixo é desenhada para não depender de qual delas vence.

## 4. A estratégia, em quatro camadas

O princípio: **capturar antes de caçar, provar antes de corrigir, e deixar a capacidade instalada**.
Cada camada vale por si; juntas, fecham a classe.

### Camada 1 — Captura (o próximo crash já nasce diagnosticado)

Sem isto, toda camada seguinte recomeça do zero.

1. **O wrapper persiste o que o motor diz.** `scripts/touring-quality-score` hoje repassa o stderr e
   ninguém o guarda; um crash em CI desaparece. Gravar por execução em `~/.claude/touring/logs`:
   argumentos, duração, exit code, stderr. Custo: ~10 linhas de shell.
2. **Backtrace utilizável.** O core de hoje trouxe **um** frame porque o release não carrega símbolos
   suficientes. Ativar `debug = "line-tables-only"` + `split-debuginfo = "packed"` no perfil release
   do motor troca 1 frame por uma pilha nomeada, sem custo de runtime.
3. **Um crash vira artefato, não mensagem.** Um coletor que, ao ver exit ≥ 128, copia o core
   simbolizado e o stderr para o bundle da rodada.

**Critério de aceite** (mensurável): matar o motor de propósito com `kill -SEGV` durante um score
produz, sem intervenção, um artefato com pilha nomeada. Testável hoje, sem esperar o bug.

### Camada 2 — Reprodução determinística e barata

Um harness parametrizado por `(binário, corpus, matriz de ambiente, N repetições)` que reporta
**taxa**, nunca um caso isolado — o mesmo formato do censo que já rodei (`segv_census.sh`), só que
versionado e reaproveitável.

Primeira matriz, que testa H1 diretamente: `RAYON_NUM_THREADS ∈ {1, 2, 8}` × `N=20`. Se a coluna de 1
thread fica limpa e as outras não, a causa é corrida, e o escopo de busca cai de "o motor" para "o
que as dimensões compartilham".

**Critério de aceite**: o harness roda em qualquer crate com uma linha de configuração, e a saída é
uma tabela de taxas por célula.

### Camada 3 — Detecção proativa (sanitizers), com isolamento

Perfil `sanitize` no workspace, com a receita oficial acima e **`CARGO_TARGET_DIR` próprio** — o
aviso do tree-sitter sobre limpar o target não é conselho, é requisito: o `target/` compartilhado
serve o daemon, o juiz e as outras sessões, e contaminá-lo com artefatos instrumentados quebraria
tudo o que mede.

Ordem: **ASan** primeiro (pega corrupção de heap, que é a assinatura observada), **TSan** depois
(pega corrida, que é H1). Alvo: o corpus real dos três projetos, não fixtures — foi assim que o
patch do `tree-sitter-md` foi provado, e é o precedente desta casa.

**Critério de aceite**: `ASAN_OPTIONS=... cargo test` passa limpo no corpus, ou nomeia o sítio. Um
resultado ou outro encerra H2.

### Camada 4 — Prevenção estrutural (para não reincidir)

1. **Guarda de fonte para scanners vendorizados**: já existe uma que reprova classificador `<ctype.h>`
   (`PATCHES.md`). Estendê-la para exigir `ts_malloc`/`ts_calloc`/`ts_free` e a inclusão de
   `alloc.h` fecha a dívida da seção 2 **antes** de ela doer, e vale para todo scanner futuro.
2. **O juiz distingue não-medido de abaixo-do-piso** na mensagem. O veredito continua o mesmo
   (fail-closed); muda o remédio que ele ensina. Toca arquivo do juiz → exige atestação humana.
3. **ASan no CI sobre um corpus reduzido**, com o corpus completo em cadência semanal: o custo de
   ASan é ~2× tempo e ~3× memória, então a escala vem da amostragem, não do heroísmo.

## 5. Por que isto escala, e para onde expande

A pergunta do Gabriel foi por solução **definitiva, expansível e escalável**. O que faz esta
estratégia escalar não é nenhuma das quatro camadas isoladamente — é o fato de nenhuma delas ser
específica do `touring-quality`:

- **Por binário**: o harness da camada 2 e o coletor da camada 1 recebem o binário como parâmetro. O
  daemon (que já teve dois SIGSEGV sem autor registrado, memória `crash-log-nunca-aberto`) entra sem
  linha nova de código.
- **Por linguagem**: a guarda da camada 4 vale para qualquer gramática vendorizada — hoje bash e
  markdown, amanhã a próxima.
- **Por classe de defeito**: ASan pega corrupção, TSan pega corrida, e a mesma casca de execução
  serve UBSan ou Miri sem redesenho.
- **Por consumidor**: os artefatos da camada 1 alimentam o mesmo lugar onde o juiz e o `gate-metrics`
  já olham, então o dado novo não exige um painel novo.

O que **não** escala, e por isso está aqui: caçar cada crash por bissecção manual, e depender de um
shim observador improvisado a cada incidente. Foi o que esta madrugada custou.

## 6. Sequência sugerida

| Ordem | O quê | Por quê primeiro | Custo |
|---|---|---|---|
| 1 | Camada 1 (captura) | Sem ela, qualquer investigação recomeça do zero; e é a única que dá valor mesmo se o crash nunca voltar | ~1 h |
| 2 | Matriz `RAYON_NUM_THREADS` (camada 2) | Discrimina H1 de H2/H3 em 8 minutos de máquina | ~8 min + ~1 h de harness |
| 3 | ASan sobre o corpus (camada 3) | Encerra H2 com nome e linha, ou a descarta | ~1 h setup + execução |
| 4 | Guarda `ts_malloc` (camada 4.1) | Dívida barata, independente do resultado acima | ~30 min |
| 5 | Mensagem do juiz (camada 4.2) | Exige atestação humana — agrupar com outras mudanças do juiz | ~20 min |
| 6 | `memtest86+` (H3) | Só se 2 e 3 saírem limpos; descarta hardware antes de caçar UB inexistente | uma janela ociosa |

## 7. O que esta estratégia NÃO promete

- **Não promete achar a causa.** Promete que a próxima ocorrência chega com pilha, taxa e ambiente —
  e que ASan/TSan terão sido perguntados antes de qualquer palpite.
- **Não promete custo zero.** ASan no corpus inteiro é caro; a proposta é amostrar no CI e rodar
  completo em cadência.
- **Não descarta hardware.** H3 continua viva até um memtest limpo, e nenhuma quantidade de
  instrumentação de software a elimina.
