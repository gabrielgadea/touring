# Correções de qualidade, CI e catálogo -- relatório (17/10/2026)

Worktree `~/projects/touring-wt-ecc-qa` (HEAD `98301443`, snapshot do int97). Sem commit, push, `update-touring`, stash.
Target reutilizado: `CARGO_TARGET_DIR=~/projects/touring/target/wt-p0b`. Todo cargo sob
`hw-slot --class rust --label ecc-qa` (a vaga ficou ocupada por pipelines de outras sessões e pelo teto térmico, 96 C: esperei; um
worker `worker.sh` segurou UMA vaga e a soltou após 150 s ocioso, para encadear os jobs sem reentrar na fila).

Patch: `qa.patch` (sha256 `d1cf809474a7776ed4a591826bea6aba1ee73f7ad2534a20b548846494a922ca`), 3223 linhas, gerado com `git add -N` + `git diff`.

## Gates finais (código final, após fmt)

| Gate | Comando | rc |
|---|---|---|
| fmt | `cargo fmt --all -- --check` | 0 |
| clippy | `cargo clippy -p touring-analysis -p touring-quality -p touring-cli -p touring-server --all-targets -- -D warnings` | 0 |
| test | `cargo test -p touring-analysis --no-fail-fast` | 0 (1161 ok, 0 falhas, 4 ignorados) |
| test | `cargo test -p touring-quality --no-fail-fast` | 0 (507 ok, 0 falhas) |
| test | `cargo test -p touring-cli --no-fail-fast` | 0 (699 ok, 0 falhas, 4 ignorados) |
| test | `cargo test -p touring-server --no-fail-fast` | 0 (1821 ok, 0 falhas, 34 ignorados) |

O `clippy` só emite o aviso de dependência `proc-macro-error2 v2.0.1` (future-incompat de terceiro, não erro).
`touring-hook-handlers` não foi tocado. Ambiente: os e2e do `touring-server` procuram `target/debug/{touring,touring-hook,touring-daemon}` DENTRO
do worktree; sem eles 32 testes em 6 binários de integração falhavam ("failed to spawn touring binary", primeira rodada, rc=101). Construí `touring-hooks` e criei
3 symlinks em `touring-wt-ecc-qa/target/debug/` apontando para `wt-p0b/debug/` (nenhum target novo); depois disso, 0 falhas.

## Vermelho no original / verde no consertado

Método: para cada conserto, o comportamento ORIGINAL volta ao ponto de injeção (ou o código original inteiro, no `cicd.rs` e no
`f2_4_secrets.rs`) mantendo os testes novos; roda; restaura. Scripts: `redgreen.py`, `redgreen_cicd.py`, `redgreen2.py`.

| Conserto | vermelho (rc, falhas) | verde (rc, ok) |
|---|---|---|
| F4.7 cicd (código original + testes novos) | 101 (15 falhas, 20 ok) | 0 (47 ok, 0 falhas) |
| F2.4 alinhamento do rótulo + allowlist | 101 (5 falhas, 85 ok) | 0 (91 ok, 0 falhas) |
| kpi (scrub_home volta a identidade) | 101 (5 falhas, 92 ok) | 0 (97 ok, 0 falhas) |
| component (filtro de backup desligado) | 101 (2 falhas, 6 ok) | 0 (8 ok, 0 falhas) |
| scan (só detector de hook + parsing antigo) | 101 (6 falhas, 7 ok) | 0 (13 ok, 0 falhas) |

Falhas no vermelho do cicd: a_commented_out_uses_is_not_a_finding, a_shell_glob_in_an_earlier_step_does_not_hide_later_lines, concatenated_workflows_do_not_share_permissions, local_action_does_not_borrow_the_next_actions_ref, rule_a_oidc_at_workflow_scope_is_high_and_capped, rule_a_write_all_and_flow_style_count_as_oidc, rule_b_job_with_oidc_and_cache_is_high_for_every_cache_action, rule_b_oidc_inherited_from_the_workflow_counts, rule_c_pull_request_target_with_head_checkout_is_critical, rule_d_checkout_without_persist_credentials_in_a_write_job_is_medium, rule_d_negatives_disabled_read_only_and_other_actions, rule_e_no_cache_is_not_asked_of_a_job_holding_an_oidc_token, rule_e_rust_cache_counts_as_a_cache, rule_f_mutable_third_party_action_with_write_cannot_read_diamond, structural_findings_lead_the_evidence_even_when_a_smell_is_more_frequent.

## Mutantes (56 aplicados em cópia reversível, arquivo sempre restaurado; todos compilam e todos morreram: 56/56)

`mutate.py`: aplica 1 troca exata por mutante (a troca é conferida com `count == 1`), roda os testes do pacote, "morto" = compilou e algum
teste falhou. Por item: F4.7 estrutura/regras 22/22, F4.7 evidência 3/3, F2.4 8/8, `owasp_scan` e `scan` 10/10,
component 5/5, kpi 8/8. Na primeira rodada 2 sobreviveram/inválidos (MC14 ordem do achado estrutural: faltava o teste em que um
smell mais frequente passa à frente; MS5: padrão desatualizado após refatoração); corrigi o teste/padrão e eles morreram.
Mutante equivalente conhecido (não aplicado): em `scan_source`, `owasp.blocks() ||` é redundante HOJE porque todo padrão do registro tem
severidade >= 7.2 (cada match sozinho já é P0); fica porque o F2.1 soma severidades (vários P1 reprovariam juntos).

## F4.7 antes/depois (cópias de `release.yml`, `ci.yml` e dois outros, SEM editar os YAML)

Antes: `touring-quality` instalado (30.4.96, mesma lógica do HEAD para F4.7). Depois: binário `wt-p0b/debug/touring-quality` do worktree.

```
== ci.yml
ci.yml — 0.969 (Diamond)
  1 dims scored, 0 blockers, 0 warnings
  ✓ F4.7: 0.969 FF4_7: 4 CI/CD Pipeline smell(s) over 561 lines (rust) -- score=0.969          (touring-analysis analyze_cicd: see header for detector catalog); top: action pinned by mutable tag (use SHA for supply-chain safety -- e.g. `actions/checkout@<40-hex-sha>`) (3x)
== release.yml
release.yml — 0.860 (Gold)
  1 dims scored, 0 blockers, 0 warnings
  ✓ F4.7: 0.860 FF4_7: 5 CI/CD Pipeline smell(s) over 167 lines (rust) -- score=0.860          (touring-analysis analyze_cicd: see header for detector catalog); top: no cargo clippy in workflow (no lint gate -- warnings can ship) (1x)
== release-plz.yml
release-plz.yml — 0.550 (Unranked)
  1 dims scored, 0 blockers, 1 warnings
  ⚠ F4.7: 0.550 FF4_7: 5 CI/CD Pipeline smell(s) over 56 lines (rust) -- score=0.550          (touring-analysis analyze_cicd: see header for detector catalog); top: action pinned by mutable tag (use SHA for supply-chain safety -- e.g. `actions/checkout@<40-hex-sha>`) (2x)
== docs-rs-mirror.yml
docs-rs-mirror.yml — 0.089 (Unranked)
  1 dims scored, 1 blockers, 0 warnings
  ✗ F4.7: 0.089 FF4_7: 5 CI/CD Pipeline smell(s) over 27 lines (rust) -- score=0.089          (touring-analysis analyze_cicd: see header for detector catalog); top: no cargo clippy in workflow (no lint gate -- warnings can ship) (1x)
```

Depois (`touring-quality check --gate F4.7`):

```
== ci.yml
ci.yml — 0.889 (Gold)
smell(s) over 561 lines (rust) -- score=0.889 ; top: `actions/checkout` without `persist-credentials: false` in a job that can write (the token stays in .git/config for every later step) (8x); at job `check` line 30
== release.yml
release.yml — 0.700 (Silver)
smell(s) over 167 lines (rust) -- score=0.700 ; top: `id-token: write` in the workflow-level `permissions` (every job inherits an OIDC token -- grant it only to the job that mints it) (1x); at workflow level line 23
== release-plz.yml
release-plz.yml — 0.421 (Unranked)
smell(s) over 56 lines (rust) -- score=0.421 ; top: `actions/checkout` without `persist-credentials: false` in a job that can write (the token stays in .git/config for every later step) (2x); at job `release-pr` line 28
== docs-rs-mirror.yml
docs-rs-mirror.yml — 0.010 (Unranked)
smell(s) over 27 lines (rust) -- score=0.010 ; top: `actions/checkout` without `persist-credentials: false` in a job that can write (the token stays in .git/config for every later step) (1x); at job `docs-rs-build` line 18
```

Leitura: `release.yml` (`id-token: write` no topo + `Swatinem/rust-cache` no job `build`) cai de 0.860 Gold para 0.700 Silver (WARN) e o
primeiro achado é a regra (a); `ci.yml` deixa de ser Diamond (0.969 -> 0.889) por 7 refs mutáveis (antes lidas como 3), 8 checkouts sem
`persist-credentials: false` e 7 ações de terceiro por tag em job com permissão de escrita (sem `permissions:` o token é o padrão, de escrita).

### Por que a ação sem pin rendia ~0.03 (item f)

1. **Densidade**: o score é `1 - peso*contagem/linhas*6`. `ci.yml` tem 561 linhas e 3 refs contadas: 3*0.8/561*6 = **0.0257 ~ 0.03**. Um
   buraco de supply-chain não depende do tamanho do arquivo, mas o custo diluía com ele (561 linhas = 1/28 do custo que as mesmas 3 refs teriam
   em 20 linhas).
2. **Contagem errada**: o `ci.yml` tem 7 `uses:` mutáveis (`dtolnay/rust-toolchain`), o detector via 3. As regiões "não executáveis" vinham do
   lexer de **Rust**: o glob de shell `client/omarchy/hooks/*/*` (linha 239 do `ci.yml`) abre um comentário de bloco `/*` e engole o resto do
   arquivo (linhas 460, 480, 509, 551). Isso também escondia qualquer gate depois dele. Agora as regiões são só comentários YAML.
3. **Calibração**: achado estrutural impõe **teto** de score, sem diluição: crítico 0.40 (FAIL), alto 0.70 (WARN), médio 0.90 (nunca Diamond). Ação de
   terceiro (dono fora de `actions/` e `github/`) por tag mutável em job que pode escrever = médio (só teto, peso 0 para não contar duas vezes).
4. Bug adjacente corrigido: `uses: ./local` (sem `@` na linha) pegava o `@` da linha seguinte e contava uma segunda ref mutável.

## Itens

### 1. F4.7 -- detectores de CI (`crates/touring-analysis/src/quality/cicd.rs`, novo `cicd/structure.rs`, `quality/mod.rs`, `f4_7_cicd.rs`)
- Causa raiz: o detector era só substring + densidade; não conhecia job, `permissions` herdado, gatilho nem `with:`, e mascarava YAML com o lexer de Rust.
- Parser por indentação (sem dependência nova; o leitor de diretório concatena arquivos com `\n`, o que nenhum parser YAML de documento único aceita, e
  fragmentos têm de continuar pontuando). `permissions:` de job SUBSTITUI o do workflow (não mescla).
- (a) `id-token: write` (ou `write-all`, ou forma de fluxo) no topo: alto. (b) job com `id-token: write` (próprio ou herdado) que usa `actions/cache`(+`/restore`,`/save`),
  `Swatinem/rust-cache` ou `sccache-action`: alto. (c) `pull_request_target` + checkout de `github.event.pull_request.head`/`github.head_ref`/`refs/pull/`: crítico.
  (d) `actions/checkout` sem `persist-credentials: false` em job que escreve: médio. (e) `no-cache` não é cobrado de job com OIDC e reconhece `Swatinem/rust-cache`.
  (f) tetos por severidade (acima).
- Evidência agora aponta o local: `; at job `build` line 52` / `at workflow level line 23` (`CicdReport::primary`), consumidor dos tipos públicos novos
  (`CiRule`, `CiSeverity`, `CiFinding` re-exportados em `quality/mod.rs`).
- Limite declarado: YAML em bloco apenas; o formato de fluxo para job é ignorado (falha para "sem achado", nunca para achado falso). A fronteira entre
  documentos concatenados é `name:`/`on:`/`run-name:` no topo depois de `jobs:`.
- Testes novos: 24 em `cicd.rs` (positiva e negativa por regra, ceilings, diluição com 800 linhas de padding, ordem da evidência, glob de shell, `./local`,
  comentário, concatenação) + 8 em `structure.rs` + 1 no verificador. Mutantes 22/22 + evidência 3/3.

### 2. F2.4 -- rótulo e allowlist (`crates/touring-quality/src/lib.rs`, `verifications/f2_4_secrets.rs`)
- O detector mede **segredos commitados** (marcadores de provedor, JWT/SendGrid/OpenAI, credencial em connection string, atribuição com nome de segredo, literal de
  alta entropia) e nenhuma criptografia (algoritmo, tamanho de chave, RNG). O código estava errado, a regra (`~/.claude/rules/elite-50-quality.md`: "Secrets") e o
  CLAUDE.md estão certos. Rótulo no código: "Cryptographic Issues" -> **"Hardcoded Secrets"** (`META_TABLE`, doc do `DimId`, cabeçalho e as 4 evidências
  `Hardcoded Secrets: ...`).
- Allowlist: literais exatos que a AWS publica (par do guia IAM `AKIA...EXAMPLE` + `wJalr...EXAMPLEKEY`, e o par das páginas antigas de API). Substituídos por `x` do mesmo
  tamanho em `scan` e `first_offending` (a numeração de linhas e o alinhamento raw/projeção sobrevivem). Só vale o literal **sozinho** (vizinho alfanumérico = outra string),
  então uma chave real ao lado continua bloqueando e com a linha certa. Os literais são montados de fragmentos (o hook de pre-write não tem allowlist do próprio fonte).
- Dois testes antigos (`marcador_com_corpo_de_token_segue_bloqueando_ate_em_comentario`, `test_aws_key_still_blocks`) usavam o par de documentação como "vazamento"; a
  intenção deles (corpo de token bloqueia) foi preservada com uma chave de formato real montada em runtime.
- Testes novos: 7 + 1 em `lib.rs`. Mutantes 8/8.

### 3. `touring scan` x F2.1 (`touring-server/src/cli/scan.rs`, `touring-quality/.../f2_1_owasp.rs`)
- Reprodução (binário instalado): sonda `.py` -> gate `0.070 ... CMDi (CWE-78) at line 3`; `touring scan` -> `0 CWE(s), p0_block=false`.
- Causa raiz: **dois registros de detectores**. `scan` chamava `touring_code::cwe_scan::detect_cwes` (4 padrões de hook para Rust: CWE-798/89/22/394, orçamento de
  5 ms) e o F2.1 chama o registro CWE/OWASP polígloto (`SecurityAnalyzer`: 10 detectores). Nada em comum, e `scan` não dizia o que não olhou.
- Conserto: novo `f2_1_owasp::owasp_scan(raw, target) -> OwaspScan` (UM registro, lido pelo gate e pelo `scan`, com as mesmas allowlists); `scan` roda os dois motores, mapeia o
  match pelos limiares do próprio gate (`tier_of`: P0 reprova, P1 avisa) e **declara**: JSON ganhou `lang`, `engines[{name,ran,covers,reason}]` e `not_scanned` (F2.2, F2.3,
  F2.4 além de CWE-798, F2.5, F2.6). Um arquivo em `tests/` devolve `owasp ran:false, reason:...harness`, nunca "0" mudo. Duplicata na mesma linha fica numa entrada só, no tier mais severo.
- Adjacente (mesmo comando): a forma documentada `touring scan vulnerabilities <file>` lia a palavra `vulnerabilities` como o arquivo ("read vulnerabilities failed"); agora é pulada
  quando um caminho a segue. O hook de pre-write/edit continua no `CweScanLayer` (5 ms) -- não toquei.
- E2E com o binário do worktree: `touring scan <probe>` -> `CWE-78 P0 line 3, p0_block true, lang python, engines cwe_scan+owasp ran`; `scan vulnerabilities <probe>` idem; o gate dá 0.070.
- Teste de corpus pequeno em que os dois concordam: `scan_and_the_gate_agree_on_a_small_corpus` (6 arquivos, compara com `F2_1_Owasp.check`). Mutantes 10/10.

### 4. `component list` (`touring-server/src/cli/component.rs`)
- Causa raiz: a descoberta lista todo binário do `bin/` do toolchain e todo `touring*` do dir do canal dev (`~/.local/bin`), onde o `update-touring` e trocas manuais deixam cópias.
- Conserto: `is_backup_artifact` (`*.old`, `*.bak`, `*.bak-*`, que cobre `*.real.bak`) nos dois laços; nada é apagado. E2E: `component list --project .` com o binário instalado lista 5 backups, com o do
  worktree lista 0 (os 5 arquivos seguem em `~/.local/bin`). Mutantes 5/5.

### 5. Caminho absoluto em `docs/kpi` (`touring-cli/src/cli/kpi.rs`)
- Escritor único: `persist_snapshot` (o flush do daemon chama o mesmo, via CLI). Vazavam 232 arquivos: o campo `project_root` (232), `reason` do `context_budget` (26), `journal` do `shadow` (23) e o NOME
  (`--home-<user>-projects-touring`).
- Conserto: `persist_snapshot_at(dir, home, ...)` passa **todo string e toda chave** do payload por `scrub_home` (HOME, `/home/<qualquer>` e a forma com hífen do diretório de projetos do Claude Code,
  `-home-<user>-x` -> `-~-x`; fronteira de nome para não cortar `/home/gabriel` em `/home/g`) e o slug usa o caminho relativo ao home: `2026-10-17--~-projects-touring.json`. O JSON impresso no
  terminal mantém o caminho real. Os 232 históricos NÃO foram reescritos. `docs/kpi/README.md` documenta o novo nome. Mutantes 8/8.

## Pendências (fora do que eu podia editar)
1. `~/.claude/skills/touring-elite/references/quality/D17_secrets.md` (e o espelho `client/skills/...`) ainda abre com "D17 -- Cryptographic Issues & Secrets (F2.4)". O live é a fonte; o espelho é gerado.
2. Os 232 arquivos históricos de `docs/kpi/` seguem com o caminho absoluto `/home/<user>/...` no conteúdo e no nome. Reescrever em lote é decisão sua; o histórico do git público continua com eles.
3. Os YAML vivos (você corrige): o F4.7 novo vai marcar `release.yml` (a, b, d, f), `release-plz.yml` e `docs-rs-mirror.yml` (checkout sem `persist-credentials: false`, ação por tag).
4. O `ci.yml` do repo principal tem 43 `uses:` com 11 sem SHA (você citou); o do snapshot tem 26 com 7. A conta acima é do snapshot.
