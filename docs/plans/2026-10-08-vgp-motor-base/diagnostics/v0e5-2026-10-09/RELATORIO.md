# V0e5 — o balde "G1 rajada-de-inspeção" é a junta de TRÊS gates

**Instrumento**: `v0e_negacoes.py` rodado como a 6b rodou, sem argumento: `--since 2026-09-17`
(o padrão do próprio `inventario_erros.SINCE`, linha 30).
**Método**: o balde é definido pelo **texto-fonte** da negação, por predicado **positivo** sobre o
início literal da linha — não por inferência do nome do gate. A lista de predicados veio de
`recon_textos.py`, que imprime as linhas literais que casam o padrão.

## A causa: ordem de casamento, não sobreposição de nomes

`v0e_negacoes.py:25` define o G1 com `rajada-de-inspe|inspection burst|RAJADA`, compilado com `re.I`
(linha 53) e casado por `search` — **em qualquer ponto do texto**. `gate_of` (linha 57-61) devolve
**o primeiro padrão que casa na ordem de `GATES`**, e o G1 é o **primeiro da lista**. Logo, qualquer
negação cujo texto contenha a palavra "rajada" é creditada ao G1, mesmo sendo de outro gate:

- o **S3** diz `[CODE MODE · rajada] 2ª inspeção …` → casa `RAJADA`;
- o **G10** diz `[G10 exec-burst] … a rajada desenrolada …` → casa `rajada`.

Nem o S3 nem o G10 aparecem como linhas próprias em `GATES` enquanto o padrão do G1 as precede.

## Q1 — o balde na linha de base (`--since 2026-09-17`, até o deploy 30.4.94)

| origem | n | % do balde G1 | % de todas as negações (4.012) |
|---|---|---|---|
| **S3 rajada do code mode** | **1.270** | **68,5%** | 31,7% |
| **G10 exec-burst** | **402** | **21,7%** | 10,0% |
| **G1 próprio** | **182** | **9,8%** | 4,5% |
| outro (casou sem ser nenhum dos três) | 0 | 0,0% | 0,0% |
| **TOTAL** | **1.854** | 100% | 46,2% |

**Conservação**: 1.270 + 402 + 182 + 0 = **1.854**, e o instrumento reporta **1.855** com o mesmo
`--since` sem corte superior. A diferença de **1** é o evento do S3 em `2026-10-09T14:16:43Z–14:20Z`,
que meu corte superior exclui: confirmado por `v0e_negacoes.py --since 2026-10-09T14:16:43Z`, que
reporta `G1 rajada-de-inspeção = 1`. **1.854 + 1 = 1.855 — a conservação fecha ao evento.**
"outro" = **0**, com controle positivo: o `diagnostico.py` procurou em todo o corpus as negações do
balde que não casariam nenhum dos três predicados, e **não achou nenhuma**.

⚠ **Denominador móvel**: o total de todas as negações era **4.010** na minha primeira execução do
instrumento e **4.012** na verificação seguinte — o corpus cresce enquanto a sessão trabalha. As
colunas "% de todas as negações" são dessa execução específica (4.012); uma rerodada pode dar
±0,1 p.p. **O balde G1 (1.855) é estável** nos dois pontos: a variação de 2 é de gates fora do G1.

**Rota seguida**: o instrumento mede isto no balde agregado — **44%** (denominador `judged` = 1.855,
isto é, o próximo `tool_use` de cada negação casa `touring run`). **Não medi a rota por subgrupo**:
o `v0e_negacoes` classifica a rota pelo **gate credited**, não pelo texto-fonte, então os três
subgrupos compartilham o mesmo número e reportá-los separadamente repetiria 44% três vezes como se
fosse medida. O corte por texto-fonte que fiz conta **quantas negações** são de quem; a rota seguida
continua sendo a do balde, porque para ela a distinction não muda o que o instrumento observa.

## Q2 — desde o deploy 30.4.94 (`2026-10-09T14:16:43Z`)

| evento | n |
|---|---|
| S3 rajada do code mode — **ainda nega** | **1** |
| G1 próprio — ainda nega | **0** |
| G10 exec-burst — ainda nega | **0** |
| **reescritas do G1** (assinatura: envelope `touring run`) | **1** |

**Controle positivo declarado por você: ACHADO.**
`2026-10-09T14:19:57.853Z`, sessão `e7c33fba`, comando `for p in $(pgrep -x touring-daemon); …`,
envelope `{"exit_code": 0, "duration_ms": 42, "ceg_advisory": {"reason": "X6 denied the subprocess
capability 'pgrep' under profile 'Sandboxed'"}}`.

⚠ **Um detector que exigisse `is_error` não acharia esse evento — e o meu achava zero.** A reescrita
devolve `allow` com `updatedInput`: o `tool_result` volta **`is_error=False`**, e o texto da razão não
aparece. Reconheci o evento pelo **envelope**, não por `is_error`. Se o detector da V0e1 usava
`is_error`, a reescrita foi **invisível** para ele por construção — o que significa que "G1 = 0
negas depois do deploy" **não** prova que o gate parou de disparar: prova que parou de **negar**.

## Três achados

**1 · A V0e1 mediu o S3 e o G10, não o G1.** O número "G1 = 46% das negações (3.870 em 346 transcripts)"
tem **9,8%** de G1 próprio neste mesmo balde: 68,5% era o S3 e 21,7% o G10. O objeto da V0e1
(deixar de reprovar trabalho legítimo) foi avaliado sobre um baldo que **90,2% não era o gate V0e1**.
A aprovação da V0e1 não se sustenta no recorte correto, e ainda menos depois do 30.4.94, quando o G1
passou a reescrever em vez de negar.

**2 · O 30.4.94 desligou o G1 mas deixou o S3 aceso — e o S3 é 7× maior.** G1 próprio: 182 → 0
negas. S3: 1.270 eventos na base, **continua negando** (1 no período medido). Como o S3 é quem
domina o balde, o que a V0e1 mediu como "G1 funcionando" é majoritariamente o S3 ainda **negar**.

**3 · O balde é um artefato da ordem de `GATES`, não um conceito.** Um padrão amplo no **primeiro**
lugar captura os três gates; corrigir isso exige **ordenar o G1 abaixo do S3/G10** ou estreitar o
padrão para `\[G1 rajada-de-inspe` — sem editar o instrumento, não há como o balde voltar a significar
"o G1".

## Nota de leitura

Os números de Q1 diferem dos da V0e1 (3.870 / 46%) porque **o `--since` aqui é 2026-09-17, o padrão**,
e a V0e1 usou outra janela. Não é discrepância de método: é discrepância de denominador. Para comparar
com a V0e1 é preciso o mesmo `--since` — que não está registrado naquela evidência.

## Arquivos (rodáveis de novo)

| arquivo | o que faz |
|---|---|
| `v0e5_por_origem.py` | **o entregável**: Q1 (quebra por texto-fonte + conservação) e Q2 (S3 + reescritas + controle positivo) |
| `recon_textos.py` | imprime as linhas literais que casam o padrão — de onde saíram os predicados |
| `diagnostico.py` | prova de que "outro = 0" tem controle positivo, e por que o controle da reescrita falhava antes |
| `v0e5_por_origem.json` | os números |

```bash
cd ~/projects/touring/docs/plans/2026-10-08-vgp-motor-base/diagnostics/v0e5-2026-10-09/
TOURING_CODE_MODE=native python3 v0e5_por_origem.py     # ~3 min
```

Nada fora deste diretório foi escrito. `v0e_negacoes.py` e `inventario_erros.py` foram **importados,
nunca editados** (o script ainda afirma com `assert` que `COMPILED[0]` é o G1: se a ordem mudar, ele
falha em vez de mentir). Nenhum hook, crate ou `settings*.json` tocado; sem commit, sem restart de
daemon, sem update-touring.