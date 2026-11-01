---
type: Spec
title: V0d — corpus de replay do motor VGP
description: O que o corpus contém, de onde cada peça sai, e o juiz que decide se ele está pronto (scripts/juiz_v0d.py).
plan_id: 2026-10-08-vgp-motor-base
timestamp: 2026-10-08T21:10:00-03:00
tags: ["#kind:plan", "#status:open", "#domain:harness"]
---

# V0d — corpus de replay do motor VGP

**Para que serve.** É a régua da V2/V3/V5 (proposta.md, tabela de fases): recall ≥ 70 % nos positivos, falso deny
≤ 1 % nos negativos, p95 ≤ 150 ms. O motor julga **um arquivo no estado proposto** contra o índice de hoje; logo cada
entrada é um *snapshot* de conteúdo + o projeto contra o qual os nomes se resolvem.

**Onde fica.** Dado bruto de transcript ⇒ fora de repositório (mesma regra do inventário):
`~/.claude/touring/vgp/corpus_v0d/` com `entradas.jsonl` + `snap/<id>.<ext>` (o conteúdo). O repositório leva só o
código (`scripts/corpus_v0d.py`) e o juiz (`scripts/juiz_v0d.py`).

## Entrada (uma linha de `entradas.jsonl`)

| campo | conteúdo |
|---|---|
| `id` | sha1[:12] de `(rotulo, session, path, ts)` — determinístico (REGRA #17) |
| `rotulo` | `positivo` · `negativo` |
| `classe` | positivo: `nome` · `atributo` · `import` · `kwarg` (as 4 de API inventada da V0a); negativo: `null` |
| `origem` | `atribuido` (arquivo editado na sessão) · `inline` (`python - <<EOF` / `-c`) · `tmp` · `cache` |
| `lang` | `python` (Rust/TS entram quando a V1 indexar membros) |
| `path` | caminho ABSOLUTO do arquivo; inline: `<stdin>` ou `<string>`, como o traceback o nomeia (é o que o `frame_in_output` casa) |
| `project_root` | raiz contra a qual os nomes se resolvem (`vivos.project_root`) |
| `snap` | `snap/<id>.py`; `snap_sha256` do conteúdo |
| `nome_esperado` | positivo: o nome inventado (de `vivos.static_name`); negativo: `null` |
| `session`, `ts`, `tool_use_id` | proveniência no transcript |
| `item_v0a` | positivo: o id da linha do inventário V0a que o originou |
| `kind`, `linha`, `mensagem` | positivo: a classe da exceção, a linha e a mensagem originais (o juiz confere o frame com elas) |
| `command`, `cwd` | positivo: a invocação ORIGINAL inteira e o cwd (o juiz re-executa uma amostra com `replay.run_isolated`) |
| `contexto` | opcional: `[{path, snap, sha256}]` — outros arquivos que o replay sobrepôs (ex.: módulo irmão em `/tmp` que o script importa). O juiz confere o sha e sobrepõe os mesmos; nada sobreposto fora desta lista |
| `reproduz` | positivo: `true` só se o **replay** (bwrap, overlay com o snapshot no `path`) mostrou a classe NO FRAME (`frame_in_output`) — registro em `replay_v0d/<id>.json` |

## De onde cada peça sai (já existe — reusar, não reescrever)

- **Positivos**: linhas da V0a (`erros_inventario.jsonl`) com classe de API inventada e balde `atribuído` ou `inline`.
  Snapshot = conteúdo do arquivo no instante `first`:
  - `/tmp` e `<projeto>/.cache`: `replay.rebuild_tmp` (Write/Edit/MultiEdit/heredoc, pulando chamadas negadas);
  - arquivo do projeto: base = `git show <rev>:<path>` com `rev = git rev-list -1 --before=<ts> HEAD`, depois os
    Edit/Write da sessão até `first` (mesma dobra do `rebuild_tmp`, sem o filtro de diretório). Edit que não casa ⇒
    `incompleto` (a entrada sai do corpus e conta no relatório, nunca entra com conteúdo errado);
  - inline: o próprio programa do comando (`- <<'EOF'…EOF` ou `-c '…'`), extraído do `command` inteiro.
  Validação: `replay.py` com `files={path: snapshot}` roda o comando original; `reproduz = kind_in_output and
  frame_in_output`. **Só `reproduz = true` conta no recall** — o resto fica listado como positivo não validado.
- **Negativos**: 300 edições `.py` bem-sucedidas, `random.Random(20261008)`: tool_use `Edit`/`Write`/`MultiEdit` cujo
  resultado não é erro e sem nenhum erro de Python que nomeie o arquivo na mesma sessão até a próxima edição dele.
  Snapshot = conteúdo depois da edição (mesma reconstrução); exige `py_compile` ok. Universo e sorteio registrados
  em `negativos_candidatos.json` = `{"seed": 20261008, "candidatos": [ids em ordem canônica]}`; o juiz re-sorteia
  `random.Random(seed).sample(candidatos, 300)` e exige o mesmo conjunto de ids.

## O juiz (`scripts/juiz_v0d.py`, exit 0 = pronto)

1. `entradas.jsonl` existe; ids únicos; cada `snap` existe e o sha256 confere.
2. Negativos: ≥ 300, todos com `py_compile` ok; re-sortear com a semente devolve **os mesmos ids** (reprodutível).
3. Positivos: ≥ 1 **que reproduza** por classe presente na V0a (classe sem positivo provado = recall não mensurável nela); cada `reproduz = true` tem registro de replay com `kind_in_output` e
   `frame_in_output` verdadeiros (o juiz relê o registro — não confia no campo).
4. **Controle positivo** (aceite da proposta): ≥ 70 % dos positivos candidatos com `reproduz = true` — abaixo disso o
   corpus mede a reconstrução, não o motor.
5. Re-execução própria do juiz (`--amostra k`, default 3, sorteio com a semente): COM o snapshot reproduz. Controle
   negativo: um positivo `atribuido` cujo arquivo de hoje já não contém `nome_esperado`, rodado SEM o snapshot, NÃO
   reproduz. A re-execução só roda quando 1–4 passam.

Selftest do juiz (`--selftest`, rc 0 em 08/10/2026): corpus sintético válido → 0 pendências; 5 mutantes pegos (sha do
snapshot · um negativo a menos · registro sem frame · re-execução que não reproduz · arquivo de hoje ainda quebrado).

Universo de positivos candidatos medido em 08/10/2026 (V0a, python, balde atribuído/inline, `classe_de`): **100** —
atributo 63 (33 atribuído + 30 inline) · nome 31 (14 + 17) · import 5 (2 + 3) · kwarg 1 (inline). Controle: 570
linhas python atribuído/inline ficam sem classe (outras exceções). O kwarg tem UM candidato: se não reproduzir, o
critério 3 reprova e a decisão volta à coordenadora (não se afrouxa o juiz).
