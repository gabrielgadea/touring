# Camadas de Execução do Code Mode (E0–E5)

> Wave 2026-08-24 — a lógica dos loops do ADW recuperada e estratificada como
> arquitetura nativa do Code Mode. Princípio unificador: **em toda camada, quem
> encerra o loop é código, nunca o modelo** (Lei L2), e **todo sinal é lido
> fail-closed** (ausente = unknown, nunca zero).

## O modelo de camadas

Cada camada é a anterior **composta** — a mesma escada N0→N4 dos tool patterns,
aplicada à execução. Um trabalho entra na camada mais baixa que o resolve; subir
de camada troca N round-trips do modelo por 1 invocação do harness.

| Camada | Nome | Unidade | Quem itera | Sinal de terminação | Comando |
|---|---|---|---|---|---|
| **E0** | Execução atômica | 1 programa, 1 sandbox | ninguém | `exit_code` + `failure{kind,phase}` | `touring run` |
| **E1** | Programa orquestrado | 1 programa, N consultas ao daemon | o PRÓPRIO programa (loops em código) | `return` do programa | `touring run --orchestrate` |
| **E2** | Loop de nó | 1 nó repetido até secar | o runner ADW | `NEW_FINDINGS=<n>` (dry_rounds; ausente = unknown) | nó `loop` (spec ADW) |
| **E3** | Rodada gated | audit→act→gate→re-measure→close | o grafo do fluxo | vereditos dos gates (retry lê o veredito) | `touring adw run <flow>` |
| **E4** | Campanha | 1 FLUXO repetido até convergir | o runner (`cmd_campaign`) | predicado `--until` (exit 0) + `METRIC=<float>` | `touring adw campaign <flow> --until "cmd"` |
| **E5** | Fluxo composto / factory | specs + fragments + roteamento | factory/RL | convergência de cada fluxo | `touring factory` · `touring adw new --use` |

### E0 — Execução atômica (`touring run`)

O piso: sandbox deny-by-default, budgets **duplos** (wall + CPU busy medido em
`/proc`), taxonomia de falha em 6 kinds ortogonais com mensagem que ensina,
spill com locator, journal por execução, gate CEG X0..X7. O contrato de retorno
é desenhado para o LLM se autocorrigir sem round-trip extra: `failure.message`
nomeia a correção, `retrieval_hint` nomeia a leitura.

### E1 — Programa orquestrado (`--orchestrate`)

Loops, joins e agregações acontecem DENTRO do programa (Python + SDK `touring.*`,
9 métodos read-only com allowlist). Intermediários morrem no sandbox; só o
agregado volta (reflexo agregado-não-dump, D5). É a camada CodeAct: 1 execução
substitui N tool calls.

### E2 — Loop de nó (nó `loop` do ADW)

A lógica canônica recuperada de `adw.py::run_loop`: `body` + `max_iters` +
`dry_rounds`; o corpo imprime `NEW_FINDINGS=<n>`; o RUNNER conta e decide
(`on_dry`/`on_pass`/`on_fail`). **Fail-closed**: marcador ausente = unknown —
silêncio nunca é rodada seca (lição 2026-08-02: truncamento por `tail -c`
encerrava loops que ainda achavam material). `tally` re-emite o agregado para
um loop envolver um `parallel` inteiro. É o motor do `touring explore
--until-dry` e do scout perpétuo.

### E3 — Rodada gated (fluxo ADW)

O padrão medir→agir→verificar→re-medir→persistir como grafo: gates com
`on_fail` de volta ao agente **carregando o veredito no prompt** (o lint
`fake_waiting` recusa retry cego), `stagnation_rounds` opcional em gates,
`phase-close` como depósito de feromônio. Exemplo vivo: `error-teach.toml`
(campanha w8b).

### E4 — Campanha (`touring adw campaign`) — NOVA (2026-08-24)

O loop de FLUXO que faltava. Nasceu de um anti-padrão medido nesta própria
arquitetura: a campanha w8b rodou como `for i in 2 3 4; do touring adw run …`
— sem predicado, sem curva, sem fail-closed, invisível ao journal. O
subcomando é a **afordância que substitui a persuasão** (D8):

```bash
touring adw campaign error-teach \
  --until "python3 scripts/error_message_audit.py --check-ratio 0.8" \
  --max-rounds 8 --stagnation-rounds 2 --var batch=30
```

Contrato: `--until` é CÓDIGO; exit 0 = convergiu (Lei L2). Seu stdout pode
emitir `METRIC=<float>` — a curva da campanha e o sinal de estagnação
(métrica PRESENTE e repetida N rodadas; ausente = unknown, nunca estagnação —
o mesmo fail-closed do `NEW_FINDINGS`). Cada rodada deposita reward
proporcional ao delta da métrica (feromônio ACO); a curva inteira persiste
como memória `campaign:<flow>:<run_id>`. Saídas: `converged | flow_failed |
stagnation | max_rounds`.

### E5 — Fluxo composto e factory

Specs compostos por fragments (`[[use]]`), personas com craft (`skill =`),
tiers explícitos (o lint de codegen induz o barato), racing, e o factory
roteando tickets → fluxos com RL. As campanhas E4 são elas próprias
roteáveis: um ticket recorrente de qualidade vira `campaign` com predicado.

## Integração com as estruturas do Touring

| Estrutura | Papel nas camadas |
|---|---|
| **CEG X0..X7** | gate de E0 (todo `run`); cada sub-chamada E1 carrega identidade `<run_id>:code:<n>` no wire (S-5.2, entregue 2026-08-24) — o daemon soma o contrafactual dela em `code_mode_subcall_bytes_total` e a registra em `run_subcalls.jsonl` |
| **decompose** | E3/E4 mapeiam para subtasks com `claim` atômico; convergência de campanha fecha o subtask |
| **memory** | curvas de campanha (`#kind:campaign`), snippets harvestados (E0/E1 → `#kind:snippet` com trust medido), lessons por fase |
| **learning reward** | por rodada (E4, delta da métrica), por snippet (`snippet:` → trust ladder), por fluxo (phase-close) |
| **journal** | `run_journal.jsonl` (E0), `loop_round` (E2), journal do run (E3), curva (E4) |
| **explore/CCE** | E2 aplicado a lentes de conhecimento (`--until-dry`) |

## Aderência ao modelo LLM

- **Erro é dado, com taxonomia** — cada `kind` direciona uma correção diferente;
  o modelo escolhe a certa sem round-trip de diagnóstico.
- **Contratos tipados byte-estáveis** — `--sdk-stub` e injeções de hook
  ordenadas/bucketizadas preservam o prefixo do KV-cache (P21; auditado por
  `kv_cache_audit.py --assert-stable`, 10/10).
- **Progressive disclosure** — nudge carrega a 1-linha (~53 tok); o contrato
  completo (~360 tok) é on-demand (decisão P23, por medição).
- **1 invocação → N rodadas → digest** — E4 comprime uma campanha inteira num
  único tool call do modelo; o que volta é a curva, não os logs.
- **Sinais fail-closed** — o modelo nunca é induzido a ler silêncio como
  sucesso (`NEW_FINDINGS`/`METRIC` ausentes = unknown; `stderr_truncated`
  honesto nas duas fronteiras).

## Aderência ao Claude Code

- **Hooks como afordância** — cli-suggest deriva o comando REAL da camada certa
  (loop bash → E0/E1; `for … adw run` → E4); flow guard e Stop hook aplicam as
  leis por artefato, não por persuasão.
- **Tiers e permission modes explícitos** — nós headless declaram
  `permission_mode` (lint) e custo (`tier`, lint de codegen); subagents herdam
  o modo do orquestrador.
- **Skills carregam o craft** — `skill =` no nó injeta a Skill tool; as camadas
  são documentadas onde o modelo as encontra (este doc + `docs/code-mode.md` +
  nudges).
- **Journal e memória compartilhados** — o que uma sessão aprende (curva,
  trust, lições) é o que a próxima recalls; verificação cruzada entre sessões
  fecha o ciclo (padrão d0/d1/d3: re-sonda independente antes do completed).

## Regra de escolha de camada

**Entre na camada mais baixa que resolve; suba quando o loop escapar para o
chat.** Se você (modelo) está prestes a repetir uma invocação com pequenas
variações — isso é o sinal de que o trabalho pertence à camada de cima, onde
o runner itera por você com sinal fail-closed e curva registrada.
