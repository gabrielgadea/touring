# B4 e o wiring Python (15/09/2026)

Decisão de Gabriel (15/09): a classe "só interno" — símbolo público consumido apenas pelo próprio
arquivo — fica **fora** do `orphans_base` e é **relatada à parte**, junto do conserto do wiring Python
pedido pela analise-08, na release 30.4.52, sem tocar nos arquivos do juiz. DAG
`task_1789523522266898296`.

## O que estava errado

O levantamento (agente read-only sobre o caminho inteiro) achou três defeitos encadeados:

1. **A visibilidade de um binding de módulo Python era lida do texto do nó** (`symbols.rs`
   `detect_visibility`), que procurava `def `/`class `. Um `_FORMAS = {...}` não tem nenhum dos dois,
   caía no fallback "público sem visibilidade" e entrava no grafo como API pública — candidato a
   órfão.
2. **Python não tinha nenhuma aresta de consumo dentro do próprio arquivo.** Os três extratores da
   passada inferida são exclusivos de Rust (`languages.rs:156-161`, `method_calls.rs:455`, consulta
   com nós de Rust). As chamadas Python são extraídas (`call_graph.rs:108`), mas vão para o store de
   símbolos, nunca para `wiring_map`. Resultado: uma constante lida pelo próprio módulo e uma
   constante que ninguém lê eram a mesma linha — órfã.
3. **"Consumido só no próprio arquivo" não existia como conceito.** Uma linha `X → X` satisfaz o
   `NOT EXISTS` da consulta de órfãos, então "usado só por si" e "usado pelo projeto" liam igual.
   São 6.191 linhas assim vivas neste workspace (6.183 `ast_inferred`, 8 `ast_resolved`).

## O que foi entregue

| Parte | Contrato | Onde |
|---|---|---|
| B4_VIS | A visibilidade vem do NOME (`Symbol::python_visibility`): `_x` privado, `__x` name-mangled, dunder público. Vale para função, classe e binding de módulo. | `touring-code/src/ast/symbols.rs` |
| B4_SELF | Um arquivo Python registra o uso dos próprios símbolos públicos: `python_self_referenced_names` casa nós `identifier` fora do span da própria definição, e a aresta `(arquivo, símbolo) → arquivo` é `ast_inferred`. | `touring-code/src/ast/graph/self_refs.rs`, `touring-cli/src/cli/handlers/index.rs` |
| B4_CLASS | `internal_only_symbols`: público, com consumidor, e nenhum consumidor fora do próprio arquivo. Sai em `touring wiring orphans -j` como `internal_only` / `internal_only_count`, e **não** entra em `orphans`. | `touring-storage/src/knowledge_wiring.rs`, `touring-cli/src/cli/wiring.rs` |

Nada disso muda o juiz: a lista de órfãos é a mesma, então `orphans_base` não se move. A baseline só
encolhe onde o defeito era real (um `_privado` que nunca deveria ter sido produtor).

**Defeito encontrado durante a implementação.** O rebuild limpa as arestas inferidas DEPOIS do walk
(`index.rs`, contrato I16) e as recria numa passada final. As arestas de autoconsumo escritas durante
o walk eram apagadas em silêncio — o primeiro teste ponta a ponta falhou exatamente assim, com
`internal_only` vazio e `origin_breakdown` sem nenhuma linha `ast_inferred`. Agora elas são coletadas
no walk e escritas na transação pós-walk, junto das demais inferidas.

## Prova

Teste ponta a ponta sobre o rebuild REAL (`python_wiring_tells_imported_internal_and_dead_symbols_apart`),
num projeto de fixture com `polyglot_wiring = true`, cobrindo as quatro classes e as duas formas reais
que a analise-08 pediu:

| Símbolo | Forma | Veredito |
|---|---|---|
| `PUB2` | importado por outro arquivo | ligado (nem órfão nem interno) |
| `PUB` | lido só pelo próprio módulo | **`internal_only`** |
| `MORTO` | ninguém lê | órfão |
| `_PRIV` | privado, lido no próprio módulo (forma do `_FORMAS`) | não é produtor |
| `_SHARED` | privado, importado por um irmão (forma do `_NUMERAL_ARABICO`) | não é produtor |

Suítes: touring-code, touring-storage e touring-cli verdes (564 no touring-cli). Clippy
`-D warnings` com todas as features. Mutação: **10 de 10** mutantes mortos
(`target/audit-2026-09-14/r2/canvas-d/mut_b4.py`), arquivos restaurados.

Um mutante sobreviveu na primeira rodada e o defeito era do teste, não do código: eu mutei a
comparação `==` por `contains`, mas a varredura só olha nós `identifier`, então o texto de um
comentário nunca chega lá. A proteção real é o tipo do nó. O mutante honesto passou a ser incluir
`string_content` na varredura — com ele, `return "PUB"` viraria um uso — e o teste o mata.

**Medição viva (`scripts/eleitoral`)**: `_FORMAS` (`gerar_visualizador/recursos_externos.py:54`, lido
nas linhas 62 e 104 do próprio módulo) e `_NUMERAL_ARABICO` (`build_deck_senado/formato.py:75`,
importado por `svg.py:15`) são ambos privados por nome, portanto deixam de ser produtores e saem da
lista de órfãos. A medição no repositório vivo fica com a analise-08 depois do `touring update`, como
combinado.

## Depois do deploy (30.4.52)

No workspace touring, com a release propagada: `orphan_count` 1.519 (o mesmo do juiz, inalterado) e
**`internal_only_count` 1.984** — 769 structs, 525 métodos, 409 funções, 170 enums, 60 type aliases e
23 traits. É o número que o H6 estimava em ~1.994 pubs vivos só por autoaresta, agora nomeado e
consultável em vez de escondido dentro de "não é órfão". A política sobre eles segue sendo de Gabriel.

## B5 — as três famílias que a medição viva nomeou (16/09/2026)

A analise-08 rodou o rebuild no repositório e classificou os 231 órfãos que sobraram, com predicado
positivo por família e controles (inclusive uma retratação: um controle positivo mal escolhido que
media zero por ser verdade). **Nenhum dos 231 estava sem consumidor.** O AST mostrou a distribuição
real, e ela não era a que a amostra sugeria:

| Família | Quantos | Causa |
|---|---|---|
| from-import **relativo** (`from .modulo import Nome`) | **61** | a consulta capturava só o `dotted_name`; os pontos vivem no `import_prefix`, então o módulo era procurado a partir da raiz de fontes, e `from . import X` não casava padrão nenhum |
| uso **qualificado** (`import x as gm` + `gm.Nome`) | 70 | `import x` não carrega símbolo, então a passada de imports não escrevia consumidor algum |
| **dunder** (`__init__`, …) | 7 | invocado pelo runtime, nunca pelo nome — a mesma razão pela qual `fmt`/`hash`/`drop` já eram excluídos no Rust |

As outras três famílias dos 231, fora do escopo aprovado para o B5: `mencionado_sem_import_visivel`
(77), `so_em_teste` (10) e `so_no_proprio_arquivo` (6).

> **Correção de número (16/09, depois do deploy).** Esta tabela dizia **136** em vez de 61 para o
> from-import relativo. O 136 veio do primeiro recado da analise-08 e eu o propaguei para cá, para a
> pendência 2 da cross-audit e para a mensagem do commit `5b47b359` sem conferir contra o artefato.
> O valor medido, no `familias-dos-orfaos-2026-09-16.json` dela, é **61**, e as seis famílias somam
> exatamente 231 — a soma é o teste que eu não fiz na hora. O dimensionamento do B5 não muda (as
> famílias corrigidas são as mesmas), só o tamanho anunciado de uma delas.

Entregue, decisão de Gabriel (16/09):

| Parte | Contrato | Onde |
|---|---|---|
| B5_REL | A consulta captura o `relative_import` inteiro, e `resolve_relative_python_import` ancora no PACOTE: um ponto é o diretório do arquivo, cada ponto extra sobe um pacote, e `from . import X` é o `__init__.py` do pacote. | `python_imports.scm`, `symbol_extractors.rs` |
| B5_QUAL | `python_qualified_uses` devolve `(módulo, símbolo)` para todo `alias.Nome` cujo alias veio de um `import`; o rebuild resolve o módulo e grava a aresta como `ast_inferred`. `import a.b` sem alias fica de fora: o caminho do módulo seria palpite. | `graph/python_uses.rs`, `index.rs` |
| B5_DUNDER | `__nome__` sai da lista de órfãos pelo filtro compartilhado de linha de produtor. | `knowledge_wiring.rs` |

O teste ponta a ponta cobre as três no rebuild real, cada uma com o controle negativo ao lado:
`PUB_REL` (importado por `.formato`) sai da lista enquanto `MORTO_REL`, do mesmo arquivo, continua
órfão; `PUB_QUAL` sai por `gm.PUB_QUAL`; `__init__` nunca aparece.

Suítes: touring-code (745), touring-hooks-core (499), touring-storage e touring-cli verdes; clippy
`-D warnings` com todas as features. Mutação: **9 de 9** mortos
(`target/audit-2026-09-14/r2/canvas-d/mut_b5.py`), arquivos restaurados, e cada mutante morto pelo
teste que o cobre — nenhuma morte colateral.

**Código morto achado pela mutação.** O ramo explícito para `from . import X` sobreviveu à mutação, e
o motivo é que ele nunca fazia falta: com a cauda vazia, `dir.join("")` é o próprio diretório, então a
sonda de pacote já respondia `<dir>/__init__.py`. O ramo saiu; a asserção para `.` continua no teste,
agora exercitando o caminho geral. É a terceira remoção de código inalcançável desta sequência
(as outras: N/A por crate no D3.0 e o ramo de exit code que virou teste no D3.4) — todas encontradas
por escrever o mutante, nenhuma pela leitura.

## Medição viva depois da 30.4.53 (analise-08, geração 11 do índice)

Mesmo instrumento nas duas pontas (`classificar_orfaos_do_juiz.py` e `familias_dos_orfaos.py`,
escopo `scripts/eleitoral`), commit `e322b832eb` do projeto analise:

| Família | gen 9/10 (30.4.52) | gen 11 (30.4.53) | |
|---|---|---|---|
| from_import | 61 | **0** | alvo do B5_REL — fechado |
| dunder | 7 | **0** | alvo do B5_DUNDER — fechado |
| qualificado | 70 | **53** | alvo do B5_QUAL — **fechou pouco, ver abaixo** |
| mencionado_sem_import_visivel | 77 | 22 | efeito colateral favorável |
| so_em_teste | 10 | 10 | intocado |
| so_no_proprio_arquivo | 6 | 6 | intocado |
| **total no escopo** | **231** | **91** | `novos_orfaos` = 0 |

O desaparecimento é conserto e não cegueira, e isso foi medido: o corpus encolheu (12.269 → 10.438
registros), então "sumiu da lista" tinha duas explicações opostas. Amostra de 12 dos 140
desaparecidos: 12/12 seguem no disco **e** no índice; controle com 4 que permaneceram órfãos também
são achados pelo `index find`.

### A lacuna que a medição revelou: `from pacote import modulo as alias`

O B5_QUAL derrubou só 17 dos 70. Classifiquei os 53 sobreviventes pela forma REAL do import no
arquivo que os usa (`target/audit-2026-09-14/r2/canvas-d/qual_forma.py`): **71 ocorrências são
`from pacote import modulo [as alias]`**, com uso `alias.Nome`. Exemplo medido:

```python
# scripts/eleitoral/gerar_visualizador/montagem.py:16
from scripts.eleitoral import controle_camadas as cc
...  cc.REGRA_DA_ZONA        # controle_camadas.REGRA_DA_ZONA lê como órfão
```

`python_qualified_uses` varre apenas nós `import_statement` — `from X import Y as Z` é
`import_from_statement` e nunca é visto. E a passada de imports que lê `from x import y` trata `y`
como SÍMBOLO, quando aqui `y` é um MÓDULO: a aresta não tem para onde ir.

**Isto não é a exclusão deliberada documentada abaixo** (`import a.b` sem alias, cujo caminho de
módulo seria palpite): é lacuna de cobertura da implementação, e o caso é resolúvel sem adivinhar —
`from X import Y` importa um módulo exatamente quando `X/Y.py` ou `X/Y/__init__.py` existe, que é uma
pergunta ao disco, não um chute. Fica como candidato a B6, aguardando decisão de Gabriel.

## O que este trabalho NÃO resolveu

O mapa do levantamento listou outros defeitos do wiring Python, todos fora do escopo aprovado:

- `import x` (sem `from`) não gera linha de consumo nem de import não resolvido.
- Import relativo perde os pontos iniciais (`from .helpers import X` é procurado a partir da raiz de
  fontes), e `from . import X` não casa nenhum padrão.
- Todo import Python não resolvido é classificado como `external` (932 linhas vivas), então a dívida
  de resolvedor em Python é invisível.
- O caminho de EDIÇÃO grava `"rust"` fixo (`hook-runtime/src/wiring.rs:824`): editar um `.py` não
  atualiza consumo; só o rebuild completo.
- Há duas implementações de órfão que discordam (`knowledge_wiring.rs` e `analysis/src/wiring/orphan.rs`).
