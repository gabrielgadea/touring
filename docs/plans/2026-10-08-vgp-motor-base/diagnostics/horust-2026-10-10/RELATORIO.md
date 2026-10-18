# HORUST — medir antes de construir (10/10/2026)

Autora: instrumentos-p1. Nada foi selado, nenhum corpus de produção foi construído, `corpus_rust_natural/` não foi tocado
(`entradas.jsonl` continua `141ced89653b0413`). Nenhum motor rodou em held-out (não existe held-out Rust, ver 3).

## 1. Resposta curta

- **Held-out Rust independente: 0 positivos, em qualquer classe, nas duas janelas pedidas e até agora (13:31Z).** A janela ampliada
  (até 10/10 00:00Z) e o "até agora" acrescentam **0** positivos fora das sessões do plano VGP. Pela regra C, toda classe fica
  «não provada, n=0».
- O que sobra é o DEV: **26 positivos** (todas as sessões) / **19** fora das sessões VGP, todos com ts <= corte 2026-10-09T20:40Z,
  logo já medidos pela a6 e pelo vrs-porta. Servem de evidência de desenvolvimento, não de prova.
- O overlay de crate **não recuperou nenhum** dos positivos que o motor perde (0/16 em modo crate, 0/13 em modo workspace), sem
  regressão e com 0 falso deny. A causa não é o overlay ser ignorado (controle positivo na seção 4): é o que está em volta (seção 4).

## 2. Funil por classe (gerador -> sem classe A -> crate completo)

Universo atual (corte 2026-10-09T20:40Z). `SOBRAM` = crate completo no instante E fora da classe A.

| Janela / sessões | nome | import | atributo | kwarg | total |
|---|---|---|---|---|---|
| (a) todas, gerador hoje | 20 | 13 | 3 | 1 | 37 |
| (a) todas, crate completo | 12 | 11 | 3 | 1 | 27 |
| **(a) todas, SOBRAM (= DEV)** | **11** | **11** | **3** | **1** | **26** |
| (a) sem sessões VGP, gerador hoje | 17 | 4 | 3 | 1 | 25 |
| **(a) sem sessões VGP, SOBRAM** | **11** | **4** | **3** | **1** | **19** |
| **(b) ampliado a 10/10 00:00Z, sem sessões VGP, SOBRAM** | **11** | **4** | **3** | **1** | **19** |
| (b) idem, ts > corte A (o held-out candidato) | 0 | 0 | 0 | 0 | **0** |

- Contas das saídas, universo todas (37 -> 26): classe A **6** («item declarado no módulo do uso»), INCOMPLETA **10**
  (`dobra_quebrada` 5 + `entrada_fora_da_arvore` 5 por prioridade), interseção 5, união 11. Sem sessões VGP (25 -> 19): classe A 6,
  INCOMPLETA 5, interseção 5, união 6.
- Sessões VGP excluídas: 28 (esta sessão e subagentes, touring-4f, touring-a6 e todo transcript que cita `vgp-motor-base`;
  lista em `dados/sessoes_vgp.json`). Elas respondem por 7 dos 26 positivos (os 7 `import` do plano).
- Sensibilidades sobre os 26 do DEV: sem nenhum arquivo do crate reescrito por Bash = **19** (nome 5); com crates irmãos citados
  também completos = **23**; as duas juntas = **16** (nome 2). Entradas que citam outro crate do workspace: **29/37**.
- Independência: os 19 fora do VGP vêm de **6 sessões**, **10 arquivos**, **14 pares (arquivo, nome)**; 18 em `touring` e 1 em `analise`.

## 3. Por que o held-out está vazio, e os negativos

- Positivos nascem de resultados Bash com `error[E…]`; o intervalo (corte A, hoje 13:31Z] tem 0 fora das sessões VGP. Não é o
  reconhecedor: ele só afeta negativos.
- **Reconhecedor de `cargo` do gerador perde 47-48% das chamadas desde a W38** (`hw-slot --wait N`, `env -u`, `taskset`, `bash -c`).
  Escrevi `reconhecedor_cargo_estendido.py` (por cima, sem editar o gerador; 23 testes, 11/11 mutantes). Efeito: negativos
  materializáveis no universo atual 307 -> 377 (usáveis 275 -> 342); positivos inalterados (37/25).
- Negativos fora do DEV e fora das sessões VGP (qualquer data): **37 usáveis**. Para um falso deny com CP95 <= 1% com 0 falsos
  precisa de n >= ~300. Hoje o pool independente é ~8x menor.

## 4. O overlay de crate no motor (DEV, mesmo binário vrs-porta 42830beb, mesmos ids, com e sem overlay)

| Modo | positivos | achados antes | achados depois | perdidos | recuperados | negativos | deny antes/depois |
|---|---|---|---|---|---|---|---|
| crate | 26 | 10 | 10 | 16 | **0** | 275 | 0 / 0 |
| workspace | 23 | 10 | 10 | 13 | **0** | 248 | 0 / 0 |

- **Controle positivo** (fixture sintética, binário congelado): `lib.rs` usa `crate::m::g`, disco tem `g` em `m.rs`. Sem overlay,
  `allow`; com `--overlay m.rs=<só h>`, `warn g`. O motor honra o overlay de módulo em Rust; o 0/16 não é overlay ignorado.
- Causa dos 16 perdidos (heurística textual, ordenada; `classificar_perdidos.py`, dados em `dados_dev_*.perdas.json`):

| Causa | crate | ws | O que o overlay de crate pode fazer |
|---|---|---|---|
| uso nu em arquivo com `use super::*` (motor abstém) | 5 | 4 | nada; é limite do motor (`use super::*` não resolvido) |
| caminho/dependência de OUTRO crate (`touring_hooks::…::compose_details` x2, `touring_intelligence::…::retirement` x2, `use serial_test::…`) | 5 | 3 | nada no modo crate; no modo ws os que dependem de crate irmão irreconstruível saem por INCOMPLETA (26 -> 23 positivos) |
| `crate::cli_handlers_decompose::…` x3: o overlay DEFINE a função no instante e o rustc disse «não achei» | 3 | 3 | nada; arquivo reescrito por Bash, a dobra diverge do compilado (mesma natureza da classe A, mas qualificado, que a regra A exclui por desenho) |
| `use crate::…::{.., pending_for, ..}` | 1 | 1 | idem |
| método `.with(` (tipo de outro módulo/crate) | 1 | 1 | nada |
| `Self::` fora de `impl` (erro de contexto, não de módulo) | 1 | 1 | nada |

  Fatores sobrepostos nos 16 (crate): `cross_crate` 15, `bash_crate` 7, `nome_no_overlay` 7. São sinais, não veredito; a coluna
  acima é a primeira causa que casa.

## 4b. Controle positivo pedido: os 4 `use super::super::gates_limpo;` (ids a3a8e6e2370c, 63107c51d5ed, cf87e1831c66, 455b05e16490)

**O estado do crate no instante NÃO restaura esses 4.** Medido com o motor da VRS que resolve certo, em duas condições (disco de
hoje, sem overlay; instante, com os 78 `.rs` do crate sobrepostos), `controle_gates_limpo.py` + `comparar_disco_instante.py`:

| Binário | disco de hoje acusa `gates_limpo` | instante (crate) acusa `gates_limpo` |
|---|---|---|
| `42830beb` (vrs-porta anterior) | 4/4 | 4/4 (acerto por posição de arquivo, não por resolução: não separa disco de instante) |
| **V12 `80e3df1c`** (`target/wt-vrs2-var/V12`) | 0/4 | **0/4** |
| V13 `cc8ce548` (`target/wt-vrs2-var/V13`, o mais novo) | 0/4 | **0/4** |
| `aca22e42` (implantado) e `70e0051a` | 0/4 (`allow`) | 0/4 (`allow`) |
| `ec542c4b`, `6d147ea0`, `6715d1b8`, `8dcf0b3e`, `68f8aa70`, `49830a73` | `unknown` («adaptador rust ainda não implementado») | idem; não medem Rust |

**Por que o instante não os acusa (e por que a premissa «gates_limpo só nasceu no dfe0fd1e» não vale para o instante):** o
`gates_limpo` estava no `cli_suggester.rs` do disco desde **22:58:31Z** de 08/10, pelo `Edit` `UK4mc0vc` da própria sessão
`8614d1f9`, como `#[cfg(test)] pub(crate) fn gates_limpo` (18 min antes do cargo que reprovou). O commit `dfe0fd1e` é só a data em que
o git o registrou. O estado reconstruído tem exatamente isso (L6379 `#[cfg(test)]`, L6380 `fn gates_limpo`) e tem o `mod tests;`
em L6416 **sem** `#[cfg(test)]`. O E0432 («no `gates_limpo` in `cli_suggester`», cargo 23:16:22Z e de novo 23:19:18Z) vem do alvo
**não-teste** do `--all-targets`, que compila o arquivo de testes sem o helper. O conserto foi o `Edit` `iTcELJ9i` às 23:20:47Z
(`#[cfg(test)]` no `mod tests`), e o cargo das 23:21:04Z deu 0 erros. Ou seja: o rótulo é **real e reproduzível**, mas por
**semântica de `cfg` por alvo**, não por estado de arquivo. Nenhum overlay de arquivo o restaura; só um motor que avalie `cfg`.

**Quantos dos 37 estão na mesma situação** (estado do instante já define o nome; o rustc disse «não achei»), proxy textual
`situacao_gates_limpo.py` sobre os 26 que ficam no corpus, mais os 11 que saem:

| Situação | n | detalhe |
|---|---|---|
| definição sob `#[cfg(..)]` no instante (a dos `gates_limpo`) | **6** | `gates_limpo` x4, `burst_gate`, `pending_for` |
| definição no instante, sem `cfg` | 4 | `Capability` (`use super::*`), `cli_decompose_finalize`/`_ready`, `refresh_parent_status` (arquivos reescritos por Bash) |
| classe A (definido no módulo do uso; saem do corpus) | 6 | regra 2 |
| nome ausente no instante, presente hoje (**a célula da hipótese**) | 6 | `is_automated_payload` x3, `cli_index_ingest` (todos `use super::*`, o motor não resolve), `source_declares`, `star_target_names` (V12 já acusa no disco de hoje: o disco não apagou o rótulo) |
| nome nunca esteve neste crate | 10 | outro crate/dependência, `Self`, método, atributo |
| INCOMPLETA (fora do corpus, sem estado) | 5 | n/a |

Soma 37. Onde o instante define o nome e o rustc rejeitou: 6 (cfg) + 4 + 6 (classe A) = **16 de 37** (43%). V12/V13, mesmo ids,
disco vs instante nos 26: `restaurado` **0**, `igual_acha` 6, `igual_perde` 20, `piorou` 0; negativos (275): deny 0/0, avisos
**101 -> 95** (6 avisos a menos em código que compila; contagem líquida, não conferi se são os mesmos arquivos). Recall de nome do V12 no DEV: 6/26 nas duas condições; os
10/26 do `42830beb` incluíam os 4 `gates_limpo` por acerto de posição.

## 5. Proposta de divisão dev/held-out

1. **DEV = `corpus_rust_natural/` atual** (a6 e vrs-porta mediram nele). Com 1+2 aplicadas, o DEV honesto tem 26 (19 fora do VGP).
2. **Held-out natural hoje: vazio.** Para reabri-lo basta atividade Rust nova **fora** do plano VGP depois de 10/10 13:31Z; o
   gerador e o reconhecedor estendido já medem isso (`medir_horust.py`, ~3 min). Sugiro um corte de selagem só quando houver
   n >= 20 **por classe** (hoje o teto é nome 11, import 4, atributo 3, kwarg 1): então qualquer veredito de classe Rust será
   «não provada, n=X» por bastante tempo.
3. **Estrato de erro injetado** (`corpus_rust_real.py`, rustc como oráculo) é a única fonte que pode ter n >= 20 por classe; não
   está em disco, não o inventei. É decisão sua se vale construir.
4. Até lá, os números do DEV valem como desenvolvimento. Não digo que o overlay "melhora" ou "não melhora" o motor em prova:
   diz-se que **não moveu o recall do DEV** e **não causou falso deny nos 275 negativos**.

## 6. Decisões e riscos

- Mudar o motor antes de esperar recall do overlay: (i) `use super::*`, (ii) estado de crates irmãos/dependências (`Cargo.toml`
  do instante), (iii) guarda para arquivo reescrito por Bash (a dobra de sessão não é confiável nesses arquivos: 7 dos 16).
- A regra A que escrevi exclui uso qualificado de propósito (o rustc procura no módulo nomeado); por isso 3 casos «definido e
  não encontrado» não saem como classe A. Se quiser ampliar a regra para «o overlay do módulo nomeado define o nome», é um pedido
  novo e precisa de TDD próprio.
- Dívida de complexidade nos instrumentos: `estado_rust_no_instante.py` ~650 linhas (54 testes, 29/29 mutantes);
  `classificar_perdidos.py` é diagnóstico **sem teste** (heurística textual; os números da seção 4 dependem dela).
- O hook pós-escrita acusou falha de `cargo check(touring-code)` em `typescript/escopo.rs`: edição de outra sessão, não minha;
  registro aqui porque a REGRA #21 não aceita ignorar.

## 7. Evidência e identidade

- Testes: `pytest test_estado_rust_no_instante.py test_reconhecedor_cargo_estendido.py` = **77 passed** (hw-slot, rc 0).
- sha256 (16 primeiros): `dados/medicao.json` ba2f2724… · `dados/itens.json` b0cdeafb… · `dados_estendido/medicao.json` e04538f7… ·
  `dados_estendido/negativos_janela.json` 0a01c010… · `dados_dev_crate.comparacao.json` f7473b27… ·
  `dados_dev_ws.comparacao.json` 7111aa87… · `estado_rust_no_instante.py` 4ee2d523… · `reconhecedor_cargo_estendido.py` 8c5bb9df… ·
  `classificar_perdidos.py` e64a0d01… · `controle_gates_limpo.py` f2652db8… · `comparar_disco_instante.py` 6c1d2d6d… ·
  `situacao_gates_limpo.py` 8dfcfb04… · `dados_V12_comparacao.json` 11983dd5… · `dados_controle_gates_limpo_V12_V13.json` 5803cc46… ·
  `dados_situacao_instante_x_hoje.json` 0fab0586… · `corpus_rust_natural/entradas.jsonl` 141ced89… (inalterado).
  Binários medidos na 4b (só execução, não copiados): V12 `80e3df1c3ce980b7` e V13 `cc8ce5481c825254` em `target/wt-vrs2-var/`; os demais
  em `~/.claude/touring/vgp/binarios/`.
- Diretório: `docs/plans/2026-10-08-vgp-motor-base/diagnostics/horust-2026-10-10/` (scripts de medida, logs TDD vermelho/verde,
  mutantes, `dados/`, `dados_estendido/`). Módulos novos: `scripts/estado_rust_no_instante.py`, `scripts/reconhecedor_cargo_estendido.py`
  e seus testes. Corpora derivados DEV com overlay (não selados): `~/.claude/touring/vgp/horust_medicao/{crate,ws}/corpus`.
- Git: nada commitado; árvore com WIP de outras sessões (`corpus_v0d.py`, `v0e_negacoes.py` etc. modificados por terceiros).

## 8. O que NÃO foi verificado

- Nenhum motor/juiz rodou em held-out (não existe). Nenhum corpus foi selado.
- A classificação por causa é textual; não rodei o rustc em nenhum dos 16 para confirmar a causa.
- O estado reconstruído de arquivos reescritos por Bash não é verificável pela dobra; só sinalizado.
- `typescript/escopo.rs` (falha do hook) não foi investigado.
- 4b: a classificação por situação é textual (não resolve módulo nem avalia `cfg`); `situacao_gates_limpo.py` tem `main` com CC 18 e nenhum
  teste. A cadeia de causa dos `gates_limpo` foi lida do transcript da sessão `8614d1f9` (Edits 22:58:31Z e 23:20:47Z e os dois cargo),
  não reexecutada no rustc. O V12 foi identificado pelo caminho `target/wt-vrs2-var/V12` e pelo número nas medidas dele; confirmar com o vrs-porta.
