# SCIP export — a API existe, a CLI não

> **Origem** (20/08/2026): a skill `touring-scip` foi removida por ensinar
> `touring scip emit <file>` e `touring symbol index`, **nenhum dos dois
> existente** em `touring --help` (139 comandos verificados um a um). Ela era a
> única fonte desses comandos no corpus inteiro — e a sugestão já havia
> escapado para o enriquecimento de prompt, que passou a recomendá-los.
>
> O que era verdade fica aqui. A API Rust é real e consumida.

## O que existe (verificado por `touring index find`)

| Símbolo | Local | Tipo | Consumidores |
|---|---|---|---|
| `ScipEmitter` | `crates/touring-server/src/scip_emit.rs:135` | struct | 1 |
| `ScipDocument` | `crates/touring-server/src/scip_emit.rs:55` | struct | 1 |

O módulo tem ~14,6 KB de código vivo. **Não é órfão** — uma leitura anterior
consultou o MÓDULO `scip_emit` (refs=0) em vez dos SÍMBOLOS que ele exporta
(refs=1 cada), e concluiu o oposto. Consultar o contêiner e reportar sobre o
conteúdo é o erro que essa nota registra.

```rust
use touring_server::scip_emit::{ScipEmitter, ScipDocument};

let emitter = ScipEmitter::new(&symbol_store);
let doc: ScipDocument = emitter.emit_document("src/main.rs")?;
let docs = emitter.emit_batch(100);   // top-N arquivos
```

Forma do documento emitido (`relative_path`, `language`, `occurrences[]` com
`line`/`col_start`/`col_end`/`symbol`/`role`) — ler a fonte para o contrato
corrente, que é a única autoridade.

## O que NÃO existe

- `touring scip emit …` — sem subcomando `scip`. A funcionalidade só é
  alcançável pela API Rust acima.
- `touring symbol index` — sem subcomando `symbol`.

## O que existe e serve de alternativa

`touring query "<dsl>" -j` é real (DSL de metadata sobre o índice) e cobre boa
parte do caso de uso "exportar símbolos filtrados" que a skill removida
endereçava por um comando inexistente.

---

_Potencialização pendente (REGRA #0): a capacidade SCIP não tem superfície de
CLI. Expor `ScipEmitter` como subcomando faria a documentação removida virar
verdade, em vez de a documentação ter sido removida por não ser._
