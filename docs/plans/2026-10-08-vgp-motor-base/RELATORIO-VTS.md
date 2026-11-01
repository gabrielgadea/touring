# RELATÓRIO VTS — frente A: adaptadores TypeScript / TSX / JavaScript da camada 1 do motor VGP

> Autor: subagente `vts` · 10/10/2026 · brief: `briefs/2026-10-10-fanout-adaptadores.md` (+ emenda de aceite: controle negativo em código limpo) · **A entrega é hipótese até o juiz da coordenadora no held-out selado.**

## 1. Identidade

| Item | Valor |
|---|---|
| Worktree / branch | `/home/gabrielgadea/projects/touring-wt-vts` · `feat/2026-10-10-vgp-vts` (base `88c7cc8b` + VL0 aplicada, **nada commitado**) |
| `CARGO_TARGET_DIR` | `/home/gabrielgadea/projects/touring/target/wt-vts` |
| Núcleo copiado, byte a byte, de `touring-wt-vl0` | `crates/touring-code/src/vgp/lang/mod.rs` sha12 `c95d707b2ffd` · `crates/touring-server/src/cli/vgp.rs` sha12 `b1e5878cbb08` (conferidos com `sha256sum` depois da cópia; **não editados por mim**) |
| Arquivos meus | `crates/touring-code/src/vgp/lang/{typescript,tsx,javascript}.rs` + o diretório novo `crates/touring-code/src/vgp/lang/typescript/` (precedente: `lang/python/`) |
| Binário congelado FINAL | `~/.claude/touring/vgp/binarios/bc3c979f/touring` · sha256 `bc3c979fee274234744ac605b847060ac0aec2b1c0b957cb5ab35a2cade9b2ab` (+ `touring.sha256`) |
| Binários congelados SUPERSEDIDOS (não usar) | `d774ae2e` (anterior à linha `nao_medido` de pacotes externos, pedida na aceitação) · `6d147ea0` (construído com o snapshot ANTIGO de `cli/vgp.rs`; a lib era idêntica) · `63565ae5` (anterior à correção «varredura truncada vira nao_medido») |

## 2. Feito, com evidência

**Arquitetura.** Os três arquivos `lang/*.rs` são finos (`pub(super) fn verificar_fonte(lang, input)` → `typescript::analise::verificar`). O motor vive em `lang/typescript/` (≈ 11 mil linhas com a tabela gerada): `analise.rs` (Verificador, candidatos, `decidir`), `escopo.rs` (escopos/hoisting/`detectados`), `projeto.rs` (resolução de módulo: substituição `.js→.ts/.tsx/.d.ts`, `.mjs→.mts`, `.cjs→.cts`, `index`, `package.json` entry, `paths` por maior prefixo, `baseUrl`, `extends`/`references`, JSONC), `modulos.rs` (resumo de exports, `export *`, reexport, `import type`), `decls.rs`/`tipos.rs`/`inferencia.rs`/`jsdoc.rs` (formas de interface/type/classe, uniões, JSDoc parcial), `globais.rs` (tabela de globais + busca em vizinhança, orçamento 2500 arquivos / 32 MiB / 120 ms), `jsx.rs` (componente/prop), `tabelas.rs` (gerada por `gerar_tabelas.py`).

**Classes entregues** (a adaptador MEDE; `check` DECIDE a severidade por `Alvo`): `import` (nome não exportado + módulo relativo inexistente, com sugestão «você quis dizer»), `nome` (identificador livre), `atributo` (membro de tipo do projeto/builtin inferível localmente), `kwarg` (chave de literal contra interface/alias declarados localmente ou importados do projeto, `satisfies`, argumento de função), e no TSX `componente` e `prop`.

**Pacote externo nunca some calado (ajuste da coordenadora, f6):** todo import de pacote — `import`, `import type`, `export … from`, `require('x')`, `import('x')` literal, inclusive builtin `node:*` — vira **UMA linha** no `nao_medido` do arquivo, nunca um achado: «N pacote(s) externo(s) importado(s), não verificados na camada 1: react, lodash, @scope/pkg e mais K (a camada 2 do projeto verifica)». O nome é o do PACOTE (`lodash/fp`→`lodash`, `@scope/pkg/sub`→`@scope/pkg`, `node:fs/promises`→`node:fs`), cada um uma vez, até 6 nomes e «e mais K». Efeito: o arquivo que só tem esse motivo passa de `allow` a `unknown` (findings, quando há, continuam decidindo o veredito). Os membros via `.d.ts` seguem fora (desvio declarado, §6.1).

**Regras de segurança (direção segura = superaproximar e abster):** arquivo minificado/empacotado (linha > 2000 bytes) → `nao_medido`; `return` no topo fora de CommonJS → o arquivo roda num executor que injeta nomes → `nao_medido` dos nomes; `typeof X`/`window.X` marcam X como detectado; AMD/worker (`define`, `importScripts`, `requirejs`); DSL do `grammar.js` do tree-sitter; script clássico (sem import/require/export) → `nome` só como `Alvo::Incerto` (aviso), módulo ou código que roda agora (inline, `/tmp`) → `Projeto`; `<script src>` de página HTML na vizinhança (até 2 níveis acima quando não há `package.json`) nomeia a biblioteca do global; estreitamento (`&&`, `||`, `??`, `in`, `instanceof`, `assert*`/`invariant`), expando, formas abertas (índice, spread, herança opaca) nunca provam ausência; **varredura de vizinhança truncada**: global → `Incerto` (aviso), membro → `nao_medido` («a vizinhança não foi varrida por inteiro…»), nunca allow silencioso (corrigido nesta rodada: antes o candidato sumia em silêncio).

**Testes.** Módulo `vgp::lang::typescript`: **161 testes, 0 falhas** (unit + e2e via `vgp::check`, a mesma entrada dos hooks e do juiz). Suíte inteira `cargo test -p touring-code --no-fail-fast`: **1304 passed, 0 failed** (todos os alvos). Isolamento: os e2e usam `apoio::dir_temporario()` FORA de `/tmp` (em `/tmp` o `roda_agora` do núcleo vira deny e mascara a severidade medida).
**RED no stub** (`scratchpad/vts/red_c1.log`, sha12 `6f97d97a36d5`): trocando só na cópia de scratch o adaptador pelo stub da VL0, `27 failed; 23 passed` — os 27 são os testes de achado/`unknown`→`allow`; os 23 que «passam» no stub são os controles limpos (vácuos no stub, valem contra o motor real e são guardados pelos mutantes). **RED da linha de pacotes** (`red_f6.log`, mesma passagem, ANTES do patch): os 5 testes novos contra o código sem a implementação → `82 passed; 4 failed` (os 4 que exigem a linha falham com `nao_medido` vazio; o 5º, «só imports relativos não ganha a linha», é o controle e passa); depois do patch, `161 passed; 0 failed`. Os testes escritos depois do RED (precisão, controle negativo, sobreviventes de mutantes) foram acrescentados junto do código que cobrem; para os 6 sobreviventes da rodada final, o mutante em si é a prova de vermelho (o teste novo mata o mutante que a suíte anterior deixava vivo).

**Fontes consultadas** (Context7 indisponível ao subagente): TypeScript Handbook — Modules Reference `https://www.typescriptlang.org/docs/handbook/modules/reference.html` e Theory `https://www.typescriptlang.org/docs/handbook/modules/theory.html` (via WebFetch, regras de resolução/extension substitution); gramáticas **tree-sitter-typescript 0.23.2** (TS e TSX são gramáticas DISTINTAS) e **tree-sitter-javascript 0.25.0** (versões do `Cargo.lock`), `node-types.json`/`grammar.js` lidos do registro do cargo.

## 3. Estado por item do aceite

| # | Item | Estado | Evidência |
|---|---|---|---|
| 1 | TDD por classe (RED no stub → VERDE; controles limpos: símbolo existe, reexportado, pacote externo instalado, construção dinâmica → `nao_medido`) | **OK** | RED acima; controles em `testes.rs` (`export_star_nao_resolvido_vira_nao_medido…`, `commonjs_dinamico_e_nao_medido`, `require_com_variavel_e_nao_medido`, `formas_de_export_que_contam`, `reexport_nomeado_pede_o_nome_ao_modulo`, `pacote_instalado_ou_builtin_nunca_e_achado`) |
| 2 | Mutantes mortos e listados, ≥1 por regra de resolução e 1 «abstenção vira allow» | **OK** | 122 mutantes: **120 mortos, 2 EQUIVALENTES (J07, A19)**; «abstenção vira allow»: A15, A28, A29, **P01 (import de pacote some em silêncio) e P02 (linha omitida)** — todos mortos. Tabela em §5.1 |
| 3 | Juiz em `corpus_semeado/<lang>` com `--exigir-limite-superior`: recall de símbolo por classe ≥ 0,70 onde n ≥ 20; falso deny 0; p95 ≤ 150 ms | **OK (os três números)**; veredito do juiz = «NÃO PRONTO» só pela regra do limite superior (n pequeno de negativos: 135/200/9; exige n ≥ 299 — o teto ≤ 1 % é seu, no held-out) | §5.2 |
| 4 | Gates | **OK** — `cargo fmt --all -- --check` rc 0 · `cargo clippy -p touring-code --all-targets -- -D warnings` rc 0 · `cargo test -p touring-code --no-fail-fast` rc 0 (1304/0) · `RUSTDOCFLAGS="-D warnings" cargo doc -p touring-code --no-deps` rc 0 | `final_f6.estado` (passagem na fonte FINAL) |
| 5 | Despacho do núcleo continua verde | **OK** | `registro_sem_lang_duplicada_e_cada_adaptador_responde_pela_sua`, `linguagem_sem_adaptador_e_unknown_com_motivo_nunca_allow`, `toda_extensao_da_tabela_leva_a_uma_lang_com_adaptador_e_gramatica`, `gramaticas_aceitas_pelo_runtime_e_ts_tsx_sao_distintas` — verdes na suíte inteira |
| E | **Emenda**: controle negativo em código limpo (≥ 100 arquivos/linguagem, alvo 0) | **TS e TSX: 0 achados em 503 arquivos. JS: 11 achados (todos `nome`/`warn`, nunca deny) em 4 de 227 arquivos — cada resto com causa (§5.3)** | manifesto sha `b01e9c92fecb186c`, resultado `controle_f6.json` sha `dce2bed6cdb8c90b` |
| R | **Reprodutibilidade do binário congelado** (pedido posterior da coordenadora) | **OK** (refeita DEPOIS dos últimos mutantes) | §5.4 |
| P | **Linha `nao_medido` de pacotes externos** (ajuste da coordenadora) | **OK** | TDD (RED 4 falhas → 161 verdes), mutantes P01–P09 mortos, juiz inalterado, contagem de arquivos afetados em §5.5 |

## 4. Processos

- Todo trabalho pesado passou pelo `hw-slot` (classe `rust` para cargo/mutantes; `heavy` para juiz/controle) com o wrapper `na_fila.sh` (re-enfileira no rc 75 do gate térmico/fila; nunca contorna o slot). Passagens numa aquisição só: `final.sh f4` e `final5.sh f5` (rodadas anteriores ao ajuste da coordenadora) e, para o ajuste dos pacotes, **`f6.sh`** (RED dos testes novos contra o código sem a implementação → aplica o patch → fmt → unit → clippy → doc → suíte inteira → build release e congelamento `bc3c979f` → todos os 122 mutantes → touch + build + cmp) e **`f7.sh`** (A01/A05 com trecho atualizado + touch + build + cmp finais). Estado e logs em `scratchpad/vts/final_f6.estado`, `final_f7.estado`. A `f1` parou no clippy (1 lint: `needless_borrows_for_generic_args`, corrigido) — por isso as passagens são em estágios com parada cedo.
- **Restauração dos mutantes (mtime = AGORA):** `mutantes.py` aplica o mutante com `write_text` (mtime = agora), roda `cargo test -p touring-code --lib vgp::lang::typescript`, e restaura com `p.write_bytes(original); os.utime(p, None)` — `utime(p, None)` = **agora**, nunca o mtime original (nada de `copy2`/`cp -p`); a restauração é conferida por `sha256` do arquivo (`assert sha(p) == h0`) e o timeout mata o grupo de processos do cargo. Erros de runner (trecho ausente/ambíguo, mutante que não compila) NÃO contam como mortos. O build release de reprodutibilidade (§5.4) foi feito DEPOIS dos mutantes, com `touch` explícito.
- **Nenhum processo touring foi morto.** Só encerrei, depois de ler `/proc/<pid>/cmdline`, meus próprios waiters de fila (`na_fila.sh`/`hw-slot` do ciclo e4 — pids 552265/552267 — e do f3 — 3494972/3494973) para substituí-los por uma passagem completa. Zero `git stash/reset/checkout --`, zero `rm -rf`, zero `cargo clean`, zero rede na camada 1, held-out não lido, nenhum `update-touring`/`propagate`.

## 5. Dados, com sha

### 5.1 Mutantes (122: 120 mortos · 2 equivalentes)
Lista `mutantes_lista.json` sha `fb0169374daef515` (gerada por `mk_mutantes.py` sha `c0aa624a2b11dab2`; todo trecho verificado único no arquivo — «122 mutantes; trechos inválidos: []»). Resultados na fonte FINAL: `mutantes_f6.json` sha `62b4a759880b8148` (todos os 122, uma aquisição do slot; A01 e A05 deram `ERRO_DO_RUNNER` porque o patch dos pacotes mudou o trecho que mutam — erro de runner NÃO conta como morto) e `mutantes_f7.json` sha `e42bc88277d572a7` (A01 e A05 com o trecho atualizado: **2/2 mortos**). Mutantes novos P01–P09 cobrem a linha de pacotes (omitida, sem dedup, escopo `@scope`, subcaminho, limite da lista, «e mais N», arquivo sem pacote).

**Equivalentes (provados por leitura, não por conveniência):** J07 (`jsx.rs`: `forma.aberta || forma.origem != Projeto` → sem `aberta`) e A19 (`analise.rs`: idem) — `Forma::nao_tem(nome)` já é `!self.aberta && !membros.contains(nome)` (`tipos.rs:77`), então uma forma aberta nunca produz «ausente» e o `aberta ||` é guarda redundante defensiva; nenhuma entrada de programa distingue o mutante do original.

| id | arquivo | regra mutada | resultado | teste que mata (1º) |
|---|---|---|---|---|
| R01 | projeto.rs | regra de substituição .js -> .ts/.tsx/.d.ts removida | morto | `testes::resolucao_de_extensao_index_e_substituicao_js_para_ts` |
| R02 | projeto.rs | regra de substituição .mjs -> .mts removida | morto | `testes::mjs_e_cjs_resolvem_para_mts_e_cts` |
| R03 | projeto.rs | anexar extensão a um especificador sem extensão quebrada | morto | `testes::alias_de_paths_do_tsconfig_resolve_e_inexistente_e_achado` |
| R04 | projeto.rs | entrada index.* de diretório quebrada | morto | `testes::resolucao_de_extensao_index_e_substituicao_js_para_ts` |
| R05 | projeto.rs | alias de tsconfig `paths` ignorado | morto | `testes::alias_de_paths_do_tsconfig_resolve_e_inexistente_e_achado` |
| R06 | projeto.rs | padrão de `paths` mais LONGO deixa de vencer | morto | `projeto::tests::padrao_mais_longo_vence_e_o_asterisco_puro_pesa_zero` |
| R07 | projeto.rs | `extends` do tsconfig ignorado | morto | `testes::extends_do_tsconfig_traz_os_paths_do_arquivo_base` |
| R08 | projeto.rs | `references` do tsconfig ignorado | morto | `testes::references_do_tsconfig_trazem_os_paths_do_projeto_referenciado` |
| R09 | projeto.rs | query/hash (`?url`, `#x`) do especificador não é removida | morto | `testes::resolucao_de_extensao_index_e_substituicao_js_para_ts` |
| R10 | projeto.rs | `baseUrl` ignorado na resolução | morto | `testes::base_url_resolve_especificador_nu_e_ancora_os_paths` |
| R11 | projeto.rs | alvos de `paths` deixam de ser relativos ao `baseUrl` | morto | `testes::base_url_resolve_especificador_nu_e_ancora_os_paths` |
| R12 | projeto.rs | padrão `*` puro passa a PROVAR inexistência | morto | `testes::alias_coringa_puro_e_pacote_instalado_nunca_provam_ausencia` |
| R13 | projeto.rs | pacote instalado com o mesmo prefixo deixa de vencer o alias | morto | `testes::alias_coringa_puro_e_pacote_instalado_nunca_provam_ausencia` |
| R14 | projeto.rs | entrada `types/main` do package.json de um diretório ignorada | morto | `testes::diretorio_com_package_json_resolve_pela_entrada_declarada` |
| R15 | projeto.rs | sobreposição (overlay) do instante não conta como arquivo | morto | `testes::sobreposicao_do_instante_vale_mais_que_o_disco` |
| R17 | projeto.rs | caminho absoluto deixa de ser abstenção | morto | `testes::especificador_fora_do_alcance_e_abstencao_com_o_motivo` |
| E01 | modulos.rs | `export *` passa a re-exportar o default | morto | `testes::export_estrela_nao_reexporta_o_default` |
| E02 | modulos.rs | `export * from` deixa de ser seguido | morto | `testes::export_estrela_nao_reexporta_o_default` |
| E03 | modulos.rs | ABSTENÇÃO vira achado: `export *` não resolvido prova ausência | morto | `testes::export_star_nao_resolvido_vira_nao_medido_e_nao_achado` |
| E04 | modulos.rs | módulo marcado incompleto (CJS dinâmico) deixa de abster | morto | `testes::commonjs_dinamico_e_nao_medido` |
| E05 | modulos.rs | módulo com erro de sintaxe deixa de abster | morto | `testes::modulo_que_nao_da_para_ler_como_tabela_de_exports_e_abstencao` |
| E06 | modulos.rs | script sem import/export deixa de abster | morto | `testes::modulo_que_nao_da_para_ler_como_tabela_de_exports_e_abstencao` |
| E07 | modulos.rs | `export =` deixa de abrir a forma do módulo | morto | `modulos::tests::export_star_e_namespace_e_igual` |
| E08 | modulos.rs | `export default <expressão>` não conta como default | morto | `modulos::tests::exports_de_declaracoes_clausulas_e_default` |
| E09 | modulos.rs | `export { a as b }` exporta o nome ORIGINAL em vez do alias | morto | `modulos::tests::exports_de_declaracoes_clausulas_e_default` |
| E10 | modulos.rs | `module.exports = {…}` não reconhecido | morto | `modulos::tests::commonjs_estatico_fecha_a_tabela_e_dinamico_abre` |
| E11 | modulos.rs | `exports.nome = …` não reconhecido | morto | `modulos::tests::commonjs_estatico_fecha_a_tabela_e_dinamico_abre` |
| E12 | modulos.rs | `exports.a.b = …` aninhado deixa de abrir a tabela | morto | `testes::modulo_que_nao_da_para_ler_como_tabela_de_exports_e_abstencao` |
| E13 | modulos.rs | ABSTENÇÃO vira allow: spread em `module.exports` deixa de abster | morto | `modulos::tests::commonjs_estatico_fecha_a_tabela_e_dinamico_abre` |
| E14 | modulos.rs | ABSTENÇÃO vira allow: `Object.assign(module.exports…)` deixa de abster | morto | `testes::modulo_que_nao_da_para_ler_como_tabela_de_exports_e_abstencao` |
| E15 | modulos.rs | `export default function f` passa a exportar também o nome `f` | morto | `testes::export_default_nomeado_nao_exporta_o_nome` |
| E16 | modulos.rs | `export * as ns from` não reconhecido | morto | `modulos::tests::export_star_e_namespace_e_igual` |
| T01 | tipos.rs | forma ABERTA passa a provar ausência de membro | morto | `testes::classe_com_base_externa_ou_mixin_nunca_e_achado` |
| T02 | tipos.rs | classe com base externa/mixin deixa de abrir a forma | morto | `testes::classe_com_base_externa_ou_mixin_nunca_e_achado` |
| T03 | tipos.rs | `extends` não resolvido deixa de abrir a forma | morto | `testes::classe_com_base_externa_ou_mixin_nunca_e_achado` |
| T04 | tipos.rs | alias/enum fundido deixa de abrir a forma | morto | `testes::declaracoes_conflitantes_do_mesmo_nome_abrem_a_forma` |
| T05 | tipos.rs | assinatura de índice / tipo mapeado deixa de abrir a forma | morto | `testes::classe_com_base_externa_ou_mixin_nunca_e_achado` |
| T06 | tipos.rs | interseção com parte não resolvida deixa de abrir a forma | morto | `tipos::tests::alias_de_objeto_e_interseccao` |
| S01 | escopo.rs | `typeof x` passa a julgar o operando | morto | `escopo::tests::posicoes_que_nao_sao_referencia` |
| S02 | escopo.rs | nós de tipo/anotação passam a ser lidos como valores | morto | `escopo::tests::declaracoes_basicas_ligam` |
| S03 | escopo.rs | `for (var k in o)` deixa de içar `k` | morto | `escopo::tests::hoisting_de_var_e_de_funcao` |
| S04 | escopo.rs | `with` deixa de dissolver o escopo estático | morto | `escopo::tests::with_e_registrado` |
| S05 | escopo.rs | ligação local passa a valer como declaração de topo | morto | `testes::classe_local_que_esconde_a_de_topo_nao_e_a_de_topo` |
| I01 | inferencia.rs | variável REATRIBUÍDA mantém o tipo da declaração | morto | `inferencia::tests::variavel_reatribuida_ou_desestruturada_e_desconhecida` |
| I02 | inferencia.rs | variável DESESTRUTURADA mantém o tipo do todo | morto | `inferencia::tests::variavel_reatribuida_ou_desestruturada_e_desconhecida` |
| I03 | inferencia.rs | `this` em método estático passa a ser a instância | morto | `inferencia::tests::this_em_classe_de_topo_mas_nao_em_funcao_nem_estatico` |
| I04 | inferencia.rs | `this` em campo estático passa a ser a instância | morto | `inferencia::tests::this_em_classe_de_topo_mas_nao_em_funcao_nem_estatico` |
| I05 | inferencia.rs | `this` dentro de função aninhada passa a ser a da classe | morto | `inferencia::tests::this_em_classe_de_topo_mas_nao_em_funcao_nem_estatico` |
| G01 | globais.rs | a busca de globais lê dentro de node_modules | morto | `globais::tests::node_modules_e_o_proprio_arquivo_nao_contam` |
| G02 | globais.rs | busca por SUBSTRING em vez de palavra inteira | morto | `globais::tests::a_busca_acha_a_palavra_inteira_e_nao_um_pedaco_de_outra` |
| G03 | globais.rs | teto de arquivos da vizinhança removido | morto | `globais::tests::orcamento_estourado_marca_a_busca_como_incompleta` |
| G04 | globais.rs | globais de `@types/*` ignorados | morto | `globais::tests::globais_de_pacote_types_e_lib_do_typescript` |
| J01 | jsx.rs | spread de atributos deixa de desligar o julgamento do componente | morto | `testes::spread_em_jsx_desliga_o_julgamento_de_props_do_componente` |
| J02 | jsx.rs | elemento customizado (`my-el`) passa a ser julgado | morto | `testes::elemento_customizado_e_atributo_com_hifen_nao_sao_julgados` |
| J03 | jsx.rs | tabela de atributos do React vale mesmo sem React | morto | `testes::sem_react_instalado_ou_com_outro_jsx_nao_julga_atributo_intrinseco` |
| J04 | jsx.rs | React NÃO instalado deixa de desligar o julgamento intrínseco | morto | `testes::sem_react_instalado_ou_com_outro_jsx_nao_julga_atributo_intrinseco` |
| J05 | jsx.rs | `key`/`ref` deixam de ser universais | morto | `testes::prop_ausente_em_componente_do_projeto_e_achado` |
| J06 | jsx.rs | atributo com hífen (`data-*`, `aria-*`) passa a ser julgado | morto | `testes::atributo_desconhecido_em_elemento_intrinseco_do_react_e_aviso` |
| J07 | jsx.rs | props ABERTAS passam a provar ausência | EQUIVALENTE | `` |
| J08 | jsx.rs | atributo intrínseco passa de aviso a negação | morto | `testes::atributo_desconhecido_em_elemento_intrinseco_do_react_e_aviso` |
| J09 | jsx.rs | `React.FC<P>` deixa de dar as props do componente | morto | `testes::prop_de_componente_com_fc_e_props_abertas` |
| A01 | analise.rs | pacote (node_modules/builtin) passa a ser achado de módulo inexistente | morto | `testes::achado_e_linha_de_pacotes_convivem_e_o_achado_decide_o_veredito` |
| A02 | analise.rs | ABSTENÇÃO vira allow: dúvida sobre export deixa de ser registrada | morto | `testes::commonjs_dinamico_e_nao_medido` |
| A03 | analise.rs | ABSTENÇÃO vira achado: dúvida sobre export vira `import` | morto | `testes::commonjs_dinamico_e_nao_medido` |
| A04 | analise.rs | ABSTENÇÃO vira allow: import/require dinâmico deixa de ser registrado | morto | `testes::require_com_variavel_e_nao_medido` |
| A05 | analise.rs | ABSTENÇÃO vira allow: `nao_medido` nunca é preenchido | morto | `testes::achado_e_linha_de_pacotes_convivem_e_o_achado_decide_o_veredito` |
| A06 | analise.rs | guarda de ESTREITAMENTO (instanceof/in/guards) removida | morto | `testes::estreitamento_por_instanceof_ou_guarda_abstem` |
| A07 | analise.rs | guarda de REMENDO de protótipo no mesmo arquivo removida | morto | `testes::tipo_com_prototype_patch_no_projeto_nao_e_achado` |
| A08 | analise.rs | escrita expando (`obj.novo = 1`) passa a ser julgada | morto | `testes::escrita_em_propriedade_nova_e_expando_nao_e_achado` |
| A09 | analise.rs | membro de builtin deixa de exigir a busca por remendos em outras fontes | morto | `testes::tipo_com_prototype_patch_no_projeto_nao_e_achado` |
| A10 | analise.rs | nome livre deixa de exigir a busca de globais alheios | morto | `testes::global_de_pacote_types_nao_e_achado` |
| A11 | analise.rs | atribuição implícita em JS (`x = 1`) deixa de definir o global | morto | `testes::atribuicao_implicita_em_js_define_o_global` |
| A12 | analise.rs | tabela de globais conhecidos ignorada | morto | `testes::builtin_do_node_e_require_e_import_dinamico_de_pacote_tambem_entram_na_linha` |
| A13 | analise.rs | membro de tipo do PROJETO passa a negar como externo | morto | `testes::classe_js_define_membros_por_atribuicao_em_this` |
| A14 | analise.rs | varredura TRUNCADA deixa de rebaixar o achado a Incerto | morto | `analise::tests::decidir_mantem_derruba_ou_rebaixa_conforme_a_busca` |
| A15 | analise.rs | ABSTENÇÃO vira allow: erro de sintaxe deixa de abster o arquivo | morto | `testes::e_comercial_solto_em_texto_jsx_nao_derruba_o_arquivo_e_nao_vira_nome` |
| A16 | analise.rs | erro de sintaxe FORA de texto JSX passa a ser tolerado | morto | `testes::erro_de_sintaxe_e_nao_medido_e_nunca_achado` |
| A17 | analise.rs | erro que contém `{`/`<` passa a ser tolerado como texto JSX | morto | `testes::sinal_de_maior_ou_chave_solta_em_texto_jsx_continua_sem_medir` |
| A18 | analise.rs | spread no literal deixa de desligar o julgamento das chaves | morto | `testes::literal_com_spread_indice_ou_tipo_aberto_nao_gera_achado` |
| A19 | analise.rs | tipo ABERTO passa a provar chave excedente | EQUIVALENTE | `` |
| A20 | analise.rs | união: chave presente em UM dos membros passa a ser excedente | morto | `testes::chave_presente_em_um_membro_da_uniao_nao_e_excedente` |
| A21 | analise.rs | `instanceof` deixa de contar como estreitamento | morto | `testes::instanceof_in_e_invariant_fora_de_if_tambem_abstem_do_atributo` |
| A22 | analise.rs | `in` deixa de contar como estreitamento | morto | `testes::instanceof_in_e_invariant_fora_de_if_tambem_abstem_do_atributo` |
| A23 | analise.rs | `invariant(...)` deixa de contar como estreitamento | morto | `testes::instanceof_in_e_invariant_fora_de_if_tambem_abstem_do_atributo` |
| A24 | analise.rs | .d.ts deixa de ser abstenção de nomes/membros | morto | `testes::arquivo_de_declaracao_so_tem_os_imports_verificados` |
| A25 | analise.rs | import default passa a pedir um nome inexistente | morto | `testes::default_ausente_e_achado` |
| A26 | analise.rs | `export { a } from` deixa de pedir `a` ao módulo | morto | `testes::reexport_nomeado_pede_o_nome_ao_modulo` |
| A27 | analise.rs | módulo inexistente do PROJETO passa a ter alvo Externo | morto | `testes::arquivo_de_declaracao_so_tem_os_imports_verificados` |
| A28 | analise.rs | ABSTENÇÃO vira allow: especificador fora de alcance (URL, módulo virtual, caminho absoluto) deixa de ser registrado | morto | `testes::especificador_fora_do_alcance_e_abstencao_com_o_motivo` |
| N01 | analise.rs | nome que o arquivo detecta (typeof/window.X) volta a ser prova de ausência | morto | `testes::nome_que_o_proprio_arquivo_detecta_nao_e_achado` |
| N02 | escopo.rs | `typeof X` deixa de marcar X como detectado | morto | `escopo::tests::typeof_e_propriedade_de_window_marcam_nomes_detectados` |
| N03 | escopo.rs | `window.X` deixa de marcar X como detectado | morto | `escopo::tests::typeof_e_propriedade_de_window_marcam_nomes_detectados` |
| N04 | analise.rs | arquivo minificado/empacotado deixa de abster | morto | `testes::arquivo_minificado_ou_empacotado_nao_e_medido` |
| N05 | analise.rs | script que roda agora (inline, /tmp) deixa de negar | morto | `testes::nome_nega_em_tmp` |
| N06 | analise.rs | `require()` deixa de contar como sinal de módulo (script Node vira script de página) | morto | `testes::require_e_import_dinamico_marcam_o_arquivo_como_modulo` |
| N07 | globais.rs | globais de carregador AMD / worker deixam de ser conhecidos | morto | `testes::globais_de_carregador_amd_e_worker_sao_conhecidos` |
| N08 | analise.rs | CommonJS com `return` no topo passa a ser tratado como script de executor | morto | `testes::script_com_return_no_topo_roda_num_executor_que_injeta_nomes` |
| N09 | analise.rs | `return` DENTRO de função passa a contar como `return` no topo | morto | `testes::arquivo_limpo_e_allow_e_nao_unknown` |
| N10 | analise.rs | o DSL do tree-sitter passa a valer em qualquer arquivo | morto | `testes::grammar_js_do_tree_sitter_recebe_o_dsl_do_executor` |
| N11 | globais.rs | página HTML deixa de nomear bibliotecas pelos `<script src>` | morto | `globais::tests::pagina_htm_e_consulta_com_ponto_tambem_nomeiam_a_biblioteca` |
| N12 | globais.rs | extensão `.htm` deixa de ser página HTML | morto | `globais::tests::pagina_htm_e_consulta_com_ponto_tambem_nomeiam_a_biblioteca` |
| N13 | globais.rs | a chave de biblioteca passa a distinguir `maplibre-gl` de `maplibregl` | morto | `globais::tests::pagina_htm_e_consulta_com_ponto_tambem_nomeiam_a_biblioteca` |
| N14 | globais.rs | sufixo de versão `-3` deixa de ser removido do radical | morto | `globais::tests::radical_do_script_tira_consulta_versao_e_sufixos` |
| N15 | globais.rs | prefixo `pacote@versão` deixa de ser cortado do radical | morto | `globais::tests::radical_do_script_tira_consulta_versao_e_sufixos` |
| N16 | globais.rs | query string `?v=1` passa a integrar o radical | morto | `globais::tests::pagina_htm_e_consulta_com_ponto_tambem_nomeiam_a_biblioteca` |
| N17 | globais.rs | o `<script src>` deixa de provar que a biblioteca fornece o global | morto | `globais::tests::pagina_htm_e_consulta_com_ponto_tambem_nomeiam_a_biblioteca` |
| N18 | globais.rs | `<script src=…>` deixa de ser lido | morto | `globais::tests::pagina_htm_e_consulta_com_ponto_tambem_nomeiam_a_biblioteca` |
| N19 | globais.rs | script sem marcador deixa de procurar a página acima | morto | `globais::tests::script_sem_marcador_acha_a_pagina_ate_dois_niveis_acima_e_nao_mais` |
| N20 | globais.rs | a busca da página sobe três níveis em vez de dois | morto | `globais::tests::script_sem_marcador_acha_a_pagina_ate_dois_niveis_acima_e_nao_mais` |
| N21 | globais.rs | a busca da página passa a sair da raiz do projeto | morto | `globais::tests::script_sem_marcador_acha_a_pagina_ate_dois_niveis_acima_e_nao_mais` |
| A29 | analise.rs | ABSTENÇÃO vira allow: varredura truncada de outras fontes some em silêncio em vez de virar nao_medido | morto | `analise::tests::decidir_mantem_derruba_ou_rebaixa_conforme_a_busca` |
| A30 | analise.rs | varredura truncada de outras fontes passa a provar ausência de membro | morto | `analise::tests::decidir_mantem_derruba_ou_rebaixa_conforme_a_busca` |
| A31 | globais.rs | o prazo da varredura estoura sempre (prova a rota truncada pelo relógio) | morto | `globais::tests::pagina_htm_e_consulta_com_ponto_tambem_nomeiam_a_biblioteca` |
| P01 | analise.rs | ABSTENÇÃO vira allow: import de pacote externo some em silêncio | morto | `testes::a_linha_de_pacotes_lista_poucos_nomes_e_conta_o_resto` |
| P02 | analise.rs | ABSTENÇÃO vira allow: a linha de pacotes é omitida do nao_medido | morto | `testes::achado_e_linha_de_pacotes_convivem_e_o_achado_decide_o_veredito` |
| P03 | analise.rs | o mesmo pacote importado várias vezes é contado várias vezes | morto | `testes::import_de_pacote_externo_vira_uma_linha_de_nao_medido_e_nunca_um_achado` |
| P04 | analise.rs | pacote com escopo `@scope/pkg` é truncado em `@scope` | morto | `testes::import_de_pacote_externo_vira_uma_linha_de_nao_medido_e_nunca_um_achado` |
| P05 | analise.rs | subcaminho do pacote (`lodash/fp`) não é cortado | morto | `testes::import_de_pacote_externo_vira_uma_linha_de_nao_medido_e_nunca_um_achado` |
| P06 | analise.rs | a linha de pacotes deixa de limitar a lista | morto | `testes::a_linha_de_pacotes_lista_poucos_nomes_e_conta_o_resto` |
| P07 | analise.rs | o limite da lista de pacotes muda | morto | `testes::a_linha_de_pacotes_lista_poucos_nomes_e_conta_o_resto` |
| P08 | analise.rs | o «e mais N» conta todos, não o resto | morto | `testes::a_linha_de_pacotes_lista_poucos_nomes_e_conta_o_resto` |
| P09 | analise.rs | arquivo sem pacote externo passa a ganhar a linha | morto | `testes::arquivo_limpo_e_allow_e_nao_unknown` |

### 5.2 Juiz (binário congelado `bc3c979f`; `--exigir-limite-superior`; corpus `~/.claude/touring/vgp/corpus_semeado/<lang>`)

| Linguagem | Recall de símbolo por classe (acertos/n) | Falso deny (k/n · CP95 sup.) | Avisos em negativos | p95 |
|---|---|---|---|---|
| typescript | atributo 20/20 = 1,000 · import 20/20 = 1,000 · kwarg 20/20 = 1,000 · nome 20/20 = 1,000 (arquivo 80/80) | 0/135 · 2,19 % | 0 | 14,6 ms |
| tsx | atributo 20/20 · componente 20/20 · import 20/20 · kwarg 20/20 · nome 20/20 · prop 20/20 (arquivo 120/120) | 0/200 · 1,49 % | 0 | 18,9 ms |
| javascript | atributo **19/20 = 0,950** · import 20/20 · kwarg 20/20 · nome 20/20 (arquivo 79/80) | 0/9 · 28,31 % (**9 negativos só: precisão de JS NÃO PROVADA**; o controle de 227 é evidência de apoio) | 0 | 47,3 ms |

Recall, falso deny e avisos IDÊNTICOS aos do binário anterior (`d774ae2e`): a linha de pacotes só muda `allow`→`unknown` e nunca toca achado. JSONs: `juiz_typescript_f6.json` `69afc99b114dfcd4` · `juiz_tsx_f6.json` `a6c05e58598912cb` · `juiz_javascript_f6.json` `3444e8d68f899385`; quebra por classe: `juiz_por_classe.py` `0da6c49ddac2f27c`. **O positivo JS perdido** (`2a0f59401824`, `atributo`, `SEI Premium/assets/app.js`) deu **`unknown`, não allow**: a vizinhança sem `package.json` tem 5 arquivos de 14,2 MB; com cache frio a varredura passa dos 120 ms e o motor declara «a vizinhança não foi varrida por inteiro» (isolado e com cache quente, o mesmo caso dá deny em 23 ms). É comportamento declarado (§6.3), não regressão.

### 5.3 Controle negativo em código limpo (emenda)
Manifesto `manifesto_limpo.json` sha256 `b01e9c92fecb186c…` (gerador `manifesto.py` sha `c1b07d294f8c3ca3`; no scratchpad `…/scratchpad/vts/`), **503 TS/TSX + 227 JS**. Régua de «limpo»: TS/TSX = arquivo do programa do `tsc --noEmit -p <tsconfig>` do PRÓPRIO projeto, fora de `node_modules`/`.d.ts`, **sem diagnóstico algum** (tsc 5.8.3 em `analise/frontend` e `analise/apps/web`, 5.6.3 em `konverter/portal`, 5.9.3 em `konverter/apps/lexcore-desktop` e `packages/sdks/lexcore-sdk-ts`; Node v26.7.0). JS = `node --check` sai 0 e todo especificador relativo estático resolve para arquivo existente (JS só tem esse verificador oficial: sintaxe). Composição: typescript 186 (frontend 135, web 31, lexcore-desktop 11, lexcore-sdk-ts 6, portal 3) · tsx 317 (frontend 281, web 15, portal 13, lexcore-desktop 8) · javascript 227 (analise 123, .claude 43, touring 41, konverter 20). Corpora do juiz, held-out selado e transcritos excluídos por construção. Cada arquivo roda com `--content-file` == disco, sha conferido imediatamente antes.

| Linguagem | Arquivos | Vereditos | Achados por classe | Arquivos com achado | p95 |
|---|---|---|---|---|---|
| typescript | 186 | allow 86 · unknown 100 | **0** | 0 | ver `controle_f6.json` |
| tsx | 317 | allow 51 · unknown 266 | **0** | 0 | ver `controle_f6.json` |
| javascript | 227 | allow 107 · unknown 116 · warn 4 | `nome`/warn: **11** · deny 0 | 4 | ver `controle_f6.json` |

(Resultado `controle_f6.json` sha `dce2bed6cdb8c90b`, com o binário `bc3c979f`. O salto de `allow` para `unknown` em relação ao binário anterior (TS 183→86, TSX 310→51, JS 152→107 allow) é EXATAMENTE a linha de pacotes — achados idênticos.)

Limite superior unilateral 95 % do **deny** no controle: 0/503 TS+TSX → 0,59 %; 0/227 JS → 1,31 %; 0/730 → 0,41 %.

**Causa de cada resto (JS, 4 arquivos, 11 achados, todos aviso — script clássico):**
1. `matplotlib/backends/web_backend/js/nbagg_mpl.js:90` → `IPython` (global do Jupyter Notebook, host que o arquivo não declara).
2. `consultafns_js/pages_login_controllers_loginController.js:58` → `CryptoJS` (biblioteca carregada pela página; a página não está na vizinhança) **e linha 84 → `reason`: ACHADO VERDADEIRO** — usado fora do escopo do `function (reason)` da linha 78 (ReferenceError em runtime; `node --check` não vê).
3. `consultafns_js/pages_detalhada_services_detalhadaService.js:102` → `nomeResourceDetalhada`: **ACHADO VERDADEIRO** — o código define `this.nomeResourceDetalhada` (linha 22) e usa o nome nu.
4. `algorithmic-art/templates/generator_template.js` → `noStroke`, `rect`, `noise`, `createVector`, `cos`, `sin`, `saveCanvas`: **p5.js em modo global** (o `viewer.html` irmão carrega `p5.min.js`; o radical `p5` não é o nome das funções que o p5 injeta, e o template não é citado por nenhuma tag `<script>`).
Ou seja: 2 verdadeiros positivos num corpus declarado «limpo» (a régua do JS é só sintaxe), 8 avisos de global de hospedeiro/biblioteca de página e 1 `IPython`. `maplibregl` (que estava na lista) foi eliminado pela regra de `<script src>` + subida de 2 níveis até a página.
**`unknown` (nada medido, nunca achado):** além da linha de pacotes (§5.5), TS 3 e TSX 7 = erro de sintaxe do tree-sitter em TS válido (`import("x").T<U>` não é parseado; arquivo com erro nunca vira achado) ou módulo importado com esse erro; JS 71 = 49 minificados/empacotados, 12 `import()/require()` dinâmicos, 7 `return` no topo (executor), 3 especificadores `node_modules/.pnpm` fora de alcance.

### 5.4 Reprodutibilidade do binário final
Fonte final = os sha12 abaixo. Provada **duas vezes** (a segunda, `final_f7.estado`, DEPOIS dos últimos mutantes, que restauram por conteúdo + `os.utime(p, None)`): `find lang -name '*.rs' -exec touch {} +` + `touch cli/vgp.rs` (mtime = AGORA) → `cargo build --release -p touring-server --bin touring` (`repro_f7.log`: **`Compiling touring-code`, `touring-server`; Finished em 6m42s** — recompilou de fato) → `sha256sum` do binário refeito = `bc3c979fee274234744ac605b847060ac0aec2b1c0b957cb5ab35a2cade9b2ab` = sha do congelado → **`cmp` rc 0** (bit a bit). A primeira (`final_f6.estado`) também deu `cmp` rc 0.

| Fonte final | sha12 | linhas |
|---|---|---|
| `lang/typescript.rs` | `8b5e60d9747c` | 64 |
| `lang/tsx.rs` | `cfc38eb9d80f` | 24 |
| `lang/javascript.rs` | `cfefe2962813` | 25 |
| `typescript/analise.rs` | `4c155cc329ca` | 1166 |
| `typescript/decls.rs` | `68a47a107c11` | 798 |
| `typescript/escopo.rs` | `e9ed4fe2ee37` | 1151 |
| `typescript/globais.rs` | `74fcaa8a2706` | 742 |
| `typescript/inferencia.rs` | `c09ab81d786b` | 481 |
| `typescript/jsdoc.rs` | `f1b209859a60` | 348 |
| `typescript/jsx.rs` | `8fa606a68eb3` | 186 |
| `typescript/modulos.rs` | `507435bad5c3` | 663 |
| `typescript/projeto.rs` | `eaa7e864ac19` | 554 |
| `typescript/tabelas.rs` (gerada) | `a90b76c5eac4` | 3116 |
| `typescript/testes.rs` | `fffeeeddea63` | 1466 |
| `typescript/tipos.rs` | `887a2563657f` | 524 |
| `typescript/util.rs` | `324e8df24b48` | 204 |

### 5.5 Quantos arquivos ganham a linha de pacotes
Contagem com o binário `bc3c979f` (`conta_linha.py` sha `3b554939a429446c` → `conta_linha_f6.json` sha `04090d57bbb1bc03`; mesma chamada do juiz, com `--disk-file` e overlays do `estado_no_instante`).

| Conjunto | Arquivos | Com a linha | Observação |
|---|---|---|---|
| Controle limpo — typescript | 186 | **97** (52 %) | allow 86 · unknown 100 |
| Controle limpo — tsx | 317 | **263** (83 %) | allow 51 · unknown 266 |
| Controle limpo — javascript | 227 | **54** (24 %) | allow 107 · unknown 116 · warn 4 |
| Semeado typescript | 215 | 103 (positivos 32/80 · negativos 71/135) | positivos continuam deny 20 / warn 60 |
| Semeado tsx | 320 | 267 (positivos 102/120 · negativos 165/200) | positivos deny 20 / warn 100 |
| Semeado javascript | 89 | 1 (positivos 0/80 · negativos 1/9) | — |

**Consequência de produto que a coordenadora deve ter presente:** com a linha, a maioria dos arquivos TS/TSX reais deixa de ser `allow` e passa a `unknown` (fail-open, nunca bloqueia) — o `motivo` agora diz sempre quais pacotes não foram olhados. O juiz não é afetado (nega/avisa por achado; a linha só existe em `nao_medido`).

## 6. Decisões e riscos (para a coordenadora decidir)

1. **Pacote externo (`node_modules`).** Cada import de pacote vira UMA linha de `nao_medido` por arquivo (§2/§5.5) — nunca achado. **Membros de pacote NÃO são verificados** e `Alvo::Externo` via `.d.ts` **NÃO foi implementado** (o brief pede `.d.ts` lido e membro comprovadamente ausente → Externo): desvio declarado; implementar é trabalho novo (camada 2 do projeto cobre, como diz a própria linha).
2. **JS clássico: warn, não deny, e não abstenção.** `nome` livre em script sem import/export/require é `Alvo::Incerto` (aviso) porque a página ou o executor pode fornecê-lo. Tornar isso `nao_medido` zeraria os 11 avisos do controle, mas derrubaria o recall de `nome` em JS para ≈ 0,10 (medido numa rodada anterior: quase todo positivo semeado é script clássico). A escolha entre recall e ruído de aviso é sua; hoje: recall `nome` JS 20/20, avisos em negativos 0 no juiz, 11 avisos no controle.
3. **Varredura de vizinhança com orçamento (2500 arquivos / 32 MiB / 120 ms de parede).** O relógio torna o desfecho dependente de carga/cache em vizinhanças grandes; o desfecho truncado é SEMPRE conservador e declarado (global → aviso; membro → `unknown`), nunca deny nem allow silencioso (testes `decidir_…`, `vizinhanca_grande_demais_…`, mutantes A29–A31). 120 ms mantém o p95 ≤ 150 ms; foi 70 ms até esta rodada.
4. **Sem `package.json`/tsconfig** a vizinhança de busca era só o diretório do arquivo; agora inclui, sem recursão, as páginas dos 2 níveis acima (dentro da raiz do projeto) — é o que acha o `<script src>` que carrega o script.
5. **Gramática.** `import("x").T<U>` (tipo de import com argumento de tipo) não é parseado pelo tree-sitter-typescript 0.23.2: o arquivo abstém por sintaxe (3 de 421 arquivos do frontend). Nunca vira achado.

## 7. Git

Nada commitado, nada empurrado. No worktree: o diretório novo `crates/touring-code/src/vgp/lang/typescript/` (untracked) e `lang/{typescript,tsx,javascript}.rs`; `lang/mod.rs` e `cli/vgp.rs` são a cópia da VL0 (não meus). Outros arquivos que `git status` lista (renomeações de `checks.rs`→`lang/python.rs`, `lang/{cdn,css,html,rust,classe_css}.rs` etc.) vieram com a VL0 aplicada e não foram tocados por mim.

## 8. O que NÃO foi verificado

- Membros de pacote externo por `.d.ts` (`Alvo::Externo`): **não implementados** (§6.1). O `nao_medido` por pacote EXISTE desde a f6.
- Tipos do DOM não são modelados; JSDoc é parcial (tipo simples/objeto/`@param`/`@type`); `prop` de elemento intrínseco do React (`<div foo>`) é `Incerto`/aviso.
- O controle JS prova só «não negou código que passa em `node --check`», não semântica; TS/TSX são provados pelo `tsc` do projeto (versões acima), mas só nos 5 projetos listados (não rodei Vue/Svelte/Astro, `.mts/.cts` não aparecem no manifesto).
- O held-out selado nunca foi lido; o teto de falso deny ≤ 1 % não está provado (o juiz exige n ≥ 299; os negativos semeados JS são só 9).
- Mutantes: todos os 122 rodaram na fonte final (`f6`; A01/A05 completados na `f7` por trecho desatualizado). O runner roda só o módulo `vgp::lang::typescript` (não a suíte inteira) por mutante.
- Desempenho medido com cache de arquivos quente na maior parte; cache frio em vizinhança de dezenas de MB gera `unknown` (§5.2).
- Concorrência de hooks reais: o controle roda 1 processo `touring vgp check` por arquivo, 3 em paralelo; não medi o motor sob o daemon com muitas sessões.
