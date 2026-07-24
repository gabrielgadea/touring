---
type: LoopLog
title: "Log — Produtização Pln2 (execução)"
description: "Histórico cronológico da execução do DAG task_1784891751372651360 (F4'→F1→F2→F3→PILOT→F5)"
tags: [productization, pln2, log]
timestamp: 2026-07-24
plan: /00-INDEX.md
---

# Log de execução

## 2026-07-24 — Human gate D1-D4 APROVADO · F4' iniciado

- **08:15 BRT** Gabriel aprovou D1 (move-first F4'), D2 (`~/projects/touring`),
  D3 (piloto konverter), D4 (`~/.claude/rust` = cópia congelada até E2E; descarte
  futuro é decisão dele). Memória: `productization:d1-d4-approved:2026-07-24`.
- **08:16** DAG criado: `task_1784891751372651360` (11 subtasks, F4-1..F4-6 +
  F1/F2/F3/PILOT/F5). Markers loop-engineering: sessão (`fd689064741e`) +
  projeto novo (`43224dc4d9af`), ambos `active`.
- **08:17** **F4-1 DONE** — rsync `~/.claude/rust` (sem `target/`, 5,9G,
  42 crates) → `~/projects/touring`; RSYNC_EXIT=0; sem `.git` na origem
  (repo novo será versionado por Gabriel, REGRA #11). Item de nome corrompido
  `]+` copiado como está (sinalizado).
- **08:20** **F4-2 DONE** — `~/.local/bin/update-touring` parametrizado:
  `RUST_WORKSPACE="${TOURING_WORKSPACE_ROOT:-${HOME}/.claude/rust}"`. Provas:
  `bash -n` ok; verify-only legado exit 0 (retrocompat); trace `bash -x` mostra
  `RUST_WORKSPACE=/tmp/fake-root-prova` sob env (parametrização real).
- **08:22** **F4-3 IN PROGRESS** — build frio `cargo build --release` no novo
  root (background; log no scratchpad `build-newroot.log`).
- Paralelo ao build:
  - Finding REGRA #21 fechado: testes do flow-guard poluíam o MARKER_DIR de
    produção → `LOOP_ENGINEERING_HOME` env override em `loop_marker.py` +
    fixture autouse `isolated_marker_dir`; suíte 25/25; 33 markers órfãos
    removidos.
  - `validate_phase4.sh` criado (8 checks, roda pós-cutover).
  - `coevolve_claude_configs.py` criado; dry-run: **93 arquivos / 140 hits**
    operacionais (memory/, taco-forge sessions e cah-diagnostic excluídos).
  - REGRA #12: `disk-watch.sh` TARGETS += `touring-canonical` (novo target já
    monitorado, 4,07 GB durante o build); `safe-clean.sh` limpa incrementais de
    AMBOS os roots durante a transição.

Próximos: F4-4 cutover (re-symlink + daemon restart) após BUILD_EXIT=0 →
F4-5 co-evolução (apply) → F4-6 validate_phase4.sh GREEN + congelamento D4.

## 2026-07-24T08:42:20.287910-03:00 — F4-6 done

F4' move-first COMPLETA: fonte canônica em ~/projects/touring (rsync 5,9G/42 crates; build frio 9m01s exit 0; cutover symlinks+daemon; co-evolução 93 configs com backup; index 252k símbolos paths relativos; validate_phase4.sh 8/8 PASS; e2e 0.8749 PASS do novo root; ~/.claude/rust congelado com FROZEN-2026-07-24.md). Gotchas novos: daemon-ctl status imprime em stderr; grep -q+pipefail=SIGPIPE 141 em traces; testes plan_scope feature-gated (pre-hooks). Gap para F1: PID file canônico vazio (daemon-ctl como fonte).

## 2026-07-24T10:23:23.577563-03:00 — F1 done

F1 W12.5 daemon multi-instancia COMPLETA: lock per-socket (FNV-1a estavel, global mantem nome legado); 7 resolvers unificados na foundation (fonte unica, +layer legacy SOCK); opt-in [daemon] per_project=true via walk-up (resolve socket ANTES de existir — chicken-egg resolvido); daemon-ctl multi (--socket/--project, list-all com registry gravado no bind, restart loud no-op guard); spawn pina socket resolvido; pid file canonico REGRA #19 escrito no bind (provado: /run/user/1000/touring-daemon.pid=1829171); flip 5 fallbacks workspace_root para ~/projects/touring; current_uid() publico na foundation. RED empirico -> GREEN 2/2 E2E + 4/4 unit; validate_phase1.sh 8/8; clippy 0; e2e 0.8749; deploy update-touring + daemon novo 6/6. Gotcha decompose-flush comprovado 2x (DAG perdido em restart, recriado task_1784899384232534031) -> F1-FLUSH pendente com evidencia. Gotcha novo: grep -q+pipefail SIGPIPE reintroduzido e re-pego no validate (padrao capture-then-grep obrigatorio).

## 2026-07-24T11:02:54.245786-03:00 — F1-FLUSH done

F1-FLUSH resolvido como F1-ROUTING: root cause NAO era flush (WAL duravel) — era roteamento per-project do decompose (task vive no store onde foi criado; cross-cwd get=not-found falso, update=orphan write 0-rows com sucesso falso). Fix: locate_task_store (local->global) em get/update/add/validate + erro loud para task inexistente. Provas: E2E 3/3 (novo test cross-cwd routing), prova de ouro com o task real do incidente (11 subtasks visiveis + update cross-store confirmado no arquivo via sqlite3), fantasma loud, validate_phase1 8/8 ALL PASS, clippy 0, deploy update-touring + daemon 1953601. Memoria do gotcha CORRIGIDA (superseded). Limitacao declarada: finalize/ready seguem local-store.

## 2026-07-24T11:30:39.808041-03:00 — F2 done

F2 install lifecycle COMPLETA: (2.1) populate_bin no init-project — .touring/bin/ symlinka a toolchain pinada ([toolchain] channel -> TOURING_HOME/toolchains/<ch>/bin) com fallback dev ~/.local/bin, fail-open com notas (11/11 unit tests, core _inner puro sem env races); (2.2) walk-up shim W12.6 ATIVADO como ~/.claude/hooks/touring-hook via update-touring (cp+chmod, não symlink; touring/touring-daemon seguem symlinks); shim Layer 4 corrigido: root congelado -> ~/.local/bin + canonical source; (2.3) settings project-aware ok (TOURING_PROJECT_ROOT novo root da F4'). Provas vivas: init-project popula 3 bins (canal dev), shim TRACE resolve layer 2 (project_bin) dentro do projeto e layer 4 (dev) fora, hook real CC-style exit 0, validate_phase2 7/7 ALL PASS, clippy 0, e2e ok. Fix colateral: teste env do daemon_ctl frágil (sessão exporta TOURING_DAEMON_SOCKET) -> testa precedência canônico>legacy salvando ambos.

## 2026-07-24T15:13:16.726881-03:00 — F3 done

F3 (touring update + component) COMPLETA — o núcleo da propagação: (3.1) touring update [ch|--rollback|--dry-run|--all-projects via ProjectRegistry|--no-restart] re-linka .touring/bin, grava toolchain.lock (requested-vs-resolved rustup-like: touring.toml nunca reescrito) e reinicia daemon per-project no binário NOVO via restart_socket_with_bin; (3.2) touring component list/add/remove (core não-removível REGRA #0); (3.3) toolchain install --from-source (ponte dev→toolchain p/ PILOT) + --from-url (curl; sigstore=F5); (3.4) lockfile {active,previous,updated_at,reason} atômico. Design: project_toolchain.rs = casa única channel↔lock↔bins (lição F-NEW-1 recursos pareados); resolução lock>pin compartilhada por init-project/update/component. Provas: RED→GREEN E2E 5/5 (binário real, HOME/TOURING_HOME isolados); unit 63; lib completa 1416/1416; clippy 0; 50-dim 0.898-0.953 (project_toolchain Diamond); 6 P0 PASS; deploy update-touring exit 0 doctor 5/5; validate_phase3.sh 8/8 ALL PASS com binário deployado incl. daemon global inalterado. Gotcha novo: grafo release-TEST do server com E0460/E0463 (fingerprints stale pós-move F4) — grafo normal compila; harness E2E migrado p/ debug (binário testado segue release); débito segue aberto p/ investigação de fingerprints.

## 2026-07-24T15:42:11.288017-03:00 — PILOT done

PILOT (D3) konverter per-project COMPLETO — 1ª instalação real end-to-end: touring toolchain init (~/.touring criado do zero) + install --from-source ~/projects/touring 30.3.0 (1ª toolchain versionada imutável, 4 bins) + default + init-project konverter (pin 30.3.0, bins linkados) + shim resolve project_bin em sessão real (TRACE, exit 0) + [daemon] per_project=true + daemon próprio auto-spawnado pelo hook DA TOOLCHAIN (exe pinado) + touring update com restart per-project + validate_pilot.sh 9/9 ALL PASS + daemon global intacto. 2 FINDINGS REAIS: (1) BUG CORRIGIDO — restart per-project herdava env do invocador (TOURING_PROJECT_ROOT/cwd do workspace de quem chamou → DBs no projeto errado, contaminação cruzada); fix: project_root_for_socket deriva root do próprio socket (<root>/.touring/daemon.sock) e spawn pina CLAUDE_PROJECT_DIR+TOURING_PROJECT_ROOT+cwd — correto por construção p/ todo caller (update/daemon-ctl); provado via /proc/<pid>/environ antes(root errado)/depois(root=konverter); unit 17/17 + redeploy + toolchain reinstalada --force. (2) doctor project_db é resolução CLIENT-side, não do daemon consultado — false-negative em diagnóstico multi-daemon; candidato a melhoria (daemon expor root próprio no health). Gotchas: sessão CC exporta TOURING_DAEMON_SOCKET (precedência sobre walk-up — testar com env -u); component list mostra *.old do dev dir (cosmético). CO-EVOLUÇÃO CLAUDE.md 3 camadas: criado ~/projects/touring/CLAUDE.md (fonte canônica: update-touring, ponte from-source, gates, débitos); reescrita seção Touring do konverter/.claude/CLAUDE.md (drift 2-gerações 'symlink to analise' → tabela per-project pin/lock/bins/daemon/dados + operação); delta da constituição root ~/.claude/CLAUDE.md PROPOSTO aguardando aprovação de Gabriel (modelo 4 camadas + regra de resolução + rule nova touring-per-project).

## 2026-07-24T15:59:15.124548-03:00 — F5 done

F5 (distribuição & versionamento) COMPLETA na metade não-git: (5.2) install.touring.dev.sh ATIVADO — de skeleton exit-2 para instalador funcional rustup-like: 3 fontes (release channel/--from-url/--from-tarball offline), sha256 OBRIGATÓRIO no canal de release + automático quando .sha256 existe, cosign/sigstore quando bundle publicado (warn pre-GA), extract no layout toolchain bin/, default channel, PATH opt-in, fail-closed em tamper/bogus/latest-sem-versão; (F5-layout fix) release.yml CI empacota bin/ (era flat — incompatível com toolchain install; zero releases publicados = zero compat) + smoke path; package_release.sh local = artefato CI-idêntico do target/release; (5.1) staged release-plz.yml + docs-rs-mirror.yml validados YAML + PROMOTION-README — prontos p/ promoção git; sigstore JÁ ATIVO no release.yml (cosign keyless id-token, mais adiantado que o plano registrava); (REGRA #21) cargo deny advisories RED no gate → remediação REAL: crossbeam-epoch 0.9.18→0.9.20 (RUSTSEC-2026-0204) + spin 0.9.8→0.9.9 e 0.10.0→0.10.1 (yanked) → advisories ok; validate_phase5.sh 10/10 ALL PASS; deploy final exit 0 + toolchain 30.3.0 reinstalada + konverter propagado via touring update (dogfooding da cadeia completa). GIT BOUNDARY (Gabriel): promover staged workflows, tag SemVer + CHANGELOG release-plz, publicar artefatos + DNS install.touring.dev. ASK: ativação comercial touring-license (5.3). Homebrew/Scoop aguardam URLs reais do pipeline (declarado).
