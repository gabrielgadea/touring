# PLANO — adjudicador html para o estrato OSS (item D-8, decidido em 01/11)

> **Decisão do Gabriel (01/11)**: construir o adjudicador html. Isto substitui o «html NÃO
> PROVADA por decisão» que a coordenadora tinha me passado. **Este documento é o plano, e vem
> ANTES de qualquer código** — o adjudicador só é escrito depois do plano aceito.
>
> Referência: `RELATORIO-V8-SEM2.md` §4.1 (o que o D-8 substitui) e §1 das pendências
> consolidadas. O estrato OSS de js/tsx/ts (o V-8 propriamente dito) está em curso e sai antes.

---

## 1. A fonte de verdade do rótulo

### 1.1 O que o juiz exige (lido no código, não inferido)

`scripts/juiz_motor_vgp.py`:

| linha | o que diz | consequência |
|---|---|---|
| 110 | `CLASSES_WEB = {"recurso", "ancora", "cdn", "cdn_pin", "classe_css"}` | são as classes web do VGP |
| 124 | «Classes web (VL0 §4) **não têm compilador**: o rótulo vem do oráculo do corpus, que exige a mensagem dele» | **o adjudicador É a fonte do rótulo** — não há oráculo de terceiro para html |
| 146 | «negativo (delta: o hook só nega o que a edição INTRODUZ)» | o rótulo é por DELTA, como no `oraculo_ls.js` |
| 22 | recall = positivos **de símbolo** com um achado (deny OU warn) cujo `nome` == `nome_esperado` | o rótulo do positivo precisa dizer **qual token** o motor deve achar |
| 23 | falso_deny = negativos com QUALQUER achado `deny`, por balde | o negativo é o denominador |
| 272 | `b, nome = balde(e), e["nome_esperado"]` | `nome_esperado` é lido em todo positivo |
| 185 | `sobre |= {c["path"]: corpus / c["snap"] for c in entrada.get("contexto") or []}` | `contexto` tem de ser **LISTA** (a string `"[]"` quebra o `for`) |
| 160-170 | `rodar_motor` chama `vgp check --path <path> --content-file <corpus>/<snap> --project-root <root> --lang <lang>` | a entrada precisa de `path`, `snap`, `project_root`, `lang` |

E a memória `corpus-rotulo-que-o-juiz-nunca-casa` (10/10), que é a armadilha escrita por quem já
caiu nela: **`nome_esperado` é o token referenciado COMO ESTÁ NO CÓDIGO** — para `recurso`, sem a
query `?…` e sem o fragmento `#…`; e qualquer predicado que existence-teste disco a partir do
valor cru do atributo acusa os DOIS lados (mutado e original), o que é a assinatura de que não é
oráculo.

### 1.2 As regras do rótulo

| rótulo | regra |
|---|---|
| **negativo** | a edição **não introduz** nenhum achado novo → o motor **não** deve negar (um deny aqui é falso deny) |
| **positivo** | a edição introduz ≥ 1 achado novo; `nome_esperado` = o token, como está no código; `mensagem` = a mensagem do adjudicador; `reproduz` = `true` (o defeito está no conteúdo do commit, reproduz por construção) |

O formato da entrada é o de `~/.claude/touring/vgp/corpus_semeado/html/entradas.jsonl` (o
modelo que o juiz já consome):

```json
{"classe": "ancora", "contexto": [], "id": "...", "lang": "html",
 "mensagem": "id 'X' ausente no documento", "nome_esperado": "X",
 "origem": "natural", "path": "<abs>", "project_root": "<abs>",
 "reproduz": "true", "rotulo": "positivo", "snap": "snap/<id>.html"}
```

### 1.3 As classes — e o corte que a honestidade impõe

As cinco classes web do VGP **não precisam do mesmo oráculo**:

| classe | o que é | precisa da árvore de arquivos? | precisa da rede? |
|---|---|---|---|
| `ancora` | `href="#id"` cujo id não existe no documento | **não** | não |
| `cdn_pin` | URL de CDN sem pin de versão (ou com pin vencido) | não | não (a forma da URL) |
| `cdn` | URL de CDN | não | não (a forma) |
| `recurso` | atributo que aponta para arquivo local inexistente | **sim** | não |
| `classe_css` | classe usada que não existe em nenhuma folha do projeto | **sim** | não |

**O problema medido de `recurso` e `classe_css`**: resolver um caminho relativo exige a árvore do
**commit**, e o clone está em HEAD (o `exige_clone_limpo` garante isso). Um arquivo html de um
commit de 2024 referencia recursos que existiam em 2024 e talvez não existam em HEAD → o
adjudicator acusaria um `recurso` inexistente que não é defeito nenhum: **o rótulo viria do
drift da árvore, não da edição**. Esse é exatamente o defeito que a memória
`corpus-rotulo-que-o-juiz-nunca-casa` descreve em outra forma (o rótulo que não implementava a
descrição).

**=> Proposta: a primeira camada do adjudicador cobre `ancora` + `cdn_pin`, que dependem SÓ do
documento.** Sem filesystem, sem rede, sem drift. `recurso` e `classe_css` ficam para uma segunda
camada que resolva contra a árvore do commit (via `git cat-file`), e são declarados como
**desvio declarado** até lá — não como falha do motor.

---

## 2. Independência: o que impede o adjudicador de confirmar o motor

O risco real: usar o próprio adaptador html do VGP como adjudicador seria **o motor confirmando
a si mesmo**. Um bug do adaptador viraria «achado» e o juiz mediria recall 1,0 sobre um corpus
que não prova nada.

**A independência proposta é de implementação, não de especificação** — e isso precisa ser dito
com todas as letras: para html **não existe uma implementação de referência** de «este `#id`
resolve?» (é checagem de aplicação, não de spec). A defesa é em quatro camadas:

1. **Parser diferente**: o adjudicador usa o `html.parser` da stdlib do Python; o adaptador usa a
   gramática tree-sitter do crate. Dois parsers, uma gramática: um deslize de parsing aparece como
   divergência entre eles, não como concordância silenciosa.
2. **Tokenização pela regra do juiz**: o `nome_esperado` sai do token **sem query/fragmento**, e
   o predicado existence-testa o **token**, nunca o valor cru — as duas lições da memória.
3. **Controles sintéticos** (§4): casos com resposta conhecida por construção, positivos e
   negativos, com o mutante (o controle tem de **reprovar** quando o predicado é quebrado).
4. **Cruzamento com o validador de referência do WHATWG (Nu `vnu`)** — o `java` **está instalado**
   nesta máquina. O `vnu` é a referência independente mais forte que existe para html, e ele é
   usado como **CONTROLE CRUZADO**, não como fonte de rótulo:
   - o adjudicador **não pode** acusar um erro de spec num html que o `vnu` aceita → prova de que
     os achados dele são de **aplicação**, e não de spec;
   - um html que o `vnu` reprova tem de ser registrado por uma classe nossa, senão há um buraco.

   **Por que o `vnu` NÃO pode ser a fonte do rótulo**: as classes do VGP (`ancora`, `recurso`,
   `classe_css`) não são erros de spec — o `vnu` não checa se um `#id` existe. Um rótulo de `vnu`
   seria «este html viola a spec», e o motor do VGP **não reporta erro de spec** → o recall
   dessa classe sairia **0 por construção**, que é a armadilha da memória.

   O `vnu` exige o jar baixado da rede (uma vez, congelado por sha256) — se a rede não estiver
   disponível, o controle cruzado fica **declarado como não feito**, e o adjudicador continua
   válido para o que ele cobre.

---

## 3. Onde ele roda

| | |
|---|---|
| clones | `~/.claude/touring/vgp/oss/html5-boilerplate` e `~/.claude/touring/vgp/oss/dom-examples` (já existem, fora do workspace) |
| repositórios adicionais | **se o n < 299** — e vai ser: a população medida é curta (§5) |
| onde vive o adjudicador | `~/.claude/touring/vgp/oss/oraculo_html.py`, **ao lado** do `oraculo_ls.js` (que é congelado e vive fora do repo) — e o sha256 dele vai no selo |
| forma de rodar | um processo por par, protocolo **linhas JSON em stdin, frames `#<n bytes>` em stdout** — o MESMO protocolo do `oraculo_ls.js`, e **lido em BYTES** (o defeito que matou o run anterior: `#<n>` em bytes lido em caracteres trava para sempre num corpo multibyte) |

---

## 4. Testes do adjudicador — antes de rotular qualquer coisa

O repositório exige RED→GREEN com **controle positivo** e **mutante morto**; um teste que passa
sem provar nada é a modalidade de falha que o próprio bundle documenta.

| # | teste | o que prova |
|---|---|---|
| T1 | **controle positivo**: um html sintético com `href="#nao-existe"` e nenhum id `nao-existe` → o adjudicador acha 1 achado, classe `ancora`, `nome_esperado == "nao-existe"` | o predicado acha o que deve achar |
| T2 | **controle negativo**: o mesmo html **com** o id presente → 0 achados | não acusa o que está certo |
| T3 | **mutante**: o predicado existence-testa o valor cru (`x.html?v=2`) em vez do token → tem de **reprovar** | o token, não o valor cru (a lição da memória) |
| T4 | **mutante**: a comparação passa a ser «existe ALGUM id com esse prefixo» → tem de reprovar | o id é exato |
| T5 | **delta**: html antes/depois onde a edição **introduz** um id ausente → 1 achado NOVO; onde a edição não muda nada → 0 achados novos | o rótulo é por delta, como o `oraculo_ls.js` |
| T6 | **multibyte**: um html com acento → o frame volta **inteiro** (o defeito de bytes/caracteres) | o protocolo não trava |
| T7 | **controle cruzado (vnu)**: um html que o `vnu` aceita → o adjudicador não acusa erro de spec | os achados são de aplicação |
| T8 | **idempotência**: dois `cmd` seguidos → os dois respondem (o `\n` do frame não vira cabeçalho) | o framing |

---

## 5. A população: o n que o html consegue alcançar

**Medido** (mesmo método do `v8_populacao.py`, corte 2026-10-09 23:00):

| `--max-commits` | pares html elegíveis |
|---|---|
| 120 | 161 |
| 300 | 446 |
| **600** | **518** |
| 1000 | 557 |
| 2000 | 914 |

Origem: `dom-examples` 479, `html5-boilerplate` 18, `excalidraw` 10, `date-fns` 2, `zod` 1.

**O problema**: com o rendimento medido no estrato js/ts (≈ 50 % dos pares viram entrada), 518
pares dão **≈ 260 negativos — ABAIXO de 299**. Mesmo em mc=2000 (914 pares → ~460) o número
dependeria de o rendimento se manter.

**=> O html precisa de mais repositórios públicos com histórico html denso**, clonados fora do
workspace, escolhidos **por medição** (pares elegíveis por repo), não por palpite. Candidatos a
medir: `jquery/jquery` (html de teste), `jekyllrb/jekyll` e `jekyllrb/minima` (layouts),
`whatwg/html` (o spec tem exemplos), `mozilla/developer-blog`, `pages-themes/*`. A lista fina é
**medida** (pares elegíveis por repo, com o mesmo filtro), e a clonagem só entra no plano se o
repo passar de um limiar de pares.

**Com classes que dependem só do documento (`ancora`), o yield é alto** (uma edição de html que
não quebra âncora é negativa) — de modo que o n é limitado pela **população de pares**, não pelo
rendimento. É esse o número que decide se mais repositórios são necessários.

---

## 6. O teto, o corte e a disjunção — os mesmos do estrato js/ts

| | |
|---|---|
| teto de concentração | 3 % **na seleção** (`teto_de(n_lang, 0.03)`), medido contra o disco no selo (`concentra_dentro_do_teto`) |
| corte temporal | 2026-10-09 23:00 (o mesmo hardcoded do `rotular_oss.py`) |
| disjunção | a flag `--excluir-corpus` que acabei de criar no rotulador: os ids de **7 527** entradas de corpora anteriores saem da população **antes** da seleção, então não consomem oracle nem entram no teto |
| selo | **externo**, no formato do `confere_selo.py` (`sha_tree`/`arquivos`/`bytes`, fora do corpus), conferido com `confere_selo.py --corpus <dir> --selo <selo.json>` → rc 0 |
| selo do adjudicador | `oraculo_html_sha256` + `oraculo_html_bytes` + a versão da regra de tokenização, gravados no selo (como o `oraculo_ls_sha256` do `oraculo_ls.js`) |

---

## 7. O critério de aceite

O mesmo do resto do V-8:

- classe com **≥ 299 negativos**, **dentro do teto** e com o **selo conferido** → sai de «não
  provada»;
- abaixo disso → **segue declarada como não provada, com o n e o motivo**. Sem inflar o número.

**E uma advertência que precisa entrar no relatório**: o html natural renderiza **positivos
raros** (um commit que quebra um `#id` é incomum), então o **recall** do html provavelmente não
será mensurável neste estrato — o valor dele é o **lado do falso deny** (n de negativos). O recall
do html continua sendo o do corpus semeado (`corpus_semeado/html`, 100 positivos). Isso é
declarado, não escondido.

---

## 8. O que NÃO entra no escopo (e por quê)

| não entra | por quê |
|---|---|
| `recurso` e `classe_css` | exigem a árvore do commit; com o clone em HEAD o rótulo viria do drift da árvore, não da edição → desvio declarado até a segunda camada |
| classes de spec (`vnu` como fonte de rótulo) | o motor do VGP não reporta erro de spec → recall 0 por construção |
| rede no lote | o resolvedor de CDN não fala com a rede; o `vnu` (jar) é baixado uma vez, fora do lote, e congelado por sha |
| qualquer coisa no repositório | o corpus não vai para o repo (só o código do adjudicador e do selo, que são scripts versionados) |
| `update-touring`, `propagate-release.sh`, `daemon-ctl restart/stop`, daemon spawnado à mão | o AB34 está armado até 08/11 00:10 |
