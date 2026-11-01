---
type: Spec
title: VL0 — contrato multi-linguagem do motor VGP
description: O contrato único que Python, Rust, TS, TSX, JS, HTML e CSS implementam — despacho por linguagem, vocabulário de classes, política de severidade, camada 2 genérica, corpus por linguagem e o aceite de cada fase.
plan_id: 2026-10-08-vgp-motor-base
timestamp: 2026-10-09T17:40:00-03:00
tags: ["#kind:plan", "#status:active", "#domain:harness"]
---

# VL0 — contrato multi-linguagem do motor VGP

Ordem de Gabriel (09/10/2026): «preciso que ele funcione para as principais linguagens (python, rust, ts, html), pelo
menos» e «confira as versões de CDN e as classes CSS referenciadas; e também inclua tsx». Medições e sondas na
[proposta §10](proposta.md). Esta spec fixa o que TODO adaptador cumpre, para que as fases VRS · VTS · VTSX · VHTML ·
VCDN · VCSS rodem em paralelo sem renegociar a interface.

## 1. O que já existe e não muda

`touring_code::vgp::check(&CheckInput) -> CheckOutput` (`crates/touring-code/src/vgp/mod.rs`) continua a única
entrada; `Finding`, `CheckInput`, `CheckOutput` mantêm os campos (o juiz e os hooks leem esse JSON). Hoje o despacho é
`if input.lang == "python"`, e qualquer outra linguagem devolve `allow` vazio — **isso muda**: linguagem sem adaptador
devolve `unknown` com o motivo, porque `allow` afirma uma verificação que não houve (ausência ≠ zero).

## 2. Vocabulário de linguagem (fonte única)

| `lang` | extensões | gramática tree-sitter |
|---|---|---|
| `python` | `.py` `.pyi` | `tree_sitter_python` |
| `rust` | `.rs` | `tree_sitter_rust` |
| `typescript` | `.ts` `.mts` `.cts` | `tree_sitter_typescript::LANGUAGE_TYPESCRIPT` |
| `tsx` | `.tsx` | `tree_sitter_typescript::LANGUAGE_TSX` |
| `javascript` | `.js` `.mjs` `.cjs` `.jsx` | `tree_sitter_javascript` (JSX incluso) |
| `html` | `.html` `.htm` | `tree_sitter_html` (`<script>`/`<style>` reinjetados nos adaptadores JS/CSS) |
| `css` | `.css` | `tree_sitter_css` |

`pub fn lang_do_caminho(path: &str) -> Option<&'static str>` é a fonte única — hooks, CLI (`--lang` explícito vence) e
juiz derivam dela. Hoje `Lang::TypeScript` usa `LANGUAGE_TSX` para tudo (`ast/languages.rs:87`), o que quebra a
asserção `<T>expr` em `.ts`: a VTS separa as duas gramáticas.

## 3. O adaptador

```rust
/// One language's layer-1 checks. Pure: no daemon, no network, no process spawn (layer 2 lives elsewhere).
pub trait Adaptador: Sync {
    /// The `lang` value of §2 this adapter answers for.
    fn lang(&self) -> &'static str;
    /// (verdict, findings) for the PROPOSED content; delta against `input.disk` is applied by `check`, not here.
    fn verificar(&self, input: &CheckInput) -> (String, Vec<Finding>);
}
```

`check` faz, para toda linguagem e na mesma ordem: (1) escolhe o adaptador por `input.lang`; (2) parse falhou →
`unknown`; (3) roda o adaptador no proposto **e** no disco; (4) aplica o delta — achado presente no disco nunca vira
deny (o desenho do `python_pre_gate.py`); (5) aplica a política §5; (6) mede `elapsed_ms`. O delta e a política saem
do adaptador Python de hoje para o `check`, uma vez, e valem para todos.

> **Emenda 10/10/2026 (implementação do núcleo)** — o §3 acima é o desenho; o código real
> (`crates/touring-code/src/vgp/lang/mod.rs`, `vgp/politica.rs`, `vgp/mod.rs`; relatório
> `RELATORIO-VL0-nucleo.md`) diverge em três pontos, aceitos pela coordenadora. **Onde o §3 e esta emenda divergem, vale a emenda.**
>
> ```rust
> /// O que um adaptador reporta sobre UM conteúdo: os achados que provou e — quando parte do conteúdo
> /// (ou ele todo) ficou fora do alcance — por quê. Sem achados, `nao_medido` é o veredito `unknown`;
> /// com achados, eles valem e o motivo viaja ao lado. O adaptador NÃO dá veredito nem severidade.
> pub struct Verificacao {
>     pub achados: Vec<Finding>,        // construídos com `Finding::candidato(classe, nome, linha, Alvo, sugestao, reais)`
>     pub nao_medido: Option<String>,   // parse falhou, folha não resolvida, CDN fora do ar…
> }
> impl Verificacao {
>     pub fn medido(achados: Vec<Finding>) -> Self;
>     pub fn nao_medido(motivo: impl Into<String>) -> Self;
>     pub fn nao_implementado(lang: &str) -> Self;   // "adaptador <lang> ainda não implementado"
> }
>
> /// Os `lang` do §2 que o adaptador responde. Puro: sem daemon, rede ou processo (camada 2 é outra).
> pub trait Adaptador: Sync {
>     fn lang(&self) -> &'static str;
>     /// Mede `input.content`. O delta contra `input.disk` é do `check` (que chama isto de novo sobre o disco).
>     fn verificar(&self, input: &CheckInput) -> Verificacao;
> }
>
> /// Verificação que ATRAVESSA linguagens (o pin de CDN de um html e de um `import 'https://…'`; a classe CSS
> /// de html e de JSX): roda depois do adaptador de cada `lang` de `langs()`, sobre o mesmo conteúdo, e passa
> /// pela MESMA política. FORMA DECIDIDA (coordenadora, 10/10): devolve `Verificacao`, como o adaptador —
> /// «ausência ≠ zero» vale também para cdn e classe_css. `vgp::medir` junta o `nao_medido` de cada transversal
> /// ao do adaptador (motivos separados por "; ", na ordem do registro). Sem achados, o veredito é `unknown`
> /// com todos os motivos; com achados, eles valem e os motivos viajam ao lado.
> pub trait Transversal: Sync {
>     fn nome(&self) -> &'static str;
>     fn langs(&self) -> &'static [&'static str];
>     fn verificar(&self, input: &CheckInput) -> Verificacao;   // era `-> Vec<Finding>` no worktree do núcleo
> }
>
> static REGISTRO: &[&dyn Adaptador] = &[
>     &python::Python, &rust::Rust, &typescript::TypeScript, &tsx::Tsx,
>     &javascript::JavaScript, &html::Html, &css::Css,              // UMA linha por lang; um arquivo por adaptador
> ];
> static TRANSVERSAIS: &[&dyn Transversal] = &[&cdn::Cdn, &classe_css::ClasseCss];
> ```
>
> 1. **O adaptador mede, o `check` decide.** `Finding` ganhou `alvo: Alvo` (`Externo` · `Projeto` · `Incerto`;
>    só em processo, fora do JSON) e `Finding::candidato`; a severidade que um adaptador escreva é **sobrescrita** por
>    `politica::aplicar` (a tabela do §5 como função pura). `Finding::new(.., severidade, ..)` segue para o que já vem
>    decidido (camada 2, erros de CLI). `CheckOutput` ganhou `motivo` (por que `unknown`); a linha da sombra ganhou `lang`.
> 2. **Todo `lang` do §2 tem módulo, linha de registro e stub** (`unknown`, "adaptador <lang> ainda não implementado"):
>    escrever o adaptador de Rust é trocar o corpo de `lang/rust.rs` e mais nada. Linguagem fora do registro → `unknown`
>    com motivo, nunca `allow`.
> 3. **Gramáticas**: `lang::gramatica(lang)` e `lang::parse(lang, fonte)` (parser por thread, via `parse_bounded`)
>    centralizam o §2 — `"typescript"` e `"tsx"` são duas gramáticas; os adaptadores TS/TSX **não** usam
>    `ast::Lang::TypeScript`. `lang_do_caminho` lê a tabela `EXTENSOES` (14 extensões).
> 4. **Estado da implementação do `Transversal`**: o worktree do núcleo (`touring-wt-vl0`, b7f12000) ainda tem
>    `verificar -> Vec<Finding>` e `vgp::medir` não junta motivos de transversais; a forma acima é implementada pelo
>    worktree `vtransv` e entra na integração (INT97). O stub `cdn.rs`/`classe_css.rs` migra junto.

## 4. Vocabulário de classes (`Finding.classe`)

| classe | o que é | linguagens |
|---|---|---|
| `nome` | identificador usado sem definição em nenhum escopo | todas as de código |
| `import` | nome importado que o módulo não exporta / módulo que não existe | py · rs · ts · tsx · js |
| `atributo` | membro inexistente de módulo, tipo ou valor de tipo conhecido | py · rs · ts · tsx · js |
| `kwarg` | argumento nomeado / campo de literal que a assinatura ou o tipo não tem | py (kwarg) · rs (campo de struct literal) · ts/tsx (propriedade de objeto literal tipado) |
| `componente` | `<Comp>` JSX não importado nem definido | tsx · jsx |
| `prop` | prop que o tipo `Props` do componente do projeto não tem | tsx |
| `recurso` | `src`/`href` local que não existe no disco | html |
| `ancora` | `#id` · `for=` · `aria-*` · `getElementById`/`querySelector('#x')` sem o id no documento; id duplicado | html (e o JS dentro dele) |
| `cdn` | versão inexistente (404), arquivo inexistente na versão, `integrity` que não bate | html · js/ts (`import` de URL) |
| `cdn_pin` | versão não exata (range, `latest`, `/gh/` sem tag, sem versão) — com a versão exata resolvida | html · js/ts |
| `classe_css` | classe usada que nenhuma folha carregada define (com «você quis dizer») | html · tsx · jsx |

Classe nova exige linha nesta tabela antes do código.

## 5. Política de severidade (P4 generalizada)

1. **deny** só onde nenhuma edição seguinte pode tornar o código certo **e** a precisão medida é alta:
   código que roda agora (inline, `/tmp`, `.cache` do projeto) nas classes de símbolo · alvo EXTERNO certo em arquivo
   do projeto (stdlib, crate/pacote de terceiro instalado, builtin) · `cdn` com 404 confirmado.
2. **warn** que ensina no resto: símbolo do PROJETO ainda inexistente (o RED do TDD, o consumidor antes do provedor),
   `cdn_pin`, `classe_css` (classe-gancho de JS é legítima), receptor de tipo desconhecido.
3. **unknown** quando não dá para medir: parse falhou, rede fora ou estourou o orçamento, folha CSS não resolvida,
   classes geradas (Tailwind/UnoCSS), `className` dinâmico. Nunca deny.
4. **Delta** (`--disk-file`): mora no `check` — `julgar` → `politica::aplicar_delta` — e vale para TODA linguagem com
   adaptador (um adaptador novo nasce com ele). Só `deny` é rebaixado a `warn` («pré-existente»), pela chave
   (`classe`, `nome`) do achado no disco; um `warn` nunca é tocado e o que o disco não tinha segue negado.

## 6. Camada 2 genérica

O pool persistente do `touring-pyright` (trava por processo `habilitar_neste_processo()`, sessões por projeto,
backoff, prazo) vira genérico sobre um `Servidor` — `pyright-langserver` (LSP), `rust-analyzer` (LSP, só diagnósticos
nativos, `checkOnSave` desligado), `tsserver` do `node_modules/typescript` do PROJETO (protocolo tsserver) ou
`deno lsp` em projeto deno — sem nenhuma dependência `touring-*` (a regra que dissolveu o ciclo de pacote). O
verificador é **descoberto no projeto, nunca instalado** pelo Touring; ausente → `camada2: "unknown: <motivo>"` com
a camada 1 de pé. A fusão segue a do Python: por (nome, linha), a mais forte vence.

## 7. Corpus e juiz por linguagem

- Um diretório por linguagem com o esquema que `scripts/juiz_motor_vgp.py` já lê: `entradas.jsonl` (`id`, `rotulo`
  positivo|negativo, `reproduz`, `classe`, `nome_esperado`, `origem`, `lang`, `path`, `project_root`, `snap`,
  `mensagem`, `contexto`) + `snap/` + `estado_no_instante/manifesto.json` (`disk_file` para o delta).
- Erro natural quando existe (Rust: 84 erros rustc reais); **corpus semeado** quando não (TS/TSX/JS/HTML: 0 edições
  com erro na janela) — `origem: "semeado"`, contado no balde arquivo e reportado à parte, nunca somado ao natural.
- Todo positivo semeado é confirmado por um **oráculo independente do motor** (o `tsc`/`deno` do projeto para TS/TSX;
  disco para `recurso`; o documento parseado para `ancora`; o 404 do CDN; todas as folhas resolvidas para
  `classe_css`) que acusa o mutado **e não** o original; item sem oráculo conclusivo sai do corpus, nunca é rotulado
  por suposição.
- Limiares iguais aos do Python: recall de detecção ≥ 0,70 por classe com n ≥ 20 · falso deny ≤ 1% por balde ·
  p95 ≤ 150 ms (camada 1) e ≤ 1 s (camada 2).

## 8. Aceite da VL0

1. Python **sem regressão por id**: juiz v2 e V3 no corpus V0d com os mesmos positivos detectados e os mesmos
   negativos com deny, antes e depois (binário congelado, sha anotado).
2. Teste de tabela de `lang_do_caminho` (todas as extensões de §2 + uma desconhecida → `None`) e de `check` com
   linguagem sem adaptador → `unknown` com motivo (mutante: devolver `allow` precisa morrer).
3. Delta e política §5 aplicados no `check` para todas as linguagens, com teste por regra (mutante por regra).
4. Hooks (`pre-edit-prevention`, `pre-write`, MultiEdit) derivam `lang` de `lang_do_caminho`; `.rs`/`.ts`/`.tsx`/`.html`
   passam a gravar sombra com `lang` certo (hoje só `.py`).
5. `cargo fmt --check`, `clippy -D warnings`, suítes dos crates tocados com 0 falhas.

**Sequência**: a VL0 começa depois que a V2/V3 convergirem (a Kimi corrige R1–R3 nos mesmos arquivos). Os corpora
semeados (§7) e as sondas de custo da camada 2 não dependem dela e começam já.
