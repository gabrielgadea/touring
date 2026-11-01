---
type: Archive
title: "Item 8 do CLAUDE.md — portfólio de fluxos ADW e suas expansões"
description: "Texto integral removido do CLAUDE.md do projeto na condensação de 2026-09-02, preservado sem edição."
tags: [claude-md, historico, arquivo]
timestamp: 2026-09-02T00:00:00-03:00
plan_id: 2026-08-18-graph-engineering-flow-portfolio
---

# Item 8 do CLAUDE.md — portfólio de fluxos ADW e suas expansões

> Removido do `CLAUDE.md` do projeto em 2026-09-02, na condensação que trouxe o arquivo de 332
> para perto das 200 linhas recomendadas pela documentação oficial do Claude Code. O
> invariante e os gotchas ficaram lá; o texto abaixo é a narrativa completa, preservada
> sem edição para quem precisar do racional.

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
   **Expansão do portfólio 28/08/2026** (bundle `docs/plans/2026-08-28-adw-specs-expansao/`):
   a library saltou para **19 specs** — 6 novos codificam o modus operandi que era manual:
   `release-gate` (4 gates medidos + pausa humana antes de deploy) · `memory-curation`
   (memórias antigas verificadas contra o código, veredito stale/valid/resolved) ·
   `exercise-idle-infra` (KPIs STUB/afordâncias uso-zero → exercício de estreia por item) ·
   `code-mode-adherence` (régua M1 → calibração E/A/M por failure_kind) · `adw-curation`
   (o meta-ADW: cura a própria library por evidência) · `guard-sweep` (100% dos guards;
   a estreia achou 3 falhando e forçou as correções). Todos com prior-art create_new
   registrado, lint 0 erros e evidência comportamental em promotions.json.
   **Potencialização 28/08/2026** (bundle `docs/plans/2026-08-28-adw-potencializacao/`):
   `adw race` deixou de copiar `target/`/`.git`/`.claude` por lane (era N×~30GB);
   teto do sandbox de nó alinhado ao executor real (600s, guard D8 cruzado); lint novo
   `readonly_sem_sandbox` (leitor provado ganha o convite ao CEG — 3 fragments aplicados);
   nó `agent` ganhou `retries` de TRANSPORTE (exit≠0/timeout; teto A14=3 é ERRO de lint;
   veredito segue no gate); ZTE exercitado ao vivo (bypass conformal auditado, KPI 0→0.02);
   `error-teach` promovido à library (10 runs de evidência); KPI `plan_refine_iters`
   corrigido (produtor grava `{version,iterations}`, consumidor só lia array).
   **Skills como ADWs 28/08/2026** (bundle `docs/plans/2026-08-28-skills-adw-potencializacao/`):
   library em **23 specs** — o modus operandi das 5 skills TACO virou comando:
   `cross-audit` (7 fases: harmony_map/scan_debt/prove_invariants como nós code +
   auditor com o craft + report datado) · `plan-excellence` (pipeline Pln2:
   ground_truth_collector → scaffold → author → gap_detector P0 + plan_validator
   como gate falante) · `skill-refine` (REFINE por evidência: mine_transcripts +
   quality_gate → propostas roteadas, nunca apply) · `converge-close` (judge_attest
   → loop_converged → doc_link_gate, zero agentes — Lei L2 pura); analysis-loop já
   era ADW-nativa (profile_to_adw verificado por execução, lint 0/0). Três lições
   de executor pagas pelas estreias: (a) agente headless herdava os hooks da sessão
   e o `work-outer` o capturava — `_agent_claude` agora spawna com
   `TOURING_WORK_OUTER_DISABLED=1` (nó de ADW já é gatado pelo próprio flow);
   (b) gate mudo → feedback vazio → retry degrada (13→6→0 FACT medidos) — gates
   FALANTES (a REASON ensina a correção, A5); (c) **texto de agente nunca entra no
   sandbox**: o X6 classifica o programa inteiro (posicionais inclusos) e prosa com
   cara de comando vira deny não-determinístico — lint novo
   `sandboxed_gate_reads_agent_text` (com precedência sobre o convite dual) +
   2 fragments da library corrigidos (worker-critic-pair, critic-panel); e os
   placeholders angulados `<x>` em `echo` de REASON casavam com o detector de
   redirect do `command_writes` (falso escritor) — guard da library agora nomeia
   os 4 escritores DELIBERADOS.

