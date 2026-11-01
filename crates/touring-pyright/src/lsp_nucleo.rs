//! The LSP half of layer 2, shared by every language-server verifier (pyright, rust-analyzer, deno).
//!
//! One check is: open the overlays → open the proposed content → obtain the diagnostics of *that*
//! content → close → (when the disk content is known) obtain the diagnostics of the disk content the
//! same way and drop what was already there (the delta *proposed − disk*). What differs per server is
//! kept out of here: how diagnostics are *obtained* ([`Modo`], chosen from what the server advertises)
//! and how they become findings ([`Normalizador`]).
//!
//! ## Two ways to get the diagnostics of an open document
//!
//! * **Push** (`textDocument/publishDiagnostics`): the answer is the publication whose `(uri, version)`
//!   equals the `didOpen` we sent — a publication of an older version, or with a null version (the
//!   cleanup pyright emits ~50 ms after every `didClose`), is not an answer ([`LspClient::wait_publish`]).
//! * **Pull** (`textDocument/diagnostic`, LSP 3.17): the server answers a request, so there is nothing to
//!   match — but a server that is still loading answers an EMPTY report (rust-analyzer 1.98 does), which
//!   is indistinguishable from "no errors". Servers that report their readiness
//!   (`experimental/serverStatus.quiescent`, see [`Perfil::exige_quiescencia`]) are only believed once
//!   quiescent, and an answer that spans a drop back to non-quiescent is asked again.
//!
//! The mode is read from the `initialize` result (`diagnosticProvider` present → pull), not hard-coded
//! per server: pyright 1.1.408 advertises none and keeps pushing, exactly as before.

use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use serde_json::{Map, Value};

use crate::achado::{Balde, L2Achado, L2Resultado, Severidade, file_uri};
use crate::client::{ClientError, LspClient};
use crate::servidor::{Edicao, Verificado};

/// How the diagnostics of an open document are obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Modo {
    /// `publishDiagnostics`, matched by `(uri, version)`.
    Push,
    /// `textDocument/diagnostic`.
    Pull,
}

impl Modo {
    /// Pull when the server advertises a `diagnosticProvider` in its `initialize` result.
    pub(crate) fn do_resultado(initialize: &Value) -> Self {
        match initialize.pointer("/capabilities/diagnosticProvider") {
            Some(provider) if !provider.is_null() => Self::Pull,
            _ => Self::Push,
        }
    }
}

/// What differs between servers that the shared check needs to know.
pub(crate) struct Perfil {
    /// Server name in `unknown` reasons (`camada 2 (<nome>) indisponível: …`).
    pub(crate) nome: &'static str,
    /// Extension of the virtual file an inline program is opened as (`py`, `rs`, `ts`).
    pub(crate) extensao_inline: &'static str,
    /// Believe a pull only once the server reports `quiescent: true` (rust-analyzer).
    pub(crate) exige_quiescencia: bool,
    /// The LSP `languageId` of a document, from its path.
    pub(crate) language_id: fn(&str) -> &'static str,
}

/// What a check works on: the target, the proposed content, the optional disk baseline, overlays.
pub(crate) struct Pedido<'a> {
    pub(crate) uri: String,
    pub(crate) origem: PathBuf,
    pub(crate) balde: Balde,
    pub(crate) content: &'a str,
    pub(crate) disk: Option<&'a str>,
    pub(crate) overlays: &'a [(String, String)],
    pub(crate) deadline: Instant,
}

impl<'a> Pedido<'a> {
    /// The same target seen with other `content` (the disk baseline).
    pub(crate) fn com_conteudo(&self, content: &'a str) -> Pedido<'a> {
        Pedido {
            uri: self.uri.clone(),
            origem: self.origem.clone(),
            balde: self.balde,
            content,
            disk: None,
            overlays: self.overlays,
            deadline: self.deadline,
        }
    }
}

/// Turns a server's diagnostics into findings.
pub(crate) trait Normalizador: Sync {
    /// The findings attributable to `pedido.content`. `Err(reason)` = the diagnostics cannot be
    /// trusted for this file (e.g. rust-analyzer says it is not part of any crate): the check is
    /// `unknown` with that reason, never an implicit allow.
    fn achados(
        &self,
        diags: &[Value],
        pedido: &Pedido<'_>,
        raiz: &Path,
    ) -> Result<Vec<L2Achado>, String>;

    /// The `(rule, name)` pairs the disk content already denies — the "before" of the delta. By
    /// default, those of its findings.
    fn chaves(
        &self,
        diags: &[Value],
        pedido: &Pedido<'_>,
        raiz: &Path,
    ) -> HashSet<(String, String)> {
        self.achados(diags, pedido, raiz)
            .unwrap_or_default()
            .into_iter()
            .map(|a| (a.regra, a.nome))
            .collect()
    }
}

/// Why a check could not be measured.
#[derive(Debug)]
pub(crate) enum Falha {
    /// The transport or the server failed (deadline, dead server, protocol error).
    Cliente(ClientError),
    /// The server answered, but its answer cannot be trusted for this file.
    Normalizador(String),
}

impl From<ClientError> for Falha {
    fn from(e: ClientError) -> Self {
        Self::Cliente(e)
    }
}

/// A live language server driven through [`LspClient`].
pub(crate) struct NucleoLsp {
    client: LspClient,
    root: PathBuf,
    budget: Duration,
    modo: Modo,
    perfil: &'static Perfil,
}

/// The parameters of a server start.
pub(crate) struct Inicio<'a> {
    pub(crate) comando: Command,
    pub(crate) sections: Map<String, Value>,
    /// The directory the server is rooted at (`rootUri`); not always the project root.
    pub(crate) raiz_lsp: &'a Path,
    pub(crate) capabilities: Value,
    pub(crate) init_options: Value,
    pub(crate) startup: Duration,
    /// Which diagnostics mode to use (see [`EscolhaDeModo`]).
    pub(crate) modo: EscolhaDeModo,
}

/// How the diagnostics mode of a session is decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EscolhaDeModo {
    /// Whatever the `initialize` result advertises.
    Anunciado,
    /// This mode, whatever the server advertises (pyright stays on push whatever a newer version
    /// advertises: its measured behaviour is the contract).
    Forcado(Modo),
    /// This mode or no session at all: a server that does not advertise it cannot be trusted with
    /// the other (rust-analyzer's push publishes syntax errors first and the rest later).
    Exigido(Modo),
}

impl NucleoLsp {
    /// Wraps an already-initialized client (the seam tests use to inject a scripted server).
    pub(crate) fn de_cliente(
        client: LspClient,
        root: PathBuf,
        budget: Duration,
        modo: Modo,
        perfil: &'static Perfil,
    ) -> Self {
        Self {
            client,
            root,
            budget,
            modo,
            perfil,
        }
    }

    /// Spawns the server, completes `initialize` and picks the diagnostics mode from its answer.
    pub(crate) fn iniciar(
        inicio: Inicio<'_>,
        root: PathBuf,
        budget: Duration,
        perfil: &'static Perfil,
    ) -> Result<Self, ClientError> {
        let Inicio {
            comando,
            sections,
            raiz_lsp,
            capabilities,
            init_options,
            startup,
            modo,
        } = inicio;
        let mut client = LspClient::spawn(comando, sections)?;
        let name = raiz_lsp
            .file_name()
            .map_or_else(|| "project".into(), |n| n.to_string_lossy().into_owned());
        let initialize = client.initialize_with(
            &file_uri(raiz_lsp),
            &name,
            capabilities,
            init_options,
            Instant::now() + startup,
        )?;
        let anunciado = Modo::do_resultado(&initialize);
        let modo = match modo {
            EscolhaDeModo::Anunciado => anunciado,
            EscolhaDeModo::Forcado(modo) => modo,
            EscolhaDeModo::Exigido(modo) if modo == anunciado => modo,
            EscolhaDeModo::Exigido(modo) => {
                return Err(ClientError::Protocol(format!(
                    "the server does not advertise {modo:?} diagnostics (it advertised {anunciado:?})"
                )));
            }
        };
        Ok(Self::de_cliente(client, root, budget, modo, perfil))
    }

    /// The project root paths are resolved against.
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    /// Replaces the per-check budget.
    pub(crate) fn com_budget(mut self, budget: Duration) -> Self {
        self.budget = budget;
        self
    }

    /// Waits for the workspace to finish loading (servers that report it); `Ok` right away otherwise.
    pub(crate) fn esperar_pronto(&mut self, deadline: Instant) -> Result<(), ClientError> {
        if self.perfil.exige_quiescencia {
            self.client.wait_quiescent(deadline)?;
        }
        Ok(())
    }

    /// Checks one edit with `norm`. The whole call — overlays, the check and the baseline — shares
    /// one budget; exceeding it, or a dead server, is [`crate::achado::Verdict::Unknown`] with the
    /// reason.
    pub(crate) fn verificar(&mut self, norm: &dyn Normalizador, e: &Edicao<'_>) -> Verificado {
        self.verificar_ate(norm, e, self.budget)
    }

    /// [`NucleoLsp::verificar`] with an explicit budget (warm-up checks get a long one).
    pub(crate) fn verificar_ate(
        &mut self,
        norm: &dyn Normalizador,
        e: &Edicao<'_>,
        budget: Duration,
    ) -> Verificado {
        let started = Instant::now();
        let deadline = started + budget;
        let origem = if e.path.starts_with('<') {
            inline_path_ext(&self.root, e.path, e.content, self.perfil.extensao_inline)
        } else {
            self.root.join(e.path)
        };
        let pedido = Pedido {
            uri: file_uri(&origem),
            origem,
            balde: Balde::from_path(e.path, &self.root),
            content: e.content,
            disk: e.disk,
            overlays: e.overlays,
            deadline,
        };
        let mut opened = Vec::new();
        let outcome = self.run_check(norm, &pedido, &mut opened);
        for uri in &opened {
            let _ = self.client.close(uri);
        }
        let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        match outcome {
            Ok(achados) => Verificado {
                resultado: L2Resultado::from_achados(achados, elapsed_ms),
                saudavel: true,
            },
            Err(falha) => Verificado {
                saudavel: matches!(falha, Falha::Normalizador(_)),
                resultado: L2Resultado::unknown(self.motivo(&falha, budget), elapsed_ms),
            },
        }
    }

    fn run_check(
        &mut self,
        norm: &dyn Normalizador,
        pedido: &Pedido<'_>,
        opened: &mut Vec<String>,
    ) -> Result<Vec<L2Achado>, Falha> {
        self.open_overlays(pedido.overlays, opened)?;
        let diags = self.diagnose(&pedido.uri, pedido.content, pedido.deadline)?;
        let mut achados = norm
            .achados(&diags, pedido, &self.root)
            .map_err(Falha::Normalizador)?;
        self.rebaixar_se_degradado(&mut achados);
        if let Some(on_disk) = pedido.disk {
            let ja = self.pre_existing(norm, pedido, on_disk)?;
            achados.retain(|a| !ja.contains(&(a.regra.clone(), a.nome.clone())));
        }
        Ok(achados)
    }

    fn open_overlays(
        &mut self,
        overlays: &[(String, String)],
        opened: &mut Vec<String>,
    ) -> Result<(), ClientError> {
        for (path, text) in overlays {
            let uri = file_uri(&self.root.join(path));
            self.client
                .open(&uri, (self.perfil.language_id)(path), text)?;
            opened.push(uri);
        }
        Ok(())
    }

    /// `(rule, name)` pairs the file already denies on disk — the "before" of the delta.
    fn pre_existing(
        &mut self,
        norm: &dyn Normalizador,
        pedido: &Pedido<'_>,
        on_disk: &str,
    ) -> Result<HashSet<(String, String)>, Falha> {
        let before = self.diagnose(&pedido.uri, on_disk, pedido.deadline)?;
        Ok(norm.chaves(&before, &pedido.com_conteudo(on_disk), &self.root))
    }

    /// One `didOpen → diagnostics of this content → didClose` round-trip. The document is closed even
    /// when obtaining the diagnostics fails, so a timeout never leaves a stale open document behind.
    fn diagnose(
        &mut self,
        uri: &str,
        text: &str,
        deadline: Instant,
    ) -> Result<Vec<Value>, ClientError> {
        let language_id = (self.perfil.language_id)(uri);
        let version = self.client.open(uri, language_id, text)?;
        let obtained = match self.modo {
            Modo::Push => self.client.wait_publish(uri, version, deadline),
            Modo::Pull => self.pull_trustworthy(uri, deadline),
        };
        let _ = self.client.close(uri);
        obtained
    }

    /// A pull answer that is believed: the server was quiescent (when it reports that) before the
    /// request and did not go busy again while answering.
    fn pull_trustworthy(
        &mut self,
        uri: &str,
        deadline: Instant,
    ) -> Result<Vec<Value>, ClientError> {
        if !self.perfil.exige_quiescencia {
            return self.client.pull_diagnostics(uri, deadline);
        }
        loop {
            self.client.wait_quiescent(deadline)?;
            self.exigir_saude()?;
            let busy_reports = self.client.quiescent_false_count();
            let items = self.client.pull_diagnostics(uri, deadline)?;
            self.client.drain()?;
            let still_quiet = self.client.quiescent() == Some(true);
            if still_quiet && self.client.quiescent_false_count() == busy_reports {
                return Ok(items);
            }
        }
    }

    /// A server that loaded its workspace only in part reports `health: warning` (rust-analyzer: "Failed
    /// to read Cargo metadata with dependencies" — the dependencies are then unresolved, so every
    /// import from a declared crate is "unknown" for a reason that is not the edit's). What it says is
    /// still worth showing, but not worth blocking: a finding may never deny while the server is degraded.
    fn rebaixar_se_degradado(&self, achados: &mut [L2Achado]) {
        let Some(("warning", mensagem)) = self.client.saude() else {
            return;
        };
        let nota = format!(
            " [{} está degradado: {}]",
            self.perfil.nome,
            mensagem.chars().take(120).collect::<String>()
        );
        for achado in achados
            .iter_mut()
            .filter(|a| a.severidade == Severidade::Deny)
        {
            achado.severidade = Severidade::Warn;
            achado.mensagem.push_str(&nota);
        }
    }

    /// A workspace that failed to load is quiescent too — and answers every file with an empty
    /// report. The server's own `health: error` is what tells that apart from a clean project.
    fn exigir_saude(&self) -> Result<(), ClientError> {
        match self.client.saude() {
            Some(("error", mensagem)) => Err(ClientError::Protocol(format!(
                "{} reportou saúde `error`: {}",
                self.perfil.nome,
                if mensagem.is_empty() {
                    "sem mensagem"
                } else {
                    mensagem
                }
            ))),
            _ => Ok(()),
        }
    }

    fn motivo(&self, falha: &Falha, budget: Duration) -> String {
        match falha {
            Falha::Normalizador(motivo) => motivo.clone(),
            Falha::Cliente(ClientError::Timeout(what)) => {
                format!(
                    "prazo de {} ms estourado esperando {what}",
                    budget.as_millis()
                )
            }
            Falha::Cliente(other) => {
                format!("camada 2 ({}) indisponível: {other}", self.perfil.nome)
            }
        }
    }
}

/// The virtual file an inline program (`<string>`, `<stdin>`) is opened as:
/// `<root>/.vgp_inline_<hash>.<ext>`. It is **never written to disk** — it exists only as a `didOpen`
/// document, so relative imports and the project's import roots resolve as they would for a script
/// placed there. The hash keys it by `(path, content)`, stable within a process (all it needs to be).
pub(crate) fn inline_path_ext(root: &Path, path: &str, content: &str, ext: &str) -> PathBuf {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    content.hash(&mut hasher);
    root.join(format!(".vgp_inline_{:016x}.{ext}", hasher.finish()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::achado::{Escopo, Severidade, Verdict};
    use crate::testutil::{Arvore, FAKE_PULL};
    use serde_json::json;

    /// pull + quiescence, like rust-analyzer.
    static PERFIL_RA: Perfil = Perfil {
        nome: "fake-ra",
        extensao_inline: "rs",
        exige_quiescencia: true,
        language_id: |_| "rust",
    };

    /// pull without readiness reporting: the control that shows what the gate prevents.
    static PERFIL_SEM_PORTAO: Perfil = Perfil {
        nome: "fake-sem-portao",
        extensao_inline: "rs",
        exige_quiescencia: false,
        language_id: |_| "rust",
    };

    /// Reads the fake's `x_nome` as the finding name; every diagnostic is a project-file `nome`.
    struct NormTeste;

    impl Normalizador for NormTeste {
        fn achados(
            &self,
            diags: &[Value],
            pedido: &Pedido<'_>,
            _raiz: &Path,
        ) -> Result<Vec<L2Achado>, String> {
            Ok(diags
                .iter()
                .filter_map(|d| {
                    let nome = d.get("x_nome")?.as_str()?.to_string();
                    let linha = d.pointer("/range/start/line")?.as_u64()? + 1;
                    Some(L2Achado {
                        classe: "nome".into(),
                        nome,
                        linha: u32::try_from(linha).ok()?,
                        escopo: Escopo::Proprio,
                        severidade: if pedido.balde == Balde::Arquivo {
                            Severidade::Warn
                        } else {
                            Severidade::Deny
                        },
                        regra: d.get("code")?.as_str()?.to_string(),
                        mensagem: d.get("message")?.as_str()?.to_string(),
                    })
                })
                .collect())
        }
    }

    /// Always refuses: models rust-analyzer's `unlinked-file`.
    struct NormRecusa;

    impl Normalizador for NormRecusa {
        fn achados(&self, _: &[Value], _: &Pedido<'_>, _: &Path) -> Result<Vec<L2Achado>, String> {
            Err("o servidor não considera este arquivo parte do projeto".into())
        }
    }

    fn sessao(mode: &str, perfil: &'static Perfil, budget_ms: u64) -> (Arvore, NucleoLsp) {
        let tree = Arvore::nova("nucleo");
        let mut cmd = Command::new("python3");
        cmd.arg("-c").arg(FAKE_PULL).arg(mode);
        let mut client = LspClient::spawn(cmd, Map::new())
            .expect("python3 must be on PATH to run the fake server");
        let init = client
            .initialize(
                &file_uri(&tree.raiz),
                "p",
                json!({}),
                Instant::now() + Duration::from_secs(10),
            )
            .expect("fake initialize");
        let modo = Modo::do_resultado(&init);
        let nucleo = NucleoLsp::de_cliente(
            client,
            tree.raiz.clone(),
            Duration::from_millis(budget_ms),
            modo,
            perfil,
        );
        (tree, nucleo)
    }

    fn edicao<'a>(path: &'a str, content: &'a str, disk: Option<&'a str>) -> Edicao<'a> {
        Edicao {
            path,
            content,
            disk,
            overlays: &[],
        }
    }

    fn nomes(r: &L2Resultado) -> Vec<&str> {
        r.achados.iter().map(|a| a.nome.as_str()).collect()
    }

    /// The result of a check (the tests below do not care about the session-health flag).
    fn rodar(s: &mut NucleoLsp, norm: &dyn Normalizador, e: &Edicao<'_>) -> L2Resultado {
        s.verificar(norm, e).resultado
    }

    // ── mode from the initialize result ────────────────────────────────────

    #[test]
    fn the_mode_follows_what_the_server_advertises() {
        let pull = json!({"capabilities": {"diagnosticProvider": {"identifier": "x"}}});
        let nulo = json!({"capabilities": {"diagnosticProvider": null}});
        let ausente = json!({"capabilities": {}});
        assert_eq!(Modo::do_resultado(&pull), Modo::Pull);
        assert_eq!(
            Modo::do_resultado(&nulo),
            Modo::Push,
            "pyright 1.1.408 advertises null"
        );
        assert_eq!(Modo::do_resultado(&ausente), Modo::Push);
        assert_eq!(Modo::do_resultado(&json!({})), Modo::Push);
    }

    // ── choosing the mode at start-up ──────────────────────────────────────

    fn iniciar_falso(modo: EscolhaDeModo) -> Result<NucleoLsp, ClientError> {
        let tree = Arvore::nova("iniciar");
        let mut comando = Command::new("python3");
        comando.arg("-c").arg(FAKE_PULL).arg("ok");
        let inicio = Inicio {
            comando,
            sections: Map::new(),
            raiz_lsp: &tree.raiz,
            capabilities: json!({}),
            init_options: Value::Null,
            startup: Duration::from_secs(10),
            modo,
        };
        NucleoLsp::iniciar(
            inicio,
            tree.raiz.clone(),
            Duration::from_secs(1),
            &PERFIL_RA,
        )
    }

    #[test]
    fn the_mode_choice_follows_the_server_unless_forced_and_refuses_unless_required() {
        // the fake advertises pull
        assert_eq!(
            iniciar_falso(EscolhaDeModo::Anunciado).unwrap().modo,
            Modo::Pull
        );
        assert_eq!(
            iniciar_falso(EscolhaDeModo::Forcado(Modo::Push))
                .unwrap()
                .modo,
            Modo::Push,
            "pyright's push is forced whatever a newer version advertises"
        );
        assert_eq!(
            iniciar_falso(EscolhaDeModo::Exigido(Modo::Pull))
                .unwrap()
                .modo,
            Modo::Pull
        );
        let err = iniciar_falso(EscolhaDeModo::Exigido(Modo::Push))
            .err()
            .expect("must refuse");
        assert!(
            matches!(&err, ClientError::Protocol(m) if m.contains("does not advertise")),
            "{err:?}"
        );
    }

    // ── pull + quiescence ──────────────────────────────────────────────────

    #[test]
    fn pull_check_waits_for_quiescence_and_is_not_fooled_by_the_early_empty_report() {
        let (_t, mut s) = sessao("ok", &PERFIL_RA, 5000);
        let r = rodar(
            &mut s,
            &NormTeste,
            &edicao("src/a.rs", "# undef fantasma\n", None),
        );
        assert_eq!(r.verdict, Verdict::Warn, "{r:?}");
        assert_eq!(nomes(&r), ["fantasma"]);
        assert_eq!(r.achados[0].linha, 1);
    }

    #[test]
    fn without_the_gate_the_same_pull_reads_the_empty_trap_as_allow() {
        // The control of the test above: it proves the fixture reproduces the trap, so the gate is
        // what makes the previous test pass (a mutant that drops the gate flips it to this).
        let (_t, mut s) = sessao("ok", &PERFIL_SEM_PORTAO, 5000);
        let r = rodar(
            &mut s,
            &NormTeste,
            &edicao("src/a.rs", "# undef fantasma\n", None),
        );
        assert_eq!(
            r.verdict,
            Verdict::Allow,
            "the early empty report was believed: {r:?}"
        );
    }

    #[test]
    fn a_pull_that_spans_a_busy_report_is_asked_again() {
        let (_t, mut s) = sessao("oscila", &PERFIL_RA, 5000);
        // first check lands inside the false/true oscillation of the fake
        std::thread::sleep(Duration::from_millis(350));
        let r = rodar(
            &mut s,
            &NormTeste,
            &edicao("src/a.rs", "# undef fantasma\n", None),
        );
        assert_eq!(nomes(&r), ["fantasma"], "{r:?}");
    }

    #[test]
    fn server_cancelled_answers_are_retried_inside_a_check() {
        let (_t, mut s) = sessao("cancela", &PERFIL_RA, 5000);
        let r = rodar(
            &mut s,
            &NormTeste,
            &edicao("src/a.rs", "# undef fantasma\n", None),
        );
        assert_eq!(nomes(&r), ["fantasma"], "{r:?}");
    }

    #[test]
    fn a_server_that_never_gets_quiet_is_unknown_with_the_deadline_never_allow() {
        let (_t, mut s) = sessao("nunca", &PERFIL_RA, 500);
        let t0 = Instant::now();
        let r = rodar(
            &mut s,
            &NormTeste,
            &edicao("src/a.rs", "# undef fantasma\n", None),
        );
        assert_eq!(r.verdict, Verdict::Unknown, "{r:?}");
        assert!(
            r.motivo.as_deref().is_some_and(|m| m.contains("prazo")),
            "{r:?}"
        );
        assert!(
            t0.elapsed() < Duration::from_millis(2500),
            "{:?}",
            t0.elapsed()
        );
    }

    #[test]
    fn a_workspace_that_failed_to_load_is_unknown_with_the_servers_message_never_allow() {
        // quiet AND empty: indistinguishable from a clean project except by `health: error`
        let (_t, mut s) = sessao("saude_erro", &PERFIL_RA, 5000);
        let r = rodar(
            &mut s,
            &NormTeste,
            &edicao("src/a.rs", "# undef fantasma\n", None),
        );
        assert_eq!(r.verdict, Verdict::Unknown, "{r:?}");
        let motivo = r.motivo.expect("reason");
        assert!(
            motivo.contains("saúde `error`") && motivo.contains("falha ao carregar o workspace"),
            "{motivo}"
        );
    }

    #[test]
    fn a_degraded_workspace_never_denies_but_still_reports_with_the_reason() {
        // health `warning`: the dependencies did not load, so an import from a declared crate is
        // unresolved for a reason that is not the edit's. Inline code would deny; a degraded one cannot.
        let (_t, mut s) = sessao("saude_aviso", &PERFIL_RA, 5000);
        let r = rodar(
            &mut s,
            &NormTeste,
            &edicao("<string>", "# undef fantasma\n", None),
        );
        assert_eq!(r.verdict, Verdict::Warn, "{r:?}");
        assert_eq!(r.achados.len(), 1);
        assert_eq!(r.achados[0].severidade, Severidade::Warn);
        assert!(
            r.achados[0].mensagem.contains("degradado")
                && r.achados[0].mensagem.contains("Cargo metadata"),
            "{:?}",
            r.achados[0].mensagem
        );
        // control: the same edit on a healthy server denies
        let (_t2, mut ok) = sessao("ok", &PERFIL_RA, 5000);
        let sadio = rodar(
            &mut ok,
            &NormTeste,
            &edicao("<string>", "# undef fantasma\n", None),
        );
        assert_eq!(sadio.verdict, Verdict::Deny, "{sadio:?}");
    }

    #[test]
    fn a_dead_server_is_unknown_with_its_name_never_allow() {
        let (_t, mut s) = sessao("morre", &PERFIL_RA, 5000);
        let r = rodar(&mut s, &NormTeste, &edicao("src/a.rs", "# undef x\n", None));
        assert_eq!(r.verdict, Verdict::Unknown, "{r:?}");
        let motivo = r.motivo.expect("unknown carries its reason");
        assert!(
            motivo.contains("fake-ra") && motivo.contains("indisponível"),
            "{motivo}"
        );
    }

    #[test]
    fn the_delta_against_disk_drops_what_the_disk_already_had() {
        let (_t, mut s) = sessao("ok", &PERFIL_RA, 8000);
        let disk = "# undef velho\n";
        let proposto = "# undef velho\n# undef novo\n";
        let r = rodar(
            &mut s,
            &NormTeste,
            &edicao("src/a.rs", proposto, Some(disk)),
        );
        assert_eq!(nomes(&r), ["novo"], "{r:?}");
        let sem_delta = rodar(&mut s, &NormTeste, &edicao("src/a.rs", proposto, None));
        assert_eq!(nomes(&sem_delta), ["velho", "novo"], "{sem_delta:?}");
    }

    #[test]
    fn session_health_follows_the_kind_of_failure_not_the_verdict() {
        // about the FILE (the normalizer refuses it): the warm session is kept
        let (_t, mut s) = sessao("ok", &PERFIL_RA, 5000);
        let recusa = s.verificar(&NormRecusa, &edicao("src/a.rs", "x\n", None));
        assert_eq!(recusa.resultado.verdict, Verdict::Unknown);
        assert!(
            recusa.saudavel,
            "a refusal about the file must not condemn the session"
        );
        let boa = s.verificar(&NormTeste, &edicao("src/a.rs", "# undef x\n", None));
        assert!(boa.saudavel);
        // about the SESSION (deadline, dead server): condemned
        let (_t2, mut surdo) = sessao("nunca", &PERFIL_RA, 300);
        let prazo = surdo.verificar(&NormTeste, &edicao("src/a.rs", "x\n", None));
        assert!(!prazo.saudavel, "a deadline overrun condemns the session");
        let (_t3, mut morto) = sessao("morre", &PERFIL_RA, 5000);
        let morte = morto.verificar(&NormTeste, &edicao("src/a.rs", "x\n", None));
        assert!(!morte.saudavel, "a dead server condemns the session");
    }

    #[test]
    fn a_normalizer_refusal_is_unknown_with_its_own_reason() {
        let (_t, mut s) = sessao("ok", &PERFIL_RA, 5000);
        let r = rodar(&mut s, &NormRecusa, &edicao("src/a.rs", "x\n", None));
        assert_eq!(r.verdict, Verdict::Unknown, "{r:?}");
        assert_eq!(
            r.motivo.as_deref(),
            Some("o servidor não considera este arquivo parte do projeto")
        );
    }

    #[test]
    fn inline_programs_are_opened_as_a_virtual_file_with_the_servers_extension() {
        let (t, mut s) = sessao("ok", &PERFIL_RA, 5000);
        let r = rodar(
            &mut s,
            &NormTeste,
            &edicao("<string>", "# undef fantasma\n", None),
        );
        assert_eq!(r.achados.len(), 1, "{r:?}");
        assert_eq!(
            r.achados[0].severidade,
            Severidade::Deny,
            "inline runs now: {r:?}"
        );
        let virtual_path = inline_path_ext(&t.raiz, "<string>", "# undef fantasma\n", "rs");
        assert!(
            !virtual_path.exists(),
            "the virtual file must never be written"
        );
        assert!(
            virtual_path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with(".vgp_inline_") && n.ends_with(".rs")),
            "{virtual_path:?}"
        );
    }

    #[test]
    fn inline_path_is_keyed_by_path_and_content_and_by_extension() {
        let raiz = Path::new("/p");
        let a = inline_path_ext(raiz, "<string>", "x", "py");
        assert_eq!(a, inline_path_ext(raiz, "<string>", "x", "py"));
        assert_ne!(a, inline_path_ext(raiz, "<string>", "y", "py"));
        assert_ne!(a, inline_path_ext(raiz, "<stdin>", "x", "py"));
        assert_eq!(
            inline_path_ext(raiz, "<string>", "x", "ts")
                .extension()
                .and_then(|e| e.to_str()),
            Some("ts")
        );
    }
}
