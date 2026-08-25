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

8. **Portfólio de fluxos ADW (v30.4.1, 19/08/2026)**: fluxos são **compostos**, não
   copiados. `[[use]] module/as/with` faz inlining de um fragmento sob namespace
   (`recall.memory`); o kit tem 11 peças (`recall-pack · prior-art · diagnose-pack ·
   fanout-lenses · gate-rust · gate-quality50 · conflict-guard · human-approve ·
   phase-close · converge · critic-panel`). Todo fluxo publicado declara `[purpose]`
   com `when_not_to_use` — é o campo que deixa o portfólio **descartar** um fluxo em
   vez de só recomendar o mais próximo, e o minerador o indexa **antes** do cabeçalho
   para o teto de 600 chars cair no boilerplate. Fan-out `parallel` tem forma estática
   (`branches = [...]`) e **dinâmica** (`branches = "{{vars.x}}"` + `template`, clonado
   por valor em runtime — o `Send` do LangGraph); `max_branches` é verificado **antes**
   de rodar qualquer ramo. `on_branch_fail`: `all|any|ignore|best_effort|quorum:N`.
   Criar: `touring adw new` (prior-art com veredito obrigatório) · inspecionar:
   `touring adw explain` (grafo plano) · `touring adw fragments`. Bundle:
   `docs/plans/2026-08-18-graph-engineering-flow-portfolio/`.

9. **Reivindicação atômica + Wayfinder no `decompose` (v30.4.1)**: `touring decompose
   claim <task> --owner <id> [--lease-secs N]` / `release` — `ready` só LÊ, então duas
   sessões que o consultam recebem o MESMO subtask; `claim` é um UPDATE condicional e
   exatamente uma vence. `touring decompose ticket … --kind decision|implementation
   --fog clear|hazy|unknown --origin-ticket <id>` e `frontier <task>`: decisões abertas
   **bloqueiam** a fronteira de implementação, e névoa não avaliada reporta `unknown`,
   nunca `clear`.

10. **Code mode ATIVO neste workspace (v30.4.14+, 25/08)**: `.touring/touring.toml`
    declara `[code_mode] mode = "code"` — este workspace é o **piloto** da apresentação
    por escopo. Efeito: inspeção `grep`/`cat`/`find` modelo-direta é **negada** com a
    rota derivada (que executa de verdade); `cat >` heredoc é escrita e passa; `ls`/`wc`/
    `sed-n` **isoladas** passam, mas em **rajada de turno** o T3-B funde qualquer classe
    de inspeção (a 1ª executa intacta, as K−1 voltam como 1 programa não-escrito).
    Resolução: prefixo `TOURING_CODE_MODE=<v>` no comando → env do hook → alias
    `TOURING_CODE_ONLY=1` → `touring.toml` → default `both`. Relaxar por-comando:
    prefixo `TOURING_CODE_MODE=native` (exportar no shell NÃO chega ao hook — processos
    irmãos). Kill switches humanos: `TOURING_CODE_GATES_DISABLED=1` (todos),
    `TOURING_T3_FUSE_DISABLED=1` (só a fusão). Estratégia por contexto completa:
    `~/.claude/skills/Touring/references/code-mode-operational.md`. **Propagação
    (regra 2 ampliada)**: o rótulo da toolchain NÃO prova o build — 24/08 o lock dizia
    `30.4.14` rodando binário de 24/08. Após propagar, a prova é SEMPRE comportamental
    (um deny T3/modo-code ao vivo no projeto alvo), nunca por versão.

## Referências

- Instruções do crate principal: `crates/touring-server/.claude/CLAUDE.md`
- Programa de produtização: `docs/plans/touring-productization-pln2/00-INDEX.md`
- Guia da biblioteca de hashtags (facetas, codetags, MOCs): `docs/memory-hashtag-library.md`
- Constituição TACO global: `~/.claude/CLAUDE.md` (autoridade: Gabriel)
