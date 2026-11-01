---
type: Relatorio
title: P0 do held-out — coletor de atributos de módulo (bloco composto e ligação dinâmica)
plan_id: 2026-10-08-vgp-motor-base
timestamp: 2026-10-09T23:30:00-03:00
autor: subagente p0-atributo (herdou o P0 da sessão Kimi touring-35)
brief: briefs/2026-10-09-touring-35-p0-atributo-dinamico.md
tags: ["#kind:report", "#status:delivered", "#domain:harness"]
---

# Relatório P0-atributo (09/10/2026)

> **A entrega é hipótese até o held-out novo (semente 20261011) e o juiz da coordenadora.** Nada commitado,
> nada propagado. **Binário FINAL congelado (P0 + determinismo + dataclass):** `~/.claude/touring/vgp/binarios/4501e1e5/touring`,
> sha256 `4501e1e56294a81af7379a7e56929c01344e4b611cbf6e7a7a9129366bc09073` (arquivo `touring.sha` ao lado; o rebuild do fonte final reproduz o MESMO sha, `cmp` rc 0).
> Binários anteriores, SUPERADOS e mantidos como medida «antes»: `f55ec4b5` (só o P0; sha `f55ec4b5e027055a349bd5fa82a5f9d386ea97a8c8627177a9e8d7967f347117`) e `70e0051a` (P0 + determinismo, sem a semântica do dataclass; sha `70e0051a0e8879bf8cf31207ad43f7f2ffabbc0cdd1494946cbee1293286632f`).
> Atualizado em 10/10/2026 com os pedidos da coordenadora: comparação contra o corpus anterior (`.bak`) e o `corpus_v0d` novo (§2.6.1), determinismo de `reais`/`sugestao` (§2.9) e `params` do dataclass pela semântica do `dataclasses`, com veredito por id idêntico ao do `f55ec4b5` (§2.10).

## 1. Identidade

- Executor: subagente `p0-atributo` (modelo Sonnet 5.5), delegado pela touring-09 (`team-lead`) em 09/10 ~19:50–23:30 BRT; pedidos adicionais da coordenadora em 10/10 (corpus `.bak` × `corpus_v0d` novo; determinismo de `reais`/`sugestao`).
- Worktree: `/home/gabrielgadea/projects/touring-wt-vgp-attr` · branch `fix/2026-10-09-vgp-atributo-dinamico` · base `88c7cc8b`
  (snapshot do motor). **Sem commit.** Árvore principal e worktrees da VL0/L2 intocados.
- Binário de referência (RED/baseline): `~/.claude/touring/vgp/binarios/e935aaf1/touring` (sha256 `e935aaf17d9a09d0…`).
- Consultas ao Context7 (citadas por pedido): CPython `Lib/_collections_abc.py` e `Lib/re/__init__.py` (o idioma PEP 562 de
  `__getattr__` com guarda literal + `raise AttributeError` final → base da análise de `__getattr__` FECHADO);
  `Doc/reference/datamodel.rst` (`sys.modules[__name__].__class__ = …` → sinal dinâmico); `tree-sitter-python`
  (`try_statement`/`if_statement`/`with`/`for`/`match`/`global_statement`/`type_alias_statement`; as formas foram conferidas
  por sonda com o py-tree-sitter 0.25 antes de codar).

## 2. Feito, com evidência

### 2.1 Diff (arquivos e funções)

| arquivo | mudança |
|---|---|
| `crates/touring-code/src/vgp/namespace.rs` **(novo, 966 L)** | coletor de namespace de módulo. `collect()` + `Namespace{names,has_star,has_getattr,has_dynamic}`; varredura de **cursor único** (`walk_tree`/`Visitor`: `AliasCandidates`, `BindWalk`, `SignalWalk`); ligações em TODO ramo (`bind_node`, `bind_target`, `bind_import*`, `bind_type_alias`, `bind_global`); sinais dinâmicos (`signal_store`, `signal_call`, `call_by_name/method`, `hands_namespace`, `bind_key`, `bind_dict_args`); **`__getattr__` fechado** (`closed_getattr`, `guarded`, `literal_guard`, `guard_comparison`, `guard_boolean`) |
| `crates/touring-code/src/vgp/namespace/tests.rs` **(novo, 480 L)** | 31 testes do coletor + 1 instrumento `#[ignore]` (censo) |
| `crates/touring-code/src/vgp/modules.rs` | `ModuleInfo.has_dynamic`; `extract_module_info` chama `namespace::collect` (une `names`/`has_star`/`has_getattr`/`has_dynamic`); `collect_def` não decide mais `has_getattr` (o coletor decide fechado×aberto); `Resolver::site_packages`, **`Resolver::extension_exists`**, `dir_has_extension`; helpers `pub(super)`. **Determinismo (§2.9):** `ClassInfo.field_order` (campos na ordem de declaração) e `init.params = field_order` + teste `dataclass_init_segue_a_ordem_de_declaracao` |
| `crates/touring-code/src/vgp/checks.rs` | `module_has`: `has_dynamic` ⇒ `Tri::Unknown`; submódulo = `module_exists` **ou** `extension_exists`. **Determinismo (§2.9):** `REAIS_MAX`, `reais_dos_membros` (ordem total antes do corte; usada pelos 2 sítios de `check_membro`), `sugestao_proxima` com desempate `(distância, nome)` |
| `crates/touring-code/src/vgp/mod.rs` | `pub mod namespace;` |
| `crates/touring-code/src/vgp/checks/tests.rs` | +10 testes `p0_*` (e2e do motor com fixtures do venv e do projeto, nos 3 baldes) · +4 testes `det_*` (10 chamadas byte a byte, `reais` pinados, desempate em 720 permutações, ordem dos membros) |

Desenho (1 parágrafo): o extrator antigo lia só os filhos DIRETOS do módulo. O coletor atravessa `if/elif/else`, `try/except/else/finally`,
`with`, `for`/`while` (+`else`), `match` e devolve um **superconjunto** dos nomes (ramo que nunca roda também conta: só reduz deny).
Os tipos (`imports`/`plain_imports`/`classes`/`functions`, que alimentam a inferência) continuam só do topo — assim um alias ligado a
módulos diferentes em ramos diferentes não cria receptor falso. Os mecanismos de ligação dinâmica que se **fecham** viram nomes
(`globals()[k] = …` com `k` literal ou laço sobre lista literal; `setattr(<módulo>, "x", …)`; `globals().update({"a": 1})`;
`__getattr__` guardado por `attr == "lit"`/`attr in (…)` cujo fim é `raise` — PEP 562: cair no fim devolve `None`, que é VALOR válido);
os que não se fecham levantam `has_dynamic`/`has_getattr` e o motor **se abstém** (`Tri::Unknown`, silencioso e igual nos 3 baldes).

### 2.2 RED → GREEN nos módulos REAIS (venv da analise: numpy 2.4.1, Pillow 11.3.0, `scripts/aco/rust_bridge.py`)

`sonda_p0.py` (`diagnostics/p0-atributo-2026-10-09/`): 3 baldes — `proj` (arquivo virtual no projeto), `tmp` (`/tmp/…`), `inline` (`<string>`).

| sonda | RED `e935aaf1` (proj / tmp / inline) | GREEN `f55ec4b5` (proj / tmp / inline) |
|---|---|---|
| `np.array(...)` · `np.where(x)` | warn / **deny** / **deny** (`array`,`where`) | allow / allow / allow |
| `Image.LANCZOS` (Pillow) | warn / **deny** / **deny** | allow / allow / allow |
| `rb.RUST_AVAILABLE` (try/except) | warn / **deny** / **deny** | allow / allow / allow |
| controle `np.arrai` | warn / deny / deny | **warn / deny / deny (idêntico)** |
| controle `os.path.join` | allow ×3 | allow ×3 (idêntico) |
| observação `os.pathh` | allow ×3 | allow ×3 (idêntico — ver 2.3) |
| `math.sqr` (tabela fechada) | deny ×3 | deny ×3 |

O `np.arrai` seguir deny **não é trivial**: o `numpy/__init__.py` real tem os TRÊS mecanismos (bloco composto `else:`,
`for ta in [...]: globals()[ta] = …` e um `__getattr__` de módulo dentro do `else:`). Aplicar o item 2 do brief ao pé da letra
(qualquer `__getattr__`/`globals()[…] =` ⇒ abstenção) transformaria o controle em silêncio — foi exatamente o que os mutantes
M4/M5 reproduzem (§2.5). Por isso os mecanismos FECHADOS são fechados, não só abstidos.

Regressão (aceite 1): os mesmos casos como testes — `checks/tests.rs::p0_*` (10) + `namespace/tests.rs` (31) — **RED no código
original** (M0, 6–7 testes falham) e **GREEN** no código final (108 testes `vgp::` verdes, 1 ignorado).

### 2.3 `os.pathh` → allow: é DESENHO (lacuna de cobertura declarada), não defeito do coletor

`stdlib_model.rs` (doc, linhas 1–7) fecha só `unicodedata` e `math`: «a name absent from these tables is NOT a deny». `import os` →
`plain_imports["os"]="os"` → `resolve_module("os")` procura `<root>/os.py`, `<root>/os/__init__.py`, o irmão do importador e
`<root>/.venv/lib/python*/site-packages/os*` — a stdlib mora no interpretador base, não no venv → `None`; `stdlib_module_members("os")` → `None`
→ `tipo_de` = `None` → nenhuma checagem. Prova: `math.sqr` nega nos 3 baldes e `os.pathh` não, **com o código original e com o novo**
(teste `p0_stdlib_sem_tabela_fechada_nao_e_modelada_por_desenho`). Fechar isso é decisão à parte (mais tabelas fechadas — `os` varia por
plataforma — ou resolver a stdlib do interpretador).

### 2.4 Achados ALÉM das 3 sondas (mesma família: namespace de módulo de terceiros julgado fechado quando não é)

Medidos contra o **runtime** (`dir(módulo)` importado no venv da analise; 358 pacotes, 18.013 nomes públicos; `varre_runtime.py`):

| medição | `e935aaf1` (base) | `f55ec4b5` (novo) |
|---|---|---|
| pacotes com ≥1 atributo REAL negado | **114** | **1** (`humanfriendly`: `aliases`/`module` são atributos do *proxy* de depreciação que substitui o módulo — artefato da medição) |
| nomes reais negados | **4.044** (cv2: 2.435) | **2** |
| `from pkg import <extensão .so>` negado (159 imports em 72 pacotes) | **112** (`from lxml import etree`, `PIL._imaging`, `pyarrow._compute`…) | **0** |

Repetido em 10/10 com o binário final `4501e1e5` (`varre_4501.json`, `ext_varre_4501.out`): **358 pacotes, 1 com falso deny (`humanfriendly`, 2 nomes) e 0 dos 159 imports de extensão negados** — idêntico ao `f55ec4b5`.

O segundo (`from lxml import etree` → deny) **não estava no brief** e é pré-existente nos dois binários: submódulo compilado (`etree.cpython-312-…so`)
não tem `.py`, então `module_exists` dizia «não existe». Corrigido (`Resolver::extension_exists`; `.so`/`.pyd` com o ponto depois do nome — `etree_extra.so`
não é `etree_`). REGRA #21: falha observada, mesma família, conserto de 20 linhas com teste e 2 mutantes — por isso entrou.

Censo do namespace ESTÁTICO contra o runtime (`censo.py`, `censo2.py`): **358 pacotes** → 82 abstêm (`star`/`getattr`/`dynamic`), 1 buraco sem marca
(`humanfriendly`, acima); **1.091 submódulos** (≤ 2 níveis, ~95 pacotes populares) → 105 abstêm, **1 buraco sem marca**
(`matplotlib.backends.backend_agg`: 8 nomes injetados por um decorador de OUTRO módulo, `@_Backend.export`). Esse censo achou, e o
conserto fechou, 3 buracos reais: `from __future__ import annotations` (18 pacotes, nó `future_import_statement`),
`sys.modules["pkg.alias"] = sub` marcado como «módulo substituído» (pptx — superabstenção), e `lazy_import(globals(), …)` (websockets — um chamador que RECEBE o namespace).

### 2.5 Mutantes (aceite 2) — 23/23 mortos, testes limpos (108 verdes) antes de cada rodada

Cada mutação aplicada por substituição de texto no worktree, `cargo test -p touring-code --lib vgp::`, arquivo restaurado por `cp` (verificado idêntico).
Os 3 pedidos no brief estão em **negrito**.

| id | mutação | testes que a matam |
|---|---|---|
| M0 | **código original** (sem coletor; `has_getattr` de topo) — prova o RED | 6 (`p0_numpy_*`, `p0_pillow_*`, `p0_rust_bridge_*`…) |
| **M1** | **remover a travessia de compostos** (numpy/rust_bridge voltam a deny) | 11 (`p0_numpy_bloco_composto…`, `p0_rust_bridge…`, `if_elif_else…`, `try_except…`, …) |
| **M2** | **remover o sinal dinâmico** (Pillow volta a deny) | 7 (`p0_pillow_*`, `globals_com_chave_nao_literal_e_aberto`, …) |
| **M3** | **severidade divergente arquivo×inline** (alvo incerto nega só em tmp/inline) | 2 (`p0_pillow_setattr_dinamico_abstem_com_a_mesma_resposta_nos_tres_baldes`, `p0_pillow_import_de_nome_dinamico_nao_nega`) |
| M4 | `__getattr__` nunca fecha (`np.arrai` vira silêncio) | 6 (inclui `p0_controle_numpy_typo_continua_negando_em_tmp_e_inline`) |
| M5 | `globals()[k]` sobre lista literal não fecha | 5 |
| M6 | sem a exigência do `raise` final | 1 (`getattr_sem_raise_final…`) |
| M7 | `else`/`elif` herdam a guarda | 1 (`getattr_return_no_else…`) |
| M8 | alias `module = sys.modules[__name__]` não conta | 5 |
| M9 | `setattr` em qualquer objeto vira sinal | 2 (`setattr_sobre_objeto_que_nao_e_o_modulo_nao_conta`, …) |
| M10 | `global` em função não declara nome | 1 |
| M11 | entregar `globals()`/o módulo a um chamador não abstém | 1 |
| M12 | `sys.modules[qualquer] = x` substitui ESTE módulo | 1 |
| M13 | módulo nomeado por literal conta como este módulo | 1 |
| M14 | `from __future__ import …` não liga | 1 |
| M15 | sem a lista de chamadores só-leitura (`print(globals())`) | 2 |
| M16 | `type X = …` não liga | 1 |
| M17 | `exec` nunca abre | 2 |
| M18 | `or` de duas guardas literais não fixa os nomes | 1 |
| M19 | sem extensão compilada (`lxml.etree` volta a deny) | 1 |
| M20 | extensão sem o ponto obrigatório (`etree_extra.so` passa por `etree_`) | 1 |
| M21 | a profundidade de escopo não desce ao sair de `def`/`class` | 1 |
| M22 | o laço literal não é desempilhado ao sair do `for` | 1 |

Honestidade do processo: numa rodada intermediária um teste novo (`o_escopo_volta…`) estava quebrado no código SEM mutação e «matou» 15 mutantes de graça;
percebi pela repetição do nome em todas as linhas, corrigi o teste (a chave literal fechava a ligação) e refiz a rodada com os testes limpos primeiro
(`mut_pos_teste.sh` aborta se os testes não passam). A tabela acima é a rodada refeita.

### 2.6 Juiz v2 e V3, por id, binário congelado `f55ec4b5` × `e935aaf1` (aceite 3)

Corpus: a coordenadora **promoveu** o `corpus_v0d` durante a sessão (dir-pai mexido 20:37). Medi nos DOIS: o promovido (sha `entradas` `28d93bcb…`/`inline_neg` `0af8c7a8…`)
e o anterior (`corpus_v0d.bak-2026-10-09`, sha `7b819af2…`/`57574182…` = o corpus dos `k_v2_vivo.json`/`k_v3_vivo.json`; o brief cita o nome `corpus_v0d.bak-2026-10-09`, que existe).
O binário-base foi **re-executado por id** nos dois (o `k_*.json` guarda só agregados): reproduz `k_v2_vivo.json`/`k_v3_vivo.json` exatamente
(mesmos `perdidos`, `falsos`, positivos 11/13·20/27 e 12/13·22/27, avisos 58 e 60).

| corpus · camadas | comparados por id | divergências | NOVO deny | deny removido | perdidos (base → novo) | falsos deny | positivos arq/inline | avisos em negativos | p95 base → novo | veredito novo |
|---|---|---|---|---|---|---|---|---|---|---|
| bak · v2 (camada 1) | 762 | 10, **todas warn→menos warn** | **0** | 0 | 9 → 9 (**mesmos ids**) | 0/400 · 0/300 (lim. sup. 0,75% / 0,99%) | 11/13 · 20/27 | 58 → **49** | 111,9 → 144,2 ms | PRONTO |
| bak · V3 (1,2) | 762 | 12, todas warn→menos warn | **0** | 0 | 6 → 6 (mesmos) | 0 | 12/13 · 22/27 | 60 → 50 | 1000,9 → 1101,1 ms | NÃO PRONTO (só p95) |
| promovido · v2 | 738 | 10, todas warn | **0** | 0 | 8 → 8 (mesmos) | 0/400 · 0/274 | 11/13 · 21/27 | 58 → 49 | 136,6 → 132,3 ms | PRONTO |
| promovido · V3 | 738 | 10, todas warn | **0** | 0 | — (mesmos) | 0 | 12/13 · 23/27 | 61 → 52 | 1024,6 → 1119,6 ms | NÃO PRONTO (só p95) |

As 10 divergências do v2 (e do V3 promovido) são 9 programas que usavam `np.array`/`np.where`/… em arquivo do projeto (warn falso eliminado) e 1 de constante de módulo ligada em bloco
composto (`BANDA_DA_HISTERESE_PT`); nenhuma mudou um deny. As 2 divergências a mais do V3-bak (`158cbb7178bb`, `76d4d4a88939`: `membro get`, warn→allow) NÃO aparecem no v2 do mesmo
corpus — a camada 1 é idêntica nesses ids —, logo vêm da camada 2 (pyright) entre execuções feitas com 1 h de distância, não do coletor. Detalhe por id em `cmp_*.json`.

**O p95 do V3 reprova nos DOIS binários** com a máquina a 92–96 °C e load 6–7 (base 1000,9 e 1024,6 ms; antes, em máquina mais fria, a Kimi mediu 907,9). O p95 do v2
oscila entre 110 e 145 ms nos dois binários em horários diferentes; no mesmo horário (promovido) o novo foi 132 contra 137 do base.
Custo do coletor, medido de três jeitos:
- **p95 pareado por id** (`juiz_tempos.py`/`tempos.sh`: 762 chamadas × 3 repetições alternadas base/novo, mínimo por id): **p95 110,2 → 112,9 ms (+2,7)**; mediana 2,12 → 2,45 ms; 104 ids ficaram mais rápidos e 75 mais lentos em > 5 ms (27 em > 15 ms) — ruído térmico dos dois lados.
- **A/B alternado** (`bench_ab.py`, 30 rodadas): **+0,4 a +2,4 ms** em numpy/pandas/scipy/requests/projeto e **+7 ms** em módulo de 4.000 linhas (Pillow `Image.py`: 21 → 28 ms). O primeiro desenho (3 varreduras recursivas, um `Node::walk()` alocado por nó) custava +17 ms nesse módulo; a varredura de cursor único cortou para +7.
- **Pior caso**: `pymupdf/__init__.py` (26.093 linhas, 924 KB): 150–160 → 212–215 ms (+60 ms; ~2,3 µs/linha nas 2–3 varreduras). As chamadas que importam `pymupdf` são as maiores altas do corpus (145 → 207 ms). Seguem 2 caminhos de redução se isso importar: fundir a varredura de aliases com a de sinais (2 em vez de 3) e um pré-filtro textual (`setattr`/`globals`/`sys.modules`/`__getattr__`/`global`…) que pula os passes quando o módulo não tem nenhum desses gatilhos — não feitos (sem ganho no pymupdf, que os tem).

#### 2.6.1 Rodada FINAL (10/10) com o binário `4501e1e5`, nos dois corpora que a coordenadora pediu

Por pedido dela, a comparação contra `k_v2_vivo.json`/`k_v3_vivo.json` usa `--corpus ~/.claude/touring/vgp/corpus_v0d.bak-2026-10-09` (sha `entradas` `7b819af2…` / `inline_neg` `57574182…`) e o `corpus_v0d` novo
(staging com evidência; `entradas` `28d93bcb…` / `inline_neg` `0af8c7a8…`) contra `k_v2_ev.json`/`k_v3_ev.json`. A base `e935aaf1` foi **reexecutada por id** nos dois e **reproduz** as quatro referências em todas as chaves de medida
(`pronto`, `recall_simbolo`, `positivos_simbolo`, `perdidos` e `falsos` por id, `negativos`, `avisos_em_negativos`, `chamadas`, `erros`…); só `p95_ms` difere (máquina), mais as chaves `falso_deny_por_balde`/`exigir_limite_superior`
que o juiz atual acrescenta (`comparar_k.py`).

| corpus · camadas | comparados por id | divergências × `e935aaf1` | NOVO deny | perdidos (base → novo) | falsos deny (CP95 sup.) | positivos arq/inline | avisos em negativos | p95 base → novo (1 run, máquina quente) | veredito do juiz |
|---|---|---|---|---|---|---|---|---|---|
| bak · v2 | 762 | 10, **todas warn→menos warn** | **0** | 9 → 9 (**mesmos ids**) | 0/400 (0,75%) · 0/300 (0,99%) | 11/13 · 20/27 | 58 → **49** | 110,1 → 119,2 ms | PRONTO |
| bak · V3 | 762 | 10, todas warn→menos warn | **0** | 6 → 6 (mesmos) | 0 | 12/13 · 22/27 | 60 → 51 | 1000,9 → 1055,9 ms | NÃO PRONTO (só p95) |
| ev · v2 | 738 | 10, todas warn→menos warn | **0** | 8 → 8 (mesmos) | 0/400 (0,75%) · 0/274 (1,09%) | 11/13 · 21/27 | 58 → 49 | 131,3 → 144,0 ms | PRONTO |
| ev · V3 | 738 | 10, todas warn→menos warn | **0** | 5 → 5 (mesmos) | 0 | 12/13 · 23/27 | 61 → 52 | 984,0 → 1037,7 ms | NÃO PRONTO (só p95) |

Os 10 ids divergentes são os mesmos nos quatro (9 `warn→allow` de `np.array`/`np.where`/… + 1 `warn→warn` com menos achados). O limite superior do inline do `ev` (1,09%, n = 274) fica acima do teto de 1% porque o juiz desta rodada
NÃO roda com `--exigir-limite-superior` e n é pequeno; é igual no binário base (0 falsos nos dois) — o teto de 1% é da coordenadora, no held-out.

**Condição da coordenadora (mensagem 3): veredito por id IDÊNTICO ao do `f55ec4b5`**, comparando `verdict` + `(classe, nome, linha, severidade)` de cada achado (`compara_f55.py`; só `reais`/`sugestao` ficam de fora):

| comparação | corpus · camadas | ids | diferenças |
|---|---|---|---|
| `f55ec4b5` × `4501e1e5` | ev · v2 | 738 | **0** |
| `f55ec4b5` × `4501e1e5` | ev · V3 | 738 | **0** |
| `f55ec4b5` × `4501e1e5` | bak · v2 | 762 | **0** |
| `f55ec4b5` × `4501e1e5` | bak · V3 | 762 | **2**, ambos explicados por timeout do pyright no run do `f55` (abaixo) |
| `70e0051a` × `4501e1e5` | ev · v2 e V3 | 738 | **0** e **0** |
| `70e0051a` × `4501e1e5` | bak · v2 | 762 | **0** |

As 2 diferenças do bak·V3 são `158cbb7178bb` e `76d4d4a88939` (`membro get`): no run do `f55` o campo `camada2` é «unknown: prazo de 1000 ms estourado esperando publishDiagnostics» (a camada 2 não respondeu a tempo, o achado é só dela, e a camada 1 desses ids é idêntica nos
três binários — v2 sem diferença) e no run do `4501e1e5` a camada 2 respondeu (`ok`). **Re-medi só esses 2 ids em V3, 5 rodadas alternadas por binário** (`v3_dirigido_2ids.json`): `f55ec4b5`, `4501e1e5` e `e935aaf1` dão **`warn` com camada 2 `ok` em 5/5** nos dois ids.
Ou seja, com a camada 2 respondendo o veredito é o mesmo; nenhum veredito mudou. Essas 2 diferenças são **não determinismo por TEMPO da camada 2** (o limite de 1000 ms do pyright sob carga), não do motor: com a medição pareada do `tempos_dc` os timeouts por repetição são 24·19·21 no binário final contra 48·32·19 na base. (Isto substitui a hipótese da §2.6 acima, «vêm da camada 2 entre execuções», por evidência: o campo `camada2` do próprio motor.)

**Timeouts da camada 2 por run de V3** (limite de 1000 ms do pyright; sob carga a contagem sobe e leva o p95 do V3 a ~1000 ms):

| run | `e935aaf1` | `f55ec4b5` | `4501e1e5` |
|---|---|---|---|
| bak · V3 | 27 | 96 | 45 |
| ev · V3 | 22 | 85 | 25 |

Esses números mudam de run para run com a carga da máquina (92–97 °C, 5–7 sessões), não com o binário — por isso o p95 do V3 é medido PAREADO (alternando os binários no mesmo período, abaixo), não pelos runs isolados.

**Custo medido PAREADO** (`tempos_dc.sh` → `agrega_tempos.py`; 738 ids do `corpus_v0d`, 3 repetições ALTERNADAS entre os binários no mesmo período, mínimo por id; a máquina estava a 92–97 °C com os gates rodando junto):

| modo | `e935aaf1` (base) | `f55ec4b5` | `4501e1e5` (final) |
|---|---|---|---|
| camada 1 (v2) · p95 do mínimo por id | 113,0 ms | 119,9 ms | **111,7 ms** |
| camada 1 (v2) · mediana do mínimo por id | 2,38 ms | 2,75 ms | 2,79 ms |
| V3 (camadas 1,2) · p95 do mínimo por id | 957,4 ms | — | **920,3 ms** |
| V3 · camada 2 estourada, por repetição | 48 · 32 · 19 | — | 24 · 19 · 21 |

Diferença pareada do `4501e1e5` contra a base: mediana +0,18 ms na camada 1 (p95 da diferença 10,6 ms; 67 ids > 5 ms mais lentos, 5 mais rápidos) e −4,9 ms no V3 (ruído do pyright). Os timeouts da camada 2 NÃO sobem com o binário novo quando medidos alternados (24·19·21 contra 48·32·19 da base): a contagem dos runs isolados da tabela anterior (27/96/45) é a carga da máquina.
O `f55ec4b5` também foi medido pareado na camada 1 (119,9 ms); a diferença entre `f55ec4b5` e `4501e1e5` (8 ms no p95) é ruído térmico de repetição, não código: a mudança do dataclass só roda quando há `kwarg` numa classe importada.


### 2.7 Gates (aceite 5) — árvore FINAL (binário `4501e1e5`), `det_gates2.sh` → `dg2.rc`

| gate | rc | evidência |
|---|---|---|
| `cargo fmt --all -- --check` | 0 | `dg2_fmt.log` |
| `clippy -D warnings --all-targets` (`touring-code`, `touring-server`, `touring-hooks`) | 0 | `dg2_clippy.log` |
| `clippy -D warnings --all-targets --features pre-hooks,post-hooks` (`touring-hook-handlers`) | 0 | `dg2_clippy_hh.log` |
| `cargo test -p touring-code` (lib + integração + doctests) | 0 | **1.166 passed, 0 failed, 6 ignored** (eram 1.155: +5 `det_*`/`dataclass_init…` e +6 `dc_*`) |
| `cargo test -p touring-server` (lib + E2E, com os binários debug por symlink) | 0 | **1.799 passed, 0 failed, 34 ignored** |
| `cargo test -p touring-hook-handlers --features pre-hooks,post-hooks` | 0 | **733 passed, 0 failed** |
| `cargo test -p touring-hooks` | **101** | 1.169 passed, **3 falhas — as MESMAS 3 do baseline `88c7cc8b` sem a minha mudança** (nota abaixo) |
| rebuild release do fonte final + `cmp` com o binário congelado | 0 | `4501e1e56294a81a` = `4501e1e56294a81a`, **`cmp` rc 0** (o último ajuste do fonte foi só um comentário, com o mesmo número de linhas: o binário não mudou) |

Proveniência do `dg2` (pedido da coordenadora, fonte congelada): o `checks.rs` foi tocado pela última vez às **12:28:37 -03** (15:28:37Z; só o comentário de doc de `reais_dos_membros`, mesmo número de linhas) e o `dg2` **começou às 12:50:27 -03** (15:50:27Z, nascimento de `dg2.rc`), gravando no topo do `dg2.rc` o sha que LEU: `checks.rs` **`ed8a5319d87dea65`**, `modules.rs` `e121c6e0a8f2ee01`, `checks/tests.rs` `c61ff1e34c33fd06`, `namespace.rs` `b4bfbab5cc9b558d`, `namespace/tests.rs` `18a21f07b4c516e6`, `mod.rs` `fa142ff96b9fa7e6`.
O rebuild release e o `cmp` (último passo do `dg2`) rodaram de 13:01:11 a 13:08:02 -03, portanto também sobre o `ed8a5319`. O congelamento do `4501e1e5` (11:55 -03) é que foi feito sobre o `7fb2c562`; o `cmp` rc 0 prova que a diferença entre os dois é só comentário.

Rodada anterior (09/10, `f55ec4b5`): os mesmos gates, com 1.155 / 1.799 / 733 e as mesmas 3 falhas de `touring-hooks`.

Nota `touring-hooks` (3 falhas, **não são minhas, mas continuam abertas**): `cli_suggester_e2e::classifier_edit_rust_emits_pre_edit_triage_with_tdg_gate`,
`wave25_task_enrichment_e2e::task_create_payload_returns_enriched_context` e `::enter_plan_mode_prompt_is_treated_as_subject`. Prova de que não vêm do P0: `baseline_hooks.sh` trocou
os 4 arquivos tocados pelos de `88c7cc8b` (`git show`, só leitura), pôs `namespace.rs`/`namespace/` de lado (`mv`), rodou os dois alvos e restaurou (sha256 conferido, `cmp` rc 0):
as MESMAS 3 falham (e 18+4 passam). Nenhum dos três alcança `vgp` (`cli_suggester.rs` nem cita `vgp`). Causa **não** esgotada: o teste do `cli_suggester` difere em 61 linhas entre o snapshot e a árvore principal
(trabalho em curso de outra sessão — o `cli_suggester.rs` é idêntico nas três árvores, o teste não); os dois `wave25` pedem enriquecimento do daemon (memória/ready/órfãos) e recebem contexto vazio num worktree sem `.touring/`
(provável, não provado). Entrego como achado para a coordenadora/sessão dona; não os editei por estarem fora do escopo e sob edição concorrente.

Nota (REGRA #21, nada «pré-existente»): na 1ª rodada `touring-server` deu rc 101 com 6 binários de E2E vermelhos (`binary_e2e`, `cli_wave6_e2e`, `e2e_hook_integration`,
`f3_update_component_e2e`, `graph_service_e2e`, `w12_5_per_project_daemon_e2e`) — todos por `Failed to spawn touring binary at <worktree>/target/debug/touring`: os E2E procuram os binários em
`<workspace>/target/{debug,release}` e este worktree usa `CARGO_TARGET_DIR=vgp-test`. Construí `touring` e `touring-daemon` (debug), liguei por symlink em `target/debug/` (removidos ao final),
e a suíte inteira passou. Não é defeito do código, é o contrato de localização do harness; fica registrado porque «0 falhas» só vale com os binários presentes.

### 2.8 Achado fora de escopo, MEDIDO e relevante para o hook: arquivo de 1.082 linhas leva 13–26 s (os DOIS binários)

Nos tempos por id apareceu `8fc9d0ee8d9c` (`scripts/eleitoral/gerar_docx_operacao.py`, 1.082 linhas, 50 KB, uma classe `Renderizador` de ~860 linhas): **26.070 ms no `e935aaf1`, 26.867 ms no `f55ec4b5`**
(13 s numa repetição com a máquina menos carregada); depois dele `f0aa856d45b5` (301 linhas) 363–499 ms e `116d8af3bbab` (1.697 linhas) 1.260–1.340 ms. O coletor não é a causa (+3% do total); o p95 não o vê (é 1 chamada em 762),
mas um hook em modo **Ativo** bloquearia o turno por 13–26 s nesse arquivo. **Hipótese NÃO verificada** (gdb não anexou ao binário release; sem `perf`): `Checks::tipo_do_nome_interno` chama `para_cada_atribuicao`, que
**re-percorre a árvore inteira a cada consulta de nome** sem memoização — cada `self.x` num arquivo com milhares de acessos custa uma varredura (O(acessos × nós) ≈ 4·10⁸ visitas). Conserto provável: memoizar `tipo_do_nome` por nome
(só quando `em_curso` está vazio na entrada, para não cachear o `None` de um ciclo). Não toquei: é `checks.rs` fora do brief; sugiro P próprio, com o `8fc9d0ee8d9c` como RED (alvo ≤ 150 ms).

### 2.9 Pedido 2 da coordenadora (10/10): o motor era NÃO DETERMINÍSTICO em `reais` e `sugestao`

Pedido (mensagens de 10/10): «No mesmo binário e no mesmo id, `findings[].reais` sai numa ordem diferente a cada chamada […] e em 2 ids o texto de
`sugestao` oscila […] Ordene `reais` e dê à sugestão um desempate total (distância, depois o nome). O teste chama o motor 10× no mesmo input e exige
saída byte a byte igual, e o mutante (sem ordenação) tem de morrer», com a correção: em `15d469304a03` e `2463291faf76` o CONJUNTO varia — «o desempate total
precisa valer ANTES do corte». Evidência da coordenadora: `diagnostics/holdout-20261011/motor_ordem_de_reais.out` (a única coisa dali que li: 7 ids e contagens, nada de held-out).

**Causa: 3 fontes, não 2.** Todas são «coleção sem ordem total vazando para o JSON»; `HashSet` itera numa ordem diferente a CADA instância (RandomState por instância) e o motor
monta uma instância nova por chamada:

| # | onde (antes) | sintoma |
|---|---|---|
| 1 | `checks.rs::check_membro`, 2 sítios: `membros.iter().take(8).cloned().collect()` direto do `HashSet` de membros da classe | a ordem de `reais` muda; com > 8 membros o **conjunto** muda (o corte pega 8 quaisquer), e como a `sugestao` sai dos 8 mostrados, o «Você quis dizer» aparece e some |
| 2 | `modules.rs::collect_class`: `ci.init.params = ci.fields.iter().cloned().collect()` (dataclass) | os `reais` do achado `kwarg` saem na ordem do conjunto — e a ordem de declaração do `__init__` (que `FuncSig.params` documenta «in order») se perde; com > 10 campos o conjunto também varia |
| 3 | `checks.rs::sugestao_proxima`: `min_by_key` só pela distância | empate de distância cai na ordem de entrada (que a fonte 1 deixava aleatória) |

`reals_of_module` já ordenava ANTES de cortar (`sort` → `truncate(8)`), e `sig.params` de função comum é `Vec` em ordem. Auditei por grep toda iteração
(`for … in`, `.iter()`, `.keys()`, `.values()`, `.into_iter()`, `.extend()`) em `vgp/*.rs` e `vgp/namespace/`: fora das três fontes acima, o que itera é `Vec` (`scope.rs::issues` percorre `scopes` e `uses`)
ou slice estático (`stdlib_model`), e os `HashMap`/`HashSet` restantes só aparecem em `contains`/`get`/`insert`. A varredura em escala de corpus (abaixo: 0 divergências em 738 + 765 ids) confere, mas só cobre os caminhos que o corpus exercita.

**Conserto** (`checks.rs`, `modules.rs`):
- `reais_dos_membros(&HashSet) -> Vec`: ordem TOTAL sobre o conjunto — nomes públicos primeiro, depois os `_`-prefixados, alfabético em cada grupo — e só DEPOIS o corte (`REAIS_MAX = 8`, constante que `reals_of_module` também usa). Os dois sítios de `check_membro` passam por ela.
- `sugestao_proxima`: `min_by` com `(distância, nome)` — o resultado não depende da ordem de `reais`.
- `ClassInfo.field_order: Vec<String>` (campos na ordem de declaração, preenchido junto do `fields`); `init.params = field_order.clone()`.

**Decisão para você validar**: a ordem alfabética pura poria `__init__`/`__repr__` na frente e eles encheriam o corte de 8 (a recusa «ensina» pelos `reais`, e o `juiz_v4_sessoes.py` mede se a chamada seguinte usa um deles).
«Públicos primeiro» é a mesma regra que `reals_of_module` já tinha (que descarta `_`; aqui os privados ficam depois, não somem). É a única escolha que fiz além do pedido; reverter é 1 linha (`starts_with('_')` na chave do `sort_by`) e o mutante D5 prova que o teste a trava.
O que NÃO mudei: a `sugestao` continua calculada sobre os `reais` exibidos (≤ 8), não sobre todos os membros (limite pré-existente: um typo de um membro fora dos 8 primeiros fica sem «você quis dizer»).

**Testes (RED → GREEN)** — 5 novos, todos RED no código de `f55ec4b5` e GREEN no final (`det_red`/`det_green` em `resultados/`):

| teste | o que prova | falha no RED (valor real do log) |
|---|---|---|
| `checks::tests::det_mesma_entrada_dez_chamadas_dao_os_mesmos_bytes_nos_tres_baldes` | 10× o mesmo input em `/tmp`, inline e arquivo; veredito + `findings` serializados idênticos; cobre os 4 caminhos que tiram `reais` de coleção (membro de classe larga, membro de classe pequena, kwarg de dataclass, atributo de módulo com 12 nomes) | «a chamada 2 difere da 1ª»: `reais` `["theta","zeta","beta","iota","gamma","mu","alpha","eta"]` × `["lambda_","eta","nu","beta","__init__","mu","iota","epsilon"]` |
| `checks::tests::det_reais_sao_o_corte_de_uma_ordem_total_e_a_sugestao_sai_dela` | valores EXATOS de `reais` e do «Você quis dizer» nos 4 caminhos | `["lambda_","alpha","gamma","eta","mu","__init__","nome","delta"]` em vez de `["alpha","beta","delta","epsilon","eta","gamma","iota","kappa"]` |
| `checks::tests::det_sugestao_desempata_por_distancia_e_depois_pelo_nome_em_toda_permutacao` | `sugestao_proxima("casa", …)` em TODAS as 720 permutações de 6 candidatos (4 empatados a distância 1, um mais longe que vem antes no alfabeto) dá sempre `asa` | 4 respostas diferentes: `[Some("asa"), Some("cada"), Some("casas"), Some("caso")]` |
| `modules::tests::dataclass_init_segue_a_ordem_de_declaracao` | 20 instâncias novas: `init.params == ["zulu","alfa","mike","bravo"]` | `["bravo","mike","zulu","alfa"]` |
| `checks::tests::det_ordem_dos_membros_publicos_primeiro_alfabetica_e_cortada_em_oito` | 50 conjuntos novos → mesma lista; públicos primeiro; corte em 8 | RED por não compilar (a função não existia); GREEN depois |

**RED e GREEN em escala de corpus** (`det_varre.py`: cada id roda K vezes no mesmo binário pelo executor do próprio juiz, saída sem `elapsed_ms` comparada byte a byte):

| binário | corpus | ids | K | ids que divergem |
|---|---|---|---|---|
| `f55ec4b5` (antes) | `corpus_v0d` (738) | 738 | 5 | **7** — exatamente os 7 ids de `motor_ordem_de_reais.out` |
| `f55ec4b5` (antes) | bak, só os 7 ids | 7 | 10 | **7** — 10 ordens distintas de `reais` em cada um; conjuntos distintos de `reais`: `15d469304a03`=10, `2463291faf76`=10, `c876d78f82ee`=9, `cda03737f157`=9, `8b7a3476e024`=6, `34511b9ac100`=5, `0dcf0bab45c5`=1; `sugestao` com 2 e 3 textos em `c876d78f82ee` e `cda03737f157` |
| `70e0051a` (depois) | `corpus_v0d` | 738 | 5 | **0** |
| `70e0051a` (depois) | bak | 765 | 5 | **0** |
| `70e0051a` (depois) | bak, só os 7 ids | 7 | 10 | **0** |
| **`4501e1e5` (final)** | `corpus_v0d` | 738 | 5 | **0** |
| **`4501e1e5` (final)** | bak | 765 | 5 | **0** |
| **`4501e1e5` (final)** | bak, só os 7 ids | 7 | 10 | **0** |

Saída dos 7 ids antes×depois, com os achados completos: `resultados/determinismo_7ids_antes_depois.json`.
Nota: no bak o `carregar()` do juiz devolve 765 ids e o juiz executou 762 chamadas (3 a menos: um filtro do próprio juiz que não investiguei); a varredura cobre os 765.

**Mutantes do determinismo** (`mutantes_det.py`, rodada com os testes limpos antes: `vgp::` 113 verdes; arquivo restaurado e conferido por `cmp`) — **10/10 mortos**:

| id | mutação | testes que a matam |
|---|---|---|
| D1 | `reais_dos_membros` sem ordenação (o pedido: «sem ordenação») | 3 (`det_mesma_entrada…`, `det_ordem_dos_membros…`, `det_reais_sao_o_corte…`) |
| D2 | corta ANTES de ordenar (o «ANTES do corte» da coordenadora) | 3 (os mesmos) |
| D3 | `sugestao_proxima` volta ao `min_by_key` só por distância | 1 (`det_sugestao_desempata…`) |
| D4 | desempate pelo nome ANTES da distância | 1 (`det_sugestao_desempata…`) |
| D5 | sem «públicos primeiro» (alfabética pura) | 2 (`det_ordem_dos_membros…`, `det_reais_sao_o_corte…`) |
| D6 | ordem invertida | 2 |
| D7 | sem o corte de 8 | 2 |
| D8 | `init.params` volta a sair do `HashSet` | 3 (`det_mesma_entrada…`, `det_reais_sao_o_corte…`, `dataclass_init_segue…`) |
| D9 | a ordem de declaração nunca é registrada | 3 (`det_reais_sao_o_corte…`, `dataclass_init_sao_os_campos`, `dataclass_init_segue…`) |
| D10 | `reals_of_module` sem `sort` (atributo de módulo) | 2 (`det_mesma_entrada…`, `det_reais_sao_o_corte…`) |

Observação, não corrigida (fora do pedido): nos ids `c876d78f82ee`/`cda03737f157` a sugestão para um membro de 1 letra é `Você quis dizer `n`?` (qualquer nome de 1 letra está a distância 1 de qualquer outro). Um piso de comprimento mínimo no nome buscado resolveria; é decisão de política.

### 2.10 Pedido 3 da coordenadora (10/10): os `params` do dataclass pela semântica do `dataclasses`

Decisões dela (mensagens de 10/10): ordem dos `reais` APROVADA («públicos primeiro…»; motivo registrado em §2.9 e no comentário de `reais_dos_membros`);
params do dataclass NÃO em ordem alfabética nem de conjunto: ORDEM DE DECLARAÇÃO, pela regra do `dataclasses` — «campos das bases primeiro, seguindo o MRO […]; redeclarado mantém a posição;
`ClassVar` fora, `InitVar` dentro, `kw_only` depois dos posicionais; base não resolvida ⇒ abstém», com testes de herança e de `kw_only`.

**Fonte (Context7, `/python/cpython/v3.13.9`, `Doc/library/dataclasses.rst`)**: (1) herança — «the final field order is determined by insertion order across the MRO»; um campo redeclarado em `C(Base)`
mantém a posição original (`@dataclass class Base: x; y` + `@dataclass class C(Base): z; x` ⇒ `__init__(self, x, y, z)`); (2) `kw_only` — «placing all keyword-only parameters after regular parameters; the relative order among
keyword-only parameters is preserved» (`def __init__(self, x, z, *, y, w, t)`), via `_: KW_ONLY`, `field(kw_only=True)` ou `@dataclass(kw_only=True)`; (3) `InitVar` — parâmetro do `__init__` que não é campo
(`fields()` não o devolve); `ClassVar` — pseudo-campo, nunca parâmetro. Da implementação do CPython (a regra do MRO): a coleta percorre o MRO REVERSO, cada base dataclass contribui seus campos, e a classe vem por último.

**O que mudou** (`modules.rs`, `checks.rs`):
- `modules.rs`: `DcField{name, kw_only, init}`; `ClassInfo.dc_fields` (só nomes ANOTADOS, sem `ClassVar`; `_: KW_ONLY` é sentinela, não campo; `field(kw_only=…, init=…)` lido; redeclarar mantém a 1ª posição),
  `dc_bases` (bases posicionais: `Base`, `pkg.Base`, `Base[int]` ↦ `Base`; `metaclass=…` nunca entra), `bases_opaque` (base que não se nomeia: chamada, `*bases`…), `dc_kw_only` e `dc_init_false` (do decorador);
  `ModuleInfo.file` (a identidade de uma classe no MRO é (arquivo, nome)); o `init.params` de um dataclass SEM `__init__` explícito passa a ser os campos da própria classe, na ordem do `dataclasses`.
- `checks.rs`: `construtor_de` (assinatura contra a qual `C(...)` é checado: `__init__` explícito vale — o dataclass nunca o sobrescreve —; `init=False` ⇒ abstém; senão o gerado), `parametros_do_dataclass`
  (MRO reverso, upsert por nome, posicionais e depois `kw_only`), `mro` (**C3 de verdade**, com identidade (arquivo, nome), então a mesma `A` por duas rotas — direta e reexport — é um nó só) e `resolve_base`
  (builtin/typing ⇒ inofensiva; classe do projeto/venv, seguindo reexport; o resto ⇒ **abstém**). Base irresolvida, dotada de ponto, opaca ou hierarquia inconsistente ⇒ `None` ⇒ nenhum achado de `kwarg`.

**Testes novos: 6 `dc_*` em `checks/tests.rs` (o `dataclass_init_segue_a_ordem_de_declaracao`, de `modules.rs`, é do §2.9 e continua valendo)** — RED no código do `70e0051a` (troca temporária dos 2 arquivos, com o marcador `.TROCA-EM-CURSO`
de 11:13:21 a 11:13:32, restauração conferida por `cmp`), GREEN no final:

| teste | prova | RED no `70e0051a` (valor real) |
|---|---|---|
| `dc_heranca_bases_primeiro_e_campo_redeclarado_mantem_a_posicao` | exemplo da doc: `C(Base)` ⇒ `[x, y, z]`; campos da base são aceitos na filha | `["z", "x"]` (só a própria classe, em ordem de conjunto) |
| `dc_kw_only_vai_depois_dos_posicionais` | exemplo da doc (`KW_ONLY` na base + `field(kw_only=True)` na filha) ⇒ `[x, z, y, w, t]`; `@dataclass(kw_only=True)` + `field(kw_only=False)` ⇒ `[b, a, c]` | `["z","t"]` e `["a","b","c"]` |
| `dc_classvar_fora_initvar_dentro_init_false_fora_e_sem_anotacao_nao_e_campo` | `ClassVar` (nu e `typing.`), `field(init=False)` e atributo sem anotação NÃO são parâmetros; `InitVar` é | nenhum dos 4 era negado (`[]`) |
| `dc_heranca_multipla_segue_o_c3_e_a_mesma_base_por_duas_rotas_e_um_no_so` | diamante `D(B, C)` ⇒ `[a, c, b, d]` (MRO D,B,C,A; reverso A,C,B); a mesma `A` por reexport é um nó; duas classes DISTINTAS `A` ⇒ `[o, a, p]` | `["d"]` |
| `dc_base_irresolvida_ou_opaca_abstem_em_todo_balde` | `E(Fora)` com módulo irresolúvel e `F(fabrica())`: nenhum achado em /tmp, inline e arquivo | `q1`/`q2` negados (falso deny) |
| `dc_generic_e_inofensivo_base_subscrita_resolve_init_explicito_vale_e_init_false_abstem` | `Generic[T]` não declara campos; `Base[int]` é `Base`; `__init__` explícito vale; `init=False` abstém | `x`, `extra` e `j1` negados (falsos denys) |


**Mutantes (rodada final, `mutantes_dc.py`; 119 testes `vgp::` verdes antes (`dc_test1.log`); marcador `.TROCA-EM-CURSO` de 11:13:38 a 11:16:05; o fonte voltou byte a byte, `cmp` rc 0 nos 3 arquivos) — 26/26 mortos, cada um por teste NOMEADO (nenhum «morto» por erro de compilação):**

| id | mutação | testes que a matam |
|---|---|---|
| D1 | `reais_dos_membros` sem ordenação | 3 (`det_mesma_entrada_dez_chamadas_dao_os_mesmos_bytes_nos_tres_baldes`, …) |
| D2 | corta ANTES de ordenar | 3 (`det_mesma_entrada_dez_chamadas_dao_os_mesmos_bytes_nos_tres_baldes`, …) |
| D3 | `sugestao_proxima` volta ao `min_by_key` só por distância | 1 (`det_sugestao_desempata_por_distancia_e_depois_pelo_nome_em_toda_permutacao`) |
| D4 | desempate pelo nome antes da distância | 1 (`det_sugestao_desempata_por_distancia_e_depois_pelo_nome_em_toda_permutacao`) |
| D5 | alfabética pura (sem «públicos primeiro») | 2 (`det_ordem_dos_membros_publicos_primeiro_alfabetica_e_cortada_em_oito`, …) |
| D6 | ordem descendente | 2 (`det_ordem_dos_membros_publicos_primeiro_alfabetica_e_cortada_em_oito`, …) |
| D7 | sem o corte de 8 | 2 (`det_ordem_dos_membros_publicos_primeiro_alfabetica_e_cortada_em_oito`, …) |
| D10 | `reals_of_module` sem `sort` | 2 (`det_mesma_entrada_dez_chamadas_dao_os_mesmos_bytes_nos_tres_baldes`, …) |
| N1 | C3 trocado por DFS de primeira ocorrência (diamante sai `c,a,b,d`) | 1 (`dc_heranca_multipla_segue_o_c3_e_a_mesma_base_por_duas_rotas_e_um_no_so`) |
| N2 | percorre o MRO na ordem direta (a classe antes das bases) | 4 (`dc_generic_e_inofensivo_base_subscrita_resolve_init_explicito_vale_e_init_false_abstem`, …) |
| N3 | campo redeclarado perde a 1ª posição | 1 (`dc_heranca_bases_primeiro_e_campo_redeclarado_mantem_a_posicao`) |
| N4 | `ClassVar` deixa de ser excluído | 1 (`dc_classvar_fora_initvar_dentro_init_false_fora_e_sem_anotacao_nao_e_campo`) |
| N5 | `field(init=False)` ignorado | 1 (`dc_classvar_fora_initvar_dentro_init_false_fora_e_sem_anotacao_nao_e_campo`) |
| N6 | `kw_only` não vai para depois dos posicionais | 1 (`dc_kw_only_vai_depois_dos_posicionais`) |
| N7 | `_: KW_ONLY` ignorado | 1 (`dc_kw_only_vai_depois_dos_posicionais`) |
| N8 | `@dataclass(kw_only=True)` ignorado | 1 (`dc_kw_only_vai_depois_dos_posicionais`) |
| N9 | `field(kw_only=…)` ignorado | 1 (`dc_kw_only_vai_depois_dos_posicionais`) |
| N10 | base irresolvida não abstém | 1 (`dc_base_irresolvida_ou_opaca_abstem_em_todo_balde`) |
| N11 | base opaca (`fabrica()`) não abstém | 1 (`dc_base_irresolvida_ou_opaca_abstem_em_todo_balde`) |
| N12 | só os campos da própria classe (sem bases) | 5 (`dc_base_irresolvida_ou_opaca_abstem_em_todo_balde`, …) |
| N13 | `__init__` explícito do dataclass é sobrescrito pelos campos | 1 (`dc_generic_e_inofensivo_base_subscrita_resolve_init_explicito_vale_e_init_false_abstem`) |
| N14 | `@dataclass(init=False)` não abstém | 1 (`dc_generic_e_inofensivo_base_subscrita_resolve_init_explicito_vale_e_init_false_abstem`) |
| N15 | `Base[int]`/`Generic[T]` tratada como opaca | 1 (`dc_generic_e_inofensivo_base_subscrita_resolve_init_explicito_vale_e_init_false_abstem`) |
| N16 | identidade no MRO só pelo nome (sem o arquivo) | 1 (`dc_heranca_multipla_segue_o_c3_e_a_mesma_base_por_duas_rotas_e_um_no_so`) |
| N17 | `ModuleInfo.file` não preenchido | 1 (`dc_heranca_multipla_segue_o_c3_e_a_mesma_base_por_duas_rotas_e_um_no_so`) |
| N18 | `Generic`/`ABC`/… tratados como base desconhecida | 1 (`dc_generic_e_inofensivo_base_subscrita_resolve_init_explicito_vale_e_init_false_abstem`) |

Os mutantes D8/D9 do §2.9 (`init.params` do `HashSet`, ordem de campo não registrada) deixam de existir no código final: o `field_order` virou `dc_fields`, e a ordem é coberta por N2/N3/N6 e pelo `dataclass_init_segue_a_ordem_de_declaracao`.

## 3. Estado por item do aceite

| # | aceite | estado | evidência |
|---|---|---|---|
| 1 | 3 sondas = testes RED→GREEN nos 2 baldes; 2 controles iguais | **feito** | §2.2; `p0_*` + `namespace::tests`; M0 reproduz o RED; as sondas no binário final dão resposta IDÊNTICA à do `f55ec4b5` (`green_4501e1e5.json`) |
| 2 | mutantes mortos e listados (compostos · sinal dinâmico · severidade) | **feito** (23/23 do P0; 26/26 do determinismo + dataclass) | §2.5, §2.9, §2.10 |
| 3 | juiz v2 e V3, binário congelado com sha, sem regressão por id | **feito, com ressalva de p95 do V3** | §2.6.1: 0 NOVO deny, mesmos perdidos e falsos que `k_*_vivo`/`k_*_ev`, veredito por id idêntico ao do `f55ec4b5`; o p95 do V3 é do pyright sob carga e é medido PAREADO |
| 4 | proibido ler holdout | **respeitado** | não li `corpus_holdout_*` nem `B/diagnostics/holdout*/`, com a ÚNICA exceção autorizada: `motor_ordem_de_reais.out` (ids e contagens). `ls` do diretório pai listou nomes de pastas, sem abrir |
| 5 | fmt, clippy, suítes com 0 falhas | **feito** para `touring-code`, `touring-server` e `touring-hook-handlers`; `touring-hooks` tem 3 falhas **idênticas ao baseline** `88c7cc8b` (fora do P0) | §2.7 |
| — | `os.pathh` explicado | **feito** | §2.3 |
| P2 | `reais` ordenado e sugestão com desempate total; teste de 10 chamadas byte a byte; mutante «sem ordenação» morre | **feito** (3 fontes, não 2) | §2.9 |
| P3 | `params` do dataclass por declaração: bases pelo MRO, redeclarado mantém posição, `ClassVar` fora, `InitVar` dentro, `kw_only` depois, abstém base irresolvida; testes de herança e de `kw_only`; doc oficial citada | **feito** | §2.10 |
| P3.3 | veredito por id idêntico ao do `f55ec4b5` no vivo e no bak, v2 e V3; se mudar, parar | **cumprido**: 0 diferenças em 738/738 (ev v2, V3) e 762/762 (bak v2); bak·V3 2 ids por timeout da camada 2 do `f55`, re-medidos 5/5 iguais | §2.6.1 |
| P4 | marcador `.TROCA-EM-CURSO` em toda troca temporária de fonte | **feito** | 11:13:21–11:13:32 (RED do dataclass) e 11:13:38–11:16:05 (26 mutantes); fonte restaurado e conferido por `cmp`; o marcador foi removido |

## 4. Processos

Nenhum processo meu vivo ao final (jobs `hw-slot` rust/heavy, cargo, juízes, varreduras, mutantes: todos terminaram — conferido por `ps` antes de fechar). Nada de `kill`/`pkill`; `rm -r`/`stash`/`reset` também não. Todo cargo passou por `hw-slot --class rust` e todo juiz/varredura por `hw-slot --`;
esperas longas por temperatura (> 88 °C, até 97 °C) e por jobs de outras sessões (`4f-rotulador-v9`, `r96-v3-ab-e935`, `gates97-*`, `vhtml-*`, `int97-*`). A rede caiu entre ~05:09 e ~09:40 BRT e a sessão foi retomada pela coordenadora: o estado em disco foi conferido ANTES de agir (sha256 dos 6 arquivos = último GREEN, sem marcador pendente, nenhum processo vivo).
Imports de pacotes do venv para o censo/varredura rodaram em subprocesso com `HOME` e `PATH` mínimos, sem rede nem credenciais.
Armadilha que paguei e registro: `cp -p` na RESTAURAÇÃO do fonte preserva o mtime antigo, o cargo reaproveita o build da troca temporária e os testes «novos» falham sobre o código velho — o `dc_red.sh` tinha esse defeito (a 1ª `det_a.sh` reprovou 6 testes por isso; os mutantes não foram afetados porque reescrevem o arquivo com mtime novo). Restaurar copia SEM `-p` (mtime novo).
Lixo que deixei, todos binários congelados SUPERADOS: `~/.claude/touring/vgp/binarios/3552f5d9/` (sem `.so`), `…/4b7578d2/` (sem a varredura de cursor único), `…/f55ec4b5/` (P0 sem determinismo) e `…/70e0051a/` (sem a semântica do dataclass); não removi (`rm -r` é proibido).
O `touring-wt-vgp-attr/target/` (≈1 GB) foi criado por hooks de post-write rodando `cargo check` com o target padrão; é ignorado pelo git.

## 5. Dados com sha

| dado | sha256 / n |
|---|---|
| **binário final** `…/binarios/4501e1e5/touring` | `4501e1e56294a81af7379a7e56929c01344e4b611cbf6e7a7a9129366bc09073` |
| binário `70e0051a` (P0 + determinismo) | `70e0051a0e8879bf8cf31207ad43f7f2ffabbc0cdd1494946cbee1293286632f` |
| binário `f55ec4b5` (só P0) | `f55ec4b5e027055a349bd5fa82a5f9d386ea97a8c8627177a9e8d7967f347117` |
| binário de referência `e935aaf1` | `e935aaf17d9a09d0cdb24010df500fb4e4dab20f4200fa30362258ef8731fd1d` |
| corpus `corpus_v0d` novo (staging com evidência; `sha_tree` 60ebf06bd598fd95 segundo a coordenadora): `entradas.jsonl` · `entradas_inline_neg.jsonl` | `28d93bcba17a7993…` · `0af8c7a8f339ffb5…` (os mesmos dois arquivos que medi como «promovido» em 09/10) |
| corpus anterior `corpus_v0d.bak-2026-10-09` (`sha_tree` b06b89113c869097 segundo a coordenadora): `entradas.jsonl` · `entradas_inline_neg.jsonl` | `7b819af27bccb0db…` · `57574182256f2e9c…` |
| fonte FINAL: `checks.rs` · `modules.rs` · `checks/tests.rs` | `ed8a5319d87dea65…` · `e121c6e0a8f2ee01…` · `c61ff1e34c33fd06…` |
| fonte FINAL inalterado: `namespace.rs` · `namespace/tests.rs` · `mod.rs` | `b4bfbab5cc9b558d…` · `18a21f07b4c516e6…` · `fa142ff96b9fa7e6…` |
| `checks.rs` no instante do congelamento do `4501e1e5` | `7fb2c562e57d188c…` (a diferença para o final é UM comentário, mesmo número de linhas: o rebuild reproduz o binário byte a byte) |
| fonte de `70e0051a`: `checks.rs` · `modules.rs` · `checks/tests.rs` | `48cea2d787bbaf02…` · `74fee42ada31dea2…` · `de675a4a1d31bda7…` |
| fonte de `f55ec4b5`: `modules.rs` · `checks.rs` · `checks/tests.rs` | `ef3e19d4f1b3c2a6…` · `14fff3e0c887992f…` · `68154515abfd338f…` |
| varredura de runtime (P0) | 358 pacotes, 18.013 nomes; censo 358 pacotes + 1.091 submódulos; 159 imports de extensão |
| determinismo | 738 ids (ev) · 765 ids (bak) · 7 ids da evidência; K = 5, 5 e 10 |

Artefatos (scripts + JSONs) em `diagnostics/p0-atributo-2026-10-09/` (`LEIAME.md` lista cada um).

## 6. Decisões e riscos (para a coordenadora validar)

1. **Item 2 do brief × controle `np.arrai`.** Contraditórios para o numpy real (tem os 3 mecanismos). Resolvi pela precisão: fechar o que é literal, abster no resto. O risco é a análise de `__getattr__` fechado estar errada para algum módulo — a defesa é o censo contra o runtime (1.091 submódulos, 1 buraco sem marca) e os mutantes M4–M7/M18.
2. **Item 3 (mesma severidade arquivo×inline).** Implementei como: alvo INCERTO ⇒ abstenção silenciosa idêntica nos 3 baldes. A severidade de alvo FECHADO segue a P2 (np.arrai: deny em tmp/inline, **warn em arquivo do projeto**). Se a intenção era também «terceiro fechado ⇒ deny em todo balde» (P4), é mudança de política em `check_attribute` (`sev(false)`) que **não fiz** — aumentaria denies no balde arquivo e mexe no teto de 1%.
3. **Silêncio, não warn**, para a abstenção. Warn em `Image.LANCZOS` válido seria ruído de 99%; e o juiz conta recall por deny **ou** warn, mas os positivos por id ficaram idênticos com silêncio. Reversível em 1 ponto (`module_has`).
4. **Escopo ampliado** com `.so`/`.pyd` (§2.4): 112/159 `from pkg import <extensão>` negados. Se a coordenadora preferir separar, o conserto são `Resolver::extension_exists` + 1 linha em `module_has` + o teste `p0_submodulo_compilado…`.
5. **Abstenção sobe**: 82/358 pacotes (23%) e 105/1.091 submódulos (9,6%) abstêm (scipy, sklearn, pydantic, networkx, yaml, regex… por `import *`/`__getattr__` preguiçoso/`globals()`). Isso é recall perdido nesses módulos por construção; nos positivos do corpus não houve perda por id.
6. **Superabstenção deliberada**: `sys.modules[...]`/`import_module(<variável>)` contam como «este módulo»; passar `globals()` ou o módulo a QUALQUER chamador fora de uma lista de só-leitura abstém. Só adiciona silêncio, nunca deny.

7. **Ordem dos `reais` de membro: públicos primeiro, `_` depois (aprovada pela coordenadora em 10/10).** Motivo: alfabética pura põe `__init__`/`__repr__` na frente e eles encheriam o corte de 8 — e a recusa «ensina» pelos `reais` (o `juiz_v4_sessoes.py` mede se a chamada seguinte usa um deles). Registrado também no comentário de `reais_dos_membros`.
8. **Só nomes ANOTADOS são campos de dataclass** (a doc: campos = anotações de classe). Antes, todo atributo de classe contava, inclusive `ClassVar` e `x = 1`; agora `V(sem_anotacao=1)` e `V(n=1)` (ClassVar) são negados — correto em runtime (`TypeError`), mas é um endurecimento: vale conferir no held-out se algum dataclass real passa kwarg de atributo não anotado. Nos dois corpora o veredito por id não mudou (os 2 achados de `kwarg`, `1e0916c353ee` e `2463291faf76`, foram mantidos).
9. **Abstenção ampliada no dataclass**: base irresolvida, DOTADA DE PONTO (`m.Base`), opaca (`fabrica()`, `*bases`), hierarquia inconsistente no C3, profundidade > 8, `@dataclass(init=False)` ⇒ nenhum achado de `kwarg`. É recall perdido por construção em dataclass com base que o coletor não enxerga; o ganho é zero falso deny por campo herdado (antes, `C(x=1)` com `x` da base era NEGADO).
10. **`__init__` explícito num dataclass vale** (o `dataclasses` nunca o sobrescreve). Antes, os `params` do `__init__` explícito eram trocados pelos campos.
11. **ACHADO pré-existente, NÃO corrigido (aguardando A/B da coordenadora): falso deny de `kwarg` em classe COMUM sem `__init__` próprio.** `class Base: def __init__(self, a, b=2)` + `class Filha(Base): pass` + `Filha(a=1)` num /tmp ou inline dá `deny kwarg a` (`reais` vazios) — nos binários `e935aaf1` E `70e0051a` —, porque `sig_de` usa o `ci.init` da própria classe (vazio) e não segue a base; só ocorre com classe IMPORTADA de módulo do projeto (a definida no próprio arquivo não é checada). O conserto reaproveita o `mro`/`resolve_base` desta rodada (`__init__` = o primeiro do MRO; base irresolvida ⇒ abstém), mas muda escopo e pode mexer em veredito do corpus; fiz B (só dataclass) e deixei a sonda em `diagnostics/p0-atributo-2026-10-09/scripts/sonda_init/` (README com o comando). É candidato a P próprio e, pelo risco de falso deny no held-out, de prioridade alta.
12. **Sugestão «Você quis dizer `n`?» para membro de 1 letra** (qualquer nome de 1 letra está a distância 1 de outro): ruído conhecido, não tocado (§2.9).

## 7. Git

`git status` do worktree: `M checks.rs`, `M checks/tests.rs`, `M mod.rs`, `M modules.rs`, `?? namespace.rs`, `?? namespace/`. **Sem add/commit/push/stash/reset/checkout.** Branch criada com `git worktree add -b fix/2026-10-09-vgp-atributo-dinamico` sobre `88c7cc8b`.
Árvore principal e os worktrees `touring-wt-vl0`/`-l2`/… não foram tocados. A VL0 deve rebasear sobre esta branch depois do commit da coordenadora. O marcador `.TROCA-EM-CURSO` não existe mais (as duas janelas de troca estão em §3).
Os `baseline_hooks.sh` da rodada de 09/10 usaram `git show` (só leitura) e `mv`, nunca checkout nem stash.

## 8. O que NÃO foi verificado

- **Held-out (20261011 e anteriores)**: é da coordenadora; não li nada (a única leitura de `holdout*/` foi `motor_ordem_de_reais.out`, por exceção dela). O P0M roda com o `f55ec4b5`; o binário final só vale para ele pela condição de veredito por id (§2.6.1), não por medida no held-out.
- **p95 do V3**: o motor de V3 passa pelo pyright (limite de 1000 ms) e o p95 dos runs isolados reprova em TODOS os binários (inclusive a base) com a máquina a 92–97 °C. O p95 PAREADO (3 repetições alternadas entre os binários, mínimo por id, `tempos_dc/agregado.json`) dá `4501e1e5` **920,3 ms** contra `e935aaf1` **957,4 ms** (e 111,7 contra 113,0 ms na camada 1): sem custo mensurável; mas a medida segue sendo de máquina quente, e o «PRONTO» do V3 (p95 ≤ 1000 ms num run só) só se confirma numa máquina fria.
- **Hooks em sombra / daemon com o binário novo**: exercitei `touring vgp check` (CLI), que chama a mesma `check()`; não subi o daemon nem rodei sessão real. As suítes de `touring-hook-handlers` (733) passam; as de `touring-hooks` têm as 3 falhas da §2.7, idênticas ao baseline e fora do P0.
- **As 3 falhas de `touring-hooks`** (`cli_suggester_e2e::classifier_edit_rust_emits_pre_edit_triage_with_tdg_gate`, `wave25_task_enrichment_e2e::{task_create_payload_returns_enriched_context, enter_plan_mode_prompt_is_treated_as_subject}`) continuam abertas NESTE worktree; a coordenadora informou que a 1ª já foi corrigida na árvore principal (30.4.96) e que verificará as duas do `wave25`. Não as editei (fora do P0, sob edição concorrente).
- **Dataclass — limites conhecidos**: (a) `@dataclass_transform`/pydantic/attrs: só decorador cujo texto casa `dataclass` conta; (b) base dotada de ponto (`m.Base`) abstém em vez de resolver; (c) `field(default_factory=…)`, `__post_init__` e `kw_only` por `dataclasses.field` com valor não literal (`kw_only=FLAG`) não são interpretados (valor não literal ⇒ vale o default da classe); (d) `__slots__`/herança de classe COMUM que herda de dataclass (a classe filha não-dataclass) segue a regra antiga (achado 11 da §6); (e) dataclass definido no próprio arquivo checado não é verificado (`sig_de` só olha classe importada, como antes).
- **Pré-existente, medido e NÃO corrigido**: falso deny de `kwarg` em classe comum sem `__init__` próprio (§6, item 11). Aguarda decisão A/B.
- **Não cobertos pelo coletor de namespace** (limitações conhecidas): (a) nomes injetados por **decorador de outro módulo** (`matplotlib.backends.backend_agg`, `@_Backend.export`); (b) *proxy* que troca o módulo por **nome** passado a uma função (`humanfriendly.define_aliases(module_name=__name__)`); (c) `setattr` cujo módulo chega **por parâmetro** (`def _install(mod): setattr(mod, k, v)`); (d) `from x import *` não é resolvido (segue abstenção, como antes); (e) `__all__ += [...]` ainda sobrescreve `dunder_all` no passe de topo (pré-existente; inofensivo porque os nomes vão para `names`).
- **Stdlib sem tabela fechada** (`os`, `sys`, `json`…): continua sem checagem de atributo (§2.3).
- **Outros idiomas** (TS/Rust): fora do escopo.
- **Reprodutibilidade do binário: VERIFICADA** para o final — o rebuild release do mesmo worktree/target (`det_gates.sh`, último passo) deu o MESMO sha256 do congelado `4501e1e5` (`cmp` rc 0), com o fonte final (que difere do fonte do congelamento só por um comentário de mesmo número de linhas).
