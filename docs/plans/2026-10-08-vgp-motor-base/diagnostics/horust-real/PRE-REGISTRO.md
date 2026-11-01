# HORUST-REAL — pré-registro do held-out de RECALL de Rust por erro injetado

Autora: instrumentos-p1. Autorização: mensagem da coordenadora de 10/10/2026 («APROVO o estrato de erro injetado como held-out de
Rust (HORUST-REAL), construído SÓ por você e selado»). Hora de `date -u` lida ANTES de escrever este arquivo: **2026-10-10T14:22:37Z**
(a hora de gravação e o sha256 deste arquivo ficam em `pre-registro.hora`, escrito logo depois). Nenhuma entrada foi gerada antes
deste registro: até aqui existem só o worktree, a contagem de arquivos elegíveis (1.393) e o `cargo check` de base em andamento.
Este arquivo NÃO é editado depois; desvios vão para a seção «Desvios» do `RELATORIO.md`, com hora.

## 1. O que isto mede, e o que NÃO mede (limitação declarada)

Mede **recall do motor VGP sobre erro INJETADO em contexto real**: uma linha de um arquivo `.rs` real do touring, que compila limpo na
base, é alterada de um jeito que o `rustc` rejeita naquela linha, com o resto do projeto intacto no disco. **Não** é a distribuição
natural de erros: não cobre `use super::*`, `#[cfg]` por alvo, crates irmãos ou dependência ausente, arquivos reescritos por Bash,
nem o estado intermediário de uma edição de verdade. O erro é de uso (nome, import, atributo, campo de literal), 1 linha, fora de
`#[cfg(test)]`. A taxa medida aqui NÃO substitui nem estende o recall natural, que continua «não provada, n=0» (regra C). Os
negativos são arquivos INTEIROS intactos (régua estrita), não edições: medem falso alarme em código real que compila.

## 2. Fonte

- Worktree NOVO e destacado: `/home/gabrielgadea/projects/touring-wt-horust-real` em `c118aa524358d42b9bb2aa40b4794b2c5919a5ae`,
  limpo (`git status --porcelain` vazio) antes e depois de CADA rodada. Alvo do cargo:
  `CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/horust-real`. Todo cargo por `hw-slot --class rust --wait 7200`, uma
  chamada por rodada (o slot é solto entre rodadas, para não travar as outras sessões).
- Base registrada: `cargo check --workspace --lib --bins --keep-going --message-format=json` (arquivo `base_check.jsonl`). Um pacote só
  entra se produziu ZERO diagnósticos de nível `error`. Os arquivos compilados de cada alvo (lib ou bin) vêm do dep-info (`*.d`) desse
  check: só arquivo lido pelo rustc conta como «compilado». Arquivo presente em mais de um alvo do mesmo pacote sai (ambíguo).
- **Elegível**: `crates/*/src/**/*.rs`, compilado em um pacote limpo, entre 15 linhas e 200 KB, e FORA de:
  (a) o dev: todo caminho de `~/.claude/touring/vgp/corpus_rust_natural/entradas.jsonl` (141ced89), positivos e negativos;
  (b) o controle de 130 `.rs` do vrs-porta: todo `.rs` de `touring-code`, `touring-identity` e `touring-pyright` em `88c7cc8b`
  (`git ls-tree -r`; conferido contra `varredura_real.py`, que varre exatamente esses 3 crates);
  (c) as fixtures/fontes do VRS e do estrato semeado: `crates/touring-code/src/vgp/**`, `crates/touring-pyright/**`,
  `crates/touring-identity/**`.
  Disjunção por ARQUIVO verificada por interseção de conjuntos (tem de dar vazio; o relatório traz as quatro contagens).
- Estrato 2 (crates do `~/.cargo/registry`, `--offline`): **não será construído** (opcional; fora de escopo desta rodada).

## 3. Operadores (mesmo mapeamento de classe do `corpus_rust_sanidade_v2` e do `corpus_rust_natural`)

No máximo **1 injeção por arquivo**, no total (um arquivo serve a uma classe só). Linhas em comentário, em literal de string, em
atributo `#[..]`, em `macro_rules!` e **depois do primeiro `#[cfg(test)]`** do arquivo não são sítios. O nome novo tem de ser um
identificador que NÃO aparece como token em nenhum `.rs` do crate; se aparecer, tenta outra variante, senão descarta o sítio.
**Estilo do nome novo** (metade das tentativas cada): **near-miss** (distância de edição 1 em 70%, 2 em 30%; operações: apagar um
caractere interno, transpor dois adjacentes, trocar um caractere, inserir um) e **plausível inexistente** (`get_`/`try_`/`_all`/
`_with`/`_info`/`_config`/`_id`/`_count` e equivalentes de módulo/tipo). A escolha da operação e do sufixo é por hash da semente.

| Classe | Operador | Código esperado | Proporção tentada |
|---|---|---|---|
| nome | chamada livre `ident(` (minúscula, sem `.`/`::` antes, fora de `fn`) → nome novo | E0425 | 3 |
| nome | tipo em posição de tipo (`-> T`, `: T`, `<T>`, `&T`, `impl T`; fora de Option/Result/Vec/String/Box/Self) → nome novo | E0412 | 2 |
| import | `use crate::|super::|self::|touring_*::…;` simples (sem `pub`, sem `as`, sem chaves): último ou penúltimo segmento; ou um nome dentro de `use …::{a, b};` | E0432 | 3 |
| import | `Tipo::fn(` ou `modulo::item` em expressão (fora de std/core/alloc/prelude): o primeiro segmento | E0433 | 2 |
| atributo | `.campo` sem `(` → nome novo | E0609 | 1 |
| atributo | `.metodo(` → nome novo | E0599 | 1 |
| kwarg | linha `campo: expr,` de literal de struct de várias linhas → renomeia a chave | E0560 | 2 |
| kwarg | a mesma linha de campo é REMOVIDA (literal sem `..base`, mais de um campo) | E0063 | 1 |

`nome_esperado`: o nome novo, exceto E0063, cujo `nome_esperado` é o TIPO do literal (convenção do `corpus_rust_natural`). A
mensagem gravada é a do rustc (`error[E….]: …`), para o juiz classificar como símbolo.

## 4. Oráculo = rustc, e o que vale como positivo

Uma **rodada** = um pacote, um alvo (lib ou bin) e uma FASE: fase R (resolução: nome + import) ou fase T (tipos: atributo + kwarg), com
até 1 injeção por arquivo, todas aplicadas juntas, um `cargo check -p P --lib|--bins --keep-going --message-format=json`. Contrato
de oráculo próprio (`cargo_json`: stream `stdout`, sinal = mensagem `build-finished`; `rc=0` sem sinal é erro nomeado, nunca «limpo»),
pelo `chama_oraculo` sem editá-lo. Diagnóstico = nível `error` com span primário; dedupe por (arquivo, linha, coluna, código).
**Válido** se e só se, no arquivo injetado, há EXATAMENTE 1 diagnóstico: código == o esperado do operador; na linha da mutação (E0063:
numa linha de 0 a 60 antes dela); com o nome novo (E0063: o tipo) entre crases na mensagem. Qualquer outro erro no arquivo = cascata =
descarte. Erro em arquivo SEM injeção na rodada = não atribuível → a rodada é partida ao meio e repetida (recursivo; uma injeção só que
ainda cascateia é descartada). Descartes contados por motivo: `sem_sitio`, `nome_existe_no_crate`, `sem_erro`, `codigo_inesperado`,
`linha_diferente`, `outro_erro_no_arquivo`, `cascata_cruzada`, `nome_fora_da_mensagem`, `oraculo_falhou`. `rc=70` do hw-slot
(machine check) repete a rodada uma vez. Restauração por SHA dos bytes originais (nunca `git checkout`); sha divergente aborta tudo.
**Reconfirmação independente**: uma amostra de 10% por classe (mínimo 3) é repetida SOZINHA, uma injeção por `cargo check`; tem de
dar o mesmo diagnóstico, senão o lote inteiro da classe é suspeito e o relatório diz.

## 5. Tamanhos, rodadas e tetos

- Tentativas por classe por rodada de geração: **72**. Se uma classe terminar com menos de **20** válidos, nova rodada de geração
  (sementes `…-r2`, `…-r3`), no máximo 3. Se ainda assim ficar abaixo de 20, entrego o n real e a classe sai «não provada, n=X»; não
  completo com outra coisa.
- **Teto de concentração**: no máximo 25% dos positivos de uma classe vêm de um mesmo crate (maior N tal que nenhum crate passe de
  floor(0,25·N); o excedente sai por ordem de hash). Sorteio das tentativas em round-robin entre pacotes, ordenados por hash.
- Semente: `horust-real-20261010`; toda ordem é `sha256(semente|chave)`. IDs: `sha256(rel|classe|linha|nome_novo|semente)[:12]`.
- **Negativos: no mínimo 320** (alvo 340): arquivos reais INTACTOS, elegíveis (seção 2), de pacotes limpos, **disjuntos de todo arquivo
  tentado** (não só dos válidos), round-robin entre pacotes, teto de 25% por crate (se inviável para 320, subo para 30% e registro
  como desvio nomeado).

## 6. Formato e regime do juiz

Corpus em `~/.claude/touring/vgp/corpus_horust_real_20261010/` (`entradas.jsonl`, `snap/<id>.rs`, `base/<id>.rs` só dos positivos,
`holdout_meta.json`). Campos como os do juiz atual: `rotulo`, `reproduz:"true"`, `lang:"rust"`, `origem:"semeado"` (balde arquivo),
`classe`, `codigo_esperado`, `nome_esperado`, `path` (caminho absoluto do arquivo REAL no worktree), `project_root` (o worktree),
`snap`, `mensagem`, `contexto:"[]"`, mais `crate`, `operador`, `estilo`, `linha`, `sha_original`. **Sem** `estado_no_instante/manifesto.json`:
régua ESTRITA (arquivo inteiro), sem `--disk-file`; quem quiser o regime de delta tem o original em `base/`. O worktree tem de existir
e estar em `c118aa52` quando o juiz rodar. Selo FORA do corpus em `diagnostics/horust-real/selo.json`: `sha_tree`, `arquivos`, `bytes`,
`entradas_jsonl_sha256`, `holdout_meta_sha256`, o sha256 DESTE arquivo, o sha256 do gerador, e HEAD+tree do worktree;
conferido por `scripts/confere_selo.py` antes e depois. **Ninguém roda motor nem juiz nele.** Quem julga é a coordenadora, uma vez.

## 7. Verificações antes de selar (todas viram linha do relatório)

(1) interseção vazia com dev, controle de 130 e fixtures VRS; (2) cada positivo reconfirmado no rustc (rodada + amostra sozinha);
(3) «abrir 3» por classe, à mão, lendo a linha injetada, a mensagem do rustc e o original (leitura, nunca motor); (4) o gerador com
testes e mutantes (meta: todos os mutantes de lógica mortos; os sobreviventes nomeados); (5) determinismo: a lista de tentativas
(sem o oráculo) reproduz byte a byte a partir dos mesmos insumos; (6) worktree limpo e em `c118aa52` ao fim; (7) n por classe,
por crate, por código e por estilo, com os descartes por motivo.

## 8. O que NÃO farei

Não rodo motor nem juiz no corpus. Não toco em `corpus_rust_natural/`, em `B/scripts/corpus_*.py` nem em `crates/**` do worktree fora
da rodada (e ali só com restauração por SHA). Não construo o estrato de registry. Não completo classe com n<20 com outra fonte.


---

# ADENDOS (acrescentados ao fim; os primeiros 10.097 bytes acima são o pré-registro original, sha256 45fd89a1…, verificável pelo prefixo)

Regra de método da coordenadora (mensagens de 10/10/2026): mudança de implementação que faz o código cumprir o §3/§4 como escritos NÃO
precisa de adendo (vai listada no `RELATORIO.md`); QUALQUER mudança do que conta como VÁLIDO ou ELEGÍVEL (aceitar outro código, mudar
sítio, operador ou proporção, reaproveitar descartados) vira «Adendo N» aqui, datado, com motivo. Esta regra vale a partir deste
acréscimo e substitui a frase «Este arquivo NÃO é editado depois» do cabeçalho: o original continua íntegro (é o prefixo), e o selo
carrega o sha do original e o do arquivo com os adendos.

## Adendo 1 — mudanças de validade/elegibilidade aplicadas DEPOIS da rodada 1 (RETROATIVO)

**Hora deste registro: 2026-10-10T22:05:25Z (`date -u`).** Foi escrito DEPOIS da aplicação, não antes: a rodada 2 e a revalidação rodaram e terminaram às
2026-10-10T17:19:42Z (mtime de `estado/resultados.jsonl` e de `estado/lotes_e.log`), e a reconfirmação às 2026-10-10T18:04:10Z
(`estado/reconfirmar.log`). A regra chegou depois. Logo, este adendo NÃO protege contra escolha informada pelo resultado da rodada 1: as
três mudanças abaixo foram decididas OLHANDO o diagnóstico da rodada 1 (`estado/diagnostico_operador.{log,json}`). O que ele garante é
que a leitura posterior saiba exatamente o que foi feito. **Nenhum motor, nenhum juiz e nenhum VGP rodou** neste corpus em momento algum.

Resultado da rodada 1 que motivou tudo (regras originais, 288 tentativas): `nome_tipo` 0 de 29 válidos, porque o rustc deste worktree
responde **E0425** («cannot find type») e não E0412 a um tipo ausente; `import_use` 9 válidos de 29, porque mutar o NOME importado
(último segmento) deixa as outras ocorrências dele no arquivo sem import e o rustc acusa E0425/E0412 nelas (cascata);
`import_use_chaves` 0 de 14, pelo mesmo motivo.

**1.1 Aceitar E0425 além de E0412 em `nome_tipo` (§3, linha 2 da tabela; §4 «código == o esperado»).** O código esperado do operador passa a
ser «E0425 ou E0412» (`operadores_rust.ACEITOS`). O código gravado no corpus é o que o rustc deu. A classe `nome` já era E0425/E0412 no
mapeamento do `corpus_rust_sanidade_v2`; o que muda é o critério de VALIDADE do operador. Tipo ausente que o rustc classifica como
trait (E0405, p.ex. `Default -> RawDefault`) continua descartado por `codigo_inesperado`.

**1.2 Rodada 2 de REPARO, fora do gatilho do §5.** O §5 só prevê nova rodada de geração para classe com menos de 20 válidos; nenhuma
classe estava abaixo (tabela A). A rodada 2 (semente `horust-real-20261010-r2`, 100 tentativas: 40 `nome_tipo` + 60 `import_use`) gerou
só os dois operadores quebrados, e em `import_use` só o **penúltimo segmento** (o módulo), que o §3 já permitia («último ou
penúltimo») e que não cascateia: o rustc liga um símbolo-fantasma ao import que falhou. Mudou a proporção tentada (os outros 6
operadores não rodaram na rodada 2) e o sítio (só o penúltimo). Arquivos da rodada 1 não se repetem (1 injeção por arquivo no total).

**1.3 Revalidação de descartes (reaproveitamento).** As 22 injeções de `nome_tipo` que a rodada 1 descartou por `codigo_inesperado` foram
REFEITAS, as mesmas injeções (mesmo id, mesmo arquivo, mesmo texto), com o critério de 1.1; 17 passaram a válidas (E0425), 5 continuam
descartadas (E0405). O comando `revalidar` também re-rodou 6 descartes `codigo_inesperado` da própria rodada 2 (E0405), sem mudança.
O registro novo vale (o último de cada id), e o arquivo `resultados.jsonl` é append-only: o histórico fica.

**1.4 Procedimento (não muda o que é válido).** O §4 define a rodada como «um pacote, um alvo, uma fase». Foi executada assim: um
`cargo check` por (grupo de pacotes SEM relação de dependência, direta ou transitiva, alvo, fase), com vários grupos por posse do
`hw-slot`. A validade continua por arquivo; um erro em arquivo sem injeção parte o lote ao meio, como no §4; cargo sempre com `--locked`.
Declarado aqui porque o texto original dizia «um pacote».

**1.5 Efeito medido no n (válidos por classe).**

| | kwarg | atributo | import | nome |
|---|---|---|---|---|
| A. Regras originais, só rodada 1 | 53 | 57 | 36 | 38 |
| + 1.3 revalidação (17 `nome_tipo`) | 53 | 57 | 36 | 55 |
| + 1.2 rodada 2 (50 `import_use` + 27 `nome_tipo`) | 53 | 57 | 86 | 82 |

Todas as classes têm n ≥ 20 só com as regras originais: o reparo NÃO foi necessário para chegar ao piso. Ele muda a COMPOSIÇÃO:
`import` passa de 9 E0432 e 27 E0433 (proporção §3: 3 para 2) para 59 E0432 e 27 E0433; `nome` passa de só `nome_valor` (E0425) para
`nome_valor` + `nome_tipo`. Para a leitura sobre o corpus de regras originais, o `holdout_meta.json` lista os ids por regra
(`ids_regra_original`, `ids_regra_adendo`); o corpus selado traz os dois grupos.

## Adendo 2 — estrato `cfg` (separado, não conta nas 4 classes)

Registrado ANTES de gerar qualquer entrada dele: arquivo `PRE-REGISTRO-ADENDO-CFG.md`, hora de `date -u` **2026-10-10T17:19:46Z**
(escrita em `pre-registro-cfg.hora`: 17:20:12Z), sha256 `590b00f306b69f571665b29fcc0db04f8d97b51a2ef34090e7af223dc3e59d0e`.
Corpus e selo próprios; o selo do estrato carrega o sha do original (45fd89a1…), o do arquivo com adendos e o do adendo cfg.
