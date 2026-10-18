# RELATÓRIO VL0 — núcleo multi-linguagem do motor VGP (§1–§5 e §8)

> Sessão: subagente `vl0-nucleo` (backend Sonnet 5.5), delegação da coordenadora `team-lead` (substitui a touring-35/Kimi,
> que atingiu o limite de uso). Data: 09–10/10/2026. Nada commitado, nada propagado (ordem do brief).
> B = `/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base`.

## 1. Identidade

| item | valor |
|---|---|
| worktree | `/home/gabrielgadea/projects/touring-wt-vl0` · branch `feat/2026-10-09-vgp-vl0` · base `88c7cc8b` (HEAD intacto, sem commit) |
| escopo | `VL0-multilinguagem-spec.md` §1–§5 e §8; a §6 (camada 2 genérica, `crates/touring-pyright/`) é do `vl0-camada2-generica` — **não toquei** |
| spec emendada | `B/VL0-multilinguagem-spec.md` sha256 `5ce667479c16793a…` — bloco «Emenda 10/10/2026 (implementação do núcleo)» no fim do §3 (trait real copiado do código, `Verificacao`, `Transversal` — com a forma decidida `-> Verificacao` e o item 4 «estado da implementação» —, registro) + o item 4 do §5 (delta mora no `check`, só rebaixa `deny`); o resto da spec não foi tocado (autorização da coordenadora, 10/10) |
| builds | `CARGO_TARGET_DIR=…/target/vgp-test` (check/test/clippy) e `…/target/vgp-judge` (release); todo cargo por `scripts/hw-slot --class rust` |
| Context7 consultado | `/tree-sitter/tree-sitter-typescript` (`LANGUAGE_TYPESCRIPT` e `LANGUAGE_TSX` são duas `LanguageFn` distintas, `.into()` para `Language`) · `/websites/rs_tree-sitter` (`Language`, `Parser`, `Tree`: `Send + Sync`; `set_language` recusa ABI fora de `MIN_COMPATIBLE_LANGUAGE_VERSION..=LANGUAGE_VERSION`) · `/tree-sitter/tree-sitter-html` (`<script>`/`<style>` chegam como nó `raw_text`, a reinjeção é do adaptador HTML). A escolha *trait object × enum* é de linguagem (statics exigem `Sync`), sem doc externa. |

## 2. Feito, com evidência

### 2.1 Desenho

```
vgp/mod.rs        Finding(+alvo) · CheckInput · CheckOutput(+motivo) · check() = escolhe o adaptador → julgar()
vgp/politica.rs   Alvo{Externo,Projeto,Incerto} · roda_agora · severidade (a tabela do §5) · aplicar · aplicar_delta · verdict_dos
vgp/lang/mod.rs   trait Adaptador · Verificacao · trait Transversal · REGISTRO (1 linha/lang) · TRANSVERSAIS ·
                  lang_do_caminho (+EXTENSOES) · gramatica · parse (parser por thread × lang, via parse_bounded)
vgp/lang/python.rs + python/{modules,scope,stdlib_model,tests}.rs   o código Python de hoje, MOVIDO (git mv)
vgp/lang/{rust,typescript,tsx,javascript,html,css}.rs               stubs registrados: unknown "adaptador <lang> ainda não implementado"
vgp/lang/{cdn,classe_css}.rs                                        stubs de Transversal (ver §6, decisão 5)
```

**O adaptador MEDE, o `check` DECIDE.** `Adaptador::verificar(&CheckInput) -> Verificacao { achados, nao_medido }`:
o adaptador devolve achados — cada um diz *a quem se refere* (`Alvo`) — ou o motivo de não ter medido. Veredito, severidade
e delta são do `check`, uma vez, para toda linguagem (a severidade que um adaptador escreve no `Finding` é **sobrescrita**
por `politica::aplicar`: um adaptador novo não consegue inventar `deny`). Pipeline de `check` (mesma ordem do §3):
adaptador por `input.lang` (ausente → `unknown` + motivo "linguagem `x` sem adaptador registrado") → adaptador (+ transversais)
no proposto → política → delta contra o disco (só roda se há `deny` a rebaixar) → veredito = `deny` > `warn` > (`unknown` se
algo não foi medido, senão `allow`).

**Política §5 como função pura** (`politica::severidade(classe, alvo, roda_agora)`), precedência P4 → P1 → P2 → P3:
classe consultiva (`cdn_pin`, `classe_css`) = `warn` sempre · `Externo` = `deny` em qualquer balde · `Projeto` = `deny` só onde
roda agora (inline, `/tmp/`, `.cache/` da raiz do projeto) · `Projeto` fora disso e `Incerto` = `warn`.

**Onde mora o delta (`--disk-file`)** — no `check`, não no adaptador: `vgp::julgar` (`crates/touring-code/src/vgp/mod.rs`) roda o
adaptador (e os transversais) sobre o conteúdo do disco e chama `politica::aplicar_delta(achados, no_disco)` (`politica.rs`),
que rebaixa a `warn` ("pré-existente (já estava no disco): …") todo `deny` cuja chave (`classe`, `nome`) o disco já tinha. Só `deny`
é tocado; `warn` nunca; o que o disco não tinha segue negado; disco ilegível (adaptador devolve `nao_medido`) não suprime nada;
sem `--disk-file` não há nada a rebaixar. O CLI entrega `--disk-file` intacto a qualquer `lang`. **Teste por linguagem registrada**:
`vgp::lang::tests::delta_vale_para_toda_lang_registrada_o_pre_existente_no_disco_nunca_vira_deny` itera `langs_registradas()` (as 7)
e, para cada uma, sobre um adaptador de teste que responde por ela (`Falso(lang, achados)`), prova: disco com UM dos dois achados ⇒ o
pré-existente é `warn`, o introduzido `deny`, veredito `deny` · disco igual ⇒ tudo `warn` · sem disco ⇒ `deny`. Soma:
`cli::vgp::tests::disk_file_chega_ao_check_para_qualquer_lang` (rust, typescript, tsx, html, python: `lang` derivado do caminho **e**
`input.disk` intacto), os testes Python reais via `check` (`delta_so_nega_o_que_a_edicao_introduz`) e, para a camada de
política, `politica::tests::delta_*`. Mutantes: **D4** (delta só vale se `lang == "python"`) e **C3** (CLI descarta o disco fora de Python)
— ambos mortos; mais G4, D1, D2, D3 (§2.4).

**Registro**: `static REGISTRO: &[&dyn Adaptador]` (trait objects: o conjunto é aberto, cada adaptador mora no próprio arquivo, o
despacho custa uma chamada por `check`; `Sync` é o supertrait que deixa os adaptadores num `static`). Os sete `lang` do §2 **já
têm** módulo, linha de registro e stub: quem escreve o adaptador de Rust troca o corpo de `rust.rs` e nada mais — nenhuma
linha compartilhada para o merge. `gramatica("typescript")` ≠ `gramatica("tsx")` (`LANGUAGE_TYPESCRIPT` × `LANGUAGE_TSX`);
`ast::Lang::TypeScript` (TSX para tudo) **não** é tocado e não deve ser usado pelos adaptadores.

### 2.2 Arquivos tocados (shas dos finais; `wc -l`)

| arquivo | sha12 | linhas | o que mudou |
|---|---|---|---|
| `crates/touring-code/src/vgp/mod.rs` | `83d9c5071c8b` | 220 | reescrito: tipos + `check`/`julgar`/`medir` (despacho, política, delta, veredito) |
| `crates/touring-code/src/vgp/politica.rs` | `c67ddf190a35` | 236 | NOVO |
| `crates/touring-code/src/vgp/lang/mod.rs` | `c95d707b2ffd` | 533 | NOVO (trait, registro, `lang_do_caminho`, gramáticas, `parse`, 13 testes) |
| `crates/touring-code/src/vgp/lang/{rust,typescript,tsx,javascript,html,css}.rs` | `106413bd…` `cf99ab29…` `489d35c3…` `14e30992…` `18fbae6d…` `4247ad57…` | 49 cada | NOVOS (stubs + teste cada) |
| `crates/touring-code/src/vgp/lang/{cdn,classe_css}.rs` | `8465c6af…` `ac5e3930…` | 49 cada | NOVOS (stubs de `Transversal` + teste) |
| `crates/touring-code/src/vgp/lang/python.rs` | `03270afba334` | 827 | ex-`checks.rs` (rename); ver §2.3 |
| `…/lang/python/{modules,stdlib_model,tests}.rs` | `d89b8550…` `407e2d4b…` `4c9629e8…` | 770 · 466 · 687 | **rename puro, 0 linha alterada** |
| `…/lang/python/scope.rs` | `fe03b8758d2b` | 1270 | rename + 2 linhas (imports de teste `crate::vgp::scope::ScopeTree` → `…::lang::python::scope::ScopeTree`) |
| `crates/touring-code/src/vgp/sombra.rs` | `a95cbfa87b75` | 462 | linha ganha `lang`; `motivo` é o do motor; `com_log_em` (redirecionamento por thread); 2 testes novos |
| `crates/touring-hook-handlers/src/hooks/pre_edit_prevention.rs` | `13029b980311` | 1888 | `lang_do_caminho` no lugar de `.ends_with(".py")` + `lang: "python"`; módulo `vl0_lang_tests` (3 testes) |
| `crates/touring-hook-handlers/src/hooks/pre_write.rs` | `3f0e2fff92a3` | 1413 | idem |
| `crates/touring-hook-handlers/src/hooks/pre_write_tests.rs` | `9da9e222a51b` | 1277 | 2 testes novos |
| `crates/touring-server/src/cli/vgp.rs` | `b1e5878cbb08` | 471 | `lang_efetiva` (flag vence, senão caminho, senão python), `politica::verdict_dos`, `motivo`; 3 testes novos |
| `B/VL0-multilinguagem-spec.md` (árvore principal) | `5ce667479c16793a` | — | só a emenda no fim do §3 e o item 4 do §5 |

### 2.3 Python: o que mudou **na lógica** (para a integração com o worktree `touring-wt-vgp-attr`)

Movi sem reescrever. As únicas linhas de lógica tocadas, todas em `lang/python.rs` (ex-`checks.rs`), todas *mecânicas*:
`check_python` → `Python::verificar` (devolve só `run_checks` do proposto; delta e veredito saíram para `vgp::check`) ·
`run_checks` (remoção de `sev_nome`; `Finding::new(sev)` → `Finding::candidato(Alvo)`: `nome` ausente = `Projeto`, `escaped`
(import*/exec) = `Incerto`) · campo `Checks::deny_natural` **removido** · `Checks::sev(bool) -> &str` → `Checks::alvo(bool) -> Alvo`
(`externo` → `Externo`, senão `Projeto`) com os **5 sítios** `self.sev(..)` renomeados (`check_import_from`, `check_attribute`
×2 — módulo do projeto e função importada —, `check_membro`, `check_call`) · literais `"deny"` (módulo stdlib fechado) → `Alvo::Externo` e `"warn"` (dunder em
função, builtin tipado, `__dict__` com `__slots__`) → `Alvo::Incerto` · `out_find(.., sev: &str)` → `alvo: Alvo` ·
`verdict_dos` migrou para `politica.rs`. **Quem mexer na severidade do coletor (P0: "mesma severidade em arquivo e inline para
o mesmo alvo externo incerto") mexe agora em `Alvo::` nesses sítios, e a decisão por caminho já não está lá — está em
`politica.rs`.** Mapeamento antigo → novo, equivalente por construção: `deny` se `inline ∨ /tmp/ ∨ externo`, senão `warn`
⇔ `Externo` → deny · `Projeto` → deny se `roda_agora` · `Incerto` → warn.

**Única mudança de comportamento deliberada em Python**: `.cache/` da raiz do projeto passa a "rodar agora" (spec §5.1, que o
Python de hoje não tinha). Efeito medido no corpus: **nenhum** (os 6 negativos com `.cache` no caminho têm resultado idêntico).

### 2.4 Mutantes (§8.2–§8.3, e os demais invariantes) — 45 de 45 mortos

Runner `scratchpad/vl0/mutantes.py`: aplica UM mutante, roda os testes alvo sob `hw-slot`, exige `test result: FAILED`,
restaura o arquivo byte a byte (hash verificado). Resultado em `scratchpad/vl0/mutantes.json`.

| regra | mutantes (id → n.º de testes que o matam) |
|---|---|
| §8.2 sem adaptador ⇒ `unknown` + motivo, nunca `allow` | G1 devolver `allow` (2) · G2 sem o motivo (2) · stubs devolvendo `allow`: rust (1) typescript (1) tsx (1) javascript (1) html (1) css (1) · S-motivo texto do stub (6) |
| §8.3 política | P1 externo deixa de negar (7) · P2a projeto nunca nega (23) · P2b projeto sempre nega (4) · P3 incerto nega (8) · P4 consultiva nega (2) · R-inline (2) · R-tmp (23) · R-cache (2) · R-raiz `.cache` não ancorado (2) · G3 política não aplicada (25) |
| §8.3 delta | G4 delta não aplicado (2) · D1 rebaixa o que não é `deny` (1) · D2 chave só pelo nome (1) · D3 sem "pré-existente" (2) · **D4 delta só vale para `lang == "python"` (2)** |
| veredito: o adaptador não decide | G6 não-medido vira `allow` (7) · G8 motivo se perde (7) · G7 transversal nunca roda (1) · V1 `deny` não domina (6) · V2 vazio vira `warn` (9) |
| §2 vocabulário | L1 `.tsx`→typescript (2) · L2 `.mts` some (1) · L3 maiúsculas (1) · L4 gramática TSX=TS (1) · L5 TS=TSX (1) · L6 html sai do registro (2) · L7 transversal ao contrário (1) |
| §8.4 sombra e hooks | SB1 `lang` fixo na linha (1) · SB2 motivo do motor se perde (1) · H1 pre-edit só `.py` (2) · H2 pre-edit `lang` fixo (2) · H3 pre-write só `.py` (1) · H4 pre-write `lang` fixo (1) |
| §2 CLI | C1 ignora o caminho (1) · C2 ignora `--lang` (2) · **C3 descarta o `--disk-file` fora de Python (1)** |

Dois mutantes saíram "ERRO DO RUNNER" na 1.ª rodada (trecho de substituição ambíguo/reformatado pelo `rustfmt`: S-motivo e L3);
corrigi a *definição* e rodei de novo — mortos (6 e 1).

## 3. Estado por item do §8

| § | aceite | resultado |
|---|---|---|
| 8.1 | Python sem regressão **por id** | **Corpus de backup `corpus_v0d.bak-2026-10-09` (o vivo anterior, o que a coordenadora mandou usar): 762/762 ids idênticos** (veredito + `{classe,nome,severidade,linha}`, 3 snaps que não compilam saem como no juiz), 0 diferentes, 0 erros — `e935aaf1` × `b7f12000`. Juiz v2 **rc 0 PRONTO**, e **todos os campos batem com `k_v2_vivo.json`** (11/13 · 20/27 · recall deny 5/13 · 20/27 · avisos 58 · `perdidos` 9 ids, `falsos` 0, `defeito_de_corpus` 3 ids — listas de ids iguais). V3 (`--camadas 1,2`) **rc 1 só por p95** 1014,2 ms > 1000 (máquina a 95 °C); o resto bate com `k_v3_vivo.json` (12/13 · 22/27 · recall deny 6/13 · 22/27 · avisos 60 · `perdidos` 6 ids, `falsos` 0). **Corpus vivo novo** (`60ebf06b…`, referência `k_v2_ev.json`/`k_v3_ev.json`): 738/738 ids idênticos; v2 rc 0 e V3 rc 0 (11/13 · 21/27 e 12/13 · 23/27, 0 falsos — os números de `k_*_ev`). Ver §5. |
| 8.2 | `lang_do_caminho` em tabela + `check` sem adaptador ⇒ `unknown` com motivo; mutante `allow` morre | `lang_do_caminho_tabela_do_paragrafo_2` (27 casos: 14 extensões, caixa, `.d.ts`, dotfile `.py`, `<string>`, `Makefile`, `.scss`…) · `linguagem_sem_adaptador_e_unknown_com_motivo_nunca_allow` (`go`, `""`, `Python`, `ts`) · 6 testes de stub · mutantes G1, G2, stubs ×6, S-motivo mortos |
| 8.3 | delta e política no `check` para toda linguagem, teste e mutante por regra | `regra_de_politica_vale_para_adaptador_de_qualquer_lang`, `delta_vale_para_qualquer_lang…`, **`delta_vale_para_toda_lang_registrada_o_pre_existente_no_disco_nunca_vira_deny`** (um adaptador de teste por `lang` do REGISTRO, as 7) e `transversal_roda…` usam um `Falso` — provam que moram no `check`, não em Python; `disk_file_chega_ao_check_para_qualquer_lang` (CLI); 9 testes em `politica.rs`; mutantes P1–P4, R-×4, G3, G4, D1–D4, C3 mortos |
| 8.4 | hooks derivam `lang`; `.rs/.ts/.tsx/.html` gravam sombra com `lang` certo | pre-edit Edit (7 extensões) · MultiEdit (7) · desconhecidas (`.md`, `.json`, `Makefile` → 0 linha) · pre-write (7 + 3 desconhecidas); a linha ganhou o campo `lang`; mutantes H1–H4, SB1, SB2 mortos |
| 8.5 | `fmt --check`, `clippy -D warnings`, suítes 0 falhas | ver §2.5 abaixo |

### 2.5 Gates finais (fontes finais, após os mutantes e os testes de delta por lang)

| gate | comando (todos por `hw-slot --class rust`) | resultado |
|---|---|---|
| fmt | `cargo fmt --all -- --check` | rc 0 |
| clippy `touring-code` | `cargo clippy -p touring-code --all-targets -- -D warnings` | rc 0 |
| clippy `touring-server` | idem, `-p touring-server` | rc 0 |
| clippy `touring-hook-handlers` | idem, `--features pre-hooks,post-hooks` | rc 0 |
| rustdoc | `RUSTDOCFLAGS="-D warnings" cargo doc -p touring-code --no-deps` | rc 0 (1.ª vez rc 101: 19 erros de intra-doc link para itens privados, `<lang>` solto e `[1]` na linha de contrato da sombra — todos em arquivos meus; corrigidos) |
| testes `touring-code` | `cargo test -p touring-code --no-fail-fast` | **1146 passed · 0 failed · 5 ignored** (25 binários, 99 deles `vgp::`) |
| testes `touring-server` | `cargo test -p touring-server --no-fail-fast` com `TOURING_BINARY` e symlinks (ver abaixo) | **1802 passed · 0 failed · 34 ignored** (18 binários) |
| testes `touring-hook-handlers` | `cargo test -p touring-hook-handlers --features pre-hooks,post-hooks --no-fail-fast` | **738 passed · 0 failed · 4 ignored** |

Todos os gates acima rodaram **uma única vez, juntos, sobre a fonte final** (fase 2A, 10/10), depois do último edit de código. **Primeira
rodada de `touring-server` (09/10) teve 27 falhas** (14 em `binary_e2e` +
13 em `cli_wave6_e2e`/`f3_update_component_e2e`/`w12_5_per_project_daemon_e2e`): todas "binário não encontrado" — esses e2e procuram
`<workspace>/target/{debug,release}/touring[-daemon]`, e o brief manda usar `CARGO_TARGET_DIR` dedicado. Resolvi sem target novo:
`TOURING_BINARY=…/vgp-test/debug/touring` (o `binary_e2e` o honra), `cargo build -p touring-hooks --bin touring-daemon` no target
dedicado e dois symlinks em `touring-wt-vl0/target/debug/`. Nenhuma dessas falhas tocava o VGP.

## 4. Processos

Nenhum processo meu ficou vivo. Sem `kill`/`pkill` em processo touring, sem `update-touring`, sem `propagate`, sem commit/push/stash/`rm -rf`.
Um único `kill -INT` num processo **meu** (o runner de mutantes, enquanto esperava slot do `hw-slot`) para que o `finally` restaurasse o
arquivo mutado — verifiquei a restauração (hash). O `hw-slot` barrou por temperatura (> 88 °C, 93–97 °C a maior parte da noite) e por
`rust-1` ocupado por outros subagentes: por isso os lotes (suítes, mutantes, gates) rodam cada um **dentro de uma única** aquisição de
slot, em vez de um slot por mutante.

## 5. Dados com sha

| artefato | caminho / sha |
|---|---|
| binário ANTES (baseline) | `~/.claude/touring/vgp/binarios/e935aaf1/touring` · sha256 `e935aaf17d9a09d0…` |
| binário DEPOIS (congelado, VL0) | `~/.claude/touring/vgp/binarios/b7f12000/touring` + `touring.sha256` · sha256 `b7f12000576170c2742508651e044736505aed09f50c8404130d5ca4836382e6` · `cargo build --release -p touring-server --bin touring` (target `vgp-judge`). **Reconstruído a partir da fonte FINAL em 10/10 (touring-code e dependentes recompilados, relinkado às 00:25): mesmo sha256 byte a byte** — o selo vale para a fonte final. |
| juiz | `scripts/juiz_motor_vgp.py`: sha `0e693b139e7d9215` nas rodadas do corpus vivo novo (09/10) e `6c8a31b02075da1e` (o corrente, que passa `--disk-file` nos pares M) nas rodadas do corpus de backup (10/10) |
| corpus V0d | **vivo antigo = `~/.claude/touring/vgp/corpus_v0d.bak-2026-10-09/`** (`entradas.jsonl` `7b819af2`, `estado_no_instante/manifesto.json` `5606891e`) — o §8.1 oficial. **Vivo novo = `~/.claude/touring/vgp/corpus_v0d/`** após o P2 das 22:43Z (`entradas.jsonl` `28d93bcb`, mesmo manifesto `5606891e`; sha_tree `60ebf06bd598fd95` segundo a coordenadora). Durante a sessão eu li o corpus mudar sem aviso (o brief citava o manifesto `48f5b544` e um backup que ainda não existia), por isso as primeiras rodadas foram feitas contra o vivo novo, com baseline `e935aaf1` rerodado no mesmo corpus; a coordenadora então indicou o backup e eu repeti o §8.1 nele. |
| saídas do juiz | `scratchpad/vl0/juiz/{base-e935,vl0-b7f12000}_{v2,v3}.{json,log}` (vivo novo) · `bak-b7f12000_{v2,v3}.{json,log}` (backup) · por id: `ids_e935_vs_b7f12000.json` (738) e `ids_bak_e935_vs_b7f12000.json` (762) |

Números no **corpus de backup** (o §8.1 oficial; referência `k_v2_vivo.json`/`k_v3_vivo.json` = `e935aaf1`, 09/10) — **idênticos nos campos que não são tempo**:

| | `k_*_vivo.json` (e935aaf1) | b7f12000 |
|---|---|---|
| v2 (`--v5-estrito`) | PRONTO · 11/13 · 20/27 · falso deny 0/400 · 0/300 · avisos 58 · `perdidos` 9 · `defeito_de_corpus` 3 · p95 108,4 ms | PRONTO (rc 0) · 11/13 · 20/27 · 0/400 · 0/300 · 58 · 9 · 3 (mesmos ids) · p95 **64,4 ms** |
| V3 (`--camadas 1,2 --p95-ms 1000`) | PRONTO · 12/13 · 22/27 · 0 falsos · avisos 60 · `perdidos` 6 · p95 907,9 ms | 12/13 · 22/27 · 0/400 · 0/300 · 60 · 6 (mesmos ids) · **p95 1014,2 ms → rc 1 só por isso** (PENDENTE único) |
| por id (camada 1) | — | 762 ids, **762 idênticos** |

**Rodada PAREADA do baseline no mesmo corpus e na mesma máquina (juiz `6c8a31b0`, `e935aaf1`)**: v2 PRONTO · p95 **147,8 ms** (o meu: 64,4) · V3
**rc 1 também só por p95: 1011,8 ms** (o meu: 1014,2) — o baseline reprova o V3 pelo mesmo motivo, com a mesma folga de ruído; os demais
campos são os mesmos. Conclusão honesta: **o V3 só fecha com p95 ≤ 1000 ms numa máquina fria** (nas rodadas da coordenadora, 907,9 e
954,2 ms); com a máquina a 92–96 °C nenhum dos dois binários o faz. Não é regressão do VL0 (o código da camada 2 não foi tocado).

Números no corpus vivo novo (`28d93bcb`; "antes" = e935aaf1 rodado agora, "depois" = b7f12000):

| | antes | depois |
|---|---|---|
| v2 (`--v5-estrito`) | PRONTO · símbolo arquivo 11/13 · inline 21/27 · falso deny 0/400 · 0/274 · avisos em negativos 58 · p95 134,2 ms | PRONTO · 11/13 · 21/27 · 0/400 · 0/274 · 58 · **p95 67,2 ms** |
| V3 (`--camadas 1,2 --p95-ms 1000`) | NÃO PRONTO só por p95 **1007,2 ms** > 1000 · 12/13 · 23/27 · 0 falsos · 61 | PRONTO · 12/13 · 23/27 · 0 falsos · 61 · p95 **998,6 ms** |
| por id (camada 1) | — | 738 ids, **738 idênticos** |

Atenção: o p95 do V3 é ruído de máquina (93–97 °C, vários juízes concorrentes): o *antes* reprova por 7 ms e o *depois* passa por
1,4 ms — **não é evidência de ganho nem de perda**; a evidência da §8.1 é o por-id e os agregados iguais.

## 6. Decisões e riscos

1. **`Verificacao { achados, nao_medido }` em vez de `(String, Vec<Finding>)`** (desvio do §3). Motivo: o motivo do `unknown` precisa
   viajar (o §2 pede "com o motivo") e, tirando o veredito do adaptador, a regra "o adaptador não decide" fica imposta pelo tipo, não
   pela boa vontade (D8). Os seis agentes seguintes escrevem contra este tipo. **ACEITA pela coordenadora (10/10)** («o objeto nomeado é
   melhor que posicional, diretriz A2»); a spec foi emendada (§1, `5ce667479c16793a`).
2. **`Finding.alvo` (`#[serde(skip)]`) + `Finding::candidato`**; `Finding::new(.., severidade, ..)` continua para o que já vem decidido
   (camada 2, erros de CLI) — assinatura intacta para `touring-pyright`/`vgp_l2.rs`. O JSON do juiz não muda (só ganhou `motivo`).
3. **`Alvo { Externo, Projeto, Incerto }`**: três valores bastam para P1–P3; `cdn` com 404 confirmado é `Externo`.
4. **`roda_agora` inclui o `.cache/` da raiz** (§5.1). Sem efeito medido no corpus (ver §2.3); declarado porque é regra nova para Python.
5. **ADIÇÃO além da spec, ACEITA pela coordenadora (10/10): `Transversal`** (+ `cdn.rs` e `classe_css.rs`, stubs registrados em `TRANSVERSAIS`). A spec lista
   `cdn` (html · js/ts) e `classe_css` (html · tsx · jsx) como classes que **atravessam** linguagens; sem um ponto de extensão, os agentes
   VCDN e VCSS editariam `html.rs`/`typescript.rs`/`tsx.rs` junto com VHTML/VTS/VTSX — conflito garantido. Custa ~60 linhas e 2 testes;
   se não quiser, remova `Transversal`, `TRANSVERSAIS`, `transversais_de` e os dois arquivos. **Risco residual que eu nomeei — um
   transversal que devolve vazio não grita que não mediu (ausência ≠ zero) — foi resolvido por DECISÃO da coordenadora (10/10)**:
   `Transversal::verificar` devolve `Verificacao` e `vgp::medir` junta os `nao_medido` dos transversais aos do adaptador ("; ", na
   ordem do registro). **Isso NÃO está implementado no `touring-wt-vl0`** (b7f12000 ainda tem `-> Vec<Finding>`): entra pelo
   worktree `vtransv` na INT97. A spec (emenda do §3, itens do `Transversal` e 4) já registra a forma decidida.
6. **`lang::parse(lang, src)`/`gramatica(lang)`** centralizam o §2 e passam por `parse_bounded` (a TS 0.23.2 tem entrada de 86 bytes que
   fica para sempre em recuperação de erro — `ast/parser.rs:36-45`). Sem tocar em `ast::Lang`.
7. **Delta mora no `check` (`julgar` → `politica::aplicar_delta`), vale para toda lang, só roda se há `deny`** no proposto (equivalente: só `deny` é
   rebaixado; poupa a 2.ª passada e o clone de `overlays`). Registrado no §5, item 4, da spec; prova por lang em §2.1.
8. **CLI**: `--lang` explícito vence; senão `lang_do_caminho(path)`; senão `python` (inline segue python).
9. **Sombra**: a linha ganhou `lang` (campo aditivo — **ACEITO pela coordenadora**, «necessário para medir por linguagem»; `juiz_v4_sessoes.py` não lê `lang`) e o
   `motivo` passou a ser o do motor (antes sempre "falha interna do motor (fail-open)"; Python com parse falho agora diz "parse da
   gramática python falhou"). `sombra::com_log_em` é um redirecionamento por thread para teste de hook sem mexer em `env`.
10. **Risco de volume**: desde o §8.4, **toda** edição/escrita de `.rs/.ts/.tsx/.js/.html/.css` grava uma linha (fsync) na sombra —
    com verdict `unknown` e motivo "adaptador X ainda não implementado" até cada fase chegar. Em `ativo` não muda nenhuma resposta
    (`unknown` → nada). O V4 (`juiz_v4_sessoes.py`) deve filtrar por `lang == "python"` até lá.
11. **API pública que sumiu**: `touring_code::vgp::{checks, modules, scope, stdlib_model}` (únicos consumidores: o próprio vgp, `sombra.rs`
    e `cli/vgp.rs`, já ajustados; busca por `vgp/(checks|modules|scope|stdlib_model)` e `check_python` no worktree inteiro: 0 referências).
    `verdict_dos` agora é `vgp::politica::verdict_dos`.
12. **Fan-out**: cada agente edita só `lang/<lang>.rs` (ou `cdn.rs`/`classe_css.rs`); nenhuma linha de `mod.rs` muda. Para TS/TSX
    usem `lang::parse("typescript"|"tsx", …)`, nunca `ast::Lang::TypeScript`.
13. **Símbolos públicos sem consumidor de produção hoje** (REGRA #0): `lang::parse`, `lang::gramatica`, `lang::langs_registradas`,
    `Verificacao::{medido,nao_medido,nao_implementado}` (estes dois últimos têm: Python e os stubs), `sombra::com_log_em` (só testes de hook).
    `parse`/`gramatica` existem para os seis adaptadores; os testes os exercitam.

## 7. Git

Worktree `touring-wt-vl0`, branch `feat/2026-10-09-vgp-vl0`, base `88c7cc8b`, **sem commit**. `git status`: 5 renames (`git mv`:
`checks.rs`→`lang/python.rs`, `modules.rs`, `scope.rs`, `stdlib_model.rs`, `checks/tests.rs`; 2 deles também editados), 6 arquivos modificados, 10 novos
(`politica.rs`, `lang/{mod,cdn,classe_css,css,html,javascript,rust,tsx,typescript}.rs`). `git diff -M HEAD --shortstat`: 11 arquivos,
+518 −131 (sem contar os 10 novos). Também existem, **fora do controle de versão**: `touring-wt-vl0/target/debug/{touring,touring-daemon}`
(symlinks para `target/vgp-test/debug`, ver §8) e os scripts em `scratchpad/vl0/`.

## 8. O que NÃO foi verificado

- **Nada rodou no binário instalado nem no daemon vivo**: os hooks foram provados por teste de função (`run_returning`,
  `vgp_sombra_de_write`), não por uma sessão real escrevendo `.rs`/`.ts`; a conferência de que o shim/daemon grava a linha com `lang` fica para o deploy.
- **Nada novo rodou depois da repriorização da coordenadora (10/10)**: nenhum juiz, build ou cargo foi lançado; não há job meu em andamento
  (conferido por `pgrep`). A rodada pareada do baseline no corpus de backup (V3 `e935aaf1`, p95 1011,8 ms) já tinha **terminado** antes da mensagem.
  O selo exato do binário INTEGRADO fica para a INT97, conforme a coordenadora.
- **V3 por id**: só agregados e listas de ids (`perdidos`, `falsos`, `defeito_de_corpus`), não a comparação completa dos achados da camada 2 por id.
- **Efeito do `.cache/` além do corpus**: 6 negativos, nenhum mudou; não há positivo `.cache` — a regra está provada por teste e mutante, **não por precisão medida**.
- **p95 absoluto do V3**: máquina a 93–97 °C e juízes concorrentes. No corpus de backup deu 1014,2 ms (> 1000, único PENDENTE); no vivo novo 998,6 ms
  (passou por 1,4 ms) e o baseline `e935aaf1` 1007,2 ms — o V3 do `e935aaf1` na mesma máquina também reprova. Não há número limpo; o p95 não é
  evidência a favor nem contra o VL0 (o v2 deu 64–67 ms contra 108–134 ms do baseline sob a mesma carga).
- **Workspace inteiro**: clippy/testes só nos três crates tocados (+ doc de `touring-code`); `touring e2e -j`, `elite_aggregate.py` e `loop_converged.py`
  **não** rodei (não pedidos no brief; a coordenadora tem o juiz).
- **Binário × fonte**: o `b7f12000` original foi construído antes de remover o `#[derive(Default)]` de `Verificacao` e das correções de doc/rustfmt;
  **reconstruí a partir da fonte final e o sha256 é o mesmo** (compilação determinística, só mudou comentário, derive sem uso e `#[cfg(test)]`) — o selo vale.
  Como o binário é idêntico, todas as rodadas do juiz (feitas antes dos últimos edits de teste/doc) valem para a fonte final.
- **E2E de `touring-server`**: os 3 binários de teste que procuram `target/{debug,release}/touring[-daemon]` do workspace só passaram depois de eu
  apontar symlinks para o target dedicado e compilar `touring-daemon` (debug) — é artefato do target dedicado, não do VL0, mas fica registrado.
- Janela de CDN/rede, `classe_css`, `tsx` real: **nada disso existe ainda** (stubs).
