# PLANO — adjudicador html para o estrato OSS (item D-8) — **v2, medido**

> **Decisão do Gabriel (01/11)**: construir o adjudicador html. Isto substitui o «html NÃO
> PROVADA por decisão» que a coordenadora tinha me passado. O plano vem **ANTES de qualquer
> código** — e esta é a **v2**, corrigida pela medição.
>
> **O que mudou da v1 para a v2 (e por quê)**: a v1 propunha *escrever* um adjudicador html
> novo, de camada 1, cobrindo `ancora` + `cdn_pin`. **A medição mostrou que ele já existe** —
> `scripts/oraculo_instancia_git.py` (331 linhas, 4 classes, disciplina C→D) e
> `scripts/cdn_pin.py` (21 casos em `test_cdn_pin.py`) — **e que um corpus html já foi montado
> e selado** em 10/10 (`corpus_holdout_html_git_20261010`, **236 negativos** + 12 positivos
> `cdn_pin`). A v1 também supunha que `recurso`/`classe_css` dependeriam do clone em HEAD (e
> por isso dariam drift); **isso é falso**: o oráculo resolve a árvore **no commit C**
> (`git ls-tree -r C`), que é exatamente a «segunda camada» que a v1 previa.
>
> **O bloqueio real, medido**: (1) **população** — o `LIMITE_COMMIT = 40` corta o rendimento em
> 240 negativos, abaixo dos 299; (2) **base do delta** — as entradas html não carregam o
> «antes», então o juiz não tem contra o que comparar; (3) **congelamento** — o selo não
> carrega o sha256 do adjudicador. **Nenhum dos três é «construir o adjudicador».**

---

## 1. O adjudicador que existe (e por que ele serve)

| peça | arquivo | o que faz |
|---|---|---|
| oráculo por instância | `scripts/oraculo_instancia_git.py` | INSTANTE = C, BASE = C^, árvore do instante = `git ls-tree -r C`, conteúdo = `git show C:<rel>`. Positivo exige **D** (um commit posterior que CORRIGE a referência): é o par C→D que prova o erro real. Negativo = commit que toca `.html` e cuja árvore em C confirma **0 introduções**. |
| critério de CDN | `scripts/cdn_pin.py` | o HOST do CDN decide (não a presença de `@`); 21 casos em `test_cdn_pin.py`. O oráculo e o montador **importam o mesmo módulo** — dois critérios para a mesma classe divergiriam em silêncio. |
| montador | `scripts/monta_holdout_html_git.py` | importa o oráculo (não reescreve), monta `entradas.jsonl` + `snap/`, sela no formato do `confere_selo.py`. |

**Por que o oráculo por instância é a resposta certa** (e não um detector de padrão): a versão
anterior, de padrão, produziu **10 falsos positivos** — o commit *removia* `.no-js`, não
corrigia referência quebrada. A prova C→D elimina isso: o alvo **existe em D** e **não** existia
em C.

**Independência — o que impede o adjudicador de confirmar o motor** (mantido da v1, com uma
correção): para html **não existe implementação de referência** de «este `#id` resolve?» (é
checagem de aplicação, não de spec). A independência é de **implementação**, não de spec:

1. o oráculo resolve a árvore por `git ls-tree -r C` + `git cat-file -e` — **nada** do adaptador
   html do VGP é importado;
2. o `nome_esperado` sai do **token** (a URL como escrita no atributo), nunca do valor cru;
3. **controles sintéticos** (§4) — positivos e negativos, com o mutante reprovando;
4. **controle cruzado**: o `vnu` (WHATWG, jar local) como CONTROLE, nunca como fonte de rótulo —
   porque as classes do VGP não são erros de spec, e um rótulo de `vnu` daria recall 0 por
   construção (a armadilha da memória `corpus-rotulo-que-o-juiz-nunca-casa`).

---

## 2. A população — MEDIDA em 01/11 (`mede_rendimento_html.py`)

Corte temporal `2026-10-10T00:00:00Z` (o mesmo do corpus de 10/10). Negativos = commit que toca
`.html` e **não introduz** URL de CDN sem versão exata (delta entre C e C^ — dois `git show` por
arquivo; o POSITIVO, que exige a busca de filhos, é o caro).

| repo | commits tocando `.html` | ≤ corte | negativos (limite 40) | (100) | (250) | (todos) |
|---|---:|---:|---:|---:|---:|---:|
| `dom-examples` | 487 | 487 | 40 | 100 | 250 | 440 |
| `lodash` | 489 | 489 | 40 | 100 | 250 | 478 |
| `html5-boilerplate` | 381 | 381 | 40 | 100 | 250 | 362 |
| `express` | 255 | 255 | 40 | 100 | 250 | 252 |
| `excalidraw` | 133 | 133 | 40 | 100 | 132 | 132 |
| `zod` | 42 | 42 | 35 | 35 | 35 | 35 |
| `date-fns` | 5 | 5 | 5 | 5 | 5 | 5 |
| **TOTAL** | 1 792 | 1 792 | **240** | **540** | **1 172** | **1 704** |

**Leitura**: a população **existe** e é folgada. O corte de 40 não é uma limitação do acervo — é o
teto do script, posto quando o custo era proibitivo (≈ 1 750 commits × 60 revisões filhas ×
`ls-tree` inteiro). A memoização posterior (`filhos_de`, `arvore`, ambos memoizados por rev)
trocou o custo: **o gargalo era o número de chamadas repetidas, não a duração de cada uma**.

**O limite de 40 → 100 já entrega 540 negativos**, acima dos 299 com folga, sem clonar um repo
novo. (A clonagem de repos adicionais fica como **reserva**, não como plano.)

---

## 3. Os três defeitos a corrigir ANTES de rotular

| # | defeito | medido | correção |
|---|---|---|---|
| 1 | **`LIMITE_COMMIT = 40`** corta o rendimento | 240 negativos (< 299) | limite **100** (540) ou **250** (1 172), medido; o custo é medido antes de subir |
| 2 | **as entradas html não carregam o «antes»** — sem `snap_antes` e sem `estado_no_instante`, o juiz não recebe `--disk-file` | o `snap_antes` é o que o juiz usa (`rodar_motor`: `elif entrada.get("snap_antes")`) | gravar **`snap_antes`** (o conteúdo em C^) por entrada: o delta passa a ser exato e o clone em HEAD **deixa de ser o «antes»** — o drift que a v1 temia |
| 3 | **o selo não carrega o sha256 do adjudicador** | o selo de 10/10 tem `orcamento` em prosa, nenhum hash | `oraculo_sha256` + `cdn_pin_sha256` + bytes + versão do critério, gravados no selo (como o `oraculo_ls_sha256` do estrato js/ts) |

**Um quarto item, de disjunção**: o id html é `sha256(repo|rev|caminho|classe|nome)[:12]` e o id do
estrato js/ts é `sha256(repo+commit+path)[:16]` — **esquemas diferentes**, então a disjunção por id
**não funciona entre eles**. A disjunção html tem de ser medida por **identidade do par**
(`repo`, `rev`, `caminho`) e por **sha256 do conteúdo**, contra **todos** os corpora — e o número
reportado, não presumido.

---

## 4. O que eu executo (passo a passo)

1. **`monta_camada_html_oss.py`** (novo, no bundle): importa o oráculo e o `cdn_pin.py` (não
   reescreve nenhum dos dois), monta a camada html nova num diretório **novo**, com:
   - `LIMITE_COMMIT` parametrizado (100), **corte antes do limite**;
   - `snap_antes` por entrada (o conteúdo em C^) — o defeito 2 corrigido;
   - `snap` (o conteúdo em C) — o estado que o motor julga;
   - o **delta** declarado: negativo = 0 introduções; positivo = ≥ 1 introdução de URL de CDN
     sem versão exata, com `nome_esperado` = a URL **como escrita**;
   - `classe`, `mensagem`, `reproduz`, `origem`, `path`, `project_root`, `contexto: []`
     (lista — a string `"[]"` quebra o `for` do juiz).
2. **Testes do montador** (antes de rotular): (a) controle positivo — um commit sintético que
   introduz uma URL sem pin **é** positivo, com o `nome_esperado` certo; (b) controle negativo —
   um commit que **não** introduz é negativo; (c) **mutante**: o `snap_antes` ausente tem de
   **reprovar** (é o defeito 2 virado teste); (d) **mutante**: o `nome_esperado` com o valor cru
   em vez do token; (e) **idempotência**: duas execuções dão os mesmos ids.
3. **Selar** a camada nova (selo **externo**, formato do `confere_selo.py`), com o
   `oraculo_sha256`, o `cdn_pin_sha256`, os denominadores por classe, o **teto medido**
   (`max(1, int(n × 0,03))` por chave) e a **disjunção** medida.
4. **Conferir** com `confere_selo.py` (rc 0) e **julgar** com `juiz_motor_vgp.py`, do mesmo jeito
   que o estrato js/ts.
5. **Atualizar** o `RELATORIO-V8-SEM2.md` com o n por classe, a concentração, o hash do selo e o
   **veredito por classe** — e a advertência de que o **recall** do html natural é
   provavelmente **não mensurável** (um commit que quebra um `#id` é incomum): o valor do html é
   o **lado do falso deny**.

---

## 5. O critério de aceite (o mesmo do resto do V-8)

- classe com **≥ 299 negativos**, **dentro do teto** e com o **selo conferido** → sai de «não
  provada»;
- abaixo disso → **segue declarada como não provada, com o n e o motivo**. Sem inflar o número.

**E o que NÃO entra**: `recurso` e `classe_css` continuam fora da camada 1 (exigem a árvore do
commit e o par C→D para a prova do alvo — a **segunda** camada, com o custo que ela tem); classes
de spec (`vnu` como fonte de rótulo); rede no lote; qualquer conteúdo de corpus no repositório
(o selo e o código são scripts versionados; o corpus, não).

**Regra do AB34 continua valendo**: nada de `update-touring`, `propagate-release.sh`,
`daemon-ctl restart|stop`, daemon à mão.
