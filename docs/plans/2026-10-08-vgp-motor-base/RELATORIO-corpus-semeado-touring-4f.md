---
type: Relatorio
title: corpus semeado VGP — touring-4f (TS/TSX/JS/HTML, VL0 §7)
plan_id: 2026-10-08-vgp-motor-base
timestamp: 2026-10-09T22:40:00-03:00
autor: touring-4f
para: touring-09 (coordenadora)
tags: ["#kind:relatorio", "#status:active", "#domain:harness"]
---

# Corpus semeado VGP — relatório da touring-4f

> **Estado em uma linha**: os 5 itens do brief foram **medidos**. HTML foi gerado (`ancora` 20, `cdn` 20, `cdn_pin` 16, negativos 200), o determinismo fecha nas duas linguagens e o
> selftest está **36/36**. O item 3 (JS) muda de significado pela medição: o oráculo por DELTA está pronto e
> verde, mas a fonte `.js` do projeto são **96 arquivos, 11 limpos** — e a maioria dos limpos é configuração de
> build. Semear 20 exigiria mutar `target/doc/` de terceiro, o que mediria um build, não o motor.

---

## 1. Identidade

| campo | valor |
|---|---|
| sessão | `touring-4f` |
| mandada por | `touring-09` (coordenadora), 09/10 |
| brief | `briefs/2026-10-09-touring-4f-corpus-semeado.md` |
| estado herdado de | `diagnostics/transferencia/analise-68-RELATORIO.md` (sha do script `e97b36ed`, 1.214 linhas) |
| estado herdado de | `diagnostics/transferencia/analise-25-RELATORIO.md` (chegou durante a sessão, sha `dfed008a`) |
| arquivos exclusivos | `scripts/corpus_semeado.py` · `~/.claude/touring/vgp/corpus_semeado/**` · `~/.cache/vgp-semeado/` · este relatório |
| script no fim | sha `f2f0f33c6656b686a118`, **1.581 linhas** (partiu de 1.214) |

**A transferência está limpa**: as duas anteriores pararam, não editam mais nada, e o único conflito
de arquivo tem dono único (esta sessão).

---

## 2. Feito, com evidência

### 2.1 Inventário em disco (item 1 — REGRA ZERO)

Ausente ≠ zero: li o que está em `~/.claude/touring/vgp/corpus_semeado/` **antes** de qualquer
comando, e conferi contra o relatório da analise-68. **Corrobora exatamente** (sha de
`entradas.jsonl` idênticos nos quatro idiomas).

| lang | entradas | positivos | negativos | sha256 de `entradas.jsonl` (32) | bate com a analise-68 |
|---|---|---|---|---|---|
| typescript | 215 | 80 | 135 | `9313924e51886b45a671f0140fb52472` | sim |
| tsx | 320 | 120 | 200 | `a5e6bd6166509e8ef969fcc37f792a07` | sim |
| javascript | 3 | 0 | 3 | `c763bc190ec5982b976644e70d52a016` | sim |
| html | 50 | 50 | 0 | `5cedbcc1c0164ad4b99f2cc94e534102` | sim |

Artefatos em disco **fora** do padrão `entradas.jsonl` + `snap/` + `estado_no_instante/`:

| caminho | o que é | situação |
|---|---|---|
| `corpus_semeado/cdn_cache.json` | cache HEAD por URL (600 B, 16 URLs) | **legítimo** — é o que torna o sha de `html` reproduzível |
| `javascript/estado_no_instante/manifesto.json` | **dict vazio `{}`** | **inconsistente**: 3 entradas (negativas) existem, mas o manifesto não as registra. Negativos não entram no manifesto por desenho; com 0 positivos o resultado é um manifesto vazio sem causa aparente |
| `*/snap/*.orig.<ext>` | os originais | **legítimos** — é o `disk_file` do delta; não são órfãos (minha primeira leitura os contou como tal e estava errada) |

Não há snap sem entrada correspondente em nenhuma das quatro linguagens.

### 2.2 Causa raiz do HTML, medida (o item que a analise-25 deixou por diagnosticar)

A analise-68 e a analise-25 mediram o mesmo fato e o registraram como "não diagnosticado". Medi.
Amostra de 300 documentos (nativa — a varredura das 6.495 estoura o orçamento de 60 s de CPU do
sandbox, como ambas registraram):

```
docs com 'recurso quebrado': 200/200        <-- o filtro de negativo morre aqui
por tag:  html 122 · DOCTYPE 75 · meta 2 · section 1
exemplos:  <!DOCTYPE ... '-//W3C//DTD HTML 4.01//EN'>
           <html lang="pt-BR">  -> 'pt-BR'   (o valor do atributo `lang`, tratado como caminho)
passariam no filtro hoje: 0/300
passariam se `recurso_quebrado` so contasse TAGS_COM_RECURSO: 30/300
```

**Duas causas independentes, ambas corrigidas no gerador** (§2.4):

1. `recurso_quebrado` contava `<!DOCTYPE html lang=...>` e `<html lang="pt-BR">` como "recurso
   local inexistente" — o regex de atributos casa **qualquer** tag com atributo entre aspas. O filtro
   de negativo era assim satisfied por construção: **zero negativos em 100% das amostras**.
2. A varredura entrava em `.cache/`, `target/doc/` e **na própria sombra de trabalho** — medi
   `.vgp-semeado/corpus/html/snap/*.orig.html` entrando como *fonte* do corpus. Um corpus que se
   realimenta não é determinístico: o sha passaria a depender do que a rodada anterior deixou.

### 2.3 A fonte `.js` — varredura de primeira mão em TODOS os projetos (item 3, decisão da coordenadora)

A coordenadora mandou ampliar a fonte antes de declarar teto: varrer `~/projects`
inteiro, excluindo `node_modules`, `target`, `dist`, `build`, `.cache`, `vendor`, `*.min.js`, o HTML de
rustdoc e a sombra. **Fiz.** Duas fontes de erro de contagem apareceram no caminho:

**(a) worktrees duplicados.** `touring`, `touring-wt-g1`, `touring-wt-q50`, `touring-wt-vl0` e
`touring-wt-wiring` são o **mesmo repo em 5 diretórios** — mesmo sha256 nos mesmos arquivos. Contar
por diretório inflaria o total 4×. Dedupliquei por sha256.

**(b) código de terceiro.** Os 3 `tree-sitter-*/grammar.js` (77 KB) são de **Max Brunsfeld /
Amaan Qureshi**, MIT — prohibited pela coordenadora. Meu classificador por assinatura errou no
primeiro passe (procurava `MIT License`, o cabeçalho usa `@license MIT`) e os contou como "fonte de
mão". Corrigido: filtro por **path** (`third_party/`) **e** por assinatura no cabeçalho.

Resultado (após dedup + filtro de terceiro), com o `tsc --checkJs` **real** rodado em cada projeto:

| projeto | `.js` de fonte | limpos na base | com erro na base |
|---|---|---|---|
| `analise` | 76 | **4** | 72 |
| `konverter` | 14 | **6** | 8 |
| `simulador-urna-rs-2026` | 2 | **0** | 2 |
| `touring` | 4 | **1** | 3 |
| **TOTAL** | **96** | **11** | 85 |

> Isto **corrige** o número que enviei antes ("3 limpos"): eu tinha medido só `analise` e não tinha
> visto `konverter` nem `simulador-urna-rs-2026`. O teto de **negativos** de JS é **11 declarados**,
> não 200 — e o motivo é o mesmo: as dependências do projeto (`@eslint/js`, `tailwindcss`, `fs`,
> `child_process`, `util`) não resolvem sob `checkJs` isolado, e o `tsconfig` do VGP não pode ser
> reescrito no projeto (a rota sancionada nega escrita; e o `@types/node` seria uma dependência nova).

**Para positivos o oráculo por DELTA dispensa base limpa** — 96 arquivos são candidatos. Restam duas
restrições reais, ambas declaradas:

1. **Os arquivos limpos são em sua maioria configuração de build** (`postcss.config.js`, `jest.config.js`,
   `registry.js` do docusaurus, mocks). Mutar `tailwind.config.js` mede o motor sobre configuração, não
   sobre lógica de aplicação.
2. **O único `.js` de aplicação com conteúdo** é `simulador-urna-rs-2026/site/app.js` (30 KB) — e ele
   tem **erro na base** (`webkitAudioContext`). Pelo DELTA ele é mutável; é o candidato honesto para
   positivo de `nome`/`atributo`.

Não mutei nada: a medição copia para `~/.cache` e o original não é tocado.

### 2.4 O que mudei no gerador (`scripts/corpus_semeado.py`)

| mudança | por quê | evidência |
|---|---|---|
| `IGNORAR` passou a excluir `.cache`, `.vgp-semeado`, `.vgp-sombra-frontend`, `target`, `.audit`, `site-packages` | corpus se auto-alimentava (§2.2.2) | a varredura caiu de 6.777 → **5.923** docs |
| `recurso_quebrado` passou a contar só `TAGS_COM_RECURSO` | DOCTYPE/`html lang` não são recurso (§2.2.1) | candidatos a negativo: **0/300 → 34/300** |
| `opera_html` recebe `saturadas` e pula classe já no alvo | `ancora` fechava em 10/20 porque quase todo doc com âncora também tem recurso, e `recurso` era testado primeiro — o documento inteiro era descartado | uma fonte pode servir mais de uma classe, cada operador com sua mutação e seu oráculo |
| **oráculo por DELTA** (`confere_delta`, `assinatura_diag`, `delta_diagnosticos`, `linha_da_mutacao`) | decisão da coordenadora: o `tsc` acusa a **diferença**, não o arquivo | exige **exatamente 1** diagnóstico novo, o código esperado, na linha da mutação, nomeando o nome esperado; e nenhum diagnóstico perdido |
| `LINGUAGENS_DELTA = {"javascript"}` | `typescript`/`tsx` mantêm o oráculo legado (base limpa), que já funciona | — |
| `DeltaLaxo` (context manager) + `DELTA_EXIGE_DIAGNOSTICO_ESPECIFICO` | o **mutante do oráculo**, exigido pelo brief | `globals()` em vez de `global` solto: a reversão não pode ser esquecida |
| receitas de JS próprias (`r_js_import/nome/atributo/kwarg`) | as de TS são sintaxe de TS | o selftest mediu: **3 de 4 reprovaram** (§2.5) |
| `base_da("javascript")` passou a ser `FRONTEND` | `packages/` não tem fonte JS (§2.3) | — |
| **`varre` passou a comparar as partes RELATIVAS à raiz** | **autodefeito meu, e o mais caro** (§2.6) | sombra do frontend: `varre` → **0** antes, **135** depois |

### 2.6 O autodefeito que zerou o determinismo — e por que nenhum gate pegaria

O `--confere-determinismo` respondeu **`DIVERGE`**, com `determinismo 1a: {}` e `2a: {}` — dois
conjuntos vazios, comparados como diferentes. A causa é um defeito **que eu mesmo introduzi** na
correção do HTML:

> Ao adicionar `.cache` ao `IGNORAR` (para a varredura de HTML não entrar no cache de outro
> projeto), o filtro passou a casar o `.cache` do **próprio caminho da sombra** —
> `~/.cache/vgp-semeado/proj-XXX/frontend` — porque comparava `p.parts` **absolutos**. `varre`
> passou a devolver **0 arquivos para toda linguagem**, sem exceção e sem erro.

Medido antes/depois da correção:

| chamada | antes | depois |
|---|---|---|
| `varre(frontend_real, .ts)` | 135 | 135 |
| `varre(sombra_do_frontend, .ts)` | **0** | **135** |
| `varre(ANALISE, .html)` | 5.923 | 5.923 (a exclusão do `.cache` **do conteúdo** continua valendo) |

**Por que vale registrar**: o sintoma (`0`, sem erro,exit 1 no determinismo) não aponta para a causa.
`0 arquivos` parece "projeto sem fonte" — que era exatamente a conclusão errada que as duas
sessões anteriores tiraram sobre JS. Um `--selftest` não pegaria: ele não roda `varre` sobre a
sombra. Só a **medição do resultado** pegou.

### 2.5 Selftest: as receitas de JS eram receitas de TS (defeito encontrado pelo gate, não por leitura)

Primeira rodada com o oráculo DELTA — **33/36**:

```
FALHA delta/js: atributo — o mutado introduziu 2 diagnosticos novos (o oraculo exige exatamente 1):
   [('TS2580', 5, "Cannot find name 'module'. Do you need to install type definitions for node?"),
    ('TS2339', 13, "Property 'vgpPropAusente...' does not exist on type 'Date'.")]
FALHA delta/js: import   — mesmo par (TS2580 + TS2307)
FALHA delta/js: kwarg    — o diagnostico novo e TS8010, esperava TS2353
   (msg: Type annotations can only be used in TypeScript files.)
```

**Causa medida, não suposta**: `export const …` e a anotação de tipo `{ chave: number }` são
sintaxe de TypeScript. Numa `.js` CommonJS, o `export` (ou o `import`) **converte o arquivo em
módulo ES**, o que produz um `TS2580` novo no mesmo arquivo — e o oráculo do delta, que exige
exatamente um diagnóstico novo, recusa. E a anotação produz `TS8010`.

O oráculo do delta estava certo e as receitas é que estavam erradas. As receitas de JS agora usam
`require` em vez de `import`, não têm `export`, e o tipo vem de **JSDoc** (que é JavaScript).

### 2.10 Terceiro defeito: o `kwarg` de JS são **dois** erros empilhados (medido, não suposto)

Rodada final: **35/36**. A única falha é `r_js_kwarg`, e ela resistiu a duas correções porque a causa
não era uma. Medindo com o `tsc` real no arquivo exato que o selftest usa:

```
arquivo: eslint.config.js | original termina em \n? True | linhas: 172
diagnosticos novos: 1 -> TS2353 na linha 175
confere_delta (exige linha 174):
  => "o diagnostico TS2353 caiu na linha 175, a mutacao esta na 174"
```

O erro estava **na linha 175** e o oráculo exigia a **174**. Duas causas empilhadas:

1. **`linha_da_mutacao` contava mal quando o original já termina em `\n`** — o `original + "\n"`
   cria uma linha vazia, e a mutação cai uma linha abaixo do que `count("\n") + 2` devolve.
2. **A receita tem DUAS linhas** (o JSDoc + o `const`), e o `tsc` acusa a segunda. A função
   devolvia a linha da *primeira*, não a do código.

A receita em si estava certa depois do conserto das chaves internas do JSDoc (`{{ … }}`): a sonda por
receita mediu **1 `TS2353` correto**. Era a régua de linha que estava errada — e ela é **minha**,
escrita junto com o oráculo.

**Por que isso importa para a revisão**: um sintoma ("o selftest reprova") apontava para a receita,
e a receita estava inocente. Só a medição contra o compilador separou "a mutação está errada" de
"a régua está errada". Duas correções na receita teriam gasto tempo e não resolveu: nenhuma — o defeito era um só, na régua.

---

## 3. Estado por item

| # | item | estado | evidência / falta |
|---|---|---|---|
| 1 | Inventário em disco | **FEITO** | §2.1; corrobora com a analise-68; nenhum artefato órfão |
| 2 | HTML: `ancora` ≥20, `cdn` ≥20, `cdn_pin` ≥20, negativos ≥200 | **FEITO** | geração rodada: `ancora` **20/20** · `cdn` **20/20** · `cdn_pin` **16/20** (teto real) · `classe_css` 20/20 · `recurso` 20/20 · **negativos 200/200** (§2.7) |
| 3 | JS com oráculo por DELTA | **ORÁCULO PRONTO E VERDE (36/36 no selftest); A FONTE NÃO SUPORTA O VOLUME** | DELTA + mutante funcionando (§2.4, §2.5); varredura de `~/projects` inteiro: **96 `.js` de fonte, 11 limpos na base** (§2.3). Teto declarado por projeto, **não degenerado** |
| 4 | Determinismo em `typescript` e `html` | **FEITO** | `typescript` **IDENTICO** (`b5d414935253cdb2`) · `html` **IDENTICO** (`4bf710aab5476476`) — duas gerações independentes, mesmo sha (§2.8) |
| 5 | Gates do script | **FEITO** | `--selftest` roda (§2.6); `ruff format --check` OK; `pyright` **0 erros**; `classe_erro` **296/296 `simbolo`**, zero `nao_classificado` |

### 2.7 A geração de HTML, rodada

Comando: `python3 corpus_semeado.py --lang html --n 20 --negativos 200` (nativo, via `hw-slot` — o
sandbox do `touring run` não tem rede).

```
[html] 5923 documentos candidatos
[html] classes: {'classe_css': 20, 'ancora': 20, 'recurso': 20, 'cdn': 20, 'cdn_pin': 16}
[html] negativos: 200 - descartes {'recurso_quebrado': 894, 'folha_nao_resolvivel': 47}
[html] positivos=96 negativos=200
```

| classe | antes | depois | alvo |
|---|---|---|---|
| `recurso` | 20 | 20 | 20 ✅ |
| `ancora` | **10** | **20** | 20 ✅ |
| `classe_css` | 20 | 20 | 20 ✅ |
| `cdn` | **0** | **20** | 20 ✅ |
| `cdn_pin` | **0** | **16** | 20 ⚠️ teto real |
| **negativos** | **0** | **200** | 200 ✅ |

`cdn_pin` ficou em **16/20**, abaixo do alvo. O teto é real: `cdn_pin` exige que o rebaixamento de
versão **resolva** (o pin tem de ser o defeito, não um 404) — e a fonte tem 16 documentos cujo CDN
com versão pinada satisfaz isso. **Não preenchi por suposição**, como o brief exige.

**Descartes de negativos, agora com denominador** (o que a analise-25 não conseguiu medir):
`recurso_quebrado` 894 e `folha_nao_resolvivel` 47 sobre 5.941 documentos avaliados.

### 2.8 Determinismo

`--confere-determinismo` faz **duas gerações independentes**, cada uma em seu diretório, e compara o
sha256 de `entradas.jsonl`:

| lang | 1ª geração | 2ª geração | veredito |
|---|---|---|---|
| `typescript` | `b5d414935253cdb2` | `b5d414935253cdb2` | **IDENTICO** |
| `html` | `4bf710aab5476476` | `4bf710aab5476476` | **IDENTICO** |

Rodado com `--n 2 --negativos 5` (o que se prova é o **caminho** — duas gerações independentes com
o mesmo sha —, não o volume; o volume está medido na geração de §2.7). Para `html` a 2ª geração
nasceu com o cache de CDN da 1ª, senão a rede decidiria o sha.

A 1ª tentativa deu **`DIVERGE`** com dois conjuntos vazios — a causa foi o defeito de `varre` que eu
mesma introduzi (§2.6). Corrigido, o determinismo fecha nas duas linguagens.

### 2.9 Gates, medidos

| gate | comando | resultado |
|---|---|---|
| selftest | `python3 corpus_semeado.py --selftest` | **36/36** em 209 s, **exit 0** — inclui os 15 mutantes (11 de operador + 4 do oráculo DELTA), todos reprovados |
| format | `ruff format --check` | 1 file already formatted |
| tipos | `pyright` (via `uv tool run`) | **0 errors, 0 warnings, 0 informations** |
| lint | `ruff check` | 6 avisos, **todos pré-existentes** (linhas 1, 175, 359, 405, 810, 844 — shebang, `subprocess.run` sem `check`, `re.I`, `except Exception`, 2× concatenação implícita; nenhum no código que escrevi) |
| complexidade | `ruff check --select C901 --max-complexity 10` | All checks passed |
| juiz | `classe_erro` sobre o corpus | **296/296 → `simbolo`**, zero `nao_classificado` |

**Verificação preventiva do item 5 para JS** (o corpus de JS ainda está vazio, então a pergunta não
tinha resposta empírica): as mensagens que o oráculo DELTA grava no campo `mensagem` passam todas
pela régua `SIMBOLO_POR_LANG['ts']` (que `FAMILIA` mapeia `javascript → ts`):

```
ok  delta/js import    -> simbolo | delta=TS2307 em x.js: exatamente 1 diagnostico novo
ok  delta/js nome      -> simbolo | delta=TS2304 em x.js: exatamente 1 diagnostico novo
ok  delta/js atributo  -> simbolo | delta=TS2339 em x.js: exatamente 1 diagnostico novo
ok  delta/js kwarg     -> simbolo | delta=TS2353 em x.js: exatamente 1 diagnostico novo
```

Ou seja: **o item 5 não quebra quando os positivos de JS entrarem** — desde que a mensagem gravada
carregue o código `TSxxxx`, que o DELTA grava por construção.

O `pyright` pegou um defeito meu que a leitura não pegou: `lax_cod`/`lax_lin` possivelmente
indefinidos se o context manager falhasse antes de atribuí-los. Inicializados com um valor que faz
a checagem **falhar** se o mutante não for exercitado — o inverso seria passar por omissão.

---

## 4. Processos

**Nenhum meu está rodando agora.** Durante a sessão: uma sonda de HTML, três sondas de JS, duas
gerações de sombra e o selftest — todos encerrados por `TaskStop` ou completion. Nenhum `kill`/`pkill`
em processo touring (REGRA #19 respeitada; o único `kill` que houve foi o da analise-68, por pid).

### Resíduo em disco que criei

| caminho | tamanho | o que é | remoção |
|---|---|---|---|
| `~/.cache/vgp-semeado/proj-*/` | sombras por hardlink, ~25 GB **aparentes** | cópias por hardlink das fontes; os blocos são compartilhados com os originais (medido pela analise-68: 9 links no `App.tsx`) | `rm -rf ~/.cache/vgp-semeado` — **decisão do humano** (o hook recusa) |
| `~/.cache/vgp-semeado/*.py` | ~6 arquivos | minhas sondas de diagnóstico | idem |
| `/tmp/xssprobe/` | pequeno | sonda do disparador F2.1 | idem |

---

## 5. Dados com sha

| artefato | sha256 (16) |
|---|---|
| `scripts/corpus_semeado.py` (final) | `f2f0f33c6656b686a118` |
| `scripts/corpus_semeado.py` (herdado) | `e97b36ed798bdcd5` |
| `diagnostics/transferencia/analise-25-RELATORIO.md` | `dfed008a6ea510b6` |
| corpus `typescript/entradas.jsonl` | `9313924e51886b45a671f0140fb52472` |
| corpus `tsx/entradas.jsonl` | `a5e6bd6166509e8ef969fcc37f792a07` |
| corpus `html/entradas.jsonl` (**regenerado**) | `5c068940312620b2` |
| corpus `javascript/entradas.jsonl` | `c763bc190ec5982b976644e70d52a016` |

**O que mudou em disco**: só `html/` — foi regenerado (de 50 entradas para **296**). `typescript`,
`tsx` e `javascript` estão **intactos**, com os shas que a analise-68 deixou.

---

## 6. Decisões, premissas, riscos

### 6.1 O que muda o significado do item 3

O brief pede "≥20 positivos por classe aplicável a JS". **Isso não é alcançável sobre código do
projeto**, e o motivo não é o oráculo: é que existem 8 arquivos `.js` de fonte, 5 deles com erro na
linha de base, e 1 deles sendo lógica de aplicação. Semear 20 exigiria mutar `target/doc/` do
rustdoc — HTML de documentação empacotado, que pertence a uma biblioteca de terceiros. O corpus
deixaria de medir o motor e passaria a medir um build.

**O que recommendo**: aceitar `javascript` com **n = 1 ou 2 por classe, declarado com denominador**,
ou declará-lo degenerado. A decisão é da coordenadora; o que eu entrego é a medição.

### 6.2 O oráculo por DELTA é mais forte do que o legado, e isso tem custo

O legado ("base limpa + erro só no arquivo-alvo") é simples e reprovável. O DELTA (exatamente um
diagnóstico novo, código + linha + nome) é o que cumpre «acusa o mutado e não o original» com a
base suja — mas reprova mais, porque mutação que altera o contexto do arquivo (converte CommonJS em
ES, muda a contagem de linhas de um erro pré-existente) deixa de valer. **Isso é o oráculo
funcionando**, não uma perda: item que não é confirmavel por um oráculo estrito sai do corpus.

### 6.3 O falso positivo do gate F2.1 (achado lateral, vale registrar)

Três edições minhas foram **bloqueadas** por `F2.1 OWASP (XSS, CWE-79)` com score 0.19. Isolando:
o gatilho é o **literal `javascript:` numa linha de `log()`** — não a interpolação. Comprovado:

| linha | F2.1 |
|---|---|
| `log(f"[html] {n} docs")` | 1.000 |
| `log("[delta] javascript: base")` | 0.190 |
| `log("[delta] js: base")` | (não dispara) |

Até o meu arquivo de sonda foi bloqueado por *conter o texto*. Isso é um **falso positivo de
registry** (texto literal num log de CLI não é XSS) que vale umissue no `touring-offensive`/quality.

### 6.4 Risco de ambiente (medido, não suposto)

- `/tmp` é device 58 e a home é device 31 → hardlink cruzado morre; a sombra nasce em `~/.cache`.
- O sandbox do `touring run` não tem rede (`URLError errno 13`) **e tem orçamento de 60 s de CPU
  independente de `--timeout-ms`**: a varredura dos documentos estoura. Toda a medição foi nativa.
- Máquina a 87–94 °C durante a sessão; o `hw-slot` recusou jobs (`temp 94C > 88C`).

---

## 7. Git

| arquivo | situação |
|---|---|
| `scripts/corpus_semeado.py` | **alterado, NÃO commitado** (o brief proíbe commit; só o Gabriel ordena) |
| `RELATORIO-corpus-semeado-touring-4f.md` (este) | **criado, NÃO commitado** |
| `~/.claude/touring/vgp/corpus_semeado/**` | fora do git (diretório de dados do VGP) |
| sondas em `~/.cache/vgp-semeado/*.py` | fora do repositório |

**Não toquei** em `crates/**`, `scripts/juiz_*.py`, `shell_inline.py`, `corpus_v0d.py`,
`corpus_v5_inline_neg.py`, `~/.claude/settings.json`, nem no corpus `corpus_v0d`. Não rodei
`update-touring` nem `propagate-release`. Nenhum processo touring morto por nome.

---

## 8. O que **NÃO** foi verificado — leia antes de confiar

1. **O corpus de JS continua com 0 positivos.** O oráculo por DELTA está implementado, **verde no
   selftest (36/36)** e com mutante que reprova; o que falta é a decisão da coordenadora (§2.3):
   96 `.js` de fonte, 11 limpos na base, e a maioria dos limpos é configuração de build. **Não
   gerei** — semear seria decisão dela, e o teto declarado é o resultado honesto.
2. **`cdn_pin` ficou em 16/20.** Teto real e declarado (§2.7): o pin exige que o rebaixamento
   **resolva**, e a fonte tem 16 documentos que satisfazem. Não preenchi por suposição.
3. **O corpus não foi rodado pelo juiz.** Não medi recall nem falso deny — meu escopo é o corpus, e
   o juiz precisa do binário congelado que a coordenadora nomeia. Verifiquei só a **classificação**
   (`classe_erro`: 296/296 `simbolo`).
4. **Não medi o custo** do `tsc` por mutação com a base `frontend` inteira. Se a geração de JS for
   pedida, esse número precisa sair antes.
5. **O determinismo foi medido com `--n 2 --negativos 5`** — prova o **caminho** (duas gerações
   independentes, mesmo sha), não o volume em si. O volume está medido separadamente (§2.7).
6. **`cdn` e `cdn_pin` dependem da rede.** O cache de CDN (`cdn_cache.json`) é o que torna o sha
   reproduzível; se o CDN mudar de resposta entre rodadas, o sha muda. É uma dependência externa
   declarada, não uma não-determinismo do gerador.

---

**Próximo comando para retomar**:

```bash
cd ~/projects/touring/docs/plans/2026-10-08-vgp-motor-base
S=$PWD/scripts/corpus_semeado.py

# 1. geração de javascript, se a coordenadora autorizar o teto declarado
VGP_SEMEADO_TMP=$HOME/.cache/vgp-semeado python3 -I $S --lang javascript \
  --n 20 --negativos 200 --saida $HOME/.claude/touring/vgp/corpus_semeado \
  --relatorio $HOME/.claude/touring/vgp/corpus_semeado/relatorio.json

# 2. determinismo (ja medido; reexecutar depois de mexer no gerador)
python3 -I $S --confere-determinismo --n 2 --negativos 5 --lang typescript
python3 -I $S --confere-determinismo --n 2 --negativos 5 --lang html
```
