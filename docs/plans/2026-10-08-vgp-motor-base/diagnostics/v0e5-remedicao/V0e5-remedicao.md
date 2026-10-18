# V0e5-remedição (2ª rodada) — os avisos e o G10 estavam(sum) por cegueira

Você estava certa nas duas, e a causa foi medida, não presumida. Este documento substitui o anterior.

## Os dois defeitos, com a evidência que os prova

**1 · "S3 = 0 avisos" era cegueira.** O aviso **não é erro de ferramenta**: vive num registro
`attachment` (`hook_additional_context` / `hook_success`), não num `tool_result is_error`. Ler só
tool_result dá zero **por construção**.

E o `content` desses anexos é uma **lista de strings** (`['[CODE MODE · rajada] aviso — 2ª inspeção …']`),
não string nem lista de dict. O `_text` do `inventario_erros` — que era o que eu usava — só junta
`x.get("text")` de dicts, então **devolvia vazio**. Meu `grep` achou "2ª inspeção", vi a mesma frase da
negação antiga e atribuí a ela, sem ler o registro. Era exatamente o erro que você nomeou.

**2 · "G10 = 0 em C" era cegueira, por um defeito de precedência que eu tinha já medido e não
reproduzido.** O texto do exec-burst é `[G10 exec-burst] 5ª chamada seriada … — a rajada desenrolada …`.
Ele casa o padrão do **G1** (`RAJADA`, na frase "a rajada desenrolada"), e o G1 é o **primeiro** de
`GATES`: `gate_of` credita ao G1 e o G10 nunca aparece. Prova no próprio instrumento:

```
v0e_negacoes.py --since 2026-09-17  →  G1 = 1.869 · exec-burst = 0
V0e-negacoes.txt (08/10)            →  G1 = 1.782
```
A diferença **87** é o G10 sendo roubado. O instrumento tem `exec-burst: 0` — literalmente zero, em
4.053 negações. Sem o congelamento, o número só cresce e o roubo some.

## Os 4 controles nomeados, agora verdes

| controle | instante | o que é | resultado |
|---|---|---|---|
| AVISO S3 `grep` | `16:33:28.421Z` | attachment `hook_additional_context`, PreToolUse | ✅ |
| AVISO S3 `python-inline` | `17:30:30.875Z` | idem | ✅ |
| negação `write→run` | `17:31:31.293Z` | `[G10 write→run] 2º script escrito-e-executado` | ✅ |
| negação `exec-burst` | `17:35:24.110Z` | `[G10 exec-burst] 5ª chamada seriada` | ✅ |

`--selftest rc 0`. Os controles checam **o gate atribuído**, não só o texto — porque achar o texto e
classificar no gate errado era exatamente o defeito. Quando apertei o controle para checar o gate, ele
**pegou um terceiro defeito meu**: o `write→run` estava saindo como `exec-burst` (o primeiro nome que
`startswith` na lista). Um controle que só checava texto passaria.

## C — a tabela, com `--ate` congelado em `2026-10-09T17:44:00Z`

15 eventos em C: **5 avisos do S3 + 10 negações**.

| gate | exposta | exp. C | neg. C | taxa/100 | IC 95% |
|---|---|---:|---:|---|---|
| G1 rajada-de-inspeção | Bash | 175 | **0** | 0,00 | [0,00 · 2,15] |
| **exec-burst (o "G10" do balde)** | Bash | 175 | **1** | 0,57 | [0,10 · 3,17] |
| write→run | **Bash** | 175 | **1** | 0,57 | [0,10 · 3,17] |
| G11 orçamento de bypass | Bash | 175 | 1 | 0,57 | [0,10 · 3,17] |
| rm guard | Bash | 175 | 0 | 0,00 | [0,00 · 2,15] |
| G7 re-inspeção | Read | 22 | 2 | 9,09 | [2,53 · 27,82] |
| G3 edit sem Read | Edit | 42 | 0 | 0,00 | [0,00 · 8,38] |
| python_pre_gate | Edit | 42 | 4 | 9,52 | [3,77 · 22,07] |

**O "G1 = 1 em C" da rodada anterior era o exec-burst.** Com o G10 pelo prefixo próprio, o G1 fica em
**0** e o exec-burst em 1. Era o mesmo evento, com o nome trocado.

**Exposição do `write→run` = Bash**, não Write: o gate dispara quando o *script escrito é executado*
(`[G10 write→run] 2º script escrito-e-executado na janela de 600s`), então o que ele vê é a chamada que
executa. Isso também explica por que o `Write` do gate, na 1ª rodada, media 7.091 expõe para um gate que
dispara no run.

## O S3 avisando — primeira medida real

**5 avisos em C** (o 1º dado real). Rota seguida pela chamada seguinte: **0 de 5**, ou seja **0,0%**,
IC 95% **[0,0 · 43,5]**.

⚠ **Com n=5 isso não é veredito.** O IC vai até 43%: os dados são compatíveis com 0% e com 40%. Não
digo que o aviso não ensina a rota — digo que **5 amostras não respondem**. Os avisos de C são:
`wc` (17:30:29) · `python-inline` (17:30:30) · `grep` (17:31:45) · `sed-n` (17:33:34) · e o das 17:33 que
o limite de `--ate` corta.

O ponto substantivo: **o S3 passou a avisar e o aviso não está sendo seguido.** Com n=5 isso é um
sinal fraco para investigar, não uma conclusão.

## A régua dos 3.892

Você escreveu 3.870; o arquivo diz **3.892** em 346 transcripts. O `mtime` dele é
**`2026-10-08 17:49:14 -0300`** = `2026-10-08T20:49:14Z` — essa é a fonte do instante congelado.

**Não reproduzo, e a causa é medida.** Rodando o instrumento agora (`--since 2026-09-17`, sem teto) dá
**4.053** em 352 transcripts, porque **os transcripts de 08/10 continuam sendo escritos** — o número
cresce com a sessão. Congelar o instante resolve para o *futuro*, mas **não recupera o passado**: o
corpus de 08/10 já mudou desde as 17:49. Um `3.892` reproduzível exigiria o conjunto de transcripts do
instante, e ele não foi gravado.

O que dá para afirmar com número: **a diferença para o original é 161 (+4,1%)**, atribuível a
negações acumuladas depois de 08/10 17:49.

## O que ficou sem resposta

1. **A reprodução dos 3.892 é impossível** — o conjunto de transcripts do instante não foi gravado, e
   os arquivos mudam. Congelar o instante serve de agora em diante.
2. **`exec-burst` = 0 no instrumento é um defeito da régua, não do meu script.** Ele não tem como
   creditar o G10 com a ordem atual de `GATES`. Qualquer medição que use `v0e_negacoes` sem o prefixo
   próprio herda o erro. **Consertar isso é editar `v0e_negacoes.py`, que está proibido neste brief** —
   registro como dívida.
3. **O S3 avisando tem n=5.** O sinal de "não seguido" é real mas o IC [0,0 · 43,5] não sustenta
   veredito. Precisa de mais amostras.
4. **`G1 = 0` e `rm = 0` em C com 175 de exposição**: IC [0,00 · 2,15] — compatível com até 2,15 por
   100. Ausência de dado, não ausência do gate.

## Arquivos

| arquivo | o que faz |
|---|---|
| `remedicao.py` | **o entregável**: `--ate` obrigatório, avisos de `attachment`, G10 por prefixo, IC · `--selftest` rc 0 |
| `remedicao.json` | números + **todos os eventos de C** para conferência |
| `ver_controles.py` | imprime a tabela |
| `explicar_regua.py` | a diferença para a régua, gate por gate |

```bash
cd ~/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/v0e5-remedicao/
python3 remedicao.py --selftest --ate 2026-10-09T17:44:00Z   # 13 controles
python3 remedicao.py --ate 2026-10-09T17:44:00Z               # varre e grava
python3 ver_controles.py
```

**Idempotência**: duas execuções com o mesmo `--ate` dão JSON **idêntico** (verificado). Sem `--ate`
o script recusa rodar (exit 2): sem instante congelado o número não reproduz — era o ponto (d).

Nada fora deste diretório foi escrito; `v0e_negacoes.py`, `inventario_erros.py` e `crates/`
intocados. Sem git, sem update-touring, sem kill, sem `settings.json`.