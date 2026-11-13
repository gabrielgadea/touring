# C2 — Um passe "referências do arquivo" (nota de desenho)

Linha de base (12/11, `cargo test -p <crate> --no-fail-fast`): touring-code 1759 ok/3 falhas (os 3 RED do C7, `c7_red_a1..a3`) · hook-runtime 570 · hooks-core 482.

## Interface (`crates/touring-code/src/ast/graph/references.rs`, re-exportada por `ast::graph`)
```rust
pub struct FileReferences<'a> { source: &'a str, lang: Lang }          // struct concreta, sem trait
pub struct InferredReferences { pub method_names: Vec<String>, pub type_refs: Vec<String>,
                                pub qualified_calls: Vec<(String, String)> }
pub struct ModuleReferences { pub paths: Vec<String>, pub declared: BTreeSet<String> }
impl<'a> FileReferences<'a> {
  pub fn new(source: &'a str, lang: Lang) -> Self;
  pub fn imports(&self) -> Vec<ImportInfo>;                      // todos os imports, como escritos (inclui alias de tipo)
  pub fn consumed_imports(&self) -> Vec<ImportInfo>;             // imports que são USO (ver invariantes)
  pub fn forwarded_paths(&self) -> HashSet<String>;              // `módulo::símbolo` de todo `pub use` (só Rust)
  pub fn inferred(&self) -> InferredReferences;                  // chamadas, tipos/consts, pares qualificados
  pub fn python_qualified_uses(&self) -> Vec<(String, String)>;  // `import m as x` + `x.Nome` (só Python)
  pub fn module_references(&self) -> ModuleReferences;           // módulos que os caminhos nomeiam + filhos declarados (só Rust)
  pub fn module_declarations(&self) -> BTreeMap<String, ModuleDeclaration>; // `mod x;` / `#[path]` / inline (só Rust)
  pub fn self_references(&self, symbols: &[Symbol]) -> BTreeSet<String>;    // uso dos próprios símbolos (Rust, Python)
}
```
`ImportInfo` e `ModuleDeclaration` seguem `pub` (tipos devolvidos pela interface).

## Invariantes (decididas atrás do seam, uma vez)
1. Língua sem regra devolve vazio, nunca erro: o chamador deixa de saber "isto é só Rust" (hoje 4 `if lang == Rust`/`ends_with(".rs")` fora do crate).
2. **Importar não é usar** (decisão de 18/09/2026): `consumed_imports` = `imports` menos, em Rust, o re-export que o arquivo não nomeia fora de `use`
   (casado por par `(módulo, símbolo)`) e, em Python, o nome que nunca é referenciado fora do import. Remove SÍMBOLOS, nunca módulos.
3. **Re-export só conta se usado**: a mesma regra de `rust_names_used_outside_use` serve a `consumed_imports`, a `module_references.paths` e a `inferred.type_refs` (já internas).
4. **Declaração não é referência**: o nome que uma declaração introduz não é referência a si mesmo (`inferred`, `self_references`); `module_references.declared` cobre as 3 formas de `mod`.
5. Fonte que não parseia (orçamento, erro) devolve vazio; nenhuma função entra em pânico.
6. Comportamento: mesmas listas (cada método chama o mesmo extrator). A ordem de `imports` é a do extrator, e a do Python vem de um `HashMap`: nenhum
   consumidor pode depender dela, e o teste de caracterização compara `imports` ordenado por módulo.

## O que fica atrás do seam
Os 15 itens `pub` de `ast/graph/{imports,method_calls,python_uses,module_paths,self_refs}.rs` viram `pub(crate)` ou somem:
`extract_imports`, `rust_reexports`, `rust_names_used_outside_use`, `extract_method_calls`, `extract_qualified_calls`, `extract_type_and_const_refs`,
`python_qualified_uses`, `python_unreferenced_imports`, `rust_module_paths`, `rust_module_declarations`, `rust_declared_module_names`, `self_referenced_names`.
`rust_declared_child_modules` não tem consumidor de produção (só 2 testes dela): sai, e os testes passam a perguntar a `rust_module_declarations`.
`drop_symbols` + a composição "reexports − usados" saem de `ast_bridge.rs` para `references.rs`.

## Chamadores que mudam (grep em `crates/`, antes da mudança)
- `touring-hooks-core/src/bridges/ast_bridge.rs`: 136 (`extract_imports`), 158+160 (`rust_names_used_outside_use`, `rust_reexports`), 173 (`python_unreferenced_imports`),
  309 (`extract_method_calls` em `extract_file_method_calls`, que SAI: 1 chamador, o do CLI). `extract_file_imports` e `extract_consumed_imports` ficam como ADAPTER de caminho
  (`Lang::from_path` → interface → tuplas): 9 chamadores externos intactos.
- `touring-hook-runtime/src/wiring.rs`: 762+764 (`rust_declared_module_names`, `rust_module_paths`), 898 (`python_qualified_uses`), 935 (`self_referenced_names`),
  1067-1069 (os 3 de `inferred`), 1090 (`rust_reexports`; `reexported_paths` sai, seus 2 testes em `wiring_tests.rs:1048,1060` passam a `forwarded_paths`).
- `touring-cli/src/cli/handlers/index.rs`: 1122 (`self_referenced_names`), 1135 (`python_qualified_uses`), 1179 (`extract_file_method_calls`), 1182 (`extract_type_and_const_refs`), 1193 (`extract_qualified_calls`).
- `touring-hooks-core/src/symbol_extractors.rs:1222` (`rust_module_declarations`).
- `touring-server/src/server/tools_analysis.rs:148` e `touring-bindings/src/python/ast_bindings.rs:228,336` (`extract_imports` → `imports()`).
- Testes externos: `touring-code/tests/cross_audit.rs` (7 chamadas de `extract_imports`), `touring-cli/tests/b6_python_qualified_path.rs:37,115`.
Total: 31 chamadas de extrator fora do crate migradas para a interface.

## Fora do escopo (declarado)
- UM parse por arquivo: os extratores seguem parseando por conta própria (trocar `&str` por `&Tree` em ~11 funções e ~150 testes é outra mudança);
  o ganho de locality (regra num módulo, teste sem banco) não depende dele. A ORDEM de gravação no banco (`refresh_file_wiring`) é do consumidor.
- Sem conflito com `docs/adr/0001..0005` (nenhum trata de extração de referências).

## Prova (regra 1)
`tests/file_references_characterization.rs`, escrito ANTES (6 línguas, re-export usado/não usado, declaração, fonte vazia) contra os extratores compostos como os consumidores compõem; depois, só `snapshot` mudou (chama a interface), os valores ficaram. Mutante (filtro de "usado" removido) reprova 2 testes.
