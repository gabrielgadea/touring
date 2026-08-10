---
type: PhaseReport
title: "P6 — A2 (tokens reais) e A1 (clones Type-2), mais a paridade de status que mandava refazer trabalho pronto"
description: "Fecha os dois itens deixados abertos: a economia de contexto passa a ser medida em bytes exatos e tokens cl100k, e a duplicação passa a enxergar clones renomeados. Inclui a correção do hook PostCompact e a assimetria C08 entre quatro leitores do DAG."
plan_id: 2026-08-07-gortex-insights
tags: [implementacao, tokens, minhash, type2, duplicacao, hooks, regra-21, c08]
timestamp: 2026-08-08T03:30:00-03:00
okf_version: "0.1"
---

# P6 — Implementação

Fecha o `dag_done`: **A2** e **A1**, os dois que a sessão anterior deixou abertos
com desenho pronto. Mais duas correções pedidas/encontradas no caminho.

## A2 — a economia de contexto deixa de ser inventada ✅

`ctx_roi` calculava `routed × 30_000 + compressed × 20_000`, dividia por 4 e
convertia em dólares. **Três palpites empilhados**: duas constantes de bytes por
evento e um divisor de caracteres por token. Nada ali tocava um byte real.

Pior que as constantes serem arbitrárias: a de compressão é ~40× o máximo
possível. O único chamador de produção de `compress_for` é
`derive_summary_with_tool`, que lê **512 bytes** do arquivo persistido
(`buf.len().min(512)`). Uma economia de 20.000 bytes por evento num sítio que
nunca vê mais de 512 é impossível por construção.

### O que passou a ser medido

| Onde | O que se grava | Custo |
|---|---|---|
| `compress_for` | `raw.len()` e `out.len()` exatos, mais tokens cl100k dos dois textos | `len()` + tokenização só onde há tokenizador |
| `build_sandbox_wrapper_args` | `res.output_bytes` (captura real do sandbox) e o tamanho do envelope | dois `len()` |

Um profile que casa e não comprime nada agora contribui **zero** — antes
contribuía 20.000 fictícios.

### A costura do tokenizador

`cl100k_base` já existe em `touring_cortex::enrichment::count_tokens`, mas
**`touring-cortex` depende de `touring-hooks`** — o daemon não pode chamá-lo sem
inverter o grafo. Em vez de mover o tokenizador, o ledger expõe um `OnceLock<fn(&str)
-> Option<usize>>` e cada processo instala o que puder pagar
(`touring-hooks/src/token_meter.rs`, instalado em `daemon_main.rs`).

Duas decisões que sustentam a honestidade do número:

- O tipo devolve **`Option`**. A tabela BPE carrega preguiçosamente e pode
  falhar; uma falha tem de virar *não medido*, nunca zero token para um texto
  real.
- Os dois lados têm de tokenizar, ou o evento não contribui token nenhum —
  `in - out` entre duas medições diferentes não é uma diferença.

Custo: registro é grátis; a tabela só carrega no primeiro evento real, então um
daemon com o roteamento dormente nunca paga. `TOURING_TOKEN_METER=0` desliga.

### O envelope

`tokens_saved` e `usd_saved` são **`null`** quando não houve medição; a
estimativa existe sob `tokens_saved_estimate` + `estimate_basis`. Uma estimativa
nunca ocupa o campo que o leitor toma por medida — é o mesmo princípio do
`name_only_candidates: Option` do P5, aplicado a outro número.

E quando não houve evento algum, o envelope diz *"isto relata ausência de
atividade, não uma economia de zero"* e nomeia o gate
(`TOURING_HOOK_ROUTING`, limiar `TOURING_HOOK_ROUTING_THRESHOLD`).

### O defeito que só apareceu ao procurar onde ler

`ctx_roi` lê atômicos **do próprio processo**. Mas quem serve a tool MCP é a
ponte `touring serve`, que nunca comprime nem roteia nada — seus contadores são
zero por construção. A métrica relatava o processo errado desde sempre.

Corrigido com o mesmo padrão que o `cli-gate-event` já usa para os contadores do
CEG: a tool consulta `gate-metrics` do **daemon** via `daemon_query`. O campo
`source` do envelope sempre diz de qual processo vieram os números.

**Prova**: 7 testes de invariante sobre `ctx_roi_from_snapshot` (incluindo um
guarda estrutural que relê o próprio fonte e reprova se `30_000` ou
`saturating_mul` voltarem ao caminho do ROI) + 2 de integração sobre o ledger.

### O que a instrumentação revelou sobre o próprio subsistema

Com bytes reais no lugar, os contadores do daemon dizem **zero eventos** — e a
causa não é "pouco tráfego". Cadeia verificada:

1. `check_tool_output_routing` (o único caminho até `compress_for` em produção)
   tem exatamente um chamador: `pre_tool_use::run`, em
   `crates/touring-hook-handlers/src/hooks/pre_tool_use.rs:71`.
2. `~/.claude/settings.json` **não registra `pre-tool-use` em nenhum matcher**.
   Para `Bash` registra `pre-bash`, `block_git.sh`, `touring-process-guard.sh`,
   `cli-suggest` e `ceg-observe`.
3. `pre_bash.rs` (26 KB, existe) não referencia o router: `grep -c
   "tool_output_router\|check_tool_output_routing"` → **0**.

Ou seja: os 30 profiles de compressão e todo o roteamento para sandbox estão
**inalcançáveis nesta instalação**, e o `ctx_roi` vinha respondendo `$0,0000`
não por não haver economia, mas por não haver caminho. A flag
`TOURING_HOOK_ROUTING` está ligada por padrão (`!= "0"`) — não é ela que
desliga; é a ausência do registro.

**Não corrigido de propósito**: registrar um hook é mudança em `settings.json`,
que é gate humano. O envelope agora **diz** isso em vez de esconder atrás de um
zero — que é exatamente o trabalho que o A2 tinha para fazer.

### O defeito que a medição revelou: 30 profiles inalcançáveis

Disparando o roteamento à mão pelo daemon (`touring-hook pre-tool-use`, sem
mexer no `settings.json`), o ledger gravou dois eventos reais:

| comando roteado | bytes capturados | envelope | economia MEDIDA | fórmula antiga |
|---|---:|---:|---:|---:|
| `grep -r` sem match | 0 | 324 | **0** — o evento *custou* 324 | 30.000 |
| `grep -r tcp /etc/services` | 8.172 | 327 | **7.845** | 30.000 |

O primeiro evento gastou contexto em vez de economizar, e a fórmula antiga
creditava 30.000 aos dois igualmente. É a demonstração mais direta possível do
que o A2 conserta — em produção, não em teste.

Mas `compression_profile_applied_count` continuou **0** nos dois, com um sumário
sendo produzido a partir de 8 KB. Causa:

```rust
// sandbox_output_store.rs, o ÚNICO call site de produção
compress_for(tool_name, &serde_json::Value::Null, &raw)
```

Todo profile embutido despacha por `detect_in_command`, que lê
`args["command"]`. Com `Null` o lookup devolve `""`, nenhum needle casa, e **os
30 profiles são inalcançáveis por construção**. Os argumentos reais existiam
dois frames acima, em `execute_and_store` — só não desciam.

Corrigido passando `original_args`. O guarda que faltava é
`audit_compression_profiles_are_reachable_from_the_sandbox_path`: exige que o
contador se mova quando um comando roteado casa com um profile, e que a
compressão seja real (linha `FAILED` preservada, ruído `... ok` removido).

**O padrão, de novo**: o defeito sobreviveu porque nada contava bytes. Um número
que não existe esconde exatamente o defeito que ele mediria.

## A1 — duplicação que enxerga clones renomeados ✅

F1.3 media Type-1: cópia literal módulo espaços. Renomeie uma variável e o bloco
some do radar. O número publicado sempre foi um **limite inferior apresentado
como medida**.

Duas etapas novas, ambas em `touring-analysis/src/quality/duplication.rs`:

1. **Recorrência exata da forma normalizada** — identificadores viram `$I`,
   números `$N`, literais `$S`; palavras-chave, operadores e pontuação ficam.
   Cópia renomeada vira dois streams byte-idênticos.
2. **Clones com lacuna** — MinHash (32 permutações, 8 bandas × 4 linhas) propõe
   candidatos; `JaccardComputer` decide. `touring-simd/src/similarity/minhash.rs`,
   ao lado do Jaccard que o verifica.

### Três constantes escolhidas por medição, não por gosto

**Limiar 0.75 e janela de 12 linhas.** Com `SHINGLE_K=4`, cada token alterado
destrói até 4 shingles: `J = (m-d)/(m+d)`. Numa janela de 6 linhas (~57
shingles), 0.85 tolera **1 token** — o passo LSH seria redundante com a etapa
exata, isto é, maquinário morto. Na de 12 (~117 shingles), 0.75 tolera ~4.

**`MIN_DISTINCT_TOKENS = 16`.** Sem ele o combinado deu **31–56% por crate** —
"metade do código é duplicado" não é acionável, logo não é medida que valha
publicar. A causa é estrutural: seis `let x = f(y);` viram um só stream de seis
tokens distintos e casam com qualquer outra sequência assim no corpus. Essas
janelas não têm auto-informação suficiente para *evidenciar* cópia — as formas
coincidem porque a linguagem tem poucas formas.

A perda de recall é real e está registrada em teste
(`a_run_of_identically_shaped_lines_is_below_the_entropy_floor`): um clone
renomeado feito só de atribuições triviais é indistinguível de idioma e **não é
reportado**. Trade deliberado e visível, não redescoberto depois como bug.

### O que ele acha de verdade

Amostra real de `touring-simd`, via sonda sobre as janelas que casaram:

- `chebyshev_batch` × `chebyshev_batch_par` — diferem em `.iter()` → `.par_iter()`
- `DotF32` × `DotF64` — o par f32/f64 que genéricos ou macro eliminariam

São clones genuínos. Medido depois do piso:

| crate | Type-1 (pontuado) | regiões Type-2 | linhas invisíveis ao Type-1 | combinado |
|---|---:|---:|---:|---:|
| touring-analysis | 8,4% | 203 | +3.702 | 35,5% |
| touring-cli | 7,2% | 178 | +2.965 | 26,3% |
| touring-quality | 11,1% | 71 | +2.053 | 42,7% |
| touring-simd | 7,1% | 34 | +568 | 25,1% |
| touring-foundation | 2,9% | 59 | +1.261 | 16,8% |

### Type-2 na nota — decisão do Gabriel (08/08, "resolva tudo")

`F1.3 = min(banda_type1, banda_combinado)`, com **bandas separadas**.

A banda do Type-1 fica intocada (3%/8%/20%, calibração jscpd), então todo score
histórico permanece comparável através dela. O combinado ganha banda própria
(<15% saudável, 15–30% warn, 30–50% pay-down), calibrada contra a distribuição
medida (16,8% … 42,7%) e contra a literatura, onde cobertura Type-2/3 de 15–30%
é o comum em sistemas reais.

**Por que `min` e não mistura nem substituição**: só pode BAIXAR — nada que
reprovava por Type-1 passa a aprovar, então a dimensão nunca fica mais
permissiva por acidente. E deixa cada razão ser julgada na escala em que foi
calibrada, em vez de forçar um número pela banda do outro.

Dois guardas estruturais: `combining_never_relaxes_the_type1_verdict` (varre o
produto cartesiano das duas razões) e
`the_combined_band_discriminates_across_the_measured_range` — a banda tem de
**separar** os crates observados, porque um score constante não carrega
informação (era exatamente o risco de reusar a banda do Type-1).

**Custo medido**: F1.3 0,592 → **0,509**; composite 0,939 → **0,936**, ainda
Platinum, folgado acima do Gold (0,80).

<details><summary>Análise original (antes da decisão) — por que eu havia deixado fora</summary>

A banda do `score_duplication` (< 3% saudável, > 20% = 0,1) foi calibrada para
Type-1. O combinado mede uma coisa **diferente**, não uma versão mais rigorosa da
mesma: uma sequência de atribuições de mesma forma é clone Type-2 e não é clone
Type-1. Trocar o número por baixo do pano mudaria o significado de todo score
histórico sem que ninguém tivesse decidido isso — e o resultado (17–43%) fixaria
a dimensão em 0,1 em todo o workspace por definição, não por descoberta.

Essa análise continua correta quanto ao diagnóstico — reusar a banda do Type-1
fixaria tudo em 0,1. A decisão do Gabriel resolveu isso dando ao combinado uma
banda própria, em vez de deixá-lo fora.

</details>

`type2_clone_regions` conta **regiões contíguas**, não formas de janela: uma
função copiada gera uma região e uma dúzia de janelas deslocadas. A primeira
versão contava janelas e transformava 34 clones reais em 188.

## Correções fora do escopo original

### O hook PostCompact (pedido do Gabriel)

`loop_resume.py` devolvia `hookSpecificOutput.additionalContext` ecoando o nome
do evento recebido. O harness valida esse objeto como união discriminada por
`hookEventName`, e **PostCompact não é membro** — todo pós-compactação morria em
`(root): Invalid input`: o estado era calculado corretamente e descartado pelo
validador. Por isso o mesmo script funcionava numa das duas registrações e
falhava na outra.

Agora só eventos com canal de contexto recebem aquela forma; os demais recebem
`systemMessage` (campo de raiz, válido em qualquer evento), então o estado
aparece em vez de virar erro. Guarda estrutural sobre **todos** os eventos, não
só o que quebrou.

### A assimetria C08 entre quatro leitores do DAG (REGRA #21)

O mesmo hook dizia *"6/6 subtasks pending"* com 4 de 6 concluídas. `touring
decompose` fecha subtarefa como `"completed"`, e dos quatro leitores só
`loop_converged.py` listava esse valor:

| sítio | conjunto terminal | correto? |
|---|---|---|
| `loop_converged.py` | `done, completed, finalized` | ✅ |
| `loop_stop_guard.py` | `done, finalized` | ❌ |
| `loop_snapshot.py` | `done, finalized` | ❌ |
| `loop_resume.py` | `done, finalized` | ❌ |

Consequência: o contexto seguinte era instruído a **refazer trabalho pronto**.
Uma definição só (`loop_marker.TERMINAL_SUBTASK_STATUSES` +
`pending_subtask_ids`), usada pelos quatro, e um teste que reprova se algum
reintroduzir o conjunto privado.

Verificado contra o marcador vivo: `2/6 subtasks pending`, saída `systemMessage`
válida.

### Validação sequenciada atrás de I/O (achada pela própria suíte)

`cli::neural::run` construía o `HookRuntime` — que abre o knowledge DB — **antes**
de conferir o nome do subcomando. Com o disco em 94%, o SQLite devolveu
`disk I/O error` e o teste que afirma `"Unknown neural hook"` falhou por um
motivo alheio ao que ele testa.

O conserto é de ordem, não de mensagem: o guard roda antes de qualquer disco.
Um typo passa a falhar rápido, com a mensagem certa, sem depender de
infraestrutura. `VALID_SUBCOMMANDS` — que era uma **terceira** cópia privada de
vocabulário, no módulo de teste — virou `NEURAL_SUBCOMMANDS` em produção, com um
teste anti-drift exigindo um braço de `match` para cada nome.

**Sintoma para reconhecer o padrão**: teste instável cuja mensagem de erro fala
de infraestrutura em vez do domínio.

Contexto medido (REGRA #12, não corrigido — apagar cache é decisão do Gabriel):
`/home` em **94%** (55 GB livres), `target/` deste workspace em **279,37 GB**,
já acima do limiar de 50 GB do `safe-clean.sh stats`.

## Gates

| Gate | Resultado |
|---|---|
| `cargo test --workspace` | **15.311 passed, 0 failed** (exit 0) |
| `cargo clippy --workspace --all-targets -- -D warnings` | **exit 0** |
| `cargo test -p touring-analysis --lib quality::duplication` | 17/17 |
| `cargo test -p touring-simd --lib similarity::minhash` | 8/8 |
| `cargo test -p touring-hooks --test token_meter_e2e --test wave3_extended_e2e` | 30/30 |
| `pytest test_flow_guard.py` | 47/47 |
| testes novos desta fase | **38** |
