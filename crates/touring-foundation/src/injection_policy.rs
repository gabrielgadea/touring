//! Política de injeção de contexto dos hooks — o interruptor
//! `TOURING_HOOK_INJECTION`, nativo (S-4, 26/10/2026).
//!
//! RESUMO
//! ------
//! O corpus de 1.028 sessões (21 dias, 440.867 invocações de hook) mostrou que
//! a injeção de contexto responde por 27,8% dos bytes da janela do modelo e
//! que o perfil dela é misto: parte carrega FATOS sobre o código (blast
//! radius, file context, syntax pre-check) e parte carrega SERMÃO (nudges, eco
//! de pós-tool, log de depuração). Desligar tudo junto perderia os fatos;
//! deixar como está paga o sermão.
//!
//! ```text
//! full         (padrão) — nada é suprimido; comportamento idêntico ao atual
//! no-advisory  — suprime só as famílias de sermão; mantém fatos e gates
//! off          — suprime todo o contexto injetado; mantém TODAS as decisões
//! ```
//!
//! CONTRATO DE ENFORCEMENT (o que este módulo NÃO toca)
//! ----------------------------------------------------
//! A injeção de contexto e o enforcement são canais separados do protocolo de
//! hook. As decisões vivem em `permissionDecision`, `permissionDecisionReason`,
//! `decision`, `reason` e no exit code ([`DECISION_FIELDS`]); o contexto vive
//! em `hookSpecificOutput.additionalContext`, `systemMessage` (raiz) e
//! `hookSpecificOutput.systemMessage` ([`CONTEXT_FIELDS`]). Este módulo remove
//! SOMENTE os campos de contexto. Um hook que bloqueia continua bloqueando,
//! com o mesmo código de saída e a mesma razão.
//!
//! ASSIMETRIA DECLARADA — o que cada nível garante
//! -----------------------------------------------
//! - `off` → remove TUDO, incondicionalmente. Mesmo um payload que nenhuma
//!   família reconhece sai. **Este nível não tem fail-open.**
//! - `no-advisory` → remove só as famílias comprovadamente de sermão. Um
//!   payload **não reconhecido passa** — decisão medida, não descuido: o canal
//!   de enriquecimento (blast radius, file context, syntax pre-check) é o de
//!   MAIOR volume e de MAIOR impacto (59,9% — §4.6 da análise consolidada) e
//!   **não tem marcador confiável**; classificá-lo por substring descartaria
//!   conhecimento do repositório. O log de depuração — que é ruído — tem
//!   marcador próprio (`debug-stdout`) e é cortado.
//! - `full` → não remove nada.
//!
//! A taxonomia de famílias NÃO é duplicada aqui: ela mora em
//! [`crate::injection_family`] (a régua canônica) e este módulo a consome. O
//! espelho Python (`client/hooks/injection_policy.py`) carrega as mesmas
//! listas de nomes — o teste
//! [`tests::the_rust_policy_mirrors_the_python_policy`] reprova quando os dois
//! lados divergem.
//!
//! ONDE É APLICADA (S-2, 27/10/2026)
//! ---------------------------------
//! Os sítios de emissão chamam o MESMO helper — [`apply_at`] / [`apply`] para
//! uma saída inteira, [`retain_allowed`] para os avisos que viajam colados num
//! veredito — e cada corte vai para o diário
//! ([`crate::injection_journal`], lido pelo `touring kpi`):
//!
//! ```text
//! SITE_DAEMON_FORWARD  touring-hooks/main.rs      forward_daemon_output (rota daemon, no CLIENTE)
//! SITE_EMIT_RUNTIME    touring-hooks-core         HookResponse::emit    (o enum único desde o I-3, 31/10)
//! SITE_VERDICT         touring-hook-runtime       verdict.rs            (avisos coláveis)
//! SITE_STANDALONE      touring-hooks/main.rs      prompt-enhance, task-validation
//! ```
//!
//! O nível vem do ambiente do processo que EMITE ([`current`]): no cliente
//! `touring-hook` é o do Claude Code (o braço de um experimento troca por
//! sessão). Na rota do daemon, o cliente o transporta no `DaemonRequest` e o
//! actor o aplica por requisição ([`with_level_override`]); uma requisição
//! legada, sem o campo, mantém o ambiente do boot do daemon. `Full` — o
//! default — não parseia, não aloca e não toca o disco: a saída é a de antes,
//! byte a byte.

use crate::injection_family;
use crate::injection_journal::{self, Cut};
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};

/// A env var do interruptor único.
pub const ENV_VAR: &str = "TOURING_HOOK_INJECTION";

/// Nível de injeção em vigor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InjectionLevel {
    /// Nada é suprimido (default).
    Full,
    /// Suprime só as famílias de sermão ([`ADVISORY_FAMILIES`]).
    NoAdvisory,
    /// Suprime todo o contexto injetado; preserva TODAS as decisões.
    Off,
}

impl InjectionLevel {
    /// Parse tolerante: trim + lowercase, como a política Python. Valor fora da
    /// tabela devolve `None` (o chamador decide o default).
    #[must_use]
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "full" => Some(Self::Full),
            "no-advisory" => Some(Self::NoAdvisory),
            "off" => Some(Self::Off),
            _ => None,
        }
    }

    /// O nome do nível, para telemetria (`injection_policy_active` no KPI).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::NoAdvisory => "no-advisory",
            Self::Off => "off",
        }
    }
}

/// Default fail-open: valor mal digitado NÃO apaga a injeção.
pub const DEFAULT_LEVEL: InjectionLevel = InjectionLevel::Full;

/// Famílias que são SERMÃO: suprimidas em `no-advisory`.
///
/// `past-lessons` (o aviso de "Similar command failed recently", que **não
/// nomeia o comando que falou** — logo não tem alvo e o modelo não pode
/// obedecê-lo) e `generic-noise` (o payload literalmente `[generic]`) entraram
/// na rodada 3/4: o corpus mediu **1.676 blocos** e **1.499 blocos**
/// respectivamente.
///
/// TRÊS NOMES COM "session/contract", e são TRÊS COISAS:
/// `session-start` — os DASHBOARDS da abertura (Advisory, o nome da régua);
/// `code-mode-contract` — o CONTRATO do code mode (Fact);
/// `session-contract` — o bloco "MUST touring doctor -j" (Fact).
pub static ADVISORY_FAMILIES: &[&str] = &[
    "touring-suggest",
    "prompt-enhance",
    "plugin-hook",
    "post-tool-echo",
    "debug-stdout",
    "past-lessons",
    "generic-noise",
    "delegation-nudge",
    "session-start",
    "pre-bash",
    "prose-integrity",
];

/// Famílias que carregam FATO sobre o código ou PAYLOAD DE GATE. Nenhuma é
/// suprimida em `no-advisory`.
pub static FACT_FAMILIES: &[&str] = &[
    "pre-edit",
    "pre-write",
    "pre-read",
    "post-edit",
    "post-write",
    "pre-edit-prevention",
    "code-mode-contract",
    "session-contract",
    "compaction-gaps",
    "context-monitor",
    "pipefail-evidence",
    "evidence-guard",
    "code-mode-deny",
    "loop-outer",
    "failure-echo",
    "quality-gate",
];

/// Campos que carregam DECISÃO — nunca removidos por esta política.
///
/// `systemMessage` NÃO está aqui: ele é uma mensagem, não um veredito. Sob
/// `off` ele é removido como contexto, e sob `no-advisory` segue o regime das
/// demais famílias. A revisão de segurança de 26/10/2026 apontou a confusão, e
/// ela estava certa: tratar `systemMessage` como decisão fazia o nível `off`
/// NÃO desligar tudo.
pub static DECISION_FIELDS: &[&str] = &[
    "permissionDecision",
    "permissionDecisionReason",
    "decision",
    "reason",
    "continue",
    "stopReason",
    "suppressOutput",
];

/// Campos que carregam CONTEXTO — sujeitos à política.
pub static CONTEXT_FIELDS: &[&str] = &["additionalContext", "systemMessage"];

/// Decisão pura, injetável (molde de `code_gates_disabled_from`,
/// `crates/touring-cli/src/cli_suggester.rs:3940`). É o que permite testar a
/// política **sem ambiente** — e ao daemon resolver uma vez e propagar.
#[must_use]
pub fn level_from_raw(raw: Option<&str>) -> InjectionLevel {
    raw.and_then(InjectionLevel::parse).unwrap_or(DEFAULT_LEVEL)
}

/// Só o CLIENTE resolve pelo ambiente: o env do daemon é o do boot
/// (`crates/touring-cli/src/cli_suggester.rs:3935-3936`).
#[must_use]
pub fn level_from_env() -> InjectionLevel {
    level_from_raw(std::env::var(ENV_VAR).ok().as_deref())
}

thread_local! {
    /// Pilha de níveis por requisição. Um frame `None` significa "herdar o
    /// daemon" e, por isso, esconde frames de requisições mais externas.
    static REQUEST_LEVELS: RefCell<Vec<Option<InjectionLevel>>> = const { RefCell::new(Vec::new()) };
    /// Nível-base do daemon nesta thread. Separado da pilha para que uma
    /// requisição legada aninhada durante yield não herde a requisição heavy.
    static DAEMON_LEVEL_OVERRIDE: Cell<Option<InjectionLevel>> = const { Cell::new(None) };
}

/// Trunca a pilha à profundidade anterior mesmo durante unwinding.
struct RequestLevelReset(usize);

impl Drop for RequestLevelReset {
    fn drop(&mut self) {
        REQUEST_LEVELS.with(|levels| levels.borrow_mut().truncate(self.0));
    }
}

/// Restaura o nível-base anterior mesmo durante unwinding.
struct DaemonLevelReset(Option<InjectionLevel>);

impl Drop for DaemonLevelReset {
    fn drop(&mut self) {
        DAEMON_LEVEL_OVERRIDE.with(|slot| slot.set(self.0));
    }
}

fn with_request_level<R>(level: Option<InjectionLevel>, run: impl FnOnce() -> R) -> R {
    let depth = REQUEST_LEVELS.with(|levels| {
        let mut levels = levels.borrow_mut();
        let depth = levels.len();
        levels.push(level);
        depth
    });
    let _reset = RequestLevelReset(depth);
    run()
}

/// Executa `run` com `level` no lugar do ambiente deste processo.
///
/// Serve à fronteira cliente → daemon: a requisição carrega o nível do Claude
/// Code e o actor o aplica antes de chamar o handler. Overrides podem ser
/// aninhados; o nível anterior sempre volta ao sair, inclusive em pânico.
pub fn with_level_override<R>(level: InjectionLevel, run: impl FnOnce() -> R) -> R {
    with_request_level(Some(level), run)
}

/// Executa uma requisição legada com o nível-base do daemon, sem herdar uma
/// requisição mais externa que esteja fazendo yield no mesmo actor.
pub fn with_inherited_level<R>(run: impl FnOnce() -> R) -> R {
    with_request_level(None, run)
}

/// Executa `run` com o nível-base do daemon nesta thread.
///
/// Produção normalmente usa o ambiente de boot. O override-base existe para
/// embeddings e testes que precisam simular esse ambiente sem mudar o processo.
pub fn with_daemon_level_override<R>(level: InjectionLevel, run: impl FnOnce() -> R) -> R {
    let previous = DAEMON_LEVEL_OVERRIDE.with(|slot| slot.replace(Some(level)));
    let _reset = DaemonLevelReset(previous);
    run()
}

/// Nível da requisição atual, ou o nível-base do daemon fora de um override.
#[must_use]
fn current_level() -> InjectionLevel {
    let request = REQUEST_LEVELS.with(|levels| levels.borrow().last().copied());
    match request {
        Some(Some(level)) => level,
        _ => DAEMON_LEVEL_OVERRIDE
            .with(Cell::get)
            .unwrap_or_else(level_from_env),
    }
}

/// O contexto desta família deve chegar ao modelo?
///
/// `level` explícito vence o ambiente; sem ele, resolve do ambiente.
///
/// **Família DESCONHECIDA devolve `false` (não injeta).** Deliberado — a
/// correção da falha silenciosa de 26/10/2026 (§8.11 da análise consolidada):
/// o rename de `session-start` deixou call sites com um nome inexistente, e um
/// nome inexistente caía em "permitir" — os dashboards seguiam sendo injetados
/// e **nada acusava**. Falhar fechado na injeção (e aberto na sessão, porque
/// isto não levanta exceção) troca "injeta por engano" por "não injeta".
#[must_use]
pub fn allows(family: &str, level: Option<InjectionLevel>) -> bool {
    if !ADVISORY_FAMILIES.contains(&family) && !FACT_FAMILIES.contains(&family) {
        return false;
    }
    match level.unwrap_or_else(level_from_env) {
        InjectionLevel::Full => true,
        InjectionLevel::NoAdvisory => !ADVISORY_FAMILIES.contains(&family),
        InjectionLevel::Off => false,
    }
}

/// Este payload inteiro deve chegar ao modelo?
///
/// Conservador por construção: em `no-advisory` o payload só é suprimido
/// quando NÃO contém nenhuma família de fato/gate. Um payload que misture
/// sermão e fato é preservado inteiro — nunca se perde conhecimento do
/// repositório por causa de um aviso que veio de carona.
#[must_use]
pub fn allows_payload(payload: &str, level: Option<InjectionLevel>) -> bool {
    match level.unwrap_or_else(level_from_env) {
        InjectionLevel::Full => true,
        InjectionLevel::Off => false,
        InjectionLevel::NoAdvisory => {
            let found = injection_family::families(payload);
            if found.is_empty() {
                // DESCONHECIDO: não se descarta. Preferimos deixar passar um
                // payload que não reconhecemos a descartar um fato do
                // repositório. O log de depuração cru tem marcador próprio
                // (`debug-stdout`) e é coberto.
                return true;
            }
            found.iter().any(|f| !ADVISORY_FAMILIES.contains(f))
        }
    }
}

/// Remove os campos de contexto de um holder JSON, preservando decisões.
///
/// A remoção é por ALVO (só [`CONTEXT_FIELDS`] são considerados), então uma
/// decisão nunca sai — e a guarda abaixo torna isso enforcement, não
/// declaração: se um dia um campo aparecer nas duas listas, a DECISÃO vence
/// (D8 — a regra vive no executor, não no comentário).
///
/// Cada campo removido entra em `cuts` com a família PRIMÁRIA do texto e seu
/// tamanho — é a telemetria do corte (S-3), medida no único lugar onde o corte
/// acontece.
fn strip_holder(
    holder: &mut serde_json::Map<String, serde_json::Value>,
    level: InjectionLevel,
    cuts: &mut Vec<Cut>,
) {
    for field in CONTEXT_FIELDS {
        if DECISION_FIELDS.contains(field) {
            continue;
        }
        let Some(text) = holder.get(*field).and_then(|v| v.as_str()) else {
            continue;
        };
        if !allows_payload(text, Some(level)) {
            cuts.push(Cut {
                family: injection_family::injection_family(text),
                bytes: text.len(),
            });
            holder.remove(*field);
        }
    }
}

/// Um `hookSpecificOutput` que sobrou só com o nome do evento não leva nada ao
/// modelo: sem o contexto cortado, é o `{}` do Allow com um rótulo.
fn is_bare_event(hso: &serde_json::Map<String, serde_json::Value>) -> bool {
    hso.len() == 1 && hso.contains_key("hookEventName")
}

/// A passada sobre um envelope JSON já parseado: `hookSpecificOutput` primeiro,
/// raiz depois — a mesma ordem do espelho Python.
///
/// `collapse_bare_event` (só a camada de aplicação liga): depois de um corte,
/// um `hookSpecificOutput` reduzido ao `hookEventName` também sai, e o
/// envelope vira o `{}` canônico em vez de uma forma que depende de a
/// plataforma aceitar um hso sem conteúdo em todo evento.
fn strip_envelope(
    obj: &mut serde_json::Value,
    level: InjectionLevel,
    collapse_bare_event: bool,
    cuts: &mut Vec<Cut>,
) {
    if let Some(hso) = obj.get_mut("hookSpecificOutput") {
        if let Some(map) = hso.as_object_mut() {
            strip_holder(map, level, cuts);
        }
        let emptied = hso.as_object().is_some_and(|m| {
            m.is_empty() || (collapse_bare_event && !cuts.is_empty() && is_bare_event(m))
        });
        if emptied && let Some(root) = obj.as_object_mut() {
            root.remove("hookSpecificOutput");
        }
    }
    if let Some(root) = obj.as_object_mut() {
        strip_holder(root, level, cuts);
    }
}

/// O texto que não é envelope JSON (stdout cru): decide pelo conjunto de
/// famílias do texto inteiro.
fn filter_raw(level: InjectionLevel, text: &str, stripped: &str) -> (String, Vec<Cut>) {
    if allows_payload(stripped, Some(level)) {
        return (text.to_string(), Vec::new());
    }
    let cut = Cut {
        family: injection_family::injection_family(stripped),
        bytes: stripped.len(),
    };
    (String::new(), vec![cut])
}

/// O miolo único de `filter_output` (paridade com o espelho Python) e de
/// [`apply_at`] (a aplicação nos sítios de emissão): devolve o texto filtrado
/// E o que foi cortado.
fn filter_core(level: InjectionLevel, text: &str, collapse_bare_event: bool) -> (String, Vec<Cut>) {
    if level == InjectionLevel::Full {
        return (text.to_string(), Vec::new());
    }
    let stripped = text.trim();
    if stripped.is_empty() {
        return (text.to_string(), Vec::new());
    }
    let looks_json = stripped.starts_with('{') || stripped.starts_with('[');
    let parsed = looks_json
        .then(|| serde_json::from_str::<serde_json::Value>(stripped).ok())
        .flatten();
    let Some(mut obj) = parsed else {
        return filter_raw(level, text, stripped);
    };
    if !obj.is_object() {
        return (text.to_string(), Vec::new());
    }
    let mut cuts = Vec::new();
    strip_envelope(&mut obj, level, collapse_bare_event, &mut cuts);
    // A serialização compacta é a forma do protocolo; se falhar (não deve —
    // o objeto veio de um parse válido), devolve a entrada intocada, que é a
    // direção fail-open da sessão — e nada foi cortado.
    serde_json::to_string(&obj).map_or_else(|_| (text.to_string(), Vec::new()), |s| (s, cuts))
}

/// Filtra a resposta de um hook conforme a política.
///
/// Texto que não é JSON (stdout cru) é tratado pelo conjunto de famílias do
/// texto: o corpus mediu 5,3 MB (9,0% de toda a injeção) de log de depuração
/// cru chegando ao modelo por esse caminho, e 1% de unicidade — é ruído, não
/// contexto. Em `no-advisory` e em `off` ele é suprimido (em `no-advisory`,
/// só quando nenhuma família de fato casa).
///
/// A decisão do hook (permissionDecision, decision, exit code) nunca é
/// tocada: quem bloqueia continua bloqueando. Em `full` a saída é
/// byte-idêntica à entrada.
#[must_use]
pub fn filter_output(level: InjectionLevel, text: &str) -> String {
    filter_core(level, text, false).0
}

// ── aplicação nos sítios de emissão (S-2, 27/10/2026) ─────────────────────
//
// `filter_output` é a regra pura (paridade com o espelho Python). O que segue
// é o que os sítios de emissão dos hooks chamam: o MESMO miolo, mais o
// diário do que foi cortado ([`crate::injection_journal`]). Um único helper
// para os 4 sítios (regra C08 — o predicado escrito 4 vezes nunca se
// encontra).

/// Sítio: o cliente `touring-hook` repassando a resposta do daemon
/// (`forward_daemon_output`).
pub const SITE_DAEMON_FORWARD: &str = "daemon-forward";

/// Sítio: `HookResponse::emit` (rota standalone). Desde o I-3 (31/10/2026) o
/// enum é um só, em `touring-hooks-core`; o rótulo `emit-runtime` ficou porque
/// era o do emissor que de fato rodava. O antigo `emit-core` não tinha chamador
/// e foi removido; linhas antigas do diário ainda podem trazê-lo.
pub const SITE_EMIT_RUNTIME: &str = "emit-runtime";

/// Sítio: a montagem do veredito (`verdict.rs`) — os avisos de gates que
/// viajam colados na razão do deny ou no contexto.
pub const SITE_VERDICT: &str = "verdict";

/// Sítio: saídas standalone que imprimem JSON sem passar por `HookResponse`
/// (`prompt-enhance`, `task-validation`).
pub const SITE_STANDALONE: &str = "standalone";

/// Registra no diário o que a política cortou. Fail-open: sem sítio de
/// gravação, ou com erro de I/O, o hook segue como se nada tivesse acontecido.
fn journal_cuts(sink: Option<&Path>, level: InjectionLevel, site: &str, cuts: &[Cut]) {
    if cuts.is_empty() {
        return;
    }
    // The live counter counts every cut, journal or not: a sink-less process
    // (no HOME) still cut those bytes from the window.
    crate::gate_metrics::record_injection_suppressed(
        cuts.len() as u64,
        cuts.iter().map(|c| c.bytes as u64).sum(),
    );
    if let Some(path) = sink {
        let _ = injection_journal::record(path, level.as_str(), site, cuts);
    }
}

/// Aplica a política a UMA saída de hook, no nível dado.
///
/// - `Full` devolve o texto **emprestado**, sem alocar, sem parsear e sem
///   tocar o disco — o comportamento de hoje, byte a byte.
/// - Nada cortado devolve o texto original **emprestado**: um envelope que a
///   política não alterou não é re-serializado.
/// - Algo cortado devolve o envelope filtrado e registra o corte em `sink`.
///   Só `additionalContext`/`systemMessage` saem; `permissionDecision`,
///   `permissionDecisionReason`, `updatedInput`, `updatedToolOutput` (o
///   resultado reescrito do PostToolUse, S-14), `decision`, `reason`,
///   `continue`, `stopReason` ficam como vieram (contrato C13). Um envelope
///   cujo único conteúdo era contexto vira `{}`.
#[must_use]
pub fn apply_at<'a>(
    level: InjectionLevel,
    site: &str,
    text: &'a str,
    sink: Option<&Path>,
) -> Cow<'a, str> {
    if level == InjectionLevel::Full {
        return Cow::Borrowed(text);
    }
    let (filtered, cuts) = filter_core(level, text, true);
    if cuts.is_empty() {
        return Cow::Borrowed(text);
    }
    journal_cuts(sink, level, site, &cuts);
    Cow::Owned(filtered)
}

/// O nível em vigor nesta thread e, só quando ele corta, o diário onde
/// registrar. `Full` não resolve `HOME` nem aloca caminho nenhum.
///
/// Fora de um override, quem chama é o processo que EMITE: no cliente
/// `touring-hook` o ambiente é o do Claude Code; no daemon é o do boot. Dentro
/// do project actor, [`with_level_override`] substitui esse ambiente pelo nível
/// transportado na requisição. O repasse da resposta ao stdout
/// ([`SITE_DAEMON_FORWARD`]) acontece no CLIENTE.
#[must_use]
pub fn current() -> (InjectionLevel, Option<PathBuf>) {
    let level = current_level();
    if level == InjectionLevel::Full {
        return (level, None);
    }
    (level, injection_journal::journal_path())
}

/// [`apply_at`] com o nível e o diário do ambiente deste processo.
#[must_use]
pub fn apply<'a>(site: &str, text: &'a str) -> Cow<'a, str> {
    let (level, sink) = current();
    apply_at(level, site, text, sink.as_deref())
}

/// Os itens que a política deixa chegar ao modelo, no nível dado — o mesmo
/// predicado de [`allows_payload`], item a item.
///
/// É o sítio do `verdict.rs`: um aviso de gate que viaja colado num veredito
/// contamina o payload inteiro (a política é conservadora com mistura de
/// sermão e fato e preserva tudo), então o filtro age ANTES da colagem. Em
/// `Full` devolve os itens como vieram.
#[must_use]
pub fn retain_allowed(
    level: InjectionLevel,
    site: &str,
    items: Vec<String>,
    sink: Option<&Path>,
) -> Vec<String> {
    if level == InjectionLevel::Full {
        return items;
    }
    let mut kept = Vec::with_capacity(items.len());
    let mut cuts = Vec::new();
    for item in items {
        if allows_payload(&item, Some(level)) {
            kept.push(item);
        } else {
            cuts.push(Cut {
                family: injection_family::injection_family(&item),
                bytes: item.len(),
            });
        }
    }
    journal_cuts(sink, level, site, &cuts);
    kept
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    use std::collections::BTreeSet;

    // ── nível ──────────────────────────────────────────────────────────────

    #[test]
    fn level_from_raw_aceita_os_tres_niveis_e_e_tolerante() {
        assert_eq!(level_from_raw(Some("full")), InjectionLevel::Full);
        assert_eq!(
            level_from_raw(Some("no-advisory")),
            InjectionLevel::NoAdvisory
        );
        assert_eq!(level_from_raw(Some("off")), InjectionLevel::Off);
        // tolerância do espelho Python: trim + lowercase.
        assert_eq!(
            level_from_raw(Some("  No-Advisory \n")),
            InjectionLevel::NoAdvisory
        );
        assert_eq!(level_from_raw(Some("OFF")), InjectionLevel::Off);
    }

    /// Default fail-open: valor mal digitado ou ausente NÃO apaga a injeção.
    #[test]
    fn level_from_raw_com_valor_ruim_ou_ausente_e_full() {
        assert_eq!(level_from_raw(None), InjectionLevel::Full);
        assert_eq!(level_from_raw(Some("ful")), InjectionLevel::Full);
        assert_eq!(level_from_raw(Some("")), InjectionLevel::Full);
        assert_eq!(DEFAULT_LEVEL, InjectionLevel::Full);
    }

    /// A env var é a fonte do interruptor. Serializado pelo mutex: outro teste
    /// deste processo não pode ler o valor no meio da troca.
    #[test]
    fn override_aninha_e_restaura_o_nivel_mesmo_com_panico() {
        with_level_override(InjectionLevel::Full, || {
            assert_eq!(current().0, InjectionLevel::Full);
            with_level_override(InjectionLevel::NoAdvisory, || {
                assert_eq!(current().0, InjectionLevel::NoAdvisory);
                with_level_override(InjectionLevel::Off, || {
                    assert_eq!(current().0, InjectionLevel::Off);
                });
                assert_eq!(current().0, InjectionLevel::NoAdvisory);
            });
            assert_eq!(current().0, InjectionLevel::Full);

            let panic = std::panic::catch_unwind(|| {
                with_level_override(InjectionLevel::NoAdvisory, || {
                    assert_eq!(current().0, InjectionLevel::NoAdvisory);
                    panic!("força a restauração durante unwinding");
                });
            });
            assert!(panic.is_err());
            assert_eq!(current().0, InjectionLevel::Full);
        });
    }

    #[test]
    fn requisicao_legada_usa_o_nivel_base_mesmo_sob_override_externo() {
        with_daemon_level_override(InjectionLevel::NoAdvisory, || {
            with_level_override(InjectionLevel::Full, || {
                with_inherited_level(|| {
                    assert_eq!(current().0, InjectionLevel::NoAdvisory);
                });
                assert_eq!(current().0, InjectionLevel::Full);
            });
            assert_eq!(current().0, InjectionLevel::NoAdvisory);
        });
    }

    #[test]
    fn level_from_env_le_a_env_var_do_interruptor() {
        static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _g = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // SAFETY: a escrita na env é serializada pelo ENV_LOCK acima e nenhum
        // outro teste deste crate lê `TOURING_HOOK_INJECTION`.
        unsafe {
            std::env::set_var(ENV_VAR, "off");
        }
        assert_eq!(level_from_env(), InjectionLevel::Off);
        unsafe {
            std::env::set_var(ENV_VAR, "no-advisory");
        }
        assert_eq!(level_from_env(), InjectionLevel::NoAdvisory);
        unsafe {
            std::env::remove_var(ENV_VAR);
        }
        assert_eq!(level_from_env(), InjectionLevel::Full);
    }

    #[test]
    fn o_nome_do_nivel_e_o_valor_da_env_var() {
        for (level, name) in [
            (InjectionLevel::Full, "full"),
            (InjectionLevel::NoAdvisory, "no-advisory"),
            (InjectionLevel::Off, "off"),
        ] {
            assert_eq!(level.as_str(), name);
            assert_eq!(InjectionLevel::parse(name), Some(level));
        }
    }

    // ── allows (por família) ───────────────────────────────────────────────

    #[test]
    fn full_libera_tudo() {
        for family in ADVISORY_FAMILIES.iter().chain(FACT_FAMILIES) {
            assert!(allows(family, Some(InjectionLevel::Full)), "{family}");
        }
    }

    #[test]
    fn no_advisory_corta_so_o_sermao() {
        for family in ADVISORY_FAMILIES {
            assert!(
                !allows(family, Some(InjectionLevel::NoAdvisory)),
                "{family} e' sermao e devia sair"
            );
        }
        for family in FACT_FAMILIES {
            assert!(
                allows(family, Some(InjectionLevel::NoAdvisory)),
                "{family} e' fato e devia ficar"
            );
        }
    }

    #[test]
    fn off_corta_tudo() {
        for family in ADVISORY_FAMILIES.iter().chain(FACT_FAMILIES) {
            assert!(!allows(family, Some(InjectionLevel::Off)), "{family}");
        }
    }

    /// §8.11: um nome que a política não conhece NÃO injeta — o modo de falha
    /// do rename que chegou ao vivo ("injeta por engano") virou "não injeta".
    #[test]
    fn familia_desconhecida_falha_fechado() {
        for name in [
            "session-start-contract",
            "similar-fail",
            "compact-warning",
            "",
        ] {
            assert!(
                !allows(name, Some(InjectionLevel::Full)),
                "{name:?} nao existe e nao pode injetar"
            );
        }
    }

    // ── allows_payload (pelo conjunto) ─────────────────────────────────────

    #[test]
    fn no_advisory_preserva_o_fato_que_veio_de_carona_no_sermao() {
        let mixed = "[TOURING SUGGEST · x]\nHOTSPOT: a.rs blast 12";
        assert!(
            allows_payload(mixed, Some(InjectionLevel::NoAdvisory)),
            "mistura de sermao e fato e preservada inteira"
        );
        // `⚡ last fail:` e' marcador de post-tool-echo (sermão) E padrão de
        // failure-echo (fato) — verificado contra o espelho Python em
        // 26/10/2026: o conjunto manda, e o payload passa.
        assert!(allows_payload(
            "⚡ last fail: python3 — sqlite3.OperationalError",
            Some(InjectionLevel::NoAdvisory)
        ));
        assert!(
            !allows_payload("[TOURING SUGGEST · x]", Some(InjectionLevel::NoAdvisory)),
            "sermao puro sai"
        );
    }

    #[test]
    fn o_desconhecido_passa_em_no_advisory_e_sai_em_off() {
        let unknown = "[gabrielgadea] payload sem nenhum marcador";
        assert!(allows_payload(unknown, Some(InjectionLevel::NoAdvisory)));
        assert!(
            !allows_payload(unknown, Some(InjectionLevel::Off)),
            "off e' incondicional — nao tem fail-open"
        );
        assert!(allows_payload(unknown, Some(InjectionLevel::Full)));
    }

    // ── filter_output ──────────────────────────────────────────────────────

    fn envelope(ctx: &str) -> String {
        json!({
            "hookSpecificOutput": {"hookEventName": "PreToolUse", "additionalContext": ctx},
            "permissionDecision": "deny",
            "permissionDecisionReason": "gate G2",
        })
        .to_string()
    }

    #[test]
    fn full_e_byte_identico() {
        let input = envelope("[TOURING SUGGEST · x]\n  MUST touring doctor -j");
        assert_eq!(filter_output(InjectionLevel::Full, &input), input);
        let raw = "[EVENT_READER] lixo de depuracao";
        assert_eq!(filter_output(InjectionLevel::Full, raw), raw);
    }

    /// O contrato de enforcement: `off` remove TODO o contexto e preserva
    /// TODA decisão — o gate continua falante.
    #[test]
    fn off_remove_o_contexto_e_preserva_a_decisao() {
        let out = filter_output(
            InjectionLevel::Off,
            &envelope("HOTSPOT: a.rs blast 12\nMUST touring ast blast a.rs"),
        );
        let v: Value = serde_json::from_str(&out).expect("saida continua JSON");
        assert_eq!(v["permissionDecision"], json!("deny"));
        assert_eq!(v["permissionDecisionReason"], json!("gate G2"));
        let hso = &v["hookSpecificOutput"];
        assert!(
            hso.get("additionalContext").is_none(),
            "o contexto some: {out}"
        );
        assert_eq!(
            hso["hookEventName"],
            json!("PreToolUse"),
            "o que nao e' contexto nem decisao fica"
        );
    }

    #[test]
    fn off_remove_o_system_message_da_raiz_e_o_hook_esvaziado() {
        let input = json!({
            "hookSpecificOutput": {"additionalContext": "HOTSPOT: a.rs"},
            "systemMessage": "Contexto em 63% — a autocompactacao dispara aos 65%.",
            "decision": "block",
            "reason": "gate falhou",
        })
        .to_string();
        let out = filter_output(InjectionLevel::Off, &input);
        let v: Value = serde_json::from_str(&out).expect("saida continua JSON");
        assert!(v.get("systemMessage").is_none(), "{out}");
        assert!(
            v.get("hookSpecificOutput").is_none(),
            "hookSpecificOutput vazio nao sobra no envelope: {out}"
        );
        assert_eq!(v["decision"], json!("block"));
        assert_eq!(v["reason"], json!("gate falhou"));
    }

    #[test]
    fn um_corte_soma_no_contador_vivo_mesmo_sem_diario() {
        // I-2: `touring gate-metrics -j` mostra o que ESTE processo cortou. Sem
        // sink (processo sem HOME) o corte ainda saiu da janela e conta. Delta
        // `>=` porque o contador é global e outros testes cortam em paralelo.
        let snap = || crate::gate_metrics_snapshot::GateMetricsSnapshot::capture();
        let before = snap();
        let text = envelope("[TOURING SUGGEST · x]\n  MUST touring ast meta a.rs");
        let out = apply_at(InjectionLevel::NoAdvisory, SITE_VERDICT, &text, None);
        assert_ne!(out.as_ref(), text, "o sermão foi cortado");
        let after = snap();
        assert!(
            after.injection_suppressed_blocks_count > before.injection_suppressed_blocks_count,
            "o corte precisa contar"
        );
        assert!(
            after.injection_suppressed_bytes_count > before.injection_suppressed_bytes_count,
            "os bytes cortados precisam contar"
        );
        // `full` não corta nada e não conta nada pelo próprio caminho.
        let full = apply_at(InjectionLevel::Full, SITE_VERDICT, &text, None);
        assert_eq!(full.as_ref(), text);
        assert!(
            !after.injection_level.is_empty(),
            "o nível vigente é publicado"
        );
    }

    #[test]
    fn no_advisory_remove_o_sermao_e_mantem_o_fato() {
        let sermon = filter_output(
            InjectionLevel::NoAdvisory,
            &envelope("[TOURING SUGGEST · x]\n  MUST touring ast meta a.rs"),
        );
        let v: Value = serde_json::from_str(&sermon).expect("saida continua JSON");
        assert!(
            v["hookSpecificOutput"].get("additionalContext").is_none(),
            "{sermon}"
        );
        assert_eq!(v["permissionDecision"], json!("deny"));

        let fact = envelope("HOTSPOT: a.rs blast 12\nMUST touring ast blast a.rs");
        let out = filter_output(InjectionLevel::NoAdvisory, &fact);
        let v: Value = serde_json::from_str(&out).expect("saida continua JSON");
        assert!(
            v["hookSpecificOutput"]["additionalContext"]
                .as_str()
                .is_some_and(|s| s.contains("HOTSPOT: a.rs")),
            "o fato fica: {out}"
        );
    }

    /// stdout cru (nao-JSON) e o caminho dos 5,3 MB de log de depuracao:
    /// cortado quando so tem marca de ruido, preservado quando carrega fato.
    #[test]
    fn stdout_cru_segue_o_conjunto_de_familias() {
        let debug = "[EVENT_READER] 2026-10-26 hook trace";
        assert_eq!(
            filter_output(InjectionLevel::NoAdvisory, debug),
            "",
            "debug-stdout e' sermao e sai"
        );
        let desconhecido = "aviso solto sem json e sem marcador";
        assert_eq!(
            filter_output(InjectionLevel::NoAdvisory, desconhecido),
            desconhecido
        );
        assert_eq!(filter_output(InjectionLevel::Off, desconhecido), "");
    }

    /// Um corpo JSON que não é OBJETO não é envelope de hook. O array volta
    /// intocado; a string crua não começa por `{`/`[` e segue o caminho do
    /// stdout cru — sob `off`, sai. Comportamento verificado contra o espelho
    /// Python em 26/10/2026 (`filter_output('off', '"texto"') == ''`).
    #[test]
    fn json_que_nao_e_envelope_segue_a_regra_do_espelho() {
        let arr = json!(["a", "b"]).to_string();
        assert_eq!(filter_output(InjectionLevel::Off, &arr), arr);
        let s = json!("texto").to_string();
        assert_eq!(filter_output(InjectionLevel::Off, &s), "");
        assert_eq!(filter_output(InjectionLevel::NoAdvisory, &s), s);
        assert_eq!(filter_output(InjectionLevel::Off, "   "), "   ");
    }

    // ── paridade com o espelho Python ──────────────────────────────────────

    const PYTHON_POLICY: &str = include_str!("../../../client/hooks/injection_policy.py");

    /// Os nomes entre aspas de um bloco da política Python (`frozenset(...)`
    /// ou tupla `(...)`), a partir do cabeçalho declarado.
    fn python_block_names(header: &str) -> BTreeSet<String> {
        let start = PYTHON_POLICY
            .find(header)
            .unwrap_or_else(|| panic!("{header} ausente na política Python"));
        let rest = &PYTHON_POLICY[start + header.len()..];
        let end = rest.find(')').expect("bloco sem fechamento");
        let mut names = BTreeSet::new();
        let mut cur = String::new();
        let mut inside = false;
        for c in rest[..end].chars() {
            match (c, inside) {
                ('"', false) => {
                    inside = true;
                    cur.clear();
                }
                ('"', true) => {
                    inside = false;
                    names.insert(std::mem::take(&mut cur));
                }
                (_, true) => cur.push(c),
                (_, false) => {}
            }
        }
        names
    }

    fn rust_set(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|s| (*s).to_string()).collect()
    }

    /// As MESMAS listas de nomes nos dois lados — nem um nome a mais, nem um a
    /// menos. É o juiz em Rust do "predicado escrito duas vezes nunca se
    /// encontra": quando um lado ganhar uma família, este teste reprova até o
    /// outro ganhar também.
    #[test]
    fn the_rust_policy_mirrors_the_python_policy() {
        assert_eq!(
            rust_set(ADVISORY_FAMILIES),
            python_block_names("ADVISORY_FAMILIES = frozenset("),
            "ADVISORY_FAMILIES divergiu do espelho Python"
        );
        assert_eq!(
            rust_set(FACT_FAMILIES),
            python_block_names("FACT_FAMILIES = frozenset("),
            "FACT_FAMILIES divergiu do espelho Python"
        );
        assert_eq!(
            rust_set(DECISION_FIELDS),
            python_block_names("DECISION_FIELDS = ("),
            "DECISION_FIELDS divergiu do espelho Python"
        );
        assert_eq!(
            rust_set(CONTEXT_FIELDS),
            python_block_names("CONTEXT_FIELDS = ("),
            "CONTEXT_FIELDS divergiu do espelho Python"
        );
        // 27 nomes de família (11 advisory + 16 fact), disjuntos.
        assert_eq!(ADVISORY_FAMILIES.len() + FACT_FAMILIES.len(), 27);
        assert!(
            rust_set(ADVISORY_FAMILIES).is_disjoint(&rust_set(FACT_FAMILIES)),
            "uma familia nao pode ser sermao E fato"
        );
        // Contexto e decisão são canais separados TAMBÉM na tabela: a guarda
        // de `strip_holder` (decisão vence em caso de sobreposição) tem aqui a
        // prova de que a sobreposição não existe.
        assert!(
            rust_set(CONTEXT_FIELDS).is_disjoint(&rust_set(DECISION_FIELDS)),
            "um campo nao pode ser contexto E decisao"
        );
    }

    /// Toda família que a TAXONOMIA sabe nomear tem uma classificação na
    /// política — a guarda da §8.11 em Rust: um nome órfão aqui cairia no
    /// fail-closed de `allows` e o hook pararia de injetar sem nada dizer.
    /// (`pre-bash` e `prose-integrity` não têm marcador de texto — chegam pela
    /// marca do emissor — e por isso a tabela `EMITTERS` também é coberta.)
    #[test]
    fn every_taxonomy_family_has_a_policy_classification() {
        let known = |f: &&str| ADVISORY_FAMILIES.contains(f) || FACT_FAMILIES.contains(f);
        for (family, _) in injection_family::FAMILY_MARKERS {
            assert!(
                known(family),
                "{family} da taxonomia sem classe na política"
            );
        }
        for (family, _) in injection_family::FAMILY_PATTERNS {
            assert!(
                known(family),
                "{family} da taxonomia sem classe na política"
            );
        }
        for (_, family, _) in injection_family::EMITTERS {
            assert!(
                known(family),
                "{family} dos emissores sem classe na política"
            );
        }
    }

    /// Paridade de CLASSIFICAÇÃO: para os payloads fixados pela régua, a
    /// política nativa e o espelho Python tiram a mesma conclusão. A terceira
    /// coluna é o veredito sob `no-advisory` — decidido pelo CONJUNTO, não
    /// pela família primária: `⚡ last fail:` tem primária `post-tool-echo`
    /// (sermão) mas também casa o padrão de `failure-echo` (fato), e passa.
    /// Os vereditos foram confirmados executando o espelho Python em
    /// 26/10/2026.
    #[test]
    fn a_classificacao_da_politica_e_a_da_regua() {
        for (payload, family, no_advisory_allows) in [
            (
                "[TOURING SUGGEST · code-mode-loop]",
                "touring-suggest",
                false,
            ),
            ("[CODE MODE · rajada] 2a inspecao", "code-mode-deny", true),
            (
                "[CODE MODE — contrato desta sessão · apresentação `code`]",
                "code-mode-contract",
                true,
            ),
            ("MUST    touring doctor -j", "session-contract", true),
            (
                "`cat` exited 0, but its output shows errors after a pipe",
                "pipefail-evidence",
                true,
            ),
            ("[generic]", "generic-noise", false),
            (
                "Contexto em 63% — a autocompactação dispara aos 65%.",
                "context-monitor",
                true,
            ),
            (
                "[COMPACTAÇÃO · lacunas do resumo, medidas no transcript]",
                "compaction-gaps",
                true,
            ),
            (
                "[delegation-nudge] 6 leituras (Read/Grep/Glob) em 6 min",
                "delegation-nudge",
                false,
            ),
            (
                "⚡ last fail: python3 — sqlite3.OperationalError",
                "post-tool-echo",
                true,
            ),
            ("failed (exit 1): o comando quebrou", "failure-echo", true),
            ("[E2 contrato] o gate exige", "quality-gate", true),
            (
                "Similar command failed recently (similarity=57%)",
                "past-lessons",
                false,
            ),
            ("[EVENT_READER] hook trace", "debug-stdout", false),
            ("[SUGESTÃO DE SKILL] use X", "plugin-hook", false),
            ("HOTSPOT: a.rs blast 12", "pre-edit", true),
            (
                "Touring Knowledge: 5156 files known",
                "session-start",
                false,
            ),
            (
                "[PROMPT ENHANCEMENT -- DEBUG MODE]",
                "prompt-enhance",
                false,
            ),
        ] {
            assert_eq!(
                injection_family::injection_family(payload),
                family,
                "primária divergiu para {payload:?}"
            );
            assert!(
                injection_family::families(payload).contains(&family),
                "{family} ausente do conjunto para {payload:?}"
            );
            assert_eq!(
                allows_payload(payload, Some(InjectionLevel::NoAdvisory)),
                no_advisory_allows,
                "veredito no-advisory divergiu do espelho Python para {payload:?}"
            );
        }
    }

    // ── aplicação nos sítios de emissão (S-2) ──────────────────────────────

    const SERMON: &str =
        "[TOURING SUGGEST · code-mode-scan · conf=0.95]\n  MUST    touring run --lang bash";
    const FACT: &str = "HOTSPOT: a.rs blast 12\nMUST touring ast blast a.rs";

    /// Um envelope REAL de PreToolUse: a decisão mora DENTRO do
    /// `hookSpecificOutput`, ao lado do contexto que a política corta.
    fn real_deny(ctx: &str) -> String {
        json!({"hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "permissionDecisionReason": "G2: pipe sem pipefail",
            "updatedInput": {"command": "set -o pipefail; ls | wc -l"},
            "additionalContext": ctx,
        }})
        .to_string()
    }

    fn journal_lines(path: &Path) -> Vec<Value> {
        std::fs::read_to_string(path)
            .unwrap_or_default()
            .lines()
            .map(|l| serde_json::from_str(l).expect("cada linha do diário é JSON"))
            .collect()
    }

    /// O contrato C13 no helper compartilhado: sob `no-advisory`, o sermão sai
    /// e TODA a decisão (veredito, razão, entrada reescrita) fica como veio; o
    /// corte vai para o diário com a família e os bytes.
    #[test]
    fn test_no_advisory_drops_sermon_keeps_decision() {
        let dir = tempfile::tempdir().expect("tempdir");
        let sink = injection_journal::journal_path_in(dir.path());
        let input = real_deny(SERMON);
        let out = apply_at(
            InjectionLevel::NoAdvisory,
            SITE_DAEMON_FORWARD,
            &input,
            Some(&sink),
        );
        let v: Value = serde_json::from_str(&out).expect("a saída continua JSON");
        let hso = &v["hookSpecificOutput"];
        assert!(
            hso.get("additionalContext").is_none(),
            "o sermão sai: {out}"
        );
        assert_eq!(hso["permissionDecision"], json!("deny"));
        assert_eq!(
            hso["permissionDecisionReason"],
            json!("G2: pipe sem pipefail")
        );
        assert_eq!(
            hso["updatedInput"],
            json!({"command": "set -o pipefail; ls | wc -l"})
        );
        assert_eq!(hso["hookEventName"], json!("PreToolUse"));

        let lines = journal_lines(&sink);
        assert_eq!(lines.len(), 1, "um corte, uma linha");
        assert_eq!(lines[0]["site"], json!(SITE_DAEMON_FORWARD));
        assert_eq!(lines[0]["level"], json!("no-advisory"));
        assert_eq!(lines[0]["cuts"][0]["family"], json!("touring-suggest"));
        assert_eq!(lines[0]["cuts"][0]["bytes"], json!(SERMON.len()));
    }

    /// S-14: o resultado reescrito de um PostToolUse (`updatedToolOutput`) é o que o
    /// modelo passa a VER no lugar da saída da ferramenta — não é contexto, e nenhum
    /// nível o corta. O sermão que viaja ao lado sai; o resultado fica byte a byte.
    #[test]
    fn test_updated_tool_output_survives_every_level() {
        let result = json!({"stdout": "kept\n[touring result-compactor] ...", "stderr": "", "interrupted": false});
        let envelope = json!({"hookSpecificOutput": {
            "hookEventName": "PostToolUse",
            "updatedToolOutput": result,
            "additionalContext": SERMON,
        }})
        .to_string();
        for level in [InjectionLevel::NoAdvisory, InjectionLevel::Off] {
            let out = apply_at(level, SITE_DAEMON_FORWARD, &envelope, None);
            let v: Value = serde_json::from_str(&out).expect("a saída continua JSON");
            let hso = &v["hookSpecificOutput"];
            assert_eq!(hso["updatedToolOutput"], result, "{level:?}: {out}");
            assert!(hso.get("additionalContext").is_none(), "{level:?}: {out}");
            assert_eq!(hso["hookEventName"], json!("PostToolUse"));
        }
        // sem contexto algum a cortar: o envelope volta emprestado, intocado
        let only = json!({"hookSpecificOutput": {
            "hookEventName": "PostToolUse",
            "updatedToolOutput": result,
        }})
        .to_string();
        assert!(matches!(
            apply_at(InjectionLevel::Off, SITE_DAEMON_FORWARD, &only, None),
            Cow::Borrowed(_)
        ));
    }

    /// Sem env (`Full`) o texto volta EMPRESTADO — o mesmo ponteiro: nada é
    /// parseado, alocado nem gravado. É a identidade de bytes do default.
    #[test]
    fn test_full_is_borrowed_and_byte_identical() {
        let dir = tempfile::tempdir().expect("tempdir");
        let sink = injection_journal::journal_path_in(dir.path());
        let input = real_deny(SERMON);
        let out = apply_at(InjectionLevel::Full, SITE_EMIT_RUNTIME, &input, Some(&sink));
        assert!(matches!(out, Cow::Borrowed(s) if std::ptr::eq(s, input.as_str())));
        assert!(!sink.exists(), "Full não escreve diário");
    }

    /// Um envelope de FATO sob `no-advisory` não é alterado — nem
    /// re-serializado (a ordem das chaves e os bytes ficam os do emissor) — e
    /// não deixa linha no diário.
    #[test]
    fn test_no_advisory_leaves_a_fact_envelope_untouched() {
        let dir = tempfile::tempdir().expect("tempdir");
        let sink = injection_journal::journal_path_in(dir.path());
        let input = format!(
            r#"{{"hookSpecificOutput":{{"hookEventName":"PreToolUse","additionalContext":{}}}}}"#,
            json!(FACT)
        );
        let out = apply_at(
            InjectionLevel::NoAdvisory,
            SITE_EMIT_RUNTIME,
            &input,
            Some(&sink),
        );
        assert!(matches!(out, Cow::Borrowed(s) if std::ptr::eq(s, input.as_str())));
        assert!(!sink.exists(), "nada cortado, nada registrado");
    }

    /// `off` num envelope cujo ÚNICO conteúdo era contexto devolve o `{}`
    /// canônico — não um `hookSpecificOutput` com só o nome do evento.
    #[test]
    fn test_off_context_only_envelope_becomes_empty_object() {
        let input = json!({
            "hookSpecificOutput": {"hookEventName": "PostToolUse", "additionalContext": FACT},
            "systemMessage": "Contexto em 63% — a autocompactação dispara aos 65%.",
        })
        .to_string();
        let out = apply_at(InjectionLevel::Off, SITE_EMIT_RUNTIME, &input, None);
        assert_eq!(out, "{}");
    }

    /// Para QUALQUER campo de decisão, `off` devolve o envelope igual ao de
    /// entrada menos os dois campos de contexto — a prova de que a política
    /// não escreve em nenhum canal de enforcement.
    #[test]
    fn test_off_keeps_every_decision_field() {
        let before = json!({
            "continue": true,
            "stopReason": "nenhum",
            "suppressOutput": true,
            "decision": "block",
            "reason": "gate falhou",
            "systemMessage": "Contexto em 63%",
            "hookSpecificOutput": {
                "hookEventName": "PostToolUse",
                "permissionDecision": "deny",
                "permissionDecisionReason": "G2",
                "updatedInput": {"command": "ls"},
                "additionalContext": SERMON,
            },
        });
        let input = before.to_string();
        let out = apply_at(InjectionLevel::Off, SITE_EMIT_RUNTIME, &input, None);
        let mut after: Value = serde_json::from_str(&out).expect("continua JSON");
        // A saída, com o contexto devolvido, é a entrada: nada além do
        // contexto saiu, nada foi reescrito.
        after["systemMessage"] = before["systemMessage"].clone();
        after["hookSpecificOutput"]["additionalContext"] = json!(SERMON);
        assert_eq!(after, before);
        for field in DECISION_FIELDS {
            assert_eq!(
                after
                    .get(*field)
                    .or_else(|| after["hookSpecificOutput"].get(*field)),
                before
                    .get(*field)
                    .or_else(|| before["hookSpecificOutput"].get(*field)),
                "{field} mudou"
            );
        }
    }

    /// stdout cru: o log de depuração sai e é registrado com a família dele.
    #[test]
    fn test_raw_stdout_noise_is_cut_and_journaled() {
        let dir = tempfile::tempdir().expect("tempdir");
        let sink = injection_journal::journal_path_in(dir.path());
        let raw = "[EVENT_READER] 2026-10-27 hook trace";
        let out = apply_at(
            InjectionLevel::NoAdvisory,
            SITE_STANDALONE,
            raw,
            Some(&sink),
        );
        assert_eq!(out, "");
        let lines = journal_lines(&sink);
        assert_eq!(lines[0]["cuts"][0]["family"], json!("debug-stdout"));
        assert_eq!(lines[0]["cuts"][0]["bytes"], json!(raw.len()));
    }

    /// O diário é fail-open: um caminho impossível de gravar não muda a
    /// resposta do hook (o corte acontece igual).
    #[test]
    fn test_unwritable_journal_never_changes_the_answer() {
        let dir = tempfile::tempdir().expect("tempdir");
        let plain = dir.path().join("plain");
        std::fs::write(&plain, "x").expect("write");
        let sink = plain.join("nested").join("j.jsonl");
        let input = real_deny(SERMON);
        let out = apply_at(
            InjectionLevel::NoAdvisory,
            SITE_EMIT_RUNTIME,
            &input,
            Some(&sink),
        );
        assert!(
            !out.contains("MUST    touring run"),
            "o corte acontece: {out}"
        );
        assert!(out.contains("permissionDecision"));
    }

    /// `retain_allowed` — o sítio do veredito: aviso a aviso, o mesmo
    /// predicado de `allows_payload`; `Full` devolve tudo como veio.
    #[test]
    fn test_retain_allowed_filters_item_by_item_and_journals_the_cut() {
        let items = || vec![SERMON.to_string(), FACT.to_string()];
        let dir = tempfile::tempdir().expect("tempdir");
        let sink = injection_journal::journal_path_in(dir.path());

        assert_eq!(
            retain_allowed(InjectionLevel::Full, SITE_VERDICT, items(), Some(&sink)),
            items()
        );
        assert!(!sink.exists());

        let kept = retain_allowed(
            InjectionLevel::NoAdvisory,
            SITE_VERDICT,
            items(),
            Some(&sink),
        );
        assert_eq!(kept, vec![FACT.to_string()], "o sermão sai, o fato fica");
        let lines = journal_lines(&sink);
        assert_eq!(lines[0]["site"], json!(SITE_VERDICT));
        assert_eq!(lines[0]["cuts"][0]["family"], json!("touring-suggest"));

        assert!(retain_allowed(InjectionLevel::Off, SITE_VERDICT, items(), None).is_empty());
    }

    /// Orçamento de custo (+1 ms no p99 do hook): 10.000 aplicações no caso
    /// mais caro — `no-advisory` num envelope com sermão, parseando,
    /// cortando, re-serializando e gravando o diário — e o caso de TODOS os
    /// hooks por padrão, `Full`. Roda com `--ignored --nocapture` e imprime
    /// os números; o teste só falha se o caso padrão deixar de ser desprezível.
    #[test]
    #[ignore = "benchmark: cargo test -p touring-foundation bench_apply_cost -- --ignored --nocapture"]
    fn bench_apply_cost() {
        let dir = tempfile::tempdir().expect("tempdir");
        let sink = injection_journal::journal_path_in(dir.path());
        let input = real_deny(&SERMON.repeat(8));
        let n = 10_000u32;

        let t = std::time::Instant::now();
        for _ in 0..n {
            std::hint::black_box(apply_at(
                InjectionLevel::Full,
                SITE_EMIT_RUNTIME,
                std::hint::black_box(&input),
                Some(&sink),
            ));
        }
        let full = t.elapsed();

        let t = std::time::Instant::now();
        for _ in 0..n {
            std::hint::black_box(filter_output(
                InjectionLevel::NoAdvisory,
                std::hint::black_box(&input),
            ));
        }
        let filter = t.elapsed();

        let t = std::time::Instant::now();
        for _ in 0..n {
            std::hint::black_box(apply_at(
                InjectionLevel::NoAdvisory,
                SITE_EMIT_RUNTIME,
                std::hint::black_box(&input),
                Some(&sink),
            ));
        }
        let with_journal = t.elapsed();

        let per = |d: std::time::Duration| d.as_secs_f64() * 1e6 / f64::from(n);
        println!(
            "bytes/envelope={} | Full: {:.3} us/call | filter_output(NoAdvisory): {:.1} us/call | apply_at(NoAdvisory)+journal: {:.1} us/call",
            input.len(),
            per(full),
            per(filter),
            per(with_journal)
        );
        assert!(
            per(full) < 5.0,
            "o caso padrão não pode custar: {:.3} us",
            per(full)
        );
    }
}
