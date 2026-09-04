---
type: Lessons
title: "Lições consolidadas — /tmp como sintoma: sandbox, harness e portfólio (02/09/2026)"
description: "Registro completo do que foi feito e de todas as lições da wave B+C+W, com a evidência que produziu cada regra."
tags: [licoes, code-mode, ceg, sandbox, portfolio, wiring, harness, claude-md]
timestamp: 2026-09-02T14:30:00-03:00
plan_id: 2026-09-02-tmp-sandbox-portfolio-afordancia
---

# Lições consolidadas — 02/09/2026

> **Origem**: Gabriel observou que o esgotamento da quota do `/tmp` era sintoma de algo maior:
> "há muito sendo escrito no temp que deveria estar sendo escrito no sandbox; além disso, a
> infraestrutura do portfólio, de se reusar aquilo que já existe construído como base, como se
> fossem blocos pré-moldados, [não está efetiva]". O diagnóstico virou um canvas de 9 seções;
> Gabriel escolheu abrir as camadas **B (executor)** e **C (biblioteca)** e investigar o
> **wiring (W)** antes de rejulgar a wave TIER-2.

---

## Parte I — O que foi feito

DAG `task_1788340910617776382`, 12 subtasks, todas fechadas, `loop_converged` verde.
Versão final **30.4.35**, propagada e provada por comportamento.

| Fase | Entrega | Prova |
|---|---|---|
| **W** | 4 furos do detector de wiring fechados: chamadas de função livre ausentes da query tree-sitter, `super`/`self` não resolvidos, `./` não canonizado (+ migração das linhas duplicadas), inferência ausente no caminho do rebuild e do hook | órfãos escopados **3148 → 1740** contra baseline normalizado |
| **B1** | Tmp privado por run: `RunTmp` cria `/tmp/touring-run-*`, o filho recebe `TMPDIR` apontado para ele, o Landlock troca `/tmp`+`/var/tmp` pelo dir do run (+`/dev`), e o resto é medido em `tmp_bytes` antes da remoção (`TOURING_RUN_KEEP_TMP=1` preserva) | touring-ceg 582/582; ao vivo `stdout: /tmp/touring-run-nSjUgA` |
| **B2** | `run_journal.jsonl` v2: `source`/`file`/`harvest`/`brief`/`orchestrate`/`tmp_bytes`, todos `serde(default)`; agregados em `JournalAggregate` | `journal_v2_tests`, `run_source_tests`; ao vivo `source:"file"` com path casando |
| **B3** | F2.1 XSS só dentro de tag HTML aberta: kwarg python `onerror=` deixa de ser P0 BLOCK | `cwe_patterns` tests; o censo que estava travado passou |
| **B4** | `tmp_bytes` alcança o payload do CLI; montagem do payload extraída para `full_payload`, função pura | `tmp_bytes_payload_tests`; ao vivo `"tmp_bytes": 8192` |
| **B5** | X6 cobre a **família** de escrita: `write_bytes(`/`write_text(`/`writelines(`/`.mkdir(`/`.touch(`/`.unlink(`/`os.makedirs`/`os.rename`/`os.replace`/`os.rmdir`/`shutil.rmtree` | 584/584 com teste de recall E de precisão; ao vivo `X6 denied … 'write_text('` |
| **C1** | Escada automática de snippets: corpo com `harvest_hint` ganha chave `snippet:auto:<sig12>` e vira memória provisional na 2ª execução limpa | `auto_harvest_tests`; ao vivo `harvest_hint` + `snippet_trust: ○ untrusted` |
| **C2** | Bloco vindo do scratchpad do harness (`/tmp/claude-*`) que qualifica é copiado para `<cwd>/.touring/scratch/<YYYY-MM>/`, e o payload devolve `persisted_as` | `scratch_persist_tests`; ao vivo arquivo em disco |
| **C3** | A intenção que alimenta o prior-art pré-write sai da docstring, ignorando `#tags:` e shebang | `intent_codetag_tests` |
| **C4** | KPI `code_mode_reuse` em `touring kpi -j`: `reuse_ratio` com piso 0.20 e status STUB/PASS/FAIL | ao vivo `reuse_ratio 0.14`, `status FAIL`, `total_tmp_bytes 116503` |
| **C5** | `#origin:auto-harvest` nomeava faceta inexistente e era descartada; trocada por `#process:auto-harvest` + guard que exige faceta canônica nas tags mintadas | ao vivo `memory query "#process:auto-harvest"` devolve o corpo, `unknown_facets: []` |
| **Z** | REGRA #0: 7 `pub const` estreitadas e `duration_p50`/`duration_p99` do mirror ligados ao KPI de sinal | 1740 → 1734; ao vivo `duration_ms_p99: 184` no KPI |

**Correções de harness feitas no caminho**: `loop_phase_close.resolve_subtask_id` não casava sufixo
com hífen, então sete fases fecharam com `dag_updated: false` enquanto relatório, memória e reward
diziam sucesso. Corrigido sob TDD, com `test_phase_close_resolve.py` e o espelho `client/` sincronizado.

**Registro de hook**: `post-bash` passou a ser registrado também em `PostToolUseFailure`, sem o qual
todo comando que falha permanecia invisível ao mirror. Backup em `settings.json.bak-20260902T101814-postfailure`.

---

## Parte II — As lições

### L1. O executor pode estar certo e a superfície errada

Três defeitos do dia moram no mesmo lugar: entre um executor correto e a superfície que alguém lê.
`tmp_bytes` era medido e nunca chegava ao payload do CLI. O X6 detectava escrita e só reconhecia
uma grafia. A tag existia e nomeava uma faceta que o índice recusa.

**Nenhum dos três aparece num teste de unidade do executor. Todos aparecem no primeiro comando de
prova comportamental.** 1589 testes verdes não os pegaram; dois comandos pegaram.

> **Regra**: todo campo novo precisa de um consumidor na rota que alguém usa de fato, não apenas no
> struct. E todo detector cobre a **família** do idioma ou não cobre nada.

### L2. Detector de capacidade cobre família, nunca uma grafia

`WRITE_TOKENS` do X6 carregava só `.write(`. `Path(...).write_bytes(b"x" * 8192)` escreveu 8 KiB
dentro do sandbox e voltou com `forbidden_calls: []`.

O remédio tem duas metades que andam juntas: ampliar o **recall** (a família `pathlib`/`os`/`shutil`)
e guardar a **precisão** com um segundo teste. `os.rename`/`os.replace` ficam qualificados justamente
porque `.rename(` e `.replace(` nus são dominados por `DataFrame.rename` e `str.replace`, que não
tocam disco. É a lição do B3 no mesmo dia, aplicada preventivamente.

### L3. Rulesets do Landlock empilham por interseção

Quando duas camadas aplicam Landlock na mesma execução, o permitido final é a **interseção**, não a
união. Ao trocar `/tmp` pelo tmp privado no funil, um teste que concedia um write pela policy do X8
quebrou: a policy concedia e o funil já não concedia.

> **Regra**: toda camada que ADICIONA um ruleset precisa receber os grants das camadas anteriores,
> senão "conceder" vira "negar" em silêncio. O mesmo vale para o diretório de compilação do rustc.

### L4. Instrumento antes do sistema, duas vezes no mesmo dia

**Primeiro**: o gate de órfãos reprovou a wave TIER-2 acusando 1740 órfãos "todos novos" contra um
baseline de 5389. A causa era o formato da chave: o baseline foi gravado com todo caminho prefixado
por `./`, e a correção W3 passou a canonizar sem o prefixo. Nenhuma linha casava.

Normalizando os dois lados, o baseline de 5388 linhas colapsa para **3148 únicas** — a assinatura do
próprio defeito que a W3 corrigiu, o mesmo símbolo contado duas vezes.

**Segundo**: estreitar constantes fez o clippy acusar 4 defeitos no crate de handlers. Nenhum era
real. `project_root` é consumido sob `#[cfg(feature = "tantivy-fts")]`; as `flush_*` são chamadas por
`session_hooks.rs`, atrás de `session-hooks`. Com `--all-features`, zero erros.

> **Regra**: quando um gate por conjunto nomeado reprova tudo de uma vez, suspeite do formato da
> chave antes da dívida. Quando um lint acusa código morto num crate com `cfg(feature)`, rode
> `--all-features` antes de acreditar. **A configuração que faz compilar não é a configuração que
> serve de gate.**

### L5. Mudança de canonicalização invalida todo baseline nomeado gravado antes dela

Corolário direto da L4. Re-baselinear faz parte da mudança, com atestação escrita: por que o conjunto
mudou, que a contagem caiu, e cada símbolo remanescente nomeado e classificado. O baseline anterior
fica preservado ao lado (`orphans-scoped.pre-w3-2026-09-02.txt`), nunca apagado.

### L6. Faceta desconhecida é erro duro, e o erro é silencioso para quem escreve

As sete facetas canônicas são `kind`, `purpose`, `lang`, `domain`, `process`, `artifact`, `status`.
`#origin:` não existe, e faceta desconhecida é erro duro: a tag é descartada. A memória continuava
alcançável pela chave e invisível pela faceta.

> **Regra**: ao mintar tags em código, um guard deve ler as tags que o código produz e exigir faceta
> canônica. O vocabulário que o store recusa não é vocabulário.

### L7. CLAUDE.md é constituição, não changelog

Escrevi 42 linhas de narrativa de wave no `CLAUDE.md` do projeto. É o anti-padrão que a própria
REGRA #16 nomeia, e que o Context7 confirma nas docs oficiais do Claude Code
(`code.claude.com/docs/en/{memory,best-practices,claude-directory}`): manter conciso, alvo **abaixo
de 200 linhas**, apenas orientação amplamente aplicável, delegando workflow e conhecimento de
domínio para skills e para `.claude/rules/` por tópico. Arquivo inchado faz o modelo **ignorar**
instruções.

> **Regra**: no CLAUDE.md entra só o invariante que muda o comportamento de quem trabalha ali, mais
> o gotcha que custaria horas, com ponteiro para o bundle. O racional, a evidência e a história ficam
> em `docs/plans/` e na memória. A constituição paga o custo em toda sessão: um parágrafo que só
> interessa a quem viveu a wave é imposto sobre todas as futuras.

### L8. Um `pub` sem consumidor não é API, é decisão não tomada

Estreitar sete constantes de `pub const` para `const` remove o órfão porque a decisão foi tomada,
não porque foi escondida. É diferente de silenciar o detector.

Quando o símbolo tem valor real, a saída é ligá-lo ao consumidor natural. `duration_p50`/`duration_p99`
computavam percentis que ninguém lia, enquanto o KPI que já lia o mesmo mirror descartava a duração de
cada entrada. Um contador diz se os hooks são usados; o percentil diz se valem o que custam.

### L9. O gate de fase mente quando o resolvedor de identidade erra

`resolve_subtask_id` aceitava `phase + " "` e os subtasks são `B1-ceg-…`. Sete fases fecharam com
relatório, memória, reward e log gravados, e `dag_updated: false`. O sucesso parcial é pior que a
falha: tudo diz "feito" e o grafo não anda.

> **Regra**: quando um campo de veredito e um efeito colateral discordam, o veredito é o campo, não o
> exit code. O `subtask_updated` já dizia a verdade; ninguém o lia.

### L10. Prova comportamental é a única prova de propagação

Rótulo de versão nunca prova build. Depois de propagar 30.4.35, a prova foi um deny ao vivo em cada
projeto pinado, e não `--version`. O mesmo vale para o binário local: `tmp_bytes: 8192` no payload é
prova; "30.4.35" no cabeçalho não é.

### L11. Probes precisam ser derivados do predicado, não chutados

O primeiro probe de C1/C2 falhou silenciosamente porque seu corpo não qualificava para
`harvest_hint`, e eu concluí que a feature não funcionava. Ler o predicado no executor
(`persist_scratch_block` exige `--file` sob `/tmp/claude-` **e** `auto_harvest_candidate`) produziu um
probe que passou de primeira.

> **Regra**: quando a prova falha, pergunte primeiro se o probe satisfaz as pré-condições do que
> está sendo provado.

### L12. A régua nasce medindo o problema, e isso é o entregável

`code_mode_reuse` estreou em **0,14** contra um piso de 0,20. A leitura correta não é "o KPI
falhou", é "a afordância do portfólio ainda não pega, e agora isso é um número no dashboard em vez
de uma impressão". Medir era o entregável desta wave; subir o número é a próxima.

---

## Parte III — Os números

| Medida | Valor |
|---|---|
| baseline de órfãos, normalizado | 3148 |
| órfãos após W | 1740 |
| resolvidos pelo detector W | 1426 |
| realmente novos | 18 |
| órfãos após REGRA #0 (Z) | 1734 |
| `reuse_ratio` na estreia | 0,14 contra piso 0,20 |
| `total_tmp_bytes` medido | 116.503 |
| latência dos hooks, p99 | 184 ms |
| testes verdes na wave | ceg 584, server 1589, cli 500, hook-handlers 692, quality 399, hook-runtime 408 |
| CLAUDE.md do projeto | 332 linhas, 197 em 4 itens de narrativa |

---

## Parte IV — Comandos que reproduzem as provas

```bash
# B1 + B4 — tmp privado e a medição da limpeza
TOURING_TRUSTED_OK=1 touring run --lang python --allow-forbidden --code '
import tempfile, pathlib
d = pathlib.Path(tempfile.gettempdir())
(d / "left.bin").write_bytes(b"x" * 8192)
print(d)'
# espera: stdout /tmp/touring-run-*, campo "tmp_bytes": 8192

# B5 — a família de escrita é negada
touring run --lang python --code 'import pathlib; pathlib.Path("/tmp/x").write_text("y")'
# espera: X6 denied the file-write capability 'write_text('

# C4 + Z — as réguas no dashboard
touring kpi -j | jq '{code_mode_reuse, signal: .code_mode_signal_use}'

# C5 — a memória auto-colhida pela faceta canônica
touring memory query "#kind:snippet #process:auto-harvest"

# convergência (o único "pronto")
python3 ~/.claude/skills/loop-engineering/scripts/loop_converged.py \
  --task task_1788340910617776382 --scope /home/gabrielgadea/projects/touring \
  --bundle docs/plans/2026-09-02-tmp-sandbox-portfolio-afordancia
```

---

## Parte V — O que ficou para decisão de Gabriel

1. **`sdk::load_signal_report`** — função pública documentada, **zero consumidores e zero testes**
   (medido: só a definição em `sdk.rs:227`; os exemplos usam `signal_report_from_journal`, que é
   outra função). Existem 4 relatórios emitidos em disco que ela saberia ler.
2. **Despacho por tabela de comandos** — identificador de função em posição de valor não é
   `call_expression`, então o handler lê como órfão. Medido: **94** dos 1734 órfãos são handlers
   nomeados numa tabela de despacho, ou seja 5,4% do ruído.
3. **CLAUDE.md do projeto** — 332 linhas contra as ~200 recomendadas, com 197 em quatro itens de
   narrativa de wave (8, 10, 12, 13). O item 14 já foi reduzido de 42 para 21 linhas.

Cada uma tem um canvas de 9 seções na resposta ao Gabriel de 02/09/2026.

---

## Cross-links

[Índice do bundle](/index.md) · [Histórico](/log.md) · [Retomar](/RETOMAR-AQUI.md) ·
[Fase W](/phases/W-wiring-rebuild-perde-arestas-inferidas-fix-detector-rejulgar-TIER2.md) ·
[Órfãos novos vs baseline](/diagnostics/orphans-new-vs-baseline.txt)
