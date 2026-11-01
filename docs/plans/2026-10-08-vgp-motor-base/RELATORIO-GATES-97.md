# RELATORIO gates-97 (30.4.97) — G10ROTA · G7G3G9 · HOOKCWD

Autor: engenheiro `gates-97` (equipe da coordenadora touring-09). Data: 10/10/2026.
Brief: `docs/plans/2026-10-08-vgp-motor-base/briefs/2026-10-10-gates-97.md`.
Estado: **entregue como hipótese até o juiz da coordenadora** (nada commitado, nada implantado).

## 1. Identidade

| Campo | Valor |
|---|---|
| Worktree | `/home/gabrielgadea/projects/touring-wt-gates97` |
| Branch | `fix/2026-10-10-gates-97` (HEAD `c118aa52`; a árvore tem o WIP implantado da 30.4.96 no index, sem commit) |
| Alvo de build | `CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/wt-gates97` |
| Cargo | sempre sob `scripts/hw-slot --class rust --wait N -- …` (uma vaga por lote; ver seção 4) |
| Scratchpad (logs, scripts, manifestos) | `/tmp/claude-1000/-home-gabrielgadea-projects-touring/751b3a8c-4a8b-49b3-885e-3956e002b458/scratchpad/` |

## 2. Feito, com evidência

### 2.1 Os três casos reais (linhas literais dos transcritos)

**G10ROTA** — `~/.claude/projects/-home-gabrielgadea-projects-analise/05166acc-ff08-4644-8481-ab8c2316c4a4.jsonl`, 10/10/2026 01:39Z:

- L5786 (assistant, Bash): `TOURING_CODE_MODE=native .venv/bin/python3 - <<'EOF' … p.write_text(t.replace(velho, "> Estado 09/10 22:38 (o relógio da máquina; eu tinha escrito «10/10 ~00:30» — errado):"), encoding="utf-8") …`
- L5789 (resultado): `PreToolUse:Bash hook error: [G10 exec-burst] 5ª chamada seriada do executor `python-inline` (0 `touring run` na janela de 600s) — a rajada desenrolada JÁ é o programa; rode-o de uma vez:  touring run --lang python --code 'import pathlib …p.write_text(…)…'` (a rota nomeada é `touring run --lang python`, que o X6 nega sob Sandboxed por `file-write`).
- L5798 (a rota que de fato funcionou): `TOURING_TRUSTED_OK=1 .touring/bin/touring run --allow-forbidden --lang python --code 'import pathlib … p.write_text(…)'`.

**G7G3G9** — mesma sessão, L5441-5448, e `~/.claude/projects/-home-gabrielgadea-projects-touring/2e4db816-c57e-43e8-9a44-aca77d1874da.jsonl`, L14641-14650:

- 05166acc L5441: `Read` de `…/RETOMADA.md` (limit 8); L5442: `PreToolUse:Read hook error: [G7 re-inspeção] 3ª leitura de …/RETOMADA.md na janela — reler é o sinal mais direto de programa-faltando …`; L5448: o agente cai para `TOURING_CODE_MODE=native .venv/bin/python3 - <<'EOF'` (edição por python).
- 2e4db816 L14641: `Edit` em `…/prova_equivalencia_js.py`; L14642: `<tool_use_error>File has been modified since read, either by the user or by a linter. Read it again before attempting to write it.</tool_use_error>`; L14644: `Read` (offset 53, limit 20); L14647: `PreToolUse:Read hook error: [G7 re-inspeção] 3ª leitura de …/prova_equivalencia_js.py na janela …`; L14649 (assistant): «O gate G7 pede um programa em vez da3ª leitura. Faço a edição por script …». É o círculo: Edit obsoleto -> a releitura que o harness exige é a 3ª -> G7 nega -> só sobra python.

**HOOKCWD** — `docs/plans/2026-10-08-vgp-motor-base/RETOMAR-AQUI-2026-10-09.md` L410-412 e L663 (smoke do analise, 30.4.96): «o subprocesso herdava TOURING_PROJECT_ROOT=touring → o daemon do analise recusava e o hook devolvia `{}` (erro só no stderr)» e «sem CLAUDE_PROJECT_DIR, o hook resolve a raiz pelo TOURING_PROJECT_ROOT global ou sobe até ~/.touring, e o daemon pinado recusa em 0 ms, recusa que o CC engole calado». Reproduzi A/B/C à mão com daemon REAL pinado (`scratchpad/manual_pin2.sh`): no binário sem conserto as três chamadas voltam `{}` com a recusa só no stderr e o trace grava `daemon-json`.

### 2.2 Frente 1 — G10ROTA

Causa: o deny do G10 nomeava `touring run --lang python …` mesmo quando o corpo escreve; o X6 nega isso sob Sandboxed, e o predicado do deny era paralelo ao do executor (viola D8).

Conserto (um predicado, duas pontas):
- NOVO `crates/touring-ceg/src/gateway/run_admission.rs`: `admission_outcome`, `only_subprocess_denials`, `blocking_reason`, `admission`, `run_route` -> `RunRoute::{Sandboxed, Trusted{sandbox_denial}, Unrunnable{reason}}`. É o MESMO código que `gate_run` do `touring run` usa (`crates/touring-server/src/cli/run.rs`: as cópias locais de `blocking_reason`/`only_subprocess_denials` foram removidas e importadas daqui).
- `crates/touring-ceg/src/gateway/sandbox_executor.rs`: `sandbox_can_write(cwd, path)` derivado de `sandbox_write_roots_at`, as MESMAS raízes que entram no ruleset Landlock (o tmp privado do run é excluído de propósito: some com o run).
- NOVO `crates/touring-cli/src/cli_suggester/g10_route.rs`: `Route::{Sandboxed, Trusted, NoWritableDest{dest}, Unrunnable}`; `route_for` pergunta ao executor; destino python literal fora do que o Landlock concede -> `NoWritableDest`.
- `crates/touring-cli/src/cli_suggester.rs`: o deny do G10 exec-burst, o remédio do python-inline e o deny do par write->run (mesmo defeito no segundo deny do G10; ver 6) usam a rota. Rota Trusted: `TOURING_TRUSTED_OK=1 touring run --allow-forbidden --lang python --code '<corpo verbatim>'`, com a frase que explica o porquê ANTES do comando e que a elevação é POR COMANDO e vale só dentro do workspace. Destino fora: «nenhuma rota de `touring run` escreve ali — a Trusted também passa pelo Landlock»; sai o bypass nativo `TOURING_GATE_OK=1` e nenhum comando de `touring run` para colar.

Vermelho (árvore sem conserto; `logs/p1/red_cli.log`, `red_server.log`): `cargo test -p touring-cli --lib gates97_tests` -> 3 passed (os controles) / **7 failed**, entre eles `g10_corpo_que_escreve_nomeia_a_rota_trusted`, `g10_destino_fora_do_workspace_nao_promete_rota`, `g10_par_write_run_de_script_que_escreve_nomeia_a_rota_trusted`, `g10_par_write_run_com_destino_fora_nao_promete_rota`; `touring-server --test g10_trusted_route_e2e` -> **2 failed** (`o_comando_que_o_deny_do_g10_nomeia_executa_e_escreve`, `destino_fora_do_workspace_nenhuma_rota_escreve_de_fato`).
Verde: ver tabela 3. O comando que o deny nomeia **executa de fato** em teste (`crates/touring-server/tests/g10_trusted_route_e2e.rs`: `cli_suggester::run` real + binário `touring` real; a rota antiga falha e não escreve, a nomeada faz a substituição do caso real; o destino fora não tem rota e a melhor candidata não escreve fora). O corpo de leitura segue com a rota Sandboxed idêntica à de sempre (controle).

### 2.3 Frente 2 — G7G3G9

Causa: G7 conta a 3ª leitura de um arquivo na janela como re-inspeção, inclusive a releitura que o harness EXIGE depois de uma modificação externa; o G9 empurra a edição para o `Edit`, que fica obsoleto. Cada gate isolado estava certo; o defeito é a composição.

Conserto mínimo em `cli_suggester.rs` (só o G7; G3 e G9 não mudaram de código):
- `FileStamp{mtime_ns,len}`, `file_stamp`, `g7_stamps()` (moka, mesma chave e TTL do `g7_seen`) e `g7_refresh_if_changed`: se o carimbo medido do arquivo MUDOU desde a leitura que o ledger lembra, a contagem do G7 zera. Arquivo inalterado ou não mensurável segue contando (a 3ª leitura inalterada continua negada).
- `g7_edit_route_note`: o deny do G7 passa a nomear a rota — « Para EDITAR {fp}: Edit direto — o arquivo não mudou desde a leitura registrada, então o G3 e o harness a aceitam; se ele mudar, a releitura é liberada.» — só quando há carimbo (sem medição, a afirmação «não mudou» não tem lastro e a frase não sai).

Invariante provado (`crates/touring-cli/src/cli_suggester/gates97_tests.rs::composicao_g7_g3_g9_sempre_deixa_uma_rota_de_edicao`): percorre TODAS as sequências de até 5 operações de {Read, Edit, Modify (escrita externa), Run (`touring run` citando o arquivo), SedCego (`sed -i` -> G9)} com o modelo do harness (o Edit só vale com um Read bem-sucedido depois da última modificação) e exige, em cada estado, uma rota de edição que algum gate nomeia e nenhum nega; o `sed -i` nega NOMEANDO `Edit tool em `<arquivo real>``; e o G3 fica mudo diante de arquivo já lido.
Vermelho: `red_cli.log` (acima) inclui `composicao_g7_g3_g9_sempre_deixa_uma_rota_de_edicao`, `g7_releitura_depois_de_modificacao_externa_nao_e_negada`, `g7_deny_nomeia_a_rota_de_edicao_e_a_promessa_cumpre`. Controle positivo: `g7_terceira_leitura_do_arquivo_inalterado_segue_negada` passa nas duas árvores.

### 2.4 Frente 3 — HOOKCWD

(a) Raiz — `crates/touring-hook-runtime/src/hook_runtime_ext.rs::pick_hook_project_root` (pura, testável): `CLAUDE_PROJECT_DIR` (devolvido como veio) > **`cwd` do payload** (absoluto e diretório existente; normalizado pela MESMA regra do daemon, `TouringConfig::normalize_project_root_inner`) > `TOURING_PROJECT_ROOT` > raiz do workspace Cargo > marcador `.claude/touring` mais externo > `HOME/.claude`. Os dois sítios do `crates/touring-hooks/src/main.rs` usam `HookRuntime::detect_project_root_for(payload_cwd(&input))`. Justificativa da posição no item 6.

(b) Instrumento — `main.rs::daemon_exit_reason(output, success)`: `success:false` -> **`daemon-error`**; vazio -> `daemon-empty`; JSON -> `daemon-json` (controle positivo); não-JSON -> `daemon-nonjson`. `forward_daemon_output(output, success)` grava esse motivo no trace e, quando recusado, escreve `[touring-hook] daemon refused the request: …` no stderr (stdout continua `{}`).

Prova: `crates/touring-hooks/tests/hook_root_payload_cwd_e2e.rs` com daemon REAL pinado (`<analise>/.touring/d.sock`) e hook real com `env_clear`: C0 (CLAUDE_PROJECT_DIR vence), A (cwd do payload vence TOURING_PROJECT_ROOT), B (vence a subida estrutural; HOME falso), controles (sem cwd; cwd inexistente), C (recusa legítima -> `daemon-error`, e o sucesso segue `daemon-json`). Mais 6 testes de `hook_root_tests` (hook-runtime) e 4 de `hookcwd_tests` (bin do hook).
Vermelho: o `red_hooks.log` foi gravado com um detector do e2e que depois corrigi (a recusa vai ao stderr, não ao stdout; passou a olhar os dois), então o vermelho **válido** do e2e é o dos mutantes que revertem o conserto sobre a árvore final: CWD-05 (hook ignora o cwd), CWD-06b (recusa volta a `daemon-json`) e CWD-07 (trace ignora `success`), todos mortos pelo e2e. O vermelho manual (A/B/C reproduzidos à mão) está em 2.1.

### 2.5 Mutantes (na CÓPIA = o próprio worktree; restauração por conteúdo + `os.utime(agora)`; `scratchpad/mutar.py`)

Cada mutante roda sob UMA vaga do slot, é aplicado só depois de a vaga ser obtida, e o harness confere o sha restaurado (todos `restaurado_por_conteudo=true`; ao fim, `sha256sum -c` das 11 fontes idêntico ao registrado antes do lote, salvo o teste que eu mesmo estendi).

| Mutante | Arquivo | O que muda | Testes que morrem |
|---|---|---|---|
| G10-01 | g10_route.rs | prefixo `TOURING_TRUSTED_OK=1 ` vazio | `g10_corpo_que_escreve_nomeia_a_rota_trusted`, `g10_par_write_run_de_script_que_escreve_nomeia_a_rota_trusted`, e2e `o_comando_que_o_deny_do_g10_nomeia_executa_e_escreve` |
| G10-02 | g10_route.rs | flag `--allow-forbidden ` vazia | os mesmos 3 |
| G10-03 | cli_suggester.rs | volta a rota antiga (inline sempre Sandboxed) | `g10_corpo_que_escreve_nomeia_a_rota_trusted`, `g10_destino_fora_do_workspace_nao_promete_rota`, e2e `o_comando_que_o_deny_…` |
| G10-04 | g10_route.rs | `route_for` sem `NoWritableDest` | `g10_destino_fora_…`, `g10_par_write_run_com_destino_fora_…`, e2e `destino_fora_do_workspace_nenhuma_rota_escreve_de_fato` |
| G10-05 | cli_suggester.rs | script do par write->run sempre Sandboxed | `g10_par_write_run_de_script_que_escreve_…`, `g10_par_write_run_com_destino_fora_…` |
| G10-06 | cli_suggester.rs | lead sem a explicação | `g10_corpo_que_escreve_nomeia_a_rota_trusted` |
| CEG-01 | sandbox_executor.rs | `sandbox_can_write` sempre true | `sandbox_can_write_follows_the_landlock_write_roots`, `g10_destino_fora_…` |
| CEG-02 | sandbox_executor.rs | destino sem resolver symlink | `sandbox_can_write_resolves_symlinks_out_of_the_project` |
| CEG-03 | sandbox_executor.rs | `..` ignorado na normalização | `sandbox_can_write_follows_the_landlock_write_roots` |
| CEG-04 | run_admission.rs | sem waiver de `subprocess` | `deny_so_de_subprocess_e_admitido_como_no_executor` |
| CEG-05 | run_admission.rs | Trusted nunca admite | `um_corpo_que_escreve_so_roda_no_perfil_trusted`, `g10_corpo_que_escreve_…`, `a_rota_que_o_gate_nomeia_e_a_que_o_executor_aceita` |
| CEG-06 | run_admission.rs | Trusted julgado pelo perfil Sandboxed | `um_corpo_que_escreve_so_roda_no_perfil_trusted`, `a_rota_que_o_gate_nomeia_…` |
| G7-01 | cli_suggester.rs | sem refresh por carimbo | `g7_releitura_depois_de_modificacao_externa_nao_e_negada`, `composicao_g7_g3_g9_…` |
| G7-02b | cli_suggester.rs | refresh sempre | `g7_terceira_leitura_do_arquivo_inalterado_segue_negada`, `g7_releitura_…`, `g7_deny_nomeia_…` |
| G7-03 | cli_suggester.rs | deny sem a rota de edição | `g7_deny_nomeia_a_rota_de_edicao_e_a_promessa_cumpre` |
| G7-04 | cli_suggester.rs | promete a rota sem medição | `g7_deny_sem_medicao_nao_promete_rota_de_edicao` |
| G3-01 | cli_suggester.rs | G3 fala de arquivo já lido (`else if false`) | `composicao_g7_g3_g9_…` |
| G9-01 | cli_suggester.rs | deny do G9 sem a rota `Edit tool` | `composicao_g7_g3_g9_…` |
| G9-02 | cli_suggester.rs | alvo do G9 vira `<o arquivo>` | `composicao_g7_g3_g9_…` |
| CWD-01 | hook_runtime_ext.rs | cwd do payload depois de TOURING_PROJECT_ROOT | `caso_a_…`, e2e `a_raiz_do_hook_segue_o_cwd_do_payload_…` |
| CWD-02 | hook_runtime_ext.rs | cwd sem validar `is_dir` | `cwd_de_payload_invalido_e_ignorado` |
| CWD-03 | hook_runtime_ext.rs | cwd sem normalizar | `cwd_de_payload_sem_projeto_resolve_como_o_daemon` |
| CWD-04 | hook_runtime_ext.rs | CLAUDE_PROJECT_DIR perde a primazia | `claude_project_dir_continua_primeiro` |
| CWD-05 | main.rs | hook ignora o cwd do payload | e2e `a_raiz_do_hook_segue_o_cwd_do_payload_…` |
| CWD-06b | main.rs | recusa volta a `daemon-json` | `a_recusa_do_daemon_pinado_…`, e2e, `os_outros_rotulos_seguem_como_antes` |
| CWD-07 | main.rs | trace ignora `success` | e2e `a_raiz_do_hook_segue_o_cwd_do_payload_…` |

Rodada 1 (25 mutantes, `mut/out/*.json`) e rodada 2 (5, `mut/out2/*.json`). **Sobreviventes honestos da rodada 1, e o que fiz:**
- **G3-01 e G9-01 sobreviveram** (o invariante original só exigia «não é deny», e o advisory do G3 não é deny; e não cobria o texto do G9). Reforcei o teste: o modelo ganhou `leu` (o G3 não pode falar de arquivo já lido) e a operação `SedCego` (o G9 nega nomeando `Edit tool em `<arquivo real>``); G9-02 (alvo `<o arquivo>`) é novo. Rodada 2: os três morrem. G3-01 também listava `g7_deny_nomeia_…` como killer esperado e esse teste não é sensível ao advisory do G3 — morre só pelo invariante (1 killer), registrado assim.
- **CWD-06 e G7-02** foram marcados `SOBREVIVEU/INVALIDO` pelo meu harness por um erro de DECLARAÇÃO dos controles (os testes que eu tinha chamado de controle também asseveram o ramo mutado e por isso reprovam); os killers pretendidos morreram. Reexecutados na rodada 2 com os controles corrigidos (CWD-06b, G7-02b): mortos.

## 3. Estado por item do aceite

| # | Item | Estado | Evidência (comando -> rc) |
|---|---|---|---|
| 1 | Cada frente: vermelho com o caso real (fonte citada) -> verde; mutantes mortos e listados na cópia | CUMPRIDO | vermelho: 2.2/2.3/2.4; verde: 7 linhas abaixo; mutantes: 2.5 (30 execuções, 0 sobrevivente ao fim) |
| 2a | `cargo fmt --all -- --check` | VERDE | fase 6, 12:56 UTC: rc=0 |
| 2b | `cargo clippy --workspace --all-targets -- -D warnings` | VERDE | fase 6: rc=0 (só o aviso futuro `proc-macro-error2`, nota do cargo, não do clippy). `--all-features` em `touring-server` não compila por construção (`compile_error!` de `jemalloc` x `prod-allocator` x `dhat-heap`, `crates/touring-server/src/main.rs:37-42`; `logs/p2/clippy.log`), por isso usei o workspace com features padrão |
| 2c | `cargo test` dos pacotes tocados, 0 falhas | VERDE | ceg 608 passed (fase 6, rc=0); cli 681 passed/2 ignored (fase 5, rc=0); hook-runtime 493 passed/2 ignored (fase 2, rc=0); hooks (todas as suítes; fase 2, rc=0); server: fase 2 reprovou **só** `binary_e2e` (14 testes) por ambiente — o teste procura `<worktree>/target/debug/touring` e o worktree compila em outro `CARGO_TARGET_DIR`; criei 3 symlinks (`target/debug/{touring,touring-daemon,touring-hook}` -> `…/target/wt-gates97/debug/…`, ignorados pelo git) e reexecutei: `binary_e2e` 14 passed, rc=0 (fase 3). hook-handlers (não tocado) com `--features pre-hooks,post-hooks`: 733 passed, rc=0 |
| 2d | `touring-hooks --test cli_suggester_e2e` | VERDE | fase 5: 20 passed, rc=0 |
| 3 | Denies novos com valor real derivado; comando nomeado roda | CUMPRIDO | `gates97_tests` afirmam o caminho e o comando reais (nunca `<placeholder>` derivável; o G9 nega com o arquivo real); e2e executa o comando nomeado (2.2). Único placeholder que resta é o honesto: o corpo do heredoc acima de 1500 bytes vira `'<o corpo do seu heredoc/-c, verbatim>'` (comportamento anterior, não tocado) |
| 4 | Este relatório (8 seções) | CUMPRIDO | este arquivo |

Verde das frentes, rc de cada comando (fase 1 e seguintes, `logs/p1`, `p3`, `p4`, `p6`): `cargo test -p touring-ceg --lib -- run_admission sandbox_can_write` 8 passed rc=0 · `cargo test -p touring-cli --lib -- gates97_tests g10_route --test-threads=1` 17 passed rc=0 (fase 4, depois do invariante estendido) · `cargo test -p touring-hook-runtime --lib -- hook_root_tests` 6 passed rc=0 (fase 2) · `cargo test -p touring-hooks --bin touring-hook -- hookcwd_tests` 4 passed rc=0 · `cargo test -p touring-hooks --test hook_root_payload_cwd_e2e` 1 passed rc=0 (fase 6) · `cargo test -p touring-server --test g10_trusted_route_e2e -- --test-threads=1` 2 passed rc=0 (fase 6) · `cargo test -p touring-server --lib -- cli::run` 58 passed rc=0 (fase 6).

## 4. Processos

- Toda compilação/teste passou por `hw-slot --class rust` (fases 1-6 e o lote de mutantes, cada uma numa única vaga). A fila foi o gargalo: o slot ficou com sessões vizinhas por horas (`vhtml`, `vts`, `int97`) e com o gate térmico (88 C) várias vezes; esperei sem contornar. Um primeiro disparo do lote de mutantes aplicava o mutante ANTES de obter a vaga: interrompi-o com `kill -INT` (só o PID `4075763` que eu criei; o `finally` restaurou, conferido por `sha256sum -c`) e passei a rodar o lote inteiro dentro de uma vaga só (o `hw-slot` é re-entrante).
- PIDs que matei: `506130` e `1701410` (a cadeia de vermelhos antiga, minha) e o `4075763` acima. Nenhum processo touring de terceiros; nenhum `pkill`; nenhum `daemon-ctl` foi necessário. Daemons de teste (pinados, `TOURING_IDLE_TIMEOUT_SECS`) nasceram e morreram dentro dos e2e; no experimento manual (`manual_pin2.sh`) o daemon foi encerrado pelo PID que o próprio script gravou.
- Bypass por-comando: usei `TOURING_GATE_OK=1` em poucos comandos de shell (lançar `nohup … hw-slot`, criar os 3 symlinks), sempre precedido de um `touring run` para devolver o crédito do G11; nenhum kill switch.

## 5. Dados, com sha (início de 64 hex; final, depois da última edição)

| Arquivo | Linhas | sha256 |
|---|---|---|
| `crates/touring-ceg/src/gateway/run_admission.rs` (novo) | 262 | `c3636198c5fd1556` |
| `crates/touring-ceg/src/gateway/mod.rs` (+3) | — | `8392b30bc0fe8b06` |
| `crates/touring-ceg/src/gateway/sandbox_executor.rs` (+131) | — | `3ce068a1b68f8bb8` |
| `crates/touring-cli/src/cli_suggester.rs` (+223 não staged) | — | `56016824616ea5f4` |
| `crates/touring-cli/src/cli_suggester/g10_route.rs` (novo) | 270 | `57a48c048a0ba0ca` |
| `crates/touring-cli/src/cli_suggester/gates97_tests.rs` (novo) | 578 | `e63250f4fc3902a7` |
| `crates/touring-hook-runtime/src/hook_runtime_ext.rs` (+286/-) | — | `e470bc88e65d4be1` |
| `crates/touring-hooks/src/main.rs` (+104) | — | `64c825396a5bf54d` |
| `crates/touring-hooks/tests/hook_root_payload_cwd_e2e.rs` (novo) | 325 | `356149b7fb7f347e` |
| `crates/touring-server/src/cli/run.rs` (+138/-) | — | `05fa9c2e8b8b85aa` |
| `crates/touring-server/tests/g10_trusted_route_e2e.rs` (novo) | 194 | `8d7b1e961a985e07` |

Antes -> depois (início do conserto, `logs/p1/sha_antes.txt` -> `sha_depois.txt`): cli_suggester `2fc4c6d9` -> `56016824`; sandbox_executor `9e8d9ac8` -> `3ce068a1` (depois de rustfmt); ceg `mod.rs` `f1d47d98` -> `8392b30b`; run.rs `338442db` -> `05fa9c2e`; main.rs `c77196e5` -> `64c825396`. `git diff --stat` (não staged): 6 arquivos, 754 inserções, 131 remoções, mais 5 arquivos novos não rastreados. Logs: `scratchpad/logs/p1..p6`, `mut/out*/`, manifestos `mut/manifest*.json`.

## 6. Decisões e riscos

1. **Posição do `cwd` do payload: abaixo de `CLAUDE_PROJECT_DIR`, acima de `TOURING_PROJECT_ROOT`.** `CLAUDE_PROJECT_DIR` é a raiz que o próprio Claude Code fixa para a sessão (continua primeiro; teste `claude_project_dir_continua_primeiro`). O `cwd` do payload é a pergunta da sessão que está falando com o hook. `TOURING_PROJECT_ROOT` é um pin global do settings (fixa `~/projects/touring` para TODA sessão), logo é a fonte errada para qualquer outro projeto, que é exatamente o caso A/B do smoke. Risco: sem `CLAUDE_PROJECT_DIR`, um `cd` da sessão para outro projeto muda a raiz (é o comportamento desejado: o cwd sobe até a raiz pela regra do daemon). Um cwd sem projeto resolve «como o daemon» (teste `cwd_de_payload_sem_projeto_resolve_como_o_daemon`), nunca para um diretório arbitrário.
2. **Extensão deliberada ao par write->run.** O segundo deny do G10 tinha o mesmo defeito (nomear `touring run --file` que o X6 nega para script que escreve); usei o mesmo predicado (`g10_script_route` lê o CONTEÚDO do script, teto 256 KiB). Fora da letra do brief, dentro do mesmo defeito.
3. **`NoWritableDest` só vê python e só vê literais** (heurística léxica: `Path.home() / …`, f-string e `os.path.join` não são resolvidos). Um destino que ela não enxerga cai na rota Trusted, onde o executor ainda recusa com mensagem que ensina. Marcas de escrita são subconjunto de `PY_INLINE_WRITER_MARKS` (teste impede a deriva).
4. **`cli_suggester.rs`: blast 23 (> 10) e grade C.** Mitigação: o conserto é aditivo (funções novas + 3 ramos), a suíte do arquivo (296 testes `cli_suggester` + 681 do crate) e o e2e dos hooks ficaram verdes, e cada ramo tem mutante morto. Não refatorei o arquivo.
5. **[CONSERTADO no Adendo ao fim deste arquivo] Achado (G5 antes do G9):** na primeira ação Bash depois de uma rajada de >= 3 edits, o advisory do G5 («rajada de 3 edits terminou sem build/teste») responde primeiro e o `sed -i` cego **executa** (o G9 só nega na chamada seguinte). Visto na primeira execução da fase 4 (o `green_cli.log` foi sobrescrito pela reexecução verde; transcrevo): o teste `composicao_g7_g3_g9_…` panicou em `gates97_tests.rs:292` com «`sed -i` cego é negado pelo G9: Some("{\"hookSpecificOutput\":{\"additionalContext\":\"[G5 telemetria] rajada de 3 edits terminou sem build/teste — P9 medido em 17%. Valide agora (cargo check/test no crate tocado); a regressão silenciosa nasce aqui.\" …»: a resposta ao `sed -i` foi o advisory, não o deny. Não é o círculo do brief (a rota de edição existe), por isso não mexi na ordem; o teste modela o G5 e exige o deny do G9 na chamada seguinte. Fica para a coordenadora decidir.
6. **Stderr novo `[touring-hook] daemon refused the request: …`**: o stdout segue `{}` e o exit 0; o que torna a recusa MEDÍVEL é o `exit_reason: daemon-error` no trace. Se o Claude Code mostra esse stderr para exit 0 não foi verificado.
7. **Contadores:** o probe do executor incrementa `ceg_captured` quando o hook consulta a rota; sem impacto funcional.
8. **Ambiente:** `/tmp/.claude/touring` se perdeu nesta máquina e a subida estrutural termina em `/tmp`; por isso dois testes de `hook_root_tests` afirmam a propriedade (`!starts_with(analise)`) em vez de igualdade com um diretório fixo (tornei-os herméticos após uma falha).
9. **REGRA #0:** `tool_for_lang` era `pub` só com consumidor interno; reduzi para privado (clippy do workspace + suítes do ceg/server reexecutados depois, fase 6, rc=0). Consumidores dos demais símbolos novos por varredura do sistema de arquivos: `run_admission::*` em `cli/run.rs` e `g10_route.rs`; `sandbox_can_write` em `g10_route.rs`; `detect_project_root_for` em `hooks/src/main.rs`.

## 7. Git

Sem commit, sem push, sem stash/reset/checkout. Branch `fix/2026-10-10-gates-97` sobre `c118aa52`; o index já carrega o WIP implantado da 30.4.96 (arquivos `M ` staged, `crates/touring-code/src/vgp/`, `crates/touring-pyright/` etc. não são meus). Meus 6 arquivos modificados (não staged) e 5 novos (não rastreados) estão listados na seção 5. Nada fora do worktree foi editado além deste relatório; criei os 3 symlinks em `<worktree>/target/debug/` (ignorados pelo git).

## 8. O que NÃO foi verificado

- Implantação: nada foi construído em release nem propagado (`update-touring`/`propagate-release`/`touring update` proibidos); o comportamento ao vivo no Claude Code (deny novo, stderr do hook) só será visto depois do deploy.
- Wiring/órfãos pelo índice do touring: o worktree não está indexado; usei varredura de arquivos para os consumidores (seção 6, item 9), não `touring wiring orphans`.
- `cargo clippy --all-features` do `touring-server` (impossível por `compile_error!` de features exclusivas) e o grafo release-TEST do server (débito conhecido do CLAUDE.md, item 4).
- A suíte completa do `touring-server` depois dos symlinks: reexecutei só `binary_e2e` (rc=0), `cli::run` e o e2e do G10; as demais suítes do server passaram na fase 2 e nenhum fonte do server mudou depois.
- Windows/macOS; sessões reais do Claude Code com `CLAUDE_PROJECT_DIR` ausente e `cwd` em subdiretório de um projeto pinado (coberto só pelos testes pinados com daemon real, não por uma sessão viva).
- A taxa em que o G7 libera releituras legítimas vs. releituras de um arquivo que muda a cada passo (loop de linter): medi a correção, não a frequência em produção.
- A bateria `touring pre-edit` (score >= 0.8) não foi registrada arquivo a arquivo neste relatório; usei `ast meta` (blast 23, grade C no `cli_suggester.rs`) e os avisos do hook de edição.

---

# Adendo (10/10/2026, tarde) — ordem G5/G9: o deny aplicável vence o advisory

Pedido da coordenadora: na 1ª Bash depois de >= 3 edits o G5 (advisory) respondia ANTES do G9 (deny de `sed -i`/`awk -i inplace`/`perl -pi`); o deny nunca era avaliado e o `sed -i` cego executava uma vez (o achado do item 5 da seção 6). Invariante: **todo deny aplicável vence todo advisory, independente da ordem de registro**; o advisory vai junto no texto do deny. Este adendo substitui o item 5 da seção 6 (agora CONSERTADO).

## A. Causa e mapa da cadeia

`code_mode_gates_with` (`crates/touring-cli/src/cli_suggester.rs`) era um corpo único de ~620 linhas com `return Some(…)` espalhado. No ramo Bash, antes do conserto, três pontos devolviam advisory e encerravam a cadeia: o **G5** (telemetria, antes de G11/G2/G9/G8/G10/S3/G6/G1), o **S3** (aviso de rajada da 2ª inspeção, antes de G6 e G1) e o **G6** (1ª repetição, antes do G1). A falha reproduzida: 3 `Edit` + `sed -i 's/a/b/' <arquivo>` -> resposta `{"additionalContext":"[G5 telemetria] rajada de 3 edits terminou sem build/teste …"}` (sem `permissionDecision`), ou seja, o comando executa.

## B. Conserto (um arquivo de produção: `cli_suggester.rs`, ~60 linhas)

- `code_mode_gates_with` virou um wrapper fino: cria `avisos: Vec<String>`, chama `code_mode_gates_chain` (o corpo antigo, com `avisos: &mut Vec<String>`) e passa o resultado por `merge_gate_verdict`.
- G5 e S3 **depositam** o texto em `avisos` e a cadeia segue. `merge_gate_verdict`: sem veredito -> `advisory_response(avisos)` (idêntico ao de antes quando só um fala); veredito **deny** -> o deny segue sendo deny e o aviso entra na `permissionDecisionReason` (`"\n\n[também, sem bloquear] …"`); reescrita (`updatedInput`) ou advisory da própria cadeia -> o aviso entra no `additionalContext`.
- Precedências preservadas POR DESENHO (documentadas no código, cada uma com teste):
  1. **S3 x G1**: na 2ª inspeção a rajada só AVISA, por ordem do Gabriel (09/10/2026, D — medido: 1.272 negações do S3 em 24 h). Depois do aviso do S3 o G1 NÃO volta a decidir a chamada (`s3_avisou`). É a única exceção consciente à invariante: o G1 ali é a família que o S3 substituiu por ordem. Se a coordenadora quiser que o G1 vença também aí, é decisão dela (ORD-07 prova que hoje a exceção está travada).
  2. **G6 consome a repetição idêntica inteira** (advisory na 1ª, deny na 2ª; o G1 trata inspeção NOVA). O advisory do G6 continua encerrando a cadeia; ele já era a última palavra antes do G1.
- Efeito colateral aceito: uma chamada Bash que antes parava no G5 agora atravessa G11/G2/G9/G8/G10/G6/G1, então os ledgers desses gates passam a contá-la (antes ficava fora da contagem); a 2ª inspeção do S3 passa a contar no G6.

## C. Prova

Testes novos em `crates/touring-cli/src/cli_suggester/gates97_tests.rs`:

| Teste | Papel | Árvore sem conserto | Árvore com conserto |
|---|---|---|---|
| `g5_advisory_nao_engole_o_deny_do_g9` | caso: 3 edits + `sed -i` | **FALHA** (resposta = só o advisory do G5) | passa: deny `[G9 escrita-cega-inline]`, rota `Edit tool em `<arquivo real>``, com `[G5 telemetria]` no texto |
| `s3_advisory_nao_engole_o_deny_do_g6` | mesma forma, S3 x G6 (`cat a`, run, `cat a`, run, `cat b`, `cat a`) | **FALHA** (resposta = só `[CODE MODE · rajada]`) | passa: deny `[G6 redundant-exact-call]` com o aviso do S3 junto |
| `g5_bash_inofensiva_depois_de_3_edits_segue_com_o_advisory` | controle pedido | passa | passa (advisory, sem deny) |
| `g9_sed_i_sem_edits_antes_segue_negado_sem_texto_de_g5` | controle pedido | passa | passa (deny do G9, sem texto de G5) |
| `s3_segunda_inspecao_parecida_segue_so_com_o_advisory` | controle da ordem D | passa | passa (só advisory, sem `updatedInput`) |

Vermelho: `scratchpad/logs/p7/red_cli.log` (`cargo test -p touring-cli --lib -- gates97_tests --test-threads=1`: 14 passed / **2 failed**, os dois acima). Verde: `green_cli.log` (22 passed, rc=0) e `green_suggester.log` (302 passed, rc=0). O invariante estendido do item 2.3 (`composicao_g7_g3_g9_…`, que já modelava o `sed -i` depois do G5) continua verde.

**Mutantes** (na cópia, restauração por conteúdo + `os.utime`, tudo dentro de uma vaga do slot; `mut/out3/*.json`) — os dois pedidos morrem:

| Mutante | O que muda | Testes que morrem |
|---|---|---|
| ORD-01 | inverter a precedência: advisory vence o deny | `g5_advisory_nao_engole…`, `s3_advisory_nao_engole…` |
| ORD-02 | «o primeiro gate que responde vence» no G5 (volta a `return`) | `g5_advisory_nao_engole_o_deny_do_g9` |
| ORD-03 | idem no S3 | `s3_advisory_nao_engole_o_deny_do_g6` |
| ORD-04 | o deny perde o texto do aviso | `g5_advisory_nao_engole…`, `s3_advisory_nao_engole…` |
| ORD-05 | sem veredito, o aviso se perde | `g5_bash_inofensiva_depois_de_3_edits_segue_com_o_advisory` |
| ORD-06 | detecção de deny invertida no merge | `g5_advisory_nao_engole…`, `s3_advisory_nao_engole…` |
| ORD-07 | o G1 volta a decidir a 2ª inspeção (quebra a ordem D) | `s3_segunda_inspecao_parecida_segue_so_com_o_advisory` |

7 de 7 mortos, todos `restaurado_por_conteudo=true`; sha final do `cli_suggester.rs` idêntico ao instalado (`b4902e706f98f18a`).

**Gates (fase 7, `scratchpad/logs/p7/`, uma vaga)**: `cargo fmt --all -- --check` rc=0 · `cargo clippy --workspace --all-targets -- -D warnings` rc=0 · `cargo test -p touring-cli` 686 passed, rc=0 · `cargo test -p touring-hooks --no-fail-fast` rc=0 (inclui `cli_suggester_e2e`: 20 passed) · `cargo test -p touring-server --no-fail-fast` rc=0 (1654 + `binary_e2e` 14 etc.) · `cargo test -p touring-hook-handlers --features pre-hooks,post-hooks --no-fail-fast` 733 passed, rc=0 (não toquei esse pacote). Sem commit.

## D. A MESMA forma de curto-circuito em outros pontos (REPORTADO, NÃO consertado)

Todos pertencem ao motor VGP e só disparam com `[vgp] modo = "ativo"` no projeto (o default `sombra` devolve `None`), por isso são latentes hoje:

1. `crates/touring-hook-handlers/src/hooks/pre_bash.rs:69-71`: `vgp_sombra_de_bash` devolve `HookResponse::Context` («[VGP · aviso]») e `run_returning` retorna na hora, ANTES do `bash_ast_validator` Block (l.77), do `PreToolValidator` Deny (l.103-117) e do W-540 Deny (l.157). Só para comando com python inline (`python3 -c`, heredoc, `touring run --code|--file`): um aviso do VGP esconderia o bloqueio de um comando perigoso na mesma linha.
2. `crates/touring-hook-handlers/src/hooks/pre_edit_prevention.rs:151-160`: `vgp_sombra_de_edit` -> `Context` retorna antes do `check_deny_gate` (S210, delta de sintaxe, l.163-167). Só `.py`.
3. `crates/touring-hook-handlers/src/hooks/pre_write.rs:133-135`: `vgp_sombra_de_write` -> `Context` retorna antes de E14 e do resto do pipeline; por varredura estática (não exaustiva) não achei deny posterior, então é o de menor risco.
4. `cli_suggester.rs`, ramo `Read`: o advisory M3 (delegação, no 9º arquivo distinto) retorna antes do deny do G7. Improvável na prática (M3 só dispara em arquivo novo no ledger do G3, enquanto o G7 conta o mesmo arquivo em outro ledger), sem caso reproduzido.
5. `cli_suggester.rs`, ramo `Edit`: o advisory do G3 retorna antes do advisory contrafactual (W6) — advisory x advisory, nenhum deny escondido. Sem ação.

**Pergunta para a coordenadora**: autoriza o mesmo tratamento (aviso depositado + merge em `run_returning`; deny e bloqueio vencem, o `[VGP · aviso]` vai junto) em `pre_bash.rs`, `pre_edit_prevention.rs` e `pre_write.rs`? O item 1 é o único que está no caminho Bash; não o toquei porque o teste vermelho exige `modo = "ativo"` e um programa python que o motor classifique como `warn`, o que não consegui prever sem rodar o motor, e porque o brief proibia consertar fora do pre-bash sem perguntar.

## E. Dados do adendo (início do sha256)

| Arquivo | sha256 |
|---|---|
| `crates/touring-cli/src/cli_suggester.rs` (antes -> depois deste adendo) | `56016824616ea5f4` -> `b4902e706f98f18a` |
| `crates/touring-cli/src/cli_suggester/gates97_tests.rs` | `e63250f4fc3902a7` -> `cc0287ccc97be5e5` |
| `crates/touring-ceg/src/gateway/run_admission.rs` (da redução de visibilidade de `tool_for_lang`, já no corpo do relatório) | `c3636198c5fd1556` |

Cópias de trabalho: `scratchpad/g5/{orig,fixed}_cli_suggester.rs`; manifesto `mut/manifest3.json`; logs `logs/p7/`. Sem commit.

## F. O que NÃO foi verificado (adendo)

- Como o Claude Code mescla as saídas de vários hooks PreToolUse (qual vence entre `cli-suggest` e `pre-bash` em paralelo): assumi só o que o código do `cli_suggester` controla.
- O comportamento ao vivo depois do deploy; o texto `[também, sem bloquear] …` no deny não foi visto no Claude Code real.
- A frequência dos casos S3 x G6 em produção: construí a sequência mínima (com `touring run` entre as inspeções) e mostrei que é alcançável, não que seja comum.

---

# Adendo 2 (10/10/2026, noite) — a mesma regra nos três handlers do VGP (`pre-bash`, `pre-edit`, `pre-write`)

Autorizado pela coordenadora: o `[VGP · aviso]` deixa de encerrar a cadeia nos três handlers; deny e bloqueio vencem, e o aviso vai junto. Latente hoje (`[vgp] modo` default = `sombra` devolve `None`), defeito vivo na V4 (`modo = "ativo"`). O item 4 do pedido (Read: M3 x G7) fica **documentado como está**, sem caso reproduzido (adendo 1, seção D, item 4). Os itens 1 e 2 da seção D do adendo 1 e o 3 estão consertados aqui.

## G. Uma regra, uma casa

`touring-hook-handlers` depende de `touring-cli`, e os dois dependem de `touring-hook-runtime` (onde mora o `HookResponse`). A regra mora em **`crates/touring-hook-runtime/src/verdict.rs`** (novo, 317 linhas, `pub mod verdict;`), com dois adaptadores sobre os MESMOS dois helpers de texto (`deny_reason_with`, `context_with`):

| Adaptador | Quem chama | Forma |
|---|---|---|
| `HookResponse::with_advisories(self, avisos)` | `pre_bash`, `pre_edit_prevention`, `pre_write` | enum: `Allow` -> `Context(avisos)`; `Context`/`ContextWithUpdatedInput` -> avisos antes do contexto; `Deny`/`Block`/`Halt` -> avisos na razão |
| `merge_pretool_json(veredito, avisos)` | `cli_suggester` (gates G5/S3/...) | JSON de fio: sem veredito (`None`, `{}`) -> advisory; deny -> razão; reescrita/advisory -> `additionalContext` |

O `merge_gate_verdict` que eu tinha escrito dentro do `cli_suggester.rs` no adendo 1 foi **removido**: o `cli-suggest` chama `merge_pretool_json`. Três cópias da regra divergiriam (lição `comentario-afirma-simetria-inexistente`: «keep in sync» não é garantia). Em vez de um comentário, há um teste que pinta a MESMA tabela de precedência nos dois adaptadores: `verdict::tests::os_dois_adaptadores_pintam_a_mesma_tabela_de_precedencia` compara, para cada veredito (`Allow`, `Context`, `Deny`, `Deny` com contexto, reescrita), o fio de `v.with_advisories(a).to_json()` com o de `merge_pretool_json(Some(v.to_json()), a)` (o único campo descartado é o `hookEventName`, carimbado por estado global de processo). Mutante VGP-06 (o adaptador JSON diverge do enum) morre aqui.

## H. Conserto nos handlers (um padrão, três sítios)

Cada handler ganhou a costura `run_returning_with(runtime, input, vgp)` (o `run_returning` público chama com a função de produção `vgp_sombra_de_*`) e o corpo antigo virou `run_chain(..., avisos: &mut Vec<String>)`. No ponto do VGP, `HookResponse::Context` é DEPOSITADO em `avisos`; qualquer outra resposta do VGP (o deny) segue encerrando. No fim, `run_returning_with` aplica `.with_advisories(avisos)`.

| Handler | O que o aviso escondia | Observável |
|---|---|---|
| `pre_bash.rs:69` | `bash_ast_validator` Block, `PreToolValidator` Deny e W-540 Deny (um `rm -rf` na mesma linha do python inline) | Deny com o aviso na razão |
| `pre_edit_prevention.rs:151` | `check_deny_gate` S210 (edição que quebra um arquivo que parseia) | Deny com o aviso na razão |
| `pre_write.rs:133` | `triad_hook::arm` (o snapshot que o `post-write` compara), os sinais do pipeline e os rewards. Não é deny, mas o write ficava SEM snapshot TRIAD quando o VGP avisava | `triad_hook::take(sessão, caminho)` volta `Some` |

Nota de telemetria (`pre_bash`): `maybe_emit_a1_block` mede o contexto da cadeia ANTES do merge; o aviso do VGP (só em modo ativo + warn) não entra nesses bytes.

## I. Prova

**Um caso REAL de `warn` do motor** (achado rodando `touring vgp check` em entradas sintéticas, sem corpus held-out): o motor nega TUDO em `/tmp/` e em código inline, então o `warn` de projeto exige arquivo do projeto FORA de `/tmp` (`from pkg.mod import real` + `real.inexistente()` -> `atributo inexistente`, `warn`); e inline, um atributo inexistente num `str` é `warn` (`x = "abc"; x.inexistente()`). O e2e usa projeto em `CARGO_TARGET_TMPDIR`, `[vgp] modo = "ativo"` e confere na linha de sombra que o motor falou (`modo: ativo`, `verdict: warn`) — sem isso o teste seria vácuo.

**Vermelho verdadeiro** (handlers ORIGINAIS, `logs/p8/red_e2e.log`): `cargo test -p touring-hooks --test vgp_ativo_ordem_e2e` -> 4 passed / **2 failed**. `bash_aviso_do_vgp_nao_esconde_o_bloqueio_do_comando_perigoso`: stdout `{"hookSpecificOutput":{"additionalContext":"[VGP · aviso] atributo `inexistente` (<string>:1)", …}}`, sem `permissionDecision` (o `&& rm -rf /tmp/work` passava). `edit_aviso_do_vgp_nao_esconde_o_deny_de_sintaxe`: idem, `"[VGP · aviso] nome `x` (uses.py:4) · atributo `inexistente` (uses.py:3)"`, sem deny. Os 4 controles passam (inclusive o `rm -rf` sozinho: o validador existe neste ambiente).

**Verde** (`logs/p8/green_*.log`, rc=0): `verdict` 11 passed · `vgp_ordem_tests` dos três handlers 11 passed (4 + 4 + 3) · `gates97_tests g10_route` 22 passed · e2e 6 passed.

| Teste (handlers, costura) | Antes (costura = comportamento original) | Depois |
|---|---|---|
| `pre_bash::…aviso_do_vgp_nao_esconde_o_bloqueio_de_comando_perigoso` | aviso retorna, `rm -rf` não é avaliado | Deny `rm -rf` + aviso |
| `pre_edit_prevention::…aviso_do_vgp_nao_esconde_o_deny_de_sintaxe` | idem para o S210 | Deny S210 + aviso |
| `pre_write::…aviso_do_vgp_nao_pula_o_resto_do_pipeline` | TRIAD não armado | TRIAD armado + aviso no contexto |
| controles por sítio: sem aviso (deny/armação iguais), inofensivo com aviso (só aviso), **deny do VGP continua encerrando** | passam | passam |

**Mutantes** (15, todos MORTOS, `mut/out4/*.json`, restaurados por conteúdo; os pedidos em negrito):

| Mutante | O que muda | Morrem |
|---|---|---|
| **VGP-01a / 02a / 03a** | «aviso retorna antes do deny» em `pre_bash` / `pre_edit_prevention` / `pre_write` | o teste do caso de cada sítio (e o e2e) |
| VGP-01b / 02b / 03b | o deny do VGP é descartado em vez de encerrar | `deny_do_vgp_continua_encerrando_a_cadeia` de cada sítio |
| **VGP-04** | o deny perde o aviso (regra compartilhada) | `deny_vence_e_leva_o_aviso_na_razao`, `g5_advisory_nao_engole…`, os testes de `pre_bash` e `pre_edit_prevention`, o e2e do bash (5) |
| VGP-05 | `Allow` + avisos perde os avisos | `sem_veredito_o_aviso_vira_contexto…`, a tabela (2) |
| VGP-06 | o adaptador JSON diverge do enum | a tabela de precedência |
| VGP-07 | JSON sem veredito perde o aviso | `sem_veredito…`, `g5_bash_inofensiva…` (2) |
| VGP-08 | detecção de deny invertida no JSON | a tabela, `g5_advisory_nao_engole…` (2) |
| VGP-09 | avisos DEPOIS do contexto | `a_reescrita_mantem_o_updated_input` |
| ORD-02 / 03 / 07 | (do adendo 1, refeitos: «primeiro gate que responde vence» no G5 e no S3; o G1 volta a decidir a 2ª inspeção) | os mesmos de antes |

**Gates** (fase 8, uma vaga, `logs/p8/`): `cargo fmt --all -- --check` rc=0 · `cargo clippy --workspace --all-targets -- -D warnings` rc=0 · `touring-hook-runtime` 499 passed · `touring-cli` 686 passed · `touring-hook-handlers --features pre-hooks,post-hooks` completo 744 passed (733 + 11 novos) · `touring-hooks` completo rc=0 (inclui `cli_suggester_e2e` e os dois e2e novos) · `touring-server` completo rc=0 (1654 + os e2e). Os 3 handlers foram restaurados por `cp` do conserto depois do vermelho e o sha conferido (`sha_fixed.txt` = `sha_reinstalado.txt` = final).

## J. Dados (início do sha256)

| Arquivo | sha256 |
|---|---|
| `crates/touring-hook-runtime/src/verdict.rs` (novo, 317 linhas) | `60be360b9c530aeb` |
| `crates/touring-hook-runtime/src/lib.rs` (+1: `pub mod verdict;`) | — |
| `crates/touring-hook-handlers/src/hooks/pre_bash.rs` (orig `88e86360…` -> final) | `17d6fc2080c8441f` |
| `crates/touring-hook-handlers/src/hooks/pre_edit_prevention.rs` (orig `6de975fb…`) | `d7a9bfdbb5175cab` |
| `crates/touring-hook-handlers/src/hooks/pre_write.rs` (orig `57391fe6…`) | `d077ecea221311c6` |
| `crates/touring-hooks/tests/vgp_ativo_ordem_e2e.rs` (novo, ~240 linhas) | `6f32e21ba8fc4712` |
| `crates/touring-cli/src/cli_suggester.rs` (`merge_gate_verdict` removido; chama o `verdict`) | `d7d66fe63d783b42` |

Cópias de trabalho: `scratchpad/vgp/{orig_*,fixed/*}.rs`; manifesto `mut/manifest4.json`; sonda do motor `scratchpad/vgpx/probe{,2}.py`. Sem commit.

## K. Limites declarados

- **Entre hooks diferentes** (ex.: `cli-suggest` e `pre-bash` rodando em paralelo para o mesmo Bash), quem resolve a precedência é o Claude Code; o invariante é nosso DENTRO de cada hook. Não verifiquei como o Claude Code mescla os dois (declarado como limite pela coordenadora).
- **M3 x G7 (ramo Read)**: documentado como está, sem caso reproduzido; nenhum teste novo.
- **`pre_edit.rs` (o outro handler `pre-edit`)** não usa o VGP (só reaproveita `compose_pre_edit_warning`), por isso não foi tocado.
- O modo `ativo` em produção continua sendo escolha do projeto (V4); o e2e o liga por `[vgp] modo = "ativo"` no `touring.toml` do projeto de teste, com `TOURING_VGP_SOMBRA_FILE` apontando para o diretório do teste (nada vai para `~/.claude/touring/vgp/sombra.jsonl`).
- O vermelho dos testes unitários por sítio é o mutante «aviso retorna antes do deny» (a costura não existia nos originais); o vermelho verdadeiro é o do e2e sobre os handlers originais.
