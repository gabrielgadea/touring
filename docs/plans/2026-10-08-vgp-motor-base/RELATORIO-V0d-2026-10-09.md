# RELATÓRIO V0d — conserto do CONSTRUTOR do corpus de replay (09/10/2026)

> Sessão: `647e47b7` (analise, backend k3) · delegação da coordenadora `analise-89` · juiz
> `scripts/juiz_v0d.py` **intocado** (sha256 `67ffdae78231c797…` antes = depois, §5).
> Backup pré-build: `~/.claude/touring/vgp/corpus_v0d.bak-20261009-pre`.

## 1. Pendências: antes → depois

| pendência (juiz, baseline 09/10) | depois |
|---|---|
| `negativos 30 < 300` | **RESOLVIDA — 300/300** (pool registrado 9.985 ids canônicos; 3.159 podados por não reconstruir/compilar, 77 rodadas) |
| `re-sorteio com a semente não devolve os mesmos ids` | **RESOLVIDA** — o pool agora é de **ids** (era `"arq@ts"`, que jamais igualaria sha1) + poda iterativa: o pool final re-sorteia exatamente a amostra final |
| `controle positivo do corpus: 30/51 = 59% < 70%` | **RESOLVIDA — 65/91 = 71,43%** |
| critério 5 (re-execução própria) — nunca tinha rodado | **RODOU E PASSOU** (amostra de 3 com snapshot reproduz + controle negativo sem snapshot não reproduz) |
| (surgiu no build 1) `ids repetidos` | **RESOLVIDA** — dedupe por id no `_montar` (2 linhas V0a do mesmo evento) |

Veredito literal do juiz (rodado duas vezes, a segunda **após** o deploy 30.4.94):

```
entradas 391 · positivos 91 · negativos 300
veredito: PRONTO
juiz_rc=0
```

## 4. Resultado do rebuild final (build 3, 486,9 s)

| | antes (08/10) | depois (09/10) |
|---|---|---|
| positivos no corpus | 51 | **91** |
| reproduzem | 30 (58,82%) | **65 (71,43%)** |
| por origem | inline 23/33 · tmp 7/15 · atribuído **0/3** | inline 41 · tmp 14 · atribuído **10** |
| por classe | atributo 23 · import 4 · kwarg 1 · nome 2 | atributo 39 · import 4 · kwarg 1 · nome 21 (as 4 classes cobertas, critério 3 ok) |
| negativos | 30 | **300** |
| incompletos | 49 | **9** (motivos precisos abaixo) + 1 duplicata de inventário |

**Incompletos finais (honestos — nunca entram com conteúdo errado)**: 2 Edit que não casa sobre a base
git · 1 inline via script `.sh` (programa indireto) · 1 dobra quebrou sobre base vazia · 4 sem base
git e sem Write da sessão · 1 duplicata de inventário.

**Irredutíveis medidos nos 26 não-reproduzidos restantes** (ficam no corpus, são o teto honesto do
replay em sandbox): rede `--unshare-net` ×3 (OSError/curl) · dependência de dados/estado da época
×~12 · daemon escondido ×1 (pipe de `touring tantivy`) · venv de /tmp sumido ×1 · frame atribuído ao
scratchfile pelo inventário ×1 · timeout ×1 · comando truncado no inventário ×1 · árvore de dados
/tmp criada fora da dobra ×2.

**Observação de campo menor**: `item_v0a` sai sempre `""` — as linhas de `erros_inventario.jsonl`
não têm campo `id`. Não afeta o juiz; registro para quem evoluir o inventário.

## 5. Aceite (exit codes literais, 09/10 ~12:55 BRT)

```
(a) cd ~/projects/analise && python3 .../scripts/juiz_v0d.py
    → entradas 391 · positivos 91 · negativos 300 · veredito: PRONTO · rc=0
(b) juiz_v0d.py --selftest → rc=0 · corpus válido sem pendências: True · 5 mutantes pegos: True×5
(c) sha256 juiz_v0d.py = 67ffdae78231c79743a02ecbf906444b1109190cd998e49aa3f2bfed5a001d2d (inalterado)
(d) cd <bundle> && python3 -m pytest -q scripts/test_corpus_v0d.py → 31 passed, rc=0
    ruff check scripts/corpus_v0d.py scripts/test_corpus_v0d.py → All checks passed, rc=0
(e) mutações: M1a/b (3 mortes) · M1c · M1d · M2a · M2b (2) · M3 (2) · M4 (2) · M5 · M6 · M7 · M8 · M9
    · M10 · M11 · M12 · M13 — cada uma rc=1 com o(s) teste(s)-alvo morto(s), restaurada e re-rodada
    a verde (31/31) — nomes dos testes na tabela da §3
```

Construtor `--selftest` herdado: rc=0 (8 controles intactos). Versão do touring no veredito final:
**30.4.94** (deploy da coordenadora aconteceu durante o meu build; o corpus e o juiz **não invocam o
CLI do touring** — replays são bwrap+python3/bash dos comandos históricos, com `touring run`
reescrito para `python3 -c`/`bash -c` — logo o veredito é imune à troca de versão em curso).

Cap de tentativas: lacuna "controle positivo" usou 3 (contexto → lang/prog_dir/assinatura →
programa-para-replay/vars/virtuais/arquivo-como-path), fechando em 71,43%.

## 2. Diagnóstico medido (antes de codar)

| medição | resultado | método |
|---|---|---|
| universo de negativos por rota | git 9.537 (analise 8.492 · touring 793 · **analise/.claude nested 229** · simulador 23) · session_work 3.969 · no-git 232 | walk-up de `.git` sobre os 13.738 candidatos (`/tmp/v0d/medir1.py`) |
| causa raiz dos 270 descartes | dupla: (a) `vivos.project_root` faz **default p/ analise** e o `relative_to` derruba o que vive fora dos 2 projetos; (b) `_conteudo_depois` exigia `.git` e foldava **sem corte em ts nem ordenação** | leitura + reprodução da decisão por rota |
| taxa de reconstrução por rota × estratégia certa (amostra 120 estratificada) | session_work **39/40** · git 19/40 · no-git 17/40 → ~8.500 recuperáveis no universo | `/tmp/v0d/medir2.py` |
| bug de identidade do sorteio | o construtor registrava `candidatos` como strings `"arq@ts"`; o juiz compara com os **ids sha1** das entradas — os conjuntos nunca seriam iguais, mesmo com 300/300 | leitura de `juiz_v0d.negative_gaps` × `construir_negativos` |
| 17 inline `touring run` | 14 `--code '…'` · 3 `--file /tmp/…` · **0 usam o stub `touring.`** · todos com `file='<string>'` (== o frame de `python3 -c`) | medir1/medir2 |
| 18 "não existia no commit anterior" | **14 com Write ≤ first na sessão** (base vazia legítima) · 4 sem Write (irredutíveis) | medir2 |
| 9 tmp "não gravados" | **0 aparecem em outro transcript** ≤ first — irredutíveis, ficam incompletos com motivo | medir2 |
| 2 dos 3 atribuído no corpus | morrem com `can't open file /tmp/…/scratchpad/*.py` — o comando depende de script de sessão que não existe mais; o contexto saía vazio porque `montar_positivo` passava `files={}` **literal** | leitura dos registros de replay |

## 3. Consertos (TDD por lacuna: RED → GREEN → mutação → restaurar)

Suíte nova: `scripts/test_corpus_v0d.py` (22 testes). Mutações aplicadas por edição cirúrgica
temporária, cada uma restaurada e re-rodada a verde (evidência: rc por etapa).

| lacuna | conserto no construtor | teste (RED→GREEN) | mutação → morreu? |
|---|---|---|---|
| 1a negativos fora de repo | `_git_root` walk-up (`.git` dir ou arquivo de worktree) substitui o default-analise | `test_conteudo_depois_sem_git_com_write` | M1a `if root is None: return None` → **3 mortes** ✓ |
| 1b base vazia legítima | dobra de base `""` **só** quando um Write/heredoc ≤ ts criou o arquivo (`created`) — senão None | `test_conteudo_depois_base_vazia_quando_write_criou` + 2 negativos | M1b (com M1a) ✓ |
| 1c corte e ordem | `_dobra_livre`: registros **ordenados por timestamp**, corte em `ts` (era `"9"*30` sem ordenação) | `test_conteudo_depois_corte_em_ts` · `test_conteudo_depois_ordena_registros` | M1c sem `sorted` → matou ordenação ✓ |
| 1d heredoc fora de /tmp | `_heredocs_de`: `cat > path <<EOF` sem as guardas de disco do replay (que protegem o hoje) | `test_conteudo_depois_heredoc_bash_cria_fora_de_tmp` | (coberta por M1a) |
| 5 re-sorteio | pool registrado = **ids canônicos**; poda iterativa: sorteado que falha sai do pool e re-sorteia — o pool final re-sorteia exatamente a amostra final | `test_sorteio_por_ids_reproduz_o_conjunto` (+ universo < n) | M1d sem poda → matou ✓ |
| 2a contexto: sessão | `_contexto` recebe `files_sessao` (rebuild_tmp da própria sessão) — antes `files={}` literal | `test_contexto_sobrepoe_ref_de_sessao_reconstruida` | M2a sem o canal → matou ✓ |
| 2b contexto: projeto no rev | `_refs_projeto` (absolutos + `-m a.b.c` + relativos ao cwd) + `_imports_do` (imports do snapshot); cada ref reconstruída por `_conteudo_depois(ref, first, …)` e sobreposta **só se difere de hoje** | `test_contexto_projeto_no_rev_de_first` · `test_contexto_import_do_snapshot_no_rev` · `test_contexto_nao_sobrepoe_o_proprio_nem_igual` | M2b sem os 2 canais → **2 mortes** ✓ (na 1ª rodada o teste media o canal errado — fixture em /tmp é session_work; corrigido com monkeypatch de `session_work`) |
| 3 touring run | `programa_inline` ganha a rota `touring run`: `--code` via **shlex** (não regex — aspa interna quebra regex), `--file` via dobra da sessão; `comando_replay` reescreve como `python3 -c` (frame `<string>`, medido) **preservando `--timeout-ms` como `timeout N`** (o `_declared_timeout` lê o teto do texto) | 4 testes (code · file · python-c · sem pipe · timeout) | M3 sem a rota → 2 mortes ✓ |
| 4 base vazia p/ positivo | `_snap_projeto`: sem base git + Write da sessão → entra; sem Write → incompleto com motivo | 2 testes | M4 → 2 mortes ✓ |
| + staleness do replay | `reproduzir` reusa o registro só se a **assinatura do overlay** (paths+sha256) é a mesma — rebuild não herda veredito de snapshot antigo | `test_replay_reexecuta_quando_overlay_muda` | M5 reuso sempre → matou ✓ |
| 6 classificar o resto | motivos precisos por balde (ver §4) — o irredutível fica fora com motivo, nunca entra errado | (controles negativos da suíte) | — |

Decisão de construtor registrada (a coordenadora valida): para `touring run`, o campo `command`
carrega a reescrita (`python3 -c` / `bash -c` / `python3 <path>` — o que o juiz executa, estável por
construção: não depende do binário touring dentro do bwrap, que tem os sockets do daemon escondidos)
e o original vai para `command_original`; `command_source` ganha o sufixo `+python-c`.

### Tentativa 2 (após o build 1 medir 59/92 = 64%)

Anatomia dos 32 não-reproduzidos, lida dos registros de replay (não da memória): ~24 irredutíveis
por construção (rede `--unshare-net` ×3 · dados/estado da época ×~12 · daemon escondido ×1 · venv de
/tmp sumido ×1 · frame atribuído ao scratchpad pelo inventário ×1 …) e ~6-8 em **bugs meus**.

| conserto | teste | mutação → morreu? |
|---|---|---|
| dedupe por id no `_montar` (2 linhas V0a do mesmo evento colidem o id determinístico — "ids repetidos" do juiz) | `test_montar_dedup_por_id` | M9 → matou ✓ |
| `comando_replay` sensível a `--lang`: bash → `bash -c` (preserva frames internos), python → `python3 -c` | `test_comando_replay_lang_bash_vira_bash_c` | M6 → matou ✓ |
| `prog_dir` (diretório do `--file`) nas bases de imports | `test_imports_do_com_base_extra` | M7 → matou ✓ |
| assinatura do replay cobre o **command** (rebuild que só muda o comando não herda o registro) | `test_assinatura_do_overlay_cobre_o_comando` | M8 → matou ✓ |

### Tentativa 3 (após o build 2 medir 61/91 = 67%)

| conserto | teste | mutação → morreu? |
|---|---|---|
| `_programa_para_replay`: para tmp/atribuido o programa é o **--code do comando** (wrapper bash), NUNCA o snapshot python — o bug "bash -c recebe python" de 5 entradas | `test_programa_para_replay_nao_inline_extrai_o_code` | M10 → matou ✓ |
| `_expande_vars` antes de extrair refs (`S=/tmp/x; python3 $S/a.py` era invisível → overlay faltando) | `test_contexto_expande_vars_de_atribuicao` | M11 → matou ✓ |
| `_resolve_mod` aceita os reconstruídos da sessão (`virtuais` = files_sessao) — o `harness.py` sumido do caso a0b2f665934b | `test_resolve_mod_aceita_arquivo_de_sessao_sumido` | M12 → matou ✓ |
| `comando_de_arquivo`: `-c '<fonte do próprio script>'` em arquivo real → `python3 <path>` (sob `-c` o frame é `<string>` e `__file__` não existe) | `test_comando_eh_o_proprio_arquivo_roda_o_path` | M13 → matou ✓ |
