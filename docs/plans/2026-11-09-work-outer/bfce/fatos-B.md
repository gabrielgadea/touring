# B — dono do veredito VGP em duas camadas: fatos (09/11/2026)

Levantamento só-leitura (subagente sonnet), sem execução. DAG `task_1794217886022321956`.

## Composições L1 + L2 (sem dependência touring-code ↔ touring-pyright)

- **A · CLI** `touring-server/src/cli/vgp.rs`: `run_json` :30 → `check_em` (L1) → `camada2()` :69
  (desligada anota; pool; `checar_lang_com_orcamento` com `--l2-budget-ms`) → `no_momento` :107
  (`rebaixar_tmp_na_escrita` só em Escrita) → `fundir_camada2` :157 (Unknown anota e não funde;
  senão Severidade→string :177, `fundir_findings` :189, `verdict_dos` :190).
- **B · sombra** `touring-code/src/vgp/sombra.rs:302` `sombra_check_em` + `touring-hook-handlers/src/hooks/vgp_l2.rs`
  `extra_em` :31 / `mapear` :54 (mesmo rebaixamento e conversão); chamada de `pre_write.rs:1393`,
  `pre_edit_prevention.rs:591`, `pre_bash.rs:972`, `vgp_execucao.rs:95`. L2 roda ANTES da L1;
  desligada → None sem nota; orçamento padrão; `catch_unwind` na L1.
- **C · teste** `vgp_l2.rs:281` `camadas_1_e_2_em`; parciais `sombra.rs:846`, `vgp.rs:447`.
- "deny" por string: `pre_write.rs:1426`, `pre_edit_prevention.rs:624` (sem `.take(3)`), `vgp_execucao.rs:157`.

## Momento / balde — 8 sítios

| # | sítio | regra |
|---|---|---|
| 1 | `politica.rs:93` `roda_agora` | inline; Escrita=false; Execucao: `/tmp/` ou 1º componente `.cache` |
| 2 | `mod.rs:260` `is_inline` | `<string>`, `<stdin>` |
| 3 | `touring-pyright/src/achado.rs:166` `Balde::from_path` | qualquer `<…>`; dentro da raiz=Arquivo; senão `/tmp` ou `temp_dir()` |
| 4 | `sombra.rs:202` `balde_de` | inline, `/tmp/`, senão arquivo |
| 5 | `typescript/analise.rs:216` | `Contexto::Execucao` fixo |
| 6 | `vgp_execucao.rs:81` | `roda_agora` como pré-filtro |
| 7 | `lsp_nucleo.rs:273`, `tsserver.rs:468` | `Balde::from_path` → severidade (`pyright.rs:1121`, `ts_diag.rs:250`, `rust_analyzer.rs:400`) |
| 8 | `typescript/globais.rs:192,218` | escopo de varredura (não momento) |

Divergências: `.cache/a.py` Execucao (L1 roda agora, L2 Arquivo); raiz sob /tmp (L1 roda agora,
L2 Arquivo — pinado em `pyright.rs:1679`); `<frozen>`; TMPDIR fora de /tmp.

## `escopo` da sombra é circular
`sombra.rs:347`: `escopo = deny ? "externo" : "projeto"`. `Finding.alvo` é `#[serde(skip)]`
(`mod.rs:47`); a L2 perde o `Escopo` em `Finding::new` (`Alvo::Projeto`, `mod.rs:75`). Nenhum leitor
de máquina de `escopo` (só `sombra.rs:646` e a doc). Leitores da sombra: `juiz_v4_sessoes.py`
(balde, path, findings[].nome), `prova_vgp_motor.py`, rollup `docs/kpi/journals.toml:205` (balde,
verdict). J15/J17 leem o JSON da CLI em Escrita e calculam o balde por conta própria.

## Testes
L1 `politica.rs:172-205`, `lang/mod.rs:354-400`; L2 `pyright.rs:1664,1712,2286`; CLI `vgp.rs:356,377,401,447,599,638`;
sombra `sombra.rs:604,620,651,758-966`; handlers `vgp_l2.rs:202,301,337,355`; TS `typescript/testes.rs:1254`;
e2e `touring-hooks/tests/vgp_ativo_ordem_e2e.rs`. Contrato: campos do JSON de `CheckOutput`; linha da sombra.

## Divergências que mudam veredito (decisão explícita)
D1 `.cache` em Execucao (aperta L2) · D2 raiz sob /tmp (afrouxa L1 ou aperta L2) · D3 `<…>`/TMPDIR ·
D4 TS com contexto real (troca alvo em /tmp na escrita).
