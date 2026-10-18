# V3 — camada 2 do motor VGP: o verificador do projeto (pyright-langserver persistente)

> Coordenadora analise-89 · 09/10/2026 · referência executável e medida: [`scripts/sonda_camada2_pyright.py`](scripts/sonda_camada2_pyright.py)
> (selftest 13/13) · medição: `diagnostics/sonda_camada2_pyright_analise_v2_{instante,estrita}.json` · aceite: juiz v2
> [`scripts/juiz_motor_vgp.py`](scripts/juiz_motor_vgp.py) (sha `13e0bff9633cd99c`).

## 1. O que a medição fixou (09/10/2026, corpus V0d, projeto analise, 315 entradas)

| fato | número | método |
|---|---|---|
| latência por edição, langserver quente | p50 18,5 ms · p95 105,6 ms | didOpen → publishDiagnostics da MESMA versão |
| detecção de símbolo em `/tmp` | 7/7 | régua do instante |
| detecção de símbolo inline | 16/28 | idem; 2 dos 12 perdidos são `__file__` em `-c`/stdin (regra da camada 1) |
| forma do dado (`'list' object has no attribute 'get'`) | 1/21 | fora do alcance estático — não entra no recall |
| negativos com achado real (P0 «nega tudo») | 10/259 | 10/10 estados intermediários (9 corrigidos depois, 1 arquivo apagado) |
| falso deny com a política P2 | 1/259 (0,39%) | deny em arquivo do projeto só para alvo EXTERNO |
| diagnóstico pull (LSP 3.17) | ausente | `capabilities.diagnosticProvider` nulo no pyright 1.1.408 |
| limpeza publicada no `didClose` | `version` nula, 0 diagnósticos, ~50 ms depois | reabrir a mesma uri 3× |

## 2. Protocolo (o que a sonda aprendeu errando)

1. **Spawn**: `<raiz>/.venv/bin/pyright-langserver --stdio` (depois `venv/`, depois `basedpyright-langserver`); cwd = raiz.
   **Nunca instalar** (proposta §5). Ausente → a camada 2 não roda e diz por quê (fail-open, motivo nomeado).
2. **initialize**: `rootUri` + `workspaceFolders` = raiz; capabilities `textDocument.publishDiagnostics.versionSupport =
   true` e `workspace.configuration = true`. Depois, `initialized`.
3. **workspace/configuration** (o servidor PEDE; sem resposta ele espera para sempre): seção `python` →
   `{"pythonPath": "<raiz>/.venv/bin/python"}`; seção `python.analysis` → `{"typeCheckingMode": "standard",
   "diagnosticMode": "openFilesOnly", "autoSearchPaths": true, "extraPaths": ["<raiz>"]}`; qualquer outra → `null`.
   Qualquer outra requisição do servidor (tem `id` e `method`) → `result: null`.
4. **Uma checagem**: `didOpen` com **versão única monotônica** (nunca reusar) → esperar a `publishDiagnostics` cuja
   `uri` E `version` casam → `didClose`. Publicação de outra versão ou com `version` nula é ignorada. Prazo por
   checagem (default 1000 ms); estourou → `unknown` com motivo, nunca allow silencioso.
5. **Delta** (proposto − disco): com o conteúdo em disco, checar também o disco na mesma uri (outra versão) e remover
   os pares `(regra, nome)` que já estavam lá.
6. **Overlays**: abrir em memória (`didOpen`) os módulos sobrepostos ANTES da checagem e fechá-los depois.
7. **Inline** (`<string>`/`<stdin>`): uri virtual `<raiz>/.vgp_inline_<hash>.py` (nunca escrita em disco).

## 3. Diagnóstico → achado (portar `negacao`, `escopo` da sonda, com os mesmos casos de teste)

- **Nega (alta precisão)**: `reportUndefinedVariable` · `reportCallIssue` que começa com `No parameter named` ·
  `reportAttributeAccessIssue` com `is not a known attribute of module` ou `is unknown import symbol` · idem com
  `Cannot access attribute "x" for class "C"` quando `C` é nome simples (`^[A-Za-z_][A-Za-z0-9_.]*$`) e ≠ `None`.
  `dict[Unknown, Unknown]`, `X | None`, `Unknown` → não é achado de negação (inferência incompleta).
- **Nome**: o 1º trecho entre aspas da mensagem.
- **Escopo do alvo**: `proprio` (reportUndefinedVariable) · `projeto` (módulo resolvido sob a raiz, absoluto ou relativo
  ao arquivo; classe que não é builtin; kwarg) · `externo` (módulo fora da raiz) · `forma` (membro de classe BUILTIN:
  `int.add`, `str.get` — pergunta de forma do dado, o mesmo que o runtime chama de forma). Dúvida → `projeto`.
- **`dinamico`**: atributo dunder (`^__\w+__$`, ex.: `__wrapped__` do `functools.wraps` num `@pytest.fixture`) de
  classe não builtin, e nome indefinido num programa com `from X import *` (espaço de nomes incognoscível).
- **Severidade (política P4 = P2 + forma + dinâmico, proposta §5 revisão de 09/10)**: `forma` e `dinamico` → `warn`
  sempre; código que roda AGORA (inline e `/tmp`) → `deny`; arquivo do projeto → `deny` só se `externo`, senão `warn`.
  Uma função pura, 15 células (3 baldes × 5 escopos), um teste por célula. Medido no corpus final (régua do instante,
  259 negativos de arquivo + 100 de `/tmp` + 300 inline): falso deny **0/259 · 0/100 · 1/300** (o 1 é defeito de
  corpus, `cat >>` extraído como programa); detecção de símbolo `/tmp` 7/7, inline 16/27; p95 68 ms. A P3 dava
  `/tmp` 1/100 (`from harness import *`) e inline 2/300 (+ `T.html.__wrapped__`); custo da P4 no recall de deny: zero.

## 4. Onde mora

- `crates/touring-lsp/src/client.rs` — cliente LSP genérico, síncrono, std + serde_json (sem tower-lsp, sem tokio):
  framing Content-Length, pedido/aviso, resposta às requisições do servidor, `open → versão`, espera por versão com
  prazo, `close`, encerramento no `Drop` (`shutdown` + `exit`, e `kill` se não sair).
- `crates/touring-lsp/src/pyright.rs` — descoberta, configuração, checagem com delta/overlays, `negacao`, `escopo`,
  `severidade`. Tipo de saída neutro (`L2Achado { classe, nome, linha, escopo, severidade, regra, mensagem }`); quem
  converte para o `Finding` do motor é o consumidor (`touring vgp check`, hooks) — sem dependência `touring-code →
  touring-lsp`.
- Sem feature nova: o módulo é sempre compilado (como `mapping`).

## 5. Aceite

1. Testes unitários: framing (ida e volta), casamento por versão (uma limpeza `version: null` ANTES do resultado tem de
   ser ignorada — teste com servidor falso), negação/escopo/severidade (os 13 casos do selftest da sonda + as 6 células
   da política), prazo estourado → `unknown`.
2. Teste de integração contra o pyright REAL do analise, marcado `#[ignore]` (roda explícito): 5 formas inventadas
   negadas, 1 módulo real limpo, reabertura da mesma uri sem ler a limpeza.
3. `cargo clippy -p touring-lsp --all-targets -- -D warnings` e `cargo fmt --check` limpos; complexidade ≤ 10 por fn.
4. Integração (depois da V2 da Kimi): `touring vgp check --camadas 1,2` passa no juiz v2 com `recall(V2+V3) >
   recall(V2)` sem piorar o falso deny, p95 ≤ 1 s.
