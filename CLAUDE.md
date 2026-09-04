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
   crates tocados + `touring e2e -j` (baseline composite 0.8749) antes de declarar pronto.
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
    regenera `touring`/`touring-daemon`, que vêm de `touring-server`, e o daemon embute
    o CLI por linkagem estática carregando handlers uma vez no boot. Depois de editar um
    handler RPC: `cargo build -p touring-server --release` + `update-touring`. A
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

## Referências

- Instruções do crate principal: `crates/touring-server/.claude/CLAUDE.md`
- Instruções do crate de código inteligente: `crates/touring-code/.claude/CLAUDE.md`
- Instruções do crate de qualidade: `crates/touring-quality/.claude/CLAUDE.md`
- Instruções do crate CLI: `crates/touring-cli/.claude/CLAUDE.md`
- Programa de produtização: `docs/plans/touring-productization-pln2/00-INDEX.md`
- Bundle code-mode-sinal F1-F6: `docs/plans/2026-08-31-code-mode-sinal/` (artefatos: phases/, knowledge/, sdksignal-report-f*.json)
- Estratégia canônica do bundle: `docs/plans/2026-08-31-code-mode-sinal/strategy-2026-08-31-yetzirah-v1.1.md`
- Guia da biblioteca de hashtags (facetas, codetags, MOCs): `docs/memory-hashtag-library.md`
- Constituição TACO global: `~/.claude/CLAUDE.md` (autoridade: Gabriel)
