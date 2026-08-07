---
okf_version: "1.0"
type: Strategy
title: "Índice Tantivy por projeto — particionamento por diretório com migração incremental"
description: >
  Estratégia para eliminar contenção de writer lock, contaminação cross-project e
  eviction silenciosa de documentos no índice Tantivy, particionando-o por projeto
  sem mudança de schema e com rollback de uma linha.
tags: [tantivy, per-project, pln2, isolamento, migracao, writer-lock]
timestamp: 2026-08-03T12:50:00-03:00
plan_id: task_1785699543075533970
status: aguardando-aprovacao
---

# Índice Tantivy por projeto

Documento de estratégia. **Nada foi implementado** — este artefato existe para o
gate humano (passo 9 do loop). Plano-mãe: [/index.md](./index.md).

## 1. O problema, em evidência

Todos os daemons resolvem o índice Tantivy para **um único diretório**,
`$HOME/.claude/touring/tantivy`, independentemente do projeto. Isso produz três
defeitos distintos, em ordem crescente de gravidade.

### 1.1 Contenção do writer lock — mitigado, não resolvido

O writer lock do Tantivy é exclusivo por diretório. Com N daemons vivos (a
topologia per-project do Pln2 prevê exatamente isso), apenas um escreve.

Corrigido em 03/08/2026 no nível do **sintoma**: a leitura não precisa do lock,
então `open_or_create` cria o reader primeiro e o writer é best-effort; um handle
degradado opera somente-leitura e readquire sob demanda. `IndexStats.writable`
publica a condição.

Verificado em runtime: com o daemon de `~/projects/analise` (PID 68025) segurando
o lock, o daemon global lê 796.676 documentos e reporta `writable: false`.

**O que continua**: enquanto aquele daemon viver, o índice do projeto `touring`
**não recebe escritas** e envelhece. Prova: `touring tantivy search
"cli_decompose_update"` → 0 hits, apesar de o símbolo existir no código.

### 1.2 Contaminação cross-project — provado

Busca a partir do projeto `touring` retorna arquivos de outro projeto:

```
apps/ai-service/venv/lib64/python3.12/site-packages/numpy/...
```

Esse caminho **não existe** em `~/projects/touring`. Pertence a
`~/projects/transferegov_pipeline`. O índice compartilhado serve, às minhas
buscas neste projeto, símbolos de um projeto sem relação.

### 1.3 Eviction silenciosa entre projetos — a mais grave

`file_path` é armazenado **relativo** (`crates/touring-cli/src/...`), e o schema
**não tem campo de projeto**. A identidade do documento é:

```rust
// tantivy_index.rs:1094
let doc_id = doc.blake3_hash.clone().unwrap_or_else(|| {
    hasher.update(doc.symbol_name.as_bytes());  // símbolo
    hasher.update(doc.file_path.as_bytes());    // RELATIVO
    hasher.update(&doc.line_number.to_le_bytes());
});
```

E `upsert_symbol` faz `writer.delete_term(blake3_hash == doc_id)` **antes** de
`add_document`. Logo, dois projetos que compartilhem `(símbolo, caminho relativo,
linha)` colidem, e **o segundo write remove o primeiro**.

Superfície de colisão medida nos projetos de `~/projects`:

| Caminho relativo | Presente em |
|---|---|
| `README.md` | 8 projetos |
| `.gitignore` | 8 projetos |
| `pyproject.toml` | 6 projetos |
| `Cargo.toml` | 5 projetos |
| `package.json` | 2 projetos |

> ⚠ **Grau de confiança.** 1.1 e 1.2 estão provados por execução. **1.3 está
> derivado da leitura do código, não executado.** É a única alegação forte sem
> prova em runtime, e por isso o primeiro entregável da F1 é o teste que a
> confirma ou a refuta. Se sobrarem 2 documentos em vez de 1, minha leitura está
> errada e a prioridade desta frente cai.

## 2. Por que particionar por diretório (e não adicionar um campo)

A alternativa óbvia seria manter um índice só, acrescentar um campo
`project_root` ao schema e filtrar toda consulta.

Ela é **dominada**, não apenas menos elegante:

| | Partição por diretório | Campo + filtro |
|---|---|---|
| Precisa do `project_root` em cada call site | sim | **sim (idêntico)** |
| Resolve contaminação | sim | sim |
| Resolve eviction | sim | sim |
| Resolve contenção do writer lock | **sim** | não |
| Índice cresce sem limite | não | **sim** |
| Exige mudança de schema | **não** | sim |

O custo dominante das duas é o mesmo — obter o root em 41 call sites. Sendo
igual, a partição entrega estritamente mais.

**Decisão: não mexer no schema.** Com índices separados, dois `README.md:1` de
projetos distintos vão para **índices distintos**; a colisão de `doc_id` passa a
ocorrer só dentro do mesmo projeto, onde ela é legítima (mesmo símbolo, mesmo
arquivo, mesma linha = mesmo símbolo). O particionamento entrega a propriedade
sem tocar no schema — e isso importa: mudança de schema dispara o ramo de
recuperação que **apaga o diretório**. Empilhar migração de local com mudança de
schema seria somar dois riscos destrutivos sem necessidade.

## 3. Desenho

### 3.1 Registry keyed com fachada de compatibilidade

O obstáculo real não é o caminho — é a assinatura. `global_tantivy() ->
Option<&'static TantivyIndex>` tem 41 chamadores e devolve `&'static`. E **um
daemon serve N projetos** (observados 2 e 4), então um singleton de processo não
pode ser per-project por definição.

```rust
static REGISTRY: OnceLock<DashMap<PathBuf, &'static TantivyIndex>> = OnceLock::new();

/// `None` ⇒ índice legado global (a fachada durante a migração).
pub fn tantivy_for(root: Option<&Path>) -> Option<&'static TantivyIndex>;

/// Mantida como fachada: delega a `tantivy_for(None)`.
pub fn global_tantivy() -> Option<&'static TantivyIndex>;
```

`Box::leak` na primeira resolução de cada root preserva o `&'static`. O vazamento
é limitado pelo número de projetos servidos, não por requisição — o mesmo trade
que o `OnceLock` atual já faz, N vezes em vez de 1.

Diretório: `<root>/.claude/touring/tantivy/`, alinhado a `symbols_db_canonical` e
`knowledge_db_canonical`. Resolução do root: **reusar
`TouringConfig::normalize_project_root`** — não escrever resolvedor novo. Ele já
trata cwd relativo, `$HOME/.claude` como não-projeto e walk-up por marcador real;
foi escrito para o incidente dos "29 stray DBs" de 20/07.

### 3.2 Os 41 call sites, por classe de dificuldade

| Classe | Onde | Fonte do root |
|---|---|---|
| **1** | `post_edit`, `post_write`, `cli/tantivy.rs`, `memory.rs`, `handlers/index.rs`, `lifecycle/*` | `rt.project_root` — conversão mecânica |
| **2** | `tools_tantivy.rs` (5 sites MCP) | sessão MCP: param opcional **ou** `normalize_project_root(cwd)` |
| **3** | `metadata_collector.rs`, `cli_e2e.rs` | sem contexto óbvio — inspeção caso a caso |

A fachada é o que torna a conversão incremental: cada site convertido passa a
`tantivy_for(Some(root))`; os não convertidos seguem servindo o legado, e o
sistema fica **verde em todo commit**.

### 3.3 Migração é regeneração, não cópia

`cli_tantivy_reindex` lê de `store.symbols_page(page_size, offset)` — ou seja, do
**symbols.db, que já é per-project**.

Isso muda a natureza do trabalho: não há transformação nem cópia dos 180 MB
legados. Cada projeto **regenera** seu índice a partir de uma fonte que já está
correta e já é local. O índice legado permanece intacto no disco durante toda a
migração, o que torna o **rollback uma única linha** (a fachada volta a apontar
para ele).

Ao final, mover para `tantivy.legacy-<ts>` em vez de apagar — a convenção
mover-em-vez-de-apagar já é praticada aqui (existe um
`tantivy.corrupted-1777666996` no disco).

## 4. Fases

| # | Fase | Entrega | Tam. |
|---|---|---|---|
| **F1** | Fundação | Teste que prova (ou refuta) a eviction · `tantivy_for` + registry + fachada · teste de idempotência do ponteiro por root | **M** |
| **F2** | Honestidade do vazio | `total_docs == 0` deixa de devolver `[]` e passa a devolver envelope com a instrução de reindex | **S** |
| **F3** | Conversão dos call sites | Classe 1 → 2 → 3. Gate: `grep -c "global_tantivy()"` fora da fachada == 0 | **L** |
| **F4** | Corte + backfill | Default vira per-project · `touring tantivy reindex` por projeto · medir custo real em um projeto antes de prescrever | **M** |
| **F5** | Aposentar o legado | `tantivy.legacy-<ts>` · remover a fachada · atualizar `touring-cli-index.md` e a skill | **S** |

Dependências lineares e acíclicas: **F1 → F2 → F3 → F4 → F5**.

**F2 precede F4 por necessidade, não por conveniência**: sem a honestidade do
vazio, a janela entre "cortou para o índice novo" e "reindexou" devolve `[]`, que
se lê como "não existe esse símbolo". Trocar um erro alto por um vazio silencioso
seria uma regressão de qualidade mesmo com a arquitetura correta — foi
exatamente a objeção que me fez **não** aplicar esta mudança unilateralmente na
sessão anterior.

## 5. Riscos

| # | Risco | Prob./Impacto | Mitigação — e o gate que a mede |
|---|---|---|---|
| **R1** | Vazio silencioso substitui erro alto | ALTA / ALTO | F2 é pré-condição de F4. Gate: teste que abre índice vazio e exige a instrução de reindex na resposta, **não** `[]` |
| **R2** | Call site esquecido serve dados de outro projeto | MÉDIA / ALTO | Gate determinístico: `grep -c "global_tantivy()"` fora da fachada == 0 |
| **R3** | Vazamento no registry | BAIXA / BAIXO | Teste de idempotência: mesmo root ⇒ mesmo ponteiro. Medir nº real de projetos por daemon antes de assumir o regime 2-4 |
| **R4** | Root mal resolvido → shard perdido | MÉDIA / ALTO | **Não** escrever resolvedor novo; reusar `normalize_project_root` (escrito para os "29 stray DBs") |
| **R5** | Regressão de latência no caminho quente | BAIXA / MÉDIO | `touring gate-metrics` antes/depois (`tantivy_upsert_count` + latência). Se degradar: cachear o handle no `HookRuntime` |
| **R6** | Perder busca na janela de migração | MÉDIA / MÉDIO | Legado intacto + fachada ⇒ rollback de uma linha |

## 6. Fora de escopo — registrado, não escondido

**`ToolOutputsIndex` tem o mesmo defeito.** Singleton global em
`~/.claude/touring/tool_outputs/`, mesmo writer lock exclusivo, mesma ausência de
partição por projeto. É a segunda instância do mesmo padrão.

Fica **fora** desta estratégia por proporção: indexa outputs de ferramenta, não
símbolos; a contaminação ali é menos danosa e o volume é menor. Está registrado
aqui como **débito irmão** para não virar descoberta-surpresa depois — a mesma
disciplina que a REGRA #21 impõe.

## 7. Incertezas que viram gate

O que eu **não** sei e que a implementação tem de medir antes de prescrever:

1. **A eviction é real?** (§1.3) — primeiro teste da F1. Refutação derruba a prioridade.
2. **`rt.project_root` serve todos os sites da Classe 1?** Verifiquei em um arquivo (`mpatch.rs`). F3 começa por **auditar** os 41 sites, não por converter.
3. **Quantos projetos um daemon serve no pior caso?** Observei 2 e 4. Se forem dezenas, o `Box::leak` precisa de teto.
4. **Qual o custo de um reindex completo** (248.932 símbolos no `touring`)? Decide se o backfill é interativo ou job de fundo. Medir na F4 em um projeto real.
