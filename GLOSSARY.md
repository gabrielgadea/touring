# Glossário do domínio — Touring

Termos que nomeiam seams do código. Um termo entra aqui quando um módulo passa a ser
chamado por ele; o código usa o equivalente em inglês indicado.

## Leitura da linha de shell (`touring-hooks-shared::command_policy::lexer`)

**Linha de shell** — o texto inteiro de um comando Bash como chega ao hook, podendo
conter vários comandos simples, pipelines, heredocs e substituições.

**Comando simples** — uma sequência de palavras entre operadores de controle
(`;` `&&` `||` `|` `&` nova linha), já sem redireções.

**Programa efetivo** (*effective program*) — o programa que um comando simples de fato
executa, depois de descartar atribuições de ambiente (`A=1`, `A+=1`) e de atravessar
wrappers (`sudo`, `env`, `timeout`, `nice`, `xargs`…) com suas flags e argumentos.
`A=1 sudo -u x timeout .5 rm -rf d` tem programa efetivo `rm`. Um único módulo responde
isso; todo gate que julga ou classifica um comando consome essa resposta, nunca uma
tabela de wrappers própria.

**Programa efetivo dentro de uma cláusula** — `effective_command_of`: o programa efetivo
depois de ler através dos abridores de cláusula (`do`, `then`, `else`, `elif`, `{`, `!` e a
condição de `if`/`while`/`until`, que é ela mesma um comando simples), com `None` para o que
não executa nada (`for`/`select`/`case`, `done`, `fi`, `esac`, `}`). `for f in *; do cat $f |
head; done` tem os comandos simples `cat` e `head`. `command_of` segue sendo a leitura crua
que os gates e o juiz lexical usam; a chave de lição (`bash_verb`) e a suspeita de pipe
mascarado (`post_bash`) perguntam o programa por esta função, e só por ela.

**Wrapper** — programa cujo papel é executar outro programa recebido como argumento.
A tabela é única e é a **união** do que qualquer consumidor reconhecia: ler através de
mais wrappers só revela o programa, nunca o esconde. Um consumidor que precise ler
menos declara isso como parâmetro com razão nomeada e teste.

**Corpo de heredoc** — os bytes entre `<<TAG` e a linha terminadora, que são dado e não
comando (salvo delimitador sem aspas com substituição `$(…)` dentro). `<<<` é
here-string, não heredoc: não abre corpo.
