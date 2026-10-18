//! VGP layer 2 for TypeScript/JavaScript — the `tsserver` of the project's own `node_modules/typescript`.
//!
//! **Which binary.** `node_modules/typescript/lib/tsserver.js`, searched from the directory of the edited
//! file up to the project root (the nearest install wins, so a monorepo package's own TypeScript checks
//! that package). A `tsserver` on `PATH` is never a substitute — the version in the project is the one its
//! `tsconfig.json` was written for — and nothing is installed: the server is started with
//! `--disableAutomaticTypingAcquisition`, the flag that stops it from running `npm install @types/…`.
//!
//! **Protocol.** tsserver is not LSP: requests are one JSON object per line on stdin, replies are
//! `Content-Length`-framed on stdout, interleaved with events (`projectLoadingStart`, `configFileDiag`, …).
//! A reply carries the `request_seq` of its request, and that is the only thing a reply is matched by —
//! the same lesson as the version match of the LSP client: the first message that looks like an answer
//! is not the answer. The diagnostics are asked with the synchronous commands (`syntacticDiagnosticsSync`,
//! `semanticDiagnosticsSync`) rather than `geterr`, whose answers arrive later as `syntaxDiag`/
//! `semanticDiag` events that would need a debounce to know they are complete.
//!
//! **What is refused instead of guessed.** A file outside every `tsconfig.json` is checked by tsserver in an
//! *inferred project* with default options and no ambient types (`Cannot find name 'process'`): noise, so
//! `unknown`. A file with syntax errors is `unknown` (the engine's rule: parse failed → unknown). A
//! JavaScript file is only type-checked under `// @ts-check` or `checkJs`; without either an empty report
//! means nothing, so `unknown`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::achado::{Balde, L2Achado, L2Resultado, file_uri};
use crate::client::{ClientError, Incoming, reader_loop, reap_child};
use crate::lsp_nucleo::{Falha, Normalizador, Pedido, inline_path_ext};
use crate::servidor::{Alvo, Ambiente, Edicao, Sessao, Verificado, dirs_ate_a_raiz, tem_extensao};
use crate::ts_diag::NormTs;

/// Per-check budget of a warm session.
const DEFAULT_BUDGET: Duration = Duration::from_millis(2000);
const NOME: &str = "tsserver";
const INFERRED_PROJECT: &str = "/dev/null/inferredProject";

// ── discovery ──────────────────────────────────────────────────────────────

/// A `tsserver` found for a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Localizado {
    /// The `node` that runs it.
    pub(crate) node: PathBuf,
    /// `…/node_modules/typescript/lib/tsserver.js`.
    pub(crate) entrada: PathBuf,
    /// The directory whose `node_modules` holds it (the session's working directory).
    pub(crate) pacote: PathBuf,
}

/// The nearest directory between the file and the project root that has `node_modules/typescript`.
fn pacote_com_typescript(raiz: &Path, arquivo: &str) -> Option<PathBuf> {
    dirs_ate_a_raiz(raiz, arquivo).into_iter().find(|dir| {
        dir.join("node_modules/typescript/lib/tsserver.js")
            .is_file()
    })
}

/// The registry sub-key of an edit: the package that holds the TypeScript used (empty when none).
pub(crate) fn subchave(raiz: &Path, arquivo: &str) -> PathBuf {
    pacote_com_typescript(raiz, arquivo).unwrap_or_default()
}

/// Finds the project's `tsserver` — or says why there is none.
pub(crate) fn localizar(raiz: &Path, arquivo: &str, amb: &Ambiente) -> Result<Localizado, String> {
    let pacote = pacote_com_typescript(raiz, arquivo).ok_or_else(|| {
        format!(
            "nenhum node_modules/typescript/lib/tsserver.js entre {arquivo} e a raiz {}: o TypeScript do projeto não está instalado e o Touring nunca o instala",
            raiz.display()
        )
    })?;
    let node = amb.achar("node").ok_or_else(|| {
        "node não está no PATH: o tsserver do projeto não pode ser executado".to_string()
    })?;
    Ok(Localizado {
        entrada: pacote.join("node_modules/typescript/lib/tsserver.js"),
        node,
        pacote,
    })
}

// ── the tsserver transport ─────────────────────────────────────────────────

/// A synchronous tsserver client: newline-delimited JSON out, framed JSON in (read by a thread so
/// every wait has a deadline).
pub(crate) struct TsClient {
    child: Child,
    stdin: Option<ChildStdin>,
    inbox: Receiver<Incoming>,
    seq: u64,
    finished: bool,
}

impl TsClient {
    pub(crate) fn spawn(mut command: Command) -> Result<Self, ClientError> {
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = command.spawn().map_err(ClientError::Spawn)?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let (tx, inbox) = mpsc::channel();
        let client = Self {
            child,
            stdin,
            inbox,
            seq: 0,
            finished: false,
        };
        let stdout =
            stdout.ok_or_else(|| ClientError::Protocol("server stdout was not piped".into()))?;
        thread::Builder::new()
            .name("tsserver-reader".into())
            .spawn(move || reader_loop(stdout, tx))
            .map_err(ClientError::Spawn)?;
        Ok(client)
    }

    #[cfg(test)]
    pub(crate) fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Sends a request and returns its `seq`.
    fn enviar(&mut self, comando: &str, argumentos: Value) -> Result<u64, ClientError> {
        self.seq += 1;
        let seq = self.seq;
        let mut linha = serde_json::to_vec(&json!({
            "seq": seq, "type": "request", "command": comando, "arguments": argumentos,
        }))?;
        linha.push(b'\n');
        let stdin = self.stdin.as_mut().ok_or(ClientError::Closed)?;
        stdin.write_all(&linha).map_err(ClientError::from)?;
        stdin.flush().map_err(ClientError::from)?;
        Ok(seq)
    }

    /// Waits for the reply whose `request_seq` is `seq`. Events and the replies of other requests
    /// (`open` and `close` are answered too, and may arrive late) are skipped.
    fn esperar(
        &mut self,
        seq: u64,
        comando: &str,
        deadline: Instant,
    ) -> Result<Value, ClientError> {
        loop {
            let restante = deadline.saturating_duration_since(Instant::now());
            let mensagem = match self.inbox.recv_timeout(restante) {
                Ok(Incoming::Message(m)) => m,
                Ok(Incoming::Eof) | Err(RecvTimeoutError::Disconnected) => {
                    return Err(ClientError::Closed);
                }
                Ok(Incoming::Failure(why)) => return Err(ClientError::Protocol(why)),
                Err(RecvTimeoutError::Timeout) => {
                    return Err(ClientError::Timeout(format!("the reply to `{comando}`")));
                }
            };
            if mensagem.get("type").and_then(Value::as_str) == Some("response")
                && mensagem.get("request_seq").and_then(Value::as_u64) == Some(seq)
            {
                return resposta_ou_erro(&mensagem, comando);
            }
        }
    }

    /// Sends a request and waits for its reply, returning the `body`.
    fn pedir(
        &mut self,
        comando: &str,
        argumentos: Value,
        deadline: Instant,
    ) -> Result<Value, ClientError> {
        let seq = self.enviar(comando, argumentos)?;
        self.esperar(seq, comando, deadline)
    }

    /// Polite shutdown (`exit`), then a bounded wait, then `kill`. Idempotent; also run by `Drop`.
    fn shutdown(&mut self) {
        if self.finished {
            return;
        }
        self.finished = true;
        let _ = self.enviar("exit", Value::Null);
        self.stdin = None;
        reap_child(&mut self.child);
    }
}

impl Drop for TsClient {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// The `body` of a successful reply; a `success: false` reply is a server error with its message.
fn resposta_ou_erro(mensagem: &Value, comando: &str) -> Result<Value, ClientError> {
    if mensagem.get("success").and_then(Value::as_bool) == Some(false) {
        return Err(ClientError::ServerError {
            method: comando.to_string(),
            message: mensagem
                .get("message")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            code: None,
        });
    }
    Ok(mensagem.get("body").cloned().unwrap_or(Value::Null))
}

// ── tsserver diagnostics as LSP-shaped values ──────────────────────────────

/// A tsserver diagnostic (`{start:{line,offset}, end, text, code, category}`, 1-based) as the
/// `{code, message, range}` shape the shared TypeScript table reads (0-based).
fn como_lsp(d: &Value) -> Value {
    let pos = |ponto: &str| {
        let linha = d.pointer(&format!("/{ponto}/line")).and_then(Value::as_u64);
        let coluna = d
            .pointer(&format!("/{ponto}/offset"))
            .and_then(Value::as_u64);
        json!({
            "line": linha.map(|l| l.saturating_sub(1)),
            "character": coluna.map(|c| c.saturating_sub(1)),
        })
    };
    json!({
        "code": d.get("code"),
        "message": d.get("text"),
        "source": "tsserver",
        "severity": if d.get("category").and_then(Value::as_str) == Some("error") { 1 } else { 2 },
        "range": {"start": pos("start"), "end": pos("end")},
    })
}

/// Is a JavaScript file type-checked? Under `// @ts-check` in the file, or `"checkJs": true` in the
/// project's config. Anything else yields an empty semantic report that means "not looked at".
pub(crate) fn js_checado(conteudo: &str, config: Option<&str>) -> bool {
    if conteudo.lines().take(20).any(|l| l.contains("@ts-check")) {
        return true;
    }
    config.is_some_and(|texto| {
        let compacto: String = texto.chars().filter(|c| !c.is_whitespace()).collect();
        compacto.contains("\"checkJs\":true")
    })
}

fn script_kind(path: &str) -> &'static str {
    if tem_extensao(path, &["tsx"]) {
        "TSX"
    } else if tem_extensao(path, &["jsx"]) {
        "JSX"
    } else if tem_extensao(path, &["js", "mjs", "cjs"]) {
        "JS"
    } else {
        "TS"
    }
}

fn e_javascript(path: &str) -> bool {
    tem_extensao(path, &["js", "jsx", "mjs", "cjs"])
}

// ── the session ────────────────────────────────────────────────────────────

/// A warm tsserver rooted at one package of the project.
pub(crate) struct SessaoTs {
    cliente: TsClient,
    root: PathBuf,
    budget: Duration,
    norm: NormTs,
}

/// Starts the project's `tsserver` (the spawner of [`crate::servidor::Servidor::Tsserver`]).
pub(crate) fn abrir(alvo: &Alvo<'_>, amb: &Ambiente) -> Result<Box<dyn Sessao>, String> {
    let achado = localizar(alvo.raiz, alvo.arquivo, amb)?;
    let mut comando = Command::new(&achado.node);
    comando
        .arg(&achado.entrada)
        .args([
            "--disableAutomaticTypingAcquisition",
            "--suppressDiagnosticEvents",
            "--noGetErrOnBackgroundUpdate",
            "--useInferredProjectPerProjectRoot",
            "--locale",
            "en",
        ])
        .current_dir(&achado.pacote);
    let cliente = TsClient::spawn(comando)
        .map_err(|e| format!("tsserver não iniciou em {}: {e}", achado.pacote.display()))?;
    Ok(Box::new(SessaoTs::de_cliente(
        cliente,
        alvo.raiz.to_path_buf(),
        DEFAULT_BUDGET,
    )))
}

impl SessaoTs {
    /// Wraps an already-spawned client (the seam tests use to inject a scripted server).
    pub(crate) fn de_cliente(cliente: TsClient, root: PathBuf, budget: Duration) -> Self {
        Self {
            cliente,
            root,
            budget,
            norm: NormTs { fonte: None },
        }
    }

    fn rodar(
        &mut self,
        pedido: &Pedido<'_>,
        abertos: &mut Vec<PathBuf>,
    ) -> Result<Vec<L2Achado>, Falha> {
        for (path, texto) in pedido.overlays {
            let arquivo = self.root.join(path);
            self.abrir_arquivo(&arquivo, texto)?;
            abertos.push(arquivo);
        }
        abertos.push(pedido.origem.clone());
        let diags = self.diagnosticar(&pedido.origem, pedido.content, pedido.deadline)?;
        let mut achados = self
            .norm
            .achados(&diags, pedido, &self.root)
            .map_err(Falha::Normalizador)?;
        if let Some(em_disco) = pedido.disk {
            let antes = self.diagnosticar(&pedido.origem, em_disco, pedido.deadline)?;
            let ja = self
                .norm
                .chaves(&antes, &pedido.com_conteudo(em_disco), &self.root);
            achados.retain(|a| !ja.contains(&(a.regra.clone(), a.nome.clone())));
        }
        Ok(achados)
    }

    fn abrir_arquivo(&mut self, arquivo: &Path, texto: &str) -> Result<(), ClientError> {
        let caminho = arquivo.to_string_lossy();
        self.cliente.enviar(
            "open",
            json!({
                "file": caminho,
                "fileContent": texto,
                "projectRootPath": self.root.to_string_lossy(),
                "scriptKindName": script_kind(&caminho),
            }),
        )?;
        Ok(())
    }

    /// One `open → projectInfo → syntactic → semantic → close` round-trip for `texto` as `arquivo`.
    fn diagnosticar(
        &mut self,
        arquivo: &Path,
        texto: &str,
        deadline: Instant,
    ) -> Result<Vec<Value>, Falha> {
        let obtido = self.perguntar(arquivo, texto, deadline);
        let _ = self
            .cliente
            .enviar("close", json!({"file": arquivo.to_string_lossy()}));
        obtido
    }

    fn perguntar(
        &mut self,
        arquivo: &Path,
        texto: &str,
        deadline: Instant,
    ) -> Result<Vec<Value>, Falha> {
        let caminho = arquivo.to_string_lossy().into_owned();
        self.abrir_arquivo(arquivo, texto)?;
        let config = self.config_do_arquivo(&caminho, deadline)?;
        exigir_checagem(&caminho, texto, &config)?;
        let sintaxe = self.cliente.pedir(
            "syntacticDiagnosticsSync",
            json!({"file": caminho}),
            deadline,
        )?;
        exigir_sintaxe(&caminho, &sintaxe)?;
        let semantica = self.cliente.pedir(
            "semanticDiagnosticsSync",
            json!({"file": caminho}),
            deadline,
        )?;
        Ok(semantica
            .as_array()
            .map(|lista| lista.iter().map(como_lsp).collect())
            .unwrap_or_default())
    }

    /// The `tsconfig.json`/`jsconfig.json` the server picked for the file, or why it picked none
    /// (an inferred project has default options and no ambient types: its diagnostics are noise).
    fn config_do_arquivo(&mut self, caminho: &str, deadline: Instant) -> Result<String, Falha> {
        let info = self.cliente.pedir(
            "projectInfo",
            json!({"file": caminho, "needFileNameList": false}),
            deadline,
        )?;
        let config = info
            .get("configFileName")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if config.is_empty() || config.starts_with(INFERRED_PROJECT) {
            return Err(Falha::Normalizador(format!(
                "camada 2 (tsserver): {caminho} não pertence a nenhum tsconfig.json do projeto (projeto inferido, sem as opções nem os tipos do projeto) — não medido"
            )));
        }
        Ok(config.to_string())
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
            Falha::Cliente(outro) => format!("camada 2 ({NOME}) indisponível: {outro}"),
        }
    }
}

/// A JavaScript file is only type-checked under `// @ts-check` or `checkJs`; without either an empty
/// semantic report means "not looked at".
fn exigir_checagem(caminho: &str, texto: &str, config: &str) -> Result<(), Falha> {
    if e_javascript(caminho) && !js_checado(texto, std::fs::read_to_string(config).ok().as_deref())
    {
        return Err(Falha::Normalizador(format!(
            "camada 2 (tsserver): {caminho} é JavaScript sem `// @ts-check` nem `checkJs` no {config}: o tsserver não checa a semântica — não medido"
        )));
    }
    Ok(())
}

/// The engine's rule: a file that does not parse is `unknown`. Semantic diagnostics of broken syntax
/// cascade (`Cannot find name` for every identifier the parser dropped).
fn exigir_sintaxe(caminho: &str, sintaxe: &Value) -> Result<(), Falha> {
    match sintaxe.as_array().and_then(|lista| lista.first()) {
        Some(erro) => Err(Falha::Normalizador(format!(
            "camada 2 (tsserver): erro de sintaxe em {caminho} (TS{}: {}) — não medido",
            erro.get("code").and_then(Value::as_u64).unwrap_or(0),
            erro.get("text")
                .and_then(Value::as_str)
                .unwrap_or("sem texto")
        ))),
        None => Ok(()),
    }
}

impl Sessao for SessaoTs {
    fn verificar(&mut self, edicao: &Edicao<'_>, prazo: Option<Duration>) -> Verificado {
        let started = Instant::now();
        let budget = prazo.unwrap_or(self.budget);
        let origem = if edicao.path.starts_with('<') {
            inline_path_ext(&self.root, edicao.path, edicao.content, "ts")
        } else {
            self.root.join(edicao.path)
        };
        let pedido = Pedido {
            uri: file_uri(&origem),
            origem,
            balde: Balde::from_path(edicao.path, &self.root),
            content: edicao.content,
            disk: edicao.disk,
            overlays: edicao.overlays,
            deadline: started + budget,
        };
        let mut abertos = Vec::new();
        let resultado = self.rodar(&pedido, &mut abertos);
        for arquivo in &abertos {
            let _ = self
                .cliente
                .enviar("close", json!({"file": arquivo.to_string_lossy()}));
        }
        let elapsed_ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        match resultado {
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

    /// The first `open` of a file loads its whole program (every file of the `tsconfig.json`): that is
    /// the warm-up, paid with the very file that asked for the session.
    fn aquecer(&mut self, edicao: &Edicao<'_>, prazo: Duration) -> Result<(), String> {
        let verificado = self.verificar(edicao, Some(prazo));
        if verificado.saudavel {
            Ok(())
        } else {
            Err(verificado
                .resultado
                .motivo
                .unwrap_or_else(|| "tsserver não ficou utilizável".to_string()))
        }
    }
}

impl SessaoTs {
    /// The OS process id of the server (for the process-accounting tests).
    #[cfg(test)]
    pub(crate) fn pid(&self) -> u32 {
        self.cliente.pid()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::achado::{Escopo, Severidade, Verdict};
    use crate::testutil::Arvore;

    /// A scripted tsserver. Before EVERY reply it emits an event and a STALE reply (`request_seq` of the
    /// previous request, empty body) — what a client that takes "the next response" would swallow.
    /// `# undef N` lines of the opened text become TS2304; `# sintaxe` becomes a syntactic diagnostic;
    /// a file under `fora/` is in an inferred project. Modes: `ok`, `silencioso` (never answers a
    /// semantic request), `morre` (exits on the first `projectInfo`).
    const FAKE_TS: &str = r##"
import sys, json
mode = sys.argv[1] if len(sys.argv) > 1 else "ok"
out = sys.stdout.buffer
docs = {}
last = [0]

def send(o):
    b = json.dumps(o).encode() + b"\n"
    out.write(b"Content-Length: %d\r\n\r\n" % len(b) + b)
    out.flush()

def reply(req, body, ok=True, message=None):
    send({"seq": 0, "type": "event", "event": "projectLoadingStart", "body": {"projectName": "x"}})
    send({"seq": 0, "type": "response", "command": req["command"], "request_seq": max(1, req["seq"] - 1),
          "success": True, "body": []})
    r = {"seq": 0, "type": "response", "command": req["command"], "request_seq": req["seq"], "success": ok}
    if ok:
        r["body"] = body
    else:
        r["message"] = message
    send(r)

def diags(text, kind):
    res = []
    for i, line in enumerate(text.split("\n")):
        if kind == "sem" and line.startswith("# undef "):
            nome = line.split()[2]
            res.append({"start": {"line": i + 1, "offset": 1}, "end": {"line": i + 1, "offset": 6},
                        "text": "Cannot find name '%s'." % nome, "code": 2304, "category": "error"})
        if kind == "syn" and line.startswith("# sintaxe"):
            res.append({"start": {"line": i + 1, "offset": 1}, "end": {"line": i + 1, "offset": 2},
                        "text": "';' expected.", "code": 1005, "category": "error"})
    return res

for raw in sys.stdin:
    req = json.loads(raw)
    cmd, args = req["command"], req.get("arguments") or {}
    if cmd == "open":
        docs[args["file"]] = args.get("fileContent") or ""
        reply(req, None)
    elif cmd == "close":
        docs.pop(args["file"], None)
        reply(req, None)
    elif cmd == "projectInfo":
        if mode == "morre":
            sys.exit(0)
        cfg = "/dev/null/inferredProject1*" if "/fora/" in args["file"] else args["file"].rsplit("/src/", 1)[0] + "/tsconfig.json"
        reply(req, {"configFileName": cfg, "languageServiceDisabled": False})
    elif cmd == "syntacticDiagnosticsSync":
        reply(req, diags(docs.get(args["file"], ""), "syn"))
    elif cmd == "semanticDiagnosticsSync":
        if mode == "silencioso":
            continue
        reply(req, diags(docs.get(args["file"], ""), "sem"))
    elif cmd == "exit":
        sys.exit(0)
    else:
        reply(req, None, ok=False, message="comando desconhecido")
"##;

    fn sessao(modo: &str, budget_ms: u64) -> (Arvore, SessaoTs) {
        let t = Arvore::nova("ts_sess");
        t.arquivo("tsconfig.json", "{}");
        let mut cmd = Command::new("python3");
        cmd.arg("-c").arg(FAKE_TS).arg(modo);
        let cliente =
            TsClient::spawn(cmd).expect("python3 must be on PATH to run the fake tsserver");
        let s = SessaoTs::de_cliente(cliente, t.raiz.clone(), Duration::from_millis(budget_ms));
        (t, s)
    }

    fn edicao<'a>(path: &'a str, content: &'a str, disk: Option<&'a str>) -> Edicao<'a> {
        Edicao {
            path,
            content,
            disk,
            overlays: &[],
        }
    }

    fn nomes(v: &Verificado) -> Vec<&str> {
        v.resultado
            .achados
            .iter()
            .map(|a| a.nome.as_str())
            .collect()
    }

    // ── the check against the scripted server ──────────────────────────────

    #[test]
    fn a_finding_is_read_from_the_reply_of_its_own_request_not_from_a_stale_one() {
        let (_t, mut s) = sessao("ok", 5000);
        let v = s.verificar(&edicao("src/a.ts", "# undef fantasma\n", None), None);
        assert_eq!(nomes(&v), ["fantasma"], "{:?}", v.resultado);
        let a = &v.resultado.achados[0];
        assert_eq!(
            (a.classe.as_str(), a.linha, a.escopo, a.regra.as_str()),
            ("nome", 1, Escopo::Proprio, "TS2304")
        );
        assert_eq!(a.severidade, Severidade::Warn);
        assert!(v.saudavel);
        let limpo = s.verificar(&edicao("src/a.ts", "export const x = 1;\n", None), None);
        assert_eq!(
            limpo.resultado.verdict,
            Verdict::Allow,
            "{:?}",
            limpo.resultado
        );
    }

    #[test]
    fn inline_code_denies() {
        let (_t, mut s) = sessao("ok", 5000);
        let v = s.verificar(&edicao("<string>", "# undef fantasma\n", None), None);
        assert_eq!(v.resultado.verdict, Verdict::Deny, "{:?}", v.resultado);
    }

    #[test]
    fn a_file_outside_every_tsconfig_is_unknown_and_keeps_the_session() {
        let (_t, mut s) = sessao("ok", 5000);
        let v = s.verificar(&edicao("fora/x.ts", "# undef fantasma\n", None), None);
        assert_eq!(v.resultado.verdict, Verdict::Unknown, "{:?}", v.resultado);
        let motivo = v.resultado.motivo.expect("reason");
        assert!(
            motivo.contains("tsconfig.json") && motivo.contains("inferido"),
            "{motivo}"
        );
        assert!(
            v.saudavel,
            "a warm program is not thrown away for a file it does not own"
        );
    }

    #[test]
    fn a_syntax_error_is_unknown_and_keeps_the_session() {
        let (_t, mut s) = sessao("ok", 5000);
        let v = s.verificar(&edicao("src/a.ts", "# sintaxe\n# undef x\n", None), None);
        assert_eq!(v.resultado.verdict, Verdict::Unknown, "{:?}", v.resultado);
        assert!(
            v.resultado
                .motivo
                .is_some_and(|m| m.contains("sintaxe") && m.contains("1005"))
        );
        assert!(v.saudavel);
    }

    #[test]
    fn the_delta_against_disk_drops_what_the_disk_already_had() {
        let (_t, mut s) = sessao("ok", 8000);
        let v = s.verificar(
            &edicao(
                "src/a.ts",
                "# undef velho\n# undef novo\n",
                Some("# undef velho\n"),
            ),
            None,
        );
        assert_eq!(nomes(&v), ["novo"], "{:?}", v.resultado);
    }

    #[test]
    fn a_silent_server_overruns_the_deadline_and_the_session_is_condemned() {
        let (_t, mut s) = sessao("silencioso", 400);
        let t0 = Instant::now();
        let v = s.verificar(&edicao("src/a.ts", "# undef x\n", None), None);
        assert_eq!(v.resultado.verdict, Verdict::Unknown, "{:?}", v.resultado);
        assert!(v.resultado.motivo.is_some_and(|m| m.contains("prazo")));
        assert!(!v.saudavel);
        assert!(
            t0.elapsed() < Duration::from_millis(2500),
            "{:?}",
            t0.elapsed()
        );
    }

    #[test]
    fn a_dead_server_is_unknown_with_its_name_and_the_session_is_condemned() {
        let (_t, mut s) = sessao("morre", 5000);
        let v = s.verificar(&edicao("src/a.ts", "# undef x\n", None), None);
        assert_eq!(v.resultado.verdict, Verdict::Unknown, "{:?}", v.resultado);
        assert!(
            v.resultado
                .motivo
                .is_some_and(|m| m.contains("tsserver") && m.contains("indisponível"))
        );
        assert!(!v.saudavel);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn dropping_the_session_kills_and_reaps_the_server() {
        let (_t, s) = sessao("ok", 5000);
        let pid = s.pid();
        assert!(Path::new(&format!("/proc/{pid}")).exists());
        drop(s);
        assert!(
            !Path::new(&format!("/proc/{pid}")).exists(),
            "server {pid} must be reaped"
        );
    }

    #[test]
    fn a_failed_reply_is_a_server_error_with_its_message() {
        let mensagem = json!({"type": "response", "success": false, "message": "No Project."});
        let err = resposta_ou_erro(&mensagem, "projectInfo").unwrap_err();
        assert!(
            matches!(&err, ClientError::ServerError { message, .. } if message == "No Project."),
            "{err:?}"
        );
    }

    // ── pure pieces ────────────────────────────────────────────────────────

    #[test]
    fn tsserver_positions_become_zero_based_lsp_ranges() {
        let d = json!({"start": {"line": 3, "offset": 25}, "end": {"line": 3, "offset": 26},
            "text": "Property 'z' does not exist on type 'Ponto'.", "code": 2339, "category": "error"});
        let lsp = como_lsp(&d);
        assert_eq!(lsp["range"]["start"], json!({"line": 2, "character": 24}));
        assert_eq!(lsp["range"]["end"], json!({"line": 2, "character": 25}));
        assert_eq!(
            (lsp["code"].clone(), lsp["message"].as_str()),
            (
                json!(2339),
                Some("Property 'z' does not exist on type 'Ponto'.")
            )
        );
    }

    #[test]
    fn javascript_is_checked_only_under_ts_check_or_check_js() {
        assert!(js_checado("// @ts-check\nlet x;\n", None));
        assert!(js_checado(
            "let x;\n",
            Some("{ \"compilerOptions\": { \"checkJs\": true } }")
        ));
        assert!(js_checado("let x;\n", Some("{\"checkJs\":true}")));
        assert!(!js_checado(
            "let x;\n",
            Some("{ \"compilerOptions\": { \"checkJs\": false } }")
        ));
        assert!(!js_checado("let x;\n", None));
        assert!(
            !js_checado(&format!("{}// @ts-check\n", "\n".repeat(30)), None),
            "only a header comment counts"
        );
    }

    #[test]
    fn script_kinds_follow_the_extension() {
        for (path, kind) in [
            ("a.ts", "TS"),
            ("a.mts", "TS"),
            ("a.tsx", "TSX"),
            ("a.js", "JS"),
            ("a.mjs", "JS"),
            ("a.jsx", "JSX"),
        ] {
            assert_eq!(script_kind(path), kind, "{path}");
        }
    }

    // ── discovery: the project's TypeScript, never a global one, never an install ──

    #[cfg(unix)]
    mod descoberta {
        use super::*;

        #[test]
        fn abrir_runs_the_projects_tsserver_with_typing_acquisition_off_end_to_end() {
            let t = Arvore::nova("ts_e2e");
            t.arquivo("tsconfig.json", "{}");
            t.arquivo("node_modules/typescript/lib/tsserver.js", "");
            let script = t.arquivo("fake_ts.py", FAKE_TS);
            // a fake `node` that records how it was called and then becomes the scripted tsserver
            t.executavel(
                "fakebin/node",
                &format!(
                    "#!/bin/sh\necho \"$@\" >> '{}'\nexec python3 '{}' ok\n",
                    t.raiz.join("WITNESS").display(),
                    script.display()
                ),
            );
            let amb = Ambiente::com_path(vec![t.raiz.join("fakebin")]);
            let alvo = Alvo {
                servidor: crate::servidor::Servidor::Tsserver,
                raiz: &t.raiz,
                arquivo: "src/a.ts",
            };
            let mut sessao = abrir(&alvo, &amb).expect("the fake server starts");
            let edicao = Edicao {
                path: "src/a.ts",
                content: "# undef fantasma\n",
                disk: None,
                overlays: &[],
            };
            let v = sessao.verificar(&edicao, Some(Duration::from_secs(20)));
            assert_eq!(nomes(&v), ["fantasma"], "{:?}", v.resultado);
            let chamada = std::fs::read_to_string(t.raiz.join("WITNESS")).unwrap();
            let entrada = t.raiz.join("node_modules/typescript/lib/tsserver.js");
            assert!(
                chamada.contains(&*entrada.to_string_lossy()),
                "the project's tsserver.js is what runs: {chamada}"
            );
            assert!(
                chamada.contains("--disableAutomaticTypingAcquisition"),
                "tsserver must never be allowed to `npm install @types/*`: {chamada}"
            );
            assert!(
                chamada.contains("--locale en"),
                "the finding table reads English messages: {chamada}"
            );
        }

        fn com_node(t: &Arvore) -> Ambiente {
            t.executavel("fakebin/node", "#!/bin/sh\n");
            Ambiente::com_path(vec![t.raiz.join("fakebin")])
        }

        #[test]
        fn the_nearest_node_modules_typescript_wins_and_names_the_package() {
            let t = Arvore::nova("ts_disc_near");
            t.arquivo("node_modules/typescript/lib/tsserver.js", "");
            t.arquivo("web/node_modules/typescript/lib/tsserver.js", "");
            let amb = com_node(&t);
            let perto = localizar(&t.raiz, "web/src/a.ts", &amb).unwrap();
            assert_eq!(perto.pacote, t.raiz.join("web"));
            assert_eq!(
                perto.entrada,
                t.raiz.join("web/node_modules/typescript/lib/tsserver.js")
            );
            assert_eq!(perto.node, t.raiz.join("fakebin/node"));
            let raiz = localizar(&t.raiz, "api/src/b.ts", &amb).unwrap();
            assert_eq!(raiz.pacote, t.raiz, "no closer install: the root's");
            assert_eq!(subchave(&t.raiz, "web/src/a.ts"), t.raiz.join("web"));
        }

        #[test]
        fn a_global_tsserver_on_path_is_never_a_substitute() {
            let t = Arvore::nova("ts_disc_global");
            t.arquivo("src/a.ts", "");
            t.executavel("fakebin/node", "#!/bin/sh\n");
            t.executavel("fakebin/tsserver", "#!/bin/sh\n");
            t.executavel("fakebin/typescript-language-server", "#!/bin/sh\n");
            let amb = Ambiente::com_path(vec![t.raiz.join("fakebin")]);
            let motivo = localizar(&t.raiz, "src/a.ts", &amb).unwrap_err();
            assert!(
                motivo.contains("node_modules/typescript") && motivo.contains("nunca o instala"),
                "{motivo}"
            );
            assert_eq!(subchave(&t.raiz, "src/a.ts"), PathBuf::new());
        }

        #[test]
        fn a_typescript_above_the_project_root_is_not_the_projects() {
            let t = Arvore::nova("ts_disc_above");
            t.arquivo("node_modules/typescript/lib/tsserver.js", "");
            t.arquivo("proj/src/a.ts", "");
            let amb = com_node(&t);
            let raiz = t.raiz.join("proj");
            assert!(localizar(&raiz, "src/a.ts", &amb).is_err());
        }

        #[test]
        fn without_node_the_project_typescript_cannot_run() {
            let t = Arvore::nova("ts_disc_nonode");
            t.arquivo("node_modules/typescript/lib/tsserver.js", "");
            let motivo =
                localizar(&t.raiz, "src/a.ts", &Ambiente::com_path(Vec::new())).unwrap_err();
            assert!(motivo.contains("node"), "{motivo}");
        }
    }
}
