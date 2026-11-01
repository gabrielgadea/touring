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

## Snapshots e séries

- Snapshot por execução: `docs/kpi/<YYYY-MM>/<data>--<root-slug>.json`
  (persistido por `persist_snapshot`; raiz derivada do workspace — desde
  2026-08-28 aponta para `~/projects/touring`, não mais a árvore congelada).
- Tendências: `docs/kpi/trends/`.
- Gate de CI/entrega: `touring kpi --check` (advisory não trip a saída).

## Referências

- Playbook do mutation testing: `docs/mutation-testing.md`
- Fundação e catálogo `touring.coupling.*`:
  `docs/2026-06-27-coupling-telemetry-infrastructure.md` (§7; ver nota de
  atualização 2026-08-28 no §1.1)
- Cross-audit da estreia do canal:
  `docs/audits/cross-audit-2026-08-28-kpi-mutation-lifecycle.md`
