# Classe de evidência na escrita do wiring: palpite por nome × referência exata

Task: task_1794549390311758325
Subtask: S-04

## Contexto (medido em 13/11/2026, correções F13 e F14 da frente de origem)

`wiring_map.contract_source` separa import resolvido (`ast_resolved`) de inferência (`ast_inferred`), mas a
inferência mistura três coisas de qualidade diferente sob a mesma origem:

1. chamada qualificada por módulo (`backup::run()`), exata;
2. chamada por tipo (`Verdict::parse(x)`, passe R1 do F14), exata ou nada;
3. palpite por nome puro com teto de 4 produtores, e o passe R2 do F14 (método por receptor), que na amostra
   manual acertou 3 de 15.

O F13 fez a regra de aresta de arquivo (`file_edge_filter!`) excluir toda linha inferida. Isso tirou 70.926
arestas falsas do touring (77.655 → 6.729), mas deixa de fora arestas verdadeiras das classes 1 e 2 e de
referência de tipo por caminho completo.

## O que fazer

- Dar origem própria às classes exatas (1 e 2) na escrita, para a regra de aresta poder aceitá-las. A coluna
  já existe; `WiringOrigin` ganha a variante e `strength()` a ordena.
- Variante medida e **não** implementada: aceitar linha inferida cujo produtor é tipo
  (struct/enum/trait/const/static/type_alias/union) com nome de produtor público único. Daria 8.987 arestas
  (2.258 a mais). Cinco abertas à mão: 4 verdadeiras, 1 falsa (`Lang::JavaScript`, variante de enum, creditada
  à struct homônima). Decidir isso pede resolvedor de caminho, não contagem de produtores.
- Python: imports que o resolvedor não consegue colocar ficam só com linha inferida (80 pares no `analise`,
  medidos em 13/11 depois do F16; o alias de módulo saiu dessa conta). Caminho sugerido pelo engenheiro: gravar
  "o import nomeia o módulo, mas não resolveu" sob `text_matched`, origem que já existe e não tem escritor; a
  regra de aresta já a aceita. Medir por classe de causa em `wiring_unresolved` antes de mexer.
- F16 moveu a gravação do alias Python para o passo do próprio arquivo no rebuild, para não reabrir a janela
  do I16. Isso está argumentado pelo código e pelos testes de paridade rebuild/edit; falta o teste que mata o
  rebuild no meio da caminhada e confere que a linha de alias não some.
- Método por receptor só tem resposta exata com o tipo do receptor: a origem `scip_resolved` (ingest do
  rust-analyzer) é o caminho; o R2 é um paliativo declarado.
- Os scripts de evidência do C8 fora do repositório (`~/claude-artefatos/touring/c8_before_after_report.py`,
  `c8_selection.py`, `c8_probe_dbs.py`) ainda codificam a regra antiga; um relatório refeito com eles diverge
  do produto.

Aceite: cada classe nova com teste que falha antes, mutante, e sonda no banco real com três arestas abertas
por classe.

## Comments
