---
type: Strategy
title: "Estratégia — /tmp como sintoma: contenção do trabalho efêmero e afordância do portfólio"
description: "Diagnóstico medido (censo /tmp, transcripts 14d, CEG, harness) da percepção de Gabriel sobre code mode/sandbox/portfólio, e decision-canvas com as alternativas em 3 camadas (substrato, executor, biblioteca)."
plan_id: 2026-09-02-tmp-sandbox-portfolio-afordancia
okf_version: 0.1
tags: [strategy, code-mode, sandbox, portfolio, harness, tmp]
timestamp: 2026-09-02T04:55:00-03:00
---

# Estratégia — /tmp como sintoma

Parte do [bundle](/index.md). Evidência bruta: `/diagnostics/census-2026-09-02.json`
(censo do `/tmp`, DBs, journal, CEG) e o censo dos transcripts (script
`transcript_census.py`, 233 transcripts / 151 sessões com tools, 14 dias).

## 1. Veredito sobre a percepção

| Hipótese de Gabriel | Veredito | Evidência decisiva |
|---|---|---|
| "Há muito sendo escrito no /tmp que deveria estar no sandbox" | **Parcialmente confirmada** | A rajada de 8 GiB foi UM processo fora do sandbox (`soffice.bin` via pytest nativo do `analise`). Em repouso, harness + Touring ocupam ~26 MB do `/tmp`. Mas outra sessão clonou um repo de 566 MB / 13.256 arquivos em `/tmp` (workspace efêmero em tmpfs), e o próprio CEG declara `TMPDIR=/tmp` e concede `/tmp` inteiro como write root compartilhado. |
| "O sandbox/code mode não está funcionando como deveria" | **Refutada no ponto, confirmada na borda** | O CEG já aplica `RLIMIT_FSIZE = 256 MiB` por arquivo (+ CPU, NOFILE, AS 80% RAM), herdado pelos filhos: o mesmo `soffice` DENTRO de `touring run` morreria em 256 MiB. A rota que falhou foi a nativa. A borda: sem tmp privado por run, sem limpeza, sem teto agregado — N × 256 MiB ainda esgotam a cota. |
| "O harness não está funcionando como deveria" | **Confirmada** | Scratchpad e captura de saída do Bash vivem em `/tmp/claude-1000` (tmpfs com `usrquota` de 8 GiB **por usuário**). Um runaway de qualquer sessão mata o Bash de TODAS as sessões, com `exit 1` mudo. Blast radius = usuário, não sessão; falha que não ensina (A5). |
| "A infraestrutura do portfólio (blocos pré-moldados) não está sendo efetiva" | **Confirmada com força** | 14 dias: **560** scripts nasceram no scratchpad, **461 (82%) rodaram ≤ 1 vez**, 51 (9%) ≥ 3 vezes; `--harvest` **29×** (5% dos scripts, 0,66% dos 4.383 `touring run`); `touring portfolio` **40×** (1 consulta a cada 14 scripts novos). Clusters de nome (gravar 12 · probe 8 · recon 7 · medir 6 · canvas 6 · patch 6) = o mesmo propósito reinventado com alvo diferente. |

## 2. Evidência

### 2.1 O incidente (fechado na origem)

- `/tmp` é tmpfs de 32 GiB (RAM) com `usrquota`; cota do usuário 8 GiB. Consumidor: `soffice.bin` PID 1674427, `--headless --convert-to txt`, temporário `lo-numeracao-*/lu*.tmp` de 2,1 → 10,3 GB em minutos.
- Origem: `~/projects/analise/packages/kazuba-converters/src/kazuba_converters/core/extractors/docx_numeracao.py` — o arquivo **já documenta o incidente de 02/09** e traz a correção: `soffice` é um lançador, `subprocess.run(timeout=)` matava só o pai; agora o **grupo** recebe SIGKILL no timeout e perfil + saída ficam num `TemporaryDirectory(prefix="lo-numeracao-")`. Pytest do `analise` tem `timeout` configurado, sem `basetemp`/`tmp_path_retention`.
- A cota **funcionou**: bloqueou em 8 GiB um arquivo que, em tmpfs, consumia RAM. O que falhou foi o blast radius (todas as sessões) e a opacidade (Bash morto sem mensagem).

### 2.2 Como o agente executa (transcripts, 14 dias)

| Métrica | Valor |
|---|---|
| Bash total / Edit / Read / Write | 21.705 / 4.610 / 2.493 / 1.773 |
| `touring run` (sandbox) | 4.383 = **20,2%** do Bash |
| Python nativo (inline `-c`/heredoc + arquivo no scratchpad) | 4.275 (**≈ 1:1** com o sandbox) |
| Inspeção crua (grep/cat/ls/find/sed -n…) | 6.488 (29,9%) |
| Denies/avisos de gate | 2.734 (**12,6%** do Bash): G3 513 · G10 402 · G7 389 · rajada 368 · P0 26 · CEG 901 (inclui advisories waived) |
| Scratch scripts escritos / rodaram ≤1× / ≥3× | 560 / 461 / 51 |
| `--harvest` / `touring portfolio` / `memory store` / `memory recall` | 29 / 40 / 400 / 272 |
| Maior sessão (analise) | 149 scripts escritos, 115 rodaram ≤1×, 264 denies |

### 2.3 O que o executor contém e o que não contém

| Camada | Estado medido |
|---|---|
| CEG rlimit (sempre on) | CPU 30 s (2× wall), NOFILE 256, **FSIZE 256 MiB**, AS 80% RAM (SEG-1); NPROC não (por UID). |
| CEG tmp | `cmd.env("TMPDIR", "/tmp")` (QW-1 27/08) — `/tmp` compartilhado é o write root do Landlock; `tempfile.gettempdir()` dentro do sandbox = `/tmp`. Staging default `/tmp/touring-staging`. Sem tmp privado por run, sem limpeza, sem `tmp_bytes` no journal. |
| run_journal (M1) | 6.435 runs; chaves: `bytes_elided, code_hash_stdout_bytes, duration_ms, exit_code, failure_kind, language, ts, run_id`. **Não registra** `--file`/inline, `--harvest`, `--brief`, `--orchestrate` → a régua é cega ao reuso. |
| Escada de trust | `snippet_stats` = 15 linhas (existe; colheita é manual). |
| Índice | 376 + 865 símbolos e 134 + 249 `file_knowledge` com caminhos `/tmp/…` (scratch indexado como se fosse projeto; o rebuild purgou 27 stale). |
| Gate P0 F2.1 (python) | `os.walk(onerror=…)` lido como XSS (CWE-79) — falso positivo bloqueou um script legítimo 1× nesta sessão. |

### 2.4 Lente externa (marcada no ledger CCE)

- **Isolamento por execução, não por usuário**: Bazel/Nix (TMPDIR por ação, apagado ao fim), systemd `PrivateTmp=`/`TemporaryFileSystem=/tmp:size=`, E2B/Modal/Daytona e o sandbox do *programmatic tool calling* da Anthropic (container/microVM por sessão com disco limitado e TTL), Cloudflare Code Mode (isolate sem FS compartilhado). Regra comum: **teto agregado + TTL + limpeza automática + blast radius = a execução**.
- **Contenção no substrato primeiro** (M3): `ulimit -f`/`RLIMIT_FSIZE` herdado pela árvore inteira é o controle mais barato; cota por usuário é a última linha.
- **Reuso como cache endereçado** (Bazel action cache): um script vira "bloco pré-moldado" quando tem endereço estável, parâmetros declarados e histórico de execução. O portfólio já tem o histórico (escada); falta o endereço (tmpfs apaga) e a convenção de parâmetros.
- pytest (Context7): `tmp_path_retention_count`/`tmp_path_retention_policy` e `--basetemp` controlam onde e quantos temporários de teste ficam.

## 3. Diagnóstico por camada (causas-raiz)

1. **Substrato**: o único teto era a cota por usuário; nada herdável por árvore de processos (`ulimit -f`), nada por sessão. O tmpfs coloca temporários na RAM.
2. **Executor (CEG)**: contenção per-arquivo correta; isolamento e agregado ausentes por decisão QW-1 (`TMPDIR=/tmp`) tomada para "declarar um write root que o Landlock já concede" — resolveu um sintoma (ferramentas sem TMPDIR) abrindo o compartilhamento.
3. **Executor (harness Claude Code)**: scratchpad e captura em tmpfs cotado por usuário; falha muda. Fora do nosso código. Resposta do guia oficial (docs `sandboxing.md`, `tools-reference.md`): (a) a localização do scratchpad **não é documentada** e não há setting para realocá-la; `TMPDIR` no bloco `env` do `settings.json` só alcança processos filhos, não a captura do próprio harness — realocar via `TMPDIR` do processo que lança o `claude` é hipótese a provar empiricamente; (b) sob cota esgotada **não há** tratamento, retry ou opção de desligar a captura documentados (só o teto de 5 GB de saída e truncagem em 64 MiB); (c) o **sandbox mode do próprio Claude Code** dá um diretório temporário **privado por sessão** via `$TMPDIR`, com `sandbox.filesystem.allowWrite/denyWrite`, sem limite de tamanho documentado.
4. **Biblioteca (portfólio)**: a afordância existe (`portfolio`, `--harvest`, escada, prior-art no pre-write) mas está **fora do caminho crítico**: o scratch é irrecuperável por construção, a colheita é manual, o prior-art casou por string de tags (devolveu `cofre.py`), o remédio do G10 nomeia `touring run --file` e não a colheita. O caminho de menor resistência é escrever de novo — e o agente escreve de novo 560 vezes em 14 dias.
5. **Medição**: M1 não enxerga reuso; sem `reuse_ratio` nenhuma das 3 camadas pode ser avaliada por código (Lei L2).

## 4. Decision canvas

**§1 Decisão.** Escolher como conter o trabalho efêmero das sessões (temporários, scratch, clones) e como tornar o reuso de blocos o caminho de menor resistência, em vez do scratchpad descartável.

**§2 Contexto.** Disparado pela exaustão da cota do `/tmp` em 02/09 (um `soffice.bin` de 10 GB, fora do sandbox), que matou o Bash de todas as sessões. A causa local já foi corrigida no `analise`; a camada sistêmica segue igual: sem tmp por execução, sem teto herdável, 82% dos scripts nascendo para morrer. Sem decisão, o próximo runaway repete o blast radius e o retrabalho continua invisível à régua.

**§3 Opções.**

- **A — Substrato (OS)**: lançar o Claude Code com `TMPDIR` por sessão fora do tmpfs (btrfs, 369 GB livres) + `ulimit -f 1 GiB` no launcher (herdado por toda a árvore, LibreOffice incluído) + aging de `/tmp/claude-1000` e de clones em `/tmp` (tmpfiles) + manter a cota de 8 GiB como última linha. Fonte: systemd/SRE (blast radius), M3.
- **B — Executor (Touring/CEG)**: tmp privado por run (`TMPDIR` próprio, Landlock write root = ele + workspace, removido ao fim, `tmp_bytes` no journal) + journal com `source/file/harvest/brief/orchestrate` + correção do falso positivo F2.1 `onerror=` + reportar à Anthropic o `exit 1` mudo do Bash sob EDQUOT. Fonte: Bazel action tmp, D8.
- **C — Biblioteca (portfólio)**: auto-colheita provisional de todo scratch script com exit 0 ≥ 2× (a escada já existe; falta o gatilho), scratch persistente e indexado (`.touring/scratch/<sessão>/`, git-ignored) em vez de tmpfs, prior-art no pre-write casando pela docstring/intenção, remédio do G10 nomeando a colheita, KPI novo `reuse_ratio`. Fonte: Bazel action cache, A13.
- **D — Só o fix local + apertar gates**: manter a correção do `analise`, endurecer G10/rajada. Fonte: inércia. Rejeitada: já há 12,6% de denies e a persuasão/punição foi medida sem efeito em `U(a)`; não muda blast radius nem reuso.

**§4 Trade-offs.**

| Opção | Prós | Contras | Custo |
|---|---|---|---|
| A | Determinística, 1 h, zero código Rust, teria parado o incidente em 1 GiB; variante A′ (sandbox mode do CC) dá tmp privado por sessão de forma documentada | Realocação do scratchpad via `TMPDIR` não é documentada (provar empiricamente); fora do tmpfs perde a limpeza por reboot (exige aging); `ulimit -f` pode matar builds legítimos > 1 GiB (raro; medir); A′ pode conflitar com socket do daemon/hooks | baixo |
| B | Fecha o compartilhamento do `/tmp` no lugar certo (executor); torna reuso mensurável | 1-2 dias sob TDD; toca Landlock/env do sandbox (QW-1 volta a ser discutida) | médio |
| C | Ataca a causa do retrabalho; usa infra já existente (escada, portfólio) | Mais lento a mostrar efeito; risco de colher lixo (mitigado pela escada + exit 0 ≥ 2×) | médio-alto |
| D | Nada a fazer | Repete o incidente; aumenta fricção | zero (falso) |

**§5 Stakeholders.** Gabriel (decide; veto em qualquer mudança de `settings.json`/launcher). Sessões concorrentes do Claude Code (todas ganham isolamento). Projeto `analise` (dono do fix local, já feito). Daemon Touring (camada B muda o contrato do sandbox — provar por comportamento, não por versão). Anthropic (harness: relatório de bug).

**§6 Riscos (da recomendação A → B → C).** (1) `ulimit -f` alto demais não contém, baixo demais quebra `cargo build` — mitigar medindo o maior artefato de `target/` antes de fixar. (2) tmp privado por run pode quebrar ferramentas que esperam `/tmp` global (LibreOffice, sockets) — mitigar com allowlist de exceções + gate 5.5 comportamental. (3) Auto-colheita polui o portfólio — mitigar exigindo exit 0 ≥ 2× e docstring; o portfólio já sabe descartar por `when_not_to_use`.

**§7 Reversibilidade.** A: total, minutos (remover env/ulimit do launcher). B: alta (feature flag + rollback de toolchain). C: alta (colheita provisional é marcada; `prune` existe). Janela: imediata.

**§8 Recomendação.** A imediatamente, B em seguida, C como wave própria com KPI — porque A é o único controle que teria parado o incidente sem código (§4, custo baixo), B move a regra para o executor (D8) e é o que torna C mensurável (§3), e C é onde mora a tese mais forte de Gabriel (82% de scripts de uso único). Confiança 0,8.

**§9 Perguntas abertas.** (a) O scratchpad do Claude Code honra `TMPDIR` do processo que o lança? A documentação não diz (guia consultado: localização não documentada, sem setting de realocação); a prova é empírica: lançar uma instância com `TMPDIR=<dir>` e conferir o caminho do scratchpad no system prompt. Alternativa documentada: o sandbox mode do Claude Code (tmp privado por sessão, `sandbox.filesystem.*`) — verificar compatibilidade com o socket do daemon Touring e os hooks. (b) Qual o maior arquivo que `cargo build --release` produz aqui? (fixa o `ulimit -f`). (c) Após C, `reuse_ratio` ≥ 20% em 2 semanas? Se não, o problema é de **descoberta** (busca por intenção), não de persistência. (d) Quantos dos 560 scripts casariam com scripts já existentes em `~/.claude/skills/*/scripts/`? (mede reinvenção contra a biblioteca canônica, não só entre si).

## 5. Próximos passos (após decisão de Gabriel)

1. A: patch no launcher (`TMPDIR`, `ulimit -f`), tmpfiles aging, prova: runaway sintético de 2 GiB morre em 1 GiB sem afetar outra sessão.
2. B: DAG própria sob TDD — `sandbox_executor` tmp por run, journal v2, F2.1 fix, gate 5.5 estendido.
3. C: DAG própria — auto-harvest no `post_bash`/`run` success, scratch persistente, prior-art por docstring, KPI `reuse_ratio`.
4. Reportar à Anthropic: Bash `exit 1` mudo sob EDQUOT no diretório de captura.

## 6. Artefatos

- `/diagnostics/touring-20260902T041748.md` (diagnóstico OKF) · `/diagnostics/census-2026-09-02.json` (censo) · ledger CCE `.touring-explore/afordancia-sandbox-portfolio-tmp-code-mode-harne.ledger.json` (convergido, lente externa visitada).
- Memórias: `censo:tmp-sandbox-portfolio:2026-09-02`, `strategy:tmp-sandbox-portfolio-afordancia:2026-09-02`.
