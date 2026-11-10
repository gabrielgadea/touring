# Brief 4b — revisão com controle positivo dos detectores F2.1/F2.4 (touring-9e, Kimi)

Decisão do Gabriel (10/11/2026): «Revisão com controle positivo: para cada commit, um teste mostrando que um
segredo ou um XSS real continua sendo pego».

## Os 7 commits (todos de 02/11 e 08/11, fora de especificação)

| commit | detector | arquivo |
|---|---|---|
| `0abafc64` | F2.1 especificador de módulo ES/TS constante e `<script>` constante não são sink | `crates/touring-analysis/src/quality/security.rs` |
| `461cfa77` | F2.1 especificador tem de ser o literal INTEIRO | idem |
| `49fb4552` | XSS: `<script>` montado só de constantes SCREAMING_SNAKE não é sink | idem |
| `1305ff40` | F2.4 cinco classes de falso positivo (748 linhas) | `crates/touring-quality/src/verifications/f2_4_secrets.rs` (confirme o caminho) |
| `059b2026` | F2.4 frase de placeholder exige vocabulário de segmento inteiro | idem |
| `44869807` | F2.4 campo de struct Go com tag nomeia chave, não atribui segredo | idem |
| `910074b6` | teste VGP html: sentinela sem sufixo hex | `crates/touring-code/src/vgp/lang/html/tests/recurso.rs` |

## O que fazer, por commit
1. Ler o diff (`git show <h>`) e escrever a condição exata em que o detector deixou de acusar.
2. Construir os **vizinhos perigosos**: entradas que se parecem com o caso liberado mas SÃO um segredo ou um sink
   reais (ex.: `require("../../" + userInput)`, `<script>` com interpolação de variável não-constante, segredo
   real de alta entropia num campo Go sem tag, `api_key = "sk-live-…"` com palavra de placeholder no meio).
   Mínimo 3 por commit.
3. Para cada vizinho, um teste que exige a detecção. Rodar.
   - passa → o controle positivo fica como teste (commit seu, mensagem em inglês).
   - falha → é um FURO: NÃO corrija o detector; registre com a entrada mínima que reproduz.
4. Mutante por commit: reverter a lógica nova num cópia e mostrar que o teste de falso positivo do commit reprova
   (prova que o teste do commit não é vácuo).

## Arquivos exclusivos seus
- Testes novos em `crates/touring-analysis/tests/security_positive_controls_test.rs` e
  `crates/touring-quality/tests/f2_4_positive_controls_test.rs` (crie; não edite os testes existentes).
- Relatório: `docs/plans/2026-11-10-decisoes-pendentes/RELATORIO-4b.md`.
- NÃO edite `security.rs`, `f2_4_secrets.rs` nem nada fora desses três caminhos. Outras sessões editam o resto.

## Como rodar (máquina compartilhada)
`hw-slot --class rust -- cargo test -p touring-analysis --test security_positive_controls_test` (idem para
touring-quality). Nunca `cargo build --release`, `update-touring` ou mexer em hooks/settings.

## Aceite (o juiz desta sessão confere)
- Tabela por commit: condição liberada, vizinhos testados (n), passou/furo, mutante (morto/vivo).
- `cargo test` dos dois arquivos novos: saída colada com a contagem; `cargo clippy --all-targets -D warnings` nos
  dois crates limpo.
- Todo furo com entrada mínima reproduzível. Zero furos sem pelo menos 3 vizinhos por commit não conta como
  aprovação.
- Um único relatório ao terminar, via SendMessage para `touring-b4`.
