---
okf_version: "1.0"
type: CLAUDE
title: "Touring — Fonte Canônica (workspace do produto)"
description: "Regras e referências do workspace fonte do Touring (L1 da arquitetura rustup-like). Auto-carregado pelo harness CC ao abrir sessão neste diretório."
plan_id: docs
tags: [rules, workspace, rustup, per-project]
timestamp: 2026-08-20T11:15:00-03:00
---

# Touring — Fonte Canônica (workspace do produto)

> Este diretório é a **fonte canônica** do Touring (Pln2 produtização, movida de
> `~/.claude/rust` em 24/07/2026 — aquele root está CONGELADO, nunca desenvolver lá).
> Camada L1 da arquitetura rustup-like: L1 fonte → L2 `~/.touring/toolchains/<v>/`
> → L3 shim CC (`~/.claude/hooks/touring-hook`) → L4 `<projeto>/.touring/`.

## Regras deste workspace

1. **Rebuild SEMPRE via `update-touring`** — nunca `cargo build` standalone para deploy
   (pipeline kill→build→install→restart→verify; REGRA #19: daemon via `touring daemon-ctl`,
   jamais pkill). O daemon global roda `target/release/touring-daemon` DESTE workspace.
2. **`cargo build` aqui NÃO afeta projetos pinados** — toolchains instaladas
   (`~/.touring/toolchains/`) são cópias imutáveis. Propagar exige passo explícito:
   `touring toolchain install --from-source . <versão> --force` e, por projeto,
   `touring update --project <root>` (rollback: `touring update --rollback`).
2.1. **Propagação reprodutível em 1 comando (02/08/2026)**: `scripts/propagate-release.sh <versão>`
   encadeia gates → `update-touring` → `toolchain install` → `toolchain default` →
   `touring update` por projeto → verify. Rollback: `scripts/propagate-release.sh --rollback`.
   Flags: `--dry-run --skip-build --skip-gates --skip-freeze --no-default`.
   **Gotchas que o script encapsula** (ambos verificados por execução):
   (a) `touring update --all-projects` SEM canal resolve pelo lock de cada projeto —
   mantém a versão velha e NÃO propaga a nova; COM canal (`touring update <v> --all-projects`)
   arrasta o workspace FONTE de `dev` para `<v>`, quebrando o dev iterativo. Por isso o
   script itera projeto a projeto pulando a fonte.
   (b) `touring --version` escreve em **stderr**, não stdout — `2>/dev/null` apaga a
   versão e qualquer gate de verificação passa sem verificar nada.
   (c) o gate 5.5 (prova comportamental, 35 asserções) tem **retry-once** com
   re-espera do doctor (28/08): a estreia da prova reprovou por transiente — project
   actor drenando índice — e o operador leu EXIT=0 porque `| tail` engoliu o exit
   (`${PIPESTATUS[0]}` é a leitura correta; o script agora falha só após 2 tentativas).
   (d) o gate 1/6 roda o S1a do shadow r3 (`scripts/r3-s1a-gate`, 03/10/2026): o benchmark
   `r3_emission_cost_is_bounded` é `#[ignore]` e sem chamador ninguém o rodava. O gate faz
   **uma** re-medição (ruído térmico) e exige a linha `1 passed` — `cargo test -- --ignored
   <filtro>` sai 0 quando o filtro não casa nada, e um exit não-zero do `hw-slot` (70 =
   machine check) reprova mesmo com `1 passed` no texto. O limite é o do teste (E5.2).
   (e) o gate 1/6 roda os **testes dos crates tocados** e o clippy leva `--all-targets`
   (09/10/2026): a 30.4.95 saiu com um e2e quebrado (`cli_suggester_e2e`) e dois testes de
   `handlers/index.rs` que nunca compilaram, porque o gate só fazia `check` + clippy sem código
   de teste. Pacotes = `git diff <último commit "release …">..HEAD` ∪ `git status`, cada arquivo
   no `manifest_path` mais longo; um `cargo test -p <pkg> --no-fail-fast` por vez sob o `hw-slot`
   (o `touring-hook-handlers` leva `--features pre-hooks,post-hooks`, §13b). rc 75 = «sem vaga»
   (repete 1×, não é falha de teste); sem commit de release ou git falho = TODOS os pacotes
   (sinal ausente nunca estreita); `Cargo.toml`/`Cargo.lock` da raiz não mapeiam a pacote e só
   avisam. `--dry-run` lista pacotes e comandos sem executar; `--skip-gates` pula tudo.
   Teste: `scripts/test_propagate_release.py` (probes + mutantes sobre cópia do script).
   (f) o gate 1/6 constrói o release (`cargo build --release --workspace`) **antes** dos testes
   (18/10/2026): os e2e executam `target/release/touring`, e o build só no passo 2 fazia o gate
   medir o binário do release ANTERIOR (a 30.4.97 reprovou em `version_matches_crate_version`).
   Todo job do gate espera a vaga rust por `PROPAGATE_SLOT_WAIT` s (default 7200), e não pelos
   600 s do hw-slot, que reprovavam o gate «sem vaga» com um lote vizinho de 40 min.
2.2. **`update-touring` é versionado (18/08/2026)**: vive em `scripts/update-touring`
   e chega ao PATH por `~/.local/bin/update-touring`, um **symlink** — a mesma forma
   já usada para os binários. Antes existia só em `~/.local/bin/`: a ferramenta que
   faz todo deploy estava fora do controle de versão. Duas consequências que só
   apareceram ao versioná-la, ambas corrigidas: (a) a raiz do workspace tinha como
   default `~/.claude/rust`, a árvore **congelada** — de qualquer terminal sem
   `TOURING_WORKSPACE_ROOT` exportada, o script reconstruía e instalava a árvore
   errada em silêncio; hoje a raiz é derivada do **próprio caminho do script**;
   (b) todo check de processo perguntava "existe algum `touring-daemon`?", o que na
   topologia per-project casa os daemons de **outros** projetos — o fallback de kill
   chegava a `kill -9` neles (o cascading kill que a REGRA #19 existe para impedir),
   `start_daemon` nunca subia o daemon global, e `verify` reprovava por causa de
   processo alheio. Agora há um resolvedor único (`global_daemon_pid`) que identifica
   o **dono do socket global**. Guardado por `scripts/test_update_touring.py`
   (invariantes de conteúdo no CI; o symlink verificado no gate 1/6 do
   `propagate-release.sh`, onde o lado vivo existe).

3. **Gates (REGRA #21 — 0 falhas)**: `cargo check` + `clippy -D warnings` + testes dos
   crates tocados + `touring e2e -j` antes de declarar pronto (o nome engana: é análise composta de
   índice, AST, wiring, qualidade e aprendizado, não teste ponta a ponta; os e2e de verdade são os `cargo test`). **Baseline do e2e (mediana
   n=5, 02/10/2026): 0,857** (min 0,846 · max 0,857); o 0,8749 antigo foi um ponto único de
   24/07 (já estava abaixo em 25/07). Leia POR COMPONENTE, não pelo overall: ast 0,776 ·
   wiring 0,809 · index 0,852 · knowledge 0,900 · quality 1,000 são constantes entre
   execuções (regressão = algum cair); `learning` (0,686↔0,886) mede o bandit e é a única
   fonte de variação do overall — não gatear por ele. **04/10/2026: `index` 0,852 → 0,982**
   (cobertura 74,5% → 100%, uma medição; overall 0,887): o 0,852 era diluição, documentos
   gravados por script fora do caminho que reindexa, que agora passam por
   `okf_emit.index_written`, e 359 arquivos sem `file_knowledge`, preenchidos pelo
   `metadata-backfill` (cross-audit X14). Um `index` caindo de novo é escritor novo sem ingest.
   Validators por fase do programa: `docs/plans/touring-productization-pln2/validate_*.sh`.
4. **Débito conhecido**: grafo release-TEST do touring-server quebrado (E0460/E0463,
   fingerprints stale do move F4′) — grafo normal e debug-test compilam; harness E2E
   roda em debug invocando o binário release.
5. **Gotcha de sessão**: sessões CC exportam `TOURING_DAEMON_SOCKET` (precedência sobre
   o walk-up per-project). Para reproduzir o ambiente de um projeto pinado:
   `env -u TOURING_DAEMON_SOCKET -u TOURING_DAEMON_SOCK ...`.

6. **Espelho `client/` (18/08/2026)**: `client/{skills,rules,agents}` é a cópia
   versionada das skills que rodam em `~/.claude/`. **Direção única: o live é a
   fonte, `client/` é gerado.** Editar `client/` à mão não muda nada que executa e
   é desfeito pelo próximo sync. Operar: `python3 scripts/sync-client-skills.py
   --check | --apply [--prune]`. Nasceu de uma cópia em massa em 25/07 que ninguém
   comparava — foi assim que a correção de shell injection chegou ao espelho e às
   instanciações mas **nunca à biblioteca implantada** de onde `adw from-template`
   copia. Agora há três guardas: `--check` no `propagate-release.sh` (comparação
   completa, exige o lado vivo), `scripts/test_sync_client_skills.py` no CI
   (integridade contra `client/MANIFEST.sha256`, funciona sem o lado vivo) e
   exclusão estrutural de estado de runtime (`.claude/`, `__pycache__`, `*.db`).
   **Correção 19/08**: o espelho nunca adotou arquivo NOVO. `is_noise` recebia o
   caminho **absoluto**, que contém `~/.claude` — e `.claude` está na lista de
   ignorados — então todo arquivo do lado live era ruído e a varredura contribuía
   nada; só caminhos já conhecidos sincronizavam, e `--check` dizia CLEAN com
   arquivo faltando. O teste passava porque sua raiz falsa era um `tmp_path` sem
   `.claude`. `is_noise` agora **recusa** caminho absoluto — o que revelou um
   quarto sítio de chamada que a correção pontual não tinha achado.

7. **Hashtag library (v30.4.0, 12/08/2026)**: toda memória carrega facetas
   `#facet:value` (7 facetas: kind/purpose/lang/domain/process/artifact/status).
   Workflows: store auto-deriva + aceita `--tag` explícita; snippets em código
   levam codetag `// #tags: kind:… purpose:… domain:…` (post_write/edit indexam
   sozinhos; remover o codetag apaga a memória); consulta por
   `memory query "#kind:… #lang:…"`, `recall "<texto> #faceta"`,
   `memory moc <tópico>` (mapa emergente) e `portfolio "<intento> #kind:…"`.
   Guia canônico: `docs/memory-hashtag-library.md`.

8. **Portfólio de fluxos ADW**: fluxos são **compostos**, não copiados —
   `[[use]] module/as/with` faz inlining de um fragmento sob namespace. Todo fluxo
   publicado declara `[purpose]` com `when_not_to_use`: é esse campo que deixa o
   portfólio **descartar** um fluxo em vez de recomendar o mais próximo. Fan-out
   `parallel` tem forma estática e dinâmica; `max_branches` é verificado **antes** de
   rodar qualquer ramo. Criar: `touring adw new` (prior-art com veredito obrigatório) ·
   inspecionar: `touring adw explain` (grafo plano) · `touring adw fragments`.
   **Três gotchas pagos por estreias**: (a) agente headless herda os hooks da sessão —
   `_agent_claude` spawna com `TOURING_WORK_OUTER_DISABLED=1`; (b) gate mudo degrada o
   retry, então a REASON de um gate ENSINA a correção; (c) **texto de agente nunca entra
   no sandbox** — o X6 classifica o programa inteiro e prosa com cara de comando vira
   deny não-determinístico. Histórico das waves e a library completa:
   `docs/plans/2026-08-{18,28}-*/`.
9. **Reivindicação atômica + Wayfinder no `decompose` (v30.4.1)**: `touring decompose
   claim <task> --owner <id> [--lease-secs N]` / `release` — `ready` só LÊ, então duas
   sessões que o consultam recebem o MESMO subtask; `claim` é um UPDATE condicional e
   exatamente uma vence. `touring decompose ticket … --kind decision|implementation
   --fog clear|hazy|unknown --origin-ticket <id>` e `frontier <task>`: decisões abertas
   **bloqueiam** a fronteira de implementação, e névoa não avaliada reporta `unknown`,
   nunca `clear`.

10. **Code mode ATIVO neste workspace**: `.touring/touring.toml` declara
    `[code_mode] mode = "code"`. Inspeção **isolada passa**; o que colapsa é a
    **rajada**, e a chamada negada traz o programa fundido pronto.

    | Gate | Nega em | Janela |
    |---|---|---|
    | rajada de inspeção (`grep`/`cat`/`find`/`ls`/`wc`/`sed-n`/python-inline read-only) | 2ª | 300s |
    | G1 rajada · G7 re-inspeção | 3ª | 300s |
    | G10 exec-burst | 5ª | 600s |
    | par write→run · G3 edit-sem-read | 2º | 600s |
    | G11 orçamento de bypass | 2º seguido · 3º sem a rota | 600s |

    Um `touring run` zera a janela. Precedência: prefixo `TOURING_CODE_MODE=<v>` no
    PRÓPRIO comando (exportar no shell não chega ao hook) → `touring.toml` → default
    `both`. Kill switch humano: `TOURING_CODE_GATES_DISABLED=1`. Um escopo que declara
    `code` também estreita o **anúncio** MCP para a fachada search+execute; todo tool
    escondido segue invocável por nome. Estratégia por contexto, calibração e histórico:
    `~/.claude/skills/Touring/references/code-mode-operational.md`.
11. **Diretrizes de elaboração de código E/A/M (28/08/2026)**: como ESCOLHER a rota
    (E1-E6: loop/agregação/≥3 fatos → programa; colapso no executor, D8), ACERTAR o
    programa (A1-A14: SDK plana, JSON tipado, erros que ensinam, output-limit explícito,
    `touring.parallel` pool 10, prompt byte-estável, **cap 3 retries — na 4ª muda de
    estratégia**, segredos nunca no sandbox, escada de trust) e MEDIR (M1-M3: `touring
    kpi -j` → `code_mode_adherence`, piso 0.8). Corpo: `docs/code-mode.md` §Diretrizes +
    rule auto-load `~/.claude/rules/code-elaboration-directives.md`. Enforcement:
    `BestPracticesGate` (`crates/touring-quality/src/builtins/best_practices.rs`) verifica
    declaração + aderência M1.

12. **Rebuild parcial não atualiza o binário**: `cargo build -p touring-cli` NÃO
    regenera `touring` (pacote `touring-server`) nem `touring-daemon` (pacote
    `touring-hooks`), e o daemon embute o CLI por linkagem estática carregando handlers
    uma vez no boot. Depois de editar um handler RPC: `cargo build --release --workspace`
    + `update-touring --no-build` (ou `update-touring` completo, que faz o mesmo build).
    `-p touring-server` não recompila o daemon e grava o `touring` com outra unificação de
    features: em 04/10/2026 saiu 1,4 MB menor com a mesma lista de 26 features no
    `--version`, que só lista as do pacote. A
    superfície SDK do `--orchestrate` e sua entrega estão em
    `docs/plans/2026-08-31-code-mode-sinal/`.
13. **Hooks: quatro coisas que custam horas**. (a) O campo `if` de um hook aceita
    **uma** regra de permissão, sem operadores lógicos, e falha ABERTO em comando não
    parseável: um valor com `|` nunca casa comando simples e dispara por acaso em
    heredoc. Um `if` por regra, e filtro por conteúdo dentro do handler. (b) `cargo test
    -p touring-hook-handlers` exige `--features pre-hooks,post-hooks`; sem elas os
    handlers nem compilam, e `clippy` com features parciais inventa dead code que
    `--all-features` não vê. (c) Edits aplicados por script, fora da ferramenta de
    edição, não passam pelo hook de reindex, e o juiz de convergência acusa órfãos
    falsos até `touring index rebuild`. (d) A prova de que um hook roda é o trace:
    `TOURING_HOOK_TRACE_FILE` grava uma linha JSON por invocação. Estado atual:
    `pre-bash` e `post-bash` sem `if`, logo o gate do `pre-bash` alcança todo Bash.
    Narrativa das waves: `docs/plans/2026-08-31-complementacao-hooks/`.
14. **Trabalho efêmero é contido pelo executor, não pela disciplina**: cada `touring run`
    recebe um tmp PRIVADO (`/tmp/touring-run-*`) — `TMPDIR` do filho aponta para ele, o
    Landlock troca `/tmp`+`/var/tmp` por esse dir, e o que sobra volta em `tmp_bytes`
    (`TOURING_RUN_KEEP_TMP=1` preserva para autópsia). **Gotcha ao mexer no CEG**: rulesets
    do Landlock EMPILHAM por interseção — toda camada que adiciona um ruleset precisa
    receber os grants das anteriores (`SandboxConfig.extra_write_roots`), senão "conceder"
    vira "negar" em silêncio. Reuso de blocos prontos é medido, não presumido:
    `touring kpi -j` → `code_mode_reuse` (piso 0.20). Racional, waves e evidência:
    `docs/plans/2026-09-02-tmp-sandbox-portfolio-afordancia/`.

> 📜 **Histórico de rodadas/releases** (as entradas datadas de 14–22/09/2026:
> rodadas 8/9, cross-audits, e os releases da segunda quinzena de setembro)
> movido para `CHANGELOG.md` em 08/10/2026 (decisão de Gabriel — Regra #16,
> este arquivo estava em 555 linhas contra o teto de 400).

15. **Hardware compartilhado (02/10/2026)**: i9-14900HX com as CPUs 8-11 OFFLINE e 6+ sessões
    na mesma máquina. Todo job pesado passa por `hw-slot` (`scripts/hw-slot`, semáforo flock
    imposto pelo kernel): `hw-slot --class rust -- cargo test …` (1 por máquina; exit 70 se
    houve machine check durante um job verde) ou `hw-slot -- <pytest cheio|juiz|OCR>` (até 2);
    `hw-slot --status` antes de lançar qualquer coisa fora do slot. Nunca MemoryHigh/Max em
    unit (o oomd mata com RAM livre) e nunca pkill (REGRA #19). Desenho, mutantes e fatos
    medidos: memória `hw-slot-semaforo-de-hardware`.

16. **Gate de integridade da prosa de saída (04/10/2026, em SOMBRA)**: hook PreToolUse
    `~/.claude/skills/Touring/scripts/hooks/prose_integrity_gate.py` em `SendMessage|Write|Edit|
    MultiEdit|NotebookEdit` acusa caractere CJK/kana/hangul/cirílico/árabe/hebraico/devanágari/largura
    total no texto que sai (grego e tipografia não), e em `Write`/`Edit` só o que o texto novo
    INTRODUZ. Sombra = registra em `~/.claude/touring/prose_integrity/events.jsonl` e avisa; nunca
    bloqueia. `TOURING_PROSE_GATE_MODE=block` bloqueia só o `SendMessage` (crases são o escape), e
    só depois que `--criterion` der `promote: true` (n ≥ 30, Wilson-95 LEGIT ≤ 0,20; o replay de
    04/10 deu 25 e 0,391). Kill switch humano `TOURING_PROSE_GATE_DISABLED=1`. A resposta no chat e
    a classe lexical (fragmento em inglês) ficam fora do alcance. Pré-registro, rótulos e resultados:
    `docs/plans/2026-10-04-integridade-prosa/`.

17. **Economia de créditos e contexto — toda tarefa é dividida antes de executada (08/10/2026,
    diretriz de Gabriel)**. A franquia Anthropic (5 h + **semanal comum a todos os modelos**) paga
    também todo subagente, workflow, loop e mensagem entre sessões — um fanout grande esgota a
    semana; e o contexto desta sessão é reenviado inteiro a cada requisição. Cada parte desce ao
    executor **mais barato que ainda passa no juiz**: determinístico → código (`touring run`) ·
    busca/extração/classificação → subagente `model: haiku` · implementar com spec, testes,
    revisão → `model: sonnet` · decidir/sintetizar/julgar → esta sessão (Opus 5.5; suba o effort
    antes de trocar de modelo, e nunca `/model` no meio — cada modelo tem o próprio cache) ·
    ambiguidade longa em que o Opus em effort alto falhou → Fable 5.1, e em headless (`claude -p`,
    SDK sem prompt) só com o sim do Gabriel, porque ali cobra créditos sem perguntar · corpus longo,
    lote extenso, 2º leitor → sessão irmã Kimi K3 1M / MiniMax M3.1-Flash-Preview 1M (cota fora da
    Anthropic; quem é quem por código: `python3 ~/.claude/tools/sessoes_irmas.py --text`; um brief
    com arquivos exclusivos e aceite escrito antes, `notify_when_idle`, um relatório, e a entrega é
    hipótese até o seu juiz). **Numa sessão irmã** (backend Kimi/MiniMax) esta matriz não se
    aplica: ela e seus subagentes ficam no próprio modelo, sem rotear para opus/sonnet/haiku/fable
    (decisão de Gabriel, 08/10). Modelo novo ou desconhecido → doc oficial antes de rotear. Executor:
    hook `agent_model_gate.py` recusa `Agent` sem `model`. Corpo: `docs/economia-de-creditos-e-contexto.md`.

## Referências

- Instruções do crate principal: `crates/touring-server/.claude/CLAUDE.md`
- Instruções do crate de código inteligente: `crates/touring-code/.claude/CLAUDE.md`
- Instruções do crate de qualidade: `crates/touring-quality/.claude/CLAUDE.md`
- Instruções do crate CLI: `crates/touring-cli/.claude/CLAUDE.md`
- Programa de produtização: `docs/plans/touring-productization-pln2/00-INDEX.md`
- Bundle code-mode-sinal F1-F6: `docs/plans/2026-08-31-code-mode-sinal/` (artefatos: phases/, knowledge/, sdksignal-report-f*.json)
- Estratégia canônica do bundle: `docs/plans/2026-08-31-code-mode-sinal/strategy-2026-08-31-yetzirah-v1.1.md`
- Guia da biblioteca de hashtags (facetas, codetags, MOCs): `docs/memory-hashtag-library.md`
- Economia de créditos e contexto (roteamento de modelos, sessões irmãs Kimi/MiniMax, atualização de modelos): `docs/economia-de-creditos-e-contexto.md`
- Constituição TACO global: `~/.claude/CLAUDE.md` (autoridade: Gabriel)
