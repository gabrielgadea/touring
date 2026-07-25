---
type: OperationsManual
title: "Manual — Touring 100% local + descolamento do root global ~/.claude"
description: "Como a topologia per-project funciona em operação, e o mapa item-a-item de /home/gabrielgadea/.claude (o que fica, o que sai, o que transforma) com os passos ordenados, verificáveis e reversíveis do descolamento"
tags: [manual, per-project, descolamento, claude-root, pln2]
timestamp: 2026-07-25
plan: /00-INDEX.md
---

# Manual — Touring local & Descolamento do `~/.claude`

> Baseado em exploração REAL de `/home/gabrielgadea/.claude` em 25/07/2026
> (tamanhos medidos, hooks contados, acoplamentos verificados). Estado de
> partida: Pln2 completo (F0-F5+PILOT), release v30.3.0 publicada no GitHub,
> konverter operando per-project.

---

## PARTE 1 — Como tudo funciona localmente

### 1.1 As 4 camadas (o modelo rustup)

```
L1  FONTE CANÔNICA      ~/projects/touring          desenvolvimento; cargo build AQUI não afeta ninguém
      │  touring toolchain install --from-source . <versão> [--force]
      ▼
L2  TOOLCHAIN HOME      ~/.touring/
      ├── toolchains/<versão>/bin/{touring,touring-hook,touring-daemon,touring-quality}
      │                                             snapshots IMUTÁVEIS (cópias, não symlinks)
      ├── default                                   canal default (fallback do shim)
      └── config.toml                               user-layer config
      │  touring update --project <root>   (ou init-project)
      ▼
L3  SHIM CC             ~/.claude/hooks/touring-hook (ARQUIVO sh, instalado por update-touring)
      │  walk-up por evento de hook:
      │    1. <projeto>/.touring/bin/touring-hook   ← projeto pinado (via CLAUDE_PROJECT_DIR/cwd)
      │    2. ~/.touring/toolchains/<default>/bin/  ← toolchain default
      │    3. ~/.local/bin/touring-hook             ← canal dev
      │    4. fonte canônica target/release/        ← último recurso
      ▼
L4  PROJETO             <projeto>/.touring/
      ├── touring.toml        [toolchain] channel = pedido do HUMANO (nunca reescrito)
      │                       [daemon] per_project = true (opt-in daemon próprio)
      ├── toolchain.lock      estado da MÁQUINA: active + previous (rollback determinístico)
      ├── bin/                symlinks → L2 (touring update é o dono)
      └── daemon.sock         socket do daemon próprio (se opt-in)
```

**Resolução do canal ativo**: `lock.active` > `touring.toml pin` > dev.
**Resolução de binário** (por nome): `toolchains/<ativo>/bin/` > `~/.local/bin/`.
**Resolução de socket**: env `TOURING_DAEMON_SOCKET` > walk-up per-project
(exige opt-in `[daemon] per_project=true`) > global `/tmp/touring-daemon-1000.sock`.
**Dados do daemon**: `<root_do_daemon>/.claude/touring/*.db` — o root é pinado
NO SPAWN, derivado do próprio socket (`<root>/.touring/daemon.sock`), nunca
herdado do invocador (fix do PILOT).

### 1.2 Fluxos operacionais

**(a) Sessão CC dentro de um projeto pinado (ex.: konverter)**
Todo evento de hook entra pelo shim (L3) → resolve `.touring/bin/touring-hook`
do PROJETO → o hook conecta/auto-spawna o daemon do projeto (socket local,
dados em `<proj>/.claude/touring/`). A versão do Touring é a pinada — pode
diferir da global. Verdade local: `.touring/bin/touring --version`.

**(b) Sessão CC fora de qualquer projeto pinado**
Shim cai na toolchain default (L2) ou dev; daemon global; dados globais.
Comportamento idêntico ao histórico — o global é apenas "um projeto a mais".

**(c) Atualizar um projeto**
```bash
touring toolchain install --from-source ~/projects/touring 30.4.0   # nova versão na L2
touring update --project <root> --channel 30.4.0                    # re-link + lock + restart daemon
touring update --project <root> --rollback                          # volta ao previous, determinístico
touring update --all-projects                                       # itera o ProjectRegistry
```

**(d) Componentes opcionais**
```bash
touring component list --project <root>
touring component add touring-quality --project <root>
```

**(e) Daemons (REGRA #19)**
```bash
touring daemon-ctl list-all                                   # global + todos per-project
touring daemon-ctl status|restart|stop --socket <proj>/.touring/daemon.sock
```

**(f) Canal de release (GitHub)**
```bash
gh release download v30.3.0 --repo gabrielgadea/touring        # (repo privado: auth)
sh install.touring.dev.sh --version 30.3.0 --from-tarball touring-x86_64-unknown-linux-gnu.tar.gz
# repo público (futuro): curl https://install.touring.dev | sh   — sem auth
```

**(g) Gotcha permanente de sessão**: toda sessão CC exporta
`TOURING_DAEMON_SOCKET` (pinado no global) — precedência sobre o walk-up.
Para testar o comportamento per-project de dentro de uma sessão:
`env -u TOURING_DAEMON_SOCKET -u TOURING_DAEMON_SOCK <cmd>`.

---

## PARTE 2 — O mapa de `~/.claude` (medido em 25/07/2026)

Vereditos: **FICA** (Claude Code genérico — não é Touring) · **DELETE**
(cache/lixo regenerável) · **ARQUIVA** (histórico → repo ou `_archive/`) ·
**MIGRA** (dado vivo Touring → per-project/toolchain) · **TRANSFORMA**
(permanece, mas muda de papel).

### 2.1 Os gigantes (99% do espaço)

| Item | Tamanho | Veredito | Destino / ação |
|---|---|---|---|
| `rust/target/` | **242 GB** | **DELETE** | Cache de build do root CONGELADO — nunca mais builda ali. `rm -rf ~/.claude/rust/target` (exceção legítima ao safe-clean: workspace morto). **Recupera 242 GB.** |
| `rust/` (crates 2,2G + docs 232M + resto) | ~8 GB | **ARQUIVA→DELETE** | Fonte congelada (D4). Agora existe backup superior: o repo GitHub `gabrielgadea/touring` (commit e136e74+). Após teu OK: `tar -czf ~/claude-rust-frozen-final.tar.gz` do delta (docs não-migrados) → delete. |
| `data/touring_pipeline.db` | 4,9 GB | **MIGRA/DELETE** | Pipeline DB global Touring. Se nenhum fluxo ativo o lê (verificar `lsof`), arquivar e deletar; senão migra com o daemon global. |
| `touring/` (symbols 704M, graph 618M, models 545M, knowledge 163M, tantivy 176M, memory 98M, taco-planning 184M) | 2,6 GB | **TRANSFORMA** | Vira o data-root do "projeto global" (o daemon global continua usando). Projetos pinados param de tocá-lo naturalmente (dados próprios). `models/` (fastembed) pode ir p/ `~/.touring/share/models` (cache compartilhado entre projetos). |
| `projects/` | 1,6 GB | **FICA** | Transcripts + memória persistente do Claude Code. Não é Touring. |
| `plugins/` | 1 GB | **FICA** | Plugins CC genéricos. |
| `security/`, `file-history/`, `checkpoints/`, `debug/`, `telemetry/` | ~700 MB | **FICA** (podar antigos) | Estado do CC; poda por idade é higiene normal, não descolamento. |
| `hooks/{touring,touring-daemon,touring-hook}.old` | 250 MB | **DELETE** | Binários antigos pré-symlink. Lixo. |
| `plans/` | 169 MB | **ARQUIVA** | Planos históricos Touring (touring-47-to-13 43M, sprint-4-6 41M, pipeline-premium 77M) → `~/projects/touring/docs/plans-archive/` (viram história versionada do produto). Planos não-Touring ficam. |

### 2.2 Os acoplamentos de configuração (o coração do descolamento)

| Item | Conteúdo real | Veredito | Ação |
|---|---|---|---|
| `settings.json` hooks | **77 hooks, 60 apontam `$HOME/.claude/hooks/touring-hook`** | **TRANSFORMA (já resolvido)** | Os 60 continuam apontando o MESMO path — que hoje é o SHIM walk-up. Nenhuma edição necessária: o shim faz o roteamento per-project. Registro permanece como interface CC↔Touring. |
| `settings.json` env | `TOURING_PROJECT_ROOT`, `TOURING_WORKSPACE_ROOT` → `~/projects/touring`; `TOURING_PILLAR_INDUCTION_ARMED` | **TRANSFORMA** | Hoje corretos (apontam a fonte canônica). Quando o daemon global também for "um projeto" (`~/` pinado), podem sair; até lá ficam. `TOURING_DAEMON_SOCKET` NÃO está no settings (vem do processo da sessão) — nada a fazer. |
| `hooks/` (touring, touring-daemon = symlinks → novo root; touring-hook = shim; `touring_*.py`, `touring-cli-suggester.sh`, `touring-process-guard.sh`) | 252 MB (maioria é `.old`) | **TRANSFORMA + DELETE .old** | Symlinks/shim ficam (são a camada L3). Python hooks touring podem, em fase 2, mover para a toolchain (`share/hooks/`) e o settings apontá-los via shim-wrapper — hoje funcionam, não urgente. |
| `~/.local/bin/{touring,touring-hook,touring-daemon,touring-quality,update-touring}` | symlinks → fonte canônica | **TRANSFORMA** | É o "canal dev". Permanece como fallback L3.3 e para o teu uso direto no PATH. |
| systemd user units (`touring-daemon.service` etc.) | apontam novo root | **FICA** | Já co-evoluídos na F4′. |

### 2.3 Skills / rules / agents / commands (o "cliente TACO")

| Grupo | Itens | Veredito | Racional |
|---|---|---|---|
| `rules/` Touring (15: touring-cli-index, decision-matrix, 4-pillars, process-hygiene, per-project, rebuild, elite-50, disk-hygiene, entity-identity, VP-Scout, TACO-subagent, tool-combination, file-metadata, code-execution-gateway, touring-elite) | 824 KB | **DUAL** | São a interface TACO↔tu — carregadas TODA sessão. Ficam em `~/.claude/rules/` (client-side), mas a CÓPIA CANÔNICA passa a ser versionada no repo (`~/projects/touring/client/rules/`) e sincronizada por `touring update`/componente `taco-client`. Código→docs→skill juntos (co-evolução). |
| `skills/` Touring (17: Touring, TACO-*, touring-*, loop-engineering, taco-planning) | ~14 MB | **DUAL** | Idem: client-side em `~/.claude/skills/`, canônico versionado no repo como componente. |
| `agents/` touring-* (scouter/architect/engineer/auditor/scriber + parcer yamls) | 916 KB | **DUAL** | Idem. |
| `commands/`, skills não-Touring (markitdown, research, remotion, lexcore-*, etc.) | — | **FICA** | Não são Touring. |
| `CLAUDE.md` (constituição TACO) | 23 KB | **FICA** | É TEU, não do produto. Já contém o modelo per-project (§Per-Project + rules/touring-per-project.md). |

### 2.4 Restos e dados menores

| Item | Veredito | Ação |
|---|---|---|
| `data/{touring_knowledge,touring_symbols,semantic_recall,pre_task_scout_cache,rlm_memory}.db` (~65 MB) | MIGRA | Acompanham o daemon global (mesmo destino do `touring/`). |
| `tools/` (disk-watch, safe-clean, holon, taco-forge, cah-diagnostic) | TRANSFORMA | disk-watch/safe-clean atualizar TARGETS (rust/ sai, ~/projects/touring entra — já entrou). holon/taco-forge: decisão tua (taco-forge está DESCONECTADO desde 02/07). |
| `scripts/`, `lib/`, `downloads/` (Rn2/Rn3 docs), `_archive/`, `skills-archive/`, `backups/` | ARQUIVA seletivo | Docs-fonte Touring (Rn2/Rn3) → repo `docs/research/`; resto fica/poda. |
| `plans/giggly-drifting-kahn.md` (o plano Pln2) | ARQUIVA | → repo `docs/plans/` (já tem o bundle; mover o plano-mestre também). |
| `security_warnings_state_*.json`, `gpu_cache.json` etc. | FICA | Estado CC. |

---

## PARTE 3 — Os passos do descolamento (ordenados, verificáveis, reversíveis)

> Pré-condições JÁ CUMPRIDAS: fonte canônica movida (F4′) · daemon global no
> binário novo · shim ativo · per-project provado (konverter) · release GitHub
> publicada (backup externo da fonte).

### D1 — Recuperar 242 GB (imediato, zero risco)
```bash
# rust/ está FROZEN: nunca mais builda; target é 100% regenerável (e obsoleto)
du -sh ~/.claude/rust/target          # confirmar ~242G
rm -rf ~/.claude/rust/target
df -h /home                            # ver o salto
```
*Verificação*: `touring doctor -j` 6/6 (nada aponta para esse target — daemon/symlinks já usam ~/projects/touring). *Rollback*: nenhum necessário (cache).

### D2 — Lixo de binários antigos (250 MB)
```bash
rm ~/.claude/hooks/{touring.old,touring-daemon.old,touring-hook.old}
```
*Verificação*: `ls -la ~/.claude/hooks/ | grep touring` → só symlinks + shim.

### D3 — Arquivar planos e docs históricos Touring
```bash
mkdir -p ~/projects/touring/docs/plans-archive
mv ~/.claude/plans/{touring-47-to-13-residual,sprint-4-6-daemon-root-cause,pipeline-premium-elevator} \
   ~/projects/touring/docs/plans-archive/
cp ~/.claude/plans/giggly-drifting-kahn.md ~/projects/touring/docs/plans/
# commit no repo (teu ou meu, com tua ordem)
```

### D4 — Dados globais Touring viram "projeto global" (transformação, não mudança)
O daemon global JÁ é per-project de fato (root = quem o spawnou). Nada a mover
hoje: projetos pinados criam dados próprios; o `~/.claude/touring/` permanece
como data-root do contexto global e **encolhe por atrito**. Opcional futuro:
`touring migrate-from-global` por projeto que queira herdar o histórico.
`models/` → `~/.touring/share/models` quando quisermos cache compartilhado.
`data/touring_pipeline.db` (4,9 GB): verificar consumo real
(`lsof ~/.claude/data/touring_pipeline.db`); sem leitor ativo → arquivar+deletar.

### D5 — Descarte final do `rust/` congelado (DECISÃO TUA — D4 do plano)
Backup agora é o GitHub (fonte completa + tags). Quando autorizares:
```bash
# delta-check antes (garante que nada exclusivo ficou):
diff -rq --exclude=target ~/.claude/rust/crates ~/projects/touring/crates | head
tar -czf ~/claude-rust-frozen-docs.tar.gz -C ~/.claude/rust docs   # docs históricos
rm -rf ~/.claude/rust
```

### D6 — Componente `taco-client` (o DUAL — skills/rules/agents versionados)
Fase estruturada (própria sessão): criar `~/projects/touring/client/{rules,skills,agents}/`
com as cópias canônicas → `touring component add taco-client` materializa/sincroniza
em `~/.claude/`. A partir daí, editar regra TACO = commit no repo + update.
Até lá: `~/.claude/` segue sendo a cópia operante (nada quebra).

### D7 — Estado final + verificação global
```bash
du -sh ~/.claude                       # alvo: ~4-5 GB (era ~262 GB)
touring doctor -j                      # 6/6
bash ~/projects/touring/docs/plans/touring-productization-pln2/validate_pilot.sh   # 9/9
# sessão CC nova em projeto pinado E fora — ambas saudáveis
```

### O que RESTA em `~/.claude` ao final
Constituição (CLAUDE.md) · settings.json (hooks→shim, o contrato CC↔Touring) ·
rules/skills/agents (client-side, sincronizados do repo) · shim + symlinks em
hooks/ · projects/ (transcripts/memória CC) · plugins/ · estado CC
(file-history, checkpoints, cache, security...) · `touring/` como data-root do
contexto global. **Touring deixa de MORAR em ~/.claude; passa a ser um produto
instalado que o ~/.claude apenas CONSOME via shim + pins.**

---

## Apêndice — Números do inventário (25/07/2026)

`~/.claude` total ≈ 262 GB → rust 250 G (242 G target) · data 5,0 G ·
touring 2,6 G · projects 1,6 G · plugins 1,0 G · security 297 M · hooks 252 M
(250 M .old) · plans 169 M · file-history 115 M · demais < 200 M somados.
Hooks no settings.json: 77 (60 touring, todos via shim). Skills touring: 17 ·
rules touring: 15 · agents touring: 5+.
