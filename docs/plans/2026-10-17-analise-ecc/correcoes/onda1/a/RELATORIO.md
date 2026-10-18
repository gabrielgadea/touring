# Onda 1, parte A — hooks e cota (18/10/2026)

Trabalho no worktree espelho `/home/gabrielgadea/projects/touring-wt-onda1`; nada do main nem do `~/.claude/` vivo foi tocado
(único efeito vivo: 1 `touring memory store` do checkpoint desta onda, `status: stored`). Sem commit, stash, push nem kill.

## Entrega

| Artefato | sha256 (16 primeiros) |
|---|---|
| `a.patch` — só as mudanças desta parte (11 arquivos, +1529/−34), base = cópia do main de 18/10 | `58cbf6047546e8a6` |
| `vivos/settings.hooks.diff` — diff da seção `hooks` de `~/.claude/settings.json` (os demais campos não mudam: verificado) | `783dec30682f3fe5` |
| `vivos/checkpoint.md` — novo `~/.claude/commands/checkpoint.md` | `27f5d8eaab4e6180` |
| `vivos/test_checkpoint_command.py` — prova do bloco bash do comando (17 testes) | — |
| `mutantes/mutants.py` + `mutantes.json` + `rerun-*.json` — runner e resultado | — |

Verificado: `git apply --check a.patch` no main passa; aplicar o patch sobre as bases e comparar com o espelho dá 11/11 arquivos iguais.

**Ordem para aplicar (importa):** (1) `git apply a.patch` no main; (2) `ln -s /home/gabrielgadea/projects/touring/client/hooks/config_protection.py ~/.claude/hooks/config_protection.py`;
(3) `python3 scripts/install_claude_hooks.py --apply` (registra o hook novo, remove `WorktreeCreate`/`WorktreeRemove`; o `--lint-only` da fonte dá 0 achados); (4) `cp vivos/checkpoint.md ~/.claude/commands/checkpoint.md`.
Inverter (3) e (2) registraria um hook cujo script não existe. `status_monitor.sh` e `check_context.sh` já são symlinks para `client/hooks/`: o item N4 vale assim que o patch entra no main.

## N4 — cota da franquia na barra de status

**Desenho.** `status_monitor.sh` lê `rate_limits.five_hour|seven_day` com um `jq` e desenha `5h 60% ↻1h04 · 7d 50%` ao fim das 3 linhas de saída (verde/SALVE/COMPACT).
Faixas por janela: verde < 70, amarelo < 85, vermelho ≥ 85 (`QUOTA_WARN_PCT`/`QUOTA_ALERT_PCT`). `↻` só no 5 h, contagem **arredondada para cima** ao minuto, só com reset futuro e ≤ 10 h.
**Ausência nunca vira zero:** payload sem `rate_limits`, `rate_limits` de tipo errado, janela sem número, lixo (`"abc"`, `true`, `[]`, negativo, string `"50"`) → nada daquela janela; só `used_percentage` numérico conta (truncado, teto 100).
Alerta único: aos 85% do 5 h o monitor grava `~/.claude/touring/context_alert/<sessão>.quota` (linha 1 = %, linha 2 = `resets=<epoch>`, a identidade da janela) — mesmo diretório e mesmo ciclo armar/limpar do `.pct` do SALVE/COMPACT;
o `check_context.sh` (PostToolUse, o mesmo canal `additionalContext`) o entrega **uma vez por janela** (selo `.quota.delivered` guarda o `resets=` entregue, então uma janela nova avisa de novo mesmo sem render abaixo do limiar).
Sai quando a leitura cai abaixo do limiar; render sem leitura **não** desarma. Vencendo junto com o contexto, sai UMA mensagem (contexto primeiro, byte a byte igual ao de antes). Independe do sensor de contexto e do monitor de projeto. Marcador cuja janela já zerou é velho: não é anunciado. Entrega vira 1 linha no journal (`detail: quota_5h`).
Texto do alerta: porcentagem e hora do reset reais + `python3 ~/.claude/hooks/rate_limits_snapshot.py --show` + a regra «descer ao executor mais barato».

**Vermelho → verde.** RED: 57 falhas / 19 passes (os 19 são negativos «ausente mostra nada», vacuamente verdes antes — por isso os mutantes). GREEN: `test_status_monitor_quota.py` 81 testes; `test_status_monitor_format.py`, `test_s42_context_sensor.py` e `test_s41_s42_s43.py` seguem verdes (115 na 1ª rodada conjunta).
**Largura:** o exemplo do pedido (`5h 60% ↻1h04 · 7d 50%`) já tem 21 colunas; o pior caso (`5h 100% ↻4h59 · 7d 100%`) tem 23 — o «~18» não é atingível com o formato pedido; está fixado em teste (≤ 23).

## T10 — proteção da régua de lint/format/teste

**Desenho.** `client/hooks/config_protection.py`, PreToolUse `Edit|Write|MultiEdit|NotebookEdit`, `permissionDecision: "ask"` com a razão pedida («mudar a régua para passar no check é o atalho que a REGRA #21 proíbe; corrija o código, ou confirme se a mudança da régua é intencional») + o arquivo/tabela que disparou.
1. **Pelo nome** (sem distinção de caixa): `ruff.toml`, `.ruff.toml`, `clippy.toml`, `.clippy.toml`, `rustfmt.toml`, `.rustfmt.toml`, `deny.toml`, `.eslintrc*`, `eslint.config.*`, `.prettierrc*`, `biome.json`, `pyrightconfig.json`, `mypy.ini`, `.pre-commit-config.yaml`, `.cargo/mutants.toml`.
2. **Pelo conteúdo, ANTES×DEPOIS do texto que a ferramenta produziria** (Edit/MultiEdit simulados sobre o disco; Write = conteúdo novo): `pyproject.toml` (tabelas `[tool.ruff|pytest|mypy|pyright|coverage|…]`, comparação semântica por `tomllib`; TOML inválido cai para a comparação por seção do texto), `setup.cfg`/`tox.ini` (seções de lint), `lib.rs`/`main.rs` (`#![allow(…)]` novo, por multiconjunto de nomes: alargar um allow existente conta; remover, apertar, `#[allow]` de item e menção em comentário não). Procurar o cabeçalho no trecho editado perderia `select = ["E"]` → `select = []`, que é exatamente o atalho.
3. Payload malformado, tool desconhecida, caminho ausente, arquivo ilegível → stdout vazio, exit 0 (fail-open; motivo em stderr). Kill switch humano `TOURING_CONFIG_PROTECTION_DISABLED=1`. Saída ASCII pura (sobrevive a qualquer locale).

**Divergências do precedente ECC (deliberadas).** Criar o arquivo também pergunta (um `ruff.toml` novo numa subpasta vence o do raiz); veredito por comparação antes×depois em vez de lista de nomes só.
**Além da lista do pedido (veto do Gabriel, uma linha cada):** `pytest.ini`, `.coveragerc`, `.flake8`, `.pylintrc`, `.shellcheckrc`, `biome.jsonc`, `prettier.config.*`; tabelas `black|isort|pylint|bandit` no pyproject; seções `flake8|pycodestyle|pydocstyle|isort|pylint|bandit` no ini. **Fora do escopo, recomendo:** `Cargo.toml` `[workspace.lints.*]` (existe em `Cargo.toml:646,705`) é a mesma brecha para clippy/rustc e cabe em 1 linha da tabela `LINT_TOOLS`-equivalente — não incluí sem seu sim.

**Registro.** `client/claude/hooks.settings.json` (grupo novo no fim de `PreToolUse`, timeout 5, sem `if`: filtro por conteúdo dentro do handler) → `vivos/settings.hooks.diff`. Falta o symlink em `~/.claude/hooks/` (passo 2 acima).
**Não verificado:** se `ask` de um hook ainda **pergunta** sob `bypassPermissions`/`auto` (a doc que consegui ler não diz; as sessões do Gabriel rodam assim). Teste de 1 minuto depois de aplicar: editar um `ruff.toml` descartável numa sessão `auto` e ver se há pergunta; se não houver, o hook só ensina via razão e o remédio de substrato é `permissions.ask` em `settings.json`.

**Vermelho → verde.** RED: 150 de 152 testes vermelhos sem o script (os 2 restantes são de registro, vermelhos antes do registro). GREEN: 152 testes. Dois acertos da suíte durante a construção: `Read` de `ruff.toml` perguntava (faltava o filtro de tool) e `content: 3` malformado num caminho protegido pergunta por design (corrigi o teste, não o hook).

## Hook `WorktreeCreate` quebrado

**Causa (contrato oficial, `code.claude.com/docs/en/hooks`, lido em 18/10/2026).** Um hook `WorktreeCreate` **substitui** o `git worktree add` padrão e «must return the path to the created worktree directory» (comando: último linha não vazia do stdout; HTTP: `hookSpecificOutput.worktreePath`); «if the hook fails or produces no path, worktree creation fails». O payload de **criação** traz `name` (slug), **não** `worktree_path` — esse campo é só do `WorktreeRemove`. O registro apontava `touring-hook worktree-create`, o passthrough de `run_lifecycle_event` (`crates/touring-hooks/src/main.rs:605`), que só grava `__worktree_create__` no knowledge DB.
**Reprodução medida:** `echo '{"hook_event_name":"WorktreeCreate","name":"probe-onda1a",…}' | ~/.claude/hooks/touring-hook worktree-create` → `rc=0`, **0 bytes** em stdout, stderr vazio ⇒ «hook succeeded but returned no worktree path» (o erro de 17/10).
Os dois handlers Rust (`handle_worktree_create` em `touring-dispatch/src/lifecycle/worktree.rs:14`, `WorktreeEnterHandler` em `touring-cortex/src/handlers/lifecycle.rs:940`) leem `worktree_path`, que nunca vem na criação: eram no-ops mesmo se o stdout fosse certo. O `statusMessage` «initializing worktree index…» não descrevia nada que acontecesse.

**Escolha: (b), sem Rust.** (a) obrigaria o handler a reimplementar o que o Claude Code já faz ao criar (`git worktree add`, ref-base, `.worktreeinclude` — «not processed» quando há hook —, branch) e a pagar a remoção (`WorktreeRemove` com exit 0 conta como removido **mesmo sem apagar o diretório**; o CC nunca apaga o branch). O ganho seria o registro de um evento que não indexa nada. (b): sem hook de criação o CC usa o default; `WorktreeRemove` só dispara para worktree criado por hook, então sairia junto (REGRA #0: não deixar registro morto). O worktree é projeto próprio sem hook algum: `.git` arquivo ⇒ `paths.rs:414` já o resolve como projeto separado.
**Guarda estrutural (lint L5)** em `scripts/install_claude_hooks.py`: `touring-hook worktree-create|remove` em `WorktreeCreate|WorktreeRemove` vira achado; `--apply` recusa uma fonte que o reintroduza. Teste do contrato: a fonte não registra nenhum dos dois eventos.
**Vermelho → verde:** 2 testes novos em `test_s49_install_claude_hooks.py` (RED 2 falhas → verde; arquivo 15 testes).
**Para o próximo release (Rust, não entregue):** apagar/religar os dois handlers órfãos (`worktree_path` pertence ao REMOVE) — hoje inalcançáveis, mas ainda listados em `docs/reference/hooks.md` como subcomando.
**Pendente de confirmação em sessão viva:** depois do passo 3, `Agent(isolation: "worktree")` deve voltar a criar o worktree (a seção de hooks costuma ser capturada no início da sessão: reiniciar o Claude Code, se a mudança não valer na hora).

## `/checkpoint` quebrado

**Causa.** O comando chamava `~/.claude/tools/Touring-native tooling/workflows/checkpoint.sh` (e dependia de um `plan-detector.sh`): nenhum dos dois existe.
**Desenho.** Um bloco bash único: (1) SNAPSHOT via `scripts/snapshot_safety.py` (fallback para a cópia do bundle vgp-motor-base) — commit WIP da working tree (rastreado + não rastreado) numa ref **nova** `safety/<data>/<tópico>-<HHMMSS>` por índice temporário + `commit-tree` + `update-ref`; índice, HEAD, branches e arquivos intocados; **nunca `git stash`**; (2) `touring memory store --tier semantic --tag "#kind:outcome"` com o resumo escrito pelo modelo (executado de verdade nesta onda: `stored`, `ignored_facets: []`); (3) `touring learning reward checkpoint 1.0` só com snapshot medido (`OK`/`COBERTO-PELO-HEAD`) e memória gravada. Falha de snapshot/memória ⇒ exit 1 e linha `SNAPSHOT: FALHOU`; fora de git ⇒ só a memória. Tópico e resumo entram por heredoc com aspas (`$ARGUMENTS`/backticks/`$()` são dado, nunca código — o mutante CK07 provou que o resumo, que o modelo escreve com backticks, exigia isso).
**Promoção:** `snapshot_safety.py` morava num diretório de plano **não versionado** no main (`git ls-files` vazio) — passou a `scripts/snapshot_safety.py` (cópia byte a byte, `+x`) com `scripts/test_snapshot_safety.py` (8 testes em repositórios git reais: arquivos/índice/HEAD idênticos antes×depois, estado staged preservado, ref existente nunca sobrescrita, árvore limpa sem ref, teto de tamanho, prefixo `safety/`, 4 mutantes).
**Provas (`vivos/test_checkpoint_command.py`, 17 testes):** o bloco é extraído do `.md` e roda num repo real com `touring`/`date` falsos no PATH — repo sujo (ref criada, nada se move, staged preservado), limpo, sem tópico (exit 2), fora de git, script ausente, ref recusada, memória que falha (sem reward), 5 tópicos hostis, resumo com sintaxe de shell, acentos/slug, bloco sem `stash`, `shellcheck` limpo. O antigo apontava para um script inexistente (asserção `test_the_old_command_…`).
**Não exercido ao vivo:** `touring learning reward checkpoint 1.0 <slug>` (sintaxe lida de `--help`; o teste usa stub — rodar de verdade poluiria o RL com um +1 de teste). Vai ser exercido na primeira vez que o Gabriel usar o comando.

## Mutantes

81 mutantes, cada um uma substituição exata numa CÓPIA do alvo (o worktree nunca é mexido), com **controle sem mutação antes** (6 suítes verdes: cp 151, reg 166, s49 15, quota 81, snap 8, ck 16 — contagens da 1ª rodada). Morto = reprovação por asserção; `bash -n`/`compile()` falho = inválido (0 inválidos).

| Alvo | Mutantes | Mortos |
|---|---|---|
| `config_protection.py` | 25 | 25 |
| `install_claude_hooks.py` (L5) | 4 | 4 |
| registro em `hooks.settings.json` | 4 | 4 |
| `status_monitor.sh` | 21 | 21 |
| `check_context.sh` | 11 | 11 |
| `snapshot_safety.py` | 7 | 7 |
| `checkpoint.md` | 9 | 9 |
| **Total** | **81** | **81** |

1ª rodada: 78/81. Três sobreviventes **eram buracos de teste**, não equivalentes: CP20 (comentário contava como mudança no caminho textual — nenhum teste exercia o caminho textual com comentário), CC09 (o teste de «porcentagem ilegível» usava um `resets=1` já vencido, então passava pela guarda errada) e CK07 (nenhum teste passava sintaxe de shell no resumo). Reforcei os três testes e reexecutei os três mutantes: mortos (`rerun-*.json`). Mutante equivalente descartado em desenho: `. >= 0` no `jq` (o regex `^[0-9]{1,3}$` repete a defesa).
Achados pela própria campanha: faltava teste do segmento nas linhas SALVE/COMPACT (só a verde era exercida) — adicionado antes da rodada.

## Testes e lint

- `client/hooks/tests` inteiro: **1378 passed** (127 s). `scripts/test_snapshot_safety.py` 8. Vivos: 17.
- `ruff check` limpo em todos os arquivos tocados (rodado da raiz do repo — de dentro de `tests/`, o `helpers` é tomado por first-party e o I001 acusa até os testes existentes); `shellcheck` limpo nos dois `.sh`. Achado e corrigido de passagem: `scripts/install_claude_hooks.py` tinha shebang sem `+x` (EXE001) — agora 755 (único `old mode/new mode` do patch).

## Falhas observadas fora desta parte (REGRA #21 — registradas, não corrigidas por serem de outras mãos)

- `scripts/test_sync_client_skills.py::test_mirror_is_in_step_with_the_live_skills` e `::test_cli_check_passes_on_this_repo`: **7 arquivos** do espelho `client/` atrasados em relação ao vivo (`prose_integrity_gate.py` + teste, `loop-engineering/SKILL.md`, `references/loop-camada-3-e-hooks.md`, `loop_converged.py`, `rules/touring-process-hygiene.md`, `agents/touring-scouter.md`). Edições vivas de outras sessões; o remédio documentado é `python3 scripts/sync-client-skills.py --apply`, que convém rodar depois que as outras partes fecharem, para não correr com elas. Não toca nenhum arquivo deste patch.
- `Cargo.toml [workspace.lints.*]` sem proteção (acima).
