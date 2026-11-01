# D-4 F21INNER — braço de XSS de DOM (`innerHTML` & cia.) na F2.1

> **Decisão de Gabriel (31/10/2026):** plano próprio, ativo. A F2.1 (OWASP, CWE-79)
> não tem braço para o *sink* de DOM `innerHTML` / `outerHTML` / `insertAdjacentHTML` /
> `document.write` / React `dangerouslySetInnerHTML` recebendo dado NÃO confiável. Só
> existia o braço posicional de URL `javascript:` (`f21-javascript-literal-posicional`).
>
> **Estado:** implementado como **AVISO** (warning). **Não bloqueia** e **não move o
> score da F2.1**. A promoção a bloqueante é decisão de Gabriel, depois do censo — o
> critério está na §8.

## 1. Qual detector a F2.1 de fato chama (rastreado, com `arquivo:linha`)

A F2.1 não tem detector próprio: ela delega. A cadeia viva, do gate ao padrão:

| # | Elo | Onde |
|---|---|---|
| 1 | verificador `F2_1_Owasp::measure` → `analyze_owasp` | `crates/touring-quality/src/verifications/f2_1_owasp.rs` (`measure` L295, `analyze_owasp` L229) |
| 2 | `owasp_scan` (allowlists: harness, detector próprio, pragma) → `owasp_matches` | `f2_1_owasp.rs` L145 → L177 |
| 3 | `SecurityAnalyzer::new().analyze(raw, lang)` | `f2_1_owasp.rs:180` |
| 4 | `SecurityAnalyzer::analyze` → `candidate_matches` → `registry.detect_every` | `crates/touring-analysis/src/quality/security.rs` (`analyze` L81, `candidate_matches` L119, `registry.detect_every` L120) |
| 5 | `PatternRegistry::all()` registra `XssPattern` | `crates/touring-offensive/src/vuln/cwe_patterns.rs:892-895` |
| 6 | `XssPattern::detect_every` (`<script`, URL `javascript:`, handler `on<evento>=`) | `cwe_patterns.rs:80-172` (regex em `:88`, `VulnMatch::new("XSS", …)` em `:159`) |
| 7 | pós-filtro `retain_exploitable` (comentário, `#[cfg(test)]`, handler constante, 1 achado por padrão) | `security.rs:143` |

**Os três lugares candidatos, e o veredito:**

- `touring-offensive/src/vuln/cwe_patterns.rs` (`XssPattern`) — **é o detector vivo do
  CWE-79**, mas é **compartilhado** com o caminho concolic/payload, que mede ENTRADA
  (`concolic.rs:449`, `:467`). O próprio `security.rs` já documenta a regra: o que é
  pergunta de **código** vive no juiz de código, nunca no `XssPattern` ("the concolic /
  payload path measures INPUT, where the handler IS the attack"). Um braço de *taint* não
  tem sentido sobre uma string de payload — não há função, parâmetro ou atribuição. Por isso
  o braço **não** foi para lá.
- `touring-analysis/src/quality/security.rs` — é o **ponto de integração** (`analyze`), e já
  hospeda o precedente do juiz de código: `python_sinks.rs` (S51, 25/09/2026: `eval(x)` e
  SQL interpolado em `execute`), chamado em `candidate_matches` (`security.rs:131`). O novo
  detector entra aqui, ao lado dele.
- `touring-analysis/src/quality/input_validation.rs` (F2.2) — **outra dimensão**. O fixture
  de `input_validation.rs:320` (`jsts_xss_sinks_flagged`) já pune `dangerouslySetInnerHTML`
  e `document.write(` como **substring, sem taint, só a presença do sink** (peso 0,6/0,5,
  WARN). Ela **não** olha `innerHTML =` (o fixture inclui `el.innerHTML = clean;` mas o teste
  só exige 2 violações, e nenhuma é o `innerHTML`) e **não** pergunta de onde vem o dado.
  Fica como está: F2.2 = "o sink existe"; F2.1 (novo) = "dado não confiável chega ao sink".
  Sobreposição conhecida e aceita (§9).

**Arquivos novos/alterados:**

| Arquivo | O quê |
|---|---|
| `crates/touring-analysis/src/quality/dom_sinks.rs` (novo) | o detector (taint intra-função, JS/TS) |
| `crates/touring-analysis/src/quality/mod.rs` | `pub(crate) mod dom_sinks;` |
| `crates/touring-analysis/src/quality/security.rs` | `SecurityReport::advisories` + chamada em `analyze` |
| `crates/touring-quality/src/verifications/f2_1_owasp.rs` | `OwaspScan::advisories` + trecho `advisory (NOT scored…)` na evidência |
| `crates/touring-analysis/tests/security_analyzer_test.rs` | 3 testes de integração |

## 2. Como é um AVISO e não um bloqueio (a garantia estrutural)

O gate P0 da F2.1 bloqueia todo `Write`/`Edit` com score < 0,5. Um falso positivo aqui
congela edições na máquina inteira. Por isso o braço **não entra** em `vuln_matches`:

- `SecurityReport` ganhou o campo `advisories: Vec<VulnMatch>`. O score
  (`1 - Σ severidade/10`) e o `combined_score` são calculados **só** sobre `vuln_matches`.
  Os avisos ficam fora **por construção**, não por um limiar bem escolhido.
- `OwaspScan::advisories` carrega os avisos; `OwaspScan::blocks()` e `score` não os leem.
- A evidência da F2.1 ganha o sufixo
  `; advisory (NOT scored, warn only): N DomXss (CWE-79) at line L — \`<linha>\``,
  que é o que o censo lê.
- Invariante testado: um arquivo com um achado P0 real (`javascript:`) tem o **mesmo**
  score, o mesmo número de `matches` e o mesmo `blocks()` com e sem um sink tainted ao lado
  (`advisories_never_move_the_score_of_a_file_that_also_has_a_real_hit`).

## 3. Fontes (o que é "dado não confiável")

Dentro da função envolvente mais interna (ou do nível de módulo, fora de qualquer função):

| Fonte | Observação |
|---|---|
| **parâmetros da função** (incluindo desestruturação `({ body })`, padrão `= default`, anotações TS, `...rest`, modificadores `private readonly`) | parâmetros de callback contam: `.then(d => …)`, `(e) => …`, `function (event) {…}` |
| `location.search` / `.hash` / `.href` / `.pathname` / `.host` / `.hostname` | também `window.location.*` e `document.location.*` |
| `document.URL` / `.documentURI` / `.cookie` / `.referrer` | |
| `window.name` | fonte clássica de DOM XSS |
| `event.data` | mensagem de `postMessage` |
| `fetch(…)` | o resultado de `fetch(...).then(...)` e de `await fetch(...)`; `.responseText` (XHR) |
| `URLSearchParams` | `new URLSearchParams(…)` e tudo derivado (`p.get('n')`) |
| `req` / `request` `.query` / `.body` / `.params` / `.headers` / `.cookies` | |

**Propagação** (dentro da mesma função, em ordem de fonte): `const|let|var x = e`, `x = e`,
`x += e` (só **adiciona** taint), `const { a } = e`, `o.k = e` (taint o objeto `o`). Uma
reatribuição simples a partir de expressão limpa **limpa** o nome (`h = '<b>'` depois de
`h` parâmetro). Um nome sai tainted de qualquer expressão que o mencione como variável
(não como propriedade `.x` nem chave de objeto `{ x: … }`).

## 4. Sanitizadores (o que limpa o taint)

- chamada cujo **nome** contém `sanitize` / `sanitise` / `escape` (último segmento do
  caminho, sem diferenciar caixa): `DOMPurify.sanitize(x)`, `sanitizeHtml(x)`, `escapeHtml(x)`,
  `_.escape(x)`. **Não** contam: `unescape` (o inverso) e `escapeRegExp` (escapa para regex,
  não para HTML) — testados como positivos.
- coerção numérica: `Number(x)`, `parseInt(x)`, `parseFloat(x)`, `Boolean(x)`.
- **caminho `textContent`**: atribuir a `.textContent` não é sink (o navegador trata como
  texto); e o idioma `t.textContent = s; el.innerHTML = t.innerHTML` escapa no caminho (a
  leitura `t.innerHTML` de um nó cujo `textContent` foi atribuído sai limpa).
- o argumento do sanitizador é removido por inteiro, inclusive dentro de `${…}`:
  `` `<p>${escapeHtml(name)}</p>` `` é limpo.

## 5. O que conta como seguro

| Forma | Veredito |
|---|---|
| literal de string `'…'` / `"…"` | seguro |
| template literal **sem** `${}` | seguro |
| concatenação de constantes (`'<b>' + 'x'`, `T + 'x'` com `const T = '…'`) | seguro |
| nome que nunca recebeu taint (não é parâmetro, não veio de fonte) | seguro — **desconhecido ≠ tainted** |
| 1º argumento de `insertAdjacentHTML` (a *posição*) | nunca é HTML; só o 2º argumento é julgado |

A regra é de **precisão primeiro**: só reporta quando um nome tainted ou uma fonte direta
chega ao sink. O que o braço não consegue provar fica quieto.

## 6. Limites explícitos (falsos negativos conhecidos, documentados em teste)

Intra-arquivo, **intra-função**, **sem taint entre funções**:

- resultado de chamada: `el.innerHTML = getInput()` com `getInput` devolvendo `location.hash`
  **não** é detectado (teste `cross_function_taint_is_documented_as_not_detected`);
- variável de módulo lida de dentro de função (`const q = location.search; function f(el){ el.innerHTML = q }`);
- captura por closure (`function outer(){ const q = …; return () => { el.innerHTML = q } }`; o
  parâmetro da função **externa** também não conta para o arrow interno);
- helper que repassa (`el.innerHTML = build(location.hash)` onde `build` é opaco ao braço —
  *o argumento* `location.hash` é fonte direta e **é** pego; o que não se pega é o retorno de
  `build` usado num sink de outra função);
- acesso por colchete `el['innerHTML'] = …` (a máscara de código apaga a string);
- `for (const x of tainted)` não vincula `x`; `$(…).html(x)`, `v-html`, `setHTMLUnsafe`,
  `createContextualFragment`, `srcdoc`: fora do escopo;
- corpo de `<script>` em HTML: **não** coberto (o `XssPattern` cobre `<script`/handlers por
  payload; taint dentro de HTML inline exigiria extrair o JS — fora desta onda);
- arquivo > 4 MiB: não analisado por este braço;
- ordem: o fluxo é "para frente" por posição; laço que realimenta o taint de uma linha
  anterior não é modelado.
- **sanitizador reconhecido pelo NOME, por substring** (`dom_sinks.rs`, função que testa
  `sanitize`/`sanitise`/`escape` no nome em minúsculas, excluindo `unescape` e `regex`): uma
  função qualquer cujo nome contenha a palavra (`escapeKey(x)`, `sanitizeLayout(x)`) **limpa**
  o taint sem escapar nada (falso negativo); um escapador real com outro nome
  (`renderizarCorpo`, que chama `escapar()` por dentro) **não** limpa (falso positivo — o
  caso 1 do censo §8). O corpo da função chamada nunca é lido.

Inversamente, o parâmetro de função ser sempre fonte **é** a causa de falso positivo
esperada: `function render(items){ el.innerHTML = items.map(...)… }` é reportado mesmo que
`items` venha de código confiável. É exatamente o que o censo (§7) mede.

## 7. Testes e mutantes

Testes (`cargo test -p touring-analysis --lib --tests` e `-p touring-quality --lib --tests`):

- `dom_sinks::tests` (12) — cada fonte × cada tipo de sink (34 casos positivos), TS
  (5 casos: anotações, desestruturação, `const x: T`), cada sanitizador (16 negativos),
  literais e constantes (14 negativos), citado/comentado/lookalike (13 negativos), só a
  família JS, **5 casos entre funções documentados como NÃO detectados** (assertando o
  comportamento atual), região da função mais interna, 5 formas de parêntese de controle de
  fluxo, parâmetros.
- `tests/security_analyzer_test.rs` (+3) — aviso nunca é `vuln_match`; achado real de XSS e
  seu score intactos ao lado de um aviso; só JS/TS.
- `f2_1_owasp::tests` (+3) — aviso não pontua nem bloqueia (`measure`/`check`), arquivo com
  hit P0 tem o mesmo score com e sem aviso, sanitizado/constante/Python sem aviso.

Total: **18 testes novos**. Gates (REGRA #21, uma execução sob `hw-slot`, todos rc=0):
`cargo check -p touring-analysis -p touring-quality --all-targets` · `cargo clippy … --all-targets
-- -D warnings` · `cargo test -p touring-analysis --lib --tests` (1058 lib, 0 falhas) ·
`cargo test -p touring-quality --lib --tests` (496 lib, 0 falhas).

**Mutantes à mão (5; todos MORTOS, cada um aplicado em `dom_sinks.rs`, rodando
`cargo test -p touring-analysis --lib dom_sinks`, arquivo restaurado e verificado por sha256):**

| Mutante | O que muda | Quem o matou |
|---|---|---|
| M1 | sanitizadores/escapers nunca neutralizam (`names_a_filter && lower.is_empty()`) | `a_sanitizer_in_the_flow_clears_the_taint` |
| M2 | reatribuição limpa deixa de limpar o taint (`remove` vira no-op) | `a_sanitizer_in_the_flow_clears_the_taint`, `literals_and_constants_are_safe` |
| M3 | parâmetros da função deixam de ser fonte | 5 testes (`every_source_reaches_every_kind_of_sink`, `typescript_annotations_…`, `control_flow_parentheses_…`, `a_sink_belongs_to_its_innermost_…`, `the_span_is_the_sink_token_…`) |
| M4 | sinks procurados no texto cru em vez da máscara de código | `quoted_reads_and_lookalikes_are_not_sinks` |
| M5 | `insertAdjacentHTML` julga o 1º argumento (a posição) | `every_source_reaches_every_kind_of_sink`, `literals_and_constants_are_safe` |

(Os mutantes foram escritos sem warning de compilação, para não derrubar o clippy de outra
sessão enquanto o arquivo estivesse mutado.)

## 8. Censo antes/depois e critério de promoção

**Método.** Binário `touring-quality` ANTES (cópia do `target/debug/touring-quality` de
17:20, sem o braço; fontes de analysis/offensive/quality sem commit posterior) e DEPOIS
(build com o braço). Para cada projeto: `score <dir> --dims F2.1` (o que o gate lê) e `score
<arquivo> --dims F2.1 --format json` por arquivo JS/TS (`git ls-files -co --exclude-standard`),
comparando valor, status e `blockers`. "Corpus do gate" = as regras de
`enumerate_security_files` (sem diretório com ponto, sem `SKIP_DIRS`, sem `*.min.*`; `vendor/`
e `third_party/` entram).

| | workspace touring | `/home/gabrielgadea/projects/analise` |
|---|---|---|
| arquivos JS/TS listados / no corpus do gate | 21 / 14 | 1050 / 598 |
| `score <dir> --dims F2.1` ANTES | 1,000 Pass, blockers `[]` (4099 arquivos) | 0,000 Fail, blockers `[F2_1]` (192 de 10853 falham) |
| `score <dir> --dims F2.1` DEPOIS | 1,000 Pass, blockers `[]` (4104 arquivos) | 0,000 Fail, blockers `[F2_1]` (192 de 10859 falham) |
| arquivos com valor/status/blockers alterados | **0** de 21 | **0** de 1050 |
| avisos ANTES | 0 | 0 |
| arquivos com aviso DEPOIS (todos / no corpus do gate) | 3 / **0** | 6 / **5** |
| sinks reportados no corpus do gate | 0 | 5 (1 por arquivo) |
| tokens de sink "crus" no corpus do gate (denominador) | 0 | 136 em 10 arquivos |

O score do F2.1 e o número de blockers **não mudaram** em nenhum dos dois projetos (a
contagem de arquivos do corpus subiu em 5 e 6 porque outras sessões criaram arquivos entre as
duas medições; a contagem de arquivos que FALHAM no analise ficou em 192). Dos 5 avisos do
analise no corpus do gate, 3 estão em arquivos que HOJE passam na F2.1 (valor 1,000) e 2 em
arquivos que já reprovam por outro motivo — ou seja, a promoção a bloqueante derrubaria 3 arquivos
hoje verdes naquele repositório.

**5 avisos abertos à mão (analise):**

| # | Arquivo:linha | Veredito | Por quê |
|---|---|---|---|
| 1 | `apps/web/src/ui/EditorView.tsx:194` (`dangerouslySetInnerHTML={{ __html: html }}`) | **FALSO POSITIVO** | `html = renderizarCorpo(bloco.texto, …)`; `renderizarCorpo` (`apps/web/src/lib/pagina.ts:99`) escapa tudo com `escapar()` e documenta "dispensa sanitizador externo". O sanitizador existe, mas o nome não contém `escape`/`sanitize` e está em outra função (limite §6) |
| 2 | `apps/web/src/ui/PaginaView.tsx:125` (idem) | **FALSO POSITIVO** | mesmo `renderizarCorpo` |
| 3 | `scripts/sei/_sei-functions-pro.js:2444` (`txt.innerHTML = html`) | **FALSO POSITIVO** | `decodeHtml`: `txt = document.createElement("textarea")` — decodifica entidades num `<textarea>`, conteúdo RCDATA, não vira elemento. Idioma inerte conhecido |
| 4 | `tools/sei-simulator/frontend/src/components/DocumentViewer.tsx:58` (`__html: content`) | **VERDADEIRO POSITIVO** | `content` é prop crua (HTML de documento SEI) direto no sink, sem sanitizador; o autor já reconheceu o risco com `biome-ignore lint/security/noDangerouslySetInnerHtml: SEI documents are HTML`. Exploração depende da procedência de `content` — é exatamente o que um aviso deve apontar |
| 5 | `frontend/test-results/assets/index-B6hUSIXg.js:17` | **FALSO POSITIVO** | bundle minificado do Vue (runtime: `Mv.innerHTML=Jy(…)` com a política `trustedTypes`), artefato de `test-results/`, não código do projeto |

Fora do corpus do gate (workspace touring, 3 arquivos, não julgados como achado):
`crates/touring-web/dist/touring-web-d3e1b058fa9ce534.js:847` é cola gerada pelo wasm-bindgen
(`__wbg_set_innerHTML_…(arg0, arg1, arg2)`; sem procedência — a procedência real vive no Rust),
e dois `3d-force-graph.min.js` de `vendor/` (minificados). Nenhum chega ao gate (`dist/` e
`*.min.*` ficam de fora).

**Precisão medida hoje: 1 verdadeiro positivo em 5 (20%).** É o número que justifica o braço
nascer como aviso: bloquear com essa precisão congelaria edição em código correto. As classes
de falso positivo achadas são específicas e consertáveis: (a) sanitizador local de nome
diferente ou em outra função (2 de 5); (b) o idioma `<textarea>` inerte (1 de 5); (c) bundle de
terceiros fora de `dist/` e sem `.min.` no nome (1 de 5).

### Critério de promoção (decisão de Gabriel; nada disso é automático)

Escada em três degraus, cada um com **atestado explícito** (`judge_attest.py --attest --why`,
como a promoção do `BestPracticesGate`):

1. **Aviso** (hoje) — evidência `advisory (NOT scored…)`, fora do score.
2. **Pontua como WARN** — o achado entra em `vuln_matches` com severidade ≤ 4,9
   (`OwaspScan::tier_of` → `P1`: score ≥ 0,5, a dimensão avisa e não reprova). Só quando **todas**:
   - **C1 precisão**: ≥ 30 avisos rotulados à mão, de ≥ 3 repositórios distintos, com a cota de
     falsos positivos **≤ 0,20 no limite superior de Wilson-95** (o mesmo critério do gate de
     prosa: n ≥ 30, Wilson-95 ≤ 0,20). Hoje: n = 5, 4 FP — reprova.
   - **C2 classes fechadas, cada uma com teste**: o `<textarea>` inerte; sanitizador local
     declarável (lista em `.touring/touring.toml` ou resumo "devolve escapado" da função
     chamada, intra-arquivo); bundles gerados fora de `dist/` (ex.: `test-results/assets/`);
     opt-out nomeado na linha (`// nosec`, `// nosemgrep`, `eslint-disable … react/no-danger`,
     `biome-ignore … noDangerouslySetInnerHtml` com justificativa — o `DocumentViewer.tsx` já tem
     um), no padrão do `python_sinks` (`# noqa: S102`).
   - **C3 estabilidade**: duas rodadas de censo separadas por ≥ 7 dias, com a mesma ordem de
     grandeza de avisos por 1000 arquivos JS/TS, zero erro/timeout do braço e a latência por
     arquivo da F2.1 sem piora > 10% (a medir; não foi medida nesta onda).
3. **Bloqueia (P0)** — severidade ≥ 5,1. Só quando, com o degrau 2 ativo por ≥ 14 dias, o censo
   seguinte der FP ≤ 0,10 (Wilson-95) e **nenhum** arquivo hoje verde de repositório de Gabriel
   passar a reprovar sem que ele tenha escolhido corrigir o sink. Antes do degrau 3, o `touring
   scan` e o evidence do gate precisam mostrar o aviso (§9).

**Reversão**: remover a chamada `super::dom_sinks::detect(source, lang)` de
`SecurityAnalyzer::analyze` (`security.rs`, 1 linha) devolve o comportamento anterior byte a
byte; `advisories` fica vazio.

## 9. Pendências conhecidas (não bloqueiam esta entrega)

- **`touring scan`** (`crates/touring-server/src/cli/scan.rs:88`) lê `OwaspScan` e não
  imprime `advisories`; o crate é de outro dono nesta onda. Follow-up: mostrar os avisos ali.
- **Sobreposição com a F2.2**: `dangerouslySetInnerHTML` e `document.write(` aparecem nas
  duas (F2.2 = o sink existe; F2.1 = dado não confiável chega nele). Não há dupla contagem
  porque F2.1 não pontua o aviso.
- **Opt-out nomeado** na linha (equivalente ao `# noqa: S102` / `# nosec` do `python_sinks`):
  `// nosec`, `// nosemgrep`, `eslint-disable… react/no-danger`. Só vale implementar junto
  com a promoção — um aviso não bloqueia, então ninguém precisa silenciá-lo ainda.
