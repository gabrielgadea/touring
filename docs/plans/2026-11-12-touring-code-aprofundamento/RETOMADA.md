# RETOMADA — aprofundamento do touring-code + cross-audit

## PARADA DE 13/11/2026 08:10 BRT — LER PRIMEIRO (substitui o que abaixo disser o contrário)

Gabriel: «Faça o commit e o deploy», depois «preciso desligar, pare e salve tudo». **O commit está feito; o
deploy foi interrompido no meio do gate 1 do rito e NÃO instalou nada pelo caminho sancionado.**

**Commits (branch `feat/2026-09-16-shell-quality-wiring-python`, 5 à frente do `origin`, sem push):**
`215b15c3` código C1–C8 + F1–F16 (135 arquivos) · `42600e54` documentos · `74d5b074` versão 30.4.110 ·
`01c3ab90` `docs/reference/modules.md` regenerado (gate do rito acusou drift) · `74260484` teste do rito que
ainda procurava o item (e) no `CLAUDE.md` (movido em 11/11 para `docs/propagate-release-gotchas.md`).
Árvore de trabalho limpa de arquivo rastreado.

**Onde o rito parou** (`scripts/propagate-release.sh 30.4.110 --stop-before-propagate`, 3ª tentativa, log em
`~/claude-artefatos/touring/propagate-30.4.110-etapa1.log`): `cargo check`, clippy, espelho `client/`,
referência, testes Python do rito, medição r3 e **build de release 30.4.110** passaram; dos 30 pacotes de
teste, 13 entraram (inferlets … touring-hooks), 0 falhas, e o de `touring-hooks` foi interrompido. Os passos
2 (`update-touring`), 3 (toolchain), 4 (default) e 5 (projetos) **não rodaram**.

**ESTADO MISTO, atenção:** `target/release/` já tem os binários 30.4.110, e `touring` no PATH é symlink para
lá (`touring --version` → 30.4.110). O daemon que estava de pé era o 30.4.109; **depois do boot o daemon
global sobe do `target/release/touring-daemon` novo**, com o `touring-hook` de `~/.claude/hooks/` ainda antigo
(cópia) e sem `update-touring`/verify. Não existe toolchain `30.4.110` em `~/.touring/toolchains/`. Primeira
coisa na retomada: `touring daemon-ctl status`, `touring doctor -j`, e então rodar o rito inteiro de novo.

**Retomar, em ordem:**
1. `scripts/hw-slot --status`; depois `scripts/propagate-release.sh 30.4.110 --stop-before-propagate` (o cache de
   debug foi limpo: 255 GiB liberados; o primeiro `cargo check` é frio, ~8 min). Cada job `rust` precisa de
   `rust-1` + um `heavy`.
2. **NÃO propagar para o `analise` sem o "livre" da sessão dona** (analise-76 pediu: há uma medida em curso;
   trocar o binário pinado muda o ambiente dela). Etapa 2 só depois: `--skip-gates --skip-build --skip-freeze`.
3. `touring index rebuild`, depois o juiz (comando na seção "Próximos passos"); esperado `orphans_base` verde.
4. Atualizar relatório e esta nota com o veredito; decidir o `git push` (não pedido).

**Auxiliares (pedido do Gabriel: usar Kimi e MiniMax), entregas em `~/claude-artefatos/touring/irmas/`, ambas
hipótese até conferir:**
- MiniMax, docs defasadas: conferi 1 achado — `~/.claude/rules/touring-cli-index.md:37` (e o espelho em
  `client/rules/`) usa `jq '{index, orphans, rl, composite_health_score}'`, e `orphans`/`rl` não existem no
  status (as chaves são `.wiring.orphan_count` e `.learning.ema_reward`). Não corrigido.
- Kimi, revisão do `215b15c3`: R1 do F14 pode creditar função livre homônima no mesmo arquivo do tipo (o
  filtro aceita `function`, não só `method`); caminho de edit limpa e regrava sem transação; `r#` continua no
  resolvedor quando não há arquivo de origem; `crate_prefix` entra por `format!` no SQL. Nenhum conferido por
  execução; viram ticket no esforço de pendências.

Snapshots `safety/wip-aprofundamento-20261113` e `…20261113b` continuam válidos, mas o código agora está nos commits.

---

# Estado anterior (13/11/2026 05:55 BRT, antes do commit)

Pedidos do Gabriel que regem a frente: «todas as sugestçoes de arquitetura estão aprovadas, implemente tudo»,
`/TACO-cross-audit`, «ligar o escritor do grafo de dependências e alinhar os seis leitores , aprovado»,
«retome e prossiga». **Nada está commitado nem instalado.** Tudo vive na árvore de trabalho da branch
`feat/2026-09-16-shell-quality-wiring-python` (HEAD `90befad5`, base da frente `a3bfb950`) e no snapshot abaixo.

## Estado

| Item | Estado | Evidência |
|---|---|---|
| C1–C8 (candidatos de aprofundamento) | fechados | notas `C1-desenho.md` … `C8-desenho.md` (C8: §12 as built, §13 regra de aresta) |
| F1–F12 (correções do cross-audit) | fechadas | `docs/audits/cross-audit-2026-11-12.md` |
| F13–F16 (rodada final de 13/11) | fechadas | relatório, seção "Rodada final"; `fix-f13-f14.json` |
| Gates do workspace (G1) | verdes, medidos pelo lead com a árvore parada, 13/11 05:35–05:52 | `~/claude-artefatos/touring/g1/summary.txt` |
| Script de artefatos do plano | rc 0 | `docs/plans/2026-11-12-work-outer/audit-plan-completion.sh` |
| DAG `task_1794458342745990025` | 13/13 subtarefas concluídas | `touring decompose get` |
| Juiz de convergência | **exit 1**: 7 de 8 cláusulas verdes | abaixo |

G1: `fmt` exit 0 · `clippy --workspace --all-targets -D warnings` exit 0 · `cargo test --workspace` 315 alvos,
18.393 passando, 0 falhas, 173 ignorados · handlers (com features) 850/0 · storage (`knowledge`) 336/0 ·
`pytest scripts/test_gen_sdk.py` 16 passando.

## Juiz

Passagem final, 13/11 05:55 (`~/claude-artefatos/touring/g1/juiz-final.out`):

```
🔄 CONTINUE  task=task_1794458342745990025
  ✅ judge_intact   ✅ dag_done 13/13   ✅ quality_gold Platinum 0.9353474   ✅ no_p0_fail
  ✅ measured_whole_scope   ✅ cargo_green   ✅ cross_audit rc=0
  ❌ orphans_base   scoped orphans=706 baseline=709; NEW: 4: references.rs::new,
                    sdk_signal_mirror.rs::from_env, trait.rs::as_str, mod.rs::parse
JUIZ_EXIT=1
```

Os quatro são falsos (têm consumidor, conferido por grep) e o conserto do detector está na árvore (F14, F15):
na sonda sobre cópia do banco, os quatro ganham consumidor e os órfãos do escopo vão de 706 a 542, com 0
entradas novas. A cláusula lê `touring wiring orphans` do **binário instalado** sobre o índice que ele escreveu,
então **só fecha depois de deploy + `touring index rebuild`**. A linha de base do juiz
(`docs/plans/2026-11-12-work-outer/.baseline/orphans-scoped.txt`) não foi editada.

## Próximos passos, em ordem (todos dependem do sim do Gabriel)

1. Decidir a forma do commit (ticket D-05 do esforço de pendências). Commits por caminho, nunca `git add -A`;
   incluir os arquivos novos (`git status --short | grep '^??'` sob `crates/` e `scripts/`) e as três remoções.
   Fora do commit: `crates/GateId` (lixo não rastreado).
2. Bump de versão + `scripts/propagate-release.sh <versão>`.
3. `touring index rebuild` (≈13 min; o índice vivo foi escrito pelo detector antigo).
4. Rodar o juiz de novo: `python3 scripts/hw-slot --wait 3600 -- python3 ~/.claude/skills/loop-engineering/scripts/loop_converged.py --task task_1794458342745990025 --scope ~/projects/touring --bundle docs/plans/2026-11-12-work-outer`.
   Esperado: `orphans_base` verde (subconjunto da linha de base). Se o G1 não for refeito depois do bump, o
   script de artefatos acusa "fonte mais nova que o início dos gates": rodar
   `scripts/hw-slot --class rust --wait 3600 -- bash ~/claude-artefatos/touring/g1-gates.sh` antes.
5. Conferir no daemon o que só o deploy mostra: `touring status -j` de `~` (F9), `file_edges` no status, o texto
   do sinal de pré-leitura num arquivo sem importador.

## Decisões abertas

Lista completa em `docs/audits/cross-audit-2026-11-12.md`, seção "Decisões abertas com Gabriel". Achados medidos e
não corrigidos têm ticket em `docs/plans/2026-11-13-touring-code-pendencias/` (DAG `task_1794549390311758325`:
S-01 cobertura do `index` no e2e, S-02 smells sem dono, S-03 ramo de reentrada do cache de parsers, S-04 classe
de evidência do wiring, D-05 commit/deploy/ativação).

## Snapshot

Branch `safety/wip-aprofundamento-20261113` → commit `5c0aa5fc` (pai `90befad5`), gravado com índice temporário
(`git commit-tree`): não alterou a árvore, o índice real nem a branch atual. Cobre `crates/` (menos
`crates/GateId`), `scripts/`, `Cargo.lock`, `GLOSSARY.md`, esta pasta, os bundles `2026-11-12-work-outer` e
`2026-11-13-work-outer`, o esforço de pendências e o relatório do cross-audit. É a árvore do G1 verde. Um
segundo snapshot, `safety/wip-aprofundamento-20261113b` → `7f6bb4e2`, tem o mesmo código (diff vazio em
`crates/`, `scripts/`, `Cargo.lock`) e os documentos de fechamento escritos depois; este arquivo ainda recebeu
esta frase depois dele. Servem
para recuperar arquivo (`git show safety/wip-aprofundamento-20261113b:<caminho>`), nunca para merge. O snapshot
anterior (`safety/wip-aprofundamento-20261112`, `8d2517fd`) é de antes do C7 final, do C8 em produção e de F13–F16.

## Onde está a evidência

| O quê | Caminho |
|---|---|
| Relatório do cross-audit | `docs/audits/cross-audit-2026-11-12.md` |
| Relatórios dos engenheiros, logs RED/GREEN, mutantes, sondas | `~/claude-artefatos/touring/cross-audit-20261112/` (`fix-f13-f14.json`, pasta `f13f14/`) |
| Gates do lead | `~/claude-artefatos/touring/g1/` (final), `g1-rodada2/` (13/11 01:53), `g1-rodada1/` (primeira) |
| Script dos gates | `~/claude-artefatos/touring/g1-gates.sh` |

## Não tocar

`crates/GateId` (lixo não rastreado). Sem deploy e sem commit sem o sim do Gabriel. Sem `git stash`.

## Sobras a limpar

- Cache de debug duplicado pela suíte do workspace: `/home` ficou em 54 GB livres (95% de uso, medido em
  13/11 05:57). `cargo clean --profile dev`
  ou `~/.claude/tools/safe-clean.sh incremental` quando a frente fechar.
- `~/claude-artefatos/touring/snapshot-20261112.index`, `snapshot-20261113.index`, `retomada-check.index`,
  `slotq-runner.sh` e `round2-then-g1.sh` (escritos e não usados: a cadeia entrou sem eles).
- `~/.cache` (97 GB) nunca foi inspecionado.
