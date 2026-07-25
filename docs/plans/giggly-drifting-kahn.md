# Touring Productization — Pln2 · De Singleton Global a Sistema Instalável, Versionado e Per-Project

> **Pln2 = (Pln1)²** — profundidade exponencial, referências file:line completas, impactos sistêmicos de 1ª/2ª ordem, script de validação cross-audit por fase. O daemon multi-instância (antigo W12.5) é a **Fase 1** de 6, não o objetivo.

---

## 1 · Context — o porquê, o problema, o resultado final e suas consequências

Touring nasceu como **ferramenta global de workspace único**: binários em `~/.local/bin/touring*` (symlinks → `~/.claude/rust/target/release/`), hooks cabeados em `~/.claude/settings.json` (≈29 registros apontando `$HOME/.claude/hooks/touring-hook`), **um** daemon singleton em `/tmp/touring-daemon-1000.sock`, e **1,6 GB de bancos globais** em `~/.claude/touring/` (symbols.db 733 MB · graph.db 644 MB · knowledge.db 164 MB · memory.db 102 MB). Essa topologia foi correta enquanto Touring servia um só workspace — `~/.claude/rust`.

Mas o objetivo **evoluiu**, e a topologia atual passou de virtude a passivo. Gabriel determinou: **Touring deve ser um produto** — um sistema cuja **fonte canônica vive em `~/projects/touring`** (o ambiente de refino e evolução, ao lado de konverter/analise/transferegov), **versionado**, **instalável e personalizável por projeto**, de modo que **cada atualização possa ser propagada a cada projeto individualmente**. Em uma frase: tirar Touring do path global e transformá-lo num *language-toolchain-like* (padrão rustup) — fonte canônica → release versionado → toolchain home → instalação per-project → update propagado.

**Consequência de NÃO migrar** (o custo do status quo, 1ª e 2ª ordem): (1ª) todos os projetos compartilham um daemon/índice/memória globais → **contaminação cruzada** (símbolos de konverter poluem o índice de analise); (2ª) Touring **não pode ser versionado nem revertido por projeto** → evoluir Touring arrisca **todos** os projetos simultaneamente, e um bug numa release derruba o ambiente inteiro de Gabriel. O incidente `touring-web-server` bin-collision desta sessão é um sintoma desse acoplamento global.

**Consequência de migrar** (o resultado final desejado, 1ª e 2ª ordem): (1ª) **isolamento** — cada projeto com seu daemon, índice, memória, config e binários pinados; (2ª) **evolução segura** — atualizar konverter para Touring vN+1 enquanto analise permanece em vN, com rollback por projeto; **distribuição real** — `curl install.touring.dev | sh`; **versionamento coerente** — SemVer com release assinado (sigstore) e changelog automatizado (release-plz). Touring deixa de ser um apêndice de `~/.claude/` e vira um **produto de primeira classe** em `~/projects/`.

A boa notícia que a exploração revelou: **a fundação já está ~70% construída** (W12.1/2/4/6/7 + W13.1-4 prontos; W13.5/6 staged). O trabalho restante é **integração e fechamento de 4 gaps cirúrgicos**, não construção do zero. Este Pln2 mapeia o arco completo e o executa **sessão a sessão**, fase a fase, cada uma com seu script de validação cross-audit.

---

## 2 · Estado verificado (file:line) — fundação existente vs gaps

| Peça | Status | Evidência (file:line) |
|---|---|---|
| W12.1 `touring init-project` (cria `.touring/{touring.toml,data,bin,hooks}`) | ✅ DONE | `crates/touring-server/src/cli/init_project.rs:113-146`; default body `:66-87` |
| W12.2 toolchain manager (`~/.touring/toolchains/<ver>/{bin,lib,share,meta.toml}`) | ✅ DONE | `crates/touring-server/src/cli/toolchain.rs:6-15` (layout), `:155-206` (init/list/default/install --from-tarball/remove) |
| W12.4 layered config (hardcoded < /etc/touring < ~/.touring/config.toml < .touring/touring.toml) | ✅ DONE | `crates/touring-foundation/src/config.rs:342-349` (`detect_layered`), `:351-363` (`find_project_toml_walk_up`) |
| W12.6 hook walk-up shim (4-layer fallback) | ⚠ DONE-script, NÃO instalado | `scripts/hooks/touring-hook-shim.sh:39-102`; **não symlinkado** em `~/.claude/hooks/touring-hook` |
| W12.7 `touring migrate-from-global` (copia DBs globais → `.touring/data/`) | ✅ DONE | `crates/touring-server/src/cli/migrate_from_global.rs:35-46` (10 DBs), `:143-222` |
| **W12.5 daemon multi-instance** | 🔴 PARTIAL — **lock global singleton bloqueia** | `daemon_lock_path()` `crates/touring-hooks-core/src/ipc.rs:60-63` (keyed só por uid); bind OK `crates/touring-dispatch/src/daemon.rs:546+633` |
| **W12.3 `touring update` / `touring component`** | 🔴 **AUSENTE** (não está no command_table) | sem handler; planejado, nunca implementado — **o gap nuclear da propagação de update** |
| Version-pin em `.touring/touring.toml` (`channel`/`version`) | 🔴 **AUSENTE** | `init_project.rs:66-87` não tem campo de versão |
| `.touring/bin/` populado | 🔴 **VAZIO** | scaffolder cria o dir (`init_project.rs:90`) mas nada instala binários nele |
| W13.1-4 (README, missing_docs, docs.rs, msrv 1.85, semver) | ✅ DONE | `.github/workflows/ci.yml:25-202` |
| W13.5 sigstore + W13.6 release-plz | ⚠ STAGED (git de Gabriel) | `scripts/touring_premium_refactor_2026/staging/w13-github-workflows/{sigstore-release.yml,release-plz.yml}` |
| W14 install.touring.dev + Homebrew/Scoop/Docker | 🟡 SKELETON (URLs placeholder) | `scripts/packaging/install.touring.dev.sh:1-134`; `homebrew/touring.rb:22-40`; `scoop/touring.json:9-15` |
| `touring-license` (Tier Free/Standard/Premium/Enterprise + policies) | 🟡 60% (JWT verify atrás de feature) | `crates/touring-license/src/lib.rs:50-128` |
| Hardcodes `~/.claude/rust` (bloqueiam fonte em ~/projects) | 🔴 2 runtime + N test | `WORKSPACE_ROOT_MARKER` `crates/touring-storage/src/knowledge_wiring.rs:24` + `crates/touring-hooks-core/src/knowledge_wiring.rs:24`; gotcha dirs `touring-cli/src/cli/gotcha.rs:94`, `touring-hook-handlers/src/hooks/session_hooks.rs:74` |
| Versão incoerente | 🔴 workspace `0.1.0` ≠ binário `30.0.0` | `Cargo.toml:143` vs `crates/touring-server/Cargo.toml:3` |

**Ativos reutilizáveis** (não reinventar): `resolve_daemon_socket_path*()` (`config.rs:403-460`, puro/testável) · `find_project_toml_walk_up()` (`config.rs:351`) · `init_project_in()` (`init_project.rs:113`) · `try_autostart_daemon()` (propaga `TOURING_DAEMON_SOCKET`, `crates/touring-hooks/src/main.rs:869-896`) · padrão de teste isolado (`crates/touring-server/tests/cli_wave6_e2e.rs:24-58`) · toolchain `meta.toml`/`default` (`toolchain.rs:31,389`).

---

## 3 · O arco — 6 fases, cada uma um marco com contribuição e efeitos

> Cada fase declara: **Contribuição** (como serve o resultado final) · **Efeitos 1ª/2ª ordem** · **Subtarefas** · **Script de validação cross-audit** (gerado COM a fase). TDD em todas (RED antes do código). Fonte de verdade dos comandos de validação: execução real, nunca inferência ([[real-exit-codes]]).

### Fase 0 — Fundação: desacoplar a fonte canônica + version-pin  ·  `[S-M, ~0.5d]`  ·  *desbloqueia tudo*

**Contribuição**: torna a **localização da fonte movível** (de `~/.claude/rust` → `~/projects/touring`) e os **projetos version-pináveis** — os dois pré-requisitos silenciosos de todo o resto.

**Subtarefas**:
- **0.1** Substituir os 2 `WORKSPACE_ROOT_MARKER` runtime (`touring-storage` + `touring-hooks-core` `knowledge_wiring.rs:24`) por resolução via `TOURING_WORKSPACE_ROOT` env → fallback config `[paths].workspace_root` → fallback atual. Idem gotcha dirs (`gotcha.rs:94`, `session_hooks.rs:74`).
- **0.2** Adicionar campo `[toolchain] channel = "<version>"` (e `workspace_root` opcional) ao schema de `.touring/touring.toml` (`init_project.rs` `DEFAULT_TOURING_TOML` + parser em `config.rs`). É o **pino de versão por projeto**.
- **0.3** Reconciliar versão: definir SemVer único (`[workspace.package] version`) e fazer o binário derivar dela (eliminar o drift 0.1.0↔30.0.0). Decisão de numeração = ASK Gabriel na execução (ex.: `1.0.0-rc.1` per W13.9).

**Efeitos**: (1ª) a fonte pode viver em `~/projects/touring`; cada projeto declara qual versão usa. (2ª) habilita rollback por projeto (Fase 3) e o release versionado (Fase 5); remove a contaminação de símbolos cross-workspace.

**Script de validação `validate_phase0.sh`** (cross-audit duplo): (a) `grep -rn "/.claude/rust" crates/ --include=*.rs | grep -v test` retorna **0** hardcodes runtime; (b) build com `TOURING_WORKSPACE_ROOT=/tmp/fake-root` resolve sem panicar; (c) `.touring/touring.toml` com `channel="x"` é lido por `detect_layered` (assert via unit test); (d) `touring --version` == `[workspace.package] version`; (e) toda citação file:line do plano re-verificada por `touring index find`/`grep` (auditoria de referências).

### Fase 1 — Daemon multi-instância per-project (antigo W12.5)  ·  `[M-L, ~2-2.5d]`  ·  🛑 *BLOCKING de W12.6/7/9/10*

**Contribuição**: **isolamento de runtime** — N daemons coexistem (1/projeto), socket em `<project>/.touring/daemon.sock`. Sem isso, nenhum projeto tem índice/memória próprios.

**Subtarefas** (detalhe técnico file:line já mapeado):
- **1.0 (RED)** `test_two_projects_two_daemons` em novo `crates/touring-server/tests/w12_5_per_project_daemon_e2e.rs` (padrão `cli_wave6_e2e.rs`): 2 TempDir-projects opt-in → 2 sockets LISTEN distintos.
- **1.1** **Per-socket lock** (O bloqueador): `daemon_lock_path()` (`ipc.rs:60-63`) → `/tmp/touring-daemon-{uid}-{blake3(socket)[..8]}.lock`, threadado no `acquire_lock` (`daemon.rs:1755`) e no `reset`.
- **1.2** Unificar os 4 resolvers hardcoded de cliente (`daemon_client.rs:25`, `tools_analysis.rs:26`, `daemon_ctl.rs:304`, `granularity_adapter.rs:122`) → delegar a `touring-foundation::config::resolve_daemon_socket_path()` (fonte única, leaf, sem ciclo). `ipc.rs::daemon_socket_path` também delega.
- **1.3** `spawn_daemon()` (`daemon_ctl.rs:442`) propaga `TOURING_DAEMON_SOCKET` (espelhar `try_autostart_daemon`).
- **1.4** daemon-ctl multi-daemon (REGRA #19): alvo por socket; `--socket/--project` em status/stop/restart; `daemon-ctl list-all`; `reset` remove lock socket-derivado. **Nunca pkill.**
- **1.5** **Ativação opt-in** (decisão A de Gabriel): layer per-project no resolver — walk-up por `.touring/touring.toml` com `[daemon] per_project=true` (default OFF) → `<dir>/.touring/daemon.sock`. Zero disrupção no `~/.claude/rust` vivo (sem flag → global).

**Efeitos**: (1ª) `lsof | grep daemon.sock` = N sockets p/ N projetos opt-in. (2ª) base para W12.6 (shim usa o daemon local) + isolamento de memória/RL por projeto; risco N×~92 MB RSS mitigado por opt-in + idle watchdog.

**Script `validate_phase1.sh`**: RED→GREEN; spawn 2 daemons isolados (TOURING_DAEMON_SOCKET temp) → `lsof -U` mostra 2 LISTEN; `daemon-ctl list-all` lista ambos; **re-verifica daemon de `~/.claude/rust` inalterado** (global, sem flag); cargo check/clippy/test + 50-dim ≥ Gold nos arquivos tocados.

### Fase 2 — Lifecycle de instalação per-project: popular `.touring/bin` + ativar o shim  ·  `[M, ~1-1.5d]`

**Contribuição**: faz o projeto **realmente usar seu próprio toolchain** (bins + hooks + daemon locais) — fecha os passos (b)+(c) do ciclo de vida.

**Subtarefas**:
- **2.1** `touring init-project` (ou `touring update` da Fase 3) **popula `.touring/bin/`** — symlink/cópia dos binários da toolchain pinada (`~/.touring/toolchains/<channel>/bin/`) para `<project>/.touring/bin/`.
- **2.2** Instalar o **hook walk-up shim** como `~/.claude/hooks/touring-hook` canônico (`scripts/hooks/touring-hook-shim.sh`, já 4-layer): resolve `.touring/bin/touring-hook` → `~/.touring/toolchains/<default>/bin/` → fallback. Fail-open (exit 0).
- **2.3** Tornar `settings.json` / `update-touring` project-aware: dual-target deixa de hardcodar só `~/.claude/hooks`; o shim faz o walk-up. `TOURING_PROJECT_ROOT`/`CLAUDE_PROJECT_DIR` deixam de ser hardcoded p/ `~/.claude/rust`.

**Efeitos**: (1ª) hooks e daemon de um projeto vêm da toolchain DELE. (2ª) habilita versões divergentes por projeto (konverter@vN, analise@vN-1) sem conflito; o `update-touring` global vira um caso particular (toolchain "dev").

**Script `validate_phase2.sh`**: em TempDir-project, `init-project` + popular bin → `<proj>/.touring/bin/touring-hook` existe e é executável; shim com `CLAUDE_PROJECT_DIR=<proj>` resolve o bin local (TRACE=1 confirma layer 2); fora do projeto, resolve o default; settings.json validado (`jq`).

### Fase 3 — `touring update` + `touring component`: propagação de update per-project  ·  `[L, ~2-3d]`  ·  *o núcleo do objetivo*

**Contribuição**: **"a cada atualização, rodar a atualização em cada projeto individualmente"** — a peça nuclear ausente (W12.3). Fecha o passo (d).

**Subtarefas**:
- **3.1** `touring update [version|--channel|--rollback]`: lê o pin do projeto (`[toolchain] channel`) → seleciona a versão em `~/.touring/toolchains/` → re-linka `.touring/bin/` → migra dados se schema mudou (reusa `migrate-from-global` para o padrão de cópia) → reinicia o daemon per-project (Fase 1). Registrar no command_table.
- **3.2** `touring component {list,add,remove}`: gerencia componentes opcionais (generators, plugins, share/) por projeto.
- **3.3** `touring toolchain install` ganha download por URL (hoje só `--from-tarball`) — fonte = release server (Fase 5) ou path local da fonte canônica.
- **3.4** Pin + lockfile: `touring update` grava a versão resolvida (rollback determinístico). `--all-projects` opcional itera projetos registrados (`crates/touring-server/src/projects/`).

**Efeitos**: (1ª) atualizar/reverter Touring por projeto, atômico. (2ª) **evolução segura** — testar vN+1 em um projeto-piloto antes de propagar; desacopla a velocidade de evolução de Touring do risco aos projetos.

**Script `validate_phase3.sh`** (cross-audit profundo): instala 2 toolchains fake (vA,vB) em `~/.touring/toolchains/`; projeto pinado em vA → `touring update --channel vB` → `.touring/bin` aponta vB + daemon reiniciou no novo binário (`readlink /proc/<pid>/exe` sem `(deleted)`); `touring update --rollback` volta a vA; `touring status` reflete a versão; auditoria: cada subcomando novo existe no command_table (`touring <cmd> --help` exit 0).

### Fase 4 — Repositório canônico em `~/projects/touring`  ·  `[S infra + git de Gabriel]`

**Contribuição**: tira Touring do path global de `~/.claude/` e o estabelece como **projeto canônico em `~/projects/`** — o ambiente de refino/evolução.

**Subtarefas** (infra; o `git mv`/move é de Gabriel — REGRA #11):
- **4.1** `update-touring`: `RUST_WORKSPACE` (`~/.local/bin/update-touring:34`) → configurável via `TOURING_WORKSPACE_ROOT` (Fase 0). Build a partir de `~/projects/touring`.
- **4.2** Resolução de fonte da toolchain (Fase 3.3) aponta para `~/projects/touring` como "dev channel".
- **4.3** Atualizar `~/.claude/CLAUDE.md` Tools Path Map + `~/.claude/rules/*` canonical paths (co-evolução docs↔código). Atualizar `settings.json` `TOURING_PROJECT_ROOT`.
- **4.4** Checklist de corte + rollback documentado (a fonte antiga `~/.claude/rust` permanece até validação E2E).

**Efeitos**: (1ª) Touring é um projeto normal em `~/projects`, versionado por Gabriel. (2ª) o ambiente de evolução é isolado dos projetos-consumidores; o "dogfooding" passa a usar o próprio mecanismo per-project.

**Script `validate_phase4.sh`**: `TOURING_WORKSPACE_ROOT=~/projects/touring update-touring --verify-only` resolve binários; `touring doctor -j` 5/6; nenhum runtime hardcode de `~/.claude/rust` resta (`grep`); docs↔código sem drift (cross-ref das paths citadas).

### Fase 5 — Distribuição & versionamento GA  ·  `[M-L, ~2-3d + git de Gabriel]`

**Contribuição**: releases versionados, assinados e instaláveis — Touring vira instalável por terceiros/máquinas novas.

**Subtarefas**:
- **5.1** Promover (git de Gabriel) os workflows staged W13.5 sigstore + W13.6 release-plz (`staging/w13-github-workflows/`). Eu preparo/valido; Gabriel promove.
- **5.2** Ativar `install.touring.dev.sh`: substituir placeholders por URLs/hashes reais emitidos pelo release pipeline; remover o `exit 2` skeleton.
- **5.3** Packaging: preencher Homebrew/Scoop/Docker com hashes reais (saída do pipeline). `touring-license` JWT-verify atrás de feature → ativar quando houver decisão comercial.
- **5.4** GA: tag SemVer (Fase 0.3), CHANGELOG via release-plz, smoke-test do tarball (já em `release.yml`).

**Efeitos**: (1ª) `curl install.touring.dev | sh` instala uma toolchain pinada. (2ª) Touring deixa de depender de build-from-source local; distribuição reprodutível e assinada (supply-chain).

**Script `validate_phase5.sh`**: `install.touring.dev.sh --dry-run` produz plano coerente com URLs reais; verificação de assinatura cosign (quando promovido); `cargo-semver-checks` + `cargo-deny` GREEN; tarball smoke-test (extrai + `touring --version`).

---

## 4 · Impactos sistêmicos — desdobramentos de 1ª e 2ª ordem (multi-perspectiva)

| Perspectiva | 1ª ordem (direto) | 2ª ordem (desdobramento) | Mitigação |
|---|---|---|---|
| **Isolamento/Correção** | daemon/índice/memória per-project | fim da contaminação cross-workspace; RL/memory por domínio | opt-in (Fase 1.5) evita disrupção do vivo |
| **Recursos** | N daemons × ~92 MB RSS | pressão de memória com muitos projetos abertos | opt-in + `TOURING_IDLE_TIMEOUT_SECS` + `daemon-ctl list-all` p/ governança |
| **Versionamento** | pin por projeto + rollback | evoluir Touring sem arriscar todos os projetos; pilotar vN+1 | lockfile determinístico (Fase 3.4) |
| **Segurança (supply-chain)** | release assinado (sigstore) + SBOM | instalação verificável; tiers de licença | cosign verify no install (Fase 5.2) |
| **REGRA #19 (process hygiene)** | N daemons → risco de kill colateral | necessidade de alvo-por-socket | daemon-ctl nunca pkill; targeting por socket (Fase 1.4) |
| **REGRA #11 (git proibido)** | promoção W13.5/6 + move da fonte são git | fronteira clara: eu construo infra, Gabriel faz git | toda fase marca o "git boundary" explicitamente |
| **DX / co-evolução** | docs↔código mudam juntos (Fase 4.3) | drift = débito; CLAUDE.md/rules acompanham | cross-ref de paths no validate de cada fase |
| **Disco** | 1,6 GB globais → per-project | duplicação de DBs entre projetos | migrate-from-global copia subset; dedup futuro |

---

## 5 · Sequenciamento, dependências e estimativa

```
Fase 0 (fundação) ──► Fase 1 (daemon, BLOCKING) ──► Fase 2 (install lifecycle) ──► Fase 3 (update propagation)
       │                                                                                    │
       └────────────────────────────────────────────────► Fase 4 (canonical repo) ◄────────┘
                                                                    │
                                                                    ▼
                                                           Fase 5 (distribuição GA)
```
Acíclico. Fase 0 destrava 1 e 4. Fase 1 é o bloqueador interno. Fase 3 precisa de 0 (pin) + 1 (daemon) + 2 (bin). Fase 4 precisa de 0 (desacople). Fase 5 precisa de 3+4. **Estimativa total**: ~8-11 dias de execução + 2 ações git de Gabriel (W13 promoção, move da fonte). **Cadência**: sessão a sessão, uma fase por vez; cada fase entrega código testado + `validate_phaseN.sh` GREEN + `update-touring` exit 0 + memory store + RL reward antes da próxima.

---

## 6 · Riscos (P×I) e mitigações

| Risco | P×I | Mitigação |
|---|---|---|
| Disrupção do `~/.claude/rust` vivo + sessões CC concorrentes | **ALTO** | opt-in default OFF; dev/teste em TempDir + `TOURING_DAEMON_SOCKET`; nunca tocar o socket vivo; validate re-verifica o vivo inalterado |
| Move da fonte quebra paths/hooks silenciosamente | **ALTO** | Fase 0 elimina hardcodes ANTES; fonte antiga mantida até E2E; rollback documentado (Fase 4.4) |
| Lock per-socket quebra detecção single-daemon / reset errado | MED | derivar lock do socket consistentemente em bind+acquire+reset; unit test (1.1) + reset-by-derived-path (1.4) |
| Unificar resolvers introduz ciclo de crate | MED | fonte única em `touring-foundation` (leaf); verificado sem back-edge |
| Propagação de update corrompe dados do projeto | MED | `--dry-run`; backup `.bak.<ts>` (padrão migrate-from-global); rollback por pin |
| Promoção W13/move da fonte sem coordenação git | MED | fronteira git explícita; eu paro no boundary, Gabriel executa git |
| N×RSS com muitos projetos | MED | opt-in + idle watchdog + observabilidade `list-all` |

---

## 7 · Verificação end-to-end (o gate de cada fase + o gate do programa)

1. **Por fase**: `validate_phaseN.sh` (gerado COM a fase) GREEN — auditando o fluxo completo da funcionalidade E re-validando **toda citação file:line/normativa** (cross-audit de dupla validação, exigência de Gabriel).
2. **Gates de código** (toda fase): `cargo check --workspace` (feat ON+OFF) + `cargo clippy --workspace -- -D warnings` + `cargo test` (crates tocados) — 0 falhas (REGRA #21).
3. **Gate 50-dim**: `touring-quality score <arquivos> --format json` ≥ Gold + 6 P0 = Pass.
4. **Deploy**: `update-touring` exit **0 real** (via `exit $rc`, nunca `| tail; echo $PIPESTATUS`); daemon fresco (não `(deleted)`).
5. **Prova runtime**: o critério da fase executado de fato (ex.: `lsof` mostra N sockets; `touring update` troca a versão; install dry-run coerente) — prova em prática, não claim.
6. **Persistência**: `touring memory store "productization:phaseN:<date>" … --tier semantic --type lesson` + `touring learning reward orchestrate 1.0`; marcar progresso em `docs/plans/touring-premium-refactor-2026/00-INDEX.md`.
7. **Salvaguarda recorrente**: enquanto a fonte não migrar, re-verificar que o daemon de `~/.claude/rust` permanece no socket global e que nenhuma sessão CC concorrente foi afetada (REGRA #19).

---

## 8 · Fronteiras explícitas (o que NÃO faço)

- **Git** (REGRA #11): não executo `git mv`/commit/promoção. Construo a infra e **paro no boundary**; Gabriel faz o move da fonte (Fase 4) e a promoção W13.5/6 (Fase 5).
- **Decisões comerciais**: numeração SemVer (Fase 0.3) e ativação dos tiers de licença (Fase 5.3) = ASK Gabriel na execução.
- **Escopo**: implemento fase a fase com sua aprovação entre fases; este Pln2 é o mapa, não um commit único.
