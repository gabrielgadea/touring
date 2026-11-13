# RETOMADA — sessão `touring-7c` (13/11/2026, estado gravado com o contexto em 55%)

Branch `feat/2026-09-16-shell-quality-wiring-python`, **12 commits à frente do `origin`, sem push**.
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

## Commits desta sessão (todos locais)

| Commit | O quê |
|---|---|
| `2c5ae63c` | pool de modelos: `client/hooks/model_pool.py`, `model_pool_roster.py` (SessionStart), gate `effort:ausente`, opus só sob Fable, doc §1/§2, CLAUDE.md regra 17 |
| `499da5f5` | effort nos 5 `touring-*`; `adw.py` passa `--effort` por tier (`tiers.toml [effort]`); lint de codegen → `workhorse`; seletor de status da regra `touring-cli-index.md` |
| `12043c9f` | guarda de injeção vê shim entre aspas; 20 achados de lint em 9 hooks |
| `67794a8f` | 9 defeitos achados pela revisão do Kimi (gate, roster, detector de shim) |
| `acf5871d` | contrato de saída do subagente, 1ª versão (teto de 8000) — **superada** |
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

## Aguardando o sim do Gabriel (recomendações do canvas)

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
