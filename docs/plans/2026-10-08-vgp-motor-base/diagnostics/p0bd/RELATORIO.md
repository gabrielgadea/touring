# P0B-d — "tipo fechado" corroborado pelas duas camadas (17/10/2026)

Worktree `/home/gabrielgadea/projects/touring-wt-p0bd`, base `d1e8c250` (a árvore estava idêntica à base antes de eu começar: diff de 0 bytes). Sem commit, sem `update-touring`.

**Patch**: `p0bd.patch` — sha256 `0dcb772a0797c7b820f5ffe7cd40e254e71f86976d240cdfa9fa29f58c1c414c`, 42.682 bytes, 9 arquivos (`p0bd.arquivos.txt`). Reconstrói: (1) o índice da base + `git apply --cached` dá o mesmo hash de árvore do worktree (`d20dd11c…`); (2) os 9 arquivos refeitos dos blobs da base num diretório temporário + `git apply` são `cmp`-idênticos ao worktree (`p0bd.reconstroi.txt`). Gerado por `scripts/build_patch.sh` com índice temporário (o índice real do worktree não foi tocado).

## Arquivos e por quê

| Arquivo | Mudança |
|---|---|
| `touring-code/src/vgp/politica.rs` | `Alvo::Fechado` (3º alvo: a ausência é certa para uma instância, o que a camada 1 não prova é que o valor seja uma). `severidade` → `warn` sozinho, em todo balde. Tabela P3 documentada. |
| `touring-code/src/vgp/mod.rs` | `Finding.receptor: Option<String>` (`#[serde(skip)]`) + `com_receptor`. |
| `touring-code/src/vgp/lang/python/stdlib_model.rs` | `TIPOS_FECHADOS: [(nome no modelo, caminho importável)]` = `[("stat_result","os.stat_result")]` e `tipo_fechado()`. A marca é explícita; str/int/list/dict/Path/set… não entram. |
| `touring-code/src/vgp/lang/python.rs` | O ramo `Tipo::Builtin(t)` saiu de `check_attribute` para `out_find_de_builtin` (CC de `check_attribute` 15→13): fechado → `Alvo::Fechado` + receptor; demais builtins seguem `Incerto` sem receptor. |
| `touring-code/src/vgp/sombra.rs` | `fundir_findings`: o ramo antigo (L2 deny escala, exceto `Incerto`) ficou byte a byte; novo `else if` promove a `deny` quando `corrobora_tipo_fechado` (L1 `Fechado` + receptores iguais pelo último segmento após `.`, inteiro). `Incerto` nunca é escalado — intocado. |
| `touring-pyright/src/pyright.rs` | `classe_receptora(msg)` = `classe_concreta` + `is_simple_name` (`X \| None`, `dict[…]`, `type[Foo]` → `None`). |
| `touring-hook-handlers/src/hooks/vgp_l2.rs`, `touring-server/src/cli/vgp.rs` | Os dois mapeadores L2→`Finding` (hook sombra e `--camadas 1,2`) carregam `.com_receptor(classe_receptora(&a.mensagem))`. |
| `touring-code/src/vgp/lang/python/tests.rs` | Testes novos + conserto de uma corrida pré-existente (abaixo). |

## Decisão campo × texto

**Campo.** A fusão vive em `touring-code`, que não depende de `touring-pyright`; ler `Cannot access attribute "x" for class "C"` ali duplicaria a gramática da mensagem do pyright num crate que não a conhece, e `sugestao` é texto para humano, não contrato. Com o campo, o parser é UM só (`classe_concreta`, a mesma função que decide o `Escopo` do achado) e a "mesma classe" é, por construção, a noção do P0B (b). Custo: 9 arquivos contra 4 na via do texto; os dois mapeadores ganham 1 linha cada. `#[serde(skip)]` (como `alvo`) em vez de `skip_serializing_if`: o JSON que o juiz lê fica **byte-idêntico**, mais forte que o campo opcional pedido. Sem classe no diagnóstico, ou campo ausente, a corroboração não ocorre (`warn`) — a ausência nunca concorda.

## Achado sobre o enunciado

O exemplo `import os` + `st = os.stat("x")` **não produz achado na camada 1**: só `unicodedata` e `math` são `Tipo::Stdlib`, então `os` não tem tipo e a cadeia morre no primeiro elo. O caso real `d94f8090d150` tem achado L1 (`classe: atributo`, "`stat_result` não tem atributo `st_m_size`…", visto em `p0b/…/novo_v3.porid.json`), logo a fonte real usa uma forma que a L1 tipa (`Path(…).stat()` / `pathlib.Path(…).stat()`). Os testes usam essas formas. Não ampliei a inferência (ordem do coordenador); tipar `os.stat(...)` seria trabalho à parte.

## RED → GREEN

Esqueleto sem comportamento (variante, campo, `TIPOS_FECHADOS=[]`, `classe_receptora→None`) + testes; `logs/rodada1/red_*.log` e `red_resumo.txt`:

```
touring-code    test result: FAILED. 175 passed; 4 failed; 1 ignored
  p0bd_membro_ausente_de_tipo_fechado_e_alvo_fechado_com_o_receptor_e_avisa_sozinho  (alvo Incerto ≠ Fechado)
  p0bd_o_modelo_de_tipos_fechados_confere_com_o_runtime   ("os.stat_result is the closed type layer 1 already types")
  sombra::p0bd_fechado_e_a_mesma_classe_na_camada_2_vira_deny   (… severidade: "warn", alvo: Incerto, receptor: None)
  sombra::p0bd_o_pyright_pode_qualificar_a_classe_so_o_ultimo_segmento_conta
touring-pyright test result: FAILED. 0 passed; 1 failed   (p0bd_classe_receptora_le_a_classe_do_diagnostico_de_membro)
touring-hook-handlers test result: FAILED. 0 passed; 2 failed   (p0bd_mapear_carrega_a_classe_receptora…, p0bd_as_duas_camadas_pelo_encaixe_real…)
```

Depois do conserto tudo verde (tabela de gates). Os testes de guarda (classe diferente / `str` / `Incerto` exec / alvo ≠ Fechado) passam desde o esqueleto por serem negativos — quem prova que mordem são os mutantes. Testes 1–7 do enunciado: 1 `p0bd_membro_ausente…` (+ `p0bd_os_demais_builtins…` para os que seguem `Incerto`), 2 `p0bd_fechado_e_a_mesma_classe…`, 3 `p0bd_o_pyright_pode_qualificar…` (`os.stat_result`, `posix.stat_result`), 4 `p0bd_classe_diferente_ou_sem_classe…`, 5 `p0bd_receptor_str_nao_e_fechado…`, 6 `p0bd_incerto_segue_sem_ser_escalado…` (+ o P0B (b) existente, verde), 7 `p0bd_o_modelo_de_tipos_fechados_confere_com_o_runtime` (`python3 -I`, Python 3.14.7; sem `python3` FALHA via `expect`; os controles `types.FunctionType` (só a condição de `__dict__` o rejeita), `ast.expr`, `logging.LogRecord`, `str/int/list/dict/object` confirmam o oráculo). Extras: `politica::p3_fechado_sozinho…`, `vgp_l2` (mapeador + fusão pelo encaixe real; `p0bd_a_corroboracao_nega_em_todo_balde_e_momento_a_politica_do_frag_nao_a_mascara` — ver a previsão), `cli/vgp.rs` (fusão do `--camadas 1,2`), `pyright` (`classe_receptora`), `p0bd_a_corroboracao_exige_o_alvo_fechado…`.

## Mutantes (`scripts/mut_p0bd.py`; cada um aplicado, testado, revertido, `cmp` byte a byte no fim)

| Mutante | Estado | Quem matou (teste legítimo) |
|---|---|---|
| M1 fusão ignora a igualdade de classe | MORTO | `p0bd_classe_diferente_ou_sem_classe_ou_so_uma_camada_segue_warn` |
| M2 `str` marcado fechado | MORTO | modelo×runtime, `p0bd_receptor_str…`, `p0bd_os_demais_builtins…`, `p0bd_incerto_segue…` |
| M3a `Fechado` nunca escala na fusão | MORTO | `p0bd_fechado_e_a_mesma_classe…`, `p0bd_o_pyright_pode_qualificar…` |
| M3b `Fechado` nasce `Incerto` no adaptador | MORTO | os dois acima + `p0bd_membro_ausente…` |
| M4 `Fechado` sozinho dá deny | MORTO | `politica::p3_fechado_sozinho…`, `p0bd_membro_ausente…`, `p4_forma_e_dinamico…`, `construtor_pontuado_pathlib…` |
| M5 critério sem a condição de `__dict__` | MORTO | `p0bd_o_modelo_de_tipos_fechados_confere_com_o_runtime` (controles `ast.expr`/`LogRecord`/`FunctionType`) |
| M6 só o nome exato | MORTO | `p0bd_o_pyright_pode_qualificar…` |
| M7 `ends_with` no lugar do segmento | MORTO | `p0bd_classe_diferente…` (`meu_stat_result`) |
| M8 adaptador sem receptor | MORTO | `p0bd_membro_ausente…`, fusão |
| M9 promove qualquer alvo com receptor igual | **SOBREVIVEU na rodada 1** (equivalente na prática: só `Fechado` carrega receptor) → teste `p0bd_a_corroboracao_exige_o_alvo_fechado…` → MORTO |
| M10 hook sem receptor | MORTO | `vgp_l2::p0bd_as_duas_camadas…`, `p0bd_mapear…` |
| M11 CLI sem receptor | MORTO | `cli/vgp::fusao_do_cli_corrobora…` |
| M12 `classe_receptora` aceita união | MORTO | `pyright::p0bd_classe_receptora…` |
| M13 corrida de `/tmp` revertida | MORTO | `nenhum_par_de_testes_compartilha_o_diretorio_temporario` |

Rodada final (`mutantes_final2/`, árvore final, sha do patch acima): `RED` (conserto revertido, 8 falhos) e M1…M13 todos MORTOS, com cada fonte restaurada por `cmp` e o `.TROCA-EM-CURSO` removido. Rodadas anteriores: `mutantes/` (1ª; nela M1 e M6 também apareciam mortos pela corrida pré-existente, ruído que já não existe), `mutantes_m9/`, `mutantes_final/`.

## Falha pré-existente consertada (REGRA #21)

`p0b_c_o_primeiro_parametro_do_init_e_a_instancia_e_nao_um_kwarg` falhou uma vez na rodada 2 (`left: [] right: [("self", ["z"])]`) e passou na 1. Causa: DOIS testes P0B (`…primeiro_parametro_do_init…` e `…base_irresolvida_new_metaclasse…`) usavam o MESMO diretório fixo `/tmp/vgp-t-p0b-c3`; os testes correm em threads paralelas do mesmo binário e cada um faz `remove_dir_all` + escreve — o fonte de um sumia debaixo do outro. Conserto: diretório próprio (`vgp-t-p0b-c3-abstem`) + guarda `nenhum_par_de_testes_compartilha_o_diretorio_temporario` (varre o próprio arquivo; era o único duplicado em 47 nomes). Estresse: 15 execuções de `vgp::` → 15× `181 passed; 0 failed` (`logs/estresse.txt`).

## Gates (árvore final, `logs/`; `scripts/gates_p0bd.sh` / `final_p0bd.sh`)

| Gate | Resultado |
|---|---|
| `cargo fmt --all -- --check` | rc 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | rc 0, 0 warnings (só o aviso de futura-incompatibilidade de `proc-macro-error2`, de terceiro) |
| `cargo test -p touring-code --no-fail-fast` | rc 0 — lib `971 passed; 0 failed; 1 ignored`; 25 suítes, 1228 passed, 0 failed |
| `cargo test -p touring-pyright --no-fail-fast` | rc 0 — lib `178 passed; 0 failed`; 10 suítes, 181 passed, 0 failed |
| `cargo test -p touring-hook-handlers --features pre-hooks,post-hooks --no-fail-fast` | rc 0 — lib `781 passed; 0 failed`; 2 suítes |
| `cargo test -p touring-server --no-fail-fast` | rc 0 — lib `1664 passed; 0 failed`; 19 suítes, 1813 passed, 0 failed |

Rodada 1 do `touring-server` deu 38 FAILED em 6 binários e2e: não é o código — esses testes procuram `<worktree>/target/debug/{touring,touring-daemon}` e o worktree usa `CARGO_TARGET_DIR` próprio (memória `worktree-e2e-exige-binarios-no-target-do-worktree`). Conserto: `cargo build -p touring-server -p touring-hooks` + symlinks em `<worktree>/target/debug/` (gitignorado, fora do patch) → 0 falhas. Build só para teste, nunca deploy.

## Previsão (corrigida a pedido do coordenador — e a correção dele NÃO se sustenta no código)

O coordenador informou: `d94f8090d150` está em `/tmp/eleitoral_probe/causa_raiz.py`, o FRAG rebaixa a ESCRITA em `/tmp` para warn, logo o P0B-d não mudaria esse id no juiz de escrita (0 mudança atribuível). **Medido, o P0B-d NÃO é mascarado pelo FRAG.** O FRAG age na severidade que CADA camada dá ao seu achado (L1: `politica::aplicar`/`roda_agora`; L2: `rebaixar_tmp_na_escrita`). O deny do tipo fechado não é severidade de camada nenhuma: nasce DEPOIS, em `fundir_findings`, da concordância das duas, e nada reaplica a política do momento sobre o achado fundido. Teste `vgp_l2::p0bd_a_corroboracao_nega_em_todo_balde_e_momento_a_politica_do_frag_nao_a_mascara`, pelo encaixe real (L1 `check_em` + `mapear` + `fundir_findings`, mesmo `contexto` nas duas camadas, como `sombra_check_em`), com `FONTE_STAT`:

| arquivo | momento | L1 sozinha | L1 + L2 `stat_result` | L1 + L2 `Foo` |
|---|---|---|---|---|
| `/home/u/proj/a.py` (fora de /tmp) | Escrita | warn | **deny** | warn |
| `/tmp/eleitoral_probe/causa_raiz.py` | Escrita | warn | **deny** | warn |
| `/tmp/eleitoral_probe/causa_raiz.py` | Execução | warn | **deny** | warn |
| `<string>` | Escrita | warn | **deny** | warn |

Consequência para os juízes: no juiz de escrita, `d94f8090d150` deve passar de warn para **deny** (não 0 mudança) — desde que, no corpus, a L1 dê o achado `Fechado` (já dá: ele aparece no `novo_v3.porid.json` do P0B) e a L2 dê o MESMO (nome, linha) com receptor `stat_result` (o `novo_v3.porid.json` do INT97 mostra `for class "stat_result"`). Os outros ids não mudam: sem L1 `Fechado`+L2 na mesma classe nada se move, a L1 sozinha segue warn e o JSON dos achados não mudou. **Se a previsão do coordenador é a intenção de projeto (um fragmento em `/tmp` escrito não deve negar nem o tipo fechado), isso é uma mudança de DESENHO que eu não fiz:** a fusão não conhece o caminho nem o momento, e o brief manda negar "independente da severidade que a camada 2 deu". O rebaixamento seria uma condição extra (p.ex. a promoção só quando `roda_agora`, passando o contexto até `fundir_findings` e seus 2 chamadores) — decisão do coordenador/Gabriel; o argumento a favor do deny em `/tmp`: um membro ausente de tipo fechado da stdlib é alvo "externo" (P1: nada que o projeto faça o cria), e P1 nega em todo balde.

## Pendências

- `os.stat(...)` direto não é tipado pela L1 (ver "Achado sobre o enunciado"); fora de escopo por ordem.
- A fusão não reaplica o delta contra o disco ao `Fechado` promovido (o `aplicar_delta` da L1 só rebaixa `deny`, e o `Fechado` nasce `warn`). Não vaza pré-existente porque `L2Resultado.achados` já é `proposto − disco`: um achado L2 que o disco já tinha nem chega à fusão. Não está comentado no código; fica registrado aqui.
- `fundir_findings` subiu a CC 13 (limiar de aviso 10, violação 15).
