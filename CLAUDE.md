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

15. **Rodada 8 (14/09/2026), quatro contratos novos**. (a) O ator do projeto cede a vez
    entre arquivos do rebuild (`touring_hook_runtime::actor_yield`): memória e `index
    status` respondem durante `index rebuild`; um hook novo que só lê memória entra em
    `touring_dispatch::daemon::may_run_during_heavy`, qualquer outro espera. (b) Produtores de wiring têm fonte
    única, `wiring::refresh_file_producers` (conteúdo + visibilidade real): nunca limpar
    produtores a partir do `symbols_json`, que não guarda visibilidade — isso zerava os
    produtores de todo arquivo editado. (c) `third_party/tree-sitter-md` é patch com prova
    ASan no corpus do analise, e `third_party/tree-sitter-bash` troca `isdigit` sobre
    codepoint por dígito ASCII (SIGSEGV provado 3/3); trocar de versão exige repetir a
    varredura (`PATCHES.md`), e a guarda de fonte reprova classificador `<ctype.h>` nos
    scanners vendorizados e confere que o patch é o que compila. Código vendorizado
    fica fora do índice (`exclude_dirs`) e do corpus de ESTILO da nota 50-dim (o parser
    gerado tem 68% de clones e bloqueava o F1.3), mas as dimensões de segurança
    (F2.1/F2.4/F2.6) o leem: é código compilado no binário (cross-audit 14/09, D3). (d) `index rebuild --dir` fora do projeto é recusado
    e walk de subdiretório não sela a geração (`generation.state = "scoped"`).

16. **Rodada 9 (14/09/2026), três decisões aprovadas por Gabriel e um follow-up**. (a) Raiz
    companheira é busca, nunca wiring: `touring_foundation::config::is_companion_key`
    é o predicado único, e o portão de escrita do `touring-storage` recusa produtor,
    consumidor e import não resolvido com chave `@companion/`. No teste do rebuild,
    só `wiring_entries` distingue: a varredura de fantasmas apaga as linhas no fim do
    walk de qualquer jeito. (b) O daemon do hook e do `daemon-ctl` nasce por
    `touring_foundation::daemon_spawn`, e o do `update-touring` por
    `launch_daemon_and_wait`: sessão própria (`setsid`) e, com o manager de usuário do
    systemd, scope próprio (`systemd-run --user --scope --expand-environment=no`). Um
    processo herda o cgroup do pai, e o fim de uma unit oneshot matava o daemon que ela
    subiu. Sem manager, ou com `TOURING_DAEMON_SCOPE=0|false|off`, o spawn é direto; o
    fallback direto só roda quando o launcher morre antes do socket aceitar conexão. O
    daemon privado do juiz fica de fora e é parado ao fim da cláusula cargo. (c) `index
    status` é respondido no despacho, antes da fila do ator, lendo o estado
    commitado com conexões só leitura (`index_status_from_disk`). Só um projeto
    sem banco ainda cai no ator. (d) `touring-quality` no PATH é o wrapper
    `scripts/touring-quality-score` (cache por hash de motor+alvo, lock único),
    alcançado por symlink como o `update-touring`; motor derivado do checkout,
    lock com espera (`TOURING_QUALITY_LOCK_WAIT_SECS`, exit 75 ao estourar),
    nunca cacheia vazio nem falha. Guardado por `scripts/test_touring_quality_score.py`
    (CI + gate 1/6 do `propagate-release.sh`).

17. **Cross-audit 14/09/2026 (`docs/audits/cross-audit-2026-09-14.md`)**. (a) Leitura
    não muta wiring: `wiring::refresh_file_wiring` é a fonte única de edição, leitura
    (só com `content_hash` novo), `file_changed` e `task_output`, e todos passam pela
    admissão do walker. (b) Heavy tem uma lista só, `touring_foundation::is_heavy_hook`:
    o daemon orçamenta por ela e o cliente espera `HEAVY_OP_CLIENT_FLOOR_SECS`
    (orçamento + 60 s); heavy ainda rodando sai com **79**, nunca 75 (não repetir,
    consultar `index status`). (c) Raiz nomeada sem marcador nunca cai no índice tantivy
    global do `$HOME` (`scoped_root`); `.git` arquivo (worktree) é marcador. (d) Um
    palpite de wiring nunca rebaixa aresta mais forte, e tipo importado de outra crate
    fica com o resolvedor de imports, não com a inferência por nome.
18. **Cross-audit rodada 2 (14/09/2026, `docs/audits/cross-audit-2026-09-14-r2.md`)**. (a) O
    índice e o corpus do touring-quality seguem o `.gitignore` (`touring_foundation::gitignore`,
    fonte única; `why` responde `gitignored`): o build `site/` do MkDocs punha 47.877 símbolos
    velhos no índice. (b) O log do daemon/MCP vai para `~/.claude/touring/logs` com 7 dias de
    retenção; o watcher conta descartes em vez de logar cada um, e o filtro vê diretórios pais
    (25,5 GB em `/tmp` num dia). (c) `use touring_x::{A, B}` resolve pela raiz do crate (307
    imports caíam em `wiring_unresolved`). (d) F2.5 sem `Cargo.lock` é `UNVERIFIED` (0.5),
    nunca pass; o pragma de fixture do F2.1 só vale em comentário no cabeçalho. (e) Remediação
    pós-juiz (15/09): uma consulta tree-sitter com nó de outra gramática não compila e o
    extrator cai no regex em silêncio (Rust nunca rodou tree-sitter: `default_import`),
    `every_import_query_compiles` guarda; todo parse passa por `ast::parser::parse_bounded`
    (2 s + 2 s/MiB, `reset()` ao cortar), porque a gramática TS 0.23.2 prende
    `ts_parser_parse` para sempre com 86 bytes ASCII (`ts_parse_budget.rs`); o F2.1 vê todos
    os matches antes de suprimir por região (um payload em comentário escondia o sink) e lê
    shell por shebang; E2E acha o binário pela raiz do próprio teste, nunca uma sobra de
    `llvm-cov-target` (`locate_binary`, fonte única em `touring-hooks/tests/common`).

19. **Embeddings em GPU (15/09/2026, 30.4.51)**. O embedder (arctic-embed-m, fastembed/ONNX)
    roda no provider CUDA da ONNX Runtime quando carrega e roda, e na CPU caso contrário:
    `TOURING_EMBED_DEVICE=auto|cuda|cpu` (default `auto`; `cuda` nunca cai para CPU),
    arena por daemon `TOURING_EMBED_CUDA_MEM_MB` (2048). Medido na RTX 4060: query 8,7→2,3 ms,
    lote de 32 330→29 ms; recall de ponta a ponta quase não muda (o embedding era ~9 de
    ~220 ms), o ganho está em lote (`memory reindex` embeda por chunk). Quatro contratos.
    (a) O fastembed 5 liga `cuda` só no candle: `ort` é nomeado direto (`=2.0.0-rc.12`,
    features `cuda`+`copy-dylibs`) e `ORT_CUDA_VERSION=13` em `.cargo/config.toml` escolhe o
    pré-compilado cu13 (sem o pin, ele adivinha e cai em cu12); o sistema precisa de CUDA 13 +
    cuDNN 9 (`omarchy pkg add cuda cudnn`). (b) A ORT cai para CPU calada quando o provider
    falha: o provider é registrado com `error_on_failure`, um warm-up prova o cuDNN (abre no
    1º run), e o device efetivo, textos por device, latência e o motivo do fallback saem em
    `touring gate-metrics` (`embedding_*`) e no `touring doctor` (`gpu_embeddings`). (c) A ORT
    abre `libonnxruntime_providers_{shared,cuda}.so` do diretório do `argv[0]`, sem seguir o
    symlink (medido): toda pasta que expõe um binário leva as `.so` —
    `update-touring` (`RUNTIME_LIBS`) em `~/.local/bin`, `toolchain install` copia para a
    toolchain e `touring update` liga em `.touring/bin` (`PROJECT_RUNTIME_LIBS`), sempre do
    diretório do próprio `touring-daemon`, nunca de outro canal (outra build da ORT); as
    duas listas são cruzadas por `test_update_touring.py`. (d) O `HttpGpuBackend` (wgpu) segue
    fora do default — é top-k por par, e o ANN SIMD leva ~6 ms —; agora escolhe o adapter
    discreto (`adapter_preference`), pois o Vulkan lista a Intel integrada primeiro.

20. **Shell no touring-quality (Canvas D, 15/09/2026, `docs/audits/canvas-d-shell-2026-09-15.md`)**.
    (a) A linguagem de um arquivo é `verifications::lang_of` (extensão, depois shebang); script shell
    numa dimensão fora de `SHELL_DIMS` responde N/A no despacho (`not_applicable_to_shell`), nunca
    regra de Rust nem leitura como README/CI. (b) `sh`/`bash`/`zsh` e shebang de shell entram no corpus
    de diretório, e só arquivo regular entra: symlink rastreado com alvo apagado dava 0,0 fail-closed.
    Antes disso os gates P0 do juiz nunca tinham lido um script. (c) Nada medido não é aprovação: lista
    vazia e roll-up sem arquivo aplicável são N/A, e N/A de arquivo sai da média. (d) F4.1 de shell vem
    do ShellCheck; ausente ou com exit fora de 0/1 é UNVERIFIED 0,5. (e) Medir motor alterado com
    `TOURING_QUALITY_BIN` num `CARGO_TARGET_DIR` separado: `target/release/touring-quality` serve todas
    as sessões e o juiz.

21. **Wiring Python e a classe interno-only (B4, 15/09/2026)**. (a) A visibilidade de um símbolo
    Python vem do NOME (`Symbol::python_visibility`), não do texto do nó: sem `def`/`class`, todo
    binding de módulo caía em "público", e `_FORMAS = {...}` entrava no grafo como API pública. (b) Um
    arquivo Python passa a registrar o uso dos próprios símbolos públicos
    (`graph::python_self_referenced_names`, aresta `(arquivo, símbolo) → arquivo`, tier `ast_inferred`).
    Isso é gravado na transação PÓS-WALK, junto das demais arestas inferidas: o rebuild limpa as
    inferidas depois do walk, e o que for escrito antes some. (c) `internal_only_symbols` é a classe
    nova — público, consumido, mas só pelo próprio arquivo. Fica FORA da lista de órfãos (a baseline do
    juiz não se move) e sai à parte em `touring wiring orphans -j` (`internal_only`,
    `internal_only_count`). Os dois filtros de linha de produtor têm fonte única
    (`public_producer_rows_sql`).

22. **B6 e a captura de crash (16/09/2026)**. (a) `python_qualified_uses` lê `import_statement`
    **e** `import_from_statement`, e a chave do mapa nome→módulo é **`asname or name`**: dois
    parte da família chega SEM alias, e o UNIVERSO da medida importa tanto quanto o número: no
    repositório em disco, que é o que o indexador varre, um braço só-alias perderia 43 de 126
    símbolos (34,1%) no projeto e 8 de 53 (15,1%) no escopo do juiz. Com as duas formas convivendo, duas ligações podem competir
    pelo mesmo nome local — o walk é uma pilha, não ordem de fonte, então cada ligação carrega o
    byte de origem e a última textual vence, como no Python. `import a.b` e `from . import x`
    seguem de fora: o caminho seria palpite. O extrator e o resolvedor só se encontram no
    rebuild, e a junção tem teste próprio (`touring-cli/tests/b6_python_qualified_path.rs`) —
    teste de componente verde não prova o caminho. (b) O perfil release **não pode ter
    `strip = true`**: era ele que fazia o core do motor trazer um frame. Hoje leva
    `debug = "line-tables-only"` + `split-debuginfo = "unpacked"`, guardado por
    `test_touring_quality_score.py`. (c) `scripts/touring-quality-score` grava um journal por
    execução em `~/.claude/touring/logs` e, com exit ≥ 128, um artefato com stderr, sinal e
    backtrace; `crash_capture_accept.sh` prova isso contra o motor real e `crash_matrix.sh`
    mede TAXA por célula de ambiente, nunca um caso isolado. (d) **`kill -SEGV` não mata um
    processo Rust**: o runtime captura 11 e 7 (guard page de stack overflow) e engole o sinal
    entregue por `kill(2)` — medido, o motor terminou o score e saiu 0. Para exercitar coletor
    de crash use **SIGABRT**; e note que `panic = "abort"` emite `ud2` → **SIGILL**. (e) Detecção
    de processo em script é por **nome do executável** (`pgrep -x`), nunca por linha de comando:
    o `pgrep -f` do `safe-clean.sh` acusava o próprio shell que o invocava e perdia os `rustc`
    reais. Narrativa e números: `docs/audits/b6-e-captura-de-crash-2026-09-16.md`.

23. **Órfãos do juiz e o cwd do daemon (18/09/2026)**. (a) `file_todos` é estado DERIVADO do
    conteúdo: o reindex lê marcadores com `TodoKind::from_comment_line` (atrás de qualquer líder de
    comentário, palavra-chave em maiúsculas, `NOTE` fora) e grava com `replace_todos`, que troca o
    conjunto do arquivo; `insert_todo` acumulava. (b) **O workspace de uma resolução Rust é o do
    ARQUIVO** (regra do Cargo: primeiro `Cargo.toml` com `[workspace]` acima dele),
    `symbol_extractors::workspace_for`; só um caminho relativo cai no processo
    (`TOURING_PROJECT_ROOT`, depois o cwd). O mapa de crates era um `Lazy` do cwd do daemon: nascido em
    `~/Work`, gravou 3.166 imports `touring_*` como `external`. Mapa vazio é `unmeasured`, nunca
    `external`; `classify_unresolved` e `definer_module` recebem o arquivo de origem, e o caminho de
    edição ancora o relativo na raiz do banco (`absolute_consumer`). (c) Remoção de símbolo: o oráculo
    final é o `cargo check` — grep perde import agrupado, e `#[deprecated]` não acende no uso de um
    alias do item. `--all-features` no workspace falha por desenho no binário `touring` (alocadores
    exclusivos). (d) **Auto-referência Rust** (`self_refs::self_referenced_names`, fonte única do
    rebuild E da edição): método conta só por CHAMADA `self.m()` (o campo `self.m` não)/`Self::m`/`Dono::m`,
    o resto por identificador solto fora de cabeçalho `impl`; item `#[test]`/`#[cfg(test)]` não conta
    (30.4.56) — `internal_only`, nunca órfão; a edição apagava essas arestas até o
    rebuild. Dentro de macro (`format!`, `assert!`) não há árvore, só `token_tree`: a forma vem dos
    tokens vizinhos (`method_calls::macro_token`, 30.4.57), fonte única do despacho F9, das refs de
    tipo/const e da auto-referência; `format!("{}", c.signal_prefix())` não ligava nada. (e) `PreToolValidator::validate_command` lê a linha como shell (aspas, `#`, pipelines e
    listas): flag é PALAVRA (`-f`, `-rf`), nunca substring (`touring-foundation` negava `git add`),
    o desvio `--dry-run` é palavra do comando (um comentário o ativava) e padrão que atravessa `|` lê a
    pipeline. Todas as camadas: o prefixo `rm ` nega recursivo+forçado por palavra, uma flag só cai no
    schema (a razão a nomeia), o NOME do comando é dobrado e a flag não (`git -F` ≠ `-f`), e opção de
    wrapper que consome valor é pulada (`sudo -u root git push -f`). (f) Envelope do `touring run`: `success` = programa limpo, `executed` = executor rodou;
    o MCP devolve `isError` na falha. (g) `GeneratorPlan::check_schema_version` barra plano de outra
    versão MAJOR no `Draft → Verified`. Narrativa: `docs/audits/orfaos-24-2026-09-18.md` e
    `docs/audits/decisoes-orfaos-2026-09-18.md`.

24. **Defeitos relatados pelo analise (18/09/2026, 30.4.58)**. (a) **Aresta nunca cruza
    linguagem**: `touring_storage::knowledge_wiring::language_family` (rust/python/js/java/go)
    é a fonte única; a inferência por nome só casa produtor da família do consumidor, e
    `touring wiring repair --purge-cross-language [--dry-run]` remove as arestas cruzadas
    já gravadas, restaurando a linha NULL de quem ficou sem nenhuma. O `repair` trata só
    produtor Rust, casa por palavra e grava `ast_inferred`. (b) **Import Python pelo caminho
    dos hooks**: `touring_hook_runtime::wiring::record_import_consumers` é o laço único do
    rebuild e da edição; `definer_module` segue `from x import S` em `.py`; a raiz Python
    vem do ARQUIVO (`.touring`/`.git`/`pyproject.toml`/`setup.*`, mais `src/`), nunca do cwd;
    linha de consumidor presa a símbolo que o módulo Python não define mais é reatribuída ao
    definidor ou removida. (c) **Memória aposentada** (`superseded_by`) nunca volta por busca:
    `touring_intelligence::rl::memory::retirement` (`live_predicate`/`retired_keys`/
    `retirement_of`) em todo leitor; o recall filtra TODOS os canais antes da RRF (ANN com
    over-fetch 40→20); leitura exata por chave continua vendo. (d) **Recusa de apagamento
    nomeia a rota aberta**: `bash_ast_validator::DELETE_ROUTE` nas duas camadas, bypass por
    PALAVRA (`discloses_intent`), guarda D8 cruzada `every_delete_refusal_names_an_open_route`.
    (e) `learning` aceita `-j` (stdout JSON até na falha) e reward negativo. (f) Caminho
    absoluto de consumidor Rust: `under_workspace` — `resolve_reexport` montava
    `<root>//<root>/…`, e o teste só falhava com `TOURING_WORKSPACE_ROOT` exportada.
    Narrativa: `docs/audits/defeitos-analise-2026-09-18.md`.

25. **Resíduos do purge no analise (18/09/2026, 30.4.59)**. (a) **Importar não é usar**
    (decisão de Gabriel): `ast_bridge::extract_consumed_imports` é o ponto único do que vira
    consumidor — Rust sem reexporte (`pub use`/`pub(crate) use`, lido por
    `imports::is_reexport_declaration`, fonte de `rust_reexports` e do filtro da passada de
    tipos), Python só com nomes referenciados fora dos imports (`python_unreferenced_imports`:
    fachada `__all__`/`__init__` e import morto não contam; alias julgado pelo nome local). O
    FIX-4 (`record_reexport_consumer`) saiu, a varredura de caminhos diretos descarta o caminho
    que o arquivo reexporta, e o grep do `wiring repair` lê só `use` sem visibilidade (o
    dry-run da 30.4.59 creditaria `pub use` e desfaria a decisão). Quem importa PELA fachada
    segue creditado ao definidor, e um reexporte que o arquivo TAMBÉM nomeia no próprio código
    é uso (`rust_names_used_outside_use`: a tabela do `touring-assists/handlers/mod.rs` deixou
    11 handlers órfãos no primeiro juiz). A edição Rust passou a usar o mesmo
    `record_import_consumers` do rebuild: o `imports_json` é regex de linha e nunca via
    `use a::{B, C}`. (b) **Método
    Python por atributo**: `python_method_calls.scm` captura `obj.m`/`obj.m()`, e
    `find_python_method_producers` só credita `method` em módulo do qual o consumidor já importa
    (uma aresta de método nunca abre a trava para outra). `record_python_qualified_uses` é a
    passada `alias.Nome` única do rebuild e da edição (a edição a perdia), com definidor, e roda
    ANTES da passada de métodos, que a lê como trava. (c) **A linha NULL é a declaração do
    produtor**: `wiring repair` não a apaga mais; a amostra do repair nunca é elidida
    (`bounded_reply`). (d) **CEG**: `SandboxResult.signal` e `describe_signal` nomeiam o sinal e
    o cap; arquivo no tmp privado com tamanho EXATO do `RLIMIT_FSIZE` vira `OutputLimit` mesmo com
    exit 0 (`capped_files`). (e) `inferlets::fs_walk` nunca entra em diretório por symlink.
    Narrativa: `docs/audits/defeitos-analise-2026-09-18.md` §30.4.59.

26. **A travessia credita o módulo (opção B, decisão de Gabriel 19/09/2026, 30.4.60)**. Só a
    FOLHA de um caminho era creditada (`Z` em `use crate::a::b::Z`), então todo módulo que serve
    de namespace era órfão por construção: 643 aqui, 392 deles com travessia real a uma linha
    (medição da sessão analise). Agora `graph::module_paths::rust_module_paths` devolve cada
    caminho que NOMEIA módulo — dos `use` (grupos, `super`/`self`) e dos caminhos inline no
    código (160 de 561 tinham o inline como 1ª evidência) — e
    `wiring::record_module_path_consumers` credita o ARQUIVO que declara cada módulo
    (`symbol_extractors::declaring_file_for_module`: `src/a/b.rs` é declarado em `src/a.rs` ou
    `src/a/mod.rs`; sob `src/`, na raiz do crate). Quatro contratos: (a) ler pela ÁRVORE deixa
    comentário e string de fora por construção (a 1ª régua do analise, em regex, contou um
    `///` que dizia "long-orphaned"); (b) a regra do reexporte é a mesma da 30.4.59 — caminho que
    só um `pub use` nomeia não credita, e o que o arquivo também usa credita; (c) o pai que usa o
    próprio filho (`mod x;` + `x::f()`) gera aresta do arquivo consigo mesmo, a classe
    `internal_only`; (d) o crédito é estrutural, então é a única isenção nova do guard
    `record_consumer_sites_resolve_the_definer` — uma declaração `pub mod` É a definição do
    módulo, e nenhuma cadeia de `pub use` a renomeia. Uma passada só para o rebuild e a edição.

27. **As três formas que a árvore e o layout não alcançam (30.4.61, 19/09/2026)**. A primeira
    passada da opção B levou os órfãos de módulo de 643 a 239, mas 65 seguiam com a travessia a
    uma linha. Medidos um a um, eram três formas, não um erro geral. (a) **Dentro da macro não há
    árvore** — a lição da 30.4.57, agora para caminhos: `handler: |args| super::activity::run(args)`
    mora num `vec![…]`, e um `token_tree` não tem `scoped_identifier` (47 dos 65, todos na tabela
    de comandos do touring-server). `method_calls::macro_paths` remonta o caminho da sequência de
    tokens — a mesma leitura que `macro_token` faz de uma chamada, um segmento mais larga. (b)
    **`#[path = "…"]` e `mod x { … }` não têm arquivo que o layout do Cargo nomeie**, e
    `declaring_file_for_module` lê o layout de trás para frente: `symbol_extractors::
    declarer_of_crate_path` caminha para FRENTE da raiz do crate, achando cada segmento como um
    `mod` do arquivo alcançado — o declarante sai por construção, e segmento não declarado devolve
    `None` em vez de palpite (15 casos, 12 deles os `cli_handlers_*` do touring-cli). (c) **`use
    crate::x;` importa o MÓDULO**: o extrator via só o caminho `crate`, então `import_paths` anexa
    também cada símbolo minúsculo ao caminho — o que não for módulo não resolve para arquivo
    nenhum, e o disco decide. Nota de método: o instrumento do analise (regex) classificou 72
    módulos como "só reexporte" que a regra da 30.4.59 conta como uso (o arquivo TAMBÉM nomeia o
    símbolo) e não vê crédito cross-crate por reexporte — divergência do instrumento, não do índice.

28. **A declaração vence a inferência (30.4.62, 19/09/2026)**. Os 8 módulos que sobraram da
    30.4.61 eram todos INLINE, por dois defeitos distintos. (a) **O conjunto errado respondia
    "este primeiro segmento é meu filho"**: `rust_declared_child_modules` existe para achar o
    ARQUIVO do filho, e por isso deixa inline de fora; mas `prefixes` o usava para saber se um
    nome é NOMEÁVEL — e `pub mod compute { … }` + `compute::f()` é tão nomeável quanto os
    outros. `rust_declared_module_names` é o conjunto das três formas, e o pai que usa o
    próprio filho inline vira `internal_only`. (b) **A ordem em `record_module_path_consumers`
    estava invertida**: o resolvedor de layout, que sonda o disco e segue reexportes (duas
    inferências), vinha antes da caminhada de declarações, que é o FATO. touring-cli declara
    `pub mod shared { … }` inline E reexporta nomes de `touring_hook_runtime::shared`; como
    `reexport_origins` casa qualquer segmento do caminho, o resolvedor creditava o módulo do
    OUTRO crate com a declaração local a uma linha. A caminhada vem primeiro — e, porque passou
    a rodar para todo caminho `crate::` em vez de só no fallback, `module_declarations_of`
    memoriza por (caminho, mtime): o mtime está na chave porque o daemon vive dias, e um
    arquivo editado entre rebuilds não pode responder com o mapa velho. Provado por MUTAÇÃO —
    com a ordem antiga o e2e credita `crates/origem/src/lib.rs`. Nota de método: o fixture do
    e2e vivia em `src/` na raiz e `detect_crate_src_root` só reconhece `crates/<nome>/src`, de
    modo que a caminhada nunca rodava ali e quem fazia o teste passar era uma aresta de
    inferência por nome; o fixture agora tem a forma do workspace real, com dois crates.

29. **O DAG mentia sobre si (30.4.63, 19/09/2026)**. Um censo do DAG vivo mostrou 554
    subtasks "abertos" em 148 tasks, e quase nada era trabalho. Quatro defeitos, todos de
    PONTA QUE NÃO SE ENCONTRA. (a) **`finalize` não arquivava**: gravava
    `status = 'finalized'` e nunca `archived_at`, enquanto `archive_completed_tasks`
    procurava `status = 'completed'` — valor que o finalize nunca produz; e o chamador em
    `hook_registry` testava `contains("archived":true)` num payload que jamais teve o
    campo. Resultado: `archived_at` NULL nas 381 tasks e todo filtro por arquivo lendo o
    histórico inteiro como vivo. Hoje o finalize carimba e DECLARA (`archived`), e os
    estados terminais vêm de `touring_foundation::task_lifecycle::TERMINAL_TASK_STATUSES`
    /`terminal_status_sql_list()`, fonte única das três rotas. (b) **A rotina de retenção
    não tinha chamador nenhum** — só os próprios testes — então nem com o predicado certo
    rodaria: agora há `touring decompose archive [--older-than-secs N] [--dry-run]`
    (`cli-decompose-archive`). (c) **O fechador conhecia 2 dos 3 estágios do espelho**:
    `task-completed` nomeava `::validate` e `::implement` como literais e ignorava
    `::scout` — 74 dos 78 subtasks abertos sob mirrors eram esse slot. `MIRROR_SCAFFOLD_STAGES`
    é a lista única do scaffolder (`bridge_task_created`) e do fechador novo
    (`close_scaffold_stages`). (d) **O scout perpétuo se alimentava do próprio rastro**: o
    corpus TF-IDF indexa descrições de decompose, e os tickets que o scout cria entravam
    nele — o ticket `task_1788296582280254749` trazia como amostra `decomp:<o ticket
    anterior>`. `is_scout_ticket` tira o ticket do corpus e o contador do `scout_perpetuo`
    desconta o eco (`own_echoes` reportado, nunca silencioso). Mais duas correções:
    `touring index rebuild --wait` (poll de `index status` até a geração selar; o default
    segue síncrono, e o `--wait` existe para o exit 79 de rebuild passado do orçamento), e
    a régua `code_mode_reuse`, que contava só `--harvest` explícito (0 de 16.205 linhas)
    enquanto a escada de trust tinha 369 corpos: hoje lê `snippet_stats::ladder_totals` e
    separa `ladder_enrolled` de `ladder_reused` — **enrolar não é reusar**, e o piso segue
    FAIL por comportamento (9 re-execuções em 8.572 runs), que é o diagnóstico honesto.
    Narrativa: `docs/audits/dag-mentia-sobre-si-2026-09-19.md`.

30. **Um payload de máquina não tem intent, e uma relaxação por-comando não é chave de
    máquina (30.4.64 → 30.4.65, 22/09/2026)**. (a) O `prompt_enhance` classificava
    notificações de agente, feedback do Stop hook e saída de `!` como se fossem pedidos do
    humano — visto ao vivo injetando TEST, CODE e DEBUG em turnos automáticos.
    `AUTOMATED_PAYLOAD_PREFIXES` + `is_automated_payload` (consultados por `compose_json`
    **e** `run_user_prompt_submit`, com guard cruzado) devolvem `{skipped:
    "automated_payload", suppressOutput}` e nenhum intent nem CILA. A lista é **medida**:
    os 8 primeiros prefixos deixavam passar 160 de 800 prompts reais (20%), 134 deles
    mensagens de outra sessão Claude; hoje são 14, com um teste por prefixo. Comandos de
    barra ficam de fora de propósito — `<command-args>` carrega as palavras do humano.
    (b) `TOURING_CODE_MODE=native <cmd>` relaxa UM comando, e a apresentação é resolvida na
    env de quem decide: o daemon. Um daemon que herda a var desliga os gates de code mode
    de TODAS as sessões, com `doctor` verde e a prova comportamental reprovando 2/40 sem
    nomear a causa — foi o que aconteceu ao reiniciar o daemon de um shell que a carregava.
    Os **dois** launchers agora limpam a var (`daemon_spawn::PER_COMMAND_RELAXATIONS` no
    Rust e o `env -u` de `launch_daemon_and_wait` no `update-touring`, listas cruzadas por
    `test_update_touring.py`); a intenção deliberada tem porta própria,
    `TOURING_DAEMON_CODE_MODE=<modo>`. Diagnóstico em 1 comando:
    `tr '\0' '\n' < /proc/<pid do daemon>/environ | grep TOURING_CODE_MODE`.
    (c) **`update-touring` não alcança sessão nenhuma** quando existe toolchain default: o
    shim serve `~/.touring/toolchains/<default>/bin`, um snapshot imutável. Código novo
    testado e instalado fica inerte até `scripts/propagate-release.sh <versão>`;
    `TOURING_HOOK_SHIM_TRACE=1` imprime o binário que responde.

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
