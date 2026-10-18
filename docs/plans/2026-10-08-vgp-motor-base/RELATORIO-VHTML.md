# RELATÓRIO VHTML — adaptador `html` da camada 1 do motor VGP (classes `recurso` e `ancora`)

> Sessão: subagente `vhtml` (frente B do fan-out dos adaptadores), delegação da coordenadora `team-lead`
> (brief: `briefs/2026-10-10-fanout-adaptadores.md`). Data: 10/10/2026. Nada commitado, nada propagado.
> B = `/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base`. **A entrega é hipótese até o juiz da
> coordenadora no held-out selado** — o que medi aqui é o corpus de desenvolvimento (regenerado com a convenção nova), um
> controle negativo em HTML limpo, dados naturais do `analise` e mutantes.
>
> **Duas rodadas.** A 1.ª (manhã) entregou o adaptador com `deny` em `/tmp`; a coordenadora a **aceitou** e decidiu três
> mudanças: (1) HTML **nunca nega** — `deny` em `/tmp`/`.cache`/inline vira `warn` (§2.2); (2) id duplicado fica `warn`, e os
> «avisos em negativos» saem POR CLASSE (§2.6); (3) o defeito do CLI com não-UTF-8 vira PATCH separado, sem aplicar neste
> worktree (§2.10). Este relatório descreve a **fonte final da 2.ª rodada**; o binário da 1.ª (`ec542c4b`) está superado.

## 1. Identidade

| item | valor |
|---|---|
| worktree | `/home/gabrielgadea/projects/touring-wt-vhtml` · branch `feat/2026-10-10-vgp-vhtml` · base `88c7cc8b` + VL0 (sem commit) |
| escopo | `crates/touring-code/src/vgp/lang/html.rs` + submódulos `html/{documento,recurso,ancora,script,texto}.rs` + testes `html/tests/{mod,recurso,ancora,script}.rs` |
| classes | `recurso` (`src`/`href` local inexistente) · `ancora` (`#id`, `for=`, `aria-labelledby/describedby`, `getElementById`/`querySelector('#x')` inline, id duplicado). CDN/`classe_css` são do `vtransv` |
| builds | `CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/wt-vhtml`; todo cargo e todo juiz por `scripts/hw-slot` (máquina a 84–97 °C quase o tempo todo; `rc 75` uma vez no início, repeti com `--wait 14400`) |
| Context7 | **não esteve disponível para mim** (a ferramenta MCP não é exposta a este subagente; `ToolSearch` inexistente). Consultei no lugar, por leitura direta: `tree-sitter-html 0.23.2` (`src/grammar.json`, `src/scanner.c`, `src/tag.h`, `queries/injections.scm` no registry do cargo) + `node-types.json` upstream (WebFetch); `tree-sitter-javascript 0.25.0` `src/node-types.json`; WHATWG HTML "parse a srcset attribute" (WebFetch). **Não consegui buscar** o trecho WHATWG "find a potential indicated element": as regras `#top`/`#` vazio/`:~:`/id por `name` de `<a>`/percent-decode do fragmento vêm do meu conhecimento da spec — marcadas em §8 |

### 1.1 Núcleo recebido: dois arquivos sincronizados byte a byte com o `touring-wt-vl0`

1. `crates/touring-code/src/vgp/lang/mod.rs`: o que o brief chamava "idêntico ao do `touring-wt-vl0`" era um **snapshot tirado
   no meio de uma edição** (mtime 23:51:13 × 23:51:27): `cargo test -p touring-code --lib` não compilava (E0061 em `mod.rs:400` e
   `:489`, `Falso(achados)` com 1 argumento contra a struct de 2). Copiei do `touring-wt-vl0` (sha256 `c95d707b2ffd…`, igual nos
   dois). A coordenadora confirmou que o erro foi dela.
2. `crates/touring-server/src/cli/vgp.rs`: a coordenadora apontou o segundo arquivo de núcleo velho (o meu era `ea73f0d9c19f`; o da
   VL0, `b1e5878cbb08`). Copiei do `touring-wt-vl0` e conferi: **`b1e5878cbb08` nos dois**. Continua assim: o conserto do CLI
   (§2.10) é um patch à parte, **não aplicado** aqui.

Nenhuma outra linha de núcleo foi tocada. No `git status` do meu worktree os dois aparecem junto com o resto da VL0 (ainda não
commitada): a integração pega só `html.rs` + `html/`.

## 2. Feito, com evidência

### 2.1 Desenho

```
html.rs            Html (Adaptador) · Saida{achados, motivos} (1 achado por (classe,nome); motivos viram `nao_medido`) · teto de 4 MiB
html/documento.rs  a árvore lida UMA vez (iterativo): elementos+atributos (nomes em minúsculas, entidades decodificadas),
                   corpos de <script>, ids (vivos × dentro de <template>), eh_pagina, tem_erro
html/recurso.rs    Disco (resolução como o navegador: diretório do arquivo ou <base> local; ?query/#frag fora; %XX decodificado;
                   raiz-relativo contra ancestrais+raízes web+caminho absoluto; overlays) · "você quis dizer" com cache de listagem
html/ancora.rs     referências (a/area/use href=#x · label for · aria-labelledby/describedby) × ids existentes · duplicados
html/script.rs     <script> inline relido com a gramática `javascript`: lookups literais + ids que o script PODE criar
html/texto.rs      entidades, %XX, marcadores de template, varredura lexical de `id=`, distância de edição
```

O adaptador **mede**; severidade, delta e veredito são do `check` (contrato do núcleo). **Todo achado é `Alvo::Incerto`**, logo
`warn` no máximo, em qualquer caminho (§2.2):

| situação | `Alvo` | severidade (política §5) |
|---|---|---|
| relativo inexistente · raiz-relativo (`/css/a.css`) sem arquivo em nenhuma raiz plausível | `Incerto` | `warn` |
| `#id`/`for`/`aria` ausente · lookup `getElementById`/`querySelector` ausente | `Incerto` | `warn` (a sugestão ganha a nota «um script da página pode criá-lo» quando a página tem `<script src>` ou script dinâmico/ilegível) |
| id duplicado | `Incerto` | `warn` (lint) |
| não deu para medir | — | `nao_medido` com o motivo (sem achado = veredito `unknown`, nunca `allow`) |

Abstenções declaradas (cada uma com teste e com mutante «abstenção vira allow», §2.4): valor de template (`{{ }}`, `{% %}`,
`<%= %>`, `<?php`, `${ }`, `#{ }`) · `<base href>` externo · caminho virtual (`<string>`) · página que herda/inclui
(`{% extends %}`, `{% include %}`, `<?php include`, `<!--#include`…) · **fragmento HTML** (sem `<html>`/`<head>`/`<body>`/doctype:
os ids podem estar na página hospedeira) · ids gerados por template · script que cria id/marcação por dado dinâmico (`el.id = n`,
`setAttribute('id', x)`, `innerHTML = x`, `insertAdjacentHTML`, `document.write`) ou ilegível ou com `type` desconhecido · script
acima de 512 KiB (2 MiB no total) · documento acima de 4 MiB · árvore com `MISSING` ou `ERROR` que engoliu `<`.

### 2.2 Decisão da coordenadora: HTML nunca nega (2.ª rodada)

**Antes**: `recurso` relativo ausente e `#id` ausente eram `Alvo::Projeto`, e a política §5 (P2) os negava onde o código «roda
agora» (`/tmp/`, `.cache/` da raiz, inline). **Agora**: `Alvo::Incerto` nos dois sítios, `warn` em todo caminho. Razão (a
coordenadora, conferida por mim na spec): VL0 §5.1 manda `deny` só onde «nenhuma edição seguinte pode tornar o código certo **e**
a precisão medida é alta», e para «código que roda agora … **nas classes de símbolo**»; `recurso` e `ancora` não são classe de
símbolo — o arquivo/id ausente é o que a próxima escrita cria (o `index.html` sai antes do `style.css`), o §5.2 pede `warn` para
«símbolo do PROJETO ainda inexistente». O único `deny` de HTML que a spec deixa é o `cdn` 404 confirmado, que é do `vtransv`.
O núcleo não mudou: a mudança é só de qual `Alvo` o adaptador declara (era a «1 linha por sítio» de §6 da 1.ª rodada).

TDD (vermelho → verde), `red2.log` → `green4.log`: os testes de política foram reescritos para a expectativa nova **antes** do
código. Contra o código antigo: **11 falharam, 59 passaram** (ex.: `ancora_nunca_nega_em_html_nem_onde_o_codigo_roda_agora`,
`recurso_nunca_nega_em_html_nem_onde_o_codigo_roda_agora`, `recurso_relativo_inexistente_e_achado_com_linha_e_nunca_nega`,
`ancora_script_externo_ou_dinamico_vira_nota_na_sugestao`). Depois da mudança: **70 passaram, 0 falharam**. Efeitos colaterais, todos
cobertos por teste: a nota «um script da página pode criá-lo» (antes era `Projeto` × `Incerto` na âncora) passou a viver na
sugestão e alimenta `tem_script_externo`/`scripts.dinamico`, que continuam em uso; o teste do delta (`aplicar_delta` só reescreve
`deny`) virou `recurso_ja_ausente_no_disco_continua_warn_e_sem_prefixo_de_delta`. Mutantes: os quatro «DENY EM /tmp» (R14, R15, A7,
A20) morrem — §2.4.

### 2.3 Convenção do `nome` de `recurso` (coordenadora, 10/10) — implementada e congelada em teste

**`nome` = o token exatamente como escrito no atributo, com o prefixo de caminho (`../../`, `./`), sem `?query` e sem
`#fragmento`.** Ex.: `src="../../shared/foo.png?v=2#x"` → `nome` `../../shared/foo.png`. A existência no disco é testada **sobre esse
token** (decodificado de `%XX` e com `\` → `/` só para resolver; o `nome` emitido mantém a barra invertida como escrita).

| propriedade | teste que congela |
|---|---|
| prefixo de caminho, query e fragmento no mesmo token; `./x` e `/x#f` são `Incerto`; `\` preservada no nome | `recurso_nome_e_o_token_como_escrito_sem_query_e_sem_fragmento` |
| query e fragmento fora do `nome`, mas o arquivo continua obrigatório | `recurso_ignora_query_e_fragmento_mas_nao_o_arquivo` |
| relativo ao arquivo, **não** à raiz do projeto | `recurso_resolve_relativo_ao_arquivo_e_nao_a_raiz_do_projeto` |
| raiz-relativo × relativo: ancestrais + raízes web, erra como `Incerto` | `recurso_raiz_relativo_procura_ancestrais_e_raizes_web_e_erra_como_incerto` |
| `#x` sozinho não é recurso; `#{x}` (interpolação) é dinâmico, não fragmento | `recurso_fragmento_de_template_nao_e_recurso_dinamico` (mutantes R23 e R24) |

### 2.4 TDD da 1.ª rodada: vermelho no stub → verde

`Html::verificar` foi trocado temporariamente pelo corpo do stub (`nao_implementado("html")`) e a suíte `vgp::lang::html`
rodou: **49 falharam, 10 passaram** (`red.log`). Os 10 verdes contra o stub: 7 testes unitários dos auxiliares puros
(entidades, `%XX`, marcador de template, `id=` lexical, distância, srcset, esquema) e 3 **controles negativos** que o stub
satisfaz por vacuidade — esses três só morrem por mutante, não por teste vermelho. Depois do corpo real: 65 verdes; na retomada
acrescentei 4 testes (o de `ancora` mata o mutante A18 e o de `script` o H3, ambos sobreviventes da rodada anterior;
`recurso_fragmento_de_template_nao_e_recurso_dinamico` mata o R23; `recurso_nome_e_o_token_…` congela a convenção); a 2.ª rodada
somou 1 (`ancora_script_externo_ou_dinamico_vira_nota_na_sugestao`): **a suíte `vgp::lang::html` tem 70 testes, 70 passaram**.
Controles limpos por classe: o recurso existe; o id existe por `<a name>`, comentário, `<template>` ou criado por script; URL
externa/`data:`/`mailto:` fora de escopo; tudo atrás de construção dinâmica sai `nao_medido` com o motivo.

### 2.5 Mutantes: **83 de 83 mortos** (0 sobreviventes, 0 inválidos)

Runner `scratchpad/vhtml/mutantes.py`: aplica UMA substituição textual por vez, roda `cargo test -p touring-code --lib
vgp::lang::html --no-fail-fast` e exige `test result: FAILED`; restaura **por conteúdo** (sha conferido) e faz `os.utime` para o
cargo não tratar o build do mutante como fresco. Rodou sobre a **fonte final da 2.ª rodada**, em 6 lotes, cada um com sua aquisição
de vaga; depois do último lote os 10 arquivos de fonte têm o mesmo sha de antes (§5). Resultado: `mutantes_out3.json`
(`{"MORTO": 83}`). Cobertura do exigido: **≥ 1 mutante por regra de resolução** (R1–R24: raiz, `%XX`, query/fragmento, esquema,
rel, `srcset`, template, `<base>`, overlay, absoluto, raízes web, ancestrais, relativo × raiz-relativo) e **seis mutantes
«abstenção vira allow»** (R20, A18, A19, H1, H2, H4 — o adaptador deixa de dizer o que não mediu). **Os quatro «deny em /tmp»
(a pedido da coordenadora) — R14, R15, A7, A20 — trocam o `Alvo::Incerto` por `Alvo::Projeto` num sítio cada e morrem** pelos testes
`*_nunca_nega_em_html_nem_onde_o_codigo_roda_agora`. Os mutantes de «script externo/dinâmico tira o deny» (A8) viraram quatro
sobre a nota da sugestão (A8, A21, A22, A23).

**resolução de `recurso` (`html/recurso.rs`)** — 24 mutantes, 24 mortos

| id | o que muda | mortos por |
|---|---|---|
| R1 | resolve contra a raiz do projeto em vez do diretório do arquivo | `recurso_nome_e_o_token_como_escrito_sem_query_e_sem_fragmento`, `recurso_resolve_relativo_ao_arquivo_e_nao_a_raiz_do_projeto` |
| R2 | sem decodificar %XX | `recurso_decodifica_percentual_e_entidades` |
| R3 | query e fragmento ficam no nome | `recurso_ignora_query_e_fragmento_mas_nao_o_arquivo`, `recurso_nome_e_o_token_como_escrito_sem_query_e_sem_fragmento` |
| R4 | esquema (http:, data:, mailto:…) deixa de ser ignorado | `recurso_fora_de_escopo_nao_vira_achado_nem_nao_medido`, `recurso_link_so_conta_quando_o_alvo_parece_arquivo` (+1) |
| R23 | fragmento #x conta como referência de recurso | `recurso_fragmento_de_template_nao_e_recurso_dinamico` |
| R24 | #{x} (interpolação Pug/Ruby) é tratado como fragmento | `recurso_dinamico_ou_de_template_e_nao_medido_com_o_motivo` |
| R5 | <a href> de rota deixa de ser ignorado | `recurso_link_so_conta_quando_o_alvo_parece_arquivo` |
| R6 | <link> de qualquer rel conta como arquivo | `recurso_link_so_conta_para_rel_de_arquivo` |
| R7 | srcset deixa de ser lido | `recurso_cobre_as_tags_que_carregam_arquivo` |
| R8 | valor de template é resolvido como caminho | `recurso_dinamico_ou_de_template_e_nao_medido_com_o_motivo` |
| R9 | <base href> externo é ignorado | `recurso_base_externa_nao_resolve_e_base_local_muda_o_diretorio` |
| R10 | overlay do instante ignorado | `recurso_conhece_o_overlay_do_instante` |
| R11 | caminho absoluto do sistema de arquivos não é tentado | `recurso_absoluto_do_sistema_de_arquivos_conta` |
| R12 | raízes web (public/, static/…) não são tentadas | `recurso_raiz_relativo_procura_ancestrais_e_raizes_web_e_erra_como_incerto` |
| R22 | atalho do primeiro segmento invertido | `recurso_absoluto_do_sistema_de_arquivos_conta`, `recurso_raiz_relativo_procura_ancestrais_e_raizes_web_e_erra_como_incerto` |
| R13 | só o diretório do arquivo é raiz candidata | `recurso_raiz_relativo_procura_ancestrais_e_raizes_web_e_erra_como_incerto` |
| R14 | DENY EM /tmp: raiz-relativo ausente vira Projeto | `recurso_nome_e_o_token_como_escrito_sem_query_e_sem_fragmento`, `recurso_nunca_nega_em_html_nem_onde_o_codigo_roda_agora` (+1) |
| R15 | DENY EM /tmp: relativo ausente vira Projeto | `recurso_ja_ausente_no_disco_continua_warn_e_sem_prefixo_de_delta`, `recurso_nome_e_o_token_como_escrito_sem_query_e_sem_fragmento` (+2) |
| R16 | <base href> local não muda o diretório | `recurso_base_externa_nao_resolve_e_base_local_muda_o_diretorio` |
| R17 | <a href> deixa de ser lido | `recurso_link_so_conta_quando_o_alvo_parece_arquivo` |
| R18 | srcset: URL terminada em vírgula engole o próximo candidato | `srcset_separa_candidatos_sem_quebrar_data_url` |
| R19 | qualquer prefixo antes de ':' vira esquema | `esquema_so_com_letra_inicial_e_dois_pontos` |
| R20 | ABSTENÇÃO vira allow: referência de template some sem motivo | `recurso_dinamico_ou_de_template_e_nao_medido_com_o_motivo` |
| R21 | contagem de referências repetidas some | `recurso_repetido_vira_um_achado_na_primeira_linha` |

**`ancora` (`html/ancora.rs`)** — 23 mutantes, 23 mortos

| id | o que muda | mortos por |
|---|---|---|
| A1 | <a name> não é alvo de fragmento | `ancora_aceita_a_name_e_conta_id_com_caixa_exata` |
| A2 | varredura lexical de id= desligada | `ancora_id_so_em_comentario_ou_template_conta_como_existente` |
| A3 | ids criados por script não contam como existentes | `script_id_criado_pelo_proprio_script_nao_e_ausente` |
| A4 | #top deixa de ser navegação especial | `ancora_navegacao_especial_nao_e_id` |
| A5 | aria-labelledby não é dividido em ids | `ancora_label_for_e_aria_checam_cada_id_da_lista` |
| A6 | id duplicado não é detectado | `ancora_id_duplicado_e_incerto_e_ignora_template`, `ancora_nunca_nega_em_html_nem_onde_o_codigo_roda_agora` |
| A7 | DENY EM /tmp: id duplicado vira Projeto | `ancora_id_duplicado_e_incerto_e_ignora_template`, `ancora_nunca_nega_em_html_nem_onde_o_codigo_roda_agora` |
| A20 | DENY EM /tmp: id ausente vira Projeto | `ancora_href_para_id_ausente_e_achado_e_presente_nao`, `ancora_nunca_nega_em_html_nem_onde_o_codigo_roda_agora` |
| A8 | a nota de script que pode criar o id some | `ancora_script_externo_ou_dinamico_vira_nota_na_sugestao`, `script_com_erro_de_sintaxe_e_nao_medido` (+3) |
| A21 | script EXTERNO deixa de gerar a nota | `ancora_script_externo_ou_dinamico_vira_nota_na_sugestao` |
| A22 | script DINÂMICO deixa de gerar a nota | `script_com_erro_de_sintaxe_e_nao_medido`, `script_com_src_ignora_o_corpo_e_tipo_desconhecido_e_dinamico` (+2) |
| A23 | a nota de script aparece quando NÃO há script | `ancora_script_externo_ou_dinamico_vira_nota_na_sugestao`, `script_com_erro_de_sintaxe_e_nao_medido` (+4) |
| A9 | fragmento HTML é medido como página | `ancora_fragmento_sem_pagina_nao_e_medido_mas_pagina_com_body_e` |
| A10 | template com extends/include é medido | `ancora_pagina_que_herda_ou_inclui_nao_e_medida` |
| A11 | fragmento %XX não é decodificado | `ancora_fragmento_percentual_casa_com_id_decodificado` |
| A12 | <base href> não tira o direito de julgar href=#x | `ancora_base_href_tira_o_direito_de_julgar_href_de_fragmento` |
| A13 | referência com template é tratada como id | `ancora_ids_de_template_e_referencia_de_template_nao_sao_medidos`, `ancora_label_for_de_template_e_nao_medido_e_o_resto_continua` (+1) |
| A14 | ids gerados por template são medidos | `ancora_ids_de_template_e_referencia_de_template_nao_sao_medidos` |
| A15 | contagem de referências repetidas some | `ancora_repetida_vira_um_achado` |
| A16 | <label for> deixa de ser lido | `ancora_base_href_tira_o_direito_de_julgar_href_de_fragmento`, `ancora_label_for_de_template_e_nao_medido_e_o_resto_continua` (+2) |
| A17 | <use href> do SVG deixa de ser lido | `ancora_use_do_svg_aponta_para_id` |
| A18 | ABSTENÇÃO vira allow: âncora de template some sem motivo | `ancora_ids_de_template_e_referencia_de_template_nao_sao_medidos`, `ancora_label_for_de_template_e_nao_medido_e_o_resto_continua` (+1) |
| A19 | ABSTENÇÃO vira allow: página não mensurável perde o motivo | `ancora_fragmento_sem_pagina_nao_e_medido_mas_pagina_com_body_e`, `ancora_ids_de_template_e_referencia_de_template_nao_sao_medidos` (+1) |

**scripts inline (`html/script.rs`)** — 15 mutantes, 15 mortos

| id | o que muda | mortos por |
|---|---|---|
| S1 | lista de seletores (#a, #b) exige os dois ids | `script_query_selector_checa_so_ids_obrigatorios_do_seletor` |
| S2 | id dentro de :not(#x) é exigido | `script_query_selector_checa_so_ids_obrigatorios_do_seletor` |
| S3 | qualquer receptor vale como document | `script_so_olha_document_e_literais` |
| S4a | setAttribute('id', dado) não vira dinâmico | `script_cria_id_por_dado_dinamico_desliga_lookups_e_avisa_na_ancora` |
| S4b | insertAdjacentHTML(dado) não vira dinâmico | `script_cria_id_por_dado_dinamico_desliga_lookups_e_avisa_na_ancora` |
| S4c | document.write(dado) não vira dinâmico | `script_cria_id_por_dado_dinamico_desliga_lookups_e_avisa_na_ancora` |
| S4d | el.id = dado / innerHTML = dado não vira dinâmico | `script_cria_id_por_dado_dinamico_desliga_lookups_e_avisa_na_ancora` |
| S5 | o argumento do lookup conta como id criado | `script_get_element_by_id_ausente_e_incerto_e_presente_nao`, `script_linha_do_lookup_e_absoluta_no_documento` (+2) |
| S7 | script com erro de sintaxe é lido assim mesmo | `script_com_erro_de_sintaxe_e_nao_medido` |
| S8 | linha do lookup relativa ao script, não ao documento | `script_linha_do_lookup_e_absoluta_no_documento` |
| S9 | corpo de <script src> é lido | `script_com_src_ignora_o_corpo_e_tipo_desconhecido_e_dinamico` |
| S10 | type desconhecido não é dinâmico | `script_com_src_ignora_o_corpo_e_tipo_desconhecido_e_dinamico` |
| S11 | literal com escape/interpolação é adivinhado | `script_so_olha_document_e_literais` |
| S14 | script enorme é relido em vez de recusado com motivo | `script_acima_do_teto_nao_e_relido_e_avisa_na_ancora` |
| S13 | window.document não é document | `script_so_olha_document_e_literais` |

**leitura do documento (`html/documento.rs`)** — 9 mutantes, 9 mortos

| id | o que muda | mortos por |
|---|---|---|
| D1 | corpo de <script> nunca é lido | `script_com_erro_de_sintaxe_e_nao_medido`, `script_com_src_ignora_o_corpo_e_tipo_desconhecido_e_dinamico` (+7) |
| D2 | <html>/<body> não tornam o documento uma página | `ancora_fragmento_sem_pagina_nao_e_medido_mas_pagina_com_body_e` |
| D3 | ids dentro de <template> entram na checagem de duplicados | `ancora_id_duplicado_e_incerto_e_ignora_template` |
| D9 | qualquer ERROR (inclusive > solto no texto) conta como erro de sintaxe | `maior_que_solto_no_texto_nao_e_erro_de_sintaxe` |
| D4 | <template id> conta como dentro do próprio template | `ancora_id_duplicado_e_incerto_e_ignora_template` |
| D5 | entidades de caractere não são decodificadas | `recurso_decodifica_percentual_e_entidades` |
| D6 | nome de tag sensível a caixa | `recurso_ignora_a_caixa_de_tag_e_de_atributo` |
| D7 | nome de atributo sensível a caixa | `recurso_ignora_a_caixa_de_tag_e_de_atributo` |
| D8 | corpo de <style> é lido como script | `script_o_corpo_de_style_nao_e_javascript` |

**auxiliares de texto (`html/texto.rs`)** — 4 mutantes, 4 mortos

| id | o que muda | mortos por |
|---|---|---|
| T1 | &amp; decodificado errado | `recurso_decodifica_percentual_e_entidades`, `entidades_numericas_nomeadas_e_desconhecidas` |
| T2 | data-id=/valid= viram id= | `ids_no_texto_le_atributo_id_e_so_ele` |
| T3 | sugestão só com distância 0 | `ancora_sugere_o_id_parecido`, `recurso_ensina_o_nome_proximo_inclusive_na_caixa` (+1) |
| T4 | sugestão ignora diferença só de caixa | `recurso_ensina_o_nome_proximo_inclusive_na_caixa` |

**adaptador e abstenções (`html.rs`)** — 8 mutantes, 8 mortos

| id | o que muda | mortos por |
|---|---|---|
| H1 | ABSTENÇÃO vira allow: o adaptador nunca diz o que não mediu | `ancora_fragmento_sem_pagina_nao_e_medido_mas_pagina_com_body_e`, `ancora_ids_de_template_e_referencia_de_template_nao_sao_medidos` (+11) |
| H2 | ABSTENÇÃO vira allow: erro de sintaxe do HTML some | `erro_de_sintaxe_do_html_e_dito_e_nao_vira_allow_calado`, `tag_nao_fechada_tambem_e_dita` |
| H3 | achados repetidos por (classe, nome) | `script_mesmo_id_procurado_duas_vezes_e_no_href_vira_um_achado` |
| H4 | ABSTENÇÃO vira allow: script ilegível some sem motivo | `script_com_erro_de_sintaxe_e_nao_medido`, `script_acima_do_teto_nao_e_relido_e_avisa_na_ancora` |
| H5 | lookups de DOM nunca são verificados | `script_cria_id_por_dado_dinamico_desliga_lookups_e_avisa_na_ancora`, `script_get_element_by_id_ausente_e_incerto_e_presente_nao` (+3) |
| H6 | âncoras nunca são verificadas | `ancora_aceita_a_name_e_conta_id_com_caixa_exata`, `ancora_base_href_tira_o_direito_de_julgar_href_de_fragmento` (+21) |
| H8 | documento enorme é parseado em vez de recusado com motivo | `documento_acima_do_teto_nao_e_lido_e_diz_por_que` |
| H7 | recursos nunca são verificados | `documento_grande_nao_tem_custo_quadratico`, `recurso_base_externa_nao_resolve_e_base_local_muda_o_diretorio` (+18) |

**Equivalentes declarados** (não contam nos 83; mutantes que não podem morrer porque o código é redundante por desenho): a faixa de
colchetes e a de aspas em `posicoes_de_topo` se protegem mutuamente; o `ids_em_template` em `ids_presentes` é coberto pela varredura
lexical; o corpo de `application/json` também (a varredura lexical só vê `id=`).

### 2.6 Juiz (`scripts/juiz_motor_vgp.py`, sha `6c8a31b02075da1e`, binário congelado `38181087`)

**Corpus medido: `~/.claude/touring/vgp/corpus_semeado/html/` REGENERADO pela 4f com a convenção nova** (`entradas.jsonl` mtime
10/10 10:09:16, 300 entradas = 100 positivos + 200 negativos; `entradas.jsonl` sha256 `bf643ba6da16bee2…`, **igual ao que a
coordenadora conferiu**; `sha_tree` **`1b83ba7392dd8778`** pelo método de `scripts/confere_selo.py::sha_tree`, 602 arquivos,
20 337 649 bytes — **a coordenadora confirmou que é o canônico**; o `4178277f` da 4f é outro método). `cdn`/`cdn_pin`/`classe_css`
são do `vtransv` e ficam fora das minhas contas. Na regeneração `cdn_pin` foi de 16 para 20 e os ids de `recurso` mudaram (derivam
do nome): a comparação por id com o corpus antigo só vale para as outras classes.

| classe | n | detectados | recall | observação |
|---|---|---|---|---|
| `recurso` — medida EXATA (igualdade do `nome` do achado com `nome_esperado`) | 20 | 20 | **1,00** | |
| `ancora` | 20 | 19 | **0,95** | o perdido (`5d5e89436807`) é um **fragmento HTML** sem `<html>`/`<body>`: abstenção deliberada (`unknown`, motivo no JSON) |

- **Falso deny: 0/200** (balde `arquivo`), limite superior Clopper-Pearson 95 % = **1,49 %** (n = 200 não prova ≤ 1 % nem com 0
  falsos; o juiz exige n ≥ 299 — o teto de 1 % é a prova do held-out, não minha). Agora é estrutural: o adaptador de HTML não
  emite nada que a política possa negar.
- **Avisos em negativos, POR CLASSE** (os 200 negativos: 196 `allow` · 4 `warn` · 0 `unknown`; o juiz imprime só o total, 4; o
  detalhe é de `negativos.py`):

  | classe | achado | negativos com `warn` |
  |---|---|---|
  | `ancora` | **id duplicado** (lint) | **4** — o mesmo `_com_1` de 2 exportações de Word, cada uma em 2 cópias (`[131]-44864586_Resolucao_000` linhas 348/1337; `[129]-44640517_Minuta_de_Resolucao` linhas 357/1348): duplicata real, não falso |
  | `ancora` | id ausente | 0 |
  | `recurso` | arquivo ausente | 0 |

- **p95: 26,0 ms** (juiz, 300 chamadas, máquina a 93–96 °C com outros juízes concorrentes); 23,7 ms na primeira rodada sem
  concorrência. ≪ 150 ms.
- O veredito do juiz é "NÃO PRONTO" **só** porque os 60 positivos de `cdn`/`cdn_pin`/`classe_css` não são meus (recall agregado
  39/100) e porque n = 200 não fecha o limite — nada em `ancora`/`recurso`.
- A cópia derivada de corpus que usei antes da regeneração (symlinks para `snap/` do original) **deixou de valer** quando o
  original foi trocado (a mesma cópia deu 19/96): descartada; nenhum número dela está neste relatório.

### 2.7 CONTROLE NEGATIVO EM HTML LIMPO (emenda de aceite da coordenadora)

**Seleção** (`scratchpad/vhtml/limpos.py`, sha12 `8f6b47aaa0b2`): script próprio que só abre o disco e usa `html.parser` — **não
importa nem chama o adaptador**. Percorre `~/projects/analise` e `~/projects/touring` (exclui `touring-wt-*`, `target`,
`node_modules`, `.git`, `.venv`, `.vgp-semeado`, `vendor`, `.cache`, `dist`, `build`; **nunca** `oss/` nem `corpus_holdout_*`) e só
aceita o `.html`/`.htm` que seja UTF-8, < 4 MiB, sem `<base>`, sem marcador de template em atributo, cujos `src`/`href`/`poster`/
`data` **relativos ao arquivo existem no disco** (raiz-relativo desqualifica: onde a página é servida não se decide pelo disco) e
cujos `#fragmento`, `label for` e `aria-labelledby/describedby` **resolvem** a um `id` ou `<a name>` do próprio documento.

| | n |
|---|---|
| `.html`/`.htm` varridos | 6 609 (4 969 aceitos + 1 640 descartados) |
| descartados | não UTF-8 1 238 · raiz-relativo 245 · recurso ausente 55 · âncora ausente 38 · `<base>` 33 · grande 18 · template 13 |
| **manifesto** (`limpos_manifesto.json`, sha256 `6f5c34d9db9e3655…`, `sha_manifesto` `9d1e4bc03bcb67be`) | **4 969** (analise 4 908 · touring 61) |
| estratos | ids únicos 4 444 · sem referências 213 · ids duplicados 312 |

**Medida** com o binário final `38181087`, cada arquivo **no lugar** (`--path` = `--content-file` = o arquivo real, sem
`--disk-file`, para todo achado contar como novo e as checagens de existência baterem na árvore real) — `aplica_limpos.py` sha12
`d39bb60bbd14`. Conferi antes que o manifesto não envelheceu: **0 de 4 969 arquivos mudaram ou sumiram** desde a seleção.

| resultado | valor |
|---|---|
| achados de `recurso` (arquivo inexistente) | **0** em 4 969 |
| achados de `ancora` por id AUSENTE | **0** em 4 969 |
| achados de `ancora` por id DUPLICADO (lint, `Incerto`, `warn`) | 955 em 313 arquivos (os 312 do estrato «duplicados» + 1 arquivo «sem referências» que também repete ids) |
| arquivos sem nenhum achado | **4 656 de 4 969** (4 444 + 212) — **todos os arquivos sem id duplicado** |
| veredito | `allow` 4 581 · `warn` 313 · `unknown` 75 (**nenhum `deny`**) |
| latência por processo | p95 82,2 · máx 749,5 ms (máquina a 94–96 °C com 2–3 jobs de outros agentes); na rodada da 1.ª versão, mais fria, p95 74,1 e máx 430,4 |

**Conferência dos 313 «restos» por oráculo independente** (`cruza_limpos.py`, sha12 `79c9c53e0652`): para CADA um dos 4 969
arquivos o `html.parser` conta os ids repetidos (fora de `<template>`) e o conjunto tem de ser **igual** ao dos `nome` dos achados do
adaptador. Resultado: **4 969 de 4 969 conjuntos iguais, 0 divergências, 0 ids só no adaptador, 0 ids só no oráculo.** A causa de
cada resto é, portanto, a mesma e verdadeira: o arquivo declara o mesmo `id` em 2+ elementos (principalmente as exportações de
Word/SEI `_com_N` e `lnkSei…`). Não é falso positivo — é o lint de duplicata que o brief pediu, **mantido como `warn`/`Incerto` por
decisão da coordenadora** («defeito real que ensina»); é a fonte dominante de `warn` em HTML limpo (313 de 4 969 = 6,3 %), e por
isso os «avisos em negativos» saem por classe (§2.6).

**Os 75 `unknown`** (1,5 %) têm causa dita no `motivo` — o adaptador não decide e diz por quê: HTML com `MISSING`/`ERROR` que
engoliu marcação 26 · script inline acima do teto de leitura (2,6–3,4 MiB, `htmlcov`/mapas) 23 · script cria id/marcação por dado
dinâmico 19 · script inline com erro de sintaxe 5 · fragmento HTML sem página 2. Nenhum virou `allow` calado.

### 2.8 Dados naturais: 5 316 HTML UTF-8 reais do `analise` (não é held-out; a árvore cresce: eram 5 273 na 1.ª sonda)

Sem rótulo, então cada achado foi confrontado com um **oráculo independente** em Python (`html.parser` + `os.path.exists`), com o
binário final:

| | achados | oráculo confirma | refuta | sem opinião |
|---|---|---|---|---|
| `recurso` relativo | 491 | 491 | **0** | — |
| `recurso` raiz-relativo (`Incerto`) | 2 910 | — | — | 2 910 (a raiz web não é conhecível) |
| `ancora` ausente | 352 | 352 (7 com o id aparecendo num `<script>`) | **0** | — |
| `ancora` duplicada | 1 820 | 1 820 | **0** | — |

Veredito: 4 545 `allow` · 635 `warn` · 136 `unknown` (2,6 %) · **0 `deny`**. p50 10,5 ms · p95 69,7 ms · p99 143,5 ms · máx 496 ms
(um `htmlcov` de 1,7 MB; máquina a 93–96 °C com outros jobs). **Três achados da 1.ª sonda mudaram o código** (todos
regressão-testados):

1. **Cauda de 4 s**: HTML de 42 MB levou 4,2 s e um mapa de 26 MB, 3,9 s (o parse do `javascript` do script inline dominava) →
   tetos de 4 MiB por documento, 512 KiB por script e 2 MiB de script por documento, com motivo.
2. **47 % dos documentos UTF-8 saíam `unknown`** por "html com erro de sintaxe": o diagnóstico mostrou que **o 1.º erro de 1 797 dos
   2 467 documentos com erro de árvore é um `>` solto no texto** (`->`, `>=`), que a gramática exclui do token `text` e marca
   `ERROR` sem perder nenhum elemento. Passou a contar só `MISSING` e `ERROR` que contém `<` (marcação engolida): `unknown` caiu de
   2 481 para ~130.
3. **Listagem de diretório por achado** (`read_dir` repetido) → cache por diretório; **raiz-relativo**: atalho pelo 1.º segmento
   em vez de ~130 `stat` por URL.

### 2.9 Robustez

600 mutações de documentos reais (semente fixa; remover/inserir `< > " ' = & {{ <!-- \0 😀 %E0%A4%A …`, truncar, enxertar pedaços de
outros) pelo binário final `38181087`: **0 saídas ≠ 0, 0 JSON inválido, 0 timeouts**; p50 8,7 ms · p95 18,7 ms · máx 61,1 ms
(tempo de processo). Fail-open confirmado como propriedade, não como intenção.

### 2.10 Defeito do CLI com não-UTF-8 — PATCH SEPARADO, não aplicado (REGRA #21)

**Defeito** (achado da 1.ª rodada, no `cli/vgp.rs` da VL0 `b1e5878cbb08`): `touring vgp check --content-file` com arquivo que não é
UTF-8 voltava `"lang":"python"` + um achado `erro` (`warn`) + `unknown` — o caminho de erro do CLI fixava `python`, e o `warn` era
contado como aviso num arquivo limpo (1 238 dos 6 609 HTML varridos são Latin-1).

**Patch**: `diagnostics/vhtml/cli-nao-utf8.patch` (sha12 `2e245078127d`, `diff -u` contra o arquivo da VL0, 4 trechos, prefixos
`a/` `b/` → `patch -p1` na raiz do repo). Feito **sem tocar nenhum worktree**: o arquivo corrigido foi compilado em dois crates
descartáveis do scratchpad que dependem por caminho de `touring-code` e `touring-pyright` (`CARGO_TARGET_DIR` próprio,
`wt-vhtml-cli`), e o `cli/vgp.rs` do meu worktree continua `b1e5878cbb08`.
- **Conserto**: `parse_args` devolve `Recusa{motivo, lang, nao_utf8}` em vez de `String`; a linguagem sai do pedido (`--lang`,
  senão a extensão do `--path`) **antes** de ler qualquer conteúdo, e toda recusa a leva; arquivo que existe mas não é texto
  (`io::ErrorKind::InvalidData`) vira `unknown` com motivo **«conteúdo não é UTF-8: content-file …»**, **sem** achado `erro`;
  `--disk-file` e `--overlay` não-UTF-8 idem; os erros de argumento (arquivo ausente, flag sem valor) mantêm o achado `erro`, mas
  agora com a `lang` do pedido. `parse_args` ficou com CC 26 (era 28).
- **Teste** (3 novos em `vgp::tests`): `conteudo_nao_utf8_e_unknown_com_motivo_e_a_lang_do_pedido` (`.html`→`html`, `.py`→`python`,
  `.tsx`→`tsx`), `disk_file_e_overlay_nao_utf8_tambem_sao_unknown_sem_achado`,
  `recusa_de_argumento_leva_a_lang_do_pedido_e_mantem_o_achado_erro`. **VERMELHO** contra o arquivo da VL0: 10 passaram, **3
  falharam** (`left: "python"`, `right: "html"`). **VERDE** com o patch: **13 passaram, 0 falharam**. `rustfmt --edition 2024
  --check` limpo no arquivo corrigido; o patch aplica limpo (`patch -p1 --dry-run`) e reproduz o arquivo corrigido (sha12
  `f766304c5212`).
- **O caminho do hook (`String`) precisa de algo?** Não para o `content`: ele chega como `String` do payload JSON (UTF-8 por
  construção). O `disk` o hook lê com `std::fs::read_to_string(file_path).ok()` (`pre_write.rs:1359` na VL0): um arquivo de disco
  não-UTF-8 vira `None`, ou seja, **sem linha de base de delta** — o lado seguro para HTML (que não nega mais) e um caso raríssimo
  para Python (sobrescrever um `.py` Latin-1 faria `deny` pré-existente contar como novo). Não proponho mudar o hook; registro.
- Quem aplica é a integração (INT97), junto com o resto de `cli/vgp.rs`.

### 2.11 Gates (fonte final; todos por `hw-slot --class rust`; `fase2.log`)

| gate | resultado |
|---|---|
| `cargo fmt --all -- --check` | rc 0 (a 1.ª tentativa da 2.ª rodada reprovou 3 trechos meus; `rustfmt --edition 2024` nos arquivos tocados, e tudo foi re-rodado) |
| `cargo clippy -p touring-code --all-targets -- -D warnings` | rc 0 |
| `RUSTDOCFLAGS="-D warnings" cargo doc -p touring-code --no-deps` | rc 0 |
| `cargo test -p touring-code --lib vgp::lang::html` | **70 passaram, 0 falharam** |
| `cargo test -p touring-code --no-fail-fast` | **25 binários · 1 215 passaram · 0 falharam · 5 ignorados** (os do `vgp::` da VL0, inclusive o teste de despacho, continuam verdes) |
| `cargo build --release -p touring-server --bin touring` | rc 0; `touring 30.4.95`; congelado em `~/.claude/touring/vgp/binarios/38181087/touring` |
| CLI patch (crates descartáveis) | vermelho 3 falhas / verde 13 passaram (§2.10) |

## 3. Estado por item do aceite

| # | item | estado | evidência |
|---|---|---|---|
| 1 | TDD por classe; controles limpos | **ok** | §2.2 e §2.4: vermelho no stub (49/59) e vermelho da política (11/70) → 70 verdes; convenção do `nome` congelada (§2.3) |
| 2 | Mutantes mortos e listados; ≥ 1 por regra de resolução e 1 «abstenção vira allow»; «deny em /tmp» tem de morrer | **ok** | §2.5: **83/83 mortos**, 0 sobreviventes; R1–R24 resolução; R20/A18/A19/H1/H2/H4 abstenção vira allow; R14/R15/A7/A20 deny em /tmp; restauração por conteúdo + `os.utime`, sha conferido |
| 3 | Juiz: recall ≥ 0,70 por classe com n ≥ 20; falso deny 0; p95 ≤ 150 ms | **ok** | corpus regenerado `1b83ba7392dd8778`: `recurso` 20/20 = 1,00 e `ancora` 19/20 = 0,95; falso deny 0/200, CP95 1,49 %; avisos em negativos 4 (todos `ancora`/duplicado); p95 26,0 ms |
| 3b | (emenda) controle negativo em HTML limpo, alvo 0 | **ok com um resto explicado** | §2.7: 4 969 arquivos; 0 achados de `recurso` e de id ausente; 955 de id duplicado em 313 arquivos, confirmados 4 969/4 969 pelo oráculo; 75 `unknown` com causa |
| 3c | (decisão) HTML nunca nega; avisos em negativos por classe | **ok** | §2.2 e §2.6 |
| 3d | (decisão) patch do CLI não-UTF-8 | **ok** | §2.10: `diagnostics/vhtml/cli-nao-utf8.patch` |
| 4 | fmt · clippy · test · doc | **ok** | §2.11 |
| 5 | Teste do despacho do núcleo verde; `html` deixa de dizer "ainda não implementado" | **ok** | `html_deixa_de_responder_nao_implementado` + a suíte `vgp::lang` da VL0 (1 215 verdes no pacote) |

## 4. Processos

Nenhum processo meu ficou vivo (conferido ao final com `ps` nos nomes dos meus scripts). Sem `kill`/`pkill` em processo touring,
sem `update-touring`/`propagate`/`touring update`, sem commit/push/stash/reset, sem `cargo clean`, sem tocar em
`~/.claude/settings.json`; **não li** `corpus_holdout_*` nem `oss/`. Duas notas de higiene, declaradas: (1) um `bash` meu (um laço
`until ! pgrep -f …` que esperava um build antigo) nunca terminava porque o `pgrep -f` casava com a própria linha de comando do
laço; identifiquei o PID por `cmdline` + `comm=bash` (nada de touring) e encerrei só esse; (2) tentei `rm -rf` num **diretório
temporário meu** do scratchpad e o hook de segurança **bloqueou** (nada foi apagado); troquei por nomes únicos de diretório. Cada
rodada pesada (fase 2, cada lote de mutantes, juiz, sondas, controle limpo, crates descartáveis do CLI) foi **uma** aquisição de
slot, sem contornar a espera.

## 5. Dados com sha

| artefato | caminho / sha |
|---|---|
| binário congelado (final) | `~/.claude/touring/vgp/binarios/38181087/touring` + `touring.sha256` · sha256 `38181087c669eac0cc7878cb8d938795e1e1b30de539e2ba89ce0c72eee35f4c` (`touring 30.4.95`, `cargo build --release -p touring-server --bin touring`, target `wt-vhtml`, `cli/vgp.rs` = o da VL0). O `ec542c4b` e o `8dcf0b3e` das rodadas anteriores estão **superados** |
| juiz | `scripts/juiz_motor_vgp.py` sha `6c8a31b02075da1e` (só leitura) |
| corpus | `~/.claude/touring/vgp/corpus_semeado/html/` regenerado: `sha_tree` `1b83ba7392dd8778` (método `confere_selo.py`, canônico segundo a coordenadora), 602 arquivos, 20 337 649 bytes, `entradas.jsonl` sha256 `bf643ba6da16bee2…` (300 linhas, mtime 10/10 10:09:16) |
| controle limpo | `scratchpad/vhtml/limpos_manifesto.json` (sha256 `6f5c34d9db9e3655…`, `sha_manifesto` `9d1e4bc03bcb67be`, 4 969 itens com sha256 por arquivo) · `limpos_resultado2.json` · `limpos_cruza2.json` |
| fonte final (sha12 · linhas) | `html.rs` `09acf508f88b` · 113 — `html/documento.rs` `66c5e49263b6` · 232 — `html/recurso.rs` `92285574bcfe` · 552 — `html/ancora.rs` `5bd729a3300c` · 301 — `html/script.rs` `24c465067cb5` · 411 — `html/texto.rs` `c10b04602217` · 294 — `html/tests/mod.rs` `ab77beb29848` · 230 — `html/tests/recurso.rs` `5458db37586f` · 412 — `html/tests/ancora.rs` `076c804e0ace` · 266 — `html/tests/script.rs` `54924280861d` · 197. Núcleo sincronizado (não é meu): `lang/mod.rs` `c95d707b2ffd`, `cli/vgp.rs` `b1e5878cbb08` |
| patch do CLI | `diagnostics/vhtml/cli-nao-utf8.patch` sha12 `2e245078127d` · arquivo corrigido `f766304c5212` · contra `b1e5878cbb08` |
| saídas | `scratchpad/vhtml/{red2.log,green4.log,fase2.log,juiz4_orig.json,negativos4.log,limpos_*2.json,natural5.log,fuzz3.log,mutantes_out3.json,tabela_mutantes3.md,cli/teste_{red,green}.log}` (scratchpad da sessão — os números relevantes estão copiados neste relatório) |
| scripts de apoio | `scratchpad/vhtml/{mutantes.py,gera_mutantes.py,gera_lotes3.py,limpos.py,aplica_limpos.py,cruza_limpos.py,natural.py,negativos.py,fuzz.py,final.sh,cli/{monta_variantes.py,testa.sh}}` |

## 6. Decisões e riscos

1. **`nome` do achado de `recurso` = a referência como escrita, sem query/fragmento** (chave natural do delta), alinhado ao
   gerador e ao corpus regenerado (§2.3). Medido: 20/20 exato.
2. **HTML nunca nega** (decisão da coordenadora, executada em §2.2): `recurso` e `ancora` são `Alvo::Incerto`, `warn` em todo
   caminho. Risco aceito: uma página de pré-visualização em `/tmp/` que referencia um arquivo que ninguém vai criar agora só leva
   `warn`. O único `deny` de HTML é o `cdn` 404 (do `vtransv`).
3. **Fragmento HTML = abstenção** (custou 1 de 20 positivos de `ancora`). Sem ela, parciais incluídos por outra página gerariam
   `#id` "ausente" falsos.
4. **Id duplicado = lint (`Incerto`, `warn`)** — mantido por decisão da coordenadora. 84 % dos achados de `ancora` nos dados
   naturais (1 820 de 2 172) e **todos** os 955 do controle limpo; sempre verdadeiros (oráculo 4 969/4 969). Os «avisos em
   negativos» do juiz agora vêm separados por classe (§2.6) para o número ser legível.
5. **A varredura lexical de `id=` superestima os ids que existem** (conta o que está em comentário, `<textarea>`, string de
   script): o lado seguro de "ausente", ao custo de um falso-negativo raro (id só num comentário).
6. **Tetos de tamanho** (4 MiB / 512 KiB / 2 MiB): um HTML gigante vira `unknown` com motivo, não `allow` (23 dos 75 `unknown` do
   controle limpo são scripts de `htmlcov`/mapas de 2,6–3,4 MiB).
7. **Equivalentes declarados**: ver §2.5 (três; nenhum entre os 83).
8. **CLI com não-UTF-8**: virou o patch de §2.10 (REGRA #21), a ser aplicado pela integração; **não está** no meu worktree, então
   o binário `38181087` ainda tem o defeito (1 238 HTML Latin-1 do `analise` continuariam voltando `lang: python`).
9. `recurso.rs` tem 552 linhas (≈ 420 de produção + testes unitários no fim); `script.rs` 411. Acima de 400 por arquivo mas com CC
   por função ≤ 15.
10. **Cópia derivada de corpus por symlink envelhece em silêncio**: a minha, usada antes da regeneração, passou a dar 19/96 quando o
    original foi trocado. Corrigido por descarte (§2.6); a lição foi para a memória.

## 7. Git

Worktree `touring-wt-vhtml`, branch `feat/2026-10-10-vgp-vhtml`, HEAD `88c7cc8b` (intacto), **sem commit**. Arquivos meus (todos
não rastreados): `crates/touring-code/src/vgp/lang/html.rs` e `crates/touring-code/src/vgp/lang/html/**`. Arquivos de núcleo cujo
conteúdo veio do `touring-wt-vl0` por cópia byte a byte (§1.1): `crates/touring-code/src/vgp/lang/mod.rs` e
`crates/touring-server/src/cli/vgp.rs` (**sem** o patch do §2.10). O resto do `git status` (renomeações de `checks.rs`,
`politica.rs`, `lang/{cdn,css,…}.rs` dos outros adaptadores, hooks) é a VL0 aplicada pela coordenadora — não toquei. Fora do
worktree: `docs/plans/2026-10-08-vgp-motor-base/RELATORIO-VHTML.md` e `diagnostics/vhtml/cli-nao-utf8.patch`.

## 8. O que NÃO foi verificado

- **O held-out selado** (é o juiz final da coordenadora); o teto de falso deny ≤ 1 % **não está provado** (n = 200 → limite 1,49 %).
- **O patch do CLI foi testado em crates descartáveis, não dentro do `touring-server`**: o arquivo é o mesmo (`#[path]`) e só
  depende de `touring-code`/`touring-pyright`/`anyhow`/`serde_json`, mas o `cargo test -p touring-server` com o patch aplicado
  não rodou; a integração o roda. Também não exercitei o patch pelo binário `touring` (o `38181087` não o contém).
- **Regras da spec WHATWG não lidas nesta sessão**: `#top` (ASCII case-insensitive), fragmento vazio, `:~:`, `<a name>` como alvo,
  decodificação percentual do fragmento. A leitura de `srcset` foi confirmada no texto da spec.
- **Context7** (indisponível para mim): a gramática foi lida no código-fonte do crate, o que é mais forte para nós/campos, mas não é a
  documentação.
- **Camada 2, hooks e daemon**: nada foi exercitado com `.html` numa sessão real; só `touring vgp check` e testes de função. Em
  particular, «HTML nunca nega» foi provado em `check`, não por uma sessão que escreva `/tmp/x.html`.
- **Resolução de raiz-relativo** é uma heurística (ancestrais + `public/`, `static/`, `dist/`…): servidores com alias/rewrite não são
  modelados — por isso `Incerto`. O controle limpo EXCLUI esses arquivos (245) justamente porque o disco não decide.
- `getElementById`/`querySelector` só com **literal** e só em `document.` / `window.document.`; jQuery, `onclick="…"`, `srcdoc` e
  `url(#id)` de SVG ficam de fora.
- **HTML não UTF-8**: o adaptador nunca o vê pelo CLI enquanto o patch de §2.10 não entrar; o controle limpo não o cobre.
- **O controle limpo é viesado para o que um humano escreveu e publicou** (principalmente exportações SEI/Word e `htmlcov` do
  `analise`; só 61 arquivos do `touring`): não prova nada sobre HTML gerado por frameworks com template, que o adaptador abstém por
  desenho. O «alvo 0» vale para o que o disco decide.
- Estimativa de precisão em dados naturais é contra um oráculo escrito por mim (`html.parser`), não contra rótulos humanos — o mesmo
  vale para o cruzamento do controle limpo (mesma família de parser que o seletor, embora independente do adaptador).
- Latências medidas com a máquina a 84–97 °C e 2–3 jobs de outros agentes: o p95 de 26–82 ms e os máximos de 430–750 ms são
  pessimistas; a conclusão (≪ 150 ms no p95 do juiz) tem folga, mas o máximo de um arquivo de 1,7 MB não foi remedido a frio.
