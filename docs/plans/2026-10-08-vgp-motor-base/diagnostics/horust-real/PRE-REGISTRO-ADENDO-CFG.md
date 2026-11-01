# HORUST-REAL / estrato cfg — adendo ao pré-registro (estrato DECLARADO À PARTE)

Autora: instrumentos-p1. Autorização: mensagem da coordenadora de 10/10/2026 («Inclua, se der para injetar de forma limpa, um operador
`cfg` por alvo, por exemplo uma definição movida para `#[cfg(test)]` e usada fora dele, confirmado pelo rustc no alvo não-teste. Ele
entra só como estrato DECLARADO à parte, sem contar nas 4 classes»). Hora de `date -u` lida ANTES de escrever este arquivo:
**2026-10-10T17:19:46Z** (hora de gravação e sha256 em `pre-registro-cfg.hora`, escritos logo depois). Este arquivo NÃO é editado depois;
desvios vão para a seção «Desvios» do `RELATORIO.md`. O `PRE-REGISTRO.md` original (sha 45fd89a1…) fica intacto: ele já declarava que o
corpus principal NÃO cobre `#[cfg]` por alvo (seção 1), e este adendo é o estrato que cobre.

**Estado no momento do registro.** Nenhuma entrada cfg foi gerada. Existem: o operador (`operador_cfg.py`) e seus 30 testes sobre
fixtures sintéticas, e UMA medição do tamanho do pool sobre o inventário elegível da rodada 1 (sem gerar entrada, sem cargo): 312 dos
1.237 arquivos elegíveis têm pelo menos um sítio cfg, 195 deles nunca tentados no corpus principal. O pool é declarado aqui porque ele
informa o n pré-registrado abaixo.

## 1. O que o estrato mede, e o que NÃO mede

Mede **recall do motor sobre uma definição que existe no disco mas só sob `#[cfg(test)]`, usada fora dele** — o mecanismo do caso
natural `gates_limpo` (o nome existe, o alvo normal o rejeita). **Não mede** o caso entre arquivos (um `use` em B de um nome movido em A,
E0432): o erro do operador cai no MESMO arquivo da mutação (E0425, chamada de função livre). **Não confirma** que o alvo de teste
compila: o rótulo vem do alvo não-teste; que o alvo de teste compila é consequência da construção (a definição segue existindo sob
cfg), não medida. Não é a distribuição natural.

## 2. Operador `cfg_fn` (código: `operador_cfg.py`, sha no selo)

Mutação: uma linha `#[cfg(test)]` inserida logo acima do `fn`. Sítio = função livre na coluna 0, antes do primeiro `#[cfg(test)]`
do arquivo, fora de `macro_rules!`, com: exatamente DUAS ocorrências do nome no arquivo mascarado (comentário e literal não contam) — a
definição e uma chamada livre (`nome(`, sem `.`, `::`, `$`) — viva e fora do corpo da própria função (recursão exclui); o nome NÃO
aparece em nenhum outro arquivo elegível do mesmo pacote; atributos acima só dos inofensivos (`inline`, `must_use`, `allow`, `expect`,
`deprecated`, `doc`, `track_caller`, `cold`; qualquer outro, p.ex. `cfg`, `test`, `no_mangle`, `cfg_attr`, exclui). Modificadores aceitos:
`pub`, `pub(crate)`, `pub(super)`, `const`, `async`, `unsafe`. Classe `cfg`, código esperado **E0425**, na linha da chamada (+1 se a
chamada vem depois da definição), nomeando a função.

## 3. Fonte, disjunção, seleção

- Mesmo worktree (`touring-wt-horust-real` em `c118aa52`, limpo antes e depois de cada lote) e mesmo `base_check` do corpus principal.
- Arquivos DISJUNTOS de: dev (`corpus_rust_natural`), controle de 130 .rs, fixtures da VRS, **todo arquivo tentado nas rodadas do corpus
  principal e todo arquivo do corpus principal selado (positivos e negativos)**. A disjunção contra o principal é verificada contra o
  `entradas.jsonl` FINAL dele; por isso a seleção cfg só roda depois do `montar` do principal. No máximo 1 injeção por arquivo.
- Semente `horust-real-cfg-20261010`; ordem por hash e round-robin entre pacotes; **80 tentativas** (ou o pool, se menor), em lotes por
  antecadeia de dependência, como no principal. Teto de concentração ≤ 25% por crate sobre os válidos finais (`selecao_com_teto`).

## 4. Oráculo e validade

Igual ao principal: `cargo check --message-format=json --locked` do alvo (lib|bins) do pacote, sempre via `hw-slot --class rust`.
Válida = exatamente 1 diagnóstico de nível erro no arquivo, código E0425, na linha esperada, nomeando a função. Cascata (erro em arquivo
sem injeção, ou mais de um erro no arquivo) = descarte por motivo, contado. Reconfirmação de 10% (mínimo 3), cada uma sozinha, no mesmo
critério. «Abrir 3» (leitura humana) antes de selar.

## 5. n, negativos, selo

- **n mínimo: 20 positivos válidos; máximo 40** (os 40 de menor hash após o teto). Abaixo de 20 o estrato é selado mesmo assim, mas
  declarado **piloto (n<20): sem taxa**.
- **Negativos PAREADOS**: o original de cada positivo (os mesmos bytes antes da mudança), um por positivo, com `pareado_com`. A única
  diferença entre o par é a linha `#[cfg(test)]`. Medem falso alarme no mesmo texto sem o defeito. **Não** incluem o controle «cfg
  correto, usado só no teste» (arquivos naturais com `#[cfg(test)] fn` usada só em teste): fica declarado como não construído.
- Corpus SEPARADO (`corpus-cfg/`, `estrato: horust-real-cfg`, classe `cfg`), selo SEPARADO (`selo-cfg.json`, fora do corpus), conferido com
  `confere_selo.py`. O selo guarda o sha do `PRE-REGISTRO.md` e o deste adendo. Régua estrita, `origem: semeado`. Nada do estrato entra na
  contagem das 4 classes nem no selo do corpus principal.
- **Ninguém roda motor nele.** Quem julga é a coordenadora, uma vez, com o binário integrado final.

## 6. Limitações adicionais (além da seção 1 do pré-registro)

Só funções livres; só E0425; só chamada com parênteses; o pool exclui nomes repetidos em outro arquivo do pacote (conservador); o
alvo de teste não é compilado; os negativos pareados partilham o arquivo com o positivo, então o estrato não serve para estimar
especificidade em arquivos distintos.
