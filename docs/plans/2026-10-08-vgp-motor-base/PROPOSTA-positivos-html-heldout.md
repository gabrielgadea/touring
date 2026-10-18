# Lacuna: nenhuma classe HTML tem recall mensuravel no held-out web

> MEDIDO em 10/10. Nada aqui foi selado: e proposta, e o criterio para declarar uma classe
> "nao provada" vem antes de qualquer construcao. Gerado por 4f; pedido da coordenadora.

## (i) Tabela de positivos por classe x linguagem — TODO o held-out web (local E oss)

Fonte: `scratch/tabela_positivos_heldout.py` (JSON em `/tmp/tabela-heldout-web.json`).

| grupo | lang | negativo | positivo_natural |
|---|---|---|---|
| `corpus_holdout_web_20261012/html` | html | 83 | **0** |
| `corpus_holdout_web_20261012/javascript` | javascript | 21 | **0** |
| `corpus_holdout_web_20261012/oss/javascript` | javascript | 3 | **0** |
| `corpus_holdout_web_20261012/oss/tsx` | tsx | 169 | 44 |
| `corpus_holdout_web_20261012/oss/typescript` | typescript | 169 | 45 |
| **total** | | **445** | **89** |

Os 89 positivos sao **todos** de simbolo TS/TSX (portador de classe ausente em todos os arquivos).
Nenhum campo `classe` existe em nenhum arquivo do held-out web.

### Consequencia, por classe

| classe | n de positivos no held-out web | recall mensuravel? |
|---|---|---|
| `recurso` | 0 | **nao** |
| `ancora` | 0 | **nao** |
| `cdn` | 0 | **nao** |
| `cdn_pin` | 0 | **nao** |
| `classe_css` | 0 | **nao** |
| (símbolo tsx/typescript) | 89 | sim |

**O juiz J nao prova nada nas 5 classes HTML.** Isso nao e um bug do juiz: e ausencia de denominador.

## (ii) A preferencia (minerar commits que CORRIGEM referencia) foi MEDIDA, e nao se sustenta

A coordenadora preferia minerar o historico git do estrato oss: commits que corrigem uma referencia,
com o PAI como positivo natural. **Eu medi o sinal antes de propor** (medir antes de construir), e o
resultado muda a proposta:

`scratch/prova_sinal_commits.py` — 3 repos HTML com `.git` real sob `~/.claude/touring/vgp/oss/`
(`html5-boilerplate` 381 commits tocando `.html`, `dom-examples` 487, `express` 255; **1.123 no
total**), amostra dos 120 mais recentes de cada = **360 commits**. Criterio: quebrado no PAI e
corrigido no FILHO, pela mesma classe.

```
=== TOTAIS na amostra de 360 commits ===
  recurso      0
  ancora       0
  classe_css  10
  cdn_pin      0
```

**E os 10 de `classe_css` sao falso positivo do detector, nao positivos naturais.** Verifiquei o
primeiro a mao (`html5-boilerplate@c37e7b7e06`, "Remove JS-conditional classes"):

```
-<html class="no-js" lang="">     pai: usa no-js=True   define .no-js={...}=False
+<html lang="">                   filho: usa no-js=False define .no-js={...}=False
```

O commit **removeu** uma classe que nunca teve definicao no arquivo. `.no-js` e o fallback que o
JS apaga; nao e uma referencia quebrada que alguem corrigiu. Meu detector de `classe_css` nao checa
se a classe tem definicao **no proprio arquivo** (so no CSS embutido), entao marca qualquer classe
sem estilo inline. Como nao defini CSS externo, o detector e estruturalmente enviesado para `True`.

Se 360 commits nao contem UM unico commit que corrija uma referencia local ou uma ancora quebrada,
a proposta de minerar nao entrega o denominador — e o honesto e dizer que o sinal rareia, nao
propor mesmo assim.

## (iii) PROPOSTA (nao selada)

### Opcao A — denominador de classes HTML no estrato semeado, com estrato proprio

O estrato semeado ja tem 20 positivos por classe HTML e o selftest ja os prova. O que falta e que
eles **nao contam** para o juiz J porque J mede so o held-out. Proposta: um `corpus_holdout_html`
separado, com o mesmo gerador e a mesma convencao ja corrigida, **rotulado como `semeado`, nunca
como `natural`**. n esperado: 20 por classe (o mesmo do semeado). Custo: um gerador, ja escrito.
Risco: passa a medir o gerador contra si mesmo — por isso fica num arquivo separado e nunca
misturado ao natural.

### Opcao B — minerar commits, com o sinal real (a proposta original, corrigida)

O sinal existe, mas rarissimo em HTML de repo publico: os devs corrigem o **texto**, nao a
**referencia**. O que aparece e `classe_css`, e so quando o detector e corrigido. Para `recurso` e
`ancora`, a fonte natural nao e repos publico: e o historico de sessao de um agente, onde o `src`
quebrado e corrigido porque a ferramenta complained. **Fonte real: os transcripts ja minerados.**
Custo alto, ganho incerto.

### Opcao C — declarar as classes nao provadas (o que eu recomendo AGORA)

Ate aqui, o juiz J mede simbolo TS/TSX. As 5 classes HTML sao **"nao provadas, n=0"** — nao "recall
0". O recall delas nao e 0; e **inexistente**. Registrar assim e honesto; forcar n e fabricar.

### n esperado e criterio de "sem n suficiente"

- **n por classe**: opcao A da 20 (o mesmo do semeado, ja provado pelo selftest); opcao B, do
  medido: 0 para `recurso`/`ancora`/`cdn_pin`, indeterminado para `classe_css` ate o detector
  ser corrigido.
- **Criterio**: uma classe so e declarada **medida** com n >= 20 (o mesmo piso do semeado) E
  denominador independente do gerador (natural, held-out). Abaixo disso, ou sem natural, o rotulo e
  **"nao provada, n=X"** e ela NAO entra na media de recall agregada. Sem excecao por convenience:
  um denominador pequeno nao e um recall, e um recall pequeno com n=3 e ruido, nao medida.

## Pendencias que mudam isto

1. `corpus_rust.py` (sanidade) tem `contexto` como STRING e o juiz quebra com TypeError (achado do
   agente da VRS). Isso e independente das classes HTML, mas e o que impede QUALQUER medicao do Rust
   rodar hoje — inclusive deste denominador.
2. Se `n >= 20` for animado para HTML natural, a opcao C so e temporaria.
---

## REGRA DO JUIZ J (pre-registrada pela coordenadora em 10/10, adotada aqui)

> Uma classe só é **medida** com **n ≥ 20** E **denominador independente do gerador**. Abaixo
> disso, o rótulo é **«não provada, n=X»**, a classe fica **fora da média agregada** e **nenhuma
> afirmação de recall** é feita sobre ela.

`corpus_semeado` **não** é prova em hipótese alguma. Ele mede o gerador contra si mesmo.

## (iv) CONTAGEM nos transcripts — medida antes de construir (opção B)

Fonte: `scripts/conta_candidatos_html.py`. Universo: **1.814 transcripts** (analise 1.214, touring
586, mais worktrees/scratchpads), **87 edições `.html`** (Write/Edit). A classe vem da **tag e do
atributo**, nunca do nome do arquivo.

| classe | referências introduzidas | acima de n≥20? |
|---|---|---|
| `recurso` | **30** | sim |
| `ancora` | **27** | sim |
| `cdn_pin` | 45 | sim |
| `classe_css` | 822 | sim |
| `id` | 99 | (auxiliar) |
| `cdn` | 0 | **nao** |

**Isto e um TETO, nao uma contagem de candidatos.** O script conta toda referencia que uma edicao
introduziu, incluindo as que ja estavam no arquivo e as que nunca foram quebradas — ele nao sabe o
estado anterior do arquivo. O **oraculo por instancia** (a existencia no disco no instante da
edicao, via `estado_no_instante`) e quem decide, e so pode **reduzir** este numero.

Por isso a leitura honesta e: `recurso` e `ancora` **passam do piso antes do oraculo**, o que e
promissor mas nao prova nada; `cdn` fica em **«não provada, n=0»** sem discussao — e a unica classe
que o sinal nao alcanca.

Contrasting com o git (secao ii): la o sinal foi **ZERO** em 360 commits. Aqui o universo e
**1.814 transcripts / 87 edicoes** e o teto e 30/27. A diferenca nao e sorte: no transcript a
referencia quebrada nasce porque **a ferramenta reclamou** (o agente escreveu um `src` que o editor
ou o verificador apontou), enquanto no repo publico o dev corrige o texto e nao a referencia. Era o
que eu previa na opcao B, e agora medido.

## (v) UNIVERSO DISPONIVEL — medido, e muda o quadro

`scripts/mede_universo_html_oss.py`. Os 7 repos com `.git` real em `~/.claude/touring/vgp/oss/`:

| repo | commits | tocando `.html` | `.html` no HEAD | com referencia local |
|---|---|---|---|---|
| dom-examples | 1.277 | 487 | 199 | **149** |
| lodash | 7.710 | 489 | 6 | 5 |
| html5-boilerplate | 2.654 | 381 | 4 | 2 |
| express | 6.181 | 255 | 8 | 2 |
| excalidraw | 4.120 | 133 | 2 | 2 |
| zod | 3.248 | 42 | 2 | 2 |
| date-fns | 2.598 | 5 | 1 | 1 |
| **total** | **27.788** | **1.792** | **222** | **163** |

Contra o estrato dos transcripts (5 arquivos de `recurso`, 3 de `ancora`), este universo e de outra
ordem de grandeza: **163 arquivos de pagina com referencia local**, e cada um tem historico — o
`git show <rev>:<rel>` da regra de instancia responde "o alvo existia?" com precisao, sem depender
do disco de hoje. `dom-examples` sozinho tem 149.

**O proximo degrau NAO e este numero.** E contar os **pares** — pai quebrado, filho corrigido — com
o mesmo oraculo por instancia. Medir "commits tocando .html" sem contar pares foi exatamente o que
produziu os 10 falsos positivos da secao (ii): o detector de `classe_css` marcou `.no-js`, que o
commit REMOVEU. O que separa um positivo de um falso positivo e o par, com o oraculo, nao o
heuristica de "o commit tocou `.html`".

**Leitura honesta do quadro completo:** os transcripts continuam sendo o estrato mais honesto (a
referencia quebrada nasce porque a ferramenta reclamou) mas sao pequenos demais. O acervo `oss/`
tem o tamanho e o historico que falta. A proposta agora tem duas pernas, e o denominador de cada
uma so pode ser medido depois do oraculo por instancia — nao antes.

---

## (vi) FECHADO — o oráculo por instância no git, com a regra `abrir-tres-antes-de-agregar`

`scripts/oraculo_instancia_git.py`, corte de 40 commits × 7 repos, INSTANTE = C, BASE = `C^`,
árvore = `git ls-tree -r C`, par C→D exigido, `dist/build/node_modules/_site` excluídos,
utilitários (Tailwind/bootstrap) excluídos.

| classe | pares C→D | n≥20? |
|---|---|---|
| `recurso` | **0** | não |
| `ancora` | **0** | não |
| `cdn` | **0** | não |
| `classe_css` | **0** | não |
| `cdn_pin` | 329 | sim — mas classe de FORMA, fora da média de recall |

**Negativos confirmados: 166** (dom-examples 12, excalidraw 27, express 27, html5-boilerplate 36,
lodash 39, zod 24, date-fns 1). Exclusões: `recurso:existe_em_C` 1.349, `classe_css:utilitário` 22,
`recurso:build` 14.

### A regra pegou três defeitos meus, o número caiu 85 → 5 → 1 → 0

1. **O par aceitava o próprio commit.** `3016097 Add Launch Handler API demo (#316)` cria o
   `index.html` **e** o `images/apple-touch-icon.png` — o pai não tinha o alvo porque o arquivo
   acabara de nascer. Some-se uma URL absoluta de MDN resolvida contra o repo. Faltavam: `D ≠ C`,
   `D` toca o mesmo `.html`, e `http(s)`/`#` não são recurso local.
2. **`href="/favicon.ico"` é absoluto do site**, e eu resolvia como `src/favicon.ico` — que **existe**
   na árvore. A referência nunca esteve quebrada; o "D corrige" era reordenação de arquivo, com o
   `href` byte-idêntico em C e em D.
3. **A condição (a) comparava a referência com ela mesma.** `alvo_de(v) == alvo_de(v2)` casa
   `href="css/style.css"` consigo mesmo; faltava `v != v2`. O commit `b78f236 Remove initial-scale=1
   from viewport` só mexe numa meta tag.

### A leitura, e ela é o resultado

Em HTML de repositório público, commit que **corrige** referência local quebrada praticamente não
existe: o dev corrige o texto, não a referência. As 1.349 exclusões `recurso:existe_em_C` são o lado
disso — a referência apontava para algo que existia, e por isso não é positivo.

Sob a regra C, as quatro classes de forma ficam **«não provada, n=0»** para recall. O que resta
medível hoje é a **precisão nos 166 negativos**, com o binário integrado, no J.

### Custo (medido, porque duas execuções minhas não terminaram)

250 commits × 60 revisões filhas × `ls-tree -r` inteiro não converge. Três correções: `LIMITE_COMMIT`
= 40 · `-n` no próprio `git rev-list` (sem ele o git constrói a lista inteira de `c..HEAD` — o lodash
tem 7.710 — e só então corta) · memoização de árvore e filhos por `(repo, rev)`. Agora roda em < 60 s.

E uma lição de leitura de processo, que me pegou duas vezes nesta onda: subi o job e **não verifiquei
que estava progredindo** — estava em `hrtimer_nanosleep` sem consumo, com a saída de uma execução
morta. O sinal era ler `/proc/<pid>/fd/1` para saber qual arquivo a execução viva escrevia, não
`pgrep`, que traz as três.

---

## (vii) O `cdn_pin` de 329 era URL ABSOLUTA, não CDN — a quarta conferência

Corpus montado em `~/.claude/touring/vgp/corpus_holdout_html_git_20261010`
(`entradas.jsonl` + `snap/` + `selo.json`, ids pelo sha da proveniência, corte `2026-10-10T00:00:00Z`).
**Disjunção medida antes de montar**: os 296 paths do `corpus_semeado/html` estão todos em
`~/projects/analise`; 0 dentro do `oss/`; 0 colisão de sha256 contra os 5.032 blobs do HEAD dos 7
repos. A exclusão exigida é de **0 entradas** — medida, não presumida.

### O defeito, e quem o produzia

`classe_de` do oráculo classificava `cdn_pin` quando o valor começa com `http` e
`VERSAO.match(valor.rsplit("@", 1)[-1])` não casa. Numa URL **sem `@`**, o `rsplit` devolve a URL
**inteira**, que nunca casa com `^\d+\.\d+\.\d+` — então **toda URL absoluta sem `@` vira
`cdn_pin`**.

Abrindo os 3 primeiros, como manda `abrir-tres-antes-de-agregar`:

| o que o corpus dizia | o que é |
|---|---|
| `https://cldr.unicode.org` | fonte CLDR |
| `https://dx.doi.org/10.1098/rspa.2016.0404` | DOI |
| `https://eclipse.gsfc.nasa.gov/SEhelp/uncertainty2004.html` | NASA |

A separação por presença de `@` (que é como se marca versão numa URL de CDN) fecha o caso:

```
COM '@'  (cdn_pin real) : 0
SEM '@'  (URL absoluta) : 329  →  128x istanbul.js.org, 68x Wikipedia, 26x GitHub, 8x MDN, …
```

**O critério corrigido** — conferido na convenção que o vtransv emite no `corpus_semeado/html` novo
(`https://cdn.jsdelivr.net/npm/gsap@3.13/dist/gsap.min.js`): URL de CDN **com** segmento de versão
`@<v>` e `v` **não** exata `X.Y.Z`. Com esse critério: **0**.

### Resultado

| | n |
|---|---|
| `recurso` | 0 |
| `ancora` | 0 |
| `cdn` | 0 |
| `classe_css` | 0 |
| `cdn_pin` | **0** |
| **negativos** | **244** |

```
corpus_holdout_html_git_20261010
  entradas.jsonl sha256 = 460bb35b67fd617e894862d464554adf10b0034427bdfef5674ccc15169fc809
  244 entradas, todas `negativo`
```

As cinco classes HTML ficam **«não provada, n=0»** para recall. O que o estrato mede é a
**precisão**: 244 negativos limpos de 7 repos, com denominador independente do gerador.

Para recall de forma seria preciso um oráculo que encontre URL de CDN **real** (com `@versão`) — e
nos 7 repos, no corte de hoje, não há nenhuma.

> **Nota de consistência:** `scripts/oraculo_instancia_git.py` ainda classifica com o critério antigo
> e devolve 329. O montador decide o que entra no corpus e usa o critério correto; os dois não
> contam igual, e isso está registrado aqui em vez de escondido.

---

## (viii) `cdn_pin` pelo HOST — 12, não 329 (critério da spec do vtransv)

### As três versões do critério e o número de cada uma

| versão | critério | n | por que falhou |
|---|---|---|---|
| 1 | `cdn_pin` = URL `http` sem versão exata (o do `classe_de` antigo) | **329** | sem `@`, o `rsplit` devolve a URL inteira, que nunca casa → **toda** URL absoluta virou `cdn_pin`: 128× istanbul.js.org, 68× Wikipedia, 26× GitHub, NASA, DOI |
| 2 | `cdn_pin` = URL **com** `@<versão>` não exata | **0** | exclui `https://unpkg.com/react` e `cdn.jsdelivr.net/npm/gsap/dist/…` (sem versão), que a spec conta como aviso; e ignora cdnjs (`/ajax/libs/<lib>/<v>/`) e `code.jquery.com/jquery-<v>.min.js` (versão no caminho/nome) |
| **3** | **`cdn_pin` = host de CDN E versão ausente/`latest`/não exata** | **12** | — |

O critério vive num módulo só — `scripts/cdn_pin.py` — e tanto o montador quanto
`scripts/oraculo_instancia_git.py` importam dele. **Dois critérios para a mesma classe divergem, e a
divergência aparece como contagem diferente sem ninguém saber qual está certa.**

### O contrato em 21 casos, não em cobertura

`scripts/test_cdn_pin.py` — **21/21, rc 0**. Não é teste de cobertura: é o contrato escrito como
casos, cada um ou citado por alguém ou já quebrado um número. O caso
`code.jquery.com/jquery-3.6.0.min.js` **reprovou** na primeira versão (a versão está no nome do
arquivo, com sufixo depois) e foi o que introduziu `VERSAO_EM_QUALQUER`.

### Os três primeiros, conferidos à mão

```
excalidraw@62228e0bbb  excalidraw-app/index.html  https://fonts.googleapis.com
excalidraw@62228e0bbb  excalidraw-app/index.html  https://fonts.gstatic.com
excalidraw@62228e0bbb  excalidraw-app/index.html  https://fonts.gstatic.com/s/nunito/v26/XRXI3I6….woff2
```

São CDN de verdade (Google Fonts) **sem versão** — `cdn_pin` legítimo pela spec. **Não há
contaminação**: DOI, Wikipedia, NASA e istanbul sumiram porque o host não está na lista.

### Veredito pela regra C

| classe | n | situação |
|---|---|---|
| `recurso` | 0 | não provada |
| `ancora` | 0 | não provada |
| `cdn` | 0 | não provada |
| `classe_css` | 0 | não provada |
| **`cdn_pin`** | **12** | **não provada (n=12 < 20)** |
| **negativos** | **236** | medida de **precisão** |

O corpus ficou com 248 entradas: 236 negativas e 12 positivas de `cdn_pin`, que **não contam para o
recall** por ficarem abaixo do piso.

```
corpus_holdout_html_git_20261010
  entradas.jsonl sha256 = 7f6bb4192f2feaa2cdf3ae4fef69d483f1723c4b04c9c6f2dd3c4cec11fa30a6
  selo (FORA do corpus): diagnostics/holdout-html-git-20261010/selo.json
  sha_tree = b6966be51d13e5ee   arquivos = 249   bytes = 1.834.680
  confere_selo.py --corpus … --selo …  ->  selo INTACTO, rc 0
```

O `sha_tree` vem da função `sha_tree` **do próprio `confere_selo.py`**, importada — não reescrita. E
o selo mora **fora** do corpus: dentro, o `sha_tree` incluiria o próprio selo, e o conferidor agora
reprova isso.

## Fecho da frente

O que a onda entregou: a convenção de `recurso` corrigida e o corpus trocado com determinismo
idêntico; o `corpus_rust_sanidade_v2` com denominador provado contra o juiz de verdade; a régua
`abrir-tres-antes-de-agregar` virando regra do programa; e o estrato de HTML do git, que mede
**precisão** em 236 negativos e **não mede recall** em nenhuma das cinco classes.
