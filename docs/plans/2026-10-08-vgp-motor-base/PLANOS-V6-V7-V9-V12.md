# PLANOS — V-6 (VRS), V-7 (VHTML), V-9 (HORUST), V-12 (aceite residual)

> Fila depois do V-8 (SEM2). Cada plano traz a **evidência que o bloqueia**, o **passo
> executável** e o **critério de aceite** — nada aqui é um resumo do relatório: é a sequência
> que fecha o item. A ordem é a da §1 das pendências consolidadas
> (`docs/plans/2026-10-29-pendencias-consolidadas.md`).
>
> Estado no momento da escrita: **V-8 em curso** (a re-rotulação OSS está rodando com a disjunção
> e o teto de 3 % na seleção; ver `RELATORIO-V8-SEM2.md` §5). O V-12 depende do n que o V-8
> entrega — por isso ele é o ÚLTIMO da fila.

---

## V-6 — VRS: aceite do adaptador Rust (P1 · M · depende de V-4, feito)

### O que bloqueia (medido, não suposto)

| # | bloqueio | evidência |
|---|---|---|
| 1 | **Recall `nome` = 5/20 = 0,25** contra o piso de 0,70 (n = 20) | `RELATORIO-VRS.md:78` (§3 item 3) |
| 2 | A causa dominante: **`use super::*` não resolve** — `resolver_caminho` devolve abstenção para `…::super` puro; 204 dos 344 ids naturais acionam isso, ligam o escopo como dinâmico e rebaixam o veredito para `unknown` (57 ids) | `RELATORIO-VRS.md` §6.5 |
| 3 | **O estrato `real` não existe**: `corpus_semeado/rust/` e `~/.claude/touring/vgp/corpus_rust_real` não existem | `RELATORIO-VRS.md` §6.2 |
| 4 | **Deny nunca foi medido para Rust**: nenhuma entrada Rust é inline nem `/tmp`, então `Alvo::Projeto` só vira `warn`. O «0/307 falsos deny» é verdadeiro e **vazio** quanto ao balde onde o adaptador de fato nega | `RELATORIO-VRS.md` §6.3 |
| 5 | **Ruído de `warn` herdado**, 4 causas (a)-(d), 1-15 linhas cada: (a) em `tests/`/`benches/`/`examples/`, `use <pacote>::X` resolve o próprio arquivo de teste; (b) `drop` ausente do prelúdio; (c) itens locais de função (`const`/`fn`/`static` dentro de `fn`) não coletados; (d) `thread_local!`/`lazy_static!` ligam o arquivo inteiro a «dinâmico». **Perguntei à coordenadora; sem resposta até o fim** | `RELATORIO-VRS.md` §6.4 |
| 6 | Contrato do corpus/julgador: `entradas.jsonl` traz `contexto` como **STRING** (`"[]"`) e o juiz faz `for c in entrada.get("contexto")` → `TypeError`; `mensagem` vem sem `error[E…]` e `classe_erro` devolve `nao_classificado` | `RELATORIO-VRS.md` §6.1 |
| 7 | Dívida de qualidade herdada: `extrair_itens` CC = 31, `coleta_usos` CC = 16, `folhas_de_uso` CC = 18, TDG C (0,72) | `RELATORIO-VRS.md` §6.8 |

O que **já está cumprido** e não se repete: RED→GREEN com controles (26/26), 24/24 mutantes
mortos + RED-stub, falso deny 0/307 no natural (limite 0,97 %), p95 128,1 ms, os quatro gates
com rc 0, e o teste de despacho do núcleo respondendo pelo adaptador real.

### Plano executável

1. **`use super::*` → `Encontrado::Modulo` do pai.** Onde: `crates/touring-code/src/vgp/lang/rust.rs`
   (`resolver_caminho`). Como: resolver `super` para o diretório pai do módulo corrente, pulando
   o segmento `super` (o mutante R3 — `super::` como `crate::` — tem de continuar morto);
   RED→GREEN com o teste novo, sem tocar nos 26 verdes; medida por id com o harness que já
   existe (`diagnostics/vrs-porta-2026-10-10/scripts/{medir_ids,comparar_ids}.py`).
   **Efeito esperado**: recall `nome` de 0,25 para ≥ 0,70, sem baixar o recall (que conta `warn`).
2. **As 4 causas de ruído (a)-(d)**, **só com o aval da coordenadora** — cada uma 1-15 linhas,
   com RED→GREEN e medida própria. Não entro nelas sem resposta.
3. **Gerar os estratos que não existem** (item 3): `corpus_rust_real` com código real
   (`cargo check` de verdade sob `hw-slot`) e, **sobretudo**, **negativos Rust no balde de deny**
   (inline / `/tmp` / `.cache`) — sem eles o teto de 1 % é vazio, exatamente como no item 4.
4. **Consertar o contrato corpus/julgador** (item 6): o gerador emite `contexto: []` (lista, não
   string) e `mensagem` com `error[E…]`; **ou** o juiz tolera. Decidir uma, não as duas.
5. **Medir em máquina quieta** — os p95 anteriores vieram com a máquina a 91-97 °C e com
   juízes concorrentes; repetir o p95 antes de alegar desempenho.

### Critério de aceite

- recall `nome` ≥ 0,70 com n ≥ 20 (hoje 0,25);
- falso deny CP95 ≤ 1 % **no estrato selado, com negativos no balde de deny** (hoje 0/307 é
  vazio quanto a esse balde);
- p95 ≤ 150 ms medido em máquina quieta;
- `cargo fmt --all -- --check`, `clippy -p touring-code --all-targets -D warnings`,
  `cargo test -p touring-code --no-fail-fast`, `RUSTDOCFLAGS="-D warnings" cargo doc -p touring-code --no-deps` — rc 0 nos quatro;
- a dívida de CC (item 7) fica **declarada**, ou resolvida em wave própria (não no meio do
  aceite: uma porta que muda a estrutura perde a rastreabilidade por id).

---

## V-7 — VHTML: aceite do adaptador HTML (P1 · M · depende de V-4, feito)

### O que bloqueia

| # | bloqueio | evidência |
|---|---|---|
| 1 | **Falso deny ≤ 1 % não está provado**: n = 200 dá limite 1,49 % | `RELATORIO-VHTML.md:458-469` (§8) |
| 2 | **O patch de CLI non-UTF-8 (§2.10) NÃO está no worktree**: o binário `38181087` ainda tem o defeito — 1 238 HTML Latin-1 do `analise` voltariam como `lang: python` | `RELATORIO-VHTML.md` §8 item 8 |
| 3 | O patch foi testado em **crates descartáveis**, não no `touring-server`; `cargo test -p touring-server` com o patch não rodou, e o binário não o contém | `RELATORIO-VHTML.md` §8 item 2 |
| 4 | **«HTML nunca nega» foi provado em `check`, não por uma sessão que escreva `/tmp/x.html`** | `RELATORIO-VHTML.md` §8 item 5 |
| 5 | População do controle limpo é **viesada** (exportações SEI/Word, `htmlcov` do próprio repositório) | `RELATORIO-VHTML.md` §8 último item |

O que **já está cumprido**: gramática da spec WHATWG lida no fonte do crate, `recurso.rs` e
`script.rs` com CC por função ≤ 15, controles limpos, equivalentes declarados (§2.5).

### Plano executável

1. **Aplicar o patch §2.10** em `crates/touring-server/src/cli/vgp.rs` (no branch do INTADAPT) e
   rodar `cargo test -p touring-server` **com** ele — é o teste que faltava.
2. **Os quatro gates** sob `hw-slot` (`fmt`, `clippy --all-targets -D warnings`, `test --no-fail-fast`,
   `doc` com `-D warnings`), mais o `touring e2e -j`.
3. **A6HTML** — o equivalente do aceite da a6 para Rust: RED→GREEN por classe com controles
   limpos, mutantes mortos listados, e o RED contra o stub (o teste tem de **reprovar** contra o
   núcleo stub, senão prova nada).
4. **JHTML** — o juiz no **held-out selado**, e o held-out tem de conter negativos HTML **no balde
   de deny**; sem isso o ≤ 1 % é vazio como no Rust.
5. **Prova fim a fim de «HTML nunca nega»**: uma sessão real escrevendo `/tmp/x.html` pelo hook
   (não `touring vgp check`).

### Critério de aceite

- falso deny CP95 ≤ 1 % no estrato selado, com n ≥ 299 **no denominador certo** (negativos);
- o binário release contém o patch §2.10 (1 238 HTML Latin-1 voltam como `lang: html`);
- gates rc 0; A6HTML com mutantes listados e mortos; «HTML nunca nega» provado por sessão real.

---

## V-9 — HORUST: held-out Rust (P1 · M · depende de V-6)

### O que bloqueia

| # | bloqueio | evidência |
|---|---|---|
| 1 | **O estrato `cfg` não está selado** — ou o cfg é selado, ou a exclusão fica registrada | §1 da pendência V-9 |
| 2 | O juiz tem de rodar **uma vez** sobre os 184 ids primários + 340 negativos | §1 da pendência V-9 |
| 3 | O selo precisa ser do formato que o **validador atual** aceita (`sha_tree`/`arquivos`/`bytes`, **fora** do corpus) — é o mesmo ajuste que o V-8 teve de fazer no selo do web | `diagnostics/horust-real/selo.json` tem `sha_tree`/`entradas_jsonl_sha256`/`base_check_sha256`, mas **não** `arquivos`/`bytes` |

O selo já tem o esqueleto do pré-registro (`pre_registro_sha256_original`,
`pre_registro_sha256_com_adendos_na_hora`, `adendos_gravado`, `reconfirmacao_sha256`,
`worktree_head`, `worktree_limpo`) — ou seja, **o pré-registro existe** e a disciplina de
«julgar uma vez» está montada.

### Plano executável

1. **Selar o estrato `cfg`** no formato do validador atual (ou registrar a exclusão, com o motivo
   no selo) — `scripts/selar_holdout_web.py` é o molde; o `confere_selo.py` é o conferidor.
2. **Congelar o pré-registro** e declarar os adendos **antes** do julgamento (o `pre_registro` já
   tem hash original e hash com adendos: os dois precisam bater com o que for rodado).
3. **Julgar uma vez**, sob `hw-slot` (classe heavy), com a saída **por id**
   (`ids_*.json`, o padrão já usado no VRS) — para que cada veredito seja rastreável ao id.
4. **Conferir o selo** com `confere_selo.py` e registrar o resultado ao lado do `selo.json`.

### Critério de aceite

- estrato `cfg` selado **ou** exclusão registrada no selo, com o conferidor passando (rc 0);
- pré-registro congelado antes do julgamento, com os adendos declarados;
- julgamento único, saída por id, com o veredito por id e o resumo;
- o veredito declara a situação da prova **sem inflar o n**.

---

## V-12 — Aceite residual VTS/VTSX, VCDN/VCSS, VL0 camada 2 (P2 · L · depende de V-4)

### O que bloqueia

| # | bloqueio | evidência |
|---|---|---|
| 1 | **TS/TSX/JS nunca foram julgados em corpora suficientes** — o juiz exige n ≥ 299 e os negativos semeados JS são **9**; TSX era n = 213 | `RELATORIO-VTS.md` §8 |
| 2 | `Alvo::Externo` via `.d.ts` **não implementado** (desvio declarado; é camada 2) | `RELATORIO-VTS.md` §6.1 |
| 3 | **A rede real não existe na camada 1**: o resolvedor nunca falou com jsDelivr/unpkg/cdnjs — só com um servidor TCP falso. O resolvedor **externo** (urllib) provou que as 53 URLs reais existem, mas **não é o código do repositório** | `RELATORIO-VTRANSV.md` §8 |
| 4 | **CSS-in-JS sem controle real**: nenhum pacote de styled-components/emotion nas árvores permitidas — só Tailwind foi exercitado | `RELATORIO-VTRANSV.md` §8 |
| 5 | `classe_css` em JSX contra projetos grandes **não medido** (o oráculo só aprovou 15 TSX limpos); a R2 por subárvore foi só **simulada em Python** (`r2s.py`), não implementada | `RELATORIO-VTRANSV.md` §8 |
| 6 | VL0 camada 2 (precisão, concorrência, daemon, custo do rust-analyzer) **não medida** sob `hw-slot` | §1 da pendência V-12 |

### Plano executável

1. **O aceite TS/TSX/JS é o que o V-8 entrega.** A camada nova (teto de 3 % na seleção, disjunta
   dos corpora anteriores, selo externo conferido) é o estrato que o juiz vai usar: o `n` por
   classe e a concentração medida do selo do V-8 **são** o critério. Se a classe não atingir
   299 negativos, ela sai como **não provada**, com o n e o motivo — e o aceite de TS/TSX/JS sai
   declarada como não provada, sem inflar o número.
2. **O `.d.ts`/`Alvo::Externo`** é trabalho de camada 2: fica declarado como desvio, com o item
   escrito (não resolvido no meio do aceite).
3. **O juiz web precisa de controles reais**: um controle contra jsDelivr/unpkg/cdnjs que
   exercite **o resolvedor do repositório** (não o urllib externo), e um pacote de CSS-in-JS
   (styled-components/emotion) como controle de abstenção — hoje só Tailwind foi exercitado.
4. **VL0 camada 2 sob `hw-slot`**: precisão, concorrência (várias sessões no daemon), daemon e o
   custo do rust-analyzer; a R2 por subárvore entra **implementada**, não simulada.

### Critério de aceite

- por classe TS/TSX/JS: a situação da prova vinda do selo do V-8, com o n e a concentração
  **medidos** — e `provada` só com n ≥ 299 **negativos** (o denominador do falso deny);
- o `.d.ts` declarado como desvio, com item próprio;
- o controle de CDN exercitando o código do repositório, e o de CSS-in-JS com pacote real;
- a camada 2 medida sob `hw-slot`, com a R2 por subárvore implementada.

### Decisões que são da coordenadora (não minhas)

1. **JS clássico: aviso ou abstração?** O `nome` livre em script sem import/export/require é
   `Alvo::Incerto` (aviso). Tornar abstention zeraria os 11 avisos do controle, mas derrubaria o
   recall de `nome` em JS para ≈ 0,10 (quase todo positivo semeado é script clássico).
   Hoje: recall `nome` JS 20/20, 11 avisos no controle. **A escolha entre recall e ruído é sua.**
2. **As 4 causas de ruído de `warn` do Rust (V-6, item 2)** — perguntado, sem resposta até hoje.
3. **O `html` como estrato OSS** — fica **NÃO PROVADA por decisão** nesta rodada (item D-8 das
   pendências): um adjudicador html sobre `html5-boilerplate`/`dom-examples` é trabalho novo, e
   o escopo não é ampliado sem o Gabriel.
