# VGP — o motor que verifica símbolos no momento da edição

> Manual do motor (V6 do plano [`plans/2026-10-08-vgp-motor-base/`](plans/2026-10-08-vgp-motor-base/proposta.md)).
> Cada número aqui vem com o método e o denominador; a fonte de cada um está no bundle do plano.

## O que ele faz

O modelo inventa nomes: `from scripts.x import funcao_que_nao_existe`, `os.caminho_inventado`, um kwarg que a função
não tem. Antes, a defesa era uma regra de texto (`vgp-protocol.md`) que pedia "verifique antes" — persuasão. O motor
troca a persuasão por **verificação no executor**: o conteúdo proposto é checado contra o projeto real, e a recusa
ensina os nomes reais e o mais próximo.

## Arquitetura

| peça | onde | papel |
|---|---|---|
| camada 1 (nativa) | `crates/touring-code/src/vgp/` | tree-sitter + tabela de símbolos do módulo extraída sob demanda (cache por sha); sem dependência externa. Um adaptador por linguagem em `vgp/lang/<lang>.rs` (registro único; sem adaptador a resposta é `unknown` com motivo, nunca `allow`); severidade e delta moram no `check` (`vgp/politica.rs`), não nos adaptadores |
| camada 2 (verificador do projeto) | `crates/touring-pyright/src/` (`pool`, `servidor`, `lsp_nucleo`, `pyright`, `rust_analyzer`, `tsserver`, `deno`) | o verificador do PRÓPRIO projeto (nunca instalado pelo Touring), persistente: uma sessão por `(verificador, raiz)` — pyright para Python, rust-analyzer para Rust, tsserver ou `deno lsp` para TS/TSX/JS. **Ligada por padrão só para Python**; as outras linguagens são opt-in em `[vgp] camada2_langs` |
| CLI | `touring vgp check` | contrato puro, sem daemon: `--path --content-file --project-root [--disk-file] [--overlay o=a]… [--lang] [--camadas 1,2] -j` (`--camadas 1,2` pede a camada 2; para uma linguagem fora de `[vgp] camada2_langs` o JSON fica na camada 1 e `camada2` diz `desligada: …`) |
| hooks | `crates/touring-hook-handlers/src/hooks/{pre_edit_prevention,pre_write,pre_bash}.rs` | Edit, MultiEdit, Write e código inline do Bash |
| registro de sombra | `crates/touring-code/src/vgp/sombra.rs` | uma linha JSON por checagem feita por hook |

**Veredito = delta proposto − disco**: o que já estava errado no arquivo nunca é culpa da edição.
**Falha aberta**: o que impede a medição (prazo, verificador ausente, servidor morto) vira `unknown` com motivo, e a
chamada segue intacta.

## A política de severidade (P4)

| escopo do alvo | inline e `/tmp` (o código roda AGORA) | arquivo do projeto |
|---|---|---|
| `proprio` — nome do próprio programa | **deny** | aviso |
| `projeto` — módulo ou classe sob a raiz | **deny** | aviso |
| `externo` — stdlib, terceiro | **deny** | **deny** |
| `forma` — membro de valor builtin (`int.add`, `str.get`) | aviso | aviso |
| `dinamico` — atributo dunder; nome sob `from X import *` | aviso | aviso |

Por que arquivo do projeto só avisa para símbolo do projeto: medido no corpus V0d do analise, com o estado do
projeto NO INSTANTE de cada edição, as 10 edições de 259 com achado real eram estados intermediários — o consumidor
escrito antes do provedor, e testes na fase RED de TDD. Negar ali bloquearia o RED que o próprio TDD exige.

**Medição da política** (régua do instante; 259 negativos de arquivo, 100 de `/tmp` escrito-e-executado e 300
inline Python, todos execuções/edições sem erro depois): falso deny **0/259 · 0/100 · 0/300**; detecção de símbolo
inventado `/tmp` 7/7 e inline 16/27; latência da camada 2 com sessão quente p95 77 ms. Fora do alcance de qualquer
verificador estático — e por isso fora do recall do juiz: a **forma do dado** (`'list' object has no attribute
'get'`, 24 dos 65 erros reproduzidos; o `json.load` devolve `Any`).

## Modo, desligamento e registro

| controle | efeito |
|---|---|
| `[vgp] modo = "sombra"` (default) em `<projeto>/.touring/touring.toml` | o motor julga e grava o registro; NADA chega ao contexto do modelo |
| `[vgp] modo = "ativo"` | deny e aviso chegam ao modelo: `[VGP · deny] …` como recusa, `[VGP · aviso] …` como contexto |
| `TOURING_VGP_MODE` | sobrescreve o modo do projeto |
| `TOURING_VGP_DISABLED=1` | kill switch do motor (humano) |
| `[vgp] camada2_langs = ["python", "rust"]` | quais linguagens têm camada 2 no projeto; **o valor declarado substitui o padrão** (`["python"]`; `[]` desliga a camada 2). Lista de uma linha; qualquer outra forma é ignorada e o padrão vale. Nomes = o `lang` do motor (`python` · `rust` · `typescript` · `tsx` · `javascript`; `html`/`css` não têm verificador e respondem `unknown`) |
| `TOURING_VGP_L2_DISABLED=1` | desliga a camada 2 inteira |
| `TOURING_VGP_L2_OFF=rust-analyzer,deno` | desliga só esses verificadores |
| `TOURING_VGP_L2_IDLE_S` (900) · `TOURING_VGP_L2_MAX` (2, por verificador) · `TOURING_VGP_L2_MAX_RA` (1) | ociosidade e teto de sessões vivas |
| `TOURING_VGP_L2_WARMUP_S` (180) | prazo do aquecimento em segundo plano (rust-analyzer, tsserver, deno) |
| `TOURING_VGP_SOMBRA_FILE` | caminho do registro (default `~/.claude/touring/vgp/sombra.jsonl`) |

Linha do registro: `{ts, session_id, tool_use_id, hook, project_root, path, balde, camadas, modo, verdict,
elapsed_ms, findings[{classe, nome, linha, severidade, escopo, sugestao, reais}], motivo?}` — sem o conteúdo do arquivo.

## Como se mede (juízes por exit code)

| fase | comando (no bundle do plano) |
|---|---|
| corpus | `scripts/juiz_v0d.py` · negativos inline/`/tmp` em `entradas_inline_neg.jsonl` |
| camada 1 | `scripts/juiz_motor_vgp.py --touring <bin>` — recall de símbolo ≥ 0,70, falso deny ≤ 1 % por balde, p95 ≤ 150 ms |
| camada 2 | o mesmo juiz em modo comparativo (`--base` da camada 1, `--p95-ms 1000`) |
| deny em uso real | `scripts/juiz_v4_sessoes.py --desde --ate` — 5 sessões com deny correto, zero falso reportado |
| ponta a ponta | `scripts/prova_vgp_motor.py` — as células da P4 contra o binário instalado |
| o plano | `scripts/juiz_plano_vgp.py` |

## Armadilhas medidas

- **Cliente LSP push casa pela versão.** O pyright não tem diagnóstico pull, e o `didClose` publica uma limpeza com
  `version` nula: sem casar pela versão, a reabertura da mesma uri lê a limpeza como resultado (falso deny 10 → 27).
- **"Compila" não prova que é o programa Python que rodou.** `cat > f <<EOF` é texto gravado; `touring run --lang
  bash --code 'head -30 /x'` é uma expressão Python válida. A linguagem vem de uma declaração, não do parse.
- **Corpus de negativos julgado contra o projeto de hoje mede a deriva**, não o verificador: o estado tem de ser o do
  instante da edição (`scripts/estado_no_instante_v0d.py`).
