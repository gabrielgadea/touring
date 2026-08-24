---
okf_version: "1.0"
type: Strategy
title: "Um detector que pode dizer 'não sei', e uma memória que grava onde não lê"
description: "O detector de escrita passa a ser fail-closed e a enxergar a escrita feita dentro de um programa; o mem-vazio é reclassificado por evidência de disco de 'recall degradado' para assimetria de raiz entre writer e reader"
tags: [adw, code-mode, deteccao-de-escrita, memoria, raiz-de-projeto]
timestamp: 2026-08-24T16:05:00-03:00
plan_id: 2026-08-24-work-outer
scope: /home/gabrielgadea/projects/touring
---

# Estratégia — dois itens abertos, fechados por medição

## O fio comum

Os dois itens deste turno tinham a mesma forma, e é ela que vale registrar:
**uma afirmação plausível ocupando o lugar de uma medição.** Num caso, um
detector concluía "leitura" porque não via escrita. No outro, um comentário de
teste atribuía uma falha ao subsistema errado havia meses. Nenhum dos dois estava
mentindo — os dois estavam *inferindo*, e ninguém tinha exercido o caminho que
revelaria o erro.

## Item 1 — o detector de escrita

### O erro de fundo não foi o padrão que faltou

O detector procurava `>`, `rm`, `sed -i`. É tentador descrever a falha como
"faltou um padrão na lista". Não é isso. O comando que escapou foi:

```
python3 ~/.claude/.../loop_marker.py write --task OUTER ...
```

Nenhum padrão de sintaxe de shell jamais o pegaria, porque a escrita **não está
na linha** — está dentro do programa. Nenhuma lista de padrões de shell é longa
o bastante para cobrir isso. O defeito era o método: **inferir o caso seguro a
partir da ausência de evidência do caso perigoso.**

### A correção

`command_writes` (`adw.py`) devolve três valores, e o terceiro é o ponto:

| valor | significado | quando |
| --- | --- | --- |
| `True` | grava comprovadamente | redirecionamento para arquivo, comando mutante, **verbo em posição de subcomando** |
| `False` | leitura comprovada | TODA palavra executada é conhecidamente de leitura |
| `None` | não dá para decidir | qualquer programa opaco — **nunca** rebaixado a `False` |

A metade que faltava é o **verbo em posição de subcomando** (`write`, `store`,
`commit`): é assim que a escrita se declara quando não há sintaxe de shell.

Dois falsos-positivos foram evitados de propósito, e ambos importam porque cada
um teria disparado em nós reais desta biblioteca:

- `2>&1` redireciona descritor, não grava — abre quase todo nó da biblioteca;
- `-o` ficou **fora** das flags de escrita porque `set -o pipefail` abre três nós.

Um detector que acusasse esses seria ruído com cara de rigor.

### O enforcement mora no executor

`_lint_readonly_claim` consome o detector. `readonly = true` contradito pelo
comando é **erro** — exit 1, verificado no binário vivo, não no harness. O aviso
do caso improvável dispara só onde a cegueira morava (script ou interpretador),
nunca sobre chamadas documentadas do CLI: punir o spec correto é exatamente como
um lint perde a atenção de que vai precisar quando o achado for real.

Isto é o D8 aplicado ao próprio incidente: o `arm_marker` **já documentava** a
limitação num comentário preciso. Descrever não é aplicar — nada verificava o
comentário, e ele envelheceu com uma afirmação falsa ao lado.

### O que a varredura mediu

| classe | nós |
| --- | --- |
| grava | **1** — `strategy-loop:arm_marker`, o caso que escapou |
| leitura provada | 1 |
| indecidível | 17 |

Ancorado por guarda estrutural sobre a **família**, com piso de 15 nós para não
passar a vácuo se a biblioteca sumir. 201 testes verdes.

## Item 2 — `mem-vazio` não era o que o nome dizia

### O que estava registrado, e por que era plausível

O registro dizia "memória degradada em projeto sem corpus: `RRF fusion from
1 sources`". Isso é uma descrição fiel do **sintoma** — e aponta para o motor de
recall, que está correto.

### O que a medição mostrou

Mesmo diretório, mesmo binário, mesma sessão:

| operação | DB de destino | raiz |
| --- | --- | --- |
| `touring diary write` | `<cwd>/.claude/touring/memory.db` | **cwd** |
| `touring memory store` | `~/.claude/touring/memory.db` | **HOME** |
| `touring memory recall` | primary(HOME) + tudo sob `~/.claude/**` | **HOME** |

`discover_canonical_dbs` (`crates/touring-cli/src/cli/shared.rs:368`) federa
`primary` mais as raízes sob `~/.claude`. Uma DB em `/tmp/…/proj/.claude/` não
está em nenhuma das duas. **O diário cai numa DB que o recall nunca abre.**

O sintoma é da pior espécie que um sistema de memória pode ter: a consulta pelo
termo exato devolve 8 resultados confiantes e **nenhum contém o termo**. O tell
está no próprio JSON — `source_db: null` em todas as entradas significa que a
fonte lexical não respondeu; tudo veio da ANN, do corpus global.

Em `~/projects/touring` funciona porque cwd e fallback-HOME coincidem com a raiz
do daemon. Foi essa coincidência que sustentou a leitura "coisa de projeto vazio".

### A correção (o dilema era falso)

Eu tinha apresentado isto como escolha de arquitetura entre mover o writer ou o
reader. **Não era.** Ler o código desfez o dilema em uma linha:

```rust
let project_root = std::env::current_dir()?;   // diary.rs, em CINCO subcomandos
```

O diário é o **único** subsistema que enraíza no cwd cru. Todo o resto usa
`TouringConfig::normalize_project_root`, cuja documentação nomeia exatamente
este dano: *"every working directory spawned its own stray `.claude/touring/`
shard — a classe das 29 DBs órfãs"*. Dois sítios já haviam recebido o remédio
(`daemon_client.rs`, `handlers/mcp.rs`), cada um com um comentário explicando a
lição. **O diário era o membro que nunca a recebeu.**

Não havia dois candidatos legítimos: havia um outlier. A correção é adotar o
contrato que já existe — não inventar política nova.

Detalhe que decide a correção: `.claude/` **não** é marcador de projeto na
normalização. Tratá-lo como marcador foi o que criou as órfãs — e teria feito
este conserto recriá-las, já que a primeira escrita do diário cria o `.claude/`.

Corrigido nos **cinco** sítios com uma definição só. Corrigir apenas o `write`
faria escrita e leitura do diário discordarem entre si — pior que o defeito.

### A prova

| verificação | resultado |
| --- | --- |
| repro exato que falhava | `achou: 1`, `RRF fusion from 3 sources` (era `1 sources`, `achou: 0`) |
| DB órfã criada no diretório sem marcador | nenhuma |
| projeto real (não-regressão) | segue em `projects/touring/.claude/touring/memory.db` |
| `test_diary_fts5_searchable` (estava `#[ignore]`) | **passa** — suíte 9/9, 0 ignorados |

O teste estava ignorado *por causa deste defeito*. Ele voltar ao verde é a prova
mais forte disponível, porque não fui eu que escolhi o critério.

### Guardas de regressão

`cli::diary::diary_root_tests`: uma guarda **estrutural** sobre os cinco sítios
(varre o fonte, piso de 5 usos para não passar a vácuo) e as duas direções da
propriedade — sem marcador cai no HOME, dentro de projeto continua no projeto.

A guarda estrutural falhou de verdade na primeira execução, por um motivo que
vale registrar: ela varre o arquivo que a contém, e o literal do padrão
proibido estava **dentro dela mesma**. A agulha passou a ser montada em tempo de
execução. Um guard auto-referente não pode carregar a própria agulha.

### O que sobrou, e não estou escondendo

- Uma DB órfã histórica segue no disco com 4 entradas de 28/06:
  `~/.claude/hooks/.claude/touring/memory.db`. **Não apaguei** — é dado, e apagar
  dado do Gabriel sem perguntar não é minha decisão. Migrar ou remover é um
  comando; diga qual.
- `activity.rs::store_path` pertence à mesma classe (chaveia `activity.jsonl`
  pelo cwd cru). Varri o disco: os 5 arquivos existentes estão todos em raízes
  legítimas, então é **latente, não observado** — registro sem corrigir, porque
  não vou mexer num subsistema onde não medi falha.
- `cortex/runtime.rs` e `impls_hook.rs` reimplementam `detect_project_root` com
  só 2 dos 4 passos da cadeia canônica (sem `TOURING_PROJECT_ROOT`, sem checagem
  de marcador, sem blacklist de `/tmp`). Drift real, fora do escopo deste turno.

## A afirmação que eu tinha escrito e estava errada

O comentário do `arm_marker` dizia que sandboxá-lo "trocaria a garantia por
observabilidade" — isto é, que a escrita quebraria sob o sandbox. **Medido: a
escrita passa.** A aplicação do CEG para shell é advisory e o landlock vigente
permite estes caminhos. Eu havia documentado como fato uma suposição que nunca
exercitei, no mesmo dia em que corrigia esse padrão em outros lugares.

A prova veio de graça: o nó `diagnose`, que é sandboxado e escreve o diagnóstico
OKF, passou nesta execução — o artefato está em disco.

## O que fica

Nada mascarado. O `mem-vazio` está aberto por decisão explícita e com a evidência
na mão; o detector está corrigido no executor e guardado por teste de família.
