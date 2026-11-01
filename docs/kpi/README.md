# `touring kpi` — commitments falsificáveis

> **Criado**: 2026-08-28. Declaração: `docs/kpi/commitments.yaml` · Resolvedor:
> `crates/touring-cli/src/cli/kpi.rs` · Saída: `touring kpi -j`.
> Doc de fundação (histórico, F1-F8 do coupling):
> `docs/2026-06-27-coupling-telemetry-infrastructure.md`.

## O contrato

Cada commitment é uma promessa **falsificável**: `id`, `name`, `threshold`,
`direction` (`gte`/`lte`), `source`, `rationale`, opcional `advisory: true`.
`touring kpi -j` resolve cada fonte e reporta `PASS` / `FAIL` / `STUB`.

**STUB é honesto**: fonte irresoluvível (cache ausente, arquivo external velho,
amostra < mínimo) reporta STUB, nunca 0 — ausência de sinal é desconhecido,
não zero (Lei L2, fail-closed).

## Gramática de fontes (3 canais)

| Canal | Forma | Resolução |
|---|---|---|
| **daemon** | `daemon:<handler>[@<scope>]:<json-pointer>` | invoca o handler no daemon e lê o pointer. `@<scope>` (2026-08-28, `split_handler_scope`) injeta o escopo no payload — ex.: `daemon:cli-mutation-test@touring-identity:/kill_rate` chama o handler com `{package: "touring-identity", cache_only: true}` |
| **derived** | `derived:<name>` | valor computado de dados já coletados (família `touring.coupling.*` e afins), braço por nome em `resolve_derived` |
| **external** | `external:<comando que gera> # → docs/kpi/external/<id>.json` | o texto após `external:` é documentação humana do comando gerador; o resolvedor lê `docs/kpi/external/<id>.json` (id = id do commitment), pointer `/value` |

### O canal external (2026-08-28)

Para medições que nenhum handler do daemon produz (contagem nextest, cobertura
llvm-cov). Alimentar é escrever o arquivo:

```bash
# exemplo: touring.test.count
echo '{"value": 15786}' > docs/kpi/external/touring.test.count.json
```

- **Frescor**: mtime > **14 dias** (`EXTERNAL_STALE_SECS`) → STUB. Uma medição
  velha não vira PASS eterno; o piso de artefato não pode envelhecer com o
  marker (lição `piso-de-artefato-envelhece-com-o-marker`).
- Campos extras no JSON são ignorados (anotar `measured_at`, comando etc. é
  bem-vindo).
- Versionável e auditável — o arquivo entra no git junto com a medição.

Primeiras medições reais (2026-08-28): `touring.test.count` = 15 786 ·
`touring.coverage.line` = 0.7528.

## Guard contrato×executor (D8)

`every_declared_source_has_an_arm_and_every_derived_arm_a_commitment`
(kpi.rs, teste estrutural) cruza o YAML e o resolvedor **nas duas direções**:

- toda fonte `daemon:`/`derived:` declarada tem braço no `invoke_handler`/
  `resolve_derived` — fonte sem braço foi exatamente o defeito que deixou o
  KPI mutation STUB por 4 meses (memória `kpi-fonte-declarada-sem-resolvedor`);
- todo braço `derived:` tem commitment que o consome — braço órfão foi o caso
  `inspect_burst_share` (existia desde o S3 de 27/08 sem nenhum commitment; o
  commitment `touring.code_mode.inspect_burst_share` é o consumidor).

## Prontidão: uma resposta para «está saudável?» (N5, 17/10/2026)

Quatro medidas respondiam «saúde» com a mesma palavra e grandezas diferentes
(`status.composite_health_score`, `e2e.overall_score`, `harness-metric.composite`, e este
`touring kpi`). `scripts/readiness.py` não elege uma nem tira média: junta os **sinais** numa **lista
de itens de atenção** (fonte, valor, limiar, rota para resolver) e dá o veredito `pronto` (exit 0) ou
`atencao` (exit 2). Fonte que falha, estoura o tempo ou imprime erro vira o item `fonte.<nome>` —
ausência de sinal nunca é zero nem é omitida. As medidas antigas aparecem como detalhe com nomes
distintos (`saude_composta_daemon`, `e2e_nota_ponderada`, `harness_composto`, `kpi_compromissos`,
`nota_fase_wiring`), cada uma com **o que mede** e **o denominador**.

- Aceitar um item exige uma entrada em `docs/kpi/readiness-dispensas.yaml` com `item`, `motivo`, `dono`
  e `valido_ate` (no máximo 90 dias à frente). Dispensa vencida volta a valer como item
  (`dispensa_expirada`); dispensa incompleta não vale e vira o item `dispensas.invalida.*`. O arquivo
  versionado começa vazio: aceitar é ato do dono, com prazo.
- Uso: `python3 scripts/readiness.py --text` (ao vivo; rode com `--root` na raiz do projeto, o `doctor`
  resolve o projeto pelo cwd) · `--json` · `--from-dir DIR` lê `DIR/{status,e2e,kpi,doctor,harness-metric}.json`.
- Severidades (`bloqueia`, `sem_medida`, `aviso`) ordenam a lista; **todo item ativo mantém o veredito em
  `atencao`**. Quais itens deveriam ser só aviso (não exit 2) é decisão aberta do Gabriel.

## Journals do runtime: manifesto e retenção (N6, 17/10/2026)

`docs/kpi/journals.toml` declara, para cada journal de `~/.claude/touring`: caminho, **escritor**,
**leitores com a janela que cada um precisa**, retenção, política de expiração e rollup.
`scripts/journal_gc.py` é o único que poda, e só o que o manifesto prova que ninguém precisa:

- **dry-run é o padrão**; `--apply` é explícito. `--verificar` confere o manifesto contra o código que
  ele nomeia (arquivo existe e cita o journal; retenção ≥ janela de todo leitor; escritor sem leitor;
  arquivo do censo sem manifesto).
- leitor com `janela_dias = "toda"` (lê o arquivo inteiro, ou ninguém verificou) **bloqueia** a poda —
  fail-closed. Desbloquear é mudar o leitor (janelar, ou ler rollup + recente), não o manifesto.
- antes de podar grava o **rollup diário** (`rollup/<id>.jsonl`: contagem e somas por dia UTC e grupo) e,
  com `expira = "arquivar"`, comprime as linhas vencidas em `arquivo/<nome>.<AAAA-MM>.gz`.
  `journal_gc.py --totais ID --de D --ate D [--onde campo=valor]` responde um KPI pelo rollup; o teste
  `test_kpi_from_rollup_equals_kpi_from_raw` prova que bate com o cálculo pelo bruto.
- idempotente e seguro a queda: o rollup guarda o digest de cada lote podado; refazer após uma queda entre
  «rollup gravado» e «bruto trocado» não conta duas vezes. Journal aberto para escrita por outro processo
  é pulado. Linha sem data legível **nunca** é podada.

## Cobertura: a catraca (N2, 17/10/2026)

`docs/kpi/coverage-floor.json` guarda o piso de cobertura de linhas. O job `coverage` do CI roda
`scripts/coverage_ratchet.py` depois de publicar o lcov: reprova se o total ficar abaixo de
`floor_pct - tolerance_pp` (0,5 pp). `floor_pct: null` = ainda não medido no CI: o passo só **registra** o
total no job summary e passa. A primeira medição real na `main` vira o piso por
`python3 scripts/coverage_ratchet.py --lcov lcov.info --floor docs/kpi/coverage-floor.json --update`
(só sobe, nunca baixa). Lcov ausente ou vazio reprova (exit 2).

## Snapshots e séries

- Snapshot por execução: `docs/kpi/<YYYY-MM>/<data>--<root-slug>.json`
  (persistido por `persist_snapshot`; raiz derivada do workspace — desde
  2026-08-28 aponta para `~/projects/touring`, não mais a árvore congelada).
  O repositório é público: desde 17/10/2026 o escritor grava o caminho relativo
  ao home (`~/projects/touring`), nunca o absoluto — o `<root-slug>` do NOME
  (`~-projects-touring`) e todo campo string do JSON (`project_root`, `reason`,
  `journal`, ...) passam por `scrub_home`. Os snapshots anteriores a essa data
  foram mantidos como estavam.
- Tendências: `docs/kpi/trends/`.
- Gate de CI/entrega: `touring kpi --check` (advisory não trip a saída).

## Referências

- Playbook do mutation testing: `docs/mutation-testing.md`
- Fundação e catálogo `touring.coupling.*`:
  `docs/2026-06-27-coupling-telemetry-infrastructure.md` (§7; ver nota de
  atualização 2026-08-28 no §1.1)
- Cross-audit da estreia do canal:
  `docs/audits/cross-audit-2026-08-28-kpi-mutation-lifecycle.md`
