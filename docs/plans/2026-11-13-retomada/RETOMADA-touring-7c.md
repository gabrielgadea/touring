# RETOMADA — sessão `touring-7c` (parada em 13/11/2026 ~14:10 BRT)

## PARADA — LER PRIMEIRO (substitui o que abaixo disser o contrário)

Gabriel: «preciso desligar. pare e salve tudo para ser retomado em uma sessão futura».

- **Git:** branch `feat/2026-09-16-shell-quality-wiring-python` **igual ao `origin`** depois do push (autorizado por
  ele: «sim, pode seguir com os três»). Último commit de código: `a0fa46b4`; depois dele só esta nota. Árvore limpa
  de arquivo rastreado; `crates/GateId` segue como lixo não rastreado (não commitar, não apagar).
- **Rito de release 30.4.110: PARADO por mim, no gate de testes (passo 1). Nada foi instalado.** Dos 30 pacotes,
  13 foram iniciados (inferlets … `touring-hook-runtime`, este interrompido compilando), **0 falhas**. Passos 2
  (`update-touring`), 3 (toolchain), 4 (default) e 5 (projetos) não rodaram. Log:
  `~/claude-artefatos/touring/propagate-30.4.110-etapa1-retomada.log`. O rito não retoma do meio: roda inteiro de novo.
- **Estado misto que continua:** `touring --version` = 30.4.110 (symlink para `target/release/`), o daemon global
  (PID 3714 na parada) roda de executável apagado, e não existe toolchain `30.4.110` em `~/.touring/toolchains/`
  (há 30.4.109). Depois do boot o daemon sobe do binário novo sem `update-touring`/verify.
- **Subagentes e assentos:** todos parados (`opus-1`, `haiku-1`, `sonnet-1..3`); nenhuma delegação aberta. As irmãs
  `touring-62` (Kimi) e `touring-0e` (MiniMax) estavam ociosas, sem tarefa pendente minha.
- **Fora deste repositório, alterado e NÃO commitado:** `~/projects/analise/CLAUDE.md` (6 trechos da matriz de
  modelos, misturados a mudanças de sessão de lá) — commit é da sessão dona de lá ou do Gabriel.

**Retomar, em ordem:**
1. `touring daemon-ctl status` e `touring doctor -j`; `scripts/hw-slot --status` (corte térmico: 88 °C).
2. Rito inteiro: `scripts/propagate-release.sh 30.4.110 --stop-before-propagate > ~/claude-artefatos/touring/propagate-30.4.110-etapa1-<data>.log 2>&1`
   em segundo plano. **Lição medida hoje:** com a máquina a 94–98 °C (jobs do analise nos slots `heavy` e um
   `markdown-oxide` do Zed a 205% de CPU) ele fez 8 pacotes em 2h30; encerrado o `markdown-oxide`, fez 5 em ~35 min.
   Rodar com a máquina fria (madrugada, ou sem sessão pesada do analise). Não subir o corte térmico, não usar `--skip-gates`.
3. `touring index rebuild`, depois o juiz (comando na seção "Fila depois do rito").
4. Consertos C5 → C2 → C3 → C1 → C4 do detector de órfãos (assentos Sonnet, teste RED antes).
5. Propagação ao `analise`: só com o "livre" da sessão dona de lá.

**Decisões do Gabriel já tomadas hoje (não reabrir):** push da branch feito · pasta `subagentes/` nunca é limpa,
alarme em 1 GB · contrato de entrega é fidelidade e persistência, não teto.

**Perguntas abertas com ele:** (a) aplicar `autoInstallIdeExtension: false` para calar o erro do `/ide` (o `/ide`
não tem modo Zed; a integração com o Zed é pelo painel de agentes dele, via ACP) — ofereci, ele não respondeu;
(b) backup de `~/claude-artefatos`; (c) se a medição TanStack devia prevalecer sobre a matriz nova no lint de
codegen dos ADWs (apliquei a matriz: `workhorse`).

---

# Estado anterior (gravado com o contexto em 55%)

Na hora desta seção a branch estava 12 commits à frente do `origin`, sem push (o push veio depois).
Árvore limpa de arquivo rastreado (`crates/GateId` é lixo não rastreado: não commitar, não apagar).

## Pedidos do Gabriel nesta sessão (palavras dele)

1. «Preveja sempre um orchestrador Fable, com um subagent Opus, 3 subagents Sonnet e 5 subagents Haiku para toda e
   qualquer sessão Anthropic […] O nível de esforço padrão deve ser medium para Fable e Opus, high para Sonnet e
   xhigh para Haiku. a invocação dos agente já deve começar no início da sessão com um startup session hook. No
   início da sessão deve-se identificar as sessões Kimi e Minimnax ativa no projeto também»
2. «retome o trabalho da sessão anterior e distribua o trabalho»
3. «faça o commit do pool de modelos, atualize as regras para as novas diretrizes e corrija tudo»
4. Correção: «penso que a solução não é a especificação e teto de caracteres, mas sim a fidelidade e riqueza do
   conteúdo que deve ser transmitido e a persistência e memória desse conteúdo»
5. `/decision-canvas` sobre limpeza de `subagentes/`, rito 30.4.110 e `git push` — **aguarda o sim dele** (abaixo).
6. «verifique o porque a integração com a IDE está com problemas» (`/ide zed` tenta instalar a extensão do VS Code).

## Commits desta sessão (todos no `origin` desde o push)

| Commit | O quê |
|---|---|
| `2c5ae63c` | pool de modelos: `client/hooks/model_pool.py`, `model_pool_roster.py` (SessionStart), gate `effort:ausente`, opus só sob Fable, doc §1/§2, CLAUDE.md regra 17 |
| `499da5f5` | effort nos 5 `touring-*`; `adw.py` passa `--effort` por tier (`tiers.toml [effort]`); lint de codegen → `workhorse`; seletor de status da regra `touring-cli-index.md` |
| `12043c9f` | guarda de injeção vê shim entre aspas; 20 achados de lint em 9 hooks |
| `67794a8f` | 9 defeitos achados pela revisão do Kimi (gate, roster, detector de shim) |
| `acf5871d` | contrato de saída do subagente, 1ª versão (teto de 8000) — **superada** |
| `a0fa46b4` | pasta de entregas nunca é limpa; alarme de 1 GB medido 1×/dia; esta nota |
| `7fe86f05` | redesenho: `subagent_output_contract.py` arquiva toda entrega inteira (disco + memória Touring + aviso ao orquestrador); teto virou opt-in |

Verificação da última rodada: `python3 -m pytest client/hooks/tests scripts/test_guard_injection_policy_shim.py
scripts/test_sync_client_skills.py -q -p no:cacheprovider` → **1628 passed**; `install_claude_hooks.py --check` rc 0;
`guard_injection_policy.py` 158 cobertos / 0 lacunas; `sync-client-skills.py --check` CLEAN.

Fora do repositório, alterados e **não commitados lá**: `~/projects/analise/CLAUDE.md` (6 trechos da matriz de
modelos; o arquivo já tinha mudanças de sessão de lá), `~/.claude/settings.json` (`modelSettings`: sonnet `high`,
haiku `xhigh`; hooks aplicados pelo instalador). Cópia anterior do settings:
`~/claude-artefatos/touring/settings.pre-pool-20261113.json`.

## Em curso

- **Rito de release** `scripts/propagate-release.sh 30.4.110 --stop-before-propagate` em segundo plano (task
  `bwfqiq11j`, log `~/claude-artefatos/touring/propagate-30.4.110-etapa1-retomada.log`): 8 de 30 pacotes de
  teste, 0 falhas, parado em `touring-dispatch` com `hw-slot: aguardando — temp 94C > 88C`. Fontes de calor
  medidas: `markdown-oxide` do Zed a 205% de CPU (PID 2714092) e os dois slots `heavy` ocupados pelo analise.
  Teto de espera por job: 7200 s. Daemon global roda de executável apagado até o `update-touring` do rito.
- **Subagente `claude-code-guide`** conferindo na doc oficial se `/ide` suporta o Zed. Fatos locais já medidos:
  a sessão roda no terminal do Zed (`TERM_PROGRAM=zed`, pai `zed-editor` PID 4049), não existe `~/.claude/ide/`,
  não há `code` no PATH (só `zeditor`), `~/.claude.json` sem `autoConnectIde`/`autoInstallIdeExtension`.

## Recomendações do canvas (ele disse sim às três; feitas)

1. `git push` da branch agora (push em branch de feature não dispara workflow: CI só em `main`/PR, release só em tag).
2. Encerrar o `markdown-oxide` (PID 2714092), medir a temperatura 10 min depois; se o rito não destravar em 1 h,
   parar e agendar para a madrugada. Descartados: subir o corte térmico e `--skip-gates`.
3. Não limpar `~/claude-artefatos/<projeto>/subagentes/` (≈ 250 KB/dia, estimativa); alarme em 1 GB. Pergunta
   aberta: backup de `~/claude-artefatos`.

## Fila depois do rito

1. `touring index rebuild`, depois o juiz: `python3 scripts/hw-slot --wait 3600 -- python3
   ~/.claude/skills/loop-engineering/scripts/loop_converged.py --task task_1794458342745990025 --scope
   ~/projects/touring --bundle docs/plans/2026-11-12-work-outer` (esperado: `orphans_base` verde).
2. Consertos do detector de órfãos, ordem C5 → C2 → C3 → C1 → C4, com teste RED antes
   (veredito e specs: `~/claude-artefatos/touring/irmas/opus-veredito-kimi-215b15c3.md`) — assentos Sonnet.
3. Propagação da 30.4.110 ao `analise`: só com o "livre" da sessão dona de lá.

## Entregas das irmãs e dos assentos (todas conferidas por execução, salvo indicação)

`~/claude-artefatos/touring/irmas/`: `haiku-conferencia-docs.md` (4 confirmados, 6 falsos; aplicados 2 + espelho) ·
`opus-veredito-kimi-215b15c3.md` (5 REAL; **não executado**) · `minimax-matriz-defasada-20261113.md` (22 linhas;
aplicados nudge, ADW, analise/CLAUDE.md) · `kimi-revisao-pool-20261113.md` (10 achados; 9 consertados em `67794a8f`,
G1 virou texto na doc).

## Achados registrados e não tratados

- Hook `TeammateIdle` manda subagentes `touring-engineer` chamarem `TaskList`/`ToolSearch`, que eles não têm.
- Nó de ADW sem `tier` não recebe `--effort`. A contagem 1/3/5 do pool é anunciada, não imposta.
- `docs/2026-06-25-harness-consolidation-master-plan-v3.md:151` cita `extract_method_calls` como API (é `pub(crate)`).
- `gh` falha nesta máquina: `open ~/.config/gh/config.yml: permission denied`.
