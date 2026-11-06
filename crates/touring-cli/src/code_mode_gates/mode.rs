//! Which presentation is in force for THIS call, and the two human switches that sit beside it.
//!
//! Precedence, outermost first: the `TOURING_CODE_MODE=<v>` PREFIX of the command → the hook
//! process' `TOURING_CODE_MODE` → the `TOURING_CODE_ONLY=1` alias → `[code_mode] mode` of the
//! project's `.touring/touring.toml` → the armed A/B policy → the default (`both`).

use std::path::Path;

use super::arm::{arm_counts_durable, code_mode_arm_armed, resolve_with_policy};
use super::spec::GATE_BYPASS_TOKEN;

/// `true` quando o comando É a rota sancionada (um programa único no sandbox).
pub(super) fn is_sanctioned_route(cmd: &str) -> bool {
    // S24: a path-qualified binary (`.touring/bin/touring run`, the per-project one) is the
    // same route — it was never credited, so a project's own binary looked like a bypass.
    cmd.split(|c: char| c.is_whitespace() || ";|&(".contains(c))
        .collect::<Vec<_>>()
        .windows(2)
        .any(|w| (w[0] == "touring" || w[0].ends_with("/touring")) && w[1] == "run")
}

/// Kill switch global (humano; env do DAEMON — exige `touring daemon-ctl
/// restart` para valer, como `TOURING_PILLAR_INDUCTION_ARMED`).
/// A decisão pura do kill switch HUMANO — injetável: a matriz de
/// hermeticidade da analise-6b (08/10) mediu 29 testes vermelhos com
/// `TOURING_CODE_GATES_DISABLED=1` no ambiente — liam o switch do processo.
pub(super) fn code_gates_disabled_from(v: Option<&str>) -> bool {
    v == Some("1")
}

pub(super) fn code_gates_disabled() -> bool {
    code_gates_disabled_from(std::env::var("TOURING_CODE_GATES_DISABLED").ok().as_deref())
}

/// A forma em que as ferramentas chegam ao modelo NESTE escopo.
///
/// A definição canônica mora em [`touring_foundation::code_mode`] porque DOIS
/// executores a impõem — este hook (`PreToolUse`) e o handshake MCP
/// (`touring-server::server::apply_curation`, S1 TRANSPORT). Duplicar o tipo e
/// o parser recriaria a falha que este workspace já pagou duas vezes: sítios da
/// mesma regra que divergem (2026-08-23) e uma declaração que promete o que o
/// executor não aplica (achado D8, 2026-08-25).
pub(crate) use touring_foundation::code_mode::CodeModePresentation;

// **Calibração REVISTA pelo S3 (27/08/2026) — a lista fixa saiu, o predicado
// de rajada entrou.** A tabela abaixo é a de 25/08 (1.312 chamadas), mantida
// porque foi ela que motivou a revisão, com o dado de 27/08 ao lado
// (115 transcripts, janela 300s, `scripts/s3_burst_distribution.py`):
//
// | classe | colapsava? | chamadas | % isoladas | % volume em rajada ≥2 |
// |---|---|---|---|---|
// | `grep`  | sim  | 770 | 12,6% | 87,4% |
// | `sed-n` | NÃO  | 408 | 18,6% | 81,4% |
// | `ls`    | NÃO  | 338 | 28,7% | 71,3% |
// | `cat`   | sim  | 316 | 26,6% | 73,4% |
// | `wc`    | NÃO  |  34 | 67,6% | 32,4% |
// | `find`  | sim  |  32 | 56,2% | 43,8% (zero rajadas ≥3) |
//
// A lista fixa errava nos dois sentidos ao mesmo tempo: negava `find`, que é
// majoritariamente isolada e não produziu UMA rajada ≥3 em 115 transcripts, e
// deixava passar `sed-n` e `ls`, que juntas somam 746 chamadas com ~76% do
// volume em rajada. Nome de classe não prediz fan-out; o que prediz é o que a
// classe está fazendo NESTA janela — e isso um contador responde, uma lista não.
//
// Os níveis por classe são exatamente o desenho que o DeepSeek ADIOU (*"its
// design depends on evidence about how models split usage under `both`"*). A
// evidência que faltava a eles é esta tabela: rodamos em `both` instrumentado,
// e depois medimos de novo para corrigir a nossa própria primeira leitura.

/// Resolve a apresentação: **prefixo do comando → env do hook → alias → projeto → default**.
///
/// Duas verdades medidas (25/08) moram nesta ordem:
///
/// 1. **Exportar no shell da tool Bash NÃO chega ao hook.** O processo do hook
///    e o shell que executa o comando são IRMÃOS spawnados pelo Claude Code —
///    `export TOURING_CODE_MODE=…` num é invisível ao outro. A env que esta
///    função lê com `std::env::var` é a do processo do hook (a do daemon que
///    o spawnou), não a "da sessão" do operador.
/// 2. **A via que atravessa é a linha de comando.** É assim que
///    `TOURING_GATE_OK=1 <cmd>` já funciona: o hook lê o prefixo `VAR=valor`
///    do próprio comando. Por isso o nível mais externo é o prefixo — útil
///    sobretudo para RELAXAR por-comando (`TOURING_CODE_MODE=native grep …`),
///    simétrico ao token de bypass.
///
/// `TOURING_CODE_ONLY=1` continua valendo como alias de `Code` (compatibilidade
/// com o piloto S-8.1), agora com o escopo e a calibração que lhe faltavam — ele
/// negava TODA classe, `ls` inclusive.
///
/// Os níveis «env do hook» e «alias» chegam INJETADOS (`CodeModeEnv`), nunca
/// lidos do processo no meio da resolução: a matriz de hermeticidade da
/// analise-6b (08/10) mediu 15 testes vermelhos sob `TOURING_CODE_MODE=native`
/// no ambiente — liam `Native` do ambiente em vez do caso sob teste (o env
/// vence o `touring.toml` do projeto scratch, por precedência documentada).
#[derive(Default, Clone)]
pub(crate) struct CodeModeEnv {
    /// Valor de `TOURING_CODE_MODE` (crú — o parse é parte do comportamento).
    pub mode: Option<String>,
    /// Valor de `TOURING_CODE_ONLY` (o alias legado de `Code`).
    pub code_only_alias: Option<String>,
    /// `TOURING_CODE_MODE_ARM_ARMED` (a política do braço A/B só age ARMADA). Lido uma vez
    /// aqui, nunca no meio da resolução: o predicado puro `resolve_with_policy` já recebia
    /// o dado, mas quem o entregava lia o env do processo.
    pub arm_armed: bool,
}

impl CodeModeEnv {
    pub(crate) fn from_env() -> Self {
        Self {
            mode: std::env::var("TOURING_CODE_MODE").ok(),
            code_only_alias: std::env::var("TOURING_CODE_ONLY").ok(),
            arm_armed: code_mode_arm_armed(),
        }
    }

    /// A decisão pura dos níveis env+alias — testável sem tocar o processo.
    fn resolve(&self) -> Option<CodeModePresentation> {
        // parse() é o parser canônico (trim + unquote): um valor entre aspas
        // também resolve.
        if let Some(m) = self.mode.as_deref().and_then(CodeModePresentation::parse) {
            return Some(m);
        }
        if self.code_only_alias.as_deref() == Some("1") {
            return Some(CodeModePresentation::Code);
        }
        None
    }
}

pub(super) fn resolve_presentation(
    project_root: &Path,
    cmd: &str,
    env: &CodeModeEnv,
) -> CodeModePresentation {
    for token in cmd.split_whitespace() {
        let Some((nome, valor)) = token.split_once('=') else {
            break;
        };
        if !nome
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        {
            break;
        }
        if nome == "TOURING_CODE_MODE" {
            match CodeModePresentation::parse(valor) {
                Some(m) => return m,
                None => break, // valor inválido não cala os níveis seguintes
            }
        }
    }
    if let Some(m) = env.resolve() {
        return m;
    }
    // A declaração humana vence SEMPRE: prefixo, env, alias e `touring.toml` já
    // decidiram acima. A política só preenche o espaço que o humano deixou em
    // aberto — sobrescrever uma declaração explícita não seria aprender, seria
    // desobedecer.
    resolve_with_policy(
        project_presentation(project_root),
        env.arm_armed,
        arm_counts_durable(project_root),
    )
}

/// Lê `[code_mode] mode` de `<root>/.touring/touring.toml`.
///
/// Delega ao parser canônico do `touring-foundation` — o mesmo que o handshake
/// MCP consulta, para que a apresentação declarada e a imposta nunca possam
/// divergir. Reexportado no escopo do módulo porque os testes o consultam
/// diretamente.
pub(crate) use touring_foundation::code_mode::project_presentation;

/// S24 — the bypass token counts only where the SHELL reads it (a command prefix), never
/// inside a heredoc body or a quoted string: a test file whose text mentions the token was
/// charged as a bypass and denied by G11 on 25/09.
pub(super) fn has_bypass_token(cmd: &str) -> bool {
    touring_ceg::gateway::gate::mask_shell_data(cmd).contains(GATE_BYPASS_TOKEN)
}
