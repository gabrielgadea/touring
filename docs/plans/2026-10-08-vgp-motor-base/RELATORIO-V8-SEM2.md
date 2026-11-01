# RELATORIO-V8-SEM2 — o held-out web: denominadores, concentração e selo

> V-8 SEM2 da lista consolidada §1a. A entrega é hipótese até o juiz da
> coordenadora (touring-00 revisa; nada foi mergeado no branch principal).

## 1. O que foi pedido

1. **Completar os denominadores** do held-out web (hoje 83 HTML + 21 JS de 300;
   `~/.claude/touring/vgp/corpus_holdout_web_20261012/meta.json` com
   `atinge_alvo=false`).
2. **Resolver a concentração OSS.**
3. **Gerar selos externos** que o validador atual aceite.
4. **Classe sem n suficiente sai como "não provada"**, sem inflar o número.

## 2. O estado medido (nada aqui é declarado sem medição)

### 2.1 O estrato `transcript` (o corpus web)

| classe | n | alvo | arquivos distintos | sessões distintas | concentração máx. por arquivo | por sessão | `atinge_alvo` |
|---|---:|---:|---:|---:|---:|---:|---|
| html | 83 | 300 | 10 | 8 | **46 (55,4 %)** | **47 (56,6 %)** | false |
| javascript | 21 | 300 | 8 | 4 | 8 (38,1 %) | **17 (81,0 %)** | false |

O topo do html é um arquivo só (46 das 83 entradas,
`…/harvest-eleitoral-rs/entregaveis/PECA-DOIS-VOTOS.html`); o topo do
javascript é uma sessão só (17 das 21).

### 2.2 O estrato `oss` (a camada de repositórios abertos)

| camada | n | chaves distintas | concentração máx. por chave | teto declarado |
|---|---:|---:|---:|---:|
| `oss/tsx` | 213 | 127 | 7 (3,3 %) | 7 ✓ |
| `oss/typescript` | 214 | 128 | 7 (3,3 %) | 7 ✓ |
| `oss/javascript` | 3 | 3 | 1 (33 %) | 1 ✓ |
| `oss/_podado_20261009-215331/tsx` | 180 | 112 | **10 (5,6 %)** | 3 % ✗ |
| `oss/_podado_20261009-215331/typescript` | 191 | 119 | **12 (6,3 %)** | 3 % ✗ |

O selo da camada (`oss/selo.json`) **declara** `teto_concentracao: 0.03` e
**mede** `concentracao_medida: {dentro_do_teto: false, infratores: [...]}` —
ou seja: a violação do teto pela camada *podada* já está registrada pelo
próprio selo (é a concentration que ele mede, não só a que ele declara).
O selo da camada principal (`oss/tsx`, `oss/typescript`) está dentro do teto
(7 = o `teto_por_chave_no_fim` que ele declara ✓).

## 3. O que foi feito

### 3.1 O selo EXTERNO que o validador atual aceita

Novo script: `scripts/selar_holdout_web.py` (no bundle). O selo anterior era
um `meta.json` com um `metodo_selo` **declarado** — nenhum dos campos que
`confere_selo.py` exige, e morava **dentro** do corpus (o `sha_tree` do corpus
incluiria o próprio selo, e o validador reprova um selo dentro).

O selo novo:

| | |
|---|---|
| onde | `~/.claude/touring/vgp/corpus_holdout_web_20261012.selo.json` — **fora** do corpus ✓ |
| formato | `sha_tree` + `arquivos` + `bytes` (os três que `confere_selo.py` exige) + os denominadores por classe + a concentração **medida** + a situação da prova |
| conferido | `python3 scripts/confere_selo.py --corpus ~/.claude/touring/vgp/corpus_holdout_web_20261012 --selo ~/.claude/touring/vgp/corpus_holdout_web_20261012.selo.json` → **INTACTO** (rc=0, `sha_tree 42106741066f40f9`) |
| denominadores | `~/.claude/touring/vgp/corpus_holdout_web_20261012.denominadores.json` (a medição completa, por classe) |

### 3.2 A situação da prova: **NÃO PROVADA**, sem inflar o número

O `n` para provar ≤ 1 % de falso deny com 0 falsos (Clopper-Pearson 95 %) é
**299**. As duas classes do web estão abaixo dele (83 e 21), e o
`atinge_alvo` é `false` nas duas. O selo e o arquivo de denominadores
declaram, por classe:

```
situacao: "NAO PROVADA"
motivo: "n=83 < 299: o limite superior de Clopper-Pearson 95% com 0 falsos
         nao fica em 1% — a classe sai como NAO PROVADA, sem inflar o numero"
```

**O alvo de 300 é inalcançável nesta população** — e isso é medido, não
suposto: o gerador (`construir_holdout_web.py`, estrato `transcript`, corte
`2026-10-09T23:00:00Z`) recolhe as edições REAIS de `.ts/.tsx/.js/.mjs/.html`
nos transcripts, e a população inteira é a que está no `meta.json`: **83 HTML
e 21–23 JS** (o `--dry-run` de hoje já dá 23 no javascript — a população
cresceu 2 com as edições novas, e continua duas ordens de grandeza abaixo do
alvo). Não há mais 217 entradas para escrever: elas não existem.

## 4. O que ficou para a coordenadora decidir

1. **A concentração do estrato `transcript`** (55,4 % por arquivo no html,
   81,0 % por sessão no javascript) **não é um bug de seleção**: o estrato
   `transcript` é por construção **sem teto** (o próprio gerador: "no estrato
   transcript nao se aplica teto, e a concentracao maxima e REPORTADA"), e a
   população vem das edições da própria máquina — 10 arquivos, 8 sessões. O
   teto de 3 % só existe no estrato `git` (`--teto-concentracao`).
   Aplicar o teto aqui reduziria o corpus para poucos itens **sem** melhorar a
   prova (o n já está abaixo de 299 por falta de população, não de teto).
   **A alternativa honesta**: declarar o estrato `transcript` como uma
   *amostra de concentration conhecida* (o rótulo e de DELTA, e o n é pequeno
   demais para provar 1 %), e deixar a prova de 1 % para o estrato `oss`.
2. **A concentração da camada `oss` podada** (5,6 % e 6,3 % contra o teto de
   3 %) está **medida e declarada** no selo da própria camada (com os
   infratores nomeados). A correção é re-rotular com o teto aplicado na
   seleção (`rotular_oss.py --teto-concentracao 0.03`), o que exige os
   clones dos repositórios — o caminho fica sendo o da coordenadora.
   **Em curso**: um `--dry-run` do rotulador com o teto de 3 % está rodando
   (`/tmp/oss_dryrun.log`); o que ele responder é se os clones estão nesta
   máquina — e o resultado entra aqui.

## 5. Regras (do briefing, em vigor)

- Nada foi mergeado no branch principal; nada foi sobrescrito nem apagado: o
  corpus de disco (`corpus_holdout_web_20261012`) e o `oss/selo.json` continuam
  como estavam, e o selo novo **mora fora** do corpus.
- hw-slot sempre; commit só dos meus paths.
- Uma classe sem n suficiente sai como **não provada** — sem inflar o número.
