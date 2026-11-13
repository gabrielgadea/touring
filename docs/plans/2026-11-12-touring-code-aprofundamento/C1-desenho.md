# C1 — Uma porta de parse por língua (nota de desenho)

Linha de base (11/11→12/11, `cargo test -p touring-code --no-fail-fast`, exit 0): lib 1451 ok/2 ign; 26 binários de teste; total 1716 passed, 0 failed, 8 ignored.

## Interface (`crates/touring-code/src/ast/grammar.rs`, tudo `pub(crate)`)
```rust
pub(crate) enum Grammar { Python, Rust, TypeScript, Tsx, JavaScript, Bash, Html, Css, Markdown, Json, Toml, Yaml, /*more-languages*/ Go, Java }
pub(crate) enum Reach { Exact, WithNeighbours }            // quem lê a tabela de extensões
pub(crate) enum Refusal { Rejected(String), Halted }       // o motivo de não haver árvore
impl Grammar {
  fn of_lang(Lang) -> Grammar;                  // CEGO ao dialeto: Lang::TypeScript -> Tsx
  fn lang(self) -> Lang;                        // Tsx|TypeScript -> Lang::TypeScript
  fn of_extension(&str, Reach) -> Option<Grammar>;  // SENSÍVEL ao dialeto: ts -> TypeScript, tsx -> Tsx
  fn of_path(&Path, Reach) -> Option<Grammar>;
  fn name(self) -> &'static str;  fn from_name(&str) -> Option<Grammar>;   // "tsx", "typescript"… (VL0 §2)
  fn language(self) -> tree_sitter::Language;
  fn parse(self, &str) -> Result<Tree, Refusal>;   // a porta: cache único + orçamento
}
fn parse_str(&Language, &str, Option<&Tree>) -> Result<Tree, Refusal>;                 // old_tree: incremental
fn parse_chunks<T, F>(&Language, usize, &mut F, Option<&Tree>) -> Result<Tree, Refusal>; // rope
```
Atrás do seam: UMA tabela `EXTENSIONS` (linha = ext minúscula, gramática, vizinha?), UM `Parser::new()` (`new_parser`),
UM cache `thread_local<HashMap<Language, Parser>>` (chave = ponteiro da gramática, `Language: Hash+Eq`), `parse_budget` (2 s + 2 s/MiB)
e o `halt` por progress callback (movidos de `ast/parser.rs`). Reentrada (parse chamado de dentro do callback do leitor)
cai num parser avulso em vez de entrar em pânico no `RefCell`.

## Decisões
- **ts × tsx decidido uma vez, em dois pontos NOMEADOS do mesmo módulo.** Quem tem caminho (VGP, polyglot) usa `of_extension`
  (`.ts/.mts/.cts` -> TypeScript, `.tsx` -> Tsx). Quem só tem `ast::Lang` (cerca de 100 chamadores de `from_path`, 8 crates) usa
  `of_lang`: `Lang::TypeScript` não carrega o dialeto, então lê TSX, o superconjunto que também lê `.ts` sem JSX. É limitação
  de `Lang` (um `Lang::Tsx` quebraria os `match` exaustivos externos), com razão e teste, não uma segunda tabela.
- **Alcance** (`Reach`): extensões que SÃO a língua × extensões de dialeto/superconjunto lidas pela gramática vizinha
  (`.scss`, `.mdx`, `.jsonc`). `ast::Lang::from_path` usa `WithNeighbours` (comportamento atual); VGP (veredito) e polyglot usam `Exact`.
- Caixa da extensão é política do leitor (preservada): `from_path` casa a caixa escrita; `lang_do_caminho` e `detect_lang` minusculam antes.
- VGP: `lang_do_caminho` = `of_extension(.., Exact)` + "tem adaptador no REGISTRO"; `gramatica`/`parse` = `from_name` + o mesmo filtro.
  Mensagens de erro em português preservadas. `EXTENSOES` e `PARSERS` saem.
- polyglot (outro motor): `detect_lang` pergunta a tabela compartilhada primeiro (identidade), cai em `Lang::from_str(ext)` só para as
  línguas que só o ast-grep tem (c, cpp, rb, kt, lua…). O parse dele não passa pela porta.
- `IncrementalParser` perde o mapa próprio de `Parser` e usa a porta; `Debug` perde o campo `languages`; a struct passa a ser `Send`.
- `surgery::detect_language` (heurística de conteúdo) só era chamada por funções SEM caminho (`replace_symbol_body`, `get_symbol_range`,
  `extract_symbol_body`); as com caminho já usam `Lang::from_path`. Vira `guess_lang_from_content`, privada, documentada como tal.
- `languages::tiers::Language` fica: é a matriz de maturidade (lista línguas sem gramática aqui: Kotlin, Swift, Ruby…). Só ganha um
  teste que prende seus nomes aos de `Grammar::name`.

## Chamadores que mudam (grep em `crates/`)
- `parse_bounded`/`parse_bounded_with` (pub(crate)): 13 chamadas (imports.rs 3, self_refs.rs 2, python_uses.rs 2, module_paths.rs 1,
  method_calls.rs 1, go_wiring.rs 1, polyglot_semantic.rs 1, surgery.rs 1, vgp/lang/mod.rs 1) + 2 `parser.parse(..)` CRUS em
  method_calls.rs (`extract_qualified_calls_inner`, `names_imported_from_other_crates`: SEM orçamento — ganham). Passam a `Grammar::parse`.
- `Parser::new()` em `src/`: 18 sítios -> 1 (`grammar.rs::new_parser`). Guarda estrutural: teste varre `src/` e reprova um segundo.
- Contratos externos inalterados (assinatura): `Lang::from_path` (103 chamadas, 13 grupos), `polyglot::{Lang, detect_lang}` (4 chamadores de
  produção: touring-cli/src/cli/polyglot.rs:255, polyglot/scan.rs:161, touring-generator/src/validate/polyglot.rs:30,
  touring-server/src/server/tools_metadata.rs:414), `vgp::lang_do_caminho` (5 externos), `ParserPool`, `IncrementalParser`, `incremental_parse`.
  `vgp::lang::gramatica` (pub, 0 chamadores fora de `vgp/lang/mod.rs`) sai: `parse` passa a resolver por `Grammar::from_name`.
- `ast/mod.rs` ganha `pub(crate) mod grammar;` (uma linha; arquivo sem dono entre os candidatos).

## Defeitos que o aprofundamento conserta (RED -> GREEN)
1. `Lang::from_path("a.mts"|"a.cts")` era `None` no ast e `typescript` no VGP; agora uma tabela só: `Some(TypeScript)`.
2. `polyglot::detect_lang` não reconhecia `.jsx .mjs .cjs .pyi .zsh .mts .cts` (a tabela do ast e a do VGP já os tinham).
3. Dois parses de `method_calls.rs` rodavam SEM orçamento (podiam travar o ator do projeto num arquivo patológico); agora devolvem
   "sem árvore" e o chamador já trata (`unwrap_or_default` / conjunto vazio).

## O que sai
`vgp::lang::{EXTENSOES, PARSERS}`, `ast::parser::{THREAD_PARSERS, get_parser, parse_bounded*}` (movidos), `IncrementalParser.parsers`,
13 montagens manuais de `Parser`, `surgery::detect_language` (renomeada).
