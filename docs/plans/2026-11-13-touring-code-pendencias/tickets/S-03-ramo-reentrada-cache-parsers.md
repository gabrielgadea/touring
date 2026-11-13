# Ramo de reentrada do cache de parsers sem chamador de produção

Task: task_1794549390311758325
Subtask: S-03

`crates/touring-code/src/ast/grammar.rs` guarda um parser por língua em cache por thread (`PARSERS`). O acesso é
`cell.try_borrow_mut()` (linha 311 em 13/11/2026): quando o cache já está emprestado, o ramo alternativo constrói um
parser avulso, para que um parse aninhado termine em vez de entrar em pânico.

Nenhum caminho de produção faz parse dentro de parse. Só um teste exercita o ramo (`the nested parse completes too`).
Pela regra 4 do plano de origem (sem seam hipotético), ou o ramo ganha um chamador real, ou sai e o `borrow_mut`
passa a acusar o aninhamento como defeito.

Decidir primeiro: existe consumidor previsto que faça parse durante um callback de progresso ou de leitura por
pedaços (`parse_chunks`)? Se não, remover o ramo e o teste dele.

## Comments
