# F — coletor de achados dos adapters do VGP: fatos (09/11/2026)

Levantamento só-leitura (subagente sonnet), sem execução; caminhos relativos a
`crates/touring-code/src/vgp/`. DAG `task_1794217886022321956`.

## Regras por adapter

| adapter | dedup | sugestão (limiar · desempate) | conjunto visto | teto |
|---|---|---|---|---|
| python `lang/python.rs` | (classe,nome) linear :1010 | 1..=2 · (dist,nome) :1049 | `reais` 8 (ordem total, :991) | — |
| rust `lang/rust.rs` | (classe,nome) :2750 | 1..=2 · `min_by_key` :2762 (chamadores ordenam antes) | `HashSet.iter().take(8)` e só então sort (:2270, :2483) — **não determinístico** | — |
| ts `lang/typescript/` | HashSet `analise.rs:427` | 1..=2 · (dist,nome) `util.rs:75` | `reais_de` ordena+corta 40 (`util.rs:88`) | 60 silencioso (`analise.rs:35`) |
| html `lang/html/texto.rs:178` | linear `html.rs:92` | `(chars/4).max(2)`, igualdade ignorando caixa = 0 · (dist,str), top 5 | 2000 | — |
| classe_css `lang/classe_css.rs:97` | HashSet :141 | `(chars/4).clamp(1,3)`, pré-filtro em BYTES (:101) · top 5, texto próprio minúsculo | todos | — |
| cdn `lang/cdn.rs:696` | (classe,nome) | — | versões 5 | — |

Levenshtein único: `lang/distancia.rs:12` (por char). Tipo único já existe: `Finding`
(`mod.rs:32`, construtor `Finding::candidato` :88); retorno `Verificacao` (`lang/mod.rs:46`).

## Testes e corpora que fixam saída

python `python/tests.rs:1170,1245,1264`; ts `util.rs:157`, `testes.rs:179`; rust: nenhum;
html `texto.rs:258`, `tests/ancora.rs:239`, `tests/recurso.rs:254,283`; classe_css
`tests/tests.rs:92,188`; replay byte a byte `tests/vgp_contencao_replay.rs` +
`fixtures/vgp_contencao_replay.json` (contém "Você quis dizer `img/logo.png`?").
Juízes J15/J17 excluem `reais`/`sugestao` da comparação (`comparar_juizes.py:4`,
`comparar_porid.py:5`; `consolidar_j15.avaliar` não lido — não verificado).

## Não muda saída visível
dedup (classe,nome) único; desempate (dist,nome) explícito no rust; frase única
". Você quis dizer `s`?" para py/rust/ts/âncora/recurso.

## Muda saída visível (decisão explícita)
1. rust: ordem total antes do corte de 8 (torna determinístico).
2. limiar único (2 fixo × len/4).
3. igualdade ignorando caixa em py/ts.
4. classe_css: pré-filtro bytes → chars.
5. formato da frase do classe_css.
6. teto de achados nos demais / `nao_medido` quando o teto do TS morde.
7. sugerir sobre o conjunto completo antes de cortar `reais`.
