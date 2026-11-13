# Glossário do domínio — Touring

Termos que nomeiam seams do código. Um termo entra aqui quando um módulo passa a ser
chamado por ele; o código usa o equivalente em inglês indicado.

## Leitura da linha de shell (`touring-hooks-shared::command_policy::lexer`)

**Linha de shell** — o texto inteiro de um comando Bash como chega ao hook, podendo
conter vários comandos simples, pipelines, heredocs e substituições.

**Comando simples** — uma sequência de palavras entre operadores de controle
(`;` `&&` `||` `|` `&` nova linha), já sem redireções.

**Programa efetivo** (*effective program*) — o programa que um comando simples de fato
executa, depois de descartar atribuições de ambiente (`A=1`, `A+=1`) e de atravessar
wrappers (`sudo`, `env`, `timeout`, `nice`, `xargs`…) com suas flags e argumentos.
`A=1 sudo -u x timeout .5 rm -rf d` tem programa efetivo `rm`. Um único módulo responde
isso; todo gate que julga ou classifica um comando consome essa resposta, nunca uma
tabela de wrappers própria.

**Programa efetivo dentro de uma cláusula** — `effective_command_of`: o programa efetivo
depois de ler através dos abridores de cláusula (`do`, `then`, `else`, `elif`, `{`, `!` e a
condição de `if`/`while`/`until`, que é ela mesma um comando simples), com `None` para o que
não executa nada (`for`/`select`/`case`, `done`, `fi`, `esac`, `}`). `for f in *; do cat $f |
head; done` tem os comandos simples `cat` e `head`. `command_of` segue sendo a leitura crua
que os gates e o juiz lexical usam; a chave de lição (`bash_verb`) e a suspeita de pipe
mascarado (`post_bash`) perguntam o programa por esta função, e só por ela.

**Wrapper** — programa cujo papel é executar outro programa recebido como argumento.
A tabela é única e é a **união** do que qualquer consumidor reconhecia: ler através de
mais wrappers só revela o programa, nunca o esconde. Um consumidor que precise ler
menos declara isso como parâmetro com razão nomeada e teste.

**Corpo de heredoc** — os bytes entre `<<TAG` e a linha terminadora, que são dado e não
comando (salvo delimitador sem aspas com substituição `$(…)` dentro). `<<<` é
here-string, não heredoc: não abre corpo.

## Espelho de sinais (`touring-code::sdk_signal_mirror`)

**Espelho de sinais** (*signal mirror*, `Mirror`) — o diário append-only, uma linha JSON por
chamada de hook observada (`ts`, `hook_name`, `duration_ms`, `success`, `origin`), em
`~/.claude/touring/sdk_signal_mirror.jsonl`. Uma interface só: `Mirror::record` grava uma
chamada e `Mirror::read(since_epoch)` devolve o agregado. O formato da linha, o caminho
padrão, o conjunto canônico e os percentis moram atrás dela, e o código de `touring-code`,
`touring-cli`, `touring-hook-handlers` e `touring-server` (Rust) só chega ao arquivo por ela.
Há **duas exceções**, ambas nomeadas e com motivo: o escritor Python do sandbox
(`touring-server/src/cli/run.rs`, outra linguagem, que grava a mesma linha por conta própria,
guardado por um teste que fixa `origin`) e `touring-quality/src/builtins/best_practices.rs`,
que monta o caminho e lê `hook_name` à mão porque o crate precisa continuar compilável sozinho
(`--no-default-features`) e `touring-code` não é dependência dele; a cópia dos oito nomes que
ele mantém é guardada por `touring-server/tests/sdk_hook_names_parity.rs`. A leitura é por
bytes, linha a linha: a linha que não é UTF-8, não é JSON ou não tem `hook_name` é pulada e
contada em `skipped_lines` — uma linha torta nunca esconde as demais nem some em silêncio —
e `None` significa "ainda não há arquivo", que não é o mesmo que um espelho vazio nem que um
espelho ilegível (`Err`).

**Agregado do espelho** (*mirror aggregate*, `MirrorAggregate`) — o que o espelho diz, já
somado: chamadas, hooks canônicos distintos, chamadas fora do cânone, chamadas por origem,
linhas puladas e p50/p99 de duração. Dado puro; o trabalho está em `Mirror::read`.

**Hook canônico** (*canonical hook*) — um dos oito nomes de `HookName::ALL`. Um alias do
daemon (`cli-index-find`) conta em `non_canonical_calls`, visível, e nunca em
`canonical_hooks_used`: foi assim que a razão leu 1,0 com só 3 hooks canônicos.

**Origem** (*origin*, `MirrorOrigin`) — quem escreveu a linha: `post_bash` (o alimentador
do PostToolUse) ou `sdk` (o SDK Python do sandbox). Linha antiga, sem origem, é
*desconhecida* e nunca é creditada ao post-bash.

## Parse de código (`touring-code::ast::grammar`)

**Porta de parse** (*grammar door*, `ast::grammar`) — o único módulo que responde "este
caminho, ou esta língua, é lido por qual gramática, e a árvore sai dentro do orçamento ou
com o motivo de não sair". Uma tabela de extensões (`EXTENSIONS`), um `Parser::new()`
(`new_parser`), um cache de `Parser` por thread e um orçamento de tempo (2 s + 2 s/MiB).
Consumidor nenhum monta parser próprio; por isso nenhum esquece o orçamento, e um teste
reprova um segundo `Parser::new()` no crate. `Grammar::parse` devolve a árvore ou uma
`Refusal`: `Halted` (estourou o orçamento) ou `Rejected` (o runtime recusou a gramática).

**Gramática** (*grammar*, `Grammar`) — uma gramática tree-sitter. Há uma a mais que
`ast::Lang`, porque `.ts` e `.tsx` são duas (`<T>x` é cast numa e elemento JSX aberto na
outra). A escolha é tomada em dois pontos nomeados, e só neles: `Grammar::of_extension`
(sensível ao dialeto: quem tem o caminho, como o VGP e o polyglot) e `Grammar::of_lang`
(cego ao dialeto: `Lang::TypeScript` não diz se veio de `.ts` ou `.tsx`, então lê TSX, o
superconjunto que lê os dois).

**Alcance** (*reach*, `Reach`) — quais linhas da tabela de extensões um leitor aceita.
`Exact`: só extensões que SÃO a língua; quem emite veredito (VGP, polyglot) precisa, porque
um `.scss` lido como CSS seria reprovado por sintaxe que o CSS não tem. `WithNeighbours`:
também as de dialeto ou superconjunto que a gramática vizinha lê (`.scss`, `.mdx`,
`.jsonc`); quem extrai estrutura (`ast::Lang::from_path`) tolera.

## Escrita do índice (`touring-code::ast::graph`, `touring-code::ast::store`)

**Chave de módulo** (*module key*, `module_key`) — o texto que um import nomeia (`utils`,
`crate::a::b`): é `DependencyEdge::to`, a chave de `reverse_deps` e a coluna
`dependencies.to_file` do SQLite. Não é caminho de arquivo. Tem um par, a **chave de
arquivo** (*file key*): o caminho como o chamador indexou (`pkg/utils.py`), que chaveia
`file_to_symbols` e `dependencies` e é `symbols.file_path` / `dependencies.from_file` no
SQL. A ponte entre as duas é uma função só, `module_key(file_path)`: o stem do nome do
arquivo (`pkg/utils.py` → `utils`). É heurística e está dita assim: um import `crate::a::b`
nunca casa o stem `b`. `SymbolIndex::importers_of` e `SymbolStore::get_reverse_deps` leem
por ela. Onde cada stem tem um dono só, memória e SQLite respondem "quem importa este
arquivo" do mesmo jeito; com homônimos (`a/utils.py` × `b/utils.py`) divergem de propósito: a
memória só credita o stem ao ÚNICO arquivo indexado que o tem (aos homônimos, a ninguém), e o
SQL, que não conhece "arquivos indexados", credita os importadores aos dois (teste
`memory_and_sqlite_part_ways_on_a_homonymous_stem`). Quem consulta `reverse_deps` por
caminho de arquivo lê a chave errada.

**Aresta resolvida** (*resolved edge*) — a aresta cujo destino é a chave de ARQUIVO do que
foi importado (`src/foo.rs`), não o texto do import (`crate::foo`). A extração nunca a produz;
o wiring a produz (`SymbolIndex::load_file_edges`, `ast::store::load_index`), e é por isso que
ela resolve o `crate::foo` do Rust e o stem homônimo (`a/utils.py` × `b/utils.py`). Em
`importers_of` a chave de arquivo vence, e o stem só responde quando o arquivo é o ÚNICO
indexado com aquele stem. Um arquivo que a fonte de arestas cobre não tem as arestas trocadas
pelas cruas quando é reindexado (`index_file` atualiza só os símbolos). Uma aresta exige
EVIDÊNCIA DE IMPORT (F13, 13/11/2026): o par consumidor→produtor entra no grafo quando ao menos
uma de suas linhas do `wiring_map` não é `ast_inferred` (um `use` que o resolvedor mapeou, uma
resposta do compilador, uma declaração). Casar um nome nu com um produtor (o palpite que impede
um SÍMBOLO de parecer órfão, teto de 4 por nome) não é dependência entre arquivos; a regra é uma
só (`file_edge_filter!`, em `touring-storage/src/knowledge_wiring.rs`) e os três leitores a
dividem. O uso de um alias de import em Python (`import m as x` + `x.Nome`) conta como evidência
(F16): o caminho do módulo vem do próprio `import`.

**Cobertura** (*edge coverage*, `Importers`) — o conjunto de arquivos pelos quais uma fonte de
arestas (o wiring) responde. Uma lista vazia de importadores não diz se o arquivo é *isolado*
(a fonte o olhou e nenhum import RESOLVIDO aponta para ele: um zero da fonte, não a afirmação de
que nada depende do arquivo) ou *sem cobertura* (a fonte nunca o olhou: "não sei"). `SymbolIndex::importers_status` devolve a diferença, tipada:
`Known(&[..])`, `Isolated` ou `Uncovered`. Um índice sem fonte de arestas responde pelos
arquivos que guarda, como sempre. Ausência de sinal não é zero.

**Geração do índice** (*index generation*, `IndexGeneration` / `IndexSlot`, em
`touring-hook-runtime/src/index_slot.rs`) — o índice de dependências carregado no daemon e o
cache HNSW que descreve ESSE índice, trocados juntos como uma unidade atrás de um `Arc`.
`IndexSlot::get` entrega a geração (zero cópia por leitura); `replace` — a primeira carga, sob
demanda, na primeira leitura que precisa do grafo (`HookRuntime::index_generation`; o daemon não
carrega ao abrir o projeto), e o recarregamento no fim de `index rebuild`, que só acontece se já
há uma geração — instala outra, com o cache vazio; `edit` — o hook que refaz as arestas
de um arquivo editado — copia ao escrever se algum leitor ainda segura a geração. Quem leu uma
geração termina com ela. Não é a `index_generation` do SQLite (o selo `building`/`complete`/
`aborted` do rebuild, I16): são conceitos distintos que só dividem o nome.

**Caminho de escrita do índice** (*index write path*) — indexar, reindexar e remover um
arquivo são um verbo cada (`index_file`/`index_files`/`remove_file` em memória;
`bulk_index`/`replace_file_symbols`/`remove_file` no SQLite), e todos escrevem pelos mesmos
passos privados: em memória `insert_symbol`, `insert_edge` e `withdraw`; no SQLite
`put_symbol_row`, `put_edge_row` e `delete_file_rows`, dentro de uma `Transaction` que
desfaz ao cair sem commit. Três regras o sustentam: `file_to_symbols[f]` lista os nomes das
definições de `f`; `reverse_deps[m]` lista o `from` de toda aresta com `to == m`; nenhum
mapa guarda lista vazia. É por isso que indexar e remover devolve o índice ao estado
anterior. Reindexar **substitui** o que o arquivo contribuiu (antes acumulava duplicata), e
remover só corta as arestas de quem o importa quando nenhum outro arquivo indexado responde
pela mesma chave de módulo (`a/utils.py` e `b/utils.py` dividem `utils`).

## Referências do arquivo (`touring-code::ast::graph::references`)

**Referências do arquivo** (*file references*, `FileReferences`) — o que um arquivo de código
diz sobre outros nomes, perguntado a UMA interface: `FileReferences::new(fonte, língua)` e um
método por pergunta (`imports`, `consumed_imports`, `forwarded_paths`, `inferred`,
`python_qualified_uses`, `module_references`, `module_declarations`, `self_references`). Os
extratores de `ast::graph` (`imports`, `method_calls`, `python_uses`, `module_paths`,
`self_refs`) são puros e privados ao crate; quem decide o que CONTA como referência é a
interface. Escrever a resposta no banco, e a ordem dessas escritas, continua do consumidor
(`wiring.rs`, o rebuild): é política de gravação, não regra de referência. Língua sem regra
responde vazio — o chamador não precisa saber que uma passada é "só Rust" — e fonte que não
parseia também.

**Importar não é usar** (*consumed import*, `consumed_imports`) — decisão de 18/09/2026: o
import que é USO é o import menos o que o arquivo só encaminha ou nunca toca. Em Rust, um
re-export (`pub use`) que o arquivo não nomeia no próprio código sai, e fica se ele também o
usa (`pub use a::X;` e `X` numa tabela); em Python, um `from x import Nome` cujo `Nome` não
aparece fora das declarações de import (fachada `__all__`, `__init__.py`, import morto) sai.
Saem SÍMBOLOS, nunca módulos. **Caminho encaminhado** (*forwarded path*, `forwarded_paths`) é
o `módulo::símbolo` de todo `pub use`: o que uma varredura de caminhos não pode ler como uso.

**Referência inferida** (*inferred reference*, `InferredReferences`) — a que o arquivo faz por
nome, sem import que a resolva: nomes chamados, tipos e constantes em posição de tipo, e o par
`(qualificador, nome)` de cada chamada qualificada. É adivinhada por nome, ao contrário do
import, que é resolvido; por isso as três saem juntas e o consumidor as grava juntas como
`ast_inferred`. Uma declaração nunca é referência a si mesma.

## Vista de projeto do VGP (`touring-code::vgp::lang::vista`)

**Vista de projeto** (*project view*, `Vista`) — o projeto como um adaptador de língua o vê:
os **overlays** da entrada (caminho → conteúdo, os arquivos no instante da edição) postos por
cima do disco. Uma interface só responde "existe / é arquivo / é diretório / o que diz"
(`existe`, `e_arquivo`, `e_diretorio`, `ler`, e `sondar` quando o chamador precisa do motivo),
e é ela quem carrega as regras que cinco adaptadores (Rust, Python, TypeScript, CSS, HTML)
copiavam. O disco só é tocado por `caminho`, dentro das raízes que o chamador passa; a vista
não toca nele por conta própria.

**Precedência** (*precedence*) — o overlay vence o disco, e o overlay não depende das raízes:
quem o declarou foi o chamador, não o conteúdo analisado. O teto de tamanho de leitura é do
disco (`limite` de `ler`), nunca do overlay. Não existe "o overlay apaga o arquivo": o mapa
`HashMap<String, String>` não sabe dizer isso e nenhum adaptador pediu.

**Chave de overlay** (*overlay key*) — o caminho absoluto lexicamente normal do arquivo (sem
`.`, `..` nem `//`). A consulta normaliza o que recebe: `./a.rs` e `sub/../a.rs` são o
arquivo `a.rs`. Só a chave é normalizada; o caminho que vai ao disco é o do chamador. Antes
da vista três adaptadores normalizavam no chamador e dois consultavam como estava escrito:
um `#[path = "./x.rs"]` não achava o overlay de `x.rs` e acusava "módulo declarado sem
arquivo".

**Diretório implícito** (*implied directory*) — existe diretório em `p` se o disco diz, ou se
algum overlay está sob `p/`: os arquivos que a entrada cria numa pasta nova fazem a pasta.
Era regra só do TypeScript; o HTML descartava `/assets/a.png` porque `assets` não estava no
disco.

## Pipeline incremental do daemon (`touring-code::ast::incremental_pipeline`, `touring-code::ast::file_heat`)

**Calor** (*heat*, `HeatMap` / `FileHeat`) — quanto o usuário trabalha num arquivo:
`edições × decaimento por recência × (1 + peso de blast)`, meia-vida de 24 h (o peso de blast
não tem produtor hoje: é sempre 0 e o ranking é edições × recência). Quem grava é
`post_edit` (edição) e `pre_read` (leitura); o mapa nasce, a cada boot do daemon, semeado
do histórico de edições (`edit_history`), então o calor sobrevive a um restart. A chave é
SEMPRE o caminho relativo ao projeto — a mesma grafia do índice, do pipeline e do histórico —
e é por ela que o calor ranqueia os documentos do pipeline.

**Orçamento do pipeline** (*pipeline memory budget*, `with_memory_budget_mb` /
`enforce_budget`) — o teto, em MB, para os bytes de rope dos documentos que o pipeline do
daemon mantém carregados. Ausente, o knob `TOURING_PIPELINE_MEMORY_MB` deixa o teto sair da
máquina (`max(64, floor((H/8 − 260)/1,43))`, H = guarda de memória do rebuild; uma medição,
um projeto); `0` = sem teto; valor inválido = o default, com aviso no log. `touring status -j`
diz quem decidiu (`budget_source`: `env`, `default`, `disabled`). Quem chama
`enforce_budget` é quem acabou de dar um arquivo ao pipeline e tem o calor na mão; o
pipeline não decide sozinho. O teto conta rope (`memory_usage_mb` ≠ RSS): o cache de árvores
de parse tem teto próprio (LRU de 128), e medido neste workspace a RssAnon é ~260 MB fixos
mais ~1,43 MB por MB de rope.

**Evicção por calor** (*heat-ranked eviction*) — a única política de saída do pipeline: o
arquivo que acabou de ser processado nunca sai; entre os demais, sai primeiro o que não tem
calor, depois o de menor score, depois por ordem de caminho (determinístico). A vítima perde
documento, árvore e símbolos em cache: a próxima edição dela é um reparse completo, nunca um
incremento sobre estado velho.

**Lote determinístico** (*the heat never orders a batch*) — `index rebuild` e
`file-knowledge populate` andam na ordem do walker (caminho), qualquer que seja o calor do
daemon: as tabelas que deixam, rowids incluídos, não dependem do que o usuário editou (I16).
O calor decide QUEM SAI do pipeline (evicção) e O QUE se pré-aquece, nunca a ordem em que um
lote escreve. Uma fila "quente primeiro" (`PrioritizedPipeline`) fez exatamente isso em
12/11/2026 e os rowids de `graph.db`/`symbols` divergiram entre um rebuild frio e um quente;
a fila saiu, e `heat_invariant::assert_tables_independent_of_heat` guarda os dois lotes.

**Pré-aquecimento** (*prefetch*, `prefetch_from_heat`) — ao começar a sessão, ler do disco os
`N` arquivos mais quentes e parseá-los, para que a primeira edição de cada um seja
incremental. Respeita o orçamento: só desloca arquivos mais frios que os que carregou.

