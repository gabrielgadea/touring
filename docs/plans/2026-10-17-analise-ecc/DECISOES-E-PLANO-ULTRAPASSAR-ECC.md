# Decisões pendentes da análise do ECC e plano para ultrapassá-lo (17/10/2026)

Pedido do Gabriel (17/10): «me explique de forma detalhada o que segue e encontre a melhor solução, a que sempre
potencializa o touring: N1–N6, maintain_tiers, VGP fase D, contents: write do job gates. Após isso, elabore um plano
para que o touring evolua e ultrapasse o ecc nos 31 itens que o ecc é melhor do que o touring».

Evidência coletada por script nesta data (`scratchpad/coleta_canvas.py`, saída copiada nos números abaixo). Marcação:
**[V]** verificado por execução ou leitura do arquivo; **[I]** inferência; **[?]** desconhecido, vira pergunta aberta.

---

## Parte 1 — Canvas das 9 decisões

### N1 — a mutação prometida que nada executa

1. **Decisão.** Executar a mutação que o `.cargo/mutants.toml` promete ou retirar a promessa.
2. **Contexto.** O arquivo diz «incremental (--in-diff) in CI, sharded full scans weekly» [V]; nenhum workflow roda
   `cargo mutants`; o passo do `ci.yml:271-279` só valida a configuração [V]. Nesta sessão a mutação foi o método que
   separou teste que prova de teste que só passa: 49/50 no C1, 121/121 no D2, 5/5 no filtro de hooks, e ela pegou um
   harness errado que «matava» mutantes em 0,14 s. Tudo isso foi manual, na máquina compartilhada.
3. **Opções.** A: retirar a promessa. B: `--in-diff` no push/PR mais varredura completa semanal em fatias, no GitHub
   Actions. C: só local, timer semanal sob `hw-slot`. D: só o `--in-diff`.
4. **Trade-offs.**

   | Opção | A favor | Contra | Custo |
   |---|---|---|---|
   | A | honestidade imediata | perde o método que mais pegou defeito na sessão | zero |
   | **B** | computação grátis (repo público usa runner padrão sem custo) [I]; mutação vira hábito, não heroísmo | varredura completa do workspace é longa; flake vira «missed» falso | M |
   | C | sem CI | disputa a máquina com 6 sessões a 78–94 °C (o mesmo motivo que derrubou o A/A do VGP) | M |
   | D | rápido | sem visão do acervo, a promessa semanal continua falsa | S |

5. **Afetados.** Gabriel (lê o CI), quem faz push, a dimensão de qualidade de testes e o KPI.
6. **Riscos de B.** (1) Gate lento ou instável: começar como aviso por 2 semanas e promover a portão só com taxa de
   vermelho falso medida, como o critério de promoção do gate de prosa. (2) Fatia acima das 6 h do job: a primeira
   rodada calibra o número de fatias. (3) Resultado que ninguém lê: publicar `mutation_score` no `touring kpi -j`.
7. **Reversibilidade.** Total, é um arquivo de workflow.
8. **Recomendação.** **B em degraus**: `--in-diff` contra o commit anterior em todo push (aviso, depois portão) e a
   varredura completa em fatias no workflow agendado do N3, com o placar no KPI. Potencializa porque a mutação passa a
   ser um sinal medido do Touring, e não uma promessa no comentário. Confiança **0,8**.
9. **Perguntas abertas.** Quanto dura a varredura completa (a primeira rodada mede)? Qual a taxa de flake da suíte no
   runner (medir em 3 rodadas)?

### N2 — nenhum piso de cobertura

1. **Decisão.** Como transformar o lcov que o CI já gera (`ci.yml:93-152`) num piso que reprova.
2. **Contexto.** O job `coverage` alimenta o F3.1, mas não tem `--fail-under` [V]. A cobertura pode cair sem ninguém ver.
3. **Opções.** A: piso fixo (`--fail-under-lines X`). B: catraca — o piso é o último valor medido na `main`, menos uma
   tolerância, e sobe sozinho quando a cobertura sobe. C: cobertura do diff (linhas novas ≥ 80 %). D: só relatar.
4. **Trade-offs.**

   | Opção | A favor | Contra |
   |---|---|---|
   | A | simples | o número é arbitrário: baixo demais não protege, alto demais bloqueia |
   | **B** | nunca regride e não exige escolher número | variação entre rodadas exige tolerância |
   | C | protege o código novo | **redundante com o N1-B**: todo mutante numa linha não coberta sobrevive, então a mutação do diff já reprova linha nova sem teste |
   | D | nada muda | o problema continua |

5. **Afetados.** Quem faz push; o F3.1, que lê o mesmo lcov.
6. **Riscos de B.** Variação falsa (teste ignorado ou condicional): tolerância de 0,5 ponto percentual, calibrada pela
   variação de 3 rodadas.
7. **Reversibilidade.** Total.
8. **Recomendação.** **B, a catraca no total**, deixando a qualidade do diff com a mutação do N1. Um piso e um juiz,
   sem duplicar. Confiança **0,75**.
9. **Perguntas abertas.** Qual é a cobertura atual [?] e quanto ela varia entre rodadas iguais [?].

### N3 — nenhum vigia agendado

1. **Decisão.** Criar um workflow agendado e definir o que ele vigia.
2. **Contexto.** Zero workflows com `schedule:` [V]. **Correção ao comparativo:** o `.github/dependabot.yml` já roda toda
   semana para cargo (patches agrupados) e GitHub Actions [V]. O que ainda depende de push: `cargo deny`
   (advisories, licenças, crates abandonadas), a varredura completa de mutação (N1) e um teste de instalação limpa
   (plano, item T6).
3. **Opções.** A: um `scheduled.yml` semanal com esses três jobs. B: confiar só no Dependabot. C: cron local.
4. **Trade-offs.** A custa um arquivo e cobre o que o Dependabot não cobre; B não pega licença nem a mutação; C disputa a
   máquina.
5. **Afetados.** Gabriel recebe o e-mail de falha (o GitHub avisa quem editou o cron por último) [I].
6. **Riscos de A.** Workflow agendado em repo público é desligado após 60 dias sem atividade [I]: o painel de prontidão
   (N5) passa a acusar «vigia desligado».
7. **Reversibilidade.** Total.
8. **Recomendação.** **A**, segunda-feira 09:00 UTC (06:00 BRT), `permissions: contents: read`, três jobs. Confiança **0,85**.
9. **Perguntas abertas.** Os alertas de segurança do Dependabot estão ligados nas configurações do repositório [?].

### N4 — a cota fora da barra de status

1. **Decisão.** Como mostrar a franquia (5 h e semanal) na barra.
2. **Contexto.** O payload da statusline traz `rate_limits.five_hour` e `seven_day` com percentual e horário de
   renovação [V]; o `status_monitor.sh:41-47` grava em `rate_limits.json` e não mostra [V]. Agora: 5 h em 60 %, semana em
   50 % [V]. Hoje duas sessões de engenheiro bateram no limite no meio da entrega e pararam [V].
3. **Opções.** A: segmento fixo «5h 60% ↻1h04 · 7d 50%». B: só acima de 70 %. C: A com cor (verde < 70, amarelo < 85,
   vermelho ≥ 85) e um marcador único aos 85 %, pelo mesmo canal do SALVE/COMPACT, que chega ao modelo. D: C mais um
   aviso no `agent_model_gate` quando um fanout grande começa com a janela de 5 h acima de 90 %.
4. **Trade-offs.** A ocupa barra sempre; B esconde o dado até tarde; C dá o dado e o alerta sem ruído; D é o primeiro
   passo de executor para a regra 17 (economia), e precisa começar como aviso, porque bloquear trabalho legítimo custa.
5. **Afetados.** Gabriel e todas as sessões; sessões irmãs (Kimi/MiniMax) não têm esse payload.
6. **Riscos de C.** Payload sem `rate_limits` (outro backend, versão antiga): não mostrar nada, nunca 0 % (ausência não
   é zero). Barra longa: o segmento cabe em 18 caracteres.
7. **Reversibilidade.** Total.
8. **Recomendação.** **C agora; D como aviso logo em seguida.** O dado já está em disco, e o caso de hoje mostra o custo
   de não vê-lo. Confiança **0,9**.
9. **Perguntas abertas.** Nenhuma que mude a escolha.

### N5 — quatro números de saúde sem reconciliação

1. **Decisão.** Como ter uma única resposta para «está saudável?».
2. **Contexto.** Quatro medidas usam a mesma palavra [V, G2:2]: `status.composite_health_score` 0,4939; e2e 0,8593;
   harness-metric 0,536; kpi com 26 PASS, 13 ADVISORY, 7 STUB e 0 FAIL de 46. Há uma quinta, no cabeçalho dos hooks
   («health: healthy 0.81»). A lição «um nome, duas grandezas» já mostrou que renomear resolve.
3. **Opções.** A: eleger uma e apagar as outras. B: um veredito de prontidão feito de uma **lista de itens de atenção**
   (cada um com fonte, limiar e rota), saída 0/2 para o Stop hook e para o CI, e as medidas antigas como detalhe com
   nomes distintos. C: média ponderada das quatro. D: só documentar.
4. **Trade-offs.** A perde informação; **B diz o que fazer**; C cria um sexto número; D não muda o comportamento.
5. **Afetados.** Gabriel, o Stop hook, o juiz `loop_converged.py`.
6. **Riscos de B.** A lista vira ruído: cada item exige rota e dono, e um item aceito precisa de dispensa com validade,
   nunca silêncio.
7. **Reversibilidade.** Total.
8. **Recomendação.** **B** (`touring readiness`, e `status` passa a citá-lo), mais a renomeação das quatro medidas.
   Fecha também as linhas G2:1 e G2:2 dos 31. Confiança **0,8**.
9. **Perguntas abertas.** Que itens o Gabriel considera bloqueantes (exit 2) e quais são só aviso.

### N6 — journals sem retenção

1. **Decisão.** Qual política de retenção aplicar aos journals de runtime.
2. **Contexto.** 1.208 arquivos e 290,8 MB nos diretórios de runtime [V]. O maior é o `hook_trace.jsonl`, com 191 MB num
   arquivo só, lido por 5 arquivos de código [V]. O `run_journal.jsonl` tem 12 leitores [V]. O `memwatch.log` tem
   **zero leitores** [V]: é um escritor sem consumidor (REGRA #0). Os logs de ciclo de 18–19/09 seguem no disco [V].
   Em disco, 290 MB é pouco perto dos 412 GiB do `vgp-semeado`. O custo real é cada leitor varrer 191 MB e não haver
   inventário de quem lê o quê.
3. **Opções.** A: rotação por tamanho. B: apagar por idade. C: **manifesto de journals** (caminho, escritor, leitores,
   janela que cada leitor precisa, retenção e rollup) com `touring journal gc --dry-run|--apply` no cron do daemon;
   antes de podar, grava agregados diários, e o manifesto acusa escritor sem leitor. D: nada.
4. **Trade-offs.** A e B podem apagar o que um KPI precisa; C custa um manifesto, mas cada poda respeita a janela
   declarada e o inventário vira guarda de órfãos; D deixa o problema crescer.
5. **Afetados.** KPIs (`code_mode_adherence`, `context_budget`, `stdin_fraction`), o r3 e quem depura.
6. **Riscos de C.** Rollup que perde informação: antes de podar, um teste recalcula o KPI pelo rollup e pelo bruto, e
   eles têm de bater.
7. **Reversibilidade.** Parcial: o que se poda não volta. Por isso o `--dry-run` é o padrão.
8. **Recomendação.** **C**, começando pelo `hook_trace` (rotação mensal com rollup diário) e pelos escritores sem
   leitor. Confiança **0,75**.
9. **Perguntas abertas.** Qual a maior janela que cada leitor usa [?]: o manifesto vai responder isso.

### maintain_tiers — o que o tier `reference` significa

1. **Decisão.** Definir o significado do tier da memória antes de ligar qualquer manutenção.
2. **Contexto.**
   - O `maintain_tiers` (`rlm.rs:1324`) não tem chamador de produção [V].
   - O `TierManager`, duplicado e divergente, foi decidido para remoção (C8).
   - No `memory.db` do projeto, `reference` tem 16.745 linhas (8.520 `transcript_lesson`, 7.863 `insight`, 362
     `lesson`). Fora da escada ficam `semantic` (4.804), `local` (85) e `episodic` (5).
   - A política padrão rebaixaria 15.484 linhas de `reference` para `working` e apagaria 5.
   - **Fato decisivo [V]:** o tier faz parte do endereço. `rlm.get` filtra `WHERE key = ?1 AND tier = ?2`
     (`rlm.rs:270`), e o cortex grava em tier fixo (`session.rs:65` guarda o resumo da sessão em `Reference`). Mover a
     linha de tier faz a busca por chave voltar vazia em silêncio. O `maintain_tiers`, como está escrito, é uma perda de
     dados sem barulho.
3. **Opções.** A: tier como escada de calor, como foi desenhado (exige reescrever todo chamador para buscar sem tier).
   B: tier como **espaço de nome**, oficializando o uso real: `reference` = conhecimento durável; o `maintain_tiers` e a
   `TierPolicy` saem. C: B mais **calor como sinal separado**: evaporação do feromônio calculada no recall a partir de
   `access_count`, `accessed_at` e crédito (`memory credit`), sem reescrever a linha; o `touring memory maintain
   --dry-run` passa a relatar candidatos (efêmeros acima de 48 h, espaços de nome sem dono). D: deixar o código morto.
4. **Trade-offs.**

   | Opção | A favor | Contra |
   |---|---|---|
   | A | fiel ao desenho original | quebra o endereçamento; reclassifica 92 % sem evidência |
   | B | honesto e simples; remove um perigo | não usa o sinal de uso |
   | **C** | dá ao ACO a evaporação que hoje só existe na metáfora; mensurável | muda o ranking do recall, então exige avaliação antes |
   | D | nada | código morto e perigoso (REGRA #0) |

5. **Afetados.** Todo recall (hooks, cortex, CLI), a injeção de contexto, as lições.
6. **Riscos de C.** Ranking pior: a evaporação entra só se um conjunto de avaliação do recall (consultas com memória
   relevante conhecida) mostrar ganho; sem ele, fica desligada. Custo de cálculo por consulta: medir antes de persistir.
7. **Reversibilidade.** B e C sem gravação no banco são totalmente reversíveis; A não é.
8. **Recomendação.** **B já** (documentar o tier como espaço de nome e remover a reescrita de tier) **e C atrás de
   avaliação**. Potencializa porque transforma a metáfora do feromônio em sinal medido, sem tocar no endereço das
   memórias. Confiança **0,75**.
9. **Perguntas abertas.** Existe conjunto de avaliação do recall [?] (se não, nasce aqui, com o método de corpus do
   ECC). Algum consumidor assume `core` = «sempre injetado» [?].

### VGP — aprovar a fase D antes de qualquer veredito de latência

1. **Decisão.** Aprovar a fase D e definir a régua que ela mede.
2. **Contexto.** O A/A do C′ deu `PROTOCOLO_INADEQUADO` [V]: o IC95 do R_p95 ficou em [0,7142; 1,0807], enquanto o
   R_típico ficou estreito, [0,9915; 1,0049]. O pré-registro manda para a fase D: tempo de CPU por chamada
   (`CLOCK_THREAD_CPUTIME_ID`), descrito como «imune à disputa por construção». Duas ressalvas:
   - **(a)** O tempo de CPU elimina a espera na fila do processador, mas não a variação de frequência. A máquina roda a
     78–94 °C, o throttling reduz o clock e cada chamada gasta mais CPU. «Imune» é forte demais.
   - **(b) [I, a verificar no código]** O motor usa o `pyright-langserver`, que é outro processo. O relógio da thread
     não vê o trabalho dele, e a fase D mediria só a camada nativa.
3. **Opções.**
   - A: fase D como pré-registrada.
   - B: fase D com **três números por chamada**: CPU da thread nativa, delta de CPU do processo do pyright (`utime +
     stime` em `/proc/<pid>/stat`) e **instruções retiradas** (contador de hardware no espaço de usuário; liberado aqui,
     `perf_event_paranoid=2` [V]). O portão fica nas instruções, que não dependem de frequência nem de fila; CPU e parede
     só são relatados.
   - C: repetir o tempo de parede com isolamento.
   - D: abandonar o portão de latência.
4. **Trade-offs.** A mede só metade do caminho; **B mede o caminho inteiro com a régua mais estável**; C briga com a
   máquina, e a lição «p95 de rodada mede a máquina» já mostrou que perde; D abre mão de uma garantia.
5. **Afetados.** O release do VGP e o portão de 150 ms.
6. **Riscos de B.** Contador indisponível em outro ambiente (CI, distro com `paranoid=3`): cai para CPU com A/A próprio.
   Instrução não é milissegundo: o absoluto continua em CPU-ms, e as instruções protegem contra regressão relativa.
7. **Reversibilidade.** Total, enquanto o portão não for usado num veredito.
8. **Recomendação.** **Aprovar a fase D na forma B**, com **emenda datada ao pré-registro antes de qualquer
   implementação ou resultado** (A/A nas instruções com IC95 ⊂ [0,99; 1,01]; A/B com limite superior ≤ 1,10). Confiança
   **0,8**.
9. **Perguntas abertas.** Confirmar no código se o pyright roda fora do processo e como o motor espera por ele.

### `contents: write` do job `gates`

1. **Decisão.** Manter ou isolar a permissão de escrita do job `gates`.
2. **Contexto.** O job tem `contents: write` [V] para o auto-sync do `ARCHITECTURE.md`, roda em push **e** em PR, faz
   checkout do branch, e os demais passos rodam com o mesmo token. Sem `persist-credentials: false`, o token fica no
   `.git/config` para todo passo seguinte, inclusive build scripts de dependências. O F4.7 novo (regra d) acusa
   exatamente esse padrão.
3. **Opções.** A: manter. B: **separar**: `gates` só leitura, com `persist-credentials: false`; um job pequeno
   `sync-architecture` com `needs: gates`, que roda só em push na `main`, só o `docs/sync_metrics.py`, e tem escrita.
   C: tirar o auto-sync (volta o «vermelho a cada .rs» de 02/08). D: gerar a métrica no release em vez de versioná-la a
   cada push.
4. **Trade-offs.** A expõe o token a todo código do job; **B reduz a superfície a um script**; C devolve trabalho manual;
   D muda o contrato do documento.
5. **Afetados.** CI, quem faz PR, a nota F4.7 do próprio repositório.
6. **Riscos de B.** Corrida com push concorrente (o script já trata); em PR, o drift vira aviso (a `main` sincroniza
   depois do merge) em vez de commit no branch do PR.
7. **Reversibilidade.** Total.
8. **Recomendação.** **B.** Privilégio mínimo sem perder o auto-sync. Confiança **0,85**.
9. **Perguntas abertas.** Nenhuma que mude a escolha.

---

## Parte 2 — Plano para ultrapassar o ECC nos 31 itens

### Regra do plano

O ECC ganha por **amplitude em prosa**: 26 reviewers, 63 skills por linguagem, checklists de texto. O Touring ultrapassa
por um caminho que o ECC não tem:

- **Checklist executável:** cada item vira um detector, um gate ou um teste, e o LLM só julga o resíduo.
- **Medida:** cada capacidade nova tem corpus, recall e precisão publicados. O ECC não mede nenhuma das suas.
- **Memória:** cada execução deposita feromônio (`memory store` + `learning reward`) e a próxima começa do que já se
  aprendeu.

O critério de «ultrapassou» é o mesmo rubric da rodada G: a linha só muda para «Touring melhor» com evidência em
disco.

### Os 31, agrupados

| Tema | Linhas | Como o Touring ultrapassa | Aceite medido | Tam. |
|---|---|---|---|---|
| **T0 Fundação** (sai das decisões) | G2:1, G2:2, G2:5 | N5 (prontidão com lista de atenção, exit 2) e N4 (cota com alerta que chega ao modelo) | Stop hook lê a prontidão; marcador dos 85 % dispara em teste | S–M |
| **T1 Revisão executável por linguagem** | G1:1, G1:3, G1:4, G1:5, G1:6, G1:19 | Reviewers `rust` e `python` (sonnet, só leitura) que rodam primeiro os detectores (`touring audit`, `touring-quality`, regras ast-grep, clippy) e julgam só o resto. Skills `rust-patterns`/`rust-testing` e `security-review` em que cada item aponta seu detector ou se declara «julgamento». Regras por linguagem com `paths:` (carregam só quando o arquivo é daquela linguagem). Caçador de falha silenciosa em Rust a partir das lições já gravadas (`unwrap_or(0)` que mente, `let _ = Result`, rc=0 sem sinal): **a lição vira detector** | Corpus semeado (≥ 40 defeitos Rust, ≥ 30 Python, com casos que não podem ser acusados): recall ≥ 0,8 e falso positivo ≤ 0,1, comparado com uma revisão de sonnet sem detectores | L |
| **T2 Build resolver com memória** | G1:2 | `rust-build-resolver` que lê o erro em JSON do cargo, consulta `gotcha match` e `memory recall` pelo código do erro, corrige o mínimo, recompila sob `hw-slot` e grava o par erro→conserto. O do ECC começa do zero a cada vez | Replay de falhas de build reais tiradas dos transcripts: menos tentativas até o verde que um agente sem memória | M |
| **T3 Custo e franquia** | G1:12 | `touring cost-report` sobre os transcripts (tokens por modelo, sessão e subagente; % de cache; US$ pela tabela de preços), mais a **previsão** de quando a janela de 5 h e a semanal estouram, citada pelo `agent_model_gate`. O ECC relata; o Touring prevê e redireciona | Erro da previsão de estouro medido contra os estouros reais | M |
| **T4 Observabilidade ativa** | G2:3, G2:4, G2:8 | `touring ops`, painel de texto com sessões irmãs, vagas do `hw-slot` com fila e temperatura, cota, fronteira do DAG e prontidão. `loop-status` sobre os transcripts: resultado de tool pendente acima de 30 min, wakeup atrasado, sessão irmã parada com mensagem não lida, **vaga presa acima de N min** (o caso do worker de hoje). Notificação de aprovação pendente, franquia ≥ 85 % e job concluído, com horário de silêncio | O replay de hoje acusa o worker que segurou a vaga rust por 47 min | M–L |
| **T5 Tour verificável** | G1:17 | `touring map --tour <tema>` gera `.tour` (CodeTour) com passos em `arquivo:linha` tirados do índice e ordenados pelo grafo de chamadas; um teste no CI acusa âncora que envelheceu | Zero âncora quebrada no CI; tour regenerado quando o código muda | M |
| **T6 Instalação e distribuição** | G3:2, G3:3, G3:4, G3:5, G3:12 | Catálogo de componentes com custo (disco, ms de partida, hooks) e `component add --plan`, que inspeciona sem mutar. Manifesto de instalação por projeto e `touring uninstall --dry-run`, que remove só o que gravou. Atualização com consentimento gravado e gates verdes, e o rollback que o ECC não tem. Teste de instalação limpa (HOME vazio, em container, no `scheduled.yml`). Gate de conteúdo do espelho: sem `curl\|sh`, segredo, `/home/<usuário>` ou symlink que escapa | Instalar → desinstalar deixa zero resíduo; o gate de conteúdo reprova cada caso semeado | L |
| **T7 Memória com fronteira de confiança** | G3:9 | Estado de revisão (não revisada / aceita / rejeitada) e recall entregue **como dado**, com proveniência. Promoção por aceite humano ou por crédito de RL. Soma-se ao C6 (redação) e à decisão do `maintain_tiers` | Injeção semeada numa memória não chega como instrução (teste) | M |
| **T8 Governança e atalhos** | G3:15, G5:16, G5:5 | `touring init --template rust\|python` com um CLAUDE.md mínimo de ponteiros. O G5:16 já tem SECURITY, CoC e CONTRIBUTING [V], então falta só o check de mensagem de commit no CI. O `touring pr` envolve o `gh` com as opções preferidas | Template gera CLAUDE.md abaixo de 60 linhas; o check de commit roda no CI | S |
| **T9 Amplitude, multi-harness e verticais** | G1:23, G1:24, G3:6, G3:7, G5:2, G5:4, G5:19 | Três destinos. **(a) Ultrapassar por outra régua:** G3:7 com matriz honesta do que é hook e do que é instrução **por backend** (Claude, e as sessões irmãs Kimi e MiniMax, que o Touring de fato tem); G5:4 com curadoria medida (telemetria de disparo e uso por skill, poda do que não é usado, teste de acerto do gatilho), contra a sobreposição que o próprio ECC admite. **(b) Decisão sua:** G1:23, G3:6 e G5:2 (portar para Codex/Cursor) têm um caminho barato — o gerador do espelho emitir `AGENTS.md` da mesma fonte, com a matriz declarando as perdas — mas só vale se você for usar outro harness. **(c) Fora do escopo:** G1:24 e G5:19 (verticais de negócio e saúde); o vertical em que o Touring já tem profundidade é o jurídico-regulatório do `analise` | (a) matriz publicada; toda skill com uso medido; (b) e (c) conforme a sua decisão | S–M |

Conta: 3 + 6 + 1 + 1 + 3 + 1 + 5 + 1 + 3 + 7 = 31.

### Sequência e dependências

| Onda | Entra | Depende de | Fecha |
|---|---|---|---|
| **1** | contents:write separado; `scheduled.yml` (N3); cota na barra (N4); prontidão (N5); manifesto de journals (N6); B do `maintain_tiers`; emenda do pré-registro do VGP | — | G2:1, G2:2, G2:5 |
| **2** | mutação `--in-diff` e completa (N1); catraca de cobertura (N2); T3 custo e previsão; T4 ops, loop-status e notificação | onda 1 (agendado, prontidão, cota) | G1:12, G2:3, G2:4, G2:8 |
| **3** | T1 revisão executável (com o corpus); T2 build resolver | onda 2 (mutação mede os testes do corpus) | 7 linhas |
| **4** | T6 instalação; T7 memória (com C do `maintain_tiers` atrás de avaliação); T5 tour; fase D do VGP implementada | onda 1 (agendado), decisão do tier | 7 linhas |
| **5** | T8; T9 (a); T9 (b) e (c) conforme a sua decisão | — | até 10 linhas |

### Como executar sem esgotar a franquia (regra 17)

- **Código determinístico:** detectores, manifesto, gc, painel, relatório de custo, gerador de tour.
- **Engenheiros sonnet em worktrees,** com spec, teste vermelho primeiro e mutantes. Uma vaga rust por vez, e cada lote
  de jobs entra numa reserva só, para não repetir o worker que segurou a vaga hoje.
- **Sessões irmãs (Kimi/MiniMax), cota fora da Anthropic:** montagem dos corpora (defeitos semeados, falhas de build
  dos transcripts, consultas de avaliação do recall).
- **Esta sessão:** julgar cada entrega com o replay contra dados reais (a lição do D2), integrar e decidir.

### Riscos do plano

| Risco | Prob. | Impacto | Mitigação |
|---|---|---|---|
| Corpus fraco faz o T1 parecer melhor do que é | média | alto | corpus com casos que não podem ser acusados, rotulado antes de rodar e congelado por sha |
| Mutação no CI instável | média | médio | aviso por 2 semanas, promoção por taxa medida |
| Franquia esgota no meio de uma onda | alta (aconteceu hoje) | médio | onda 1 entrega a cota na barra e a previsão; lotes menores |
| Escopo cresce (T9) | média | médio | (b) e (c) só com o seu sim |

---

## O que precisa do seu OK

1. As 9 recomendações da Parte 1 (ou ajustes).
2. T9 (b): portar para outro harness, sim ou não.
3. Começar pela onda 1.

---

## Parte 3 — O plano revisto depois da rodada H (18/10/2026)

A rodada H trocou o alvo. Os 31 «ECC melhor» da rodada G eram uma visão parcial; o alvo consolidado de todas as
rodadas, depois das 42 revisões, tem **70 itens acionáveis** (`ALVO-ECC.md`, gerado por `scripts/gera_alvo.py`). Fundidas
as sobreposições (E:E4 = H1:5; H2b:5 = H3:10; H4:11 ≈ H6:4; H2a:7 ≈ B:B4; H3:5 ≈ H2b:12), sobram cerca de 62 capacidades
distintas.

### O que mudou nos temas

| Tema | Mudança | Itens do alvo |
|---|---|---|
| **T10 Integridade do juiz e dos gates** (novo, prioridade 1) | O ECC está metodologicamente na frente justamente no que a lei L2/L3 do Touring promete: juiz que não se deixa enganar. Entram: proteção das configs de lint e formatação (bloqueio no pre-edit/pre-write com token próprio); corpus de cenários rotulados com comparação contra refs do git (`mustDeny`, `redundant`); braços contrafactuais pré-registrados para medir efeito e custo de cada gate; pré-voo do juiz (referência = 1, ingênuo e iscas < 1); registro imutável de avaliações (trigger SQLite que aborta UPDATE/DELETE, promoção atômica); journal com hash encadeado; fixture de reward hacking; medida de cumprimento de skill em três rigores; um predicado único de shell (A9/I21), do qual o falso positivo do heredoc de 17/10 é mais uma prova | E:E4, H1:5, A:A6, A:A7, D:D3, E:E8, H4:9, D:D11, H5:16, H2a:21, A:A8, A:A9, A:A10, E:E3, H2b:2, G4:18, H2a:17, G4:14 |
| **T1 Revisão executável** | Ganha a disciplina de teste que a rodada G superestimou: **registrar o `tdd-enforcer` como hook ou rebaixar a afirmação** (hoje é skill sem executor); nó «vermelho antes do conserto» no `bugfix.toml`; detectores de `except: pass`, `catch` vazio, `.ok()` descartado e `unwrap_or` que esconde erro; aviso de saída de depuração (`dbg!`, `println!`, `print()`); cobertura Python no F3.1; análise de cobertura comportamental do diff | H3:1, H3:2, G1:4, H1:4, H2b:11, H2b:12, H3:5, H3:21, H2a:14 |
| **T2 Build resolver** | Rebaixado: o H3:3 mostrou que a regra de 3 tentativas e os gates já existem; resta só a tabela de padrões de erro Rust como referência do `touring-engineer` | — |
| **T3 Custo e franquia** | Ganha: aviso escalonado por faixa, re-lembrete por crescimento de contexto, teto da superfície eager com registro por entrada, estimado e observado separados no mesmo registro, perfil de contexto com orçamento bloqueante, escada de escalonamento de modelo, coleta de lixo da configuração do agente. O `cost-report` usa a deduplicação por sessão do lado JS do ECC, **não** a soma do `ecc2` | G2:5, H2b:5, H3:10, C:C2, C:C3, C:C5, C:C8, E:E6, C:C7, H2b:7 |
| **T4 Observabilidade e multi-sessão** | Ganha: detector de colisão entre sessões vivas, classificador do ciclo de vida de worktree, fila de merge de branches de worktree com predição de conflito, **contrato de conclusão de delegação** (resposta final nunca é «aguardando») | G2:1, G2:2, H6:4, H4:11, D:D5, D:D10, H4:4, H1:7 |
| **T5 Tour** | Sem mudança | G1:17 |
| **T6 Instalação** | Ganha a proveniência das skills com validadores | E:E5, G3:3, G3:4, H5:3, G3:5, H5:11 |
| **T7 Memória** | Confirmado e ampliado: instinct atômico com escopo, TTL e prune, decaimento de confiança, promoção projeto→global, proveniência por unidade, «sem medida ≠ saudável» por skill, recall como dado com recusa de segredo na escrita. Liga com a decisão do `maintain_tiers` (calor como sinal separado) | B:B1, B:B2, B:B3, B:B4, B:B7, B:B8, B:B9, B:B10, H2a:7, H6:10, D:D7 |
| **T8 Governança** | Ganha: **canal privado de vulnerabilidade** (ligar o «Private vulnerability reporting» do GitHub e corrigir o `SECURITY.md`/`SUPPORT.md` circulares: ação sua), mensagem de commit no formato do histórico, criação de PR com validação, auditoria do catálogo de skills, plano que para e espera confirmação antes de tocar código, conserto do `/checkpoint` quebrado | G5:5, H3:11, H2a:22, H6:7, H2a:20, H3:14 |
| **T9 Amplitude** | Sem mudança; o H6:6 confirma que a matriz honesta deve ser **por backend** (Claude e as irmãs Kimi/MiniMax) | E:E7 |

### Sequência revista

1. **Onda 1:** fundação (N4, N5, contents:write, `scheduled.yml`, manifesto de journals) **e T10 parte 1**: proteção da
   config de lint, `tdd-enforcer` registrado ou rebaixado, predicado único de shell (começando pelo conserto do heredoc,
   já em curso).
2. **Onda 2:** mutação e catraca de cobertura; T10 parte 2 (corpus de cenários, pré-voo do juiz, registro imutável); T3; T4.
3. **Onda 3:** T1 (com o corpus do T10 medindo os detectores novos); T7.
4. **Onda 4:** T6, T5 e a fase D do VGP.
5. **Onda 5:** T8 e T9.

O T10 sobe para a onda 1 porque é o tema em que o próprio Touring promete mais do que entrega: a regra diz «o veredito é o
código», mas o código do juiz pode ser afrouxado pelo julgado e não tem corpus que o calibre.
