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

### Por que não corrigi

Mudar a resolução de raiz toca todos os projetos pinados. A escolha entre "o
`diary` passa a enraizar como o `recall`" e "o `recall` passa a federar o cwd" é
decisão de arquitetura, não conserto — e a ordem vigente é não causar regressão
em nenhum projeto. Fica registrada com a evidência que a torna decidível, e o
comentário forense do teste `e2e_diary.rs` foi corrigido para parar de apontar
para o subsistema errado.

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
