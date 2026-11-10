# Relatório 4b — controles positivos dos detectores F2.1/F2.4

Data: 10/11/2026. Executor: touring-9e (Kimi). Base revisada: `6addf398dd9be35e12d5fbbfd487d66d072f568f`.

## Escopo e método

Foram lidos os 7 diffs e exercitados **51 vizinhos perigosos** (mínimo de 3 por commit), sem editar `security.rs`, `f2_4_secrets.rs` ou o teste HTML na árvore compartilhada. **45 vizinhos continuam detectados; 6 entradas mínimas reproduzem furos.** Os furos não foram corrigidos: ficaram como testes `#[ignore]` nomeados e com a razão registrada, para que a suíte ativa preserve os 45 controles aprovados sem esconder as reproduções.

Arquivos novos:

- `crates/touring-analysis/tests/security_positive_controls_test.rs`
- `crates/touring-quality/tests/f2_4_positive_controls_test.rs`

## Resultado por commit

| Commit | Condição liberada | Vizinhos | Detectados | Furos | Mutante |
|---|---|---:|---:|---:|---|
| `0abafc64` | Especificador ES/TS constante e `<script>` constante deixam de ser sinks | 9 | 7 | 2 (`%` e template HTML) | **morto** |
| `461cfa77` | Especificador precisa ser o literal inteiro; sufixo de statement é limitado | 5 | 5 | 0 | **morto** |
| `49fb4552` | `<script>` montado só de nomes SCREAMING_SNAKE deixa de ser sink | 6 | 6 | 0 | **morto** |
| `1305ff40` | Cinco classes: comparação, parâmetro vizinho, stand-ins, texto de UI e literal legível | 16 | 16 | 0 | **morto** |
| `059b2026` | Frase placeholder exige vocabulário por segmento inteiro | 6 | 3 | 3 (substring sem separador) | **morto** |
| `44869807` | Campo Go com struct tag não é tratado como atribuição | 6 | 5 | 1 (credencial em `json` tag) | **morto** |
| `910074b6` | Sentinela HTML perde o sufixo hexadecimal; detector não mudou | 3 | 3 | 0 | **morto** |

## Furos reproduzíveis

### F4b-1 — `0abafc64`: formatação `%` alcança `<script>` sem finding XSS

Entrada mínima:

```python
def page(user):
    return "<script>var u = '%s';</script>" % user
```

Teste: `furo_0abafc64_percent_format_script_sink`. Resultado fresco: rc 101, `XSS missed`. A linha não contém `{`, `.format(` nem concatenação com identificador, forma pela qual a exceção de constante a libera.

### F4b-2 — `0abafc64`: template HTML com interpolação é tratado como HTML estático

Entrada mínima:

```html
<html><script>var u = '{{ user }}';</script></html>
```

Teste: `furo_0abafc64_interpolated_html_template_script`. Resultado fresco: rc 101, `XSS missed`. Em `lang="html"`, `is_constant_script_tag` retorna `true` para a tag sem verificar interpolação; um template com dados de runtime não é um documento estático.

### F4b-3 — `059b2026`: a via sem separadores ainda aceita substring de placeholder

Entradas mínimas:

```python
password = "protest"
api_key = "latest"
password = "protestHeaven"
```

Teste: `furo_059b2026_placeholder_substring_without_separator`. Resultado fresco: rc 101, as 3 entradas em `F2.4 missed`. A exigência de segmento inteiro protege a via de frases, mas a via posterior de texto alfabético ainda usa `has_placeholder_word`, que procura substring (`protest`/`latest` contêm `test`).

### F4b-4 — `44869807`: credencial curta disfarçada de chave `json` é liberada

Entrada mínima:

```go
Password string `json:"Tr0ub4dor&3xK9"`
```

Teste: `furo_44869807_short_credential_in_json_tag`. Resultado fresco: rc 101, `F2.4 missed`. A classificação de campo Go suprime as vias de atribuição; o valor curto não possui marcador de provedor nem atinge a regra genérica de alta entropia.

## Evidência dos testes novos

Comandos:

```bash
scripts/hw-slot --class rust -- cargo test -p touring-analysis --test security_positive_controls_test
scripts/hw-slot --class rust -- cargo test -p touring-quality --test f2_4_positive_controls_test
```

Saída-resumo fresca:

```text
touring-analysis: running 6 tests
test result: ok. 4 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out

touring-quality: running 6 tests
test result: ok. 4 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out
```

Reprodução dos furos:

```bash
scripts/hw-slot --class rust -- cargo test -p touring-analysis --test security_positive_controls_test -- --ignored
scripts/hw-slot --class rust -- cargo test -p touring-quality --test f2_4_positive_controls_test -- --ignored
```

Saída-resumo fresca:

```text
touring-analysis: test result: FAILED. 0 passed; 2 failed; 4 filtered out
touring-quality:  test result: FAILED. 0 passed; 2 failed; 4 filtered out
EXPECTED_HOLE_RESULTS analysis_rc=101 quality_rc=101
```

## Clippy e formato

```bash
scripts/hw-slot --class rust -- cargo clippy -p touring-analysis --all-targets -- -D warnings
scripts/hw-slot --class rust -- cargo clippy -p touring-quality --all-targets -- -D warnings
rustfmt --edition 2024 --check crates/touring-analysis/tests/security_positive_controls_test.rs crates/touring-quality/tests/f2_4_positive_controls_test.rs
```

Os três comandos terminaram com rc 0. O clippy de `touring-analysis` terminou em 13,00 s e o de `touring-quality` em 10,70 s; nenhum warning foi promovido a erro. A nota de future-incompat de `proc-macro-error2` é dependência externa e não alterou o resultado.

## Mutantes

Execução em cópia isolada sob `~/claude-artefatos/touring/4b-kimi-20261110/`, com `CARGO_TARGET_DIR` próprio e restauração dos originais entre mutantes. Antes das mutações, a cópia passou **34/34** testes de `security_analyzer_test` e **102/102** de `f2_4_secrets::tests`. Cada reversão foi seguida de um controle sem mutação no mesmo target.

| Commit | Reversão comportamental | Prova sob mutante | Controle restaurado | Veredito |
|---|---|---|---|---|
| `0abafc64` | Remove as exceções de módulo e `<script>` | `a_constant_`: 2 failed/1 passed, rc 101 | 3/3, rc 0 | morto |
| `461cfa77` | Aceita prefixo literal sem exigir o especificador inteiro | teste do especificador: rc 101 em `require("../../" + userInput)` | 1/1, rc 0 | morto |
| `49fb4552` | Volta a tratar qualquer interpolação/concatenação como taint | teste SCREAMING_SNAKE: rc 101 | 1/1, rc 0 | morto |
| `1305ff40` | Recupera as decisões anteriores das 5 classes + `assigned_name` | **9 failed/93 passed**, rc 101; todos os 9 testes introduzidos pelo commit falharam | 102/102, rc 0 | morto |
| `059b2026` | Volta de vocabulário por segmento para substring | `protest-heaven-sample-castle-12` liberado, rc 101 | 1/1, rc 0 | morto |
| `44869807` | Desativa o reconhecimento de campo Go com tag | `json:"password,omitempty"` acusado, rc 101 | 1/1, rc 0 | morto |
| `910074b6` | Restaura `foo.vgpAusente0c2dcab442d3.png` | F2.4 **0.000**, `high-entropy-literal` na linha 416; `score --fail-below 0.80` rc 1 | F2.4 **1.000**, rc 0 | morto |

Nota de instrumento: `touring-quality check` imprimiu o blocker do mutante HTML mas retornou rc 0. Por isso o veredito desse mutante usa `touring-quality score --dims F2.4 --format json --fail-below 0.80`, que mediu 0.000/rc 1 contra 1.000/rc 0. Os demais usam o exit code do `cargo test`.

## Verificação de escopo

Os dois arquivos de teste obtiveram **F2.1=1.000 e F2.4=1.000** com `touring-quality score --fail-below 0.80`. Eles não declaram símbolos `pub`. `git diff --check` terminou com rc 0 e o status mostra somente os três caminhos autorizados como novos; os três arquivos de produção/teste HTML revisados permanecem sem modificação. Na cópia de mutantes, `cmp -s` confirmou restauração byte a byte dos três arquivos originais.

## Decisão

A revisão **não aprova indiscriminadamente** os 7 commits: `461cfa77`, `49fb4552`, `1305ff40` e `910074b6` tiveram todos os vizinhos desta amostra detectados; `0abafc64`, `059b2026` e `44869807` possuem os furos mínimos acima. Nenhum detector foi alterado por esta tarefa.
