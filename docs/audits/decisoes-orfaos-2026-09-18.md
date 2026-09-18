# As decisões dos órfãos, executadas — 18/09/2026 (30.4.55 → 30.4.57)

DAG `task_1789740028970288075`. Continuação de `orfaos-24-2026-09-18.md`; decisões aprovadas por
Gabriel ("Execute a implementação das decisões, as quais estou aprovando; corrija e aperfeiçoe todos
os pontos e achados").

Boas práticas consultadas (Context7):

- **The Cargo Book, "Workspaces"** — o workspace de um membro é o primeiro `Cargo.toml` com
  `[workspace]` acima dele (ou `package.workspace`); nunca o diretório de quem pergunta.
- **MCP 2025-11-25, "Error Handling"** — falha de execução de ferramenta vai DENTRO do resultado com
  `isError: true`; erro de protocolo fica para ferramenta inutilizável.

## D1 — o resolvedor acha o workspace pelo arquivo

`symbol_extractors`: `find_workspace_root()` (cwd do processo, `OnceLock`) e o `Lazy`
`TOURING_CRATE_MAP` saíram. Agora `workspace_root_of(path)` implementa a regra do Cargo, com cache por
diretório; `Workspace { root, crates, aliases }` tem cache por raiz; e `workspace_for(source)` decide:
caminho absoluto → workspace do arquivo, e só dele (um arquivo fora de todo workspace não herda o
do processo); relativo → `TOURING_PROJECT_ROOT`, depois o cwd. `classify_unresolved(path, source)`
e `definer_module(module, symbol, consumer)` recebem a origem; o rebuild passa o caminho absoluto, e
o caminho de edição ancora o relativo na raiz do banco (`absolute_consumer`).

- Classe nova `unmeasured`: sem mapa de crates não há veredito — nunca `external`.
- Mapa vazio é avisado, uma vez por raiz.
- `[workspace]` é lido como cabeçalho de tabela: o `contains` antigo casava comentário.
- Pacote × alias: `is_package` substitui o prefixo fixo `touring_`, e vale para qualquer workspace.

O desenho mudou ao ler o código. A proposta de 18/09 manhã — `TOURING_WORKSPACE_ROOT` antes do cwd
— quebraria o konverter: essa variável aponta a FONTE do touring, não o projeto indexado.

Testes: um SEGUNDO workspace montado em diretório temporário (o `cargo test` roda neste repositório,
então só um resolvedor que lê o arquivo conhece os crates do fixture), arquivo fora de todo
workspace, workspace sem membros, cabeçalho `[workspace]`.

## D3 — a versão do plano é checada

`PLAN_SCHEMA_VERSION = "2.0"` e `GeneratorPlan::check_schema_version` (major igual; `2`, `2.1.3`,
`v2` passam; `8`, `1.0`, `""`, `2x` não). O gate mora em `PlanExecutor::verify` (Draft → Verified),
antes do VGP — todas as rotas (submit, verify, render, speculate, replay) passam por ali — e devolve
`ReplanRequest` com `FailureReason::SchemaVersionMismatch`, os dois variantes que existiam e ninguém
construía. A fixture `make_plan` carregava `version: "8"` pela suíte inteira; agora usa a constante.

## D4, D5 — remoções aprovadas

- `tools_status.rs` (esqueleto `StatusFamily` + `StatusInput` do W1.2 cujo `#[tool]`, W2.5, nunca
  chegou) removido, com os comentários que o declaravam "reachable public API".
- `touring-rkyv::templates`: saíram os 5 só-teste e a cascata exclusiva deles (8 structs, 13 testes de
  ida e volta). Ficam os 5 com consumidor em produção; os "Used by:" apontavam crates que não existem
  (`touring-learning`, `touring-index`) e agora apontam os reais.

## D7 — `success` é o veredito do programa

`format_output`: `success` = `exit_code == 0` e nenhuma falha taxonomizada (o `OutputLimit` inclusive:
dado perdido), `executed` = o executor produziu resultado. O adaptador MCP devolvia
`CallToolResult::success` para TODA execução; agora `error` (isError) quando o programa falhou, pelo
mesmo predicado (`program_succeeded`).

## D8 — o validador lê a linha como shell

Reproduzido pelo hook real (avalia, não executa): `git add crates/touring-foundation/…` negado
"Force operation" (`-f` dentro de `g-f`); `rm my-file.txt` negado; `git push --force … # --dry-run`
PERMITIDO (o desvio casava o comentário); `cd x && git push -f` nunca visto (parâmetros vazios).

`validate_command`: léxico com aspas, escapes e `#`; pipelines (`|`) dentro de listas (`;`, `&&`,
`||`, `&`); flag é PALAVRA — `-f` ou grupo `-rf`, nunca substring —, o desvio é palavra do próprio
comando, e padrão que atravessa `|` (`curl … | sh`) lê a pipeline inteira. A primeira versão separava
`curl x | sh` em dois comandos e os dois testes existentes de pipe-para-shell reprovaram — a
regressão de segurança foi pega pela suíte, e o desenho virou dois níveis. A negação nomeia a
palavra e o comando.

## D9 — auto-referência Rust e o caminho de edição

`self_refs::self_referenced_names(source, symbols, language)` é a fonte única do rebuild e da edição.
Para Rust, casar só o nome seria errado (`self.indels.is_empty()` é `Vec::is_empty`): método conta por
`self.m()`, `Self::m` ou `Dono::m`; o resto por identificador solto, nunca campo de `x.nome`, nem
segmento `a::nome` de outro módulo, nem o tipo de um cabeçalho `impl`. E o caminho de edição passa a
regravar essas arestas — antes só o rebuild as escrevia, então editar um arquivo rebaixava seus
símbolos internos a órfãos até o próximo rebuild (Python incluído, desde o B4).

### D9 refinado (30.4.56): chamada, não campo; o arquivo, não os testes

Uma amostra de 25 `internal_only` Rust sorteados (`seed 18092026`, evidência textual de uso no
próprio arquivo) achou duas classes de falso positivo, as duas corrigidas:

- **Campo homônimo**: `self.start` (campo) contava como uso do método `start`. `OnSelf` agora só
  conta quando o `field_expression` é a FUNÇÃO de um `call_expression` — `self.start()` sim,
  `self.start` não.
- **Uso só em teste**: um símbolo exercitado apenas pelo `mod tests` do próprio arquivo virava
  "interno". `is_test_item` pula `mod_item`, `function_item` e `impl_item` precedidos de `#[test]` ou
  `#[cfg(test…)]`: teste é consumidor de VERIFICAÇÃO, não de produção.

Os dois refinamentos mudam o comportamento do daemon: republicar a 30.4.55 com outro binário seria o
"rótulo não prova build" (`propagacao-rotulo-nao-prova-build`), então viram a **30.4.56**.

## D8 refinado — a negação do `rm` nomeia o que viu

A 30.4.55 passou a julgar cada comando da linha, e o prefixo estático `rm ` ainda lia texto cru com
`-rf|-r\s+|-f\s+`: `rm -f x` era negado "Recursive force delete" (recursão que não existia), `rm
some-r dir` casava `-r ` dentro do nome, e `rm -fr /` não casava nenhuma das três alternativas (o
schema o pegava, por sorte, com outra mensagem). O prefixo agora é `ParamCheck::Words`: dispara com
recursivo E forçado, por palavra (`-rf`, `-fr`, `-Rf`, `-r -f`, `--recursive --force`); um só dos dois
cai na regra do schema, cuja razão nomeia a flag (`Force without confirmation: flag -f`). `--`
encerra as opções (`rm -- -f` apaga um arquivo chamado `-f`), e `rmdir -p` é `--parents`. A política
não mudou: `rm -f` continua negado, agora pelo motivo certo.

A suíte existente pegou a primeira versão: `test_case_insensitive_rm` exige `RM -RF /` e `Rm -r /home`
negados. O prefixo sempre comparou o comando em minúsculas, mas o schema era buscado pelo nome exato
e o predicado novo diferenciava `-RF`. O NOME do comando agora é dobrado nos dois lugares; as FLAGS do
schema não — no git, `-F` é arquivo de mensagem e `-f` é force —, só as do predicado crítico do `rm`,
onde dobrar erra para o lado de recusar.

Relendo o `command_of` do D8 para baixar a complexidade (CC 30 no léxico, 18 no `command_of`, agora
um `ShellLexer` com um método por construção e `WRAPPERS` como tabela), apareceu um falso negativo:
`sudo -u root git push -f` tomava `root` como o comando, porque `-u` recebe valor. Cada wrapper
declara as opções que consomem o valor seguinte (`sudo -u/-g/-C/…`, `env -u/-C`, `timeout -s/-k`,
`time -f/-o`, `exec -a`); teste `a_wrapper_option_value_is_not_the_command`.

## D15 — dentro de uma macro não há árvore (30.4.57)

A geração 34 (30.4.56) trouxe um NEW que não estava nas decisões: `ImpactCategory::signal_prefix`.
Na 33 ele era `internal_only` só por causa do teste do próprio arquivo, que o D9 refinado deixou de
contar. Mas ele TEM consumidor em produção, em outro crate:

```rust
let weight = severity * category.signal_discount();                       // aresta: sim
signals.push((weight, format!("{}:{dep}", category.signal_prefix())));    // aresta: não
```

A gramática Rust do tree-sitter guarda os argumentos de uma macro como `token_tree`: tokens crus,
sem `call_expression`, `field_expression` nem `scoped_identifier`. Todo passe que pergunta à árvore
por esses nós era cego dentro de `format!`/`assert!`/`vec!`: o despacho de métodos (F9), as
referências de tipo/const por caminho e a auto-referência do D9 (onde `self.helper()` em `format!`
não contava e `x.campo` contava como uso solto). O passe de chamadas QUALIFICADAS já tinha o remédio,
só para si (`collect_qualified_in_macros`, varredura textual do D2).

`method_calls::macro_token` lê a forma pelos tokens vizinhos (confirmados na gramática 0.24.2: `.` e
`::` anônimos, `self` nomeado, palavra-chave anônima): `recv.nome(` é chamada por ponto (`on_self`
quando o receptor é `self`), `a::b::nome` é caminho (último segmento e raiz), o resto é solto, e
`called` exige um grupo `(…)` logo depois. Um classificador para os três passes; argumentos de
atributo (`#[cfg(any(test))]`) ficam de fora, porque não são macro; turbofish dentro de macro
(`x.f::<T>()`) segue não lido. O arquivo é parseado uma vez para a consulta e os tokens. O próprio
teste pegou um detalhe da gramática: `usize` em `std::usize::MAX` é `primitive_type`, não
`identifier`, e sem aceitá-lo como segmento a raiz `std` nunca era alcançada.

## As três gerações, medidas

Mesmo código-fonte indexado, três binários. Mesma consulta em todas (`consumer_type='rust_import'`
para as arestas).

| | geração 33 (30.4.55) | geração 34 (30.4.56) | geração 35 (30.4.57) |
|---|---|---|---|
| `touring_*` gravados como `external` | 2 | 2 | 2 |
| `unmeasured` | 22 | 22 | 22 |
| arestas `rust_import` | 86.584 | — | 95.192 |
| órfãos | 860 | 1.083 | **735** |
| `internal_only` | 2.605 | 2.382 | 2.514 |
| NEW contra a baseline | 2 | 3 | **2** |

A 34 mostra o refinamento do D9 em ação: 223 símbolos saíram de `internal_only` (campo homônimo, uso
só em teste) para órfão — já estavam na baseline, menos um, `signal_prefix`, que era o D15
escondido. A 35 tira 348 órfãos falsos (todo método, tipo ou const usado só dentro de macro) e põe
8.608 arestas no grafo. Python fica em zero `internal_only` aqui porque a coleta Python exige
`polyglot`, e o touring não é polyglot — condição anterior a esta rodada.

## D6 — o que fica como API por desenho

Por decisão de Gabriel (18/09, "Execute a implementação das decisões, as quais estou aprovando" sobre
a proposta que mantinha `CognitiveMCTS` e os `is_empty` como API), os dois NEW restantes entram na
baseline do juiz (`~/.claude/loop-engineering/baselines/43224dc4d9af/.baseline/orphans-scoped.txt`,
2.479 → 2.481; cópia anterior em `orphans-scoped.txt.bak-20260918T-D6`):

- `crates/touring-generator/src/source_change/mod.rs::is_empty` — par de `len` na API de
  `SourceChange` (clippy `len_without_is_empty`);
- `crates/touring-intelligence/src/reasoning/cognitive_mcts.rs::CognitiveMCTS` — alias público do
  planejador, nome estável da API.

O segundo `is_empty` da proposta (`TextEdit`, `source_change/text_edit.rs`) não é mais NEW nem órfão:
é `internal_only`, usado pelo próprio arquivo (`TextEdit::apply_all`, desde 94d28445).

## Achado para decisão — o F9 conta teste como consumidor

O D9 refinado deixou de contar o uso num item `#[test]`/`#[cfg(test)]` do próprio arquivo como
auto-referência. O passe F9 (despacho de métodos por nome) segue contando: um método chamado só pelo
`mod tests` de outro arquivo, ou do mesmo, ganha consumidor (`get_dependencies`, `with_constraint`).
Alinhar o F9 ao D9 é coerente, mas desloca centenas de símbolos de "consumido" para órfão e move a
baseline do juiz, então fica para Gabriel decidir.

## D10 — `proc-macro-error2`

Aviso future-incompat do `proc-macro-error2` 2.0.1 (a versão mais nova), trazido por sete macros de
terceiros (`leptos_macro`, `leptos_router_macro`, `reactive_stores_macro`, `validator_derive`,
`defmt-macros`, `iai-callgrind-macros`, `syn_derive`). Nenhum código do touring o dispara; a
correção é dos upstreams.

## Observado durante os gates

- `clippy-driver` morreu com SIGSEGV uma vez no touring-server; o mesmo comando passou em seguida,
  sem core registrado — crash transiente do toolchain (rustc 1.98), não do código.
- Os cores de SIGSEGV/SIGABRT do binário de teste do touring-hooks-core são de
  `panic_log::crash_path_tests::crash_child`, o filho que o teste do log de crash derruba de propósito.

## Gates

`cargo check --workspace --all-targets --all-features` (touring-server com as features compatíveis) ·
`cargo clippy … -D warnings` no workspace e no touring-server · suítes completas de touring-hooks-core,
-cli, -hook-runtime, -hook-handlers, -generator, -rkyv, -code e -server: 0 falhas (5.649 + 666 na
reexecução).

30.4.56: clippy `-D warnings` do workspace + suítes completas de touring-hooks-core (656), -code
(764), -hook-runtime (457), -hook-handlers (700) e `stringzilla_e2e` (13): 2.590, 0 falhas.
30.4.57: clippy do touring-code + suítes completas de touring-code (768), -hook-runtime (457),
-hooks-core (656) e touring-cli (579): 2.460, 0 falhas no estado final (a primeira passada do
touring-code reprovou o teste do `primitive_type`, corrigido e repassado). As duas releases saíram por
`propagate-release.sh` com exit 0: gates, prova comportamental 40/40, analise e konverter no lock novo,
e o daemon conferido pelo relógio (subiu depois do binário gravado).
