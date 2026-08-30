//! Stage **X6 CAPABILITY-GATE** of the Code Execution Gateway. Phase **P3.6**
//! of CEG Pln2 (`docs/2026-05-17-ceg-pln2-plan.md`).
//!
//! X5 ran the code once in the sandbox; X6 asks the next question: *if this
//! code runs for real, what authority would it exercise, and does the granted
//! [`CapabilityProfile`] permit it?*
//!
//! Two steps:
//!
//! 1. [`required_capabilities`] derives — lexically — the [`Capability`] set the
//!    code body would exercise: subprocess spawns, file writes, outbound
//!    network connections, environment reads.
//! 2. [`gate_capabilities`] resolves each required capability against the
//!    granted profile — deny-by-default, deny-wins (see [`crate::capability`])
//!    — producing a [`GateReport`].
//!
//! # A lexical, fail-closed heuristic
//!
//! Capability extraction is **lexical**, not semantic — a token scan, the same
//! discipline as X3's [`extract_symbols`](super::vgp_stage::extract_symbols). It
//! deliberately over-detects: when a scope cannot be determined it is recorded
//! at its broadest (an `FsWrite` of `/`, a `Run` of an unknown command), so the
//! gate resolves it against the profile's *default* disposition. Under a
//! deny-by-default profile that fails **closed** — the safe direction for a
//! security gate. A precise, AST-level capability extractor is a future
//! refinement, and would only ever *narrow* a detected capability, never miss
//! one.
//!
//! `FsRead` is deliberately not detected: every built-in profile already grants
//! workspace read, so flagging it would be pure noise. X6 concentrates on the
//! four authority classes a profile actually restricts — file write, network,
//! subprocess, environment.

use super::capture::ExecSurface;
use super::typestate::{Execution, Gated, SandboxTested};
use crate::capability::{
    Capability, CapabilityProfile, CmdScope, Decision, HostScope, KeyScope, PathScope,
};
use serde::{Deserialize, Serialize};

// ── Required-capability extraction ────────────────────────────────────────────

/// A single [`Capability`] the executed code was inferred to require, paired
/// with the lexical `operation` — a command name or source token — that
/// triggered the inference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityNeed {
    /// The authority the code would exercise.
    pub capability: Capability,
    /// The command or source token that revealed the need — kept for the
    /// human-readable gate report and the X7 canonical fix.
    pub operation: String,
}

impl CapabilityNeed {
    /// Pair a [`Capability`] with the `operation` that revealed it.
    #[must_use]
    pub fn new(capability: Capability, operation: impl Into<String>) -> Self {
        Self {
            capability,
            operation: operation.into(),
        }
    }
}

/// Shell metacharacters that separate one command from the next.
const SHELL_SEPARATORS: &[char] = &[';', '|', '&', '\n', '(', ')'];

/// Shell commands that open an outbound network connection.
const NET_COMMANDS: &[&str] = &[
    "curl", "wget", "nc", "ncat", "socat", "ssh", "scp", "sftp", "rsync", "ftp", "telnet",
];

/// Shell words that the shell itself executes — no `fork`/`exec`, so no
/// subprocess capability is required to run them.
///
/// Measured motivation (2026-08-27): every shell word was being emitted as a
/// `Capability::Run`, so `echo oi` and `cd /tmp` denied under `sandboxed` with
/// the SAME composite (0.675) as `curl http://evil.test`. A gate that fires on
/// 100% of legitimate runs carries no information, and the noise is what
/// justified downgrading shell denials wholesale — which then threw the real
/// network signal away with it (see `run.rs::gate_run`).
///
/// Deliberately EXCLUDED, though they are builtins too: `eval`, `exec`,
/// `source`, `.` and `trap` — each executes text as code or replaces the
/// process, so treating them as inert would launder exactly the construct an
/// attacker reaches for. `command`/`builtin` are excluded for the same reason:
/// they take a real command as argument.
const SHELL_BUILTINS: &[&str] = &[
    ":", "[", "[[", "alias", "break", "case", "cd", "continue", "declare", "do", "done",
    "echo", "elif", "else", "esac", "export", "false", "fi", "for", "function", "if",
    "in", "let", "local", "printf", "pwd", "read", "readonly", "return", "select", "set",
    "shift", "test", "then", "true", "typeset", "unalias", "unset", "until", "while",
];

/// Whether `command` is a shell builtin that runs in-process (see
/// [`SHELL_BUILTINS`] for why `eval`/`exec`/`source` are NOT here).
fn is_shell_builtin(command: &str) -> bool {
    SHELL_BUILTINS.contains(&command)
}

/// Source tokens that signal a subprocess spawn, across the sandbox languages.
///
/// A bare `system(` covers the Python `os.system(...)`, C `system(...)` and
/// Ruby `Kernel.system(...)` call forms, so no language-prefixed alias is
/// needed for the dominant case.
const RUN_TOKENS: &[&str] = &[
    "subprocess",
    "os.popen",
    "os.spawn",
    "child_process",
    "popen(",
    "Popen",
    "system(",
    "spawn(",
    "Runtime.getRuntime",
    "ProcessBuilder",
    "IO.popen",
    "std::process::Command",
];

/// Source tokens that signal an outbound network connection.
const NET_TOKENS: &[&str] = &[
    "socket",
    "urllib",
    "http.client",
    "requests.",
    "httpx",
    "fetch(",
    "XMLHttpRequest",
    "Net::HTTP",
    "net/http",
    "axios",
    "urlopen",
    "aiohttp",
    "reqwest",
    "HttpClient",
];

/// Source tokens that signal a filesystem write.
const WRITE_TOKENS: &[&str] = &[
    ".write(",
    "writeFile",
    "File.write",
    "fs.write",
    "ofstream",
    "O_WRONLY",
    "O_CREAT",
    "Files.write",
    "shutil.copy",
    "shutil.move",
    "os.remove",
    "os.unlink",
    "os.mkdir",
];

/// Source tokens that signal an environment-variable read.
const ENV_TOKENS: &[&str] = &[
    "os.environ",
    "getenv",
    "process.env",
    "ENV[",
    "std::env::var",
    "System.getenv",
];

/// Derive the [`Capability`] set the code body would exercise if run for real.
///
/// Dispatches on the [`ExecSurface`]: a bash surface is tokenised into shell
/// commands; a `ctx_execute` / inferlet surface is scanned for the per-language
/// capability tokens. Each distinct capability appears once, in first-seen
/// order — the result is deterministic.
#[must_use]
pub fn required_capabilities(code: &str, surface: ExecSurface) -> Vec<CapabilityNeed> {
    match surface {
        ExecSurface::BashCommand => bash_capability_needs(code),
        ExecSurface::CtxExecute | ExecSurface::Inferlet => code_capability_needs(code),
        ExecSurface::NonExec => Vec::new(),
    }
}

/// Append `need` only if its [`Capability`] is not already present — keeps the
/// need list deduplicated by capability while preserving first-seen order.
fn push_unique(needs: &mut Vec<CapabilityNeed>, need: CapabilityNeed) {
    if !needs.iter().any(|n| n.capability == need.capability) {
        needs.push(need);
    }
}

/// The first whitespace-token of `segment` that is a command, with any
/// `VAR=value` prefix skipped and any directory prefix stripped (`/bin/rm` →
/// `rm`), so the result lines up with a profile's [`CmdScope`] grants.
fn leading_command(segment: &str) -> Option<&str> {
    segment
        .split_whitespace()
        .find(|tok| !tok.contains('='))
        .map(|tok| tok.rsplit('/').next().unwrap_or(tok))
        .filter(|cmd| !cmd.is_empty())
}

/// `true` when the shell code redirects to, or `tee`s into, a file.
///
/// File-descriptor duplication (`2>&1`, `>&2`) is excluded — it redirects an
/// fd, it does not name a file to write.
/// Alvos de redirecionamento que NÃO persistem nada: escrever neles é
/// descartar (ou reapontar um descritor), nunca criar/alterar um arquivo.
///
/// `2>/dev/null` é o idioma universal de silenciar stderr e aparecia em
/// praticamente todo comando real. Contá-lo como `fs-write` fazia um comando
/// legítimo somar `subprocess` + `fs-write` e negar DURO depois que o waiver
/// passou a ser seletivo (27/08/2026) — o waiver cego anterior escondia isso.
const NON_PERSISTENT_TARGETS: &[&str] = &[
    "/dev/null", "/dev/stdout", "/dev/stderr", "/dev/tty", "/dev/fd/",
];

/// Whether the redirection target starting at `rest` merely discards output.
fn redirects_to_the_void(rest: &str) -> bool {
    let alvo = rest.trim_start().trim_start_matches(['"', '\'']);
    NON_PERSISTENT_TARGETS.iter().any(|t| alvo.starts_with(t))
}

/// Comandos que escrevem por ARGUMENTO, sem redirect — o `>` não aparece, mas
/// o arquivo é criado do mesmo jeito.
///
/// Ponto em aberto do S9, fechado em 27/08/2026. `dd if=/dev/zero of=alvo`
/// persistia sem que `bash_writes_a_file` visse nada, porque a função só
/// procurava o operador de redirecionamento. O sandbox contém o filesystem, então
/// isto nunca foi um furo de contenção — era detecção incompleta, e um `fs-write`
/// invisível é exatamente a classe que o waiver seletivo precisa enxergar para
/// decidir certo.
fn writes_via_argument(code: &str) -> bool {
    const POR_ARGUMENTO: &[&str] = &[
        "of=",        // dd
        "--output=",  // curl, sort, objcopy…
        "--output ",
        "-o ",        // curl -o, gcc -o, sort -o
        "--out-file=",
    ];
    // `-o` só conta depois de um verbo que realmente grava — senão `ls -o`
    // (formato longo sem grupo) viraria escrita.
    let tem_o_de_saida = code.contains(" -o ")
        && ["curl", "wget", "sort", "gcc", "cc", "objcopy", "ffmpeg", "tar"]
            .iter()
            .any(|v| code.contains(v));
    POR_ARGUMENTO
        .iter()
        .filter(|p| **p != "-o ")
        .any(|p| code.contains(*p))
        || tem_o_de_saida
}

fn bash_writes_a_file(code: &str) -> bool {
    if code.contains("tee ") || code.contains("| tee") {
        return true;
    }
    if writes_via_argument(code) {
        return true;
    }
    let bytes = code.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b != b'>' {
            continue;
        }
        if bytes.get(i + 1) == Some(&b'&') {
            continue; // `>&` — fd duplication, not a file write.
        }
        let mut j = i + 1;
        while bytes.get(j) == Some(&b'>') {
            j += 1; // `>>` append redirection.
        }
        while bytes.get(j).is_some_and(u8::is_ascii_whitespace) {
            j += 1;
        }
        // `> /dev/null` descarta; não é escrita que alguém precise autorizar.
        if code.get(j..).is_some_and(redirects_to_the_void) {
            continue;
        }
        if bytes.get(j).is_some_and(|c| {
            c.is_ascii_alphanumeric()
                || matches!(c, b'/' | b'.' | b'_' | b'~' | b'$' | b'"' | b'\'')
        }) {
            return true;
        }
    }
    false
}

/// Apaga redirecionamentos de DESCRITOR antes da segmentação.
///
/// `SHELL_SEPARATORS` inclui `&`, então `cmd 2>&1 | head` era partido em
/// `["cmd 2>", "1 ", " head"]` e o `1` residual virava o nome de um programa —
/// o X6 negava a "capability de subprocesso `1`". O `&` de um `>&` liga um
/// descritor a outro; não separa comandos, e o dígito ao lado não é um binário.
///
/// Cobre `2>&1`, `>&2`, `1>&2`, `&>arquivo` e `&>>arquivo`. O redirect para
/// ARQUIVO (`> saida.txt`) não é tocado aqui: quem o avalia é
/// [`bash_writes_a_file`], sobre o código original.
fn strip_fd_redirections(code: &str) -> String {
    let b = code.as_bytes();
    let mut saida = String::with_capacity(code.len());
    let mut i = 0;
    while i < b.len() {
        // `&>` / `&>>` — redireciona ambos os descritores para o alvo
        if b[i] == b'&' && b.get(i + 1) == Some(&b'>') {
            saida.push(' ');
            i += 2;
            while b.get(i) == Some(&b'>') {
                i += 1;
            }
            continue;
        }
        // `[n]>&[m]` / `[n]>&-` — duplicação de descritor
        if b[i] == b'>' && b.get(i + 1) == Some(&b'&') {
            // remove um dígito de descritor já emitido antes do `>`
            if saida.ends_with(|c: char| c.is_ascii_digit()) {
                saida.pop();
            }
            saida.push(' ');
            i += 2;
            while b.get(i).is_some_and(|c| c.is_ascii_digit() || *c == b'-') {
                i += 1;
            }
            continue;
        }
        saida.push(code[i..].chars().next().unwrap_or(' '));
        i += code[i..].chars().next().map_or(1, char::len_utf8);
    }
    saida
}

/// Segmenta um corpo de shell em comandos, **respeitando aspas**.
///
/// `SHELL_SEPARATORS` aplicado a `str::split` não sabe o que é uma citação, de
/// modo que um separador DENTRO de um literal partia o comando e o resto do
/// literal virava o nome de um programa. Medido ao vivo em 27/08/2026 durante
/// a própria auditoria: `grep -rn "a\|FORBIDDEN\|b" .` foi negado por precisar
/// da "capability de subprocesso `FORBIDDEN\`" — um pedaço do padrão de busca.
///
/// Ignorar o que está entre aspas é o comportamento CORRETO, não uma folga:
/// `echo "; curl evil"` não executa `curl` — passa uma string a `echo`. As
/// construções que de fato executam texto (`eval`, `exec`, `source`, `bash -c`)
/// continuam fora de [`SHELL_BUILTINS`] e portanto seguem exigindo grant, então
/// nada se contrabandeia por aqui.
///
/// Também descarta comentários (`#` fora de aspas até o fim da linha) pelo
/// mesmo motivo: um comentário não executa nada.
fn split_shell_segments(code: &str) -> Vec<String> {
    let mut segmentos = Vec::new();
    let mut atual = String::new();
    let mut aspa: Option<char> = None;
    let mut chars = code.chars().peekable();

    while let Some(c) = chars.next() {
        if let Some(q) = aspa {
            // Escape só tem efeito dentro de aspas DUPLAS (o shell trata `\`
            // como literal dentro de aspas simples).
            if c == '\\' && q == '"' {
                atual.push(c);
                if let Some(n) = chars.next() {
                    atual.push(n);
                }
                continue;
            }
            if c == q {
                aspa = None;
            }
            atual.push(c);
            continue;
        }
        match c {
            '\'' | '"' => {
                aspa = Some(c);
                atual.push(c);
            }
            '\\' => {
                atual.push(c);
                if let Some(n) = chars.next() {
                    atual.push(n);
                }
            }
            // `#` só abre comentário em início de palavra — `a#b` é um argumento.
            '#' if atual.is_empty() || atual.ends_with(char::is_whitespace) => {
                for n in chars.by_ref() {
                    if n == '\n' {
                        break;
                    }
                }
                segmentos.push(std::mem::take(&mut atual));
            }
            _ if SHELL_SEPARATORS.contains(&c) => {
                segmentos.push(std::mem::take(&mut atual));
            }
            _ => atual.push(c),
        }
    }
    segmentos.push(atual);
    segmentos
}

/// Extract the capability needs of a shell command body.
fn bash_capability_needs(code: &str) -> Vec<CapabilityNeed> {
    let mut needs = Vec::new();
    let sem_fd = strip_fd_redirections(code);
    for segment in split_shell_segments(&sem_fd) {
        let Some(command) = leading_command(&segment) else {
            continue;
        };
        if NET_COMMANDS.contains(&command) {
            push_unique(
                &mut needs,
                CapabilityNeed::new(Capability::Net(HostScope::any()), command),
            );
        }
        // A builtin is executed BY the shell — no spawn, so no subprocess need.
        // The segment split above means skipping `cd` in `cd /x && rm -rf /` does
        // not hide the `rm`: that is a separate segment with its own command.
        if !is_shell_builtin(command) {
            push_unique(
                &mut needs,
                CapabilityNeed::new(Capability::Run(CmdScope::new(command)), command),
            );
        }
    }
    if bash_writes_a_file(code) {
        push_unique(
            &mut needs,
            CapabilityNeed::new(Capability::FsWrite(PathScope::new("/")), "redirection"),
        );
    }
    needs
}

/// Apaga comentários e corpos de literais de string, preservando as quebras de
/// linha.
///
/// A varredura de capability só deve enxergar o que o programa EXECUTA. Um
/// comentário e o miolo de um literal não executam nada — mas `code.contains`
/// não sabe disso, e o efeito foi medido por execução em 27/08/2026: o
/// comentário `# socket` e o literal `print("a palavra socket")` eram AMBOS
/// negados como pedido de rede.
///
/// Isto não abre contrabando. O único jeito de um literal virar código é passar
/// por `eval` / `exec` / `compile` / `__import__`, e esses são pegos pela tabela
/// independente de padrões proibidos
/// (`touring_hooks_shared::forbidden_patterns`), que roda sobre o texto
/// ORIGINAL. São duas redes com propósitos distintos, e só uma delas é lexical.
///
/// Cobre os idiomas de comentário das linguagens do sandbox (`#`, `//`, `/* */`)
/// e as formas de string simples e tripla, com escape por contrabarra.
fn blank_comments_and_literals(code: &str) -> String {
    let b: Vec<char> = code.chars().collect();
    let mut saida = String::with_capacity(code.len());
    let mut i = 0usize;

    // Um caractere apagado vira espaço, salvo a quebra de linha — preservá-la
    // impede que duas linhas se fundam e criem um token que não existia.
    let apagado = |c: char| if c == '\n' { '\n' } else { ' ' };

    while i < b.len() {
        // ── comentário até o fim da linha ────────────────────────────────────
        if b[i] == '#' || (b[i] == '/' && b.get(i + 1) == Some(&'/')) {
            while i < b.len() && b[i] != '\n' {
                saida.push(' ');
                i += 1;
            }
            continue;
        }
        // ── comentário de bloco ──────────────────────────────────────────────
        if b[i] == '/' && b.get(i + 1) == Some(&'*') {
            while i < b.len() && !(b[i] == '*' && b.get(i + 1) == Some(&'/')) {
                saida.push(apagado(b[i]));
                i += 1;
            }
            // O fechamento, quando existe; um bloco não fechado consome o resto.
            for _ in 0..2 {
                if i < b.len() {
                    saida.push(' ');
                    i += 1;
                }
            }
            continue;
        }
        // ── literal de string ────────────────────────────────────────────────
        if b[i] == '"' || b[i] == '\'' {
            let aspa = b[i];
            let triplo = b.get(i + 1) == Some(&aspa) && b.get(i + 2) == Some(&aspa);
            let delim = if triplo { 3 } else { 1 };
            for _ in 0..delim {
                saida.push(' ');
            }
            i += delim;
            while i < b.len() {
                if b[i] == '\\' {
                    saida.push(' ');
                    i += 1;
                    if i < b.len() {
                        saida.push(apagado(b[i]));
                        i += 1;
                    }
                    continue;
                }
                if b[i] == aspa
                    && (!triplo || (b.get(i + 1) == Some(&aspa) && b.get(i + 2) == Some(&aspa)))
                {
                    for _ in 0..delim {
                        saida.push(' ');
                        i += 1;
                    }
                    break;
                }
                // Uma string de aspa única não atravessa a linha: se a linha
                // acabou, o literal estava malformado — parar aqui em vez de
                // engolir o resto do programa (fail-closed: o que sobra volta a
                // ser varrido normalmente).
                if !triplo && b[i] == '\n' {
                    break;
                }
                saida.push(apagado(b[i]));
                i += 1;
            }
            continue;
        }
        saida.push(b[i]);
        i += 1;
    }
    saida
}

/// `true` quando a ocorrência em `pos` é o token INTEIRO, e não o pedaço de um
/// identificador maior.
///
/// `subprocess_count = 3` continha `subprocess` e por isso pedia a capability de
/// subprocesso; `websocket` pedia rede. A fronteira só é exigida do lado em que
/// o token de fato termina em caractere de palavra — `system(` já se fecha.
fn is_whole_token(code: &str, pos: usize, token: &str) -> bool {
    let palavra = |c: char| c.is_alphanumeric() || c == '_';
    let antes_ok = !token.starts_with(palavra)
        || !matches!(code[..pos].chars().next_back(), Some(c) if palavra(c));
    let depois_ok = !token.ends_with(palavra)
        || !matches!(code[pos + token.len()..].chars().next(), Some(c) if palavra(c));
    antes_ok && depois_ok
}

/// O primeiro token de `tokens` presente em `code` como token inteiro,
/// desconsiderando comentários e literais de string.
///
/// Até 27/08/2026 isto era `code.contains(t)` — substring nua, em qualquer
/// posição, inclusive dentro de um comentário. Três falsos positivos foram
/// medidos por execução no mesmo minuto (`# socket`, `"a palavra socket"`,
/// `subprocess_count`), todos NEGANDO a execução. Um gate que nega o caso comum
/// ensina a contorná-lo, que é o oposto de uma afordância.
fn first_token_in<'a>(code: &str, tokens: &[&'a str]) -> Option<&'a str> {
    let visivel = blank_comments_and_literals(code);
    tokens.iter().copied().find(|t| {
        let mut de = 0usize;
        while let Some(rel) = visivel[de..].find(*t) {
            let pos = de + rel;
            if is_whole_token(&visivel, pos, t) {
                return true;
            }
            de = pos + t.len().max(1);
        }
        false
    })
}

/// Binários que uma chamada de subprocesso NOMEIA explicitamente no código.
///
/// `subprocess.run(["rg", …])`, `Popen(["fd", …])`, `os.system("rg foo")` e
/// `Command::new("rg")` carregam todos o binário como o primeiro literal do
/// argumento. Derivá-lo converte o pedido de `Run(any)` — que nenhum perfil
/// deny-by-default pode conceder, por construção — em `Run("rg")`, que o
/// allowlist de inspeção do perfil `Sandboxed` concede.
///
/// Devolve `(binários, houve_indecifrável)`. **Fail-closed**: se QUALQUER
/// ocorrência de token de run não permitir derivar o binário, o segundo campo
/// é `true` e o chamador mantém o pedido `Run(any)` ao lado dos derivados —
/// um programa com `run(["rg"])` E `system(var)` continua pedindo `any`.
/// Nenhum pedido é removido; o mecanismo só refina o que já era pedido.
///
/// Origem (2026-08-25): `touring run --lang python` não conseguia chamar `rg`
/// enquanto a chamada `Bash` ao lado conseguia — o caminho preferido era mais
/// fraco que o atômico, e a adoção de code mode pagava a conta.
fn invoked_binaries(code: &str) -> (Vec<String>, bool) {
    /// Janela de busca do literal após o token de chamada.
    const LOOKAHEAD: usize = 80;
    let mut bins: Vec<String> = Vec::new();
    let mut underivable = false;

    for token in RUN_TOKENS {
        let mut from = 0usize;
        while let Some(rel) = code[from..].find(token) {
            let after = from + rel + token.len();
            from = after;
            let janela = &code[after..code.len().min(after + LOOKAHEAD)];
            // O literal precisa estar na MESMA linha lógica da chamada.
            let janela = janela.split('\n').next().unwrap_or("");
            // Só POSIÇÃO DE CHAMADA conta. `import subprocess` menciona o token
            // sem invocar nada — tratá-lo como invocação indecifrável fazia todo
            // programa Python que importa o módulo pedir `Run(any)`, anulando a
            // derivação inteira (o teste de ponta-a-ponta pegou isso).
            let Some(args) = call_site_args(token, janela) else {
                continue;
            };
            match first_string_literal(args).and_then(|lit| command_name(&lit)) {
                Some(bin) => {
                    if !bins.contains(&bin) {
                        bins.push(bin);
                    }
                }
                None => underivable = true,
            }
        }
    }
    (bins, underivable)
}

/// A região de argumentos, quando `token` está em POSIÇÃO DE CHAMADA em `linha`.
///
/// Um token que já termina em `(` (`system(`, `popen(`) abre a chamada por si.
/// Caso contrário, aceita-se uma cadeia de atributos (`.run`, `::new`) até o
/// `(`. `None` quando não há chamada — `import subprocess`, uma menção em
/// comentário, o token dentro de outra string.
fn call_site_args<'a>(token: &str, linha: &'a str) -> Option<&'a str> {
    if token.ends_with('(') {
        return Some(linha);
    }
    let cadeia = linha
        .find('(')
        .filter(|abre| {
            linha[..*abre]
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':'))
        })?;
    Some(&linha[cadeia + 1..])
}

/// Primeiro literal entre aspas (simples ou duplas) de `s`, sem as aspas.
fn first_string_literal(s: &str) -> Option<String> {
    let abre = s.find(['"', '\''])?;
    let aspa = s.as_bytes()[abre] as char;
    let resto = &s[abre + 1..];
    let fecha = resto.find(aspa)?;
    Some(resto[..fecha].to_string())
}

/// Nome de comando extraído de um literal (`"/usr/bin/rg -l x"` → `rg`).
/// `None` quando o literal não é um nome de comando simples — interpolação,
/// variável, caminho com espaço — caso em que o chamador falha fechado.
fn command_name(literal: &str) -> Option<String> {
    let primeiro = literal.split_whitespace().next()?;
    let base = primeiro.rsplit('/').next()?;
    let plausivel = !base.is_empty()
        && base
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
    plausivel.then(|| base.to_string())
}

/// Extract the capability needs of an inline code body (`ctx_execute`,
/// inferlet) via the per-class token tables.
fn code_capability_needs(code: &str) -> Vec<CapabilityNeed> {
    let mut needs = Vec::new();
    if let Some(tok) = first_token_in(code, RUN_TOKENS) {
        let (bins, underivable) = invoked_binaries(code);
        for bin in &bins {
            push_unique(
                &mut needs,
                CapabilityNeed::new(Capability::Run(CmdScope::new(bin)), format!("{tok} → {bin}")),
            );
        }
        if bins.is_empty() || underivable {
            push_unique(
                &mut needs,
                CapabilityNeed::new(Capability::Run(CmdScope::any()), tok),
            );
        }
    }
    if let Some(tok) = first_token_in(code, NET_TOKENS) {
        push_unique(
            &mut needs,
            CapabilityNeed::new(Capability::Net(HostScope::any()), tok),
        );
    }
    if let Some(tok) = first_token_in(code, WRITE_TOKENS) {
        push_unique(
            &mut needs,
            CapabilityNeed::new(Capability::FsWrite(PathScope::new("/")), tok),
        );
    }
    if let Some(tok) = first_token_in(code, ENV_TOKENS) {
        push_unique(
            &mut needs,
            CapabilityNeed::new(Capability::Env(KeyScope::new("*")), tok),
        );
    }
    needs
}

/// The authority class of a [`Capability`], as a stable label for reports and
/// the X7 canonical fix.
#[must_use]
pub fn capability_class(capability: &Capability) -> &'static str {
    match capability {
        Capability::FsRead(_) => "file-read",
        Capability::FsWrite(_) => "file-write",
        Capability::Net(_) => "network",
        Capability::Run(_) => "subprocess",
        Capability::Env(_) => "environment",
    }
}

/// `true` for the authority classes a profile genuinely restricts — a denied
/// one of these is a hard block at X7. An `Env` / `FsRead` denial is a warning,
/// not a hard block: a generic environment read cannot be told apart from a
/// credential read at this lexical level.
fn is_high_authority(capability: &Capability) -> bool {
    matches!(
        capability,
        Capability::Run(_) | Capability::FsWrite(_) | Capability::Net(_)
    )
}

// ── The gate report ───────────────────────────────────────────────────────────

/// One [`CapabilityNeed`] resolved against the granted [`CapabilityProfile`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GatedCapability {
    /// The required authority.
    pub capability: Capability,
    /// The operation that revealed the need.
    pub operation: String,
    /// How the granted profile resolved the request.
    pub decision: Decision,
}

/// The **X6 CAPABILITY-GATE** result: every required capability resolved
/// against the granted profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateReport {
    /// The name of the profile the requests were resolved against.
    pub profile_name: String,
    /// One entry per required capability, in first-seen order.
    pub gated: Vec<GatedCapability>,
}

/// Rank a [`Decision`] by severity — `Allow < Prompt < Deny`.
fn decision_rank(decision: Decision) -> u8 {
    match decision {
        Decision::Allow => 0,
        Decision::Prompt => 1,
        Decision::Deny => 2,
    }
}

impl GateReport {
    /// The most severe [`Decision`] across every gated capability —
    /// `Allow < Prompt < Deny`. An empty report (the code needs no restricted
    /// authority) resolves to [`Decision::Allow`].
    #[must_use]
    pub fn worst_decision(&self) -> Decision {
        self.gated
            .iter()
            .map(|g| g.decision)
            .max_by_key(|&d| decision_rank(d))
            .unwrap_or(Decision::Allow)
    }

    /// `true` when every required capability resolved to [`Decision::Allow`]
    /// (vacuously true for an empty report).
    #[must_use]
    pub fn is_clear(&self) -> bool {
        self.gated.iter().all(|g| g.decision == Decision::Allow)
    }

    /// The first denied high-authority capability — a subprocess, file write or
    /// network connection the profile refused. This is the hard-block signal
    /// the X7 DECISION stage acts on.
    #[must_use]
    pub fn first_blocking_denial(&self) -> Option<&GatedCapability> {
        self.gated
            .iter()
            .find(|g| g.decision == Decision::Deny && is_high_authority(&g.capability))
    }

    /// `true` when at least one high-authority capability was denied.
    #[must_use]
    pub fn has_blocking_denial(&self) -> bool {
        self.first_blocking_denial().is_some()
    }

    /// Every denied capability, regardless of authority class.
    pub fn denied(&self) -> impl Iterator<Item = &GatedCapability> {
        self.gated.iter().filter(|g| g.decision == Decision::Deny)
    }
}

/// Resolve every [`CapabilityNeed`] against `profile`, producing the
/// **X6 CAPABILITY-GATE** [`GateReport`].
#[must_use]
pub fn gate_capabilities(needs: &[CapabilityNeed], profile: &CapabilityProfile) -> GateReport {
    let gated = needs
        .iter()
        .map(|n| GatedCapability {
            capability: n.capability.clone(),
            operation: n.operation.clone(),
            decision: profile.resolve(&n.capability),
        })
        .collect();
    GateReport {
        profile_name: profile.name().to_owned(),
        gated,
    }
}

// ── X6 transition ─────────────────────────────────────────────────────────────

impl Execution<SandboxTested> {
    /// **X6 CAPABILITY-GATE** — derive the code's required capabilities, resolve
    /// them against `profile`, attach the [`GateReport`] to the evidence ledger
    /// and advance to [`Gated`].
    ///
    /// The code body and surface come from the X1
    /// [`Classification`](super::classify::Classification) when one is on the
    /// ledger; absent it (an execution advanced here by bare
    /// [`advance`](Execution::advance)), the raw payload and the tool-name
    /// surface are used instead — so the gate is always sound.
    pub fn capability_gate(mut self, profile: &CapabilityProfile) -> Execution<Gated> {
        let report = {
            let (code, surface) = match self.evidence().classification.as_ref() {
                Some(cls) => (cls.code_body.source.as_str(), cls.surface),
                None => (
                    self.raw().payload.as_str(),
                    ExecSurface::detect(&self.raw().tool),
                ),
            };
            let needs = required_capabilities(code, surface);
            gate_capabilities(&needs, profile)
        };
        self.evidence_mut().gate_report = Some(report);
        self.advance()
    }
}

#[cfg(test)]
mod tests {

    /// Escrita por ARGUMENTO é escrita — o `>` não é a única forma de gravar.
    ///
    /// `dd if=/dev/zero of=alvo` era o ponto em aberto: persistia sem que o
    /// classificador visse `fs-write`. O sandbox contém o FS, então nunca foi
    /// furo de contenção — era detecção incompleta, e o waiver seletivo decide
    /// com base nas classes que enxerga.
    #[test]
    fn writing_via_argument_counts_as_a_file_write() {
        for cmd in [
            "dd if=/dev/zero of=/tmp/alvo bs=1 count=1",
            "curl -o /tmp/baixado http://x",
            "curl --output=/tmp/baixado http://x",
            "sort -o ordenado.txt entrada.txt",
        ] {
            let needs = super::required_capabilities(cmd, super::ExecSurface::BashCommand);
            assert!(
                needs
                    .iter()
                    .any(|n| matches!(n.capability, super::Capability::FsWrite(_))),
                "`{cmd}` grava arquivo por argumento: {needs:?}"
            );
        }
    }

    /// E um `-o` que NÃO é saída não vira escrita — `ls -o` é formato longo.
    #[test]
    fn a_dash_o_that_is_not_output_is_not_a_write() {
        for cmd in ["ls -o", "ls -o /tmp", "ps -o pid,comm"] {
            let needs = super::required_capabilities(cmd, super::ExecSurface::BashCommand);
            assert!(
                !needs
                    .iter()
                    .any(|n| matches!(n.capability, super::Capability::FsWrite(_))),
                "`{cmd}` não grava nada: {needs:?}"
            );
        }
    }

    /// 2026-08-27 (P0) — `2>&1` não inventa um programa chamado `1`.
    ///
    /// `SHELL_SEPARATORS` inclui `&`, então `cmd 2>&1 | head` era partido em
    /// `["cmd 2>", "1 ", " head"]` e o `1` residual virava nome de comando. O
    /// X6 negava a "capability de subprocesso `1`". Latente desde sempre;
    /// virou deny DURO quando o waiver deixou de ser cego, porque somava com o
    /// `fs-write` que o `2>/dev/null` do mesmo comando produzia.
    #[test]
    fn fd_duplication_does_not_invent_a_command() {
        for cmd in [
            "ls 2>&1",
            "ls 2>&1 | head",
            "cmd 1>&2",
            "cmd &>saida.txt",
            "cmd 2>&-",
        ] {
            let needs = super::required_capabilities(cmd, super::ExecSurface::BashCommand);
            let nomes: Vec<&str> = needs.iter().map(|n| n.operation.as_str()).collect();
            for fantasma in ["1", "2", "-"] {
                assert!(
                    !nomes.contains(&fantasma),
                    "`{cmd}` inventou o comando `{fantasma}`: {nomes:?}"
                );
            }
        }
    }

    /// O comando REAL sobrevive à limpeza dos redirects — o conserto não pode
    /// cegar o classificador junto.
    #[test]
    fn stripping_fd_redirections_keeps_the_real_command() {
        let needs = super::required_capabilities(
            "curl http://x 2>&1 | tail -1",
            super::ExecSurface::BashCommand,
        );
        assert!(
            needs.iter().any(|n| matches!(n.capability, super::Capability::Net(_))),
            "o `curl` continua sendo rede: {needs:?}"
        );
        let needs = super::required_capabilities("rm -rf /tmp/x 2>/dev/null", super::ExecSurface::BashCommand);
        assert!(
            needs.iter().any(|n| matches!(&n.capability,
                super::Capability::Run(sc) if sc.matches(&super::CmdScope::new("rm")))),
            "o `rm` continua exigindo grant: {needs:?}"
        );
    }

    /// 2026-08-27 (P0) — `/dev/null` não é escrita de arquivo.
    ///
    /// `2>/dev/null` aparece em quase todo comando real. Contá-lo como
    /// `fs-write` fazia um comando legítimo somar duas classes e negar duro.
    #[test]
    fn discarding_output_is_not_a_file_write() {
        for cmd in [
            "ls 2>/dev/null",
            "cmd >/dev/null",
            "cmd > /dev/null 2>&1",
            "cmd >/dev/stdout",
            "cmd 2>/dev/tty",
        ] {
            let needs = super::required_capabilities(cmd, super::ExecSurface::BashCommand);
            assert!(
                !needs
                    .iter()
                    .any(|n| matches!(n.capability, super::Capability::FsWrite(_))),
                "`{cmd}` descarta a saída — não é escrita: {needs:?}"
            );
        }
    }

    /// E o redirect para ARQUIVO continua sendo escrita — o conserto distingue
    /// descarte de persistência, não desliga a detecção.
    #[test]
    fn redirecting_to_a_real_file_is_still_a_write() {
        for cmd in ["echo x > saida.txt", "cmd >> log.txt", "cmd > /tmp/dados"] {
            let needs = super::required_capabilities(cmd, super::ExecSurface::BashCommand);
            assert!(
                needs
                    .iter()
                    .any(|n| matches!(n.capability, super::Capability::FsWrite(_))),
                "`{cmd}` persiste — é escrita: {needs:?}"
            );
        }
    }

    /// 2026-08-27 — builtins do shell não são spawn.
    ///
    /// Antes, TODA palavra virava `Capability::Run`, então `echo oi` negava com
    /// o MESMO composite (0.675) que `curl http://evil.test`. Um gate que
    /// dispara em 100% dos runs legítimos não carrega informação — e foi esse
    /// ruído que justificou rebaixar todo deny de shell, jogando fora o sinal
    /// de rede junto.
    #[test]
    fn shell_builtins_do_not_require_a_subprocess_grant() {
        for cmd in ["echo oi", "cd /tmp", "pwd", "true", "export X=1", "printf %s x"] {
            let needs = super::required_capabilities(cmd, super::ExecSurface::BashCommand);
            assert!(
                !needs
                    .iter()
                    .any(|n| matches!(n.capability, super::Capability::Run(_))),
                "`{cmd}` é builtin — não pode exigir subprocess: {needs:?}"
            );
        }
    }

    /// O que NÃO é builtin continua exigindo o grant — senão o conserto teria
    /// desligado o gate em vez de calibrá-lo.
    #[test]
    fn real_commands_still_require_a_subprocess_grant() {
        for cmd in ["rm -rf /tmp/x", "curl http://x", "python3 -c 1", "dd if=/dev/zero"] {
            let needs = super::required_capabilities(cmd, super::ExecSurface::BashCommand);
            assert!(
                needs
                    .iter()
                    .any(|n| matches!(n.capability, super::Capability::Run(_))),
                "`{cmd}` spawna processo — o grant é obrigatório"
            );
        }
    }

    /// `eval`/`exec`/`source` são builtins mas executam TEXTO como código: se
    /// entrassem na lista, o construto que um atacante usa para se esconder
    /// ficaria isento por definição.
    #[test]
    fn code_executing_builtins_are_not_waived() {
        for cmd in ["eval \"$X\"", "exec /bin/sh", "source /tmp/x.sh"] {
            let needs = super::required_capabilities(cmd, super::ExecSurface::BashCommand);
            assert!(
                needs
                    .iter()
                    .any(|n| matches!(n.capability, super::Capability::Run(_))),
                "`{cmd}` executa código — jamais isento"
            );
        }
    }

    /// Pular o builtin não pode esconder o que vem depois dele: `cd /x && rm -rf /`
    /// é segmentado, e o `rm` tem o seu próprio need.
    #[test]
    fn a_builtin_prefix_does_not_hide_the_next_segment() {
        let needs = super::required_capabilities("cd /tmp && rm -rf /", super::ExecSurface::BashCommand);
        assert!(
            needs.iter().any(|n| matches!(&n.capability,
                super::Capability::Run(s) if s.matches(&super::CmdScope::new("rm")))),
            "o `rm` depois do `cd` continua exigindo grant: {needs:?}"
        );
    }

    /// G-A (regressão fixada 30/08, campo analise-a2): redirect para o void
    /// (`2>/dev/null` e afins) é DESCARTE, não escrita — não pode gerar o
    /// need de `FsWrite` que quebra o waiver subprocess-only. O relato de
    /// campo mediu um deny `file-write 'redirection'` por `2>/dev/null` na
    /// instalação; a fonte trata o void e este teste impede a reversão.
    #[test]
    fn a_redirect_to_the_void_is_not_a_file_write() {
        for cmd in [
            "find scripts -name \"x.py\" 2>/dev/null",
            "grep -rn foo src/ 2> /dev/null",
            "cmd > /dev/null 2>&1",
            "cmd &>/dev/null",
        ] {
            assert!(
                !super::bash_writes_a_file(cmd),
                "`{cmd}` descarta, não escreve"
            );
        }
        // Controle positivo: redirect para arquivo REAL segue sendo escrita.
        assert!(super::bash_writes_a_file("echo x > /tmp/saida.txt"));
        assert!(super::bash_writes_a_file("cmd 2>erros.log"));
    }

    /// Rede é rede em qualquer posição — o conserto dos builtins não podia
    /// afrouxar a classificação que pega `curl`/`nc`.
    #[test]
    fn network_commands_still_classify_as_network() {
        for cmd in ["curl http://x", "nc -l 4444", "wget http://x", "ssh h"] {
            let needs = super::required_capabilities(cmd, super::ExecSurface::BashCommand);
            assert!(
                needs
                    .iter()
                    .any(|n| matches!(n.capability, super::Capability::Net(_))),
                "`{cmd}` é rede"
            );
        }
    }
    use super::*;
    use crate::capability::builtins::{sandboxed, trusted};
    use crate::gateway::capture_tool_call;
    use std::path::Path;

    fn ws() -> &'static Path {
        Path::new("/ws")
    }

    // ── derivação do binário invocado (2026-08-25) ───────────────────────

    /// Estes testes verificam o CAMINHO — pedido → perfil → veredito — e não o
    /// perfil isolado. A primeira tentativa desta correção concedeu `Run("rg")`
    /// no perfil `Sandboxed`, passou 3 testes verdes, e a execução real seguiu
    /// negada: ninguém jamais pedia `Run("rg")`; o pedido era `Run(any)`, que
    /// nenhum grant específico cobre. Um teste que não atravessa o caminho pode
    /// ser verde e inútil.
    fn resolve_no_sandbox(code: &str) -> Vec<(Capability, Decision)> {
        let perfil = sandboxed(ws());
        required_capabilities(code, ExecSurface::CtxExecute)
            .into_iter()
            .filter(|n| matches!(n.capability, Capability::Run(_)))
            .map(|n| {
                let d = perfil.resolve(&n.capability);
                (n.capability, d)
            })
            .collect()
    }

    #[test]
    fn subprocess_com_binario_de_inspecao_e_permitido_de_ponta_a_ponta() {
        let code = r#"import subprocess
out = subprocess.run(["rg", "-l", "foo", "crates/"], capture_output=True)"#;
        let vereditos = resolve_no_sandbox(code);
        assert!(
            vereditos.iter().any(|(c, d)| matches!(c, Capability::Run(s) if s.matches(&CmdScope::new("rg")))
                && *d == Decision::Allow),
            "o binário `rg` tinha de ser derivado E concedido: {vereditos:?}"
        );
        assert!(
            vereditos.iter().all(|(_, d)| *d == Decision::Allow),
            "nenhum pedido residual `any` deveria sobrar: {vereditos:?}"
        );
    }

    #[test]
    fn subprocess_com_binario_perigoso_segue_negado() {
        let code = r#"import subprocess
subprocess.run(["rm", "-rf", "/tmp/x"])"#;
        let vereditos = resolve_no_sandbox(code);
        assert!(!vereditos.is_empty());
        assert!(
            vereditos.iter().all(|(_, d)| *d == Decision::Deny),
            "`rm` derivado tem de ser negado pelo perfil Sandboxed: {vereditos:?}"
        );
    }

    /// Fail-closed: binário indecifrável mantém o pedido `any`, que nenhum
    /// perfil deny-by-default concede.
    #[test]
    fn binario_nao_derivavel_mantem_o_pedido_any() {
        let code = "import os\nos.system(comando_da_variavel)";
        let vereditos = resolve_no_sandbox(code);
        assert!(
            vereditos
                .iter()
                .any(|(c, _)| matches!(c, Capability::Run(s) if *s == CmdScope::any())),
            "sem literal não há derivação — o pedido `any` tem de permanecer"
        );
        assert!(vereditos.iter().all(|(_, d)| *d == Decision::Deny));
    }

    /// O caso que faria a correção virar um furo: um binário seguro derivado
    /// AO LADO de uma invocação opaca não pode "limpar" o pedido conservador.
    #[test]
    fn derivavel_junto_com_opaco_ainda_pede_any() {
        let code = r#"import subprocess, os
subprocess.run(["rg", "foo"])
os.system(alguma_variavel)"#;
        let vereditos = resolve_no_sandbox(code);
        assert!(
            vereditos
                .iter()
                .any(|(c, _)| matches!(c, Capability::Run(s) if *s == CmdScope::any())),
            "a invocação opaca tem de manter o `any`: {vereditos:?}"
        );
        assert!(
            vereditos.iter().any(|(_, d)| *d == Decision::Deny),
            "e o veredito global continua Deny"
        );
    }

    #[test]
    fn deriva_de_system_e_de_command_new() {
        let (bins, _) = invoked_binaries("os.system(\"rg -n foo src/\")");
        assert!(bins.contains(&"rg".to_string()), "os.system: {bins:?}");
        let (bins, _) = invoked_binaries("std::process::Command::new(\"fd\")");
        assert!(bins.contains(&"fd".to_string()), "Command::new: {bins:?}");
        // Caminho absoluto reduz ao nome do comando.
        let (bins, _) = invoked_binaries("subprocess.run([\"/usr/bin/rg\", \"x\"])");
        assert!(bins.contains(&"rg".to_string()), "caminho: {bins:?}");
    }

    // ── required_capabilities — bash surface ──────────────────────────────

    #[test]
    fn required_capabilities_bash_dedups_repeated_command() {
        let needs = required_capabilities("cargo build && cargo test", ExecSurface::BashCommand);
        let runs: Vec<_> = needs
            .iter()
            .filter(|n| matches!(n.capability, Capability::Run(_)))
            .collect();
        assert_eq!(runs.len(), 1, "`cargo` appears twice but needs one Run");
        assert_eq!(runs[0].operation, "cargo");
    }

    #[test]
    fn required_capabilities_bash_distinct_commands() {
        let needs = required_capabilities("git status; rm tmp", ExecSurface::BashCommand);
        let ops: Vec<&str> = needs.iter().map(|n| n.operation.as_str()).collect();
        assert!(ops.contains(&"git"), "got {ops:?}");
        assert!(ops.contains(&"rm"), "got {ops:?}");
    }

    #[test]
    fn required_capabilities_bash_strips_directory_prefix() {
        // `/usr/bin/rm` must reduce to `rm` so it lines up with a CmdScope grant.
        let needs = required_capabilities("/usr/bin/rm tmp", ExecSurface::BashCommand);
        assert!(needs.iter().any(|n| n.operation == "rm"), "{needs:?}");
    }

    #[test]
    fn required_capabilities_bash_redirection_is_fswrite() {
        let needs = required_capabilities("echo hi > out.txt", ExecSurface::BashCommand);
        assert!(
            needs
                .iter()
                .any(|n| matches!(n.capability, Capability::FsWrite(_))),
            "a `>` redirection must require FsWrite: {needs:?}"
        );
    }

    #[test]
    fn required_capabilities_bash_fd_dup_is_not_a_write() {
        // `2>&1` redirects a file descriptor — it does not write a file.
        let needs = required_capabilities("ls -la 2>&1", ExecSurface::BashCommand);
        assert!(
            !needs
                .iter()
                .any(|n| matches!(n.capability, Capability::FsWrite(_))),
            "fd duplication is not a file write: {needs:?}"
        );
    }

    #[test]
    fn required_capabilities_bash_network_command() {
        let needs = required_capabilities("curl https://example.test", ExecSurface::BashCommand);
        assert!(
            needs
                .iter()
                .any(|n| matches!(n.capability, Capability::Net(_))),
            "`curl` must require Net: {needs:?}"
        );
        assert!(
            needs
                .iter()
                .any(|n| matches!(n.capability, Capability::Run(_)))
        );
    }

    // ── required_capabilities — code surface ──────────────────────────────

    #[test]
    fn required_capabilities_code_subprocess() {
        let needs = required_capabilities(
            "import subprocess; subprocess.run(['ls'])",
            ExecSurface::CtxExecute,
        );
        assert!(
            needs
                .iter()
                .any(|n| matches!(n.capability, Capability::Run(_))),
            "{needs:?}"
        );
    }

    #[test]
    fn required_capabilities_code_network() {
        let needs = required_capabilities("requests.get('http://x')", ExecSurface::CtxExecute);
        assert!(
            needs
                .iter()
                .any(|n| matches!(n.capability, Capability::Net(_))),
            "{needs:?}"
        );
    }

    #[test]
    fn required_capabilities_code_write_and_env() {
        let needs = required_capabilities(
            "open('o.txt','w').write(os.environ['HOME'])",
            ExecSurface::CtxExecute,
        );
        assert!(
            needs
                .iter()
                .any(|n| matches!(n.capability, Capability::FsWrite(_)))
        );
        assert!(
            needs
                .iter()
                .any(|n| matches!(n.capability, Capability::Env(_)))
        );
    }

    #[test]
    fn required_capabilities_nonexec_surface_is_empty() {
        assert!(required_capabilities("anything", ExecSurface::NonExec).is_empty());
    }

    #[test]
    fn required_capabilities_pure_code_needs_nothing() {
        // No capability token — a pure computation requires no restricted authority.
        assert!(required_capabilities("total = 2 + 2", ExecSurface::CtxExecute).is_empty());
    }

    // ── gate_capabilities + GateReport ────────────────────────────────────

    #[test]
    fn gate_capabilities_denies_run_under_sandboxed() {
        let needs = vec![CapabilityNeed::new(
            Capability::Run(CmdScope::new("python3")),
            "python3",
        )];
        let report = gate_capabilities(&needs, &sandboxed(ws()));
        assert_eq!(report.profile_name, "Sandboxed");
        assert_eq!(report.gated[0].decision, Decision::Deny);
        assert!(!report.is_clear());
        assert_eq!(report.worst_decision(), Decision::Deny);
    }

    #[test]
    fn gate_capabilities_allows_safe_run_under_trusted() {
        let needs = vec![CapabilityNeed::new(
            Capability::Run(CmdScope::new("cargo")),
            "cargo",
        )];
        let report = gate_capabilities(&needs, &trusted());
        assert_eq!(report.gated[0].decision, Decision::Allow);
        assert!(report.is_clear());
        assert_eq!(report.worst_decision(), Decision::Allow);
    }

    #[test]
    fn empty_gate_report_is_clear_and_allows() {
        let report = gate_capabilities(&[], &sandboxed(ws()));
        assert!(report.is_clear());
        assert_eq!(report.worst_decision(), Decision::Allow);
        assert!(!report.has_blocking_denial());
    }

    #[test]
    fn first_blocking_denial_ignores_low_authority_env() {
        // A denied Run is a hard block; a denied Env is not — a generic env read
        // cannot be told from a credential read at the lexical level.
        let run_denied = gate_capabilities(
            &[CapabilityNeed::new(
                Capability::Run(CmdScope::new("rm")),
                "rm",
            )],
            &sandboxed(ws()),
        );
        assert!(run_denied.has_blocking_denial());

        let env_denied = gate_capabilities(
            &[CapabilityNeed::new(
                Capability::Env(KeyScope::new("*")),
                "environ",
            )],
            &sandboxed(ws()),
        );
        assert_eq!(env_denied.worst_decision(), Decision::Deny);
        assert!(
            !env_denied.has_blocking_denial(),
            "an Env denial alone is not a hard block"
        );
        assert_eq!(env_denied.denied().count(), 1);
    }

    #[test]
    fn capability_class_labels_every_variant() {
        assert_eq!(
            capability_class(&Capability::FsRead(PathScope::new("/"))),
            "file-read"
        );
        assert_eq!(
            capability_class(&Capability::FsWrite(PathScope::new("/"))),
            "file-write"
        );
        assert_eq!(
            capability_class(&Capability::Net(HostScope::any())),
            "network"
        );
        assert_eq!(
            capability_class(&Capability::Run(CmdScope::any())),
            "subprocess"
        );
        assert_eq!(
            capability_class(&Capability::Env(KeyScope::new("X"))),
            "environment"
        );
    }

    #[test]
    fn gate_report_serde_roundtrip() {
        let report = gate_capabilities(
            &[CapabilityNeed::new(
                Capability::Run(CmdScope::new("rm")),
                "rm",
            )],
            &trusted(),
        );
        let json = serde_json::to_string(&report).expect("serialize");
        let back: GateReport = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(report, back);
    }

    // ── X6 transition ─────────────────────────────────────────────────────

    #[test]
    fn capability_gate_transition_attaches_report_and_advances() {
        // Bare `advance()` to X5 — no classification on the ledger — exercises
        // the raw-payload fallback inside `capability_gate`.
        // ES1 P3 (2026-06-01): X3.5 PROVE inserted between X3 and X4,
        // so 5 advances become 6; ordinals renumber accordingly.
        let sandbox_tested = capture_tool_call("Bash", "cargo build > log.txt", None)
            .expect("Bash is code-bearing")
            .advance() // X1
            .advance() // X2
            .advance() // X3
            .advance() // X3.5 PROVE
            .advance() // X4
            .advance(); // X5
        assert_eq!(sandbox_tested.ordinal(), 6);

        let gated = sandbox_tested.capability_gate(&trusted());
        assert_eq!(gated.ordinal(), 7);
        assert_eq!(gated.stage(), "X6-CAPABILITY-GATE");
        let report = gated
            .evidence()
            .gate_report
            .as_ref()
            .expect("capability_gate must attach a GateReport");
        assert_eq!(report.profile_name, "Trusted");
        // `cargo` (Run) + the `> log.txt` redirection (FsWrite).
        assert!(report.gated.iter().any(|g| g.operation == "cargo"));
        assert!(
            report
                .gated
                .iter()
                .any(|g| matches!(g.capability, Capability::FsWrite(_)))
        );
    }

    // ── Classificação léxica (auditoria cruzada 27/08/2026) ──────────────────
    //
    // Os três primeiros casos NÃO são hipotéticos: foram medidos por execução
    // contra o binário instalado, cada um devolvendo um deny do X6.

    /// Um comentário não executa nada — não pode pedir capability de rede.
    #[test]
    fn a_comment_mentioning_a_token_needs_nothing() {
        let needs = code_capability_needs("# socket\nprint(1)\n");
        assert!(needs.is_empty(), "comentário pediu {needs:?}");
    }

    /// Nem o miolo de um literal. Este é o caso que mais dói na prática: um
    /// programa de code mode que PROCURA a palavra `socket` no repositório era
    /// negado por citá-la.
    #[test]
    fn a_string_literal_mentioning_a_token_needs_nothing() {
        let needs = code_capability_needs(r#"print("a palavra socket num literal")"#);
        assert!(needs.is_empty(), "literal pediu {needs:?}");
        let busca = code_capability_needs("alvo = 'subprocess'\nfor f in fs:\n    pass\n");
        assert!(busca.is_empty(), "literal de aspa simples pediu {busca:?}");
    }

    /// Um identificador MAIOR que contém o token não é o token.
    #[test]
    fn a_longer_identifier_is_not_the_token() {
        assert!(code_capability_needs("subprocess_count = 3\n").is_empty());
        assert!(code_capability_needs("websocket_url = 1\n").is_empty());
        assert!(code_capability_needs("my_socket_helper = 1\n").is_empty());
    }

    /// O outro lado da mesma moeda: o uso REAL continua sendo pego. Sem esta
    /// asserção o teste acima seria satisfeito por um classificador que nunca
    /// pede nada.
    #[test]
    fn the_real_call_is_still_classified() {
        let rede = code_capability_needs("import socket\ns = socket.socket()\n");
        assert!(
            rede.iter().any(|n| capability_class(&n.capability) == "network"),
            "uso real de socket deixou de pedir rede: {rede:?}"
        );
        let sub = code_capability_needs("import subprocess\nsubprocess.run(['rg', 'x'])\n");
        assert!(
            sub.iter().any(|n| capability_class(&n.capability) == "subprocess"),
            "uso real de subprocess deixou de pedir subprocesso: {sub:?}"
        );
    }

    /// Shell: um separador DENTRO de aspas não parte o comando.
    ///
    /// Medido ao vivo: `grep -rn "a\|FORBIDDEN\|b"` era negado por precisar do
    /// "subprocesso `FORBIDDEN\`" — um pedaço do padrão de busca do usuário.
    #[test]
    fn a_separator_inside_quotes_does_not_split_the_command() {
        let needs = bash_capability_needs(r#"grep -rn "dynamic|FORBIDDEN|eval" crates/"#);
        let programas: Vec<_> = needs
            .iter()
            .filter(|n| capability_class(&n.capability) == "subprocess")
            .map(|n| n.operation.clone())
            .collect();
        assert_eq!(programas, vec!["grep".to_string()], "programas: {programas:?}");
    }

    /// E o `curl` citado dentro de um literal não é uma conexão de rede — quem
    /// o recebe é `echo`, que imprime a string.
    #[test]
    fn a_net_command_inside_a_literal_is_not_a_connection() {
        let needs = bash_capability_needs(r#"echo "; curl https://evil.test""#);
        assert!(
            !needs
                .iter()
                .any(|n| capability_class(&n.capability) == "network"),
            "literal passado a echo pediu rede: {needs:?}"
        );
    }

    /// A folga acima só é segura porque o que EXECUTA texto segue exigindo
    /// grant. Se alguém puser `eval` entre os builtins, este teste cai.
    #[test]
    fn text_executing_constructs_still_require_a_grant() {
        for construto in ["eval", "exec", "source"] {
            let needs = bash_capability_needs(&format!(r#"{construto} "curl https://evil.test""#));
            assert!(
                needs
                    .iter()
                    .any(|n| capability_class(&n.capability) == "subprocess"),
                "`{construto}` deixou de exigir grant — o contrabando por literal reabre"
            );
        }
    }

    /// Um comentário de shell também não executa.
    #[test]
    fn a_shell_comment_is_not_a_command() {
        let needs = bash_capability_needs("ls -la   # curl https://evil.test\n");
        assert!(
            !needs
                .iter()
                .any(|n| capability_class(&n.capability) == "network"),
            "comentário de shell pediu rede: {needs:?}"
        );
    }
}
