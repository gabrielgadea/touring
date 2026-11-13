# Aprofundamento do `touring-code` — plano de execução

Pedido do Gabriel (12/11/2026): «todas as sugestões de arquitetura estão aprovadas,
implemente tudo». Origem: revisão de arquitetura de `crates/touring-code`
(`~/claude-artefatos/touring/architecture-review-20261112-0145.html`), seis candidatos.
Base: commit `a3bfb950` na branch `feat/2026-09-16-shell-quality-wiring-python`.

Vocabulário (use exatamente): **módulo**, **interface**, **implementação**, **profundo/raso**,
**seam**, **adapter**, **leverage**, **locality**. Cada módulo novo ganha um termo no
`GLOSSARY.md` da raiz, em português, com o equivalente em inglês usado no código.

## Os seis candidatos

| # | Aprofundamento | Onde | Depende de |
|---|---|---|---|
| C1 | Uma porta de parse por língua: identidade da língua + gramática + parse com orçamento de tempo num módulo só | `ast/languages.rs`, `ast/parser.rs`, `polyglot/lang.rs`, `languages/tiers.rs`, `vgp/lang/mod.rs`, 13 sítios `Parser::new()+set_language` (`ast/graph/{imports,self_refs,python_uses,module_paths,method_calls,go_wiring,polyglot_semantic}.rs`, `ast/surgery.rs`) | — |
| C2 | Um passe "referências do arquivo": o que conta como referência (ordem dos extratores, re-export só conta se usado, declaração não é referência) decidido atrás de uma interface | `ast/graph/{imports,method_calls,python_uses,module_paths,self_refs}.rs`; consumidores `touring-hook-runtime/src/wiring.rs`, `touring-hooks-core/src/bridges/ast_bridge.rs` | C1 |
| C3 | Um caminho de escrita para "arquivo indexado": indexar, reindexar e remover um arquivo como uma operação, em memória e em SQLite, com a gramática das chaves (arquivo × módulo) explícita | `ast/graph/mod.rs`, `ast/store.rs` | — |
| C4 | Vista de projeto "overlays sobre disco" no VGP: existe / é arquivo / é diretório / ler, com a convenção de chave num lugar, em cima de `vgp/lang/caminho.rs` | `vgp/lang/python/modules.rs`, `vgp/lang/rust.rs`, `vgp/lang/typescript/projeto.rs`, `vgp/lang/classe_css/folhas.rs`, `vgp/lang/html/recurso.rs` | C1 (mesmos arquivos) |
| C5 | Espelho de sinais com uma interface: registrar chamada / ler agregado; o formato da linha fica atrás do seam | `journal.rs`, `sdk.rs`, `sdk_signal_mirror.rs`; leitor `touring-cli/src/cli/kpi/code_mode.rs`; produtores `touring-hook-handlers/src/hooks/post_bash.rs`, `touring-server/src/cli/run.rs` | — |
| C6 | Superfície "lazy" do `IncrementalPipeline`: confirmar chamadores de `queue_for_lazy`, `pending_lazy_count`, `ensure_loaded`, `prefetch_from_heat`; sem consumidor de produção → remover (com os testes que só a exercitam); com consumidor → documentar quem e manter | `ast/incremental_pipeline.rs`, `crates/touring-integration-tests` | — |

Ondas: **A** = C1, C3, C5, C6 em paralelo (arquivos disjuntos). **B** = C2 e C4, depois de C1.

## Regras para todo engenheiro

1. **Comportamento preservado.** É refactor: nenhuma saída observável muda, salvo defeito que o
   próprio aprofundamento conserta — e esse vai listado, com teste RED→GREEN que o prova.
2. **Antes de editar**, rode e guarde a linha de resumo das suítes dos crates que você vai tocar
   (linha de base). Depois, as mesmas suítes: mesmo número de testes passando ou mais; nenhum
   teste removido sem a razão escrita (só se testava ATRAVÉS da interface antiga algo que o
   módulo profundo agora cobre pelo seam — "replace, don't layer").
3. **Nota de desenho primeiro**: escreva `docs/plans/2026-11-12-touring-code-aprofundamento/C<n>-desenho.md`
   (até 60 linhas): a interface (assinaturas reais), invariantes, o que fica atrás do seam, os
   chamadores que mudam (arquivo:linha, contados por grep em TODO o workspace `crates/`), o que
   sai. Depois implemente. Se o desenho exigir mudar o contrato de um crate consumidor de um
   jeito não trivial, ou contradisser um ADR de `docs/adr/`, PARE e mande a nota ao lead.
4. **Sem seam hipotético**: trait só onde há dois adapters reais hoje. Um módulo profundo pode
   ser uma função ou uma struct concreta.
5. **Sem camada**: os chamadores passam a usar a interface nova e a antiga SAI (ou vira
   `pub(crate)`/privada). Deixar as duas é deixar dois caminhos. Símbolo público novo sem
   consumidor é órfão e reprova o juiz (REGRA #0).
6. **Mecânica**: `cargo` pesado só via `scripts/hw-slot --class rust --wait 1800 -- cargo …`
   (exit 75 = sem vaga, repita); leia o exit com `${PIPESTATUS[0]}`. `touring-hook-handlers`
   leva `--features pre-hooks,post-hooks`. Sem `cargo build --release`, sem `update-touring`,
   sem pkill, sem commit, sem stash, sem `git checkout`/`reset`. Leia cada arquivo antes de
   editar. Edite só os arquivos do SEU candidato; se precisar de um arquivo de outro, peça ao lead.
7. **Outras sessões editam este checkout**: não toque em `client/hooks/rate_limits_snapshot.py`
   nem em nada fora da sua lista.
8. **Fechamento**: `cargo fmt` nos arquivos tocados; `cargo clippy -p <crate> --all-targets --
   -D warnings` em cada crate tocado; suítes verdes. Devolva por SendMessage ao lead, curto:
   arquivos tocados; contagem de itens `pub` antes→depois nos arquivos do candidato; sítios
   eliminados (número); linha de base × linha final de cada suíte, com exit; defeitos
   consertados com RED/GREEN; o termo acrescentado ao `GLOSSARY.md`; pontas soltas.

## Aceite (o lead mede)

- `cargo fmt --all --check` 0 diffs; `cargo clippy --workspace --all-targets -- -D warnings` exit 0.
- Suítes de todos os crates tocados exit 0.
- Revisão em dois eixos (padrões × esta spec) sem achado de lógica aberto.
- `loop_converged.py` exit 0.

## C7 — acrescentado em 12/11 por decisão do Gabriel («ligue os produtores de heat» · «ligue o orçamento de memória do pipeline»)

NÃO é refactor: muda comportamento do daemon. Depende do C6 (mesmo arquivo). Dono: `eng-c6-lazy`.
- **Heat**: `learning.heat_map` é escrito em `touring-hook-handlers/.../post_edit.rs:782-783` (`record_edit`) e `pre_read.rs:231-232` (`record_access`) e ninguém o lê. Ligar = dar a esses produtores um leitor de produção entre os já escritos (`process_edit_with_heat`, `PrioritizedPipeline`, `prefetch_for_session`), com efeito observável e medido.
- **Orçamento de memória**: `with_memory_budget_mb` / `max_memory_bytes` / `evict_if_over_budget` existem e nenhum chamador define orçamento; o pipeline do daemon (`touring-hook-runtime/src/hook_runtime.rs:655`) fica sem teto.
- Aceite: teste RED→GREEN por ligação; nenhum símbolo do cluster fica órfão; contador observável (evicções, uso do heat) em `touring status -j` ou `kpi`; knob de ambiente com default justificado por medição; suítes verdes.
