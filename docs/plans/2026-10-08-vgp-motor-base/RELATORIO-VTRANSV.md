# RELATÓRIO VTRANSV — frente C: `cdn` / `cdn_pin` e `classe_css` (camada 1 do motor VGP)

> Estado da entrega: **hipótese até o juiz da coordenadora no held-out selado** (brief
> `briefs/2026-10-10-fanout-adaptadores.md`). Nada aqui foi medido no held-out nem em `oss/`.
> Evidências duráveis (logs, JSONs do juiz, mutantes, controles, scripts, `SHA256SUMS.txt`):
> `~/.claude/touring/vgp/binarios/de359e81/evidencias/` (binário final, com a regra R2) e
> `~/.claude/touring/vgp/binarios/49830a73/evidencias/` (antes da R2; guarda a 1ª medição de ruído).

## 1. Identidade

| Campo | Valor |
|---|---|
| Agente | `vtransv` (subagente da frente C do fan-out dos adaptadores da camada 1) |
| Worktree | `/home/gabrielgadea/projects/touring-wt-vtransv` |
| Branch | `feat/2026-10-10-vgp-vtransv` (VL0 aplicada; HEAD `88c7cc8b`; **sem commit meu**) |
| Alvo | `CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/wt-vtransv` |
| Contrato | `briefs/2026-10-10-fanout-adaptadores.md` + `VL0-multilinguagem-spec.md` §2–§5 + as duas decisões e a emenda de aceite da coordenadora (trait, sha2/base64, controle negativo) |
| Arquivos meus | `crates/touring-code/src/vgp/lang/{cdn.rs, classe_css.rs}` e os submódulos `lang/cdn/{cache,documento,hash,resolvedor}.rs`, `lang/cdn/resolvedor/tests.rs`, `lang/cdn/tests.rs`, `lang/classe_css/{folhas,usos,projeto,tests}.rs` |
| Fora dos meus arquivos (autorizado) | emenda do núcleo (§3.1), `crates/touring-code/Cargo.toml` (+2 deps), `Cargo.lock` (+2 linhas, consequência), `crates/touring-server/src/cli/vgp.rs` (cópia byte a byte da VL0) |
| Binário do juiz (final, com a R2) | `~/.claude/touring/vgp/binarios/de359e81/touring` (sha256 `de359e81bed4e3b5a0540588390c480db2cf4068fbb9edc926473310016b34a9`, `touring.sha256` ao lado). Antes da R2: `49830a73` (superado) |

## 2. O que foi feito, com evidência

**`cdn` / `cdn_pin`** (`lang/cdn.rs` + `lang/cdn/`), camada 1 pura (nenhuma rede, nenhum processo):

- Lê URLs de CDN (jsDelivr npm/gh, unpkg, cdnjs; com ou sem esquema, escopo `@x/y`, sem versão) em `src`/`href`
  de qualquer tag, `<style>` (`@import`, `url()`), `<script type=module>`/inline (`import`, `export … from`,
  `import()`, `x.src = …`), `importmap` e `ld+json`; comentário e texto corrido não contam. `.js/.ts/.tsx` pelo
  parse de `lang::parse`.
- Decide só pelo **cache local** (`cdn/cache.rs`): 404 confirmado → achado `cdn` (`Alvo::Externo`, deny pela
  política §5); 404 mais velho que 24 h **não nega** (pede reconfirmação ao resolvedor); `http_erro`/`indeterminado`
  → `nao_medido` (um timeout nunca é 404); URL fora do cache → `nao_medido` cujo motivo NOMEIA o resolvedor
  (`vgp::lang::cdn::resolvedor::preencher`) e conta as URLs.
- `cdn_pin`: versão flutuante (faixa, `latest`, sem versão, 2 partes, branch do `/gh/`) → aviso com a versão exata
  que o cache resolveu; `integrity` que não bate com o digest do cache (algoritmo mais forte, base64/base64url,
  padding) → achado; `integrity` sem digest no cache → `nao_medido`.
- Resolvedor com rede, **separado** da camada 1: `cdn/resolvedor.rs` (`resolver_url`, `preencher`, trait
  `Transporte`, `HttpPlano` sobre `std::net` só para `http://`, prazo curto, limite de corpo 8 MiB). Regras: 2xx →
  `existe` + digests sha256/384/512; 404 → `404` + causa (versão × arquivo, pela listagem do jsDelivr); 5xx/403 →
  `http_erro`; timeout/recusa/API ilegível → `indeterminado`; página HTML no lugar do arquivo (portal cativo) →
  `indeterminado`, sem digest; `.css` baixado grava `classes`/`padroes`/`imports` no registro (usado pelo
  `classe_css`). Exercido por servidor TCP falso local nos testes (`Falso`), sem tocar a internet.
- **Digest por `sha2` + `base64` do workspace** (decisão da coordenadora): `cdn/hash.rs` virou 20 linhas de
  código (`sri(bytes)`); a implementação própria de SHA-2/base64 saiu. Os vetores `hashlib` ficaram como teste do
  encaixe (algoritmo, ordem, padding).

**`classe_css`** (`lang/classe_css.rs` + `lang/classe_css/`): classe usada em `class=` (html) / `className=` e
`class=` (JSX) que nenhuma folha CARREGADA define, com «você quis dizer» (Levenshtein por limite proporcional).
Severidade sempre aviso pela política (§5.2). Fica em silêncio com o motivo (`nao_medido`) quando: folha ausente,
pré-processador (`.scss`…), `@import` ilegível, folha remota fora do cache de CDN; Tailwind/UnoCSS/Windi/Twind/
Panda/styled-components/emotion/… em `package.json` do pacote do arquivo, config ou `<script src>`; template
(`{% … %}`, `<?php`, `<% %>`), fragmento sem `<html>`/`<head>`, script que cria folhas em tempo de execução; nenhuma
folha; **saída de editor rico** (qualquer elemento com `contenteditable`, em qualquer caixa e sem valor): as classes que
avisariam não são avaliadas e o motivo diz «classes de saída de editor rico (contenteditable): a folha do editor não
está no arquivo (N classe(s) sem definição não avaliada(s): …)» — só onde haveria aviso (regra R2, §3.3). Valores
dinâmicos (`className={x}`, `class="a {{b}}"`) são pulados, `js-*` e classes que o próprio script
do documento usa por nome são ganchos, `[class*="x"]` conta como definição, escapes CSS (`sm\:flex`) são
decodificados, módulos `.module.css` só provam existência. Em JSX o universo são as folhas do projeto (varredura
limitada a 20 000 entradas / 200 folhas; estouro → `nao_medido`) mais as importadas pelo arquivo. Folha de CDN é
lida pelo cache (`classes` gravadas pelo resolvedor), nunca pela rede.

**Testes (TDD).** RED no stub: cdn `red_cdn_stub.log` = 27 ok / 36 falhos; classe_css `red_css_esqueleto.log` =
8 ok / 38 falhos; R2 `red_r2.log` = 60 ok / **3 falhos** (os 3 testes de abstenção; os 2 controles já passavam).
GREEN final (`z5_test.log`, `cargo test -p touring-code --no-fail-fast`): **25 suítes, 1307 passaram, 0 falharam,
0 warnings**; 160 testes são meus (cdn 97: `tests` 46, `resolvedor` 32, `cache` 11, `documento` 6, `hash` 2;
classe_css 63: `tests` 59, `projeto` 4) mais 3 testes novos do núcleo (§3.1). Controles que passam limpos: URL
existente no cache, versão exata, classe definida por `[class*=]`, JS hook, `@import` seguido com ciclo protegido,
atributo dinâmico → sem achado; documento sem `contenteditable` com typo segue avisando; documento com
`contenteditable` mas sem classe indefinida não se abstém.

**Corrigido durante o GREEN/mutantes** (para o próximo): (a) `documento.rs` gerava bloco para `<script src>` vazio
(`raw_text` de largura zero); (b) o campo morto `Elemento.linha` saiu (REGRA #0); (c) **`javascript`: a gramática
0.25 reserva `class`**, então `<i class="x"/>` (Preact/Solid) vira ERRO no parse JS (`className` não) —
`usos.rs::arvore_jsx` cai para a gramática TSX quando o parse JS tem erro e o TSX é limpo; (d) 9 lints do clippy 1.98
(`chunks_exact` → `as_chunks`, `needless_collect`, `unnecessary_sort_by`); (e) a proteção contra `${}` em JSX estava
duplicada (`valor_estatico` e `tem_placeholder`), o que fazia o mutante «tira uma das duas» sobreviver sem ser lacuna
de teste — ficou só `tem_placeholder`, e o mutante que sobra mira essa única proteção.

## 3. Estado por item do aceite

| # | Item | Estado | Evidência |
|---|---|---|---|
| 1 | TDD por classe: RED no stub → GREEN + controles | **OK** | `red_cdn_stub.log`, `red_css_esqueleto.log`, `red_r2.log`, `z5_test.log` |
| 2 | Mutantes mortos e listados | **OK, 60/60** | tabela abaixo (`mutantes_out_r2.json`, rodada única dos 60, linha de base verde antes) |
| 3 | Juiz no corpus semeado (bf643ba6da16bee2): recall ≥ 0,70 onde n ≥ 20; falso deny 0; p95 ≤ 150 ms | **OK** | binário `de359e81`: `cdn` **20/20**, `cdn_pin` **20/20**, `classe_css` **20/20**; falso deny **0/200** (CP95 unilateral **1,49 %**: n=200 não prova o teto de 1 %, exige n ≥ 299 — esse teto é do held-out); p95 **50,9 ms** (`j4_html.json`; o juiz rodou junto com o build/mutantes, a rodada sem concorrência deu 29,7 ms com o `49830a73`); **avisos em negativos 49 → 2** (§3.3). `tsx`: não há `classe_css` no corpus (n=0); falso deny 0/200, 0 avisos, p95 4,1 ms (`j4_tsx.json`) |
| 4 | Gates | **OK** | `cargo fmt --all -- --check` EXIT 0 · `clippy -p touring-code --all-targets -- -D warnings` EXIT 0 · `cargo test -p touring-code --no-fail-fast` EXIT 0 (1307 ok / 0 falhas) · `RUSTDOCFLAGS="-D warnings" cargo doc -p touring-code --no-deps` EXIT 0 — `z5_*.log`, numa só aquisição do `hw-slot`, ANTES dos mutantes |
| 5 | Teste do despacho do núcleo verde | **OK** | os 16 de `vgp::lang::tests` (13 anteriores + 3 novos), inclusive `transversal_roda_depois_do_adaptador_e_passa_pela_mesma_politica` |
| E | **Emenda: controle negativo em código limpo** | **OK: 0 achados em A, B, C** (idêntico antes e depois da R2) | §3.2 |
| R | **Regra R2 (ruído do `classe_css`)**, decisão da coordenadora | **OK** | §3.3 |

O veredito do juiz sai «NÃO PRONTO» por pendências que **não são minhas**: o recall agregado (60/100) conta
`ancora`/`recurso` (40 positivos da frente `vhtml`), e o teto de falso deny por n=200. Os 60 positivos meus foram
todos detectados. **O corpus semeado mudou entre as duas primeiras medições**: a rodada inicial (binário `68f8aa70`,
`entradas.jsonl` `5c068940312620b2`) tinha `cdn_pin` n=16 e deu 16/16; o corpus atual (`bf643ba6da16bee2`, alterado
em 10/10 10:09 BRT) tem `cdn_pin` n=20. `68f8aa70` e `49830a73` estão superados; o oficial é o **`de359e81`**.

**Avisos em negativos do juiz** (a pedido): ANTES da R2, html **49 de 200** negativos receberam aviso (128 achados,
todos `classe_css`, todos `warn`; `ancoraSei`, `dark-mode-color-black`…). DEPOIS: **2 de 200** (6 achados, ver §3.3);
tsx **0**. Nenhum é deny.

### 3.1 Emenda do núcleo (`Transversal::verificar -> Verificacao`)

Aprovada pela coordenadora, implementada no meu worktree. Diffs completos contra a VL0 (`c95d707b2ffd` /
`83d9c5071c8b`) em `evidencias/emenda_lang_mod.diff` e `evidencias/emenda_mod.diff`.

| Arquivo | Linhas (pós-emenda) | Mudança |
|---|---|---|
| `crates/touring-code/src/vgp/lang/mod.rs` | 83–98 | doc do trait `Transversal` e assinatura: `fn verificar(&self, input) -> Verificacao` (antes `Vec<Finding>`) |
| `crates/touring-code/src/vgp/lang/mod.rs` | 503–517 | teste `Marca` devolve `Verificacao::medido(vec![..])` |
| `crates/touring-code/src/vgp/lang/mod.rs` | 524–586 | +3 testes e o transversal de teste `SoAbstem` (`transversal_que_nao_mediu_faz_o_veredito_sair_unknown_com_o_motivo_dele`: adaptador limpo + transversal que se abstém → `unknown` com o motivo dele, nunca `allow`, e a abstenção convive com um achado de outro transversal; `motivos_do_adaptador_e_dos_transversais_se_juntam_na_ordem_sem_perder_nenhum` → `"parse falhou; um; dois"`; `transversal_que_mediu_tudo_nao_acrescenta_motivo`) |
| `crates/touring-code/src/vgp/mod.rs` | 205–220 (`medir`) | junta o `nao_medido` do adaptador e os dos transversais, nessa ordem (ordem do registro), por `"; "`, sem perder nenhum |
| `crates/touring-code/Cargo.toml` | +4 linhas | `sha2 = { workspace = true }`, `base64 = { workspace = true }` |
| `Cargo.lock` | +2 linhas | arestas `touring-code → sha2/base64` (consequência) |
| `crates/touring-server/src/cli/vgp.rs` | arquivo | cópia byte a byte de `touring-wt-vl0` (sha `b1e5878cbb08`, conferido) |

`cdn.rs` e `classe_css.rs` implementam `verificar` devolvendo `self.medir(input)` (achados **e** motivo). Mutantes do
núcleo: `medir` ignora o `nao_medido` do transversal · `medir` perde o do adaptador · separador errado — os três mortos.

### 3.2 Emenda de aceite: controle negativo em código limpo

Fontes: `~/projects/analise` e `~/projects/touring` (nunca `oss/`, held-out, nem `.vgp-sombra*`); 5 904 arquivos HTML/
TSX/JSX candidatos (≤ 600 KB). Selecionador **independente do motor** (`oraculo.py`: `html.parser` + regex; não usa
o motor) escolhe os arquivos cujas classes estáticas estão todas definidas pelas folhas que o próprio arquivo carrega
(HTML) ou pelos `.css` do pacote (TSX); depois o binário roda sobre a seleção (medido com o `49830a73` e REPETIDO com o
`de359e81` da R2: **resultado idêntico**, `resultado_controle_r2.json` tem o mesmo sha `474e4f8994320bd9`). Manifesto
com sha de cada arquivo: `manifesto_controle*.json`; resultado: `resultado_controle*.json`.

| Conjunto | Arquivos | Achados | Observação |
|---|---|---|---|
| A — HTML real, classes todas definidas (oráculo) | 491 | **0** | 485 só com o motivo do adaptador `html` ainda não implementado; 6 com `classe_css: projeto com …` (abstenção) |
| B — TSX/JSX real, pacote sem gerador, classes todas nos `.css` do pacote | 15 | **0** | n pequeno: o oráculo exige pacote sem `@import`/`[class`/Tailwind |
| C — ABSTENÇÃO: HTML/TSX de pacotes com Tailwind (`tailwindcss`: 6 pacotes) | 415 | **0** | 350 com `nao_medido` que nomeia o gerador; os 65 restantes não têm classe estática a julgar. **Não achei pacote com CSS-in-JS nas árvores permitidas** — só Tailwind |
| D — `cdn`: arquivos com URL de CDN real; cache de controle (resolvedor EXTERNO com urllib, 53/53 URLs `existe`, digests SRI) | 92 | 3 `cdn` + 52 `cdn_pin` + 1 `classe_css` | ver abaixo |

D, todos os restos com causa: **3 `cdn` (deny)** = `gsap@3.13.0` em `pei/out/{pei-skills-e2e,v4_2_e2e,v4_3_e2e}/
gsap-injection.html`, cujo `integrity` (`sha384-Pd96BMV8…`) não é o do arquivo servido (`sha384-HOvlOYPIs…`,
calculado por `hashlib` fora do motor): **verdadeiros positivos** (o navegador recusaria o script). **52 `cdn_pin`**
(46 unpkg, 6 jsDelivr): conferidos um a um com regex de semver independente — **0 em URL de versão exata**; todos em
`@18`, `@latest`, sem versão (`unpkg.com/piii`) ou `/gh/…` sem tag: o aviso é o esperado. **1 `classe_css`**:
`accent-2` em `Apresentação Bioceânica.html:2163`, o arquivo não está no conjunto A (oráculo: folha não local) e o
CSS só tem `--accent-2` (variável), nunca `.accent-2`: verdadeiro positivo. Alvo da emenda (0 em código limpo): **atingido
em A, B e C; em D só restam verdadeiros positivos e avisos esperados**.

### 3.3 Ruído do `classe_css` e a regra R2 (saída de editor rico)

**Causas dos 128 avisos nos 49 negativos** (3 documentos abertos à mão por categoria): classe injetada pelo editor
SEI cuja folha mora no sistema (`ancoraSei` 42, `dark-mode-color-black` 24, `dark-mode-color-white` 6, `ancora_sei` 6
= 78, 47 docs) · marcadores estruturais do módulo Legis (`legis auto cap sec art par inc alin sub ignoretip anexo` =
42, 4 docs, semântica sem estilo) · relatório/slide gerado onde falta CSS ou a classe é gancho de SVG (8, 2 docs: único
resto legítimo) · gancho de JS inline 0 · folha remota fora do cache 0 (já é `nao_medido`) · erro de digitação real 0
(nenhum dos 128 tem «você quis dizer»). Dominante: documento exportado de editor rico, **118 de 128 (92 %)**, 47 de 49
docs, todos com `contenteditable`.

**Regras medidas** (sobre a saída real do binário; recall = positivos `classe_css` detectados, controle A = 0):

| Regra | Avisos neg. antes → depois | Docs neg. silenciados | Recall | Controle A |
|---|---|---|---|---|
| R1: cobertura < 70 % | 128 → 72 | 6/49 | 16/20 | 0 |
| R1: cobertura < 90 % | 128 → 6 | 47/49 | 13/20 | 0 |
| R5: só avisa com «você quis dizer» | 128 → 0 | 49/49 | 20/20 (**viés**: positivos são quase-acertos por construção) | 0 |
| **R2s: `contenteditable` por SUBÁRVORE** (só classes dentro do elemento editável e descendentes) | **128 → 36** (30 docs) | 19/49 | 20/20 | 0 |
| **R2: `contenteditable` por DOCUMENTO** | **128 → 6** (2 docs) | 47/49 | **20/20** | 0 |

**R2s não deu o mesmo, por isso implementei a R2.** A variante por subárvore deixa 36 avisos porque
`dark-mode-color-black` (24) e `dark-mode-color-white` (6) são `<span>` que o editor emite FORA de qualquer elemento
`contenteditable` (só os marcadores de âncora/Legis levam o atributo), e 6 são os restos legítimos. O custo da R2 por
documento é medido e declarado: uma classe com typo fora da ilha editável também se abstém
(`a_abstencao_e_por_documento_um_typo_fora_da_subarvore_editavel_tambem_se_cala`).

**Implementação** (`classe_css.rs`, `medir_html`): depois de julgar, se há achados E algum elemento tem `contenteditable`
(qualquer caixa, com ou sem valor), devolve `nao_medido` sem os achados; sem achados, não se abstém (teste
`documento_de_editor_sem_classe_indefinida_nao_se_abstem`). Exemplo real (`c763f2…[129]…Resolucao.html`): `unknown`,
sem achados, motivo «classe_css: classes de saída de editor rico (contenteditable): a folha do editor não está no arquivo
(14 classe(s) sem definição não avaliada(s): ancoraSei, legis, auto, …)». Mutantes mortos: predicado sempre falso (3
testes) · sempre verdadeiro (21) · abstém mesmo sem achado (1) · mantém os achados ao abster (3) · contagem errada (1) ·
sem nomes no motivo (1).

**Medida final, binário `de359e81`:**
- Semeado html (bf643ba6): recall `cdn` 20/20, `cdn_pin` 20/20, `classe_css` 20/20; falso deny 0/200; **avisos em
  negativos 49 → 2** (128 → 6 achados: `cover-agency`, `cover-proc`, `cmeta-cell` em `memorial_e2e.html` e
  `subheadline`, `cities`, `countries` em `01-slides-panorama-brasil.html`); controles A, B, C **0** e D **inalterado**
  (3 `cdn`, 52 `cdn_pin`, 1 `classe_css`; a regra só muda onde há `contenteditable`).
- **População de 5 165 HTML com `class`** (`analise` + `touring`, antes e depois NO MESMO INSTANTE,
  `populacao_r2.log`): `49830a73` 4 186 docs com aviso (81,0 %), 48 191 avisos → `de359e81` 2 855 docs (55,3 %), 46 531
  avisos. A regra silenciou **1 331 documentos e 1 660 avisos, e todos os 1 331 viraram `nao_medido` com o motivo de editor
  rico** (a abstenção aparece como número); nenhum aviso novo apareceu (asserção no script). O resto (2 855 docs) é a
  família `htmlcov` do coverage.py (`pc_cov`, `pln`, `nam`, `op`, `num`, `button_*`), que fica ABERTA (§Limites).

### Limites

1. O recall de `classe_css` foi medido **só em quase-acertos sintéticos** (classe existente + sufixo `q`) e **não prova
   digitação real**.
2. **R3** (gancho em `<script src>` local) e **R5** (só avisar com «você quis dizer») e os relatórios gerados (`htmlcov`,
   46 514 avisos fora da amostra) ficam **ABERTOS, não decididos**.
3. O ruído do `htmlcov` só pesa em `vgp check` de diretório, porque o hook só vê HTML que um agente escreve.

Além disso, a R2 é por documento (limite medido acima) e o JSX (`contentEditable`) não foi tocado.

### Tabela de mutantes (60; cópia + restauração por conteúdo; linha de base verde antes de cada lote)

Todos MORTOS. Cada linha = «regra mutada → nº de testes que falham». Por regra de resolução e «abstenção vira allow»
(marcadas A):

- cdn (`cdn.rs`): 404 confirmado não nega (14) · 404 vencido nega (1) · **URL fora do cache vira allow (5) A** ·
  **`indeterminado`/`http_erro` calado (1) A** · 404 não domina o pin (15) · pin invertido (18) · semver aceita 2
  partes (8) · `/gh/` branch é exata (3) · cdnjs não é exata (2) · sem versão é exata (4) · integrity pelo algoritmo
  mais fraco (2) · base64url sem normalizar (1) · padding conta (1) · integrity diferente não acha (3) ·
  **integrity sem digest no cache calado (1) A** · sem dedupe por nome (1) · host estranho vira URL de CDN (4)
- cache: host sem minúscula (2) · 404 só vence se novo (4)
- resolvedor: timeout vira 404 (4) · 5xx vira 404 (2) · portal cativo é o arquivo (1) · URL flutuante não resolve (10) ·
  API 404 vira erro (1) · causa trocada (2) · digest de corpo truncado (2) · classes de qualquer arquivo (1) · 404
  vencido pula a rede (1)
- hash: sha384 com o algoritmo errado (6)
- classe_css (`classe_css.rs`): **folha não resolvida vira allow (9) A** · gerador de utilitários ignorado (1) ·
  **sem folha nenhuma vira allow (2) A** · **fragmento vira allow (1) A** · **template vira allow (1) A** · **script que
  cria folha ignorado (1) A** · `js-` não pula (2) · gancho de JS não pula (1) · sem dedupe de classe (1) · sem «você
  quis dizer» (3) · alvo externo no achado (2) · `.module.css` absorvido (1)
- classe_css (`folhas.rs`): **folha remota ignorada no href (7) A** · **ignorada no @import (2) A** · padrão de
  atributo ignorado (2) · @import não seguido (2) · sem rede léxica sob erro de gramática (1) · escape não
  decodificado (1)
- classe_css (`usos.rs`/`projeto.rs`): placeholder de template não pula (1) · idem em JSX (1) · `node_modules` varrido (2) ·
  `package.json` não para a subida (1)
- núcleo (`../mod.rs`): `medir` ignora o `nao_medido` do transversal (2) · perde o do adaptador (2) · separador errado (1)
- R2 (`classe_css.rs`): predicado `contenteditable` sempre falso (3) · sempre verdadeiro (21) · **abstém mesmo sem
  achado (1) A** · mantém os achados ao abster (3) · contagem do motivo errada (1) · motivo sem os nomes (1)

Histórico honesto: a primeira rodada do lote CSS deixou **3 sobreviventes** (rede léxica sem teste que quebrasse a
gramática, dupla proteção de `${}` em JSX, e um mutante que nem compilava); virou teste novo
(`classe_so_lexica_conta_quando_a_gramatica_de_css_engasga`, 12 folhas quebradas), remoção da duplicação e mutante
redefinido. Os logs de cada rodada estão em `evidencias/mutantes_out_{a,b,c,c2,final,extra}.json` (49830a73) e
`mutantes_out_r2.json` (de359e81: os 60 numa rodada só). O `mutantes.py` agora restaura o conteúdo e põe o mtime em
«agora» (§4).

## 4. Processos

Todo cargo e todo juiz/mutante por `hw-slot` (`--class rust` para cargo/mutantes; `heavy` para juiz e controle).
Calor de 91–96 °C e outras frentes na mesma máquina: cada aquisição do slot custou de minutos a mais de uma hora de fila. Rede caiu
às 08:09Z e voltou ~12:40Z; retomei do disco, sem perda (o log `f1_*` já estava completo).
Execuções: `cargo test --lib` (RED cdn, RED classe_css, GREEN), `gates.sh` (fmt+clippy+test+doc numa aquisição),
`cargo build --release -p touring-server --bin touring`, `juiz_motor_vgp.py` (html e tsx, sha `6c8a31b02075da1e`,
`--exigir-limite-superior`, `TOURING_VGP_CDN_CACHE=…/corpus_semeado/cdn_cache.json`), `controle.py`, lotes de
mutantes (`mutantes.py`). O resolvedor EXTERNO de controle (`cache_real.py`) rodou no sandbox do Touring com
`--allow-net-port 443/80` (a rede nunca esteve na camada 1). Nenhum `kill`/`pkill`. Resta uma entrada de fila
duplicada minha (`./t.sh red1.log vgp::lang::cdn`, de um caminho de log relativo no 1º `t.sh`), inofensiva; o
`red1.log` ficou na raiz do worktree, não rastreado.

**Armadilha achada no protocolo de mutantes** (vale para as outras frentes): restaurar o arquivo com `os.utime`
para o mtime ORIGINAL faz o `cargo` julgar o artefato do último mutante «fresco»; o `cargo test` seguinte, sem tocar
nada, compila o código MUTADO. Aconteceu comigo (2 falhas fantasmas em z2/z3, exatamente as do último mutante,
`DIRETORIOS_IGNORADOS never used`). Remédio: `touch` nos arquivos mutados antes de qualquer gate de evidência
(z4 foi depois do `touch`). Os resultados dos mutantes em si não são afetados (cada mutante escreve com mtime novo).
Na rodada da R2 (z5) os gates e o release rodaram ANTES dos mutantes e o `mutantes.py` passou a restaurar o conteúdo e
pôr o mtime em «agora», o que elimina a armadilha. Rodadas da R2: RED (`red_r2.log`), `final.sh` (gates + release,
depois 60 mutantes, cada fase numa aquisição), juiz html/tsx, `controle.py`, `populacao_r2.py`.

## 5. Dados com sha

| Item | sha256 (16) |
|---|---|
| Binário final `de359e81` (com a R2) | `de359e81bed4e3b5` |
| Binário `49830a73` (antes da R2, superado) | `49830a73cf1e3b2c` |
| `j4_html.json` / `j4_tsx.json` (binário `de359e81`) | `e6c2aab61f0a31f6` / `7b542a89f063fec0` |
| `resultado_controle_r2.json` / `manifesto_controle_r2.json` / `populacao_r2.log` | `474e4f8994320bd9` / `e798aac35eb13c8f` / `59dca3eac990b45f` |
| `coleta.json` (ruído, 220 docs do semeado) / `mutantes.json` (60) / `mutantes_out_r2.json` | `2f967f241ea381b0` / `fb93bbe2f7d0ba21` / `487dee0e3e73adb3` |
| Juiz `juiz_motor_vgp.py` | `6c8a31b02075da1e` (conferido antes de rodar) |
| `corpus_semeado/html/entradas.jsonl` (medição final) | `bf643ba6da16bee2` |
| `corpus_semeado/tsx/entradas.jsonl` | `a5e6bd6166509e8e` |
| `corpus_semeado/cdn_cache.json` | `df9fa70a424fc154` |
| `j3_html.json` / `j2_tsx.json` | `4e3a23c80e23cc72` / `d3bd744d05bf98c8` |
| `resultado_controle.json` / `manifesto_controle.json` | `474e4f8994320bd9` / `2aaa70a68d27b674` |
| `candidatos.json` / `cdn_cache_real.json` / `oraculo.py` | `a98a31ee91cd32f9` / `04e1b5b3ea34d750` / `55ac869511b6fe70` |
| `mutantes.json` (54) / `mutantes_out_final.json` | `89a1d61ceab741ae` / `75d43a281894411f` |

Fontes (sha256 12 · linhas): `cdn.rs` `67dd3854d21a` 835 · `cdn/cache.rs` `e25560f79d17` 583 · `cdn/documento.rs`
`73711df9edc5` 249 · `cdn/hash.rs` `2b2e10ee8579` 77 · `cdn/resolvedor.rs` `9c6dd800ffc9` 469 ·
`cdn/resolvedor/tests.rs` `6bb75e83ca26` 838 · `cdn/tests.rs` `90baa465f933` 849 · `classe_css.rs` `22b115e50f4c`
436 · `classe_css/folhas.rs` `fc3c217c7251` 619 · `classe_css/usos.rs` `975180f1c00b` 273 · `classe_css/projeto.rs`
`fa975078b715` 329 · `classe_css/tests.rs` `bd8dd7d78eba` 993 · `lang/mod.rs` `7456ff15bc07` 592 · `vgp/mod.rs`
`7327ddfff38a` 224 · `touring-code/Cargo.toml` `32a558aee449` · `cli/vgp.rs` `b1e5878cbb08`.
**Após a R2 mudaram só dois** (os shas acima destes dois ficam superados): `classe_css.rs` `6a37adbc15bd` 468 linhas
(+`e_saida_de_editor`, `motivo_de_editor`, a ramificação em `medir_html`) e `classe_css/tests.rs` `fd6403c36a46` 1 087
linhas (+5 testes).

**Formato do cache de CDN** (proposta, implementada): `$TOURING_VGP_CDN_CACHE` ou
`~/.claude/touring/vgp/cdn_cache.json`; `{"formato":1,"urls":{<url normalizada>: Registro}}`; lê também o mapa plano
da touring-4f. `Registro`: `verdict` (`existe`|`404`|`http_erro`|`indeterminado`), `status`, `resolvida`, `causa`
(`versao_inexistente`|`arquivo_inexistente`|`outra`), `integrity[]`, `verificado_em` (unix s), `erro` e, para
`.css`, `classes`/`padroes`/`imports`. Chave `https://host-minúsculo/caminho`, sem query/fragmento.

## 6. Decisões e riscos

- **Trait**: virou `Transversal::verificar -> Verificacao` (decisão da coordenadora); o motivo de abstenção agora chega
  ao `vgp check` (`motivo` e veredito `unknown`). Efeito prático: um HTML com `classe_css` impossível de provar
  passa de `allow` para `unknown` (fail-open, nunca deny).
- **TTL do 404 = 24 h**; sem carimbo (cache plano) é confiado. Escolha minha: negar para sempre puniria uma versão
  publicada depois do lookup.
- **HTTPS fica para a integração**: `HttpPlano` só fala `http://`; o `Transporte` real (reqwest/rustls) e a ligação
  de `preencher` a comando/daemon são da integração, como a coordenadora decidiu.
- **`classe_css` é ruidoso em documentos reais**: dos 5 165 HTML com `class` das árvores amostradas, 81 % recebiam
  aviso (48 191 avisos). A R2 (§3.3) tirou 1 331 documentos e 1 660 avisos, todos transformados em `nao_medido`
  contabilizado; os 55,3 % restantes são sobretudo `htmlcov` e ficam abertos (§Limites). Efeito da R2 sobre o veredito:
  um HTML de editor rico com classes sem folha passa de `warn` a `unknown` (fail-open).
- **Rede léxica de CSS** (`.nome` sob erro de gramática) só entra quando a gramática reporta erro; tem teste e mutante.
- **Context7 indisponível**: gramática lida direto em `node-types.json`/`grammar.json` dos crates (`tree-sitter-html/
  css/javascript 0.25`, `tree-sitter-typescript 0.23.2`, `tree-sitter 0.26.9`); semântica de CDN nas docs oficiais.
- **Gates P0 de qualidade** (`touring-quality check --gate F2.1|F2.4|F4.3` nos 9 arquivos de produção): 0 bloqueios;
  F2.4 dá aviso 0,5 em `cdn.rs` e `classe_css/usos.rs` («palavra-chave de segredo, sem valor atribuído»: `integrity`/
  `token` de classe CSS) — falso positivo lexical, sem valor atribuído.
- **Binários `68f8aa70` e `49830a73`** ficaram em `~/.claude/touring/vgp/binarios/`; estão superados pelo `de359e81` e
  podem ser apagados pela coordenadora.

## 7. Git

Nenhum commit, push, stash, reset ou checkout. Não rastreados (meus): `lang/cdn.rs`, `lang/cdn/`, `lang/classe_css.rs`,
`lang/classe_css/`. Modificados fora deles, todos autorizados: `crates/touring-code/src/vgp/mod.rs` e `lang/mod.rs`
(emenda §3.1), `crates/touring-code/Cargo.toml`, `Cargo.lock`, `crates/touring-server/src/cli/vgp.rs` (cópia VL0).
O `lang/mod.rs` do snapshot estava desatualizado (E0061); foi substituído pelo canônico da VL0 antes da emenda e a
emenda é o único delta restante.

## 8. O que NÃO foi verificado

- Held-out selado (`corpus_holdout_*`) e `oss/`: **não lidos, não medidos**.
- Rede real na camada 1: inexistente por construção. O resolvedor nunca falou com jsDelivr/unpkg/cdnjs; só com
  servidor TCP falso. O resolvedor EXTERNO de controle (urllib) provou que as 53 URLs reais existem, mas não é o
  código do repositório.
- Ligação do resolvedor a comando/daemon/hook e a cadência de reconfirmação do 404 vencido (integração).
- `classe_css` em JSX contra projetos grandes: o oráculo só aprovou 15 TSX limpos (B); o custo em projeto de 20 000
  entradas não foi medido.
- Testes do `touring-server` (`disk_file_chega_ao_check_para_qualquer_lang`) e dos hooks: não rodei; só compilei o
  binário com o `cli/vgp.rs` da VL0.
- CSS-in-JS como controle de abstenção: nenhum pacote com styled-components/emotion nas árvores permitidas; só
  Tailwind foi exercitado (os detectores de `package.json` de CSS-in-JS têm teste unitário, não controle real).
- Windows/macOS; HTTPS; CSS exótico (`@scope`, `@layer` aninhado) além do testado.
- R2: o p95 de 50,9 ms do juiz html foi medido com build e mutantes rodando ao lado (29,7 ms sem concorrência com o
  binário anterior); não repeti a medição em máquina quieta. A população de 5 165 é amostra de duas árvores do próprio
  Gabriel, não de HTML da web; a R2 por subárvore (R2s) foi só simulada em Python (`r2s.py`), não implementada.
- O held-out selado não viu a R2: ela nasceu de ruído medido no corpus semeado e na população, o que pede o juiz final.
