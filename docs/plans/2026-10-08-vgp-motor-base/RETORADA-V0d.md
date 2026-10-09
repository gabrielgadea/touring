# V0d — relatório final do corpus de replay

> **09/10/2026** · corpus construído por mim, juiz **reprovando com 3 pendências**. Nada foi
> afrouxado no juiz nem nos limiares. Esta mensagem substitui o relatório que a coordenadora
> `analise-6b` esperava — a sessão encerrou antes de eu conseguir entregar por `SendMessage`.

## 1. Positivos

| | |
|---|---|
| candidatos V0a medidos | **100** (atributo 63 · nome 31 · import 5 · kwarg 1) |
| snapshots completos no corpus | **51** → inline 33 · tmp 15 · atribuído 3 |
| incompletos | **49**, por motivo (nenhum entrou com conteúdo errado) |
| **reproduzem** | **30/51 = 58,82 %** |

**Incompletos por motivo**

| motivo | n |
|---|---|
| `[inline]` sem heredoc nem `-c` no comando | 18 |
| `[tmp]` a sessão não gravou esse arquivo antes de `first` | 11 |
| `[atribuido]` o arquivo não existia no commit anterior a `first` | 14 |
| `[atribuido]` o arquivo não está sob a raiz do projeto | 4 |
| `[atribuido]` Edit que não casa sobre a base do git | 2 |

**Reproduzem por classe** — *todas as 4 classes com ≥ 1 provado* (critério 3 do juiz):

| classe | provados |
|---|---|
| atributo | 23 |
| import | 4 |
| kwarg | 1 (é o único candidato) |
| nome | 2 |

**Reproduzem por origem:**

| origem | provados | taxa |
|---|---|---|
| `inline` (o programa está no comando) | 23/33 | **70 %** |
| `tmp` (reconstruído das edições da sessão) | 7/15 | 47 % |
| `atribuido` (base `git show`) | **0/3** | **0 %** |

**Motivo dos 21 não-reproduzidos**, lido do registro de replay — não da memória:

| n | motivo |
|---|---|
| 8 | `rc 0`: o comando termina sem erro — o defeito já foi corrigido no código de hoje |
| 7 | a classe aparece, mas em **outro** frame — o replay não reproduziu o erro *deste* arquivo |
| 4 | falhou por outro motivo, sem traceback |
| 1 | TIMEOUT |
| 1 | ValueError |

## 2. O achado estrutural

**`atribuido` reproduz 0/3.** O corpus sobrepõe **um** arquivo; quando o erro depende de um
módulo irmão, o replay usa a versão **de hoje** desse módulo. Os 8 `rc 0` são exatamente isso.

Não se corrige afrouxando o juiz — corrige-se **sobrepondo também os arquivos de que a chamada
depende** (`contexto`), cada um com a sua base no mesmo rev. É trabalho de construtor.

## 3. Negativos

| | |
|---|---|
| universo (arquivos `.py` editados nos transcripts) | **4.554** |
| candidatos após o filtro | **13.738** |
| sorteados (`random.Random(20261008).sample(candidatos, 300)`) | 300 |
| entraram no corpus | **30** |
| descartados | **270**, todos por "reconstrução quebrada" |
| descartados por "não compila" | 0 |

**A causa real dos 270:** `_conteudo_depois` exige `vivos.project_root(arq)` com `.git`, e a maior
parte do universo vive em `~/.claude/hooks`, `~/.cache` e afins — que não são repositório. O
critério 2 do juiz pede **≥ 300**; hoje há 30.

## 4. Saída literal do juiz

```
entradas 81 · positivos 51 · negativos 30
  FALTA negativos 30 < 300
  FALTA re-sorteio com a semente não devolve os mesmos ids
  FALTA controle positivo do corpus: 30/51 = 59% < 70%
veredito: 3 pendências
rc=1
```

O **critério 5** (re-execução própria do juiz, com e sem o snapshot) **nunca rodou**:
`all_gaps` só chega lá depois de 1–4 passarem. Não há veredito dele.

## 5. Estado do construtor

- `--selftest` **rc 0**, 8 controles: reconstrução · Edit negado fora do snapshot · Edit que não
  casa marca quebrado · dobra de projeto ignora o disco · id determinístico · extração de
  programa inline por heredoc · por `-c` · ausência de programa.
- `ruff`: limpo. `max_cc = 10`.
- tempo de parede do build: **222,3 s** (51 replays em bwrap, `--jobs 4`).

## 6. Dois erros meus, registrados para não repetir

**1. Zero de chave ausente não é zero do fenômeno.** Os primeiros 50 registros marcaram 0/50
reproduzíveis porque passei o item para `replay._verdict_fields` **sem o campo `kind`**, e a
função faz `bool(item.get("kind")) and item["kind"] in out`. O bwrap funcionava e o traceback era
real; o zero vinha do dicionário incompleto. **O sintoma que denunciou:** `frame_in_output`
verdadeiro e `kind_in_output` falso **na mesma saída** — dois campos que medem o mesmo texto não
podem divergir sem que o instrumento esteja errado.

**2. `TypeError` numa comprehension sobre dict é quase sempre a coleção errada, não o elemento.**
`universo_negativos` chamava `_edicoes(r)` com `r` sendo **um registro**, mas `_edicoes` espera a
**lista**: o `for rec in registros` iterava as *chaves* do dict, que são strings, e
`replay._contents` levantava `AttributeError`. Antes disso eu tinha "consertado" o lugar errado
(filtro de `message` string) — verdade e irrelevante: o índice já tinha **0** registros ruins e o
crash continuava.

## 7. Os dois caminhos (a decisão é da coordenadora)

1. **Os 270 negativos** — reconstrução que não dependa de `.git`: usar `rebuild_tmp` também para
   arquivos fora de repositório, ou aceitar arquivos do próprio repositório do `analise` como
   universo.
2. **O `contexto` dos positivos** — sobrepor os arquivos de que a chamada depende, cada um com a
   base no mesmo rev. É o que move `atribuido` de 0/3 e provavelmente tira os 8 `rc 0`.

Os dois mexem no **construtor**, não no juiz.

## 8. Arquivos

Exclusivos meus: `docs/plans/2026-10-08-vgp-motor-base/scripts/corpus_v0d.py` (novo) e
`~/.claude/touring/vgp/corpus_v0d/` (`entradas.jsonl`, `snap/`, `replay_v0d/`,
`negativos_candidatos.json`, `relatorio.json`).

Não toquei: `juiz_v0d.py`, `replay.py`, `vivos.py`, `inventario_erros.py`, `V0d-spec.md`.