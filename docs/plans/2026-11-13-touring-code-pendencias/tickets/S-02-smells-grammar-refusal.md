# Três smells da revisão de padrões que ficaram sem dono

Task: task_1794549390311758325
Subtask: S-02

Fonte: revisão em dois eixos de 12/11/2026 (eixo de padrões, itens J1, J2 e a nota sobre pares). Nenhum deles muda
saída observável hoje; todos são custo de manutenção. Não foram corrigidos na frente de origem porque cada um mexe
em interface que três candidatos dividem.

1. **`Refusal → erro` em cinco lugares, com mensagens que divergem** (J2). O mapeamento de `Refusal`
   (`crates/touring-code/src/ast/grammar.rs`) para o erro de cada chamador foi escrito cinco vezes;
   `Refusal: Display` só tem um consumidor real (`polyglot_semantic`). Conserto proposto na revisão: um
   `impl From<Refusal> for AstError`, com a variante `Halted` carregando a mensagem que ensina a correção.
2. **Quatro `match` exaustivos sobre `Grammar`** (J1): `of_lang`, `lang`, `name`, `language`, mais a lista `ALL`.
   Língua nova pede cinco edições coordenadas. Uma tabela única por variante resolve.
3. **Pares `(String, String)` de (consumidor, produtor)** em `load_file_edges`, `file_dependency_edges` e a tripla
   de `co_edit_pairs`: dois campos do mesmo tipo primitivo, ordem decidida por convenção. Um tipo nomeado tira a
   chance de inverter.

Aceite: os três com teste que falha antes; nenhuma mudança de texto de erro sem estar declarada.

## Comments
