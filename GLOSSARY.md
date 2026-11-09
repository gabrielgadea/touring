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

**Wrapper** — programa cujo papel é executar outro programa recebido como argumento.
A tabela é única e é a **união** do que qualquer consumidor reconhecia: ler através de
mais wrappers só revela o programa, nunca o esconde. Um consumidor que precise ler
menos declara isso como parâmetro com razão nomeada e teste.

**Corpo de heredoc** — os bytes entre `<<TAG` e a linha terminadora, que são dado e não
comando (salvo delimitador sem aspas com substituição `$(…)` dentro). `<<<` é
here-string, não heredoc: não abre corpo.
