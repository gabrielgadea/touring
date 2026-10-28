# C1c — alcance de zstd 0.13.3 / zstd-safe 7.3.0 via tantivy 0.22

**Data:** 28/10/2026  
**Escopo final:** somente RUSTSEC-2026-0391, RUSTSEC-2026-0398 e RUSTSEC-2026-0403, os três ainda reportados por `cargo deny check advisories` após o port do C1c.  
**Veredito:** **aceitável com evidência**. As funções afetadas não são alcançadas pelo caminho do tantivy.  
**Limites desta análise:** somente leitura; nenhum build, teste ou edição de código foi executado.

## 1. Grafo efetivo

O `Cargo.lock` do tree principal confirma:

```text
tantivy 0.22.1
└── tantivy-columnar 0.3.0
    └── tantivy-sstable 0.3.0
        └── zstd 0.13.3
            └── zstd-safe 7.3.0
                └── zstd-sys 2.1.2+zstd.1.5.7
```

`tantivy-sstable 0.3.0` é o único pai de `zstd 0.13.3` no lockfile. Em paralelo, a dependência direta já portada usa `zstd 0.14.3 → zstd-safe 8.2.0`.

O manifesto original do sstable declara somente `zstd = { version = "0.13", features = ["experimental"] }` (`tantivy-sstable-0.3.0/Cargo.toml.orig:17`). O columnar não referencia `zstd` diretamente: chega a ele pelo sstable.

Nos caminhos abaixo, `$REG` significa `/home/gabrielgadea/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f`.

## 2. Caminho zstd realmente executado

O tantivy usa somente a API **bulk/one-shot**:

- escrita do sstable: `$REG/tantivy-sstable-0.3.0/src/delta.rs:5,60-62` — `Compressor::new(3)?.compress_to_buffer(...)`;
- leitura do sstable: `$REG/tantivy-sstable-0.3.0/src/block_reader.rs:5,60-64` — `Decompressor::new()?.decompress_to_buffer(...)`;
- doc store do tantivy: `$REG/tantivy-0.22.1/src/store/compression_zstd_block.rs:3,18-22,44` — as funções bulk `compress_to_buffer` e `decompress_to_buffer`.

Cada bloco cria um contexto novo. Não há contexto de streaming reutilizado, `Seekable`, `AdvancedSeekable` ou `decompress_stream` nesse caminho. O wrapper bulk confirma a chamada one-shot em `$REG/zstd-0.13.3/src/bulk/decompressor.rs:86-93` (`DCtx::decompress`), que não é a função `DCtx::decompress_stream` do advisory 0403.

### Prova de ausência executada

```bash
rg -n -w "with_context|Seekable|AdvancedSeekable|decompress_stream" \
  "$REG/tantivy-sstable-0.3.0/src" \
  "$REG/tantivy-columnar-0.3.0/src" \
  "$REG/zstd-0.13.3/src/bulk"
# NO_MATCH

rg -n -w "seekable" \
  "$REG/tantivy-sstable-0.3.0/Cargo.toml" \
  "$REG/tantivy-sstable-0.3.0/Cargo.toml.orig" \
  "$REG/tantivy-sstable-0.3.0/src"
# NO_MATCH
```

A busca cobre tanto chamadas diretas nos dois crates do tantivy quanto chamadas indiretas nos módulos bulk pelos quais elas passam.

## 3. Advisories, funções e alcance

| Advisory | Faixa que casa | Funções afetadas | O tantivy chama? | Evidência e entrada |
|---|---|---|---|---|
| **RUSTSEC-2026-0391** | zstd `>=0.13.2,<0.14.3`; casa com 0.13.3 | `zstd::stream::raw::Encoder::with_context`; `zstd::stream::raw::Decoder::with_context` | **Não, nem direta nem indiretamente pelo bulk** | O `rg` acima deu `NO_MATCH` nos dois crates e em `zstd/src/bulk`. O caminho real cria `Compressor`/`Decompressor` bulk novos por bloco (`delta.rs:62`; `block_reader.rs:63-64`), sem `with_context` e sem stream abandonado/reutilizado. |
| **RUSTSEC-2026-0398** | zstd-safe `>=7.2.2,<8.2.0`; casa com 7.3.0 | `zstd_safe::seekable::Seekable::decompress`; `zstd_safe::seekable::AdvancedSeekable::decompress` | **Não** | O advisory exige a feature `seekable` (linhas 29-32 do `.md`); ela não aparece no manifesto nem nas fontes do sstable (`rg`: `NO_MATCH`). As duas funções também não aparecem no tantivy nem no bulk. Logo, arquivo/offset de um arquivo seekable nunca chega a essas funções. |
| **RUSTSEC-2026-0403** | zstd-safe `>=4.0.0,<8.2.0`; casa com 7.3.0 | `zstd_safe::DCtx::decompress_stream` | **Não** | `rg`: `NO_MATCH`. Além disso, o próprio advisory diz explicitamente: “`zstd::stream::read::Decoder`, `zstd::decode_all` and the bulk API are not affected” (`RUSTSEC-2026-0403.md:34-35`). O tantivy usa exatamente a bulk API. O vetor exige um legacy frame corrompido na descompressão **streaming**, que não existe nesse caminho. |

Nenhum dos três advisorys está sem `[affected.functions]`; portanto, não foi necessário aplicar a regra conservadora “afeta tudo”.

## 4. Origem dos dados

- **Escrita normal:** o `DeltaWriter` serializa os dados do próprio índice e os comprime (`tantivy-sstable-0.3.0/src/delta.rs:44-62`). No uso normal do Touring, os frames foram produzidos pelo próprio tantivy/Touring.
- **Leitura normal:** o `BlockReader` recebe `OwnedBytes` do arquivo de índice e descomprime o bloco (`block_reader.rs:33-64`). O Touring abre um `MmapDirectory` local em `crates/touring-hooks-core/src/tantivy_index.rs:681-684` e `:2344-2352`.
- **Fronteira:** se alguém fornecer ou alterar um diretório de índice, esses bytes são entrada externa/corrompida. Isso não muda o veredito: 0391 e 0398 não têm o caminho de função/feature, e 0403 exclui expressamente a bulk API usada na leitura.

## 5. Existe atualização que remove o residual?

**Sim: tantivy 0.27.0**, a versão atual retornada por `cargo search tantivy --limit 8` em 28/10/2026. `cargo info tantivy@0.27.0`, `cargo info tantivy-sstable@0.27.0` e `cargo info tantivy-columnar@0.27.0` foram usados sem build.

- `$REG/tantivy-0.27.0/Cargo.toml.orig:31`: `zstd = { version = "0.14", optional = true, ... }`;
- `$REG/tantivy-sstable-0.27.0/Cargo.toml.orig:18-19`: zstd `0.14` com `experimental`;
- `$REG/tantivy-columnar-0.27.0/Cargo.toml.orig:15-16,65`: columnar/sstable 0.27 e feature de compressão encaminhada ao sstable.

Com o lock atual já contendo zstd 0.14.3, o requisito `0.14` aceita a versão corrigida.

### Índice/schema e custo de migração

**Reindexação não é exigida pelo formato persistido:**

- tantivy 0.22.1 grava `INDEX_FORMAT_VERSION = 6` e lê 4–6 (`tantivy-0.22.1/src/lib.rs:242-245`);
- tantivy 0.27.0 grava 7 e lê 4–7 (`tantivy-0.27.0/src/lib.rs:239-242`);
- o 0.27 mantém `test_format_6`, abrindo um índice formato 6 (`tantivy-0.27.0/src/compat_tests.rs:39-47`);
- o changelog também declara compatibilidade do 0.24 com índices 0.22/0.21 (`tantivy-0.27.0/CHANGELOG.md:191-194`).

O salto 0.22 → 0.27 é, ainda assim, uma migração **L4 de API**, não um bump trivial: o changelog 0.27 lista MSRV 1.88 e mudanças em `FieldType`, `IndexSettings`, `Directory`, agregações e sstable (`CHANGELOG.md:4-16`). A fachada `crates/touring-hooks-core/src/tantivy_index.rs` tem 2.805 linhas e `blast_radius = 115`; `touring wiring impact TantivyIndex --depth 2` reportou 9 consumidores diretos e 14 no total. O custo exato de callsites só pode ser fechado por check/testes após o bump, o que estava fora deste brief. O schema persistido não precisa ser recriado pelo formato, mas a compatibilidade do schema customizado e dos tokenizers deve entrar na validação da migração.

## 6. Entradas prontas para `deny.toml`

```toml
{ id = "RUSTSEC-2026-0391", reason = "zstd 0.13.3 is TRANSITIVE, sole parent tantivy-sstable 0.3.0 (tantivy-columnar 0.3.0 → tantivy 0.22.1). Affects only stream::raw::{Encoder,Decoder}::with_context when reusing an abandoned stream context. Verified 28/10: no with_context call in tantivy-sstable/columnar or zstd's reachable bulk modules; tantivy creates a fresh bulk Compressor/Decompressor per block (delta.rs:62, block_reader.rs:63-64). Vulnerable path unreachable. tantivy 0.27 accepts zstd 0.14; re-evaluate on bump; auto-expiry via unused-ignored-advisory=warn." },
{ id = "RUSTSEC-2026-0398", reason = "zstd-safe 7.3.0 is TRANSITIVE via tantivy-sstable 0.3.0. Affects only seekable::{Seekable,AdvancedSeekable}::decompress and requires the seekable feature. Verified 28/10: tantivy-sstable enables only zstd experimental, not seekable; no Seekable/AdvancedSeekable call in tantivy-sstable/columnar or zstd's bulk path. Vulnerable path unreachable. tantivy 0.27 accepts zstd 0.14; re-evaluate on bump." },
{ id = "RUSTSEC-2026-0403", reason = "zstd-safe 7.3.0 is TRANSITIVE via tantivy-sstable 0.3.0. Affects only DCtx::decompress_stream on a corrupt legacy frame; the advisory explicitly states the bulk API is not affected. Verified 28/10: tantivy uses only bulk one-shot decompression (block_reader.rs:63-64) and no decompress_stream callsite exists in tantivy-sstable/columnar or zstd's bulk modules. Vulnerable path unreachable. tantivy 0.27 accepts zstd 0.14; re-evaluate on bump." },
```

## 7. Conclusão

**Opção (a): aceitar os três com evidência.** Não é necessário subir o tantivy para tornar esses caminhos seguros. A remoção estrutural do residual continua disponível via tantivy 0.27/zstd 0.14, sem reindexação obrigatória pelo formato, mas com migração e validação L4 de API.
