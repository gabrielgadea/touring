# Os defeitos que o analise relatou — 18/09/2026 (30.4.57 → 30.4.58)

A sessão `analise-0e` mediu cinco defeitos do Touring 30.4.57 no repositório analise
(Python, poliglota) e os relatou com evidência. Gabriel mandou corrigir tudo, em
cooperação com o analise. Esta nota registra o que cada defeito era de fato. Em dois
casos a causa não era a relatada, e a correção achou quatro defeitos a mais.

## D4 — a recusa do `rm -rf` prometia uma rota que não existia

**Relatado:** a recusa dizia "pass --dry-run or scope an explicit path you have
verified", e nenhum predicado olhava caminho.

**Medido:** a rota era falsa de dois jeitos. O `rm` não tem `--dry-run`: passar a flag só
faz o `rm` sair com erro. E ninguém verificava caminho. Pior, o bypass lia **substring do
texto cru** (`trimmed.contains`), então `rm -rf / # --dry-run` passava pelo validador
estrutural. É o defeito "substring onde devia ser palavra" que a 30.4.56 corrigiu no
`PreToolValidator`, vivo no segundo sítio.

Ponta a ponta, o agente não tem rota nenhuma para apagamento recursivo:
- `rm -rf` é recusado pelas duas camadas;
- `rm -r` e `rm -f` são recusados pelo schema do `PreToolValidator`;
- `find -delete` é recusado.

O que passa é `rm <arquivo>` e `rmdir <dir>`.

**Correção:** a rota tem fonte única, `touring_hooks_shared::bash_ast_validator::DELETE_ROUTE`.
Toda recusa de apagamento das duas camadas a carrega (as regras estruturais, o prefixo
`rm `/`rmdir `, e o schema do `rm` e do `bash -c`). Cada regra declara `route` e
`route_examples`. O bypass passou a ser **palavra** dos tokens limpos (`discloses_intent`).
A guarda D8 cruzada `every_delete_refusal_names_an_open_route` roda cada exemplo de rota
pelas **duas** camadas e exige que passe. Liberar apagamento recursivo seria mudança de
política, e ficou como proposta. O hook do Claude Code aceita `"ask"`, que escalaria a
chamada ao usuário em vez de negar.

**Prova:** o teste `a_bypass_word_in_a_comment_or_string_does_not_bypass` fica vermelho com o
bypass antigo (verificado).

## D3 — memória aposentada voltava pelo recall

**Relatado:** `memory store --supersedes` marca `superseded_by`, mas o `recall` (caminho ANN)
e o `memory query` ainda devolviam a entrada.

**Medido:** só o braço SQL do recall filtrava. Onze leitores serviam aposentadas:
- no recall: os canais ANN e TF-IDF, e os `cases`;
- o `memory query`, pelo FTS e pelo universo de tags;
- `rlm` `search`, `scan_prefix`, `query_by_palace` e `query_tags`;
- o corpus dos MOCs;
- as "lições de erros passados" que os hooks injetam;
- o `memory reindex`, que devolvia a aposentada ao corpus ANN.

**Correção:** o módulo `touring_intelligence::rl::memory::retirement`, no dono do schema,
com `live_predicate` (SQL), `retired_keys` e `retirement_of` (por chave), é usado por
todos esses leitores. O recall descarta as aposentadas de **todos** os canais antes dos
`cases` e da fusão RRF, com over-fetch no ANN (40 → 20). É a prática documentada pelo Qdrant:
o filtro entra na busca, não no top-k depois dela. A decisão por chave segue a precedência
federada do braço SQL: o primeiro banco que tem a chave decide por ela. Leituras exatas
por chave (`get`, `memory list`) continuam vendo a aposentada, porque aposentar não é
apagar.

**Prova:** `memory_query_never_serves_a_retired_entry` usa o caminho real (store → query),
e fica vermelho sem os filtros (verificado).

## D1 — arestas cruzadas entre linguagens

**Relatado:** o `wiring repair` é só-Rust, mas roda em qualquer projeto. No analise, 18.035
linhas `.rs → não-.rs` escondiam cerca de 1.488 órfãos Python.

**Medido (read-only no banco do analise):** das 18.035 arestas, só **2.277** (`ast_resolved`)
eram do repair, a execução de 18/09. As outras **15.758** (`ast_inferred`, de 16/09) vinham da
**inferência por nome do próprio rebuild**: `find_producer_modules_with_kinds` casava um nome
solto num `.rs` com um produtor de qualquer linguagem. Um `cosine_similarity(…)` num
benchmark Rust virava consumidor da função Python de mesmo nome. Um `index rebuild` recriaria
tudo. O repair tinha ainda dois defeitos:
- atribuía o consumidor por substring (`No` era "achado" dentro de `NodeId`);
- gravava o palpite como `ast_resolved`.

**Correção:**
- **Família de linguagem, fonte única no `touring-storage`** (`language_family`,
  `language_family_sql`, `cross_language_edge_sql`: rust/python/js/java/go). A inferência só
  casa produtor da família do consumidor, nas consultas por nome e na qualificada.
- **Repair:** trata só produtor Rust e conta os demais (`skipped_non_rust`); casa por palavra;
  grava `ast_inferred`; `--dry-run` mostra até 10 arestas.
- **Comando novo `touring wiring repair --purge-cross-language [--dry-run]`:** remove toda
  aresta entre famílias e reinsere a linha NULL de cada produtor que ficou sem linha, pelo
  caminho de escrita do produtor, numa transação.

**Prova:**
- `the_by_name_inference_never_crosses_languages` fica vermelho sem o filtro, com
  cruzamento nos **dois** sentidos (verificado).
- `the_repair_wires_only_rust_producers_by_the_word_they_import`.
- `purge_removes_cross_language_edges_and_restores_the_orphans`.
- No analise, o predicado mede 18.035 arestas, 1.488 produtores e 210 órfãos a restaurar,
  que são os 210 símbolos da execução do repair.

## D2 — `orphans` e `impact` se contradiziam depois de dividir um módulo Python

**Relatado:** depois de dividir `grafo_memoria.py` em irmãos atrás de uma fachada e fazer
o ingest, 13 símbolos viraram órfãos, e `impact No` dizia "5 consumidores".

**Medido:** quatro mecanismos.
- (a) O `clear_wiring` apaga só as linhas NULL. A aresta anterior à divisão
  `(grafo_memoria.py, No, grafo.py)` ficou presa à fachada, onde o `impact` a encontra pelo
  nome e o `orphans` nunca olha.
- (b) O `definer_module` seguia só o `pub use` do Rust. Um consumidor que importa pela
  fachada Python era creditado a ela, **inclusive no rebuild**.
- (c) O caminho dos hooks (edição e ingest) lia `imports_json` por um separador `::`, então
  nenhum import Python ou TS resolvia até o próximo rebuild.
- (d) O `python_source_roots` só oferecia a raiz do pacote e o **cwd do processo**. Um
  `from memoria.grafo import No` fora do pacote só resolvia num daemon nascido na raiz. É o
  defeito da 30.4.55 do Rust, na forma Python.

`Aresta`, na mesma instrução de import que `No`, escapava por outro motivo: uma
autorreferência (`internal_only`), não pelo import.

**Correção:**
- (d) Raiz do projeto derivada do **arquivo**, pelo primeiro ancestral com `.touring`,
  `.git`, `pyproject.toml`, `setup.py` ou `setup.cfg`, mais `src/` e os ancestrais dentro do
  projeto. É a ordem que o Pyright documenta para imports absolutos.
- (b) `definer_module` segue a cadeia `from x import Símbolo` em módulos `.py`
  (`ModuleFacts::parse_python`, com o mesmo cache por mtime e o mesmo limite de profundidade).
- (c) O laço de imports do rebuild virou a função única
  `touring_hook_runtime::wiring::record_import_consumers`, chamada pelo rebuild e pelo
  `refresh_file_wiring` (C08).
- (a) Depois de re-registrar os produtores, as linhas de consumidor presas a um símbolo que
  o módulo Python não define mais são reatribuídas ao definidor (se o módulo o encaminha) ou
  removidas (se não o define nem importa). Em módulos Rust nada muda, porque lá uma linha
  presa a um `mod.rs` pode ser evidência legítima de consumo.

**Prova:** `a_python_module_split_keeps_every_consumer_on_the_definer`, pelo caminho real:
rebuild, divisão, ingest dos três arquivos e um novo rebuild. Todos os consumidores de `No`
terminam em `modelo.py`, o rebuild e o ingest chegam às **mesmas** arestas, e `No` não é
órfão.

## D5 — `learning status -j` com stdout vazio

**Medido:** o `learning` não aceitava `-j` nem `--json`. O clap saía com código 2, o uso ia
para o stderr e o stdout ficava vazio. Pela CLI também não era possível dar penalidade:
`learning reward <tool> -0.5` lia `-0` como flag desconhecida, verificado na 30.4.57.

**Correção:** `-j`/`--json` é global no `learning`. Com ele, o stdout é JSON mesmo em falha
(`{"error": …, "hook": …}`, exit 1). O `reward` passou a aceitar números negativos.

## Defeitos achados no caminho

1. **A resolução de reexport Rust nunca funcionava no caminho de edição.** O `resolve_reexport`
   juntava a raiz do crate sob a raiz do workspace sem checar se ela já era absoluta, e o
   caminho de edição a entrega absoluta (`absolute_consumer`). Resultado: `<root>//<root>/…`.
   O teste `a_reexported_path_wires_the_real_producer_not_a_phantom` passava ou falhava
   conforme o ambiente: com `TOURING_WORKSPACE_ROOT` exportada ele exercitava o caminho
   absoluto e falhava; sem ela (o ambiente do juiz), passava. Agora a regra tem uma cópia só
   (`under_workspace`), e o teste afirma as duas formas.
2. As 15.758 arestas da inferência (D1).
3. O bypass por substring do validador estrutural (D4).
4. A penalidade impossível pela CLI (D5).

## Operação no analise (handoff)

1. `touring update --project ~/projects/analise` para 30.4.58 (feito pelo
   `propagate-release.sh`).
2. `touring wiring repair --purge-cross-language --dry-run`, que deve medir 18.035 arestas e
   210 órfãos a restaurar. Depois, a execução real.
3. O `orphans_base` do juiz de lá vai subir, porque órfãos Python que estavam escondidos
   aparecem. O rebaseline tem que ser declarado com evidência.
4. Não rodar `index rebuild` com a 30.4.57, porque ele recriaria as arestas da inferência.

**Executado pelo analise, com o aval do Gabriel:** `{"status":"purged","edges":18035,
"symbols":1488,"orphans_restored":210}`. O banco bateu com a aritmética:
- total de 157.087 para 139.262 arestas;
- NULL +210;
- zero cruzadas;
- 88.916 da mesma família intactas.

Depois do ingest dos módulos divididos, os órfãos do escopo caíram de 13 para 2.

## 30.4.58 publicada, e o juiz que não chegava ao fim

O juiz `--rust-full` da 30.4.58 levou quatro execuções até o exit 0. Nenhuma das três
falhas era do código da rodada.

- **r1:** morto pelo systemd-oomd em 4 min. A unit tinha `MemoryHigh=24G`, que não mata:
  estrangula, e o reclaim empurrou 44,5 GB para o swap. A pressão de memória do cgroup
  disparou o oomd, com 48 GB livres na máquina. O teto macio fabricou o gatilho da morte.
- **r2:** o binário de teste do `touring_cli` morreu com SIGILL dentro de
  `ts_query__analyze_patterns` (C do tree-sitter). No core, o pc aponta para
  `sub %edx,%eax`, um opcode válido e alvo alinhado de dois saltos, e o mesmo binário passa
  isolado (579/0). No mesmo dia o kernel registrou 2 machine checks corrigidos (`Bank 0`,
  status `8000004000040005`, paridade interna) em dois núcleos, e o boot anterior tinha
  caído às 14:08 sem desligamento limpo. A máquina é um i9-14900HX (CPUID `0xb0671`,
  microcode `0x137`). Leitura: falha de execução do processador, com confiança 0,75,
  escalada ao Gabriel.
- **r3:** travou 25 min. Dois testes do `inferlets` andavam pelo `/tmp` da máquina, e as
  fixtures mortas no r1 e no r2 (o `Drop` do `TempDir` nunca rodou) tinham deixado ali
  ciclos de symlink. O walker usava `Path::is_dir`, que segue link, e a recursão ficou
  exponencial.
- **r4:** exit 0, com 8 jobs, 8 threads de teste e o walker corrigido. A 30.4.58 foi
  publicada em `b716be65..bcf76a0a`.

## 30.4.59 — os resíduos do purge

O analise relatou quatro resíduos e um defeito do CEG. Um relato inverteu a leitura, e a
investigação achou mais três defeitos.

**A linha NULL é a declaração do produtor.** O analise viu `grafo_modelo.py <- NULL` ao lado
das quatro linhas de consumidor de `No` e leu como resíduo. Não é: essa linha é a declaração,
uma por símbolo público (`knowledge_wiring.rs`, `producer_rows`), e o `record_consumer` lê dela
o kind e a visibilidade. Quem violava o contrato era o `wiring repair`, que a apagava depois de
gravar consumidores. Isso deixava produtores sem declaração e ensinou a expectativa oposta. O
repair deixou de apagar, e a varredura de órfãos continua `NOT EXISTS` um consumidor.

**Importar não é usar (decisão de Gabriel, 18/09).** `supersede` ficava fora da lista de
órfãos porque uma fachada o importava só para reexportar em `__all__`.
- **Ponto único:** `ast_bridge::extract_consumed_imports` decide o que vira consumidor.
  - Em Rust, subtrai os reexportes (`pub use`, `pub(crate) use`).
  - Em Python, subtrai os nomes nunca referenciados fora das instruções de import. Isso
    cobre fachada com `__all__`, `__init__` e import morto. O alias é julgado pelo nome
    local, e atributo, nome de keyword e string não contam como referência.
- **Três sítios reexportavam como consumo, todos corrigidos:**
  - a passada de imports do rebuild;
  - a varredura de caminhos diretos da edição, que lê linhas `use`;
  - o FIX-4, que gravava `pub use submod::X` como uso do pai, só na edição.
- **Um quarto sítio, achado pelo teste de ponta a ponta:** a passada de tipos por nome
  capturava o `scoped_identifier` de dentro do próprio `pub use`.
- **Um quinto, achado pelo dry-run com o binário instalado:** o grep do `wiring repair`
  aceitava `(pub )?use` de propósito. Rodá-lo recriaria os consumidores que a decisão
  removeu. Agora ele lê só `use` sem visibilidade.
- **Efeito medido no touring** (rebuild da 30.4.59): órfãos de 735 para 773 (+38, todos
  Rust), que são os símbolos que só um `pub use` mantinha vivos. O juiz tolera até 2481.
- **Reexporte usado no próprio arquivo é uso.** O primeiro juiz da 30.4.59 reprovou com
  11 órfãos novos: os handlers do `touring-assists`. O `handlers/mod.rs` faz
  `pub use add_missing_match_arms::ADD_MISSING_MATCH_ARMS;` e lista a constante na própria
  tabela `("add_missing_match_arms", ADD_MISSING_MATCH_ARMS)`. Nenhuma passada por nome vê
  identificador solto em expressão, e antes o `pub use` cobria esse uso por acaso. Agora
  `rust_names_used_outside_use` mantém como consumo o reexporte que o arquivo também nomeia
  fora dos `use`. A regra vale nos DOIS sítios que liam o reexporte. Na passada de imports
  ela não bastou, porque o caminho `add_missing_match_arms::X` é relativo ao módulo filho e o
  resolvedor não o localiza a partir de um `mod.rs`: a aresta só existe pela passada de tipos
  por nome. Por isso a passada de tipos descarta a captura de dentro de um reexporte apenas
  quando o nome não é usado no resto do arquivo. O segundo ciclo mediu, e os 11 continuavam
  órfãos até essa segunda parte.
- **A edição Rust usa a passada de imports do rebuild.** Ela lia o `imports_json`, uma regex
  (`^use\s+([\w:]+)`) que nunca viu `use a::{B, C}` nem esse caso. Isso era uma assimetria C08
  anterior a esta rodada. Um teste que afirmava o contrato antigo, com conteúdo vazio e
  `imports_json` sintético, foi reescrito com arquivos reais e uma lista entre chaves.
- **Fonte única do predicado:** `imports::is_reexport_declaration`, usada por
  `rust_reexports` e pelo filtro da passada de tipos. Um `use` comum segue alimentando a
  passada de tipos, que serve de rede quando o resolvedor falha.

**Método Python por atributo.** `no.tags_cli()` e `no.todas_as_arestas` eram os únicos usos
de dois métodos, e o Python não tinha a passada F9. `python_method_calls.scm` captura todo
atributo, e `find_python_method_producers` só credita produtor de kind `method` em módulo do
qual o consumidor já importa. Como uma aresta de método só nasce dentro dessa trava, ela nunca
abre a porta para outro palpite.

De passagem: a passada `alias.Nome` (B5) só existia no rebuild. A edição apagava essas arestas
com as inferidas e nunca as rederivava. Agora ela é a função única
`record_python_qualified_uses`, segue o definidor e roda antes da passada de métodos, que a lê
como trava.

**A amostra que existia para ser lida.** A família `wiring` liga `--brief` por padrão, e a
amostra de 10 arestas do repair passa dos 512 bytes, então virava `{"_elided_array_len": 10}`
no dry-run, justamente onde serve. O repair agora imprime por `bounded_reply`, sem elisão.

**CEG.** Morte por sinal virava `-1` anônimo. `SandboxResult.signal` e `describe_signal`
nomeiam o sinal e o cap (SIGXFSZ para o limite de tamanho de arquivo, SIGXCPU para o de CPU,
SIGKILL para memória). O caso do analise foi diferente: o sqlite3 morreu no cap, o script seguiu
com `echo $?` e o envelope reportou o exit 0 do programa, o que é fiel ao shell. O que faltava
era dizer que um arquivo tinha sido cortado. O `RunTmp` agora lista os arquivos com tamanho
EXATO do cap, e isso vira `OutputLimit` mesmo com exit 0.

**Defeitos achados no caminho.**
1. `require('` tem 9 caracteres, e o corte do `find_circular_imports` pulava 8, então todo
   `require('x')` virava módulo vazio.
2. A edição perdia as arestas `alias.Nome` (acima).
3. A passada de tipos contava o caminho de um `pub use` (acima).
