---
type: Handoff
title: VGP — passagem da coordenação do analise para as sessões do touring
description: Ponto de entrada único para a coordenação do plano VGP nas sessões do ~/projects/touring — ordens, documentos, DAG migrado, estado medido por fase, juízes e binários congelados, corpora, trabalho em voo, fila com o próximo comando e o aceite, regras, armadilhas e riscos.
plan_id: 2026-10-08-vgp-motor-base
timestamp: 2026-10-09T18:40:00-03:00
tags: ["#kind:plan", "#status:active", "#domain:harness"]
---

# VGP — passagem da coordenação para o touring (09/10/2026, ~21:40Z)

> Gabriel, 09/10: «estou pensando que esse trabalho no Touring deve ser feito pelas sessões do projects/touring e
> liberar as sessões do projects/analise para o trabalho que precisa ser feito aqui. Já ativei as sessões lá, uma
> Opus 5.5 coordenadora, uma kimi e uma minimax.»

**Este arquivo é o ponto de entrada.** Quem coordena daqui em diante: **touring-09** (Anthropic, coordenadora) ·
**touring-35** (Kimi K3 1M, motor) · **touring-4f** (MiniMax M3.1-Flash-Preview, lote/corpora) — identificadas por
código (`python3 ~/.claude/tools/sessoes_irmas.py --text --cwd ~/projects/touring`). A coordenação anterior era a
**analise-89**; as irmãs do analise (analise-0e Kimi · analise-25 e analise-68 MiniMax) estão sendo liberadas (§5).

## 1. Ordens do Gabriel em vigor (citadas)

O /goal: «prossiga até todo plano estar completo e perfeito e ao final me prove, na prática, que tudo funciona de forma
completa e perfeita». As doze ordens do plano, com as palavras dele, estão em [proposta.md](proposta.md) §«As ordens»
(1–12) — inclusive multi-linguagem («python, rust, ts, html, pelo menos») e «confira as versões de CDN e as classes CSS
referenciadas; e também inclua tsx». Restrições que valem sempre:

- **push só por ordem do Gabriel**; **nada commitado sem ordem dele** (o 30.4.95 foi commitado depois, por ordem).
- **nunca `update-touring`/`propagate` sem sinal** — o release 30.4.96 (§6, P3) é o próximo passo do plano aprovado;
  confirme o sinal com o Gabriel antes do `propagate`.
- **REGRA #19**: nunca `kill`/`pkill` em processo touring (só `touring daemon-ctl`); **REGRA #11**: git destrutivo só
  com o ritual, **nunca `stash`**; **nunca `rm -rf` dentro de script**.
- **REGRA #21**: toda falha observada é corrigida; **REGRA ZERO**: ausência ≠ zero — controle positivo em toda medição.
- Subagente sempre com `model` (o hook `agent_model_gate` recusa sem); **irmãs Kimi/MiniMax ficam no próprio modelo**
  e a entrega delas é hipótese até o juiz da coordenadora aprová-la. Implementação com spec fechada → subagente
  `sonnet` (ordem de 09/10: «utilizar a delegação para agentes com outros modelos, não somente para as outras sessões»).
- Não mexer em `~/.claude/settings.json` sem confirmar com o Gabriel.

## 2. Documentos (o que é cada um)

| arquivo | papel |
|---|---|
| [proposta.md](proposta.md) | **plano de registro** — ordens, medições, motor, §6 fases com estado, §7 riscos/sequência, §8 erros, §9 artefatos, §10 multi-linguagem |
| [VL0-multilinguagem-spec.md](VL0-multilinguagem-spec.md) | contrato que Python/Rust/TS/TSX/JS/HTML/CSS implementam (adaptador, vocabulário de classes, P4 generalizada, corpus, aceite) |
| [V3-camada2-spec.md](V3-camada2-spec.md) · [V0d-spec.md](V0d-spec.md) | spec da camada 2 (pyright persistente) · do corpus de replay |
| [V6-ponteiro-vgp-protocol.md](V6-ponteiro-vgp-protocol.md) | o ponteiro que substitui `~/projects/analise/.claude/rules/vgp-protocol.md` (já com a cobertura real por linguagem) |
| [RETOMAR-AQUI-2026-10-09.md](RETOMAR-AQUI-2026-10-09.md) | **diário da coordenação**, adendos datados com evidência (o detalhe de tudo abaixo) |
| [RELATORIO-V1V2V5-2026-10-09.md](RELATORIO-V1V2V5-2026-10-09.md) | relatório da Kimi (motor camada 1 + wiring da camada 2) |
| [ESTADO-V0e-kimi.md](ESTADO-V0e-kimi.md) · [RELATORIO-V0d-2026-10-09.md](RELATORIO-V0d-2026-10-09.md) · [RETOMADA.md](RETOMADA.md) | histórico V0e · V0d · retomada inicial |
| `~/projects/touring/docs/vgp-motor.md` | manual do motor (modo sombra/ativo, kill switches, registro) |
| [diagnostics/instrumentos-2026-10-09/README.md](diagnostics/instrumentos-2026-10-09/README.md) | os instrumentos que produziram os números de hoje |

## 3. O DAG

**Novo, fonte da verdade: `task_1791581548206685387`** no daemon **global** (o touring não tem daemon per-project).
Consulta: `cd ~/projects/touring && touring decompose get task_1791581548206685387`. ⚠ Uma sessão iniciada no analise
exporta `TOURING_DAEMON_SOCKET` do analise: de lá, use `env -u TOURING_DAEMON_SOCKET -u TOURING_DAEMON_SOCK`.

Migrado por [scripts/migrar_dag_para_touring.py](scripts/migrar_dag_para_touring.py): 27 subtarefas com id, status,
dependências, descrição e prioridade idênticos à origem (comparação por código), válido e sem ciclos. A origem
`task_1791484992466538442` (daemon do analise) ganhou a subtarefa `MIGRADO` (completed) com o endereço novo; os
status dela congelaram às 21:30Z e não são mais atualizados. Status no DAG novo às 21:30Z: 8 completed · 7
in_progress · 10 pending · 2 failed (D1/D2, premissa refutada). **Atualize V2/V3/V5 depois do veredito P1.**

## 4. Estado medido por fase (detalhe e evidência no RETOMAR-AQUI)

> **Atualização 21:55Z — o held-out disjunto REVERTEU o «PRONTO» abaixo para fins de V4**: 1.200 negativos fora da
> amostra, e935 com 5/600 (arquivo) e 2/600 (inline) falsos deny — limite superior 95% 1,74% e 1,05%, «≤ 1%» NÃO
> provado; 3 falsos são defeito real e geral do coletor de atributos de módulo (`np.array`, `Image.LANCZOS`, atributo em
> `try/except`). Não bloqueia o release (sombra não nega); bloqueia a V4. Detalhe:
> [diagnostics/holdout/RELATORIO.md](diagnostics/holdout/RELATORIO.md).

| fase | estado | evidência |
|---|---|---|
| V0a–V0d, V0e1–V0e4 | feitas | proposta §6 |
| V0e1b G1 sem replay | código aceito (`cli_suggester.rs` sha `c79f12b0`, 9 testes, mutantes M-a…M-e mortos) | fecha com a prova [4] no binário instalado |
| V0e5 | remedir pós-deploy com janela maior | `diagnostics/v0e5-remedicao/remedicao.py` (exige `--ate`) |
| V1 | código pronto (`cli_ast_detail`) | fecha no deploy |
| **V2** camada 1 | **juiz rc 0 PRONTO, confirmado pela coordenadora** no binário congelado **`e935aaf1`**: vivo 11/13 · 20/27 · falso deny **0/400 · 0/300** · p95 108,4 ms; corpus com evidência 11/13 · **21/27** · 0/400 · 0/274 · p95 120,4 ms | `diagnostics/juiz-e935/k_v2_vivo.json`, `k_v2_ev.json` |
| **V3** camadas 1+2 | **juiz rc 0 PRONTO, confirmado**: vivo 12/13 · 22/27 · 0 falsos · p95 907,9 ms; evidência 12/13 · **23/27** · 0 falsos · p95 **954,2 ms** (teto 1.000 — apertado) | `k_v3_vivo.json`, `k_v3_ev.json` |
| prova [1] (8 células P4) | **rc 0, 8/8**, confirmado no `e935aaf1` | `diagnostics/juiz-e935/juizes_kimi.log` |
| V4 | pendente (deploy + ativação + 5 sessões) | `scripts/juiz_v4_sessoes.py` + `fixtures_v4/` prontos |
| V5 inline | lexer [scripts/shell_inline.py](scripts/shell_inline.py) aceito (sha `b6acd415`, 107/107, 7/7 + 36/36 mutantes); corpora religados por **evidência do traceback** (61 testes) — staging pronto, **promoção pendente (P2)** | `diagnostics/juiz-e935/juiz_lado_a_lado.json` |
| V6 | ponteiro pronto (cobertura por linguagem honesta); aplicar pós-deploy | V6-ponteiro |
| VL0 · VRS · VTS · VTSX · VHTML · VCDN · VCSS | pendentes; spec VL0 escrita; corpora em construção (§5) | proposta §10–10.1 |
| J | pendente | `scripts/juiz_plano_vgp.py` + `scripts/prova_vgp_motor.py` |

**Juízo da coordenadora sobre o relatório final da Kimi** (V1+V2+V5): números confirmados; o relatório precisa de três
correções — «17 mutações» vs 23 listadas; a §0 ainda diz que a camada 2 «é dela, pluga quando…» (já está ligada:
`touring-pyright` + `vgp_l2.rs` + CLI); `__version__` (e provavelmente `defaultdict`) está classificado como lacuna do
motor, mas é **defeito de corpus** — o snap vivo guarda o `-c` errado e o corpus com evidência o resolve (21/27). Lacunas
menores, só da camada 1 (a camada 2 cobre): `json.loadz` passa sem achado; `read_textz` em `/tmp` só avisa.

## 5. Juízes, binários, corpora e trabalho em voo

**Juiz do motor**: [scripts/juiz_motor_vgp.py](scripts/juiz_motor_vgp.py) sha `b7f7fd9b` — `classe_erro` agora é por
linguagem (Rust/TS exigem o código do compilador; classes web exigem a mensagem do oráculo; `origem: "semeado"` no
balde arquivo; 4/4 mutantes mortos; Python idêntico por id). **Todo número de juiz vem de cópia congelada do binário
com o sha ao lado** (lição de 09/10).

| artefato | caminho | sha / n |
|---|---|---|
| binário pré-wave | `~/.claude/touring/vgp/binarios/c8b503c4/touring` | `c8b503c4a10c58dd` |
| binário final da Kimi | `~/.claude/touring/vgp/binarios/e935aaf1/touring` | `e935aaf17d9a09d0` |
| corpus V0d VIVO | `~/.claude/touring/vgp/corpus_v0d/` | manifesto `48f5b544` (intocado) |
| corpus V0d com evidência (a promover) | `~/.claude/touring/vgp/corpus_v0d_staging_evidencia/` | árvore `90b03f50` |
| corpus natural de Rust (aceito) | `~/.claude/touring/vgp/corpus_rust/` | `entradas.jsonl` `141ced89…`; 37 pos + 307 neg; corte 20:40Z |
| held-outs (em construção) | `~/.claude/touring/vgp/corpus_holdout/` (temporal: 77 arq + 189 inline) · `corpus_holdout_disjunto/` (meta 600 + 600) | — |
| corpora semeados (em construção) | `~/.claude/touring/vgp/corpus_semeado/<lang>/` + [scripts/corpus_semeado.py](scripts/corpus_semeado.py) | — |

**Em voo — onde cada resultado aterrissa:**

1. **Held-outs** (subagente da analise-89, termina sozinho): juiz v2 nos dois held-outs com o `e935aaf1`, falsos por
   construto, limite superior de 95% por balde. Relatório em `diagnostics/holdout/RELATORIO.md`. **Aceite**: falso
   deny ≤ 1% pelo limite superior no disjunto (0/600 → ~0,5%); o temporal entra declarado (77/189 só limitam a 3,9%/1,6%).
2. **Corpora semeados TS/TSX/JS/HTML/CDN/CSS** (analise-25 e analise-68, MiniMax): fecham a rodada em curso num ponto
   consistente e mandam o relatório a **touring-09**; o script é determinístico (semente 20261009), então a
   **touring-4f** retoma de onde parou. Aceite do brief: oráculo independente por item, ≥ 20 positivos por classe,
   ≥ 200 negativos por lang, determinismo, selftest com mutante por operador.
3. **Corpus de Rust**: entregue e aceito (37 positivos; para a VRS faltam atributo/kwarg por corpus semeado; os 14
   `introduzido: false` saem do recall do pre-edit e são reportados à parte).

## 6. Fila — próximo comando e aceite

| P | item | próximo comando / aceite |
|---|---|---|
| P1 | veredito final da entrega da Kimi | ler `diagnostics/holdout/RELATORIO.md`; aceite acima. Aprovado → DAG V2 e V3 `completed`, V5 depois de P2; pedir à **touring-35** as três correções do relatório (§4) |
| P2 | promover o corpus com evidência a VIVO | backup `cp -a ~/.claude/touring/vgp/corpus_v0d ~/.claude/touring/vgp/corpus_v0d.bak-2026-10-09` (nunca `rm`); trocar os diretórios à mão; rerodar v2/V3 no vivo → os números do §4 (evidência) |
| P3 | release **30.4.96** (Python: V1–V3 + G1 sem replay + `touring-pyright`) | os passos do 30.4.95 (`diagnostics/deploy-v0e5-2026-10-09/deploy.json`): `python3 docs/elite_aggregate.py --check` (≥ Gold) · `cargo clippy --workspace --all-targets -- -D warnings` · `cargo fmt --all -- --check` · suítes de touring-code, touring-pyright (+ `--ignored`), touring-lsp, touring-server, touring-hook-handlers, touring-hooks, touring-cli · bump em `[workspace.package]` · `scripts/propagate-release.sh 30.4.96 --dry-run` → `--stop-before-propagate` → provas → **sinal do Gabriel** → `--skip-gates --skip-build --skip-freeze`. Rollback: `--rollback` |
| P4 | provas pós-deploy, no binário INSTALADO | `python3 scripts/prova_vgp_motor.py` (partes 1–4; os REDs do 30.4.95 em [2] «registro ausente» ×5 e [4] replay/additionalContext/lote/`touring run` precisam virar verdes) · `prova_code_mode_ceg` 41/41 · teste ao vivo do `additionalContext` do G1 (3–4 greps numa sessão real) · **rerodar os juízes v2/V3 no binário instalado** (ele não é o `e935aaf1`) |
| P5 | drift: 8ª reconciliação | depois do deploy o C4 `vgp` sai e o C1 `30.4.96` vira corrente: `python3 scripts/test_drift_coverage.py` rc 0 com o ledger `docs/plans/2026-10-08-drift-reconciliacao/ledger.jsonl` |
| P6 | V6 | aplicar `V6-ponteiro-vgp-protocol.md` em `~/projects/analise/.claude/rules/vgp-protocol.md` e ajustar a linha do VGP em `~/projects/analise/.claude/CLAUDE.md`; drift verde |
| P7 | V4 | `[vgp] modo = "ativo"` em `~/projects/analise/.touring/touring.toml` + `diagnostics/v4_ativacao.json {"desde": <ISO-Z>}` + `scripts/juiz_v4_sessoes.py` (5 sessões reais, temporal) |
| P8 | V0e5 | `diagnostics/v0e5-remedicao/remedicao.py --ate <congelado>` com a janela pós-deploy |
| P9 | multi-linguagem | VL0 (spec pronta; começa depois de V2/V3 fecharem) → VRS (sonda de tempo/RSS do rust-analyzer no workspace touring, nunca concorrendo com juiz) → VTS + VTSX → VHTML + VCDN → VCSS |
| P10 | J | `scripts/juiz_plano_vgp.py` rc 0 + prova prática + memória (`~/projects/analise/scripts/memoria/grafo_memoria.py` para o grafo do analise; `touring memory store`) + `touring learning reward` + relatório ao Gabriel |

## 7. Armadilhas medidas hoje (cada uma custou um veredito falso ou quase)

- **Env herdado**: sessão do analise exporta `TOURING_DAEMON_SOCKET`/`TOURING_PROJECT_ROOT` — sonda de hook com o env
  errado devolve `{}` com o erro só no stderr. Use o env de uma sessão real do projeto alvo.
- **Binário mais novo que a fonte, sem a fonte**: o build começou antes da edição. Congele a cópia e anote o sha; prove
  frescor por comportamento (sonda de controle positivo), nunca por mtime nem por um «Fresh» posterior.
- **Sandbox do `touring run` cego a `~/.claude/projects`**: glob devolve 0 em silêncio; `os.access` mente True. Rode
  nativo (`TOURING_CODE_MODE=native`) tudo que lê transcripts.
- **`pgrep -f` + filtro de cmdline casa o próprio shell**: nunca `pgrep -f` + `kill` no mesmo comando; exclua `$$`/`$PPID`.
- **Gate F2.1 casa `<script>` em string Python** (falso XSS): ponha conteúdo em arquivo de dados, não disfarce o literal.
- **`python_pre_gate` nega Edit que chama função ainda não definida**: definição e uso no mesmo Edit.
- **Id escopado no DAG**: `subtask_id` é `task_…::X` — filtrar por `X` puro dá zero.
- **Gates de code mode** vivem no hook `cli-suggest`, não no `pre-bash`; S3 avisa na 2ª inspeção; o 1º disparo do G1
  no fluxo real é na 4ª.

## 8. Riscos abertos (decisão do Gabriel)

- **O código do motor NÃO está no git**: `crates/touring-code/src/vgp/`, `crates/touring-pyright/`,
  `crates/touring-server/src/cli/vgp.rs`, `crates/touring-hook-handlers/src/hooks/vgp_l2.rs` e os documentos de hoje
  são **não rastreados**; 26 arquivos rastreados modificados; branch `feat/2026-09-16-shell-quality-wiring-python`,
  HEAD `c118aa52`, 254 commits sem push. Um `checkout`/`clean` perde o trabalho. Recomendação: commit WIP em branch
  `safety/` assim que o Gabriel ordenar (Git Safety Protocol: commit é aditivo; nunca `stash`).
- **p95 da V3 a 46 ms do teto** (954,2/1.000 ms) — medir no binário instalado com a máquina quieta.
