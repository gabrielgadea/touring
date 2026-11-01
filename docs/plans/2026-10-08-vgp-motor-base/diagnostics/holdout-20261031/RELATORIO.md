---
type: Report
title: "V-3 / held-out 20261031 — BLOQUEADO antes do selo por timestamps vazios"
plan_id: 2026-10-08-vgp-motor-base
subtask: task_1793475566980112187
okf_version: 1
timestamp: 2026-10-31
tags: ["#kind:report", "#domain:harness", "#status:blocked"]
---

# V-3 / held-out 20261031 — bloqueio antes do selo e do J14

B = `/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base`.

## 1. Veredito

**BLOQUEADO. Não existe corpus selado, o J14 não rodou e não há p95 frio × quente.**

O corte e a semente foram preservados. A regra de parada por falta de entradas não foi o bloqueio: o construtor selecionou **600** entradas de arquivo e **534** inline, mas abortou ao reconstruir o estado no instante porque **317 entradas selecionadas têm `ts` vazio**. Não troquei a semente para contornar o defeito, não alterei o corte, não editei construtor/estado/juiz/motor e não selei o corpus parcial.

## 2. Universo e regra dos 300

Diagnóstico com todas as exclusões exigidas (`diagnostico_20261031_congelado_novo.log`, rc 0):

| Medida | Resultado |
|---|---:|
| Corte preservado | `2026-10-09T18:14:31.894Z` |
| Exclusões nas quatro chaves | 4.705 ids · 2.444 tool_use_ids · 4.741 shas · 2.475 comandos |
| Candidatos de arquivo até o corte | 9.017 |
| Pool inline `c` / viável sob teto de sessão | 2.083 / 1.064 |
| Pool inline `heredoc` / viável | 2.350 / 1.002 |
| Pool inline `touring_run` / viável | 616 / 204 |
| Pool tmp / viável | 4 / 4 |
| Selecionados no balde arquivo | **600** |
| Selecionados no balde inline agregado | **534** = c 200 + heredoc 200 + touring_run 130 + tmp 4 |

A regra histórica dos 300 é aplicada aos baldes `arquivo` e `inline` agregado, como no relatório do 20261013f; nenhum dos dois ficou abaixo de 300. Os subestratos `touring_run` e tmp ficaram abaixo de suas cotas, o que seria reportado, não convertido em troca de corte ou de regra.

## 3. Falha que impede o selo

`construir_disjunto_20261031.log`, rc 1:

```text
File ".../scripts/estado_no_instante_v0d.py", line 253, in estado_negativo
    antes = corpus_v0d._conteudo_depois(e["path"], instante_anterior(e["ts"]), indice)
File ".../scripts/estado_no_instante_v0d.py", line 46, in instante_anterior
    t = datetime.fromisoformat(ts.replace("Z", "+00:00")) - timedelta(milliseconds=1)
ValueError: Invalid isoformat string: ''
```

Contagem independente apenas dos metadados gravados, sem imprimir conteúdo de código:

| Arquivo de índice | Total | Sem `ts` | Detalhe |
|---|---:|---:|---|
| `entradas.jsonl` | 600 | **53** | todos `origem=atribuido` |
| `entradas_inline_neg.jsonl` | 534 | **264** | 260 `origem=inline` + 4 `origem=tmp` |
| **Total** | **1.134** | **317** | 27,96% das entradas selecionadas |

Por extrator entre os 264 inline/tmp sem `ts`: `touring_run` 130 · `heredoc` 67 · `c` 63 · `tmp_escrito_e_executado` 4. Primeiro id de arquivo afetado: `ce80fdd90113`. O campo `ts` existe nos registros, mas está vazio; não é uma simples ausência de chave.

O corpus parcial ficou em `~/.claude/touring/vgp/corpus_holdout_disjunto_20261031`, **sem `selo.json` e sem qualquer chamada de motor/juiz**. O construtor recusa reutilizar um destino não vazio; preservá-lo ou substituí-lo exige decisão da coordenação. Nada do conteúdo do corpus foi copiado para o repositório.

## 4. Congelamento e adjudicador

O primeiro diagnóstico parou porque o `congelados.json` do 20261013f ainda esperava o sha antigo de `confere_selo.py`. Gerei um congelamento novo (`congelados.json`) com os shas atuais; somente esse arquivo divergia do registro anterior:

- `confere_selo.py`: `fed3c847630f…` → `554adfbe564d…`.
- Adjudicador `scripts/adjudica_rotulo.py`: **`fabc36c9e470c3598c35d869ac26e96714b101f60dab963f74b0a9a9e76fca4c`**, inalterado e assertado na geração do congelamento.
- Construtor, verificador de sobreposição e `estado_no_instante_v0d.py`: shas do 20261013f preservados.

O comando exato da tentativa está em `rodar_construtor_20261031.sh`. O diagnóstico e a construção passaram por `scripts/hw-slot`, com `nice -n 19` e `TOURING_CODE_MODE=native`.

## 5. Binário 30.4.101 congelado

| Item | Valor |
|---|---|
| Origem | `~/.local/bin/touring` |
| Destino | `~/.claude/touring/vgp/binarios/a6b9a550/touring` |
| Versão | `touring 30.4.101` |
| Git completo | `a6b9a55023f2b645ec6f6daab9f2ed63718ac018` |
| sha256 | `d064e1ebef33f4ace9a06e5edbdad4582b062adef0f2a1a01e814900390f4159` |
| Registro | `~/.claude/touring/vgp/binarios/a6b9a550/touring.sha256` |

A referência `8fd0efb0` também existe e tem sha256 `8fd0efb0e62e64a882380d109e922ea4635daca7ff24321b1232b9382d4c96d7`. O runner `../j14/rodar_j14.sh` foi preparado e validado com `bash -n`, mas **não executado**.

## 6. Caminho do daemon quente — evidência de código, sem medição

A etapa 4 não foi executada porque depende do corpus híbrido selado. O caminho existe:

- `crates/touring-hooks/src/main.rs:150-200`: `touring-hook` tenta o daemon e só cai em standalone se o daemon não responder; a rota pode ser comprovada por `TOURING_HOOK_TRACE_FILE`.
- `crates/touring-hooks/src/daemon_main.rs:60-62`: o daemon chama `touring_pyright::pool::habilitar_neste_processo()` no boot.
- `crates/touring-hook-handlers/src/hooks/vgp_l2.rs:40-47`: o hook chama `pool::checar_lang` dentro desse processo.
- `crates/touring-server/src/cli/vgp.rs:1-3,76-90`: a CLI é daemonless e habilita o pool no próprio processo efêmero; por isso o juiz mede o caminho frio.

Limitação já identificada para a futura medição: `pre_write.rs:1385-1391` cria `CheckInput` com `overlays: HashMap::new()` e lê o disco atual, sem equivalente aos `--overlay`/`--disk-file` do juiz. Uma medição pelo hook precisa declarar essa diferença e não pode ser apresentada como replay semântico idêntico. Nenhum número quente foi produzido.

## 7. O que falta para destravar

A coordenação precisa decidir/corrigir a proveniência dos timestamps no universo/construtor. Depois, com o mesmo corte, adjudicador e semente, é necessário tratar explicitamente o destino parcial e repetir: construção → regra dos 300 → selo → sobreposição nas quatro chaves com controle positivo → J14 → daemon quente. Não cabe a esta sessão editar os instrumentos proibidos nem escolher outra semente para evitar os registros defeituosos.
