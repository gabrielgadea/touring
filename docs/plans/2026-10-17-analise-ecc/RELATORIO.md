---
type: Report
title: Análise do ECC (affaan-m/ECC) — lições e inspirações para o Touring
timestamp: 2026-10-17T19:00:00Z
pedido: "faça uma exploração e investigação exaustiva, completa, detalhada e analitica em https://github.com/affaan-m/ECC e verifque tudo que pode servir como lição ou inspiração para complementação, correção, aperfeiçoamento, refinamento do touring"
tags: ["#kind:report", "#domain:harness", "#process:research", "#status:active"]
---

# ECC × Touring

## Método

- **Fonte primária**: clone raso de `affaan-m/ECC`, commit `76aaa77d` (15/10/2026; 4.621 arquivos, 2.305 commits), lido e
  NUNCA executado. O clone ficou no scratchpad da sessão efe418c2.
- **Leitura por área**: 5 agentes Sonnet, um por área (A hooks, B memória, C contexto/custo, D orquestração/avaliação/`ecc2`,
  E segurança/governança). Toda afirmação sobre o Touring exigia comando de verificação. Saída: `achados/{A..E}-*.json`.
- **Pesquisa web**: workflow `deep-research` (103 agentes; 21 fontes; 95 afirmações extraídas, 25 verificadas por 3 votos
  adversariais, 18 confirmadas, 7 refutadas). Saída: `achados/W-pesquisa-web.json`.
- **Conferência adversarial local** (esta sessão) das afirmações sobre o Touring que sustentam as correções:
  `scripts/sonda_git_aninhado.py`, `scripts/confere_afirmacoes_touring.sh` e `scripts/familias_context_budget.py`.
  As linhas marcadas **[V]** foram reverificadas por mim. As demais vêm de um agente com evidência citada e ficam
  marcadas **[A]**.

## 1. O que é o ECC (fatos)

- **Pacote**: `ecc-universal` v2.2.3 (01/10/2026), MIT, de Affaan Mustafa; renomeado de `everything-claude-code`.
- **Escala declarada**: 282 mil estrelas e 42 mil forks em 17/10, segundo a API do GitHub; 71 agents, 311 skills,
  97 commands e 23 rules; alvos Claude Code, Codex, Cursor, OpenCode, Gemini e mais 11 harnesses.
- **Núcleo**: catálogo Node (hooks em processo Node por evento, `run-with-flags.js` com perfis), aprendizado contínuo v2
  ("instincts" destilados por um observer Haiku em segundo plano, opt-in), cofre de memória, eval-harness com journal
  encadeado por hash e `ecc2` (alpha Rust: sessões em worktrees, merge queue, teto de orçamento, gate de promoção de config).
- **Manutenção**: dos últimos 300 commits, 234 são de um único autor; nenhum autor bot.
- **Evidência de ganho**: nenhum ganho declarado foi medido. Os números de economia não têm método e se contradizem entre
  arquivos. A única medição de efeito é deles mesmos e deu nula: o GateGuard teve 52% contra 38%, McNemar p = 0,210, com
  custo de +37 a +61% em turnos e tokens.

**Veredito**: o ECC vale pelo **método** de alguns componentes (medir o próprio gate, corpus rotulado de cenários,
pré-voo do juiz, separar guarda de conselheiro, recusar entrada não inspecionável) e por algumas **guardas concretas**
que o Touring não tem. O catálogo e as afirmações de economia não servem de evidência. Popularidade não é efeito: nenhuma
das 18 afirmações confirmadas liga as 282 mil estrelas a ganho medido.

## 2. Defeitos do Touring que a comparação revelou (corrigir)

| # | Defeito | Evidência | Conserto proposto | Custo |
|---|---|---|---|---|
| C1 **[V] segurança** | O guarda da REGRA #11 só bloqueia a forma direta. `bash -c`, `sh -c`, `eval`, heredoc para shell, `python -c os.system` e `xargs` passam pelos **6** hooks PreToolUse de Bash, incluindo os embrulhos `hw-slot … -- bash -c '…'` usados todo dia | `scripts/sonda_git_aninhado.py`: só «simples» é bloqueado (`block_git.sh`). Causa [A]: `client/hooks/_shell_mask.py` apaga todo literal entre aspas e todo corpo de heredoc | Tratar o payload de `sh/bash/zsh -c`, `eval`, heredoc para shell e `xargs` como código: re-escanear com orçamento proporcional à entrada; estouro vira deny que nomeia o token. Corpo de heredoc só é dado para destino passivo (`cat`, `tee`). No validador Rust (`bash_ast_validator.rs`), com teste por mutação de cota (padrão do `block-no-verify.test.js` do ECC) | M |
| C2 **[V] segurança** | O shim de usuário `~/.claude/hooks/touring-hook` (Layer 2) sobe a árvore a partir de `CLAUDE_PROJECT_DIR`/`PWD` e dá `exec` no 1º `.touring/bin/touring-hook` executável, sem verificar nada. Um repositório com esse arquivo executa o próprio código como o usuário em todo evento de hook, contornando o modelo de confiança de pasta do Claude Code | shim, linhas 50-69. Os bins por projeto legítimos são symlinks para `~/.touring/toolchains/<v>/bin/` (analise e konverter → 30.4.96) | Aceitar o candidato só se o `realpath` estiver sob `$TOURING_HOME/toolchains/` e o dono for o uid, sem escrita de grupo ou outros. Senão, cair para a Layer 3 e registrar uma vez. Não quebra nenhum projeto atual | P |
| C3 **[V] segurança/CI** | `release.yml` declara `id-token: write` no nível do **workflow**, então o job `build`, que restaura `Swatinem/rust-cache`, recebe token OIDC. É a classe do incidente TanStack (envenenamento de cache + OIDC). F4.7 (`cicd.rs`) não detecta e ainda pune a ausência de cache | `release.yml:23-25` e `:68`; o job `release` já declara o próprio `id-token` (`:126-127`) | `id-token` só no job `release`; `persist-credentials: false`; portar as regras (id-token por job, `pull_request_target` com checkout do PR, cache em job com OIDC) para `cicd.rs` como BLOCK em workflow de release | P |
| C4 **[V]** | `read_stdin` devolve `{}` em timeout, vazio e erro de leitura. A guarda vê comando vazio e libera; não há teto de bytes | `hook_runtime_ext.rs:445,465,474` | Separar guarda de conselheiro: guardas negam com `stdin_state` em timeout, read-error ou parse-error, nomeando a saída; conselheiros seguem fail-open. Antes, medir no trace a fração de `stdin_state ≠ ok` (o socketpair do libuv já enganou uma vez) | M |
| C5 [A] | Release por push de tag não confere se o CI passou naquele SHA nem a assinatura da tag; os gates fortes ficam só no `propagate-release.sh` local | `release.yml:18-22,119-122` | Job `verify` antes do `build` (`gh api`: tag==SHA, `ci.yml` success, tag assinada), com deadline e paginação limitada | M |
| C6 **[V]** | `memory store` (CLI e store do servidor) grava sem varrer segredo, bidi ou caracteres de controle. O recall injeta o texto de volta como contexto | grep em `cli/memory.rs` e `memory_store.rs`: só o `transcript_miner` redige | Varredura de segredo e de bidi/controle na escrita e na leitura do recall (modo sombra primeiro). Ao injetar, embrulhar como «contexto, não instrução» | P |
| C7 **[V]** | Instrumento que mente: os gotchas somam **388.840 acertos e 0 erros prevenidos** | `touring gotcha stats` | Dar produtor ao `prevented` ou retirar o campo; zero sem produtor é «não medido» (L2) | P |
| C8 **[V]** | `TierManager` (demote por ociosidade) só tem consumidor de teste: é órfão de produção | índice: 3 definições, 0 referências; `git grep` só acha `tests/cross_audit.rs` | REGRA #0: ligar a um sweep (com o decaimento do item I9) ou remover | M |
| C9 **[V]** | Drift de doc: o juiz `loop_converged.py` tem **8** cláusulas (inclui `judge_intact` e `measured_whole_scope`); o `~/.claude/CLAUDE.md:221` diz «6 cláusulas» | `loop_converged.py:10-17` | Atualizar a constituição. Depende de aprovação do Gabriel (META-EVOLUTION) | P |
| C10 **[V]** | KPI `context_budget` em FAIL: 12.109 B injetados por turno contra teto de 2.000; a família «other» tem **82,6%** dos bytes (1.917 de 2.452 blocos sem proposição); n = 3 sessões | `scripts/familias_context_budget.py` | O emissor carimba `family` e `dedupe_key` em todo `additionalContext` (padrão `ecc-context-monitor.js`), em vez de o KPI inferir por regex depois do fato. Classificar os 82,6% antes de mexer em teto | M |
| C11 **[V]** | 2 U+FEFF literais na regex de frontmatter (BOM opcional, intencional) | `agent_model_gate.py:126`, teste `:195` | Trocar pelo escape `﻿` (o `re` interpreta em raw string); gerar por script (a ferramenta Write decodifica escapes) | P |
| C12 [A, a confirmar] | `touring scan` dá «0 CWE» num arquivo em que F2.1 acusa CMDi (CWE-78) | sonda do agente E; causa não verificada (pode ser escopo de linguagem) | Registro único de detectores e teste de corpus em que `scan` e F2.1 concordam | M |

Corrigido hoje, no mesmo espírito (achado durante a integração, não pelo ECC): o hook `touring-quality-block-all.sh`
calculava `LOCK_WAIT` e nunca o repassava ao wrapper (espera de 900 s). Detalhe em
`docs/plans/2026-10-08-vgp-motor-base/RETOMAR-AQUI-2026-10-09.md`, seção ~18:50Z. O ECC chama a mesma classe de
«skip de guarda sem registro» (A5 do agente A).

## 3. Inspirações a adotar (priorizadas)

### Alta prioridade

| # | Inspiração | No ECC | Proposta para o Touring | Custo |
|---|---|---|---|---|
| I1 | **Sonda de aderência por regra em 3 níveis de rigor do prompt** | `skills/skill-comply/` (supportive → neutral → competing; ordem temporal conferida por código; passo de baixa aderência vira candidato a hook) | Medir, por regra do protocolo, a aderência headless em 3 prompts com classificador determinístico (as classes do `audit_session_code_mode.py`). Ataca de frente o `protocol-adherence-diagnosis`: diz quais regras precisam virar afordância | M |
| I2 | **Ablação pré-registrada dos próprios gates** | `docs/gateguard/question-effectiveness.md` (braços off/gate/sem-alvo/placebo, McNemar exato, custo em turnos e tokens, resultado nulo publicado) | Pré-registrar G1/G10/G11 e o nudge do `cli_suggester` ligados × desligados (os kill switches já existem), com o fato decisivo FORA do repositório e custo medido. Cada gate declara `assume:` e `retest_on: model_change` | G |
| I3 | **Pré-voo do juiz: referência PASS, decoy FAIL, saturação** | `docker/gateguard-effectiveness/README.md:36-39`; `complex-eval/DESIGN.md:178` | `judge_attest` exige, por gate, um fixture de referência (PASS) e um decoy com a forma certa e o conteúdo errado (FAIL); alerta para gate com 100% de PASS em N rodadas. Regressão em **pass^k**: o retry-once do gate 5.5 é pass@2 | M |
| I4 | **Corpus rotulado de cenários de gate com comparação contra a ref anterior** | `docs/gateguard/evaluation.md` (must-deny, redundante; bypass deve ser 0; roda em refs antigas) | `scripts/gate_replay.py` com fixtures para G1-G11, `block_git` e o deny do pre-bash; métricas de bypass (= 0), negação redundante e divergência contra o release anterior; ligado ao `propagate-release`. Semear com as formas de C1 | G |
| I5 | **Aceite rodável no cartão de tarefa** | `team-agent-orchestration` (`acceptance`, `merge_gate`, `handoff`) | `decompose add --accept-cmd --scope`; `update --status completed` roda o aceite e recusa nomeando o motivo; `claim` recusa escopo sobreposto a claim vivo (via `conflict-check`) | M |
| I6 | **Proteção de config de lint e dos insumos do juiz** | `config-protection.js` («agents modify these to make checks pass instead of fixing the code») | Handler pre_edit/pre_write: `deny.toml`, `clippy.toml`, `rustfmt.toml`, `ruff.toml`, `rust-toolchain.toml`, `.cargo/config.toml`, `.github/workflows/*` e os juízes (`loop_converged.py`, baselines). Criação livre; edição em sombra e depois token por comando | P-M |
| I7 | **Aviso proativo de cota por tier, e gate no executor** | `ecc-context-monitor.js` (tiers com dedupe; em $ estimado); `ecc2` (teto agregado) | O Touring tem o sinal REAL (`rate_limits.json`). Aviso uma vez por tier (7d ≥ 70/85/95%, 5h ≥ 90%) e por `resets_at`; o `agent_model_gate` recusa Agent não-haiku acima do limiar pré-registrado, nomeando a rota (D8) | P |
| I8 | **Scan de Unicode invisível nas entradas** | `scripts/ci/check-unicode-safety.js` (ZW, bidi, bloco Tag E0000-E007F…) | Script no CI e no `propagate-release` sobre skills, rules, agents, CLAUDE.md e `client/`; classe «invisível» no gate de prosa em sombra | P |
| I9 | **Ciclo de vida da memória: prune, decaimento medido, promoção entre projetos** | `instinct-cli.py` (TTL 30d, prune com dry-run, promote com ≥ 2 projetos e confirmação humana, ID de projeto pelo git remote) | `memory prune --unmeasured --older-than 30d --dry-run` por tombstone; decaimento por limite de Wilson sobre reward medido (não pela confiança autodeclarada, como no ECC); `memory promote` com 2+ projetos; export/import que entra **untrusted** com teto de confiança | M |

### Média prioridade

| # | Inspiração | Proposta | Custo |
|---|---|---|---|
| I10 | ID estável por hook e perfis (`ECC_HOOK_PROFILE`, `ECC_DISABLED_HOOKS`, dry-run) | `TOURING_HOOK_PROFILE` (lido só do ambiente do daemon, nunca por escape do agente), `TOURING_DISABLED_HOOKS=id,…` e `touring-hook explain <payload>`. Substitui a flag ad hoc herdada por headless e irmãs | M |
| I11 | Detector de sessão travada (`loop-status.js`: tool_use sem tool_result > 30 min, wakeup vencido, exit 2) | Subcomando em `sessoes_irmas.py`, agendado, com limiar por classe de comando | P |
| I12 | Classificador de ciclo de vida de worktree (dirty nunca limpa; só merged é removível) | `scripts/worktree-report` só leitura (há 16 worktrees hoje), alinhado à REGRA #11 | P |
| I13 | Painel de review fail-closed (lente que falha deixa o veredito incompleto; refutar exige confiança ≥ 0,8; prova obrigatória em achado HIGH) | No `panel.quorum` do cross-audit | P |
| I14 | Proveniência por unidade aprendida | Facetas automáticas `#source` (modelo, sessão, harness) e `#author` em todo `memory store`; separa lição de sessão irmã de lição curada | P |
| I15 | Lição com gatilho observável («Use when …») | 1ª linha e faceta `#trigger` em lições novas, validadas pelo `memory store` | P |
| I16 | Telemetria por skill com «sem medida ≠ saudável» | Ledger de execução de skill/ADW (desfecho + correção do Gabriel) e bloco `skills` no kpi com `unmeasured` explícito; writer estrutural (hook), nunca opt-in | M |
| I17 | Sanitizador pré-publicação (o repo é público) | `scripts/sanitize_check.py` só leitura: segredos, `/home/<user>` (1.423 ocorrências em 452 arquivos, 232 em `docs/kpi`), arquivos perigosos, histórico | M |
| I18 | Matriz de paridade de backend em 4 estados | `docs/backend-parity.json` (Anthropic × Kimi × MiniMax: hooks, gate de prosa, CEG, matriz de modelos) com `--check` | P |
| I19 | Higiene de todo escritor JSONL (redação, teto por registro, rotação, retenção) | Auditar os escritores (`quality-runs.jsonl` tem 9.944 linhas; `/tmp/hook_invocations.log`) e aplicar o padrão do `observe.sh` | M |
| I20 | Escada de escalonamento do roteamento | Coluna «se falhar no juiz → sobe para» na matriz da economia; juntar o log do gate ao desfecho do subagente para gerar rótulos decisão→desfecho | M |
| I21 | Consolidar o predicado de shell num lugar só | `block_git.sh` + `_shell_mask.py` (Python) e `mask_shell_data` (Rust) são duas implementações do mesmo predicado; mover para o handler Rust entrega C1 e corta forks (p50 ~120 ms por Bash, n = 15, ruidoso) | M |

### Baixa prioridade

- Journal ADW com `parent_hash` e `adw verify-run`; campo `non_claims` nos artefatos de juiz (receipt do ECC).
- Pontuação por dedução nas dimensões de segurança: defesa não soma pontos (AgentShield).
- Anel de 5 chamadas idênticas no post-tool-batch e `PostToolUseFailure` para `^mcp__|Skill|Agent` (só contador).
- Sensor de contexto obsoleto = «desconhecido», nunca «saudável»; auto-checagem do medidor de custo (o ECC passou 52 dias
  gravando zeros); fator bytes→tokens por família no KPI.
- Intervalo do aviso por crescimento de contexto: ECC 60k, billion-context 50K, plano P2 100K, nenhum medido. Pré-registrar
  a escolha pelo `compactions.jsonl` (alimenta o P2 da frente `2026-10-17-contexto-por-chamada`).
- KPI da superfície eager (skills ~9,6K tokens, regras ~20,6K): é ≤ 18% do contexto por chamada, então não é o gargalo.

## 4. Onde o Touring já é superior (manter)

- **Claim** atômico com escrita condicional e lease. O do ECC lê e grava, com corrida admitida.
- **Juiz por exit code** (`loop_converged.py`). A `verification-loop` do ECC é checklist em prosa.
- **Roteamento de modelo** julgado no executor (`agent_model_gate.py`), com KPI e consentimento do Fable. O ECC tem env
  global + frontmatter estático. Cuidado: o gate trata `CLAUDE_CODE_SUBAGENT_MODEL` como decisão tomada; se alguém o
  definir, como o ECC recomenda, as regras de adequação somem sem aviso.
- **Sandbox no kernel** (CEG: Landlock, seccomp, rlimit). O ECC oferece snippet de docker-compose.
- **Recall** RRF (BM25 + TF-IDF + ANN, com classes de valor), contra ranking lexical fixo.
- **`redact_secrets`** no sandbox e no `transcript_miner`; contadores de gate sempre ligados (no ECC são opt-in).
- **Skill pin gate** no CI e `install_claude_hooks.py --check` contra deriva; deny em JSON, não exit 2 com stderr.

## 5. Anti-lições (não copiar)

- **GateGuard** (negar a 1ª edição de cada arquivo até haver fatos): 2.574 linhas de hook e efeito não confirmado
  (p = 0,210) com +37 a +61% de custo. Só o método de medir serve.
- **Prosa como enforcement**: o «Prompt Defense Baseline» está colado em 71/71 agents e 0/311 skills, e o teste confere
  presença textual. É a persuasão que o `protocol-adherence-diagnosis` mediu e viu falhar (D8).
- **Confiança e decaimento escritos para o LLM**, sem código que os aplique; promoção pela média da confiança autodeclarada.
- **Import com a confiança da fonte**, injetado em toda sessão acima de 0,7: vetor de envenenamento.
- **A/B com n = 2 por variante** e qualquer delta positivo. Contraria «taxa pontual aprova o que o limite reprova».
- **`docs/LANE-RULES.md`** proibindo hashing de manifesto, ledgers e acceptance JSON: o oposto das leis L2/L3.
- **`harness-audit` por existência e contagem** («80/80»): Goodhart de presença.
- **`git stash`** como snapshot em `checkpoint.md` e no `harness-optimizer`: banido pela REGRA #11.
- **Resumo por LLM no PreCompact** (`claude --model haiku -p`): gasta crédito e não mede o que perdeu.
- **Processo Node por hook** (24-27 spawns); números de economia sem método; banir cache em todo o CI.

## 6. Afirmações do ECC sem evidência ou refutadas

- Sem medição: «hooks disparam 100% e skills 50-80%», «Haiku ~80% mais barato» × «3-4×» × «3×», «Sonnet 60% de economia»,
  «mgrep 50% menos tokens», «MCP reduz 200k a 70k», «catálogo de 27 agentes em 2-3k tokens».
- Refutadas na votação (0-3): a semântica fixa dos níveis de confiança (0,3 «sugerido», 0,7 «auto-aprovado»), as regras de
  decaimento e a «promoção automática» a global (o código exige comando humano com confirmação).
- Não verificáveis no clone: o processo dos ~3.596 PRs (sem metadados de PR; os 300 commits medidos não têm autor bot).

## 7. Perguntas abertas

- O que dizem críticas de terceiros sobre a cadeia de suprimentos de um plugin de 282 mil estrelas que instala hooks e
  scripts na máquina do usuário? Nenhuma sobreviveu à verificação.
- Um experimento de 4 braços (I2) mostraria efeito dos gates do code mode e do pre-edit, ou só custo, como no ECC?
- Qual é a fração real de `stdin_state ≠ ok` nos hooks do Touring (pré-requisito de C4)?
- Quantos escritores JSONL do Touring gravam sem redação, sem teto e sem rotação (I19)?

## Arquivos

- `achados/A-hooks.json` · `B-aprendizado-memoria.json` · `C-contexto-custo-roteamento.json` ·
  `D-orquestracao-verificacao-ecc2.json` · `E-seguranca-governanca-instalacao.json` · `W-pesquisa-web.json`
- `scripts/extrai_agente.py` · `sonda_git_aninhado.py` · `confere_afirmacoes_touring.sh` · `familias_context_budget.py`

## 8. Correções (Gabriel 17/10: «e corrija todas as falhas e defeitos encontrados ao longo da exploração»)

DAG `task_1792263269279844388`. Snapshots antes de qualquer mudança: `safety/2026-10-17/main-pre-ecc` (21cd6f19) e
`safety/2026-10-17/int97-pre-ecc` (98301443), feitos com índice temporário (árvores de trabalho intocadas).

| Item | Estado | Evidência |
|---|---|---|
| C2 shim walk-up | **feito e implantado** (vivo + fonte + int97 + docs) | `scripts/test_touring_hook_shim.py` 8 passed; contra o shim do HEAD 6 failed; `scripts/mutantes_shim.py` 6/6 mortos; smoke: analise e konverter resolvem o próprio bin, touring cai na camada 3; backup `correcoes/c2-shim/` |
| C9 cláusulas do juiz | **feito** | `~/.claude/CLAUDE.md:221` agora lista as 8 |
| C3/C5 workflows | **feito no main e no int97; efeito só depois de commit+push (ordem do Gabriel)** | `scripts/endurece_workflows.py` (idempotente): permissões mínimas no topo de `ci.yml`/`release.yml`, `id-token` só no job `release`, `persist-credentials: false` no build, 11 `dtolnay/rust-toolchain` por SHA (0 `uses:` sem SHA), job `verify` (CI verde no SHA, bloqueante; assinatura de tag só avisa porque `v30.3.0` não é assinada). F4.7 com o motor atual: release 0,860→0,919, ci 0,970→0,995. Backup `correcoes/c3-workflows/` |
| C1, jq mudo, C11, `why:env`, sensor do `check_context`, `test_skill_size_guard` | **feito e implantado** (main = vivo, int97) | `correcoes/py/py.patch` (sha 216ef9b8; a 2ª entrega, c27398e7, soma o D2); 181 casos aninhados, 107 vermelhos no original, mutantes 49/49 e 27/27 mortos (engenheiro). Revisão: reprodução de 4.000 comandos reais com `git` pelos dois guardas (`scripts/replay_block_git.py`): 0 negações novas, 1 falso positivo antigo desfeito (`git commit -m` citando operação destrutiva). Suíte 814 passed no main e no int97; doc canônico do protocolo git sincronizado (+ espelho CLEAN) |
| `status_monitor.sh` (achado na revisão) | **feito** | o `bc` não está instalado: a barra mostrava `1000KM` e o custo nunca aparecia. Aritmética em bash + `awk`; `client/hooks/tests/test_status_monitor_format.py` (4 vermelhos no original, 5 verdes); 3 avisos antigos do shellcheck (`WIN_SRC`, `USAGE_ABS`/`SIZE_FMT` sem leitor, `printf` com variável no formato) corrigidos |
| D2 contorno de hooks (`--no-verify`, `-n`, `-c core.hooksPath`, `SKIP`, `HUSKY=0`…) só onde o repositório alvo tem hook ativo; token `GIT_HOOKS_BYPASS_OK=1` | **feito e implantado** (main = vivo, int97); **o texto da REGRA #11 aguarda o OK do Gabriel** | `correcoes/py/RELATORIO.md` §10: 439 casos, 132 vermelhos no original, mutantes 121/121. Revisão: replay de 4.000 comandos reais **com o `cwd` de cada um** (`scripts/replay_block_git.py`, `correcoes/py/evidencia/replay_d2_comandos_reais.json`) deu 1 negação nova, o `git commit --no-verify` de 29/09 no analise, e nenhuma colateral. Defeito achado na revisão e corrigido: «hook ativo» era qualquer executável não-`.sample`, e o analise guarda 5 backups executáveis (`pre-commit.pre-pii-20260925`…) que o git nunca roda; agora só os 28 nomes do githooks(5) contam (`_GIT_HOOK_NAMES`): 76 vermelhos antes, 5/5 mutantes mortos com controle verde; suíte 1253 passed no main e no int97 |
| F4.7 regras novas, F2.4 rótulo + placeholders, `scan` × F2.1, `component list` lista backups, caminho absoluto no `docs/kpi` | **integrado no int97, gates verdes** | `correcoes/qa/qa.patch` (sha d1cf8094); `correcoes/gates-int97/` |
| C4 stdin fail-closed nas guardas, C10 família da injeção | **integrado no int97, gates verdes** | `correcoes/hk/hk.patch` (sha 7dc3f401) |
| C6 sanitização da memória, C7 `prevented`, C8 TierManager | **integrado no int97, gates verdes**. C8: `TierManager` removido (sem chamador, divergente do `maintain_tiers`, que segue sem chamador e é decisão pendente). C7: o consumidor `pre_write.rs` usava Wilson sobre `prevented` sempre 0 e dava score ~0 a todo gotcha já exibido (teste novo: 1,7e-17 antes, 0,75 depois); painel de início, `gotcha stats`/`list` do agent-harness passam a dizer «não medido» (10 testes, main + int97) | `correcoes/mem/mem.patch` (sha 9d87b6eb); `correcoes/gates-int97-c8c7/` |
| Integração dos patches Rust no int97 | **feita**: hk + qa + mem aplicados sem conflito; gate 1 = fmt, check do workspace, 2 clippy, build e testes de 11 crates (10.731 passed, 0 falhas) + `test_r3_a1_kpi.py` 19; gate 2 (C8/C7) = check, 2 clippy, intelligence 1.914 e hook-handlers 782 passed, fmt corrigido (1 import) | `scripts/gates_int97.sh`, `scripts/gates_int97_c8c7.sh` |
| `pre_tool_validator.rs` tratava o corpo de heredoc como comando (o `del` do Python negado como «Windows delete», achado na integração) | **corrigido no int97, gates verdes** (fmt, 2 clippy, hooks-core 540 e hook-handlers 782 passed) | `correcoes/val/val.patch` (sha 8a6e5b31) fundido em três vias com o `pub` que o int97 já tinha; 68/154 casos errados antes e 0 depois; mutantes 65/65; replay de 81.524 comandos reais: 23 falsos positivos removidos e 2 negações novas, que são `rm -f` verdadeiros que o lexer antigo escondia. Resíduos: a camada `bash_ast_validator` ainda recusa `rm -rf /` dentro de heredoc, e existem 3 predicados de «corpo de heredoc» (este, o `_shell_mask.py` e o `sem_corpos_de_heredoc` do `vgp_execucao.rs`), assunto do T10. Fmt do main: 2 arquivos do commit `255b8830` de outra sessão formatados |
| C13 `test.db` do doctest · C14 `serve --help` · C15 `resource_monitor: ""` | em curso: ecc-mem-eng (C13) e ecc-qa-eng (C14, C15) | — |
| D3 faixas no lock · estado versionado (172 arquivos) · D4 snapshots do KPI | D3 **feito** (36 testes, 2 mutantes mortos); `.gitignore` feito para os outros dois, retirada do índice em `scripts/no_commit_tirar_kpi_do_versionamento.sh` | — |
| Lacunas N1–N6 da rodada comparativa (mutação declarada sem execução, sem piso de cobertura, sem cron, cota fora da barra, 4 números de saúde, journals sem retenção) | sem dono | `COMPARATIVO.md` §4 |

Decididas pelo Gabriel em 17/10 («recomendações aprovadas», `/decision-canvas`): D1 assinatura de tag continua aviso,
com rotação da `GEMINI_API_KEY` e assinatura obrigatória antes do 1º release para terceiros; D2 bloquear o contorno de
hooks só onde há hook ativo (feito, acima); D3 faixas no lock (feito); D4 `.gitignore` + snapshots fora do índice, sem
reescrever o histórico. Seguem com ele: o texto da classe nova na REGRA #11 e o `contents: write` do job `gates`
(justificado pelo push do auto-sync).

## 9. Rodada H (aprofundamento, 17–18/10/2026)

Resultado completo em `COMPARATIVO.md` §9, `COMPARATIVO-matriz.md` (292 linhas, 256 vigentes depois de 42 revisões) e
`ALVO-ECC.md` (70 itens acionáveis e 38 defeitos citados, 8 reconferidos). Juiz de cobertura `scripts/cobertura_ecc.py`:
L0 ∪ L1 89,9% (rc=0), contra 74,6% antes. O plano revisto está em `DECISOES-E-PLANO-ULTRAPASSAR-ECC.md` Parte 3, com o
tema novo T10 (integridade do juiz) na onda 1.
