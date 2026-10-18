# V0e-fora-de-gate — destino das 606 falhas de Bash fora de gate

**Janela**: `--since 2026-09-17` (o padrão de `inventario_erros.SINCE`, linha 30) até
`--ate 2026-10-08T20:20:00Z` (08/10 17:20 BRT). Mesmos `ROOTS`, mesmo `rglob`, mesma lógica de
`Inventory._one` — **importada**, nunca copiada: `residue_class` decide a classe e `parse_errors`
decide se há diagnóstico de linguagem.

## Controle do aceite (2): a tabela reproduz EXATAMENTE

`probe_606.py` roda o instrumento sem nada meu no caminho e compara classe a classe:

**15 classes nomeadas, todas com diferença 0** — 220 · 55 · 53 · 34 · 22 · 15 · 13 · 11 · 8 · 6 · 6 ·
5 · 4 · 4 · 2. E `não classificado` = **148**, exato. Soma: 458 nomeadas + 148 = **606**.

Uma nota de método que custou um falso alarme: minha primeira sondagem separava o `não classificado`
do total e reportava **458**, parecendo 148 a menos. A tabela de 606 **inclui** as 148 — as duas
contagens são a mesma coisa, e `causas.py` imprime a conservação explícita (`EXATA`) para que isso
não dependa de memória.

## Resultado por classe

| classe | n | % | destino |
|---|---:|---:|---|
| touring run falhou | 220 | 36,3% | **b** — o gate não declarava o orçamento (35 timeout, TODOS sem `--timeout-ms`) |
| arquivo/caminho inexistente | 55 | 9,1% | **c** — o chamador apontou caminho inexistente (`ls: cannot access`, `No module named`) |
| permissão do Claude Code negou | 53 | 8,7% | **d** — camada de permissão do Claude Code, não hook do Touring |
| is_error com saída de sucesso | 34 | 5,6% | **c** — pytest passou (20 passed, 1 skipped) e o comando **seguinte** no `&&` falhou |
| pytest falhou (sumário) | 22 | 3,6% | **c** — o teste falhou de verdade (33 failed, 101 passed) |
| sintaxe do shell | 15 | 2,5% | **b** — a mensagem não ensina a rota (4/4 lidos: aspas dentro de aspas em `--code`) |
| git | 13 | 2,1% | **c** — 2/5 pathspec ausente; **3/5 `index.lock` de sessão CC concorrente** |
| timeout | 11 | 1,8% | **c** — o `timeout 560` do próprio comando expirou (exit 143 = SIGTERM) |
| saída vazia com exit≠0 | 8 | 1,3% | **c** — grep sem casamento dentro de pipeline com `&&` |
| permissão/sandbox | 6 | 1,0% | **d** — o próprio CEG negou (Landlock KernelEnforced) |
| entrada de ferramenta inválida | 6 | 1,0% | **c** — `InputValidationError` da própria ferramenta |
| traceback cortado | 5 | 0,8% | **c** — a exceção **está** no texto; o balde não a parseou |
| comando inexistente | 4 | 0,7% | **c** — `bc: command not found` (utilitário ausente no SO) |
| CLI touring/uv/outros | 4 | 0,7% | **b** — `touring run --lang python -c`: a CLI não tem `-c` |
| cargo falhou (sem diagnóstico) | 2 | 0,3% | **c** — cargo falhou, diagnóstico fora do recorte |

**Resumo por destino**: (a) defeito a consertar: **0** · (b) mensagem que devia ensinar a rota: **39**
(6,4%) · (c) uso legítimo: **455** (75,1%) · (d) ambiente: **112** (18,5%).

## `touring run falhou` (220) — item (5)

| `failure.kind` | n |
|---|---|
| sem envelope no transcript | **133** |
| exception | 52 |
| timeout | **35** |

**Os 35 timeout: `--timeout-ms` declarado em 0, ausente em 35.** O tamanho histórico do achado de hoje
é **35 eventos** — todos no default de 30 s, nenhum declarando orçamento. Já corrigido no release de
09/10 à tarde.

⚠ **133 dos 220 (60%) não têm envelope no transcript** — a primeira linha é literalmente
`Exit code 1` e o `failure.kind` não existe para eles. Eles **não são somados** a nenhuma categoria de
`failure.kind`: ficam declarados como `sem envelope no transcript`. Isso é um limite de **observação**,
não uma classe de falha: para esses 133, o destino é indeterminado por construção.

## O que a leitura refutou do classificador por regex

O destino por regex é um palpite estruturado; a leitura de casos reais é o veredito. Onde divergiram,
o antigo fica registrado em `causas.json → refutado_pela_leitura`:

| classe | o regex propunha | a leitura mostrou |
|---|---|---|
| `CLI touring/uv/outros` | (a) defeito no código do Touring | uso de flag inexistente (`-c`). É defeito de **ensinar**, não de existir |
| `traceback cortado` | (b) o recorte escondeu a linha | a linha **estava lá** — `SemProcedencia` / `RelacaoDesconhecida` em `grafo_modelo.py:168/223`. O balde não a parseou |
| `sintaxe do shell` | (b) genérico | **uma causa só** em 4/4 lidos: citação de código dentro de `--code`. Torna a correção pontual |

Verificação do candidato (b): `touring run --help` confirma que a CLI aceita
`--code` / `--file` / `--stdin` e **não tem `-c`**. A mensagem de uso existe e é boa; o que falta é o
hook apontar para ela quando a flag falha.

## `não classificado` (148) reduzido por predicado positivo

| novo balde | n |
|---|---|
| saída vazia + exit≠0 (exit code nu) | **143** |
| sem predicado (visível, nomeado) | **5** |

Os 5 restantes sãoPredicados que a leitura mostra serem de outra natureza: `Permission to use Bash…`
(2), `The user doesn't want to proceed` (1), `Recursive force delete detected` (1),
`Override safety checks` (1) — **todos já classificados no destino (d)**. Nenhum foi somado a
`saída vazia`.

## Como rodar

```bash
cd ~/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/v0e-fora-de-gate/
python3 coleta.py --selftest                                  # rc 0, 20 controles
python3 coleta.py --ate 2026-10-08T20:20:00Z                  # bruto por item
python3 causas.py                                              # tabela + conservação + evidências
python3 tabela_md.py                                          # markdown do JSON
python3 probe_606.py                                          # controle contra a tabela do brief
python3 ler_topo.py                                           # texto inteiro das classes pequenas
```

Bruto por item: `~/.claude/touring/vgp/v0e_fora_de_gate.jsonl` (session · tool_use_id · ts · project ·
classe · destino · primeira linha ≤200 · comando ≤200 · failure_kind quando houver) — **fora do
repositório**, porque as mensagens podem conter dado de processo.

## Limites desta entrega

- **`inventory_erros.py` não foi editado.** A lógica é importada; se ela mudar, este relatório envelhece
  sem aviso — é o mesmo acoplamento que o `v0e5` explicitou com `assert COMPILED[0]`.
- **Os 133 sem envelope** são um ponto cego de observação, não uma conclusão sobre a falha.
- **Os destinos (b) e (d) não foram corrigidos** — este é um relatório de destino, não um conserto.
  O item (b) de maior valor é o `sintaxe do shell`: uma causa só em 4/4, com correção de mensagem
  pontual.