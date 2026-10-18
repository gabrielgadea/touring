# RELATÓRIO VRS — porta do adaptador Rust (camada 1) para a VL0

> Sessão: subagente `vrs-porta` (backend Sonnet 5.5), delegação da coordenadora touring-09 (`team-lead`). Data: 10/10/2026.
> B = `/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base`. Nada commitado, nada propagado.
> A entrega é hipótese até o juiz da coordenadora.

## 1. Identidade

| item | valor |
|---|---|
| autoria original | **touring-a6 (Kimi)** escreveu o resolvedor, o modelo de módulos e a caminhada de escopo (`verificar_rust`, ~2.219 linhas + 18 testes + 3 mutantes) em `~/projects/touring-wt-vrs` (branch `feat/2026-10-09-vgp-vrs`, sem commit). Parou ao bater o limite semanal (02:34Z de 10/10); o último gate dela (02:46Z) deu clippy, testes `vgp` (86), build e `binary_e2e` com rc 0. Não deixou binário congelado, juiz nem relatório. |
| esta porta | worktree `~/projects/touring-wt-vrs2`, branch `feat/2026-10-10-vgp-vrs`, VL0 aplicada, sem commit. Arquivos meus: `crates/touring-code/src/vgp/lang/rust.rs` (2.312 linhas, sha16 `76101e58b2214ef2`) e `crates/touring-code/src/vgp/lang/rust/tests.rs` (791 linhas, sha16 `ea88e948e3d7b4e3`). |
| fonte da a6 (só leitura) | `lang/rust.rs` sha16 `51ff8d5985e2e2ad` (86.379 B) · `lang/rust/tests.rs` sha16 `835f71db30010db9`. Intactos: `git status` e `Cargo.lock` do `touring-wt-vrs` idênticos antes e depois do meu build. |
| escopo editado | só `lang/rust.rs` e `lang/rust/`. `diff -rq` do `crates/touring-code/src/vgp`, de `touring-hook-handlers/src` e de `touring-server/src/cli/vgp.rs` contra o `touring-wt-vl0`: a única diferença é `lang/rust.rs` + `lang/rust/`. Sem `TEMPORARIO-VRS` (grep = 0 nos três arquivos). |
| Context7 | **não alcançável** nesta sessão (o MCP não está exposto ao subagente). Usei os equivalentes locais, citados: `rustc --explain E0432 E0433 E0425 E0560 E0599 E0061` (rustc 1.98.0) e a Rust Reference que acompanha a toolchain (`~/.rustup/toolchains/*/share/doc/rust/html/reference/`: `paths.html`, `items/use-declarations.html`, `items/modules.html`, `names/preludes.html`, `attributes.html`). |

## 2. Feito, com evidência

**Inventário do trabalho da a6** (diff contra `88c7cc8b`): classes `import` (E0432/E0433: `use`/`mod x;` que erram um alvo do projeto, `pub use` simples, com alias e glob, `#[path]`), `nome` (E0425: nome solto e cabeça de caminho sem escopo), `kwarg` (E0560 campo de literal; E0061 aridade de função livre do projeto) e `atributo` (E0599: variante de enum do projeto). Abstenções desenhadas: macro de item, glob de crate externo, crate externo sem modelo, receptor de método, `non_exhaustive`, struct-tupla. Mutantes que ela rodou: `ignorar pub use`, `macro como inexistente`, `super:: como crate::` (3/3 mortos, log `/tmp/vrs-gate-mutantes.log`). Ela não rodou o juiz.

**A porta** (`impl Adaptador for Rust`; `verificar_rust` deixou de existir):

1. O adaptador MEDE; severidade, delta e veredito saíram dele (eram `deny_natural`, o bloco de delta e o `verdict` próprio). Achados nascem com `Finding::candidato(..., Alvo, ...)`.
2. Parse por `lang::parse("rust", …)` (passa pelo `parse_bounded`), também no `modelo()` dos módulos irmãos.
3. `Resolucao::Abstencao(Abst)`: `Abst::PorDesenho` (item associado de struct do projeto — nunca reivindicado, silencioso) × `Abst::Falha(motivo)` (o adaptador TENTOU e não fechou: módulo ilegível, sem `Cargo.toml` acima do arquivo, `super` sem item, ciclo/profundidade de `pub use`, glob irresolúvel). As `Falha` viram `Verificacao.nao_medido` (`"N caminho(s) sem resolução estática: …"`); erro de sintaxe vira `nao_medido` (era `unknown` mudo). `std`, crate externo declarado e receptor de método NÃO são "não medido" (nunca foram reivindicados).
4. `Alvo`: `Projeto` para todo símbolo do projeto; `Incerto` para o nome "possivelmente dinâmico" (macro de item / glob externo) e para o projeto que mora sob `/tmp/` (herda a regra `em_crate` da a6: o prefixo `/tmp/` da política não vale para quem tem `Cargo.toml` acima — warn, nunca deny). Não existe `Externo`: a camada 1 não tem modelo de `std`/crate externo.

**Duas correções além da porta e uma consequência** (todas só REMOVEM achados; cada uma nasceu de um teste novo que ficou vermelho):

- **Atributos são IRMÃOS do item no tree-sitter-rust, não filhos.** `path_attr_de` e `tem_attr` da a6 liam `item.children()`: `#[path = "x.rs"]` e `#[non_exhaustive]` eram código morto (nenhum teste da a6 os exercitava). Novo `atributos_do_item` (irmãos anteriores, pulando comentários); `path_attr_de` agora exige `path = "…"` de fato (antes `#[cfg(feature = "pathfinder")]` contava).
- **Variante é CamelCase** (`parece_variante`): `Enum::from_path`, `Enum::ALL`, `Self::nova()` apontam item associado (impl em qualquer arquivo), não são provavelmente variante ausente. A a6 acusava E0599 neles.
- (consequência da primeira) 110 `import` e 13 `nome` falsos de módulos declarados com `#[path]` somem — ver §3.

**Medidas** (binários release congelados, juiz `scripts/juiz_motor_vgp.py` sha16 `6c8a31b02075da1e`, `--exigir-limite-superior`):

| | ANTES (a6, como estava) | DEPOIS (porta) |
|---|---|---|
| binário | `~/.claude/touring/vgp/binarios/d5b434d7/touring` sha256 `d5b434d7e8956b1ac376799dd65fb3f95382d9769004d8eec4c3aaf376eda883` (build `-p touring-server --bin touring --release --locked`, 11 m 23 s, target `wt-vrs`) | `~/.claude/touring/vgp/binarios/42830beb/touring` sha256 `42830bebffa5ed6a5d001e327b2aaad3188ada17bd8073dc933aa7d8239af9b8` (target `wt-vrs2`, 11 m 48 s; o `cargo build --release` da rodada final, com os fontes finais, não recompilou — 0,76 s — e o sha segue o mesmo) |

Tabela por classe — positivos com achado (deny OU warn) de `nome == nome_esperado` / positivos reproduzidos:

| corpus | classe | ANTES | DEPOIS |
|---|---|---|---|
| **natural** (`corpus_rust_natural`, 37 pos · 307 neg) | nome | 5/20 | 5/20 |
| | import | 7/13 | 7/13 |
| | atributo | 2/3 | 2/3 |
| | kwarg | 1/1 | 1/1 |
| | **recall de símbolo (juiz)** | **15/37 = 0,405** | **15/37 = 0,405** |
| | falso deny | 0/307 · limite CP95 **0,97 %** | 0/307 · limite CP95 **0,97 %** |
| | avisos em negativos | 222/307 | 214/307 |
| | p95 do motor (juiz) | 166,6 ms | 128,1 ms (por id: 157,7 → 128,7) |
| **sanidade** (`corpus_rust`, 80 pos · 200 neg; por id, ver §6.1) | nome | 20/20 | 20/20 |
| | kwarg | 20/20 | 20/20 |
| | import | 0/20 | 0/20 |
| | atributo | 0/20 | 0/20 |
| | falso deny | 0/200 · limite CP95 1,49 % | 0/200 · limite CP95 1,49 % |
| | p95 | 0,3 ms | 0,3 ms |

**A porta não perdeu recall em nenhuma classe, em nenhum corpus.** Sanidade: **280/280 ids idênticos** (veredito + `{classe,nome,linha,severidade}`). Natural: **182 idênticos, 162 diferentes, todos explicados**, 0 achado ADICIONADO em qualquer id:

| diferença (natural, 344 ids) | ids | causa |
|---|---|---|
| mesmos achados, `allow` → `unknown` | 57 | `nao_medido` (a6 devolvia `allow` mudo em abstenção; o motivo dominante é `use super::*` — ver §6.5) |
| achados a menos, continua `warn` | 97 | `atributo` (98 ocorrências removidas no total: `from_contract_source` 28, `ALL` 17, `from_str_opt` 10, `from_env` 8, `parse` 7, `from_str_ci` 6, `from_path` 6, `from` 3 …, todas não-CamelCase) e `import`/`nome` de módulos `#[path]` (`cli_handlers` 29, `ast_bridge` 21, `cli_handlers_mutation_test` 14, `cli_handlers_decompose` 8, `team_hooks`, `post_write`… — confirmados por `grep`: são `#[path = "…"] mod x;` em `touring-cli/src/lib.rs` e `touring-hook-handlers/src/lib.rs`) |
| `warn` → `allow` | 2 | só tinham os achados removidos acima |
| `warn` → `unknown` | 6 | os removidos + `nao_medido` |

Total de achados removidos: atributo 98 · import 110 · nome 13, em 105 ids; **nenhum era o `nome_esperado` de um positivo** (a tabela de recall é idêntica).

**Varredura em código real que compila** (130 `.rs` de `touring-code`/`touring-identity`/`touring-pyright` em `touring-wt-semeado` 88c7cc8b; todo achado é falso positivo por construção): ANTES 69 arquivos com warn (253 achados: nome 169, import 68, atributo 16); DEPOIS 63 com warn (237: nome 169, import 68, atributo 0) + 49 `unknown` + 18 `allow`; p95 34,2 → 40,2 ms.

## 3. Estado por item do aceite

| # | item | estado |
|---|---|---|
| 1 | testes por classe RED→GREEN com controles | **cumprido**. Contra o stub do núcleo (`unknown`) os 26 testes do adaptador reprovam: **0 passam, 26 falham** (`RED-stub`, no runner de mutantes). Contra a porta: **26/26 verdes**. Por classe: `nome` (6 testes: chamada direta, referência como valor, controles de escopo, `Self`, glob/macro, glob de crate declarado), `import` (9: nome inventado, segmento intermediário/lista mista, `mod x;` sem arquivo, `pub use` simples/alias, `pub use` glob, `super`/`self`, cross-crate, crate não declarado × declarado, `#[path]`), `kwarg` (3: campo e variante de enum, aridade, `non_exhaustive`/tupla), `atributo` (2: variante inventada × item associado), política/delta/parse (4), `nao_medido`/despacho (2). Controles que passam limpos: o mesmo símbolo existindo, reexportado por `pub use`, atrás de macro, de crate externo declarado. |
| 2 | mutantes mortos e listados | **cumprido: 24/24 mortos** + o RED-stub. Exigidos: `ignorar pub use` = R5 (e R6 glob), `macro como inexistente` = R10, `super:: como crate::` = R3, «abstenção vira allow» = R14 (sintaxe quebrada → `medido(vazio)`), R15 (resolução não fechada não registrada), R16 (motivo nunca sai). Demais: R1 `crate::` → arquivo corrente · R2 `self::` sem pular o segmento · R4 `mod x;` sem `x/mod.rs` · R7 membros do workspace · R8 tabela de dependências · R9 `#[path]` · R11a/R11b glob externo/irresolúvel sem dinamismo · R12 variante invertida · R13 `non_exhaustive` · R17 projeto sob `/tmp` vira scratch · R18 nome dinâmico vira Projeto · R19 aridade · R20 `mod x;` sem arquivo · R21 `Self` fora de `impl` · R22 item associado vira variante · R23 atributos lidos como filhos (o bug original). Aplicados no arquivo do worktree com o marcador `.TROCA-EM-CURSO`, restauração por conteúdo (sha verificado) + `os.utime`. 1ª rodada 22/24; R11a sobreviveu (meu teste estava confundido — o `Incerto` vinha também do projeto sob `/tmp`; teste endurecido para checar o MOTIVO "dinâmico") e R12 deu erro do runner (gatilho desatualizado pelo ajuste de variante); ambos refeitos na rodada final: mortos. Tabela completa em `diagnostics/vrs-porta-2026-10-10/dados/mutantes_resultado_{1,2}.json`. |
| 3 | juiz no corpus, recall ≥ 0,70 onde n ≥ 20, falso deny CP95 ≤ 1 %, p95 ≤ 150 ms | **NÃO cumprido no recall**: `nome` 5/20 = **0,25** (n = 20, piso 0,70). É propriedade do código da a6, idêntica antes e depois — a porta não regrediu, mas o aceite original do brief não passa. Falso deny 0/307, limite 0,97 % ≤ 1 %: **cumprido** no natural (no sanidade n = 200 dá limite 1,49 %, insuficiente por si). p95 128,1 ms ≤ 150: cumprido (ANTES 166,6 ms; a máquina esteve a 91–97 °C com juízes concorrentes — não é afirmação de desempenho). |
| 4 | `cargo fmt --all -- --check` · `clippy -p touring-code --all-targets -D warnings` · `cargo test -p touring-code --no-fail-fast` · `RUSTDOCFLAGS="-D warnings" cargo doc -p touring-code --no-deps` | **cumprido, rc 0 nos quatro** (rodada final com os fontes finais). Suíte de `touring-code`: 25 binários, **1.171 passaram · 0 falharam · 5 ignorados**; adaptador: 26/26. |
| 5 | teste de despacho do núcleo verde (rust deixa de responder «ainda não implementado») | **cumprido**: `despacho_do_nucleo_rust_responde_pelo_adaptador_real_nao_pelo_stub` + os testes de `lang/mod.rs` (tabela de extensões, registro sem duplicata, gramáticas) estão nos 1.171. |

## 4. Processos

Nenhum processo meu ficou vivo. Todo `cargo` por `scripts/hw-slot --class rust --wait 3600` (rc 75 → repete; nunca contornado); o juiz e as varreduras por `hw-slot` da classe `heavy`. A fila do `rust-1` esteve cheia (gates97, vl0-fase2b, vhtml, vtransv…) e a temperatura em 91–97 °C o tempo todo: meu build ANTES esperou ~1 h pelo slot e levou 11 m 23 s; o DEPOIS 11 m 48 s. Sem `kill`/`pkill`, sem `update-touring`/`propagate`, sem commit/push/stash/`rm -rf`/`cargo clean`, sem `~/.claude/settings.json`. Não li `corpus_holdout_*` nem `oss/`. O único arquivo meu fora do worktree/relatório é a evidência em `diagnostics/vrs-porta-2026-10-10/` (novo diretório; nada sobrescrito) e a cópia normalizada do corpus de sanidade em scratchpad (não altera o original). O `touring-wt-vrs` da a6 só foi lido; o build dele vai para `target/wt-vrs`.

## 5. Dados com sha

| artefato | caminho · sha |
|---|---|
| binário ANTES / DEPOIS | ver §2 (`binarios/d5b434d7`, `binarios/42830beb`, cada um com `touring.sha256` verificado por `sha256sum -c`) |
| juiz | `scripts/juiz_motor_vgp.py` sha16 `6c8a31b02075da1e` (só leitura) |
| corpus natural | `~/.claude/touring/vgp/corpus_rust_natural/entradas.jsonl` sha16 `141ced89653b0413` |
| corpus sanidade | `~/.claude/touring/vgp/corpus_rust/entradas.jsonl` sha16 `d268a5be45322251`; cópia normalizada (`contexto` = `[]`) sha16 `fd034e8c0a01dc6d` |
| por id (4 arquivos) | `diagnostics/vrs-porta-2026-10-10/dados/ids_{antes,depois}_{sanidade,natural}.json` — sha16 natural antes `e1a19081af65`, depois `9b9fab9bc106`; sanidade antes `d1bc6d8dc0f7`, depois `3846b64a72c6` |
| saídas do juiz | `dados/juiz_{antes,depois}_natural.{json,log}`, `dados/juiz_{antes,depois}_sanidade.log` |
| varredura real | `dados/varredura_{antes,depois}.{json,txt}` |
| mutantes | `dados/mutantes_resultado_1.json` (rodada 1) e `_2.json` (R11a, R12) |
| scripts | `diagnostics/vrs-porta-2026-10-10/scripts/` (`medir_ids.py`, `comparar_ids.py`, `explica_diff.py`, `mutantes.py`, `varredura_real.py`, `medida.sh`, `gates.sh`, …) |
| símbolos do contrato usados (linha no vrs2) | `Adaptador` `lang/mod.rs:75` · `Verificacao` `:44` (`medido` `:53`, `nao_medido` `:61`) · `parse` `:185` · `adaptador_de` `:113` · `Alvo` `politica.rs:27` · `Finding::candidato` `mod.rs:75` · `check` `mod.rs:136` · `is_inline` `mod.rs:218` |
| símbolos criados (`rust.rs`) | `Rust` `:56` · `alvo_do_projeto` `:93` · `motivo_nao_medido` `:104` · `atributos_do_item` `:332` · `Abst` `:588` · `SEM_CRATE` `:596` · `Ctx::abstem` `:1358` · `parece_variante` `:2243`; modificados: `path_attr_de` `:348`, `tem_attr` `:555`, `Resolucao` `:577`, `checar_variante` `:1851`, `empurrar` `:2264` |

## 6. Decisões e riscos

1. **O juiz não julga o `corpus_rust` (sanidade).** `entradas.jsonl` tem `contexto` como STRING (`"[]"`), e `rodar_motor` faz `for c in entrada.get("contexto")` → `TypeError` (linha 179). Normalizando, ainda sobra: `mensagem` ("delta=E0425 em …") não traz `error[E…]`, `classe_erro` devolve `nao_classificado`, o juiz responde "corpus sem positivo de SÍMBOLO — recall não medível". Medi o sanidade **por id** (`medir_ids.py` reusa o `rodar_motor` do juiz, argv idêntico) numa cópia normalizada. Decisão para a coordenadora: ou o gerador do corpus emite `contexto: []` e `mensagem` com `error[E…]`, ou o juiz tolera.
2. **O estrato `real` não existe.** `corpus_semeado/rust/` não existe (só html/javascript/tsx/typescript) e `~/.claude/touring/vgp/corpus_rust_real` (saída de `corpus_rust_real.py`) também não. O aceite foi medido no `corpus_rust_natural` (o corpus do brief da a6: 37·307) e no sanidade. O sanidade NÃO exercita `crate::`/`super::`/`pub use` do projeto real (`import` 0/20 e `atributo` 0/20 aqui são por construção: `std::collections::x` e campo de receptor `p.x` — o adaptador não tem modelo de `std` nem infere tipo de receptor).
3. **Deny nunca foi medido para Rust.** Nenhuma entrada de nenhum corpus Rust é inline nem `/tmp` avulso, os caminhos são de projeto → `Alvo::Projeto` só vira `warn`. O "0/307 falsos deny" é verdadeiro mas **vazio** quanto ao balde onde o adaptador de fato nega (`/tmp`, inline, `.cache` da raiz). O held-out selado precisa de negativos Rust nesse balde para o teto de 1 % significar algo.
4. **Ruído de `warn` alto, herdado da a6.** 214/307 negativos naturais têm ≥ 1 warn (era 222); no código real que compila, 63/130 arquivos. Não é `deny`, não conta no falso deny do juiz, mas vira `deny` em `/tmp`/inline. Causas medidas (varredura + negativos): (a) em `tests/`, `benches/`, `examples/` (40/40 arquivos com warn) `use <pacote>::X` resolve o PRÓPRIO arquivo de teste: `CrateInfo.nome` é o nome do pacote mas `root_file` é o teste; (b) `drop` ausente do prelúdio (59 negativos naturais); (c) itens locais de função (`const`, `fn`, `static` dentro de `fn`) não coletados; (d) `thread_local!`/`lazy_static!` em nível de módulo ligam o arquivo inteiro a "dinâmico". **Não corrigi (a)–(d)** — fora do escopo de "porta"; cada uma é de 1 a 15 linhas e eu as aplico com RED→GREEN e medida própria se a coordenadora autorizar (perguntei; sem resposta até o fim).
5. **`use super::*` não resolve.** `resolver_caminho` devolve abstenção para `…::super` puro; é o idioma de todo `mod tests`. 204 dos 344 ids naturais acionam isso (motivo dominante do `nao_medido`), ligam o escopo como dinâmico e rebaixam a veredito `unknown`. Resolver para `Encontrado::Modulo` do pai deve subir precisão (e não pode baixar o recall, que conta `warn`). Não aplicado.
6. **`nao_medido` muda o veredito** (`allow` → `unknown`) em 57 ids naturais sem mudar achado nenhum; consistente com a VL0 ("nunca vazio = limpo"), mas é uma decisão minha sobre O QUE é "tentou e não fechou" (§2.3). Se a coordenadora preferir o critério do Python (só erro de sintaxe), é trocar 3 linhas (`abstem`) e os testes `abstencao_*`.
7. **`Incerto` para projeto sob `/tmp`** é um uso do alvo "o motor sabe que pode errar" para preservar a regra `em_crate` da a6; a alternativa é largar a regra e deixar a política negar (mais agressivo). Mutante R17 e dois testes a travam.
8. **Dívida de qualidade herdada:** `extrair_itens` CC = 31, `coleta_usos` CC = 16, `folhas_de_uso` CC = 18 (limite 15), TDG grade C (0,72). Não refatorei: uma porta que muda a estrutura perde a rastreabilidade por id.
9. **Mudança de `.cache`:** a a6 negava para `/.cache/` em qualquer ponto do caminho; a política (§5.1) só reconhece o `.cache` da RAIZ do projeto. Mais conservador; sem efeito nos corpora.

## 7. Git

Nada commitado. `touring-wt-vrs2`: `lang/rust.rs` e `lang/rust/` são NÃO rastreados (`??`) sobre a VL0 também não commitada (renames em estágio + `lang/*.rs` novos); nenhum outro arquivo do worktree foi tocado por mim. `touring-wt-vrs`: intacto (status e `Cargo.lock` idênticos antes/depois do build). No plano B: este relatório e `diagnostics/vrs-porta-2026-10-10/` (35 arquivos, 1,5 MB), novos. Targets: `wt-vrs` (release da a6) e `wt-vrs2` (debug de teste + release).

## 8. O que NÃO foi verificado

- **O held-out selado** (`corpus_holdout_*`, `oss/`): não li, por ordem. Todos os números acima são de corpora de desenvolvimento.
- **O estrato `real`** (touring-identity/pyright): não existe em disco; não o gerei sem ordem (cargo check de verdade sob hw-slot).
- **Precisão em produção:** daemon/hook vivos, sessão real escrevendo `.rs` — nada rodou no binário instalado; o DEPOIS só foi exercitado pelo CLI `touring vgp check`.
- **O balde de deny para Rust** (§6.3) e o falso deny dele.
- **`cargo test`/clippy de `touring-server` e `touring-hook-handlers`**: não toquei neles e não os rodei com a porta (o `binary_e2e` da a6 foi no estado dela).
- **`touring e2e -j`, `elite_aggregate.py`, `loop_converged.py`**, a régua 50-dim do arquivo novo: não rodei.
- **Fidelidade do corpus natural:** parte dos "positivos" tem o nome definido no próprio snapshot (ex.: `code_mode_adherence` em `kpi.rs` L480 para um erro de L194) — ruído de reconstrução do corpus, que derruba o recall de qualquer verificador estático; não auditei os 22 positivos perdidos um a um (listados por `perdidos.py`).
- **Latência estável:** os p95 vieram com a máquina a 91–97 °C e juízes concorrentes; o ganho ANTES→DEPOIS não é evidência de desempenho.
