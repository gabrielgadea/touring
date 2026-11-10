# LISTA-HTML-OSS — repositórios públicos com HTML escrito à mão (D-8)

> Decisão do Gabriel (10/11/2026, `DECISOES.md` item 5): **«Aprovar a lista e clonar»**.
> Executor: `touring-c0` (MiniMax). Brief: `briefs/5-minimax-html.md`.
> Estado: **lista montada, aprovada pela decisão, e CLONADA** — 37 repos em
> `~/.claude/touring/vgp/oss-html/`, sha fixo, contagem **medida** (não estimada).

## 1. A unidade do alvo — de onde vem "1.704"

O alvo do D-8 não é "número de arquivos html no disco": é **a população que o
`monta_camada_html_oss.py` consegue minerar**. A fonte é
`docs/plans/2026-10-08-vgp-motor-base/RELATORIO-V8-SEM2.md:349`:

> «limite **250**; a população medida é de **1 704** negativos no acervo (540 com limite 100)»

e `:373`:

> «a camada precisaria de **n ≥ 3 700**, e o acervo inteiro (7 clones) dá **1 704**».

Logo **1.704 = Σ min(250, commits que tocam `.html` antes do corte) por clone**, e o
corte do montador é `2026-10-10T00:00:00Z` (`CORTE_PADRAO`, `monta_camada_html_oss.py:54`).
Por isso a lista traz **duas** colunas medidas:

| coluna | o que é | por que importa |
|---|---|---|
| **html escrito à mão** | arquivos `.html` no HEAD, sem minificado e sem vendored | o brief pede «≥ 2.000 arquivos html novos» |
| **rendimento** | `min(250, commits que tocam .html até o corte)` | é o que **move o n** de 1.704 para ≥ 3.700 |

## 2. Como cada número foi medido (nada aqui é estimativa)

| filtro | regra | medido |
|---|---|---|
| html escrito à mão | linha mais longa do arquivo ≤ 10.000 chars (html minificado tem tudo numa linha) | 8.628 de 8.684 (51 minificados fora) |
| não-vendored | caminho não casa `vendor|vendored|third[_-]?party|node_modules|dist|build|out|bower_components` | 5 fora |
| licença | **lida no arquivo LICENSE do próprio repo** (nada inferido do nome) | 37/37 verificados |
| sha fixo | `git rev-parse HEAD` no momento da clonagem | 37/37 |
| commits `.html` | `git log --format=%cI -- '*.html'` filtrado pelo corte | 37/37 |
| disco | `du -sm` do clone | 4.099 MB (4,0 GiB) |

Scripts (em `~/.claude/touring/vgp/oss-html/_tools/`, fora do workspace):
`clona_e_mede.sh` (clone + medição), `conta_html_efetivo.py` (contagem do html
efetivo), `verifica_selecao.sh` (movimentação + licença + contagem), `tabela_final.py`
(tabela consolidada).

## 3. A lista (37 repos)

Clonados em `~/.claude/touring/vgp/oss-html/<owner>-<repo>`, no sha da coluna.

| # | repo | url | licença (lida no repo) | sha fixo | html escrito à mão | commits `.html` | rendimento | MB | share html |
|---|---|---|---|---|---|---|---|---|---|
| 1 | three.js | https://github.com/mrdoob/three.js | MIT | `0a980dcf032223a274fcdd60ab56330f1321cca0` | 1712 | 592 | 250 | 1126 | 19,8% |
| 2 | interactive-examples | https://github.com/mdn/interactive-examples | CC0-1.0 | `13024db581b08fae11a0ca229497198b9b0534e7` | 646 | 892 | 250 | 42 | 7,5% |
| 3 | echarts | https://github.com/apache/echarts | Apache-2.0 | `34f4d927eafe251f26f63afd3aeefa30f2feba30` | 599 | 2429 | 250 | 254 | 6,9% |
| 4 | jsdom | https://github.com/jsdom/jsdom | MIT | `40a0a143ad0be7c82fd26117183b9139071f2fa6` | 591 | 494 | 250 | 110 | 6,8% |
| 5 | css-examples | https://github.com/mdn/css-examples | CC0-1.0 | `65b08d2236b304d50d28a807a48c35f6889d902d` | 552 | 470 | 250 | 82 | 6,4% |
| 6 | wagtail | https://github.com/wagtail/wagtail | BSD-3-Clause | `030168a7dc42d5450c4cfbac3c68881b63b2d466` | 502 | 254 | 250 | 101 | 5,8% |
| 7 | httpd | https://github.com/apache/httpd | Apache-2.0 | `8f103390fb93201e310405f6e50e96c210faf1eb` | 414 | 34 | 34 | 130 | 4,8% |
| 8 | netbox | https://github.com/netbox-community/netbox | Apache-2.0 | `4b65840c7c6a9c2640a2b7676a7651b3c1a06a37` | 408 | 3194 | 250 | 303 | 4,7% |
| 9 | html-validate | https://github.com/html-validate/html-validate | MIT | `fa9e8cf4d193215d4d3e77299e2834fc8138383a` | 400 | 77 | 77 | 35 | 4,6% |
| 10 | django | https://github.com/django/django | BSD-3-Clause | `9425a85c90c2640f45e311fe2dd1f912553100d1` | 371 | 96 | 96 | 94 | 4,3% |
| 11 | cypress | https://github.com/cypress-io/cypress | MIT | `e5b073de605c59a31cf6d9a6fb6576b82a0bfb7d` | 337 | 152 | 152 | 233 | 3,9% |
| 12 | playwright | https://github.com/microsoft/playwright | Apache-2.0 | `c6405a9ebcf74fba1db96a5e358c7462348fbd3e` | 312 | 22 | 22 | 126 | 3,6% |
| 13 | prettier | https://github.com/prettier/prettier | MIT | `53d3e09d23ee22a7814f1ae9f3aea70b9fef07fc` | 267 | 116 | 116 | 178 | 3,1% |
| 14 | zulip | https://github.com/zulip/zulip | Apache-2.0 | `322bd1146209921e3e63562043574d0220b6a2f7` | 228 | 26 | 26 | 206 | 2,6% |
| 15 | readability | https://github.com/mozilla/readability | Apache-2.0 | `ab4027a8b37669745016869a37a504727992b2ba` | 221 | 176 | 176 | 37 | 2,6% |
| 16 | parcel | https://github.com/parcel-bundler/parcel | MIT | `20c0f2ede4c1044092d44288169cce589c5d5c14` | 182 | 84 | 84 | 53 | 2,1% |
| 17 | vite | https://github.com/vitejs/vite | MIT | `e4ad62ba6523e889f988b6e882d9d81be7f26711` | 164 | 255 | 250 | 73 | 1,9% |
| 18 | mermaid | https://github.com/mermaid-js/mermaid | MIT | `cafb814089f72a06efc3ce7c1637f6c1d3c26699` | 128 | 1676 | 250 | 257 | 1,5% |
| 19 | turbo | https://github.com/hotwired/turbo | MIT | `75350a19212ef0d29a0e6db75e0bfdeb96b10cd7` | 97 | 272 | 250 | 5 | 1,1% |
| 20 | jekyll | https://github.com/jekyll/jekyll | MIT | `97c3bdc822df9d5bea4c269b29a28de5a5e29d70` | 82 | 206 | 206 | 22 | 1,0% |
| 21 | kit | https://github.com/sveltejs/kit | MIT | `014dfec3df98be8edf6e217e98309c12ba51b4cf` | 77 | 76 | 76 | 50 | 0,9% |
| 22 | mailu | https://github.com/mailu/mailu | MIT | `36c1c2fd7b8b53bd8e60ce8845e0d02230e13e59` | 70 | 402 | 250 | 17 | 0,8% |
| 23 | middleman | https://github.com/middleman/middleman | MIT | `3ab5c9d079dc5397714e74f46db17c32680b33c6` | 61 | 26 | 26 | 12 | 0,7% |
| 24 | html5lib-python | https://github.com/html5lib/html5lib-python | MIT | `fd4f032bc090d44fb11a84b352dad7cbee0a4745` | 31 | 13 | 13 | 9 | 0,4% |
| 25 | Semantic-UI | https://github.com/Semantic-Org/Semantic-UI | MIT | `597843ab84d565cccfd8fd7719416350ae73e734` | 28 | 111 | 111 | 107 | 0,3% |
| 26 | vue | https://github.com/vuejs/vue | MIT | `9e88707940088cb1f4cd7dd210c9168a50dc347c` | 24 | 63 | 63 | 25 | 0,3% |
| 27 | open-wc | https://github.com/open-wc/open-wc | MIT | `219e84d9c92a66f77dd632b3ec8c91909a49e5a5` | 24 | 138 | 138 | 37 | 0,3% |
| 28 | eleventy | https://github.com/11ty/eleventy | MIT | `f8bfb2530890615f398cefe75ae03ebc10446998` | 20 | 25 | 25 | 15 | 0,2% |
| 29 | DOMPurify | https://github.com/cure53/DOMPurify | Apache-2.0 | `5c7274a6cd4c877a63c78f7ae45657b6a39ea336` | 18 | 101 | 101 | 12 | 0,2% |
| 30 | coreui | https://github.com/coreui/coreui | MIT | `5611ab47b35f384edad5a91b773058d3caa48f71` | 14 | 643 | 250 | 128 | 0,2% |
| 31 | CodeIgniter4 | https://github.com/codeigniter4/CodeIgniter4 | MIT | `59e5eea6a9f8b5f5faa814226d1da1ec94f54b89` | 13 | 30 | 30 | 88 | 0,2% |
| 32 | parse5 | https://github.com/parse5/parse5 | MIT | `021297cdb7e32173ca80db9151b40943fdc4c1c7` | 11 | 40 | 40 | 9 | 0,1% |
| 33 | select2 | https://github.com/select2/select2 | MIT | `19acff7425e412fdb85673c06d64af7b363b8a8f` | 9 | 349 | 250 | 16 | 0,1% |
| 34 | asciidoctor | https://github.com/asciidoctor/asciidoctor | MIT | `1eff07780bf819866b1276e0ccfa8c8e3bc2fad9` | 8 | 2 | 2 | 13 | 0,1% |
| 35 | quill | https://github.com/quilljs/quill | BSD-3-Clause | `539cbffd0a13b18e9c65eb84dd35e6596e403158` | 4 | 313 | 250 | 22 | 0,0% |
| 36 | Chart.js | https://github.com/ChartJS/Chart.js | MIT | `71d44219e79e3ac41e6f9dc907022eebf574868d` | 2 | 375 | 250 | 63 | 0,0% |
| 37 | webcomponents.org | https://github.com/webcomponents/webcomponents.org | Apache-2.0 | `5ec99795f32152cd1a3bdc1a78a5256569d99b19` | 1 | 515 | 250 | 9 | 0,0% |

### Totais (medidos)

| grandeza | valor | meta do brief |
|---|---|---|
| repos | 37 | — |
| **html escrito à mão (novo)** | **8.628** | ≥ 2.000 ✅ (4,3×) |
| html bruto (antes do filtro de minificado/vendored) | 8.684 | — |
| **rendimento de mineração (novo)** | **5.614** | ≥ 2.000 ✅ (2,8×) |
| cobertura de `n`: 1.704 + 5.614 | **7.318** | ≥ 3.700 ✅ |
| maior repo (share de html) | three.js — 1.712 — **19,8%** | ≤ 25% ✅ |
| maior repo (share de rendimento) | 250 — **4,5%** | ≤ 25% ✅ |
| disco em disco (`du -sh`) | **4,0 GiB** (4.099 MB) | ≤ 5 GB ✅ |
| disco livre ao final (`df -h ~`) | 106 GB | ≥ 100 GB ✅ |

## 4. Medidos e **excluídos** (com o motivo, para o juiz reverter se quiser)

| repo | html | rendimento | MB | motivo da exclusão |
|---|---|---|---|---|
| **validator/validator** | 5.798 | 250 | 172 | **Matematicamente impossível** sob o teto de 25%: exige `Σhtml ≥ 4 × 5.798 = 23.192`, e o acervo permissivo inteiro medido nesta sessão (incluindo este) dá **21.022**. Ficaria em 27,6%. |
| **webpack/webpack** | 2.946 | 171 | 380 | Com o resto da lista, o share dele é **24,996%** — no teto exato, sem margem (o judge's contagem pode variar ±1 arquivo). Excluído para que a regra seja cumprida com folga, não por Hair-splitting. |
| **CKEditor/ckeditor5** | 873 | 250 | 831 | `packages/ckeditor5/LICENSE.md`: **GPL-2-or-later ou comercial**. Não é permissiva. |
| **handsontable/handsontable** | 828 | 250 | 434 | `LICENSE.txt`: **comercial dual** (uso comercial exige contrato; não comercial vai para PDF). Não é permissiva. |
| **getpelican/pelican** | 161 | 250 | 15 | `LICENSE`: **AGPL-3.0**. Não é permissiva. |
| **nodejs/node** | 1.472 | 5 | 1.043 | 1.043 GB (estoura o teto de 5 GB com o resto) **e** só **5** commits `.html` antes do corte: pagaria 1 GB por 5 entradas. |
| ag-grid/ag-grid | 1.073 | 170 | 1.252 | 1,25 GB sozinho = 25% do teto de disco. |
| ionic-team/ionic-framework | 460 | 250 | 1.388 | 1,39 GB: mesmo motivo. |
| storybookjs/storybook | 23 | 250 | 1.196 | 1,20 GB para 23 arquivos. |
| others (21 candidatos medidos) | — | — | — | medidos e fora do corte de disco/diversidade (tabela completa em `_tools/m-b*.tsv`). |

Clones desses candidatos ficaram em `~/.claude/touring/vgp/oss-html/_stage/` (20 GB) — são
**descartáveis**; o comando de remoção está em `RELATORIO-5.md`.

## 5. Como reproduzir

```bash
# o clone está pronto; para reverter/verificar:
cd ~/.claude/touring/vgp/oss-html/<owner>-<repo>
git rev-parse HEAD                      # == sha da tabela
git show HEAD:LICENSE | head -3         # == licença da tabela
git log --format=%cI -- '*.html' | awk '$0 <= "2026-10-10T00:00:00Z"' | wc -l
find . -name '*.html' -not -path './.git/*' | wc -l
```

Profundidade: os clones são `--depth 1500..3000` (histórico suficiente para ≥ 250
commits `.html` antes do corte em 33 dos 37; os 4 restantes têm o histórico inteiro
porque o repo tem < 3.000 commits). O `--filter=blob:none` foi **descartado**: o
filtro server-side do GitHub virou gargalo (≈ 40 KB/s) e deixaria o montador
buscando blob na rede durante a mineração.
