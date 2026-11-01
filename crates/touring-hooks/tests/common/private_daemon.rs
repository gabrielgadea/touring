//! Daemon privado para testes E2E que invocam o binário `touring`.
//!
//! **Por que existe.** Os testes E2E que rodam `touring <hook>` falavam com o
//! daemon GLOBAL (`/tmp/touring-daemon-1000.sock`) — o mesmo que atende as
//! sessões vivas do Claude Code. Sob a suíte completa (várias crates em
//! paralelo, máquina saturada) o daemon não respondia a tempo, o cliente saía
//! com stdout vazio e o teste quebrava no parse do JSON. O sintoma acusava o
//! código sob teste; a causa era a competição por um recurso compartilhado.
//!
//! Isolar o daemon torna esses testes **determinísticos** e, de quebra, impede
//! que a suíte polua o estado do daemon que o usuário está usando.
//!
//! Extraído de `wave24_hook_integration_e2e.rs` em 03/08/2026, quando
//! `e2e_diagnostic_rfc100.rs` reproduziu exatamente a mesma flakiness
//! (`b310_path_wired_when_predictive_blast_injects_symbols`, verde isolado e
//! vermelho na suíte completa). Duas cópias do mesmo helper seriam a assimetria
//! que a REGRA #0 manda evitar.

use std::io::Write;
use std::process::{Command, Stdio};

/// Localiza um binário do workspace, preferindo `release` a `debug`.
///
/// Devolve `None` quando não foi compilado — os testes tratam isso como SKIP,
/// não como falha (o harness E2E roda em debug invocando o binário de release).
///
/// Fonte única dos testes que spawnam `touring`/`touring-daemon`/`touring-hook`
/// (as cópias de `e2e_diagnostic_rfc100` e `wave24_hook_integration_e2e`
/// delegam aqui). A ordem das raízes está em [`target_roots`].
#[must_use]
pub fn locate_binary(name: &str) -> Option<std::path::PathBuf> {
    let workspace_target = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|p| p.join("target"))?;
    let roots = target_roots(
        std::env::var_os("CARGO_TARGET_DIR").map(std::path::PathBuf::from),
        std::env::current_exe().ok().as_deref(),
        &workspace_target,
    );
    pick_binary(name, &roots, std::path::Path::exists)
}

/// Raízes de `target/` em ordem de preferência:
///
/// 1. `CARGO_TARGET_DIR` explícito;
/// 2. a raiz que compilou ESTE binário de teste (`<raiz>/<perfil>/deps/<teste>`);
/// 3. `target/` do workspace;
/// 4. `target/llvm-cov-target/`.
///
/// A raiz do próprio teste vem antes das fixas porque é a única que pertence
/// ao build em curso. `cargo llvm-cov` redireciona o build para
/// `target/llvm-cov-target/` (o job de cobertura falhou por não achar o
/// binário em 02/08/2026), e numa rodada de cobertura o teste já mora lá. Numa
/// rodada comum, a mesma pasta guarda a sobra de uma cobertura antiga: com ela
/// em segundo lugar, o E2E do juiz de 14/09/2026 subiu um daemon instrumentado
/// de 13:53, nove horas mais velho que as correções que certificava, e cada
/// daemon desses deixou um `default_*.profraw` na raiz do workspace.
#[must_use]
pub fn target_roots(
    cargo_target_dir: Option<std::path::PathBuf>,
    test_exe: Option<&std::path::Path>,
    workspace_target: &std::path::Path,
) -> Vec<std::path::PathBuf> {
    let own_build = test_exe
        .and_then(std::path::Path::parent)
        .filter(|deps| deps.file_name().is_some_and(|n| n == "deps"))
        .and_then(std::path::Path::parent)
        .and_then(std::path::Path::parent)
        .map(std::path::Path::to_path_buf);
    let mut roots: Vec<std::path::PathBuf> = Vec::new();
    for root in [
        cargo_target_dir,
        own_build,
        Some(workspace_target.to_path_buf()),
        Some(workspace_target.join("llvm-cov-target")),
    ]
    .into_iter()
    .flatten()
    {
        if !roots.contains(&root) {
            roots.push(root);
        }
    }
    roots
}

/// Primeiro `<raiz>/{release,debug}/<name>` que existe, na ordem das raízes.
#[must_use]
pub fn pick_binary(
    name: &str,
    roots: &[std::path::PathBuf],
    exists: impl Fn(&std::path::Path) -> bool,
) -> Option<std::path::PathBuf> {
    roots
        .iter()
        .flat_map(|root| ["release", "debug"].map(|profile| root.join(profile).join(name)))
        .find(|candidate| exists(candidate))
}

/// Mundo fechado de um teste: a raiz do projeto e o `HOME` que o daemon E os
/// clientes dele enxergam, no lugar do estado ambiente da máquina.
///
/// Isolar exige os DOIS pinos, nos DOIS lados, porque o roteamento tem duas
/// metades que o `start_in` sozinho não cobre:
///
/// * o daemon escolhe o projeto pelo cwd/ambiente do CLIENTE que conecta —
///   `TOURING_PROJECT_ROOT` no spawn do daemon é só o default (medido em
///   25/09/2026, e de novo em 10/10/2026: um `memory store` com cwd no repo
///   principal gravou na base VIVA dele, apesar do daemon pinado em `/tmp`);
/// * o recall é FEDERADO: lê todo `memory.db` sob `$HOME/.claude`, então sem
///   trocar o `HOME` um sinal «semeado» disputa o ranking com o que a máquina
///   tiver (o seed ficou fora do top-2 do recall real numa das duas consultas).
///
/// Quem monta o mundo (tempdir, `create_dir_all`) é o chamador; o pino só o descreve.
#[derive(Clone, Debug)]
pub struct ProjectPin {
    /// Raiz do projeto: cwd e `CLAUDE_PROJECT_DIR`/`TOURING_PROJECT_ROOT` dos clientes.
    pub root: std::path::PathBuf,
    /// `HOME` do daemon e dos clientes (a recall federada varre `$HOME/.claude`).
    pub home: std::path::PathBuf,
}

/// Cache de pesos do fastembed da máquina REAL, para o daemon de `HOME` trocado.
///
/// Sem isto o daemon procura o modelo em `<home falso>/.claude/touring/models/` e
/// BAIXA da rede (medido em 10/10/2026: 417 MB gravados no HOME falso, e o
/// primeiro recall estourou o orçamento de 15 s do handler). A regra espelha
/// `touring_storage::embeddings::providers::fastembed::fastembed_cache_dir`
/// (`$TOURING_FASTEMBED_CACHE`, senão `~/.claude/touring/models/fastembed`); este
/// arquivo é incluído por caminho em crates que não dependem de `touring-storage`,
/// então a regra é repetida aqui em vez de importada. Só existente: sem o cache
/// o daemon se comporta como sempre se comportou na máquina sem modelo.
fn real_fastembed_cache() -> Option<std::path::PathBuf> {
    let dir = std::env::var_os("TOURING_FASTEMBED_CACHE")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(|h| std::path::PathBuf::from(h).join(".claude/touring/models/fastembed"))
        })?;
    dir.is_dir().then_some(dir)
}

/// Aponta o ambiente de um CLIENTE para o mundo de `pin`: HOME, projeto e cwd.
fn pin_client_env(cmd: &mut Command, pin: &ProjectPin) {
    cmd.env("HOME", &pin.home)
        .env("CLAUDE_PROJECT_DIR", &pin.root)
        .env("TOURING_PROJECT_ROOT", &pin.root)
        .current_dir(&pin.root);
}

/// Daemon `touring` próprio deste processo de teste, num socket exclusivo.
///
/// O `Drop` sinaliza pelo **PID que este processo criou** — nunca por nome
/// (REGRA #19: matar por nome derruba bridges MCP e handlers de outras sessões).
pub struct PrivateDaemon {
    pid: u32,
    socket: String,
    pin: Option<ProjectPin>,
}

impl PrivateDaemon {
    /// Sobe um daemon num socket único para (processo, `tag`). `None` quando o
    /// binário não foi compilado — um skip, não uma falha.
    #[must_use]
    pub fn start(tag: &str) -> Option<Self> {
        Self::start_in(tag, None, None)
    }

    /// Como [`Self::start`], mas pinando a raiz do projeto que o daemon serve
    /// (`TOURING_PROJECT_ROOT`) e, opcionalmente, um watchdog de ociosidade
    /// (`TOURING_IDLE_TIMEOUT_SECS`) para o daemon se encerrar sozinho — o que
    /// permite um daemon por PROCESSO de teste guardado num `OnceLock` (cujo
    /// `Drop` nunca roda) sem vazar processos (REGRA #19).
    ///
    /// ATENÇÃO: isto pina só o DEFAULT do daemon. Os clientes que o teste spawna
    /// herdam o cwd/ambiente de quem os lança e é por eles que o daemon escolhe o
    /// projeto — para um teste que depende do estado do projeto, use
    /// [`Self::start_isolated`] + [`Self::client_command`].
    #[must_use]
    pub fn start_in(
        tag: &str,
        project_root: Option<&std::path::Path>,
        idle_secs: Option<u64>,
    ) -> Option<Self> {
        Self::spawn(tag, project_root, idle_secs, None)
    }

    /// Daemon FECHADO no mundo de `pin` (ver [`ProjectPin`]): `HOME` e projeto
    /// próprios, embedding em CPU (não toma os 2 GiB de VRAM que cada daemon pega
    /// numa GPU compartilhada) e o cache de modelos da máquina real, somente
    /// leitura. Os clientes têm de nascer de [`Self::client_command`], que carrega
    /// o mesmo pino — um `Command::new` avulso volta ao estado ambiente.
    #[must_use]
    pub fn start_isolated(tag: &str, pin: ProjectPin, idle_secs: Option<u64>) -> Option<Self> {
        let root = pin.root.clone();
        Self::spawn(tag, Some(&root), idle_secs, Some(pin))
    }

    /// Um cliente (`touring`, `touring-hook`…) já apontado para este daemon e, se
    /// o daemon é isolado, para o mundo dele. `None` se o binário não foi compilado.
    #[must_use]
    pub fn client_command(&self, bin_name: &str) -> Option<Command> {
        let mut cmd = Command::new(locate_binary(bin_name)?);
        cmd.env("TOURING_DAEMON_SOCKET", &self.socket)
            .env("TOURING_DAEMON_SOCK", &self.socket);
        if let Some(pin) = &self.pin {
            pin_client_env(&mut cmd, pin);
        }
        Some(cmd)
    }

    fn spawn(
        tag: &str,
        project_root: Option<&std::path::Path>,
        idle_secs: Option<u64>,
        pin: Option<ProjectPin>,
    ) -> Option<Self> {
        let bin = locate_binary("touring-daemon")?;
        let socket = format!("/tmp/touring-e2e-{tag}-{}.sock", std::process::id());
        let _ = std::fs::remove_file(&socket);
        let _ = std::fs::remove_file(format!("{socket}.lock"));

        let mut cmd = Command::new(&bin);
        cmd.env("TOURING_DAEMON_SOCKET", &socket)
            .env("TOURING_DAEMON_SOCK", &socket)
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(root) = project_root {
            cmd.env("TOURING_PROJECT_ROOT", root).current_dir(root);
        }
        if let Some(secs) = idle_secs {
            cmd.env("TOURING_IDLE_TIMEOUT_SECS", secs.to_string());
        }
        if let Some(pin) = &pin {
            // Mundo fechado: HOME próprio, projeto pinado também no `CLAUDE_PROJECT_DIR`
            // (que a sessão do Claude Code exporta e que vence o `TOURING_PROJECT_ROOT`),
            // embedding em CPU e os pesos do modelo lidos de onde já estão.
            cmd.env("HOME", &pin.home)
                .env("CLAUDE_PROJECT_DIR", &pin.root)
                .env("TOURING_EMBED_DEVICE", "cpu");
            if let Some(cache) = real_fastembed_cache() {
                cmd.env("TOURING_FASTEMBED_CACHE", cache);
            }
        }
        let child = cmd.spawn().ok()?;
        let pid = child.id();
        std::mem::forget(child); // colhido pelo Drop, não pelo handle pai

        // Prontidão medida por IDA E VOLTA real, nunca pela existência do arquivo.
        //
        // Até 07/08/2026 esta espera era `Path::new(&socket).exists()`. O bind cria
        // o arquivo MUITO antes de o daemon conseguir SERVIR (abrir DBs, montar
        // índice), então o teste seguia cedo demais e o cliente recebia
        // `success=false` com payload vazio — o que quebrou
        // `b310_path_wired_when_predictive_blast_injects_symbols` na suíte completa
        // (verde em 7,7s isolado, vermelho em 15-21s sob carga). É exatamente a
        // corrida espúria que a REGRA #19 descreve para o daemon global: socket
        // ligado ≠ daemon pronto.
        //
        // `doctor -j` é o probe canônico e barato; só o veredito `daemon_health ==
        // ok` conta. Enquanto não vier, o daemon não está pronto — por construção,
        // não por tempo de espera arbitrário.
        for _ in 0..200 {
            if std::path::Path::new(&socket).exists() && daemon_answers(&socket, pin.as_ref()) {
                return Some(Self { pid, socket, pin });
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        eprintln!(
            "PrivateDaemon({tag}): daemon não ficou pronto em 10s — teste será pulado, \
             não reprovado (socket={socket})"
        );
        let _ = Command::new("kill").arg(pid.to_string()).output();
        None
    }

    /// Socket deste daemon, para passar a `run_with_stdin`.
    #[must_use]
    pub fn socket(&self) -> &str {
        &self.socket
    }
}

impl Drop for PrivateDaemon {
    fn drop(&mut self) {
        // Sinaliza pelo PID que criamos — jamais por nome (REGRA #19).
        let _ = Command::new("kill").arg(self.pid.to_string()).output();
        let _ = std::fs::remove_file(&self.socket);
        let _ = std::fs::remove_file(format!("{}.lock", self.socket));
    }
}

/// O daemon deste socket já RESPONDE? (não apenas "o socket existe")
///
/// Lê o veredito de `touring doctor -j`: só `daemon_health.status == "ok"` conta.
/// Qualquer outra coisa — erro de conexão, JSON inválido, binário ausente — é
/// "ainda não pronto", que é o que o chamador precisa saber.
/// Paga a montagem do índice ANTES de qualquer teste, fora do orçamento dele.
///
/// Prontidão tem CLASSE DE PESO, e o probe precisa exercitar a mesma que
/// certifica. Em 07/08/2026 a espera saiu de `Path::exists` para `doctor -j`, o
/// que matou a corrida do bind — mas `doctor` é consulta barata, e o que os
/// testes de fato pedem (`pre-task-scout`: blast preditivo sobre o workspace
/// real) é de outra ordem. O daemon respondia `daemon_health == ok` com o índice
/// ainda por montar, e o primeiro teste pesado pagava a montagem DENTRO do seu
/// próprio orçamento de cliente.
///
/// Efeito medido: `b310_path_wired_when_predictive_blast_injects_symbols` verde
/// isolado (14,6s) e vermelho na suíte completa de 04/09/2026, com a máquina
/// saturada por 49 crates — o MESMO teste que motivou a correção de 07/08.
/// O `--timeout` do b310 já subiu 15 → 60 por esta mesma causa, e a falha
/// reincidiu em 60: o histórico do próprio teste mede que ajustar o número não
/// remove o mecanismo.
///
/// A ordem dos testes tampouco serve de garantia — o comentário do b310 conta
/// com "o daemon já foi aquecido pelos outros testes deste binário", mas os
/// cinco começam em paralelo e nada os ordena. Aquecer aqui, uma vez, dentro do
/// `OnceLock`, torna estrutural o que era incidental.
///
/// Best-effort por construção: o valor está no EFEITO COLATERAL (índice
/// montado), nunca na resposta. Falhar aqui não é veredito sobre coisa alguma —
/// seria o falso negativo que o próprio b310 já documenta.
fn warm_heavy_path(socket: &str) {
    const PROBE: &str =
        r#"{"tool_name":"TaskCreate","tool_input":{"subject":"warmup","description":"warmup"}}"#;
    let _ = run_with_stdin(
        "touring",
        &["--timeout", "120", "pre-task-scout"],
        PROBE,
        Some(socket),
    );
}

fn daemon_answers(socket: &str, pin: Option<&ProjectPin>) -> bool {
    let probe = match pin {
        // Daemon isolado: o probe também sai do mundo dele, senão o `doctor` do
        // cliente (cwd/ambiente do processo de teste) tocaria o projeto errado.
        Some(pin) => locate_binary("touring").and_then(|bin| {
            let mut cmd = Command::new(bin);
            cmd.env("TOURING_DAEMON_SOCKET", socket);
            pin_client_env(&mut cmd, pin);
            drive(cmd, &["doctor", "-j"], "")
        }),
        None => run_with_stdin("touring", &["doctor", "-j"], "", Some(socket)),
    };
    let Some((stdout, _stderr, code)) = probe else {
        return false;
    };
    if code != 0 {
        return false;
    }
    serde_json::from_str::<serde_json::Value>(&stdout)
        .ok()
        .and_then(|v| {
            v.as_array().map(|checks| {
                checks.iter().any(|c| {
                    c.get("name").and_then(serde_json::Value::as_str) == Some("daemon_health")
                        && c.get("status").and_then(serde_json::Value::as_str) == Some("ok")
                })
            })
        })
        .unwrap_or(false)
}

/// Roda `bin_name args…` com `stdin_payload` na entrada padrão.
///
/// `socket = Some(..)` aponta o cliente para um [`PrivateDaemon`]; `None` deixa
/// o cliente resolver o daemon normalmente (use apenas quando o teste tem como
/// alvo justamente essa resolução).
#[must_use]
pub fn run_with_stdin(
    bin_name: &str,
    args: &[&str],
    stdin_payload: &str,
    socket: Option<&str>,
) -> Option<(String, String, i32)> {
    let bin = locate_binary(bin_name)?;
    let mut cmd = Command::new(&bin);
    if let Some(sock) = socket {
        cmd.env("TOURING_DAEMON_SOCKET", sock);
    }
    drive(cmd, args, stdin_payload)
}

/// Roda `cmd args…` com `stdin_payload` na entrada padrão e colhe (stdout, stderr, código).
fn drive(mut cmd: Command, args: &[&str], stdin_payload: &str) -> Option<(String, String, i32)> {
    cmd.args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().ok()?;

    if let Some(mut sin) = child.stdin.take() {
        sin.write_all(stdin_payload.as_bytes()).ok()?;
    }
    let out = child.wait_with_output().ok()?;
    Some((
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    ))
}

// ── Daemon compartilhado por PROCESSO de teste (21/08/2026) ──────────────────
//
// Os testes e2e que spawnam `touring <cmd>` herdavam `TOURING_DAEMON_SOCKET` da
// sessão e, em suíte, disputavam o ator single-thread do daemon global com tudo
// o que mais estivesse enfileirado nele (um `index rebuild` de 10-40 min bastava
// para estourar todo probe de 15s). Um daemon por binário de teste, com a raiz
// do workspace pinada e watchdog de ociosidade, remove a disputa por construção.

static SHARED: std::sync::OnceLock<Option<PrivateDaemon>> = std::sync::OnceLock::new();

/// Raiz do workspace (`crates/<este>/` → dois níveis acima).
#[must_use]
pub fn workspace_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map_or_else(
            || std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")),
            std::path::Path::to_path_buf,
        )
}

/// O daemon privado deste processo (um por binário de teste), servindo o
/// workspace real; `None` quando o binário não foi compilado.
#[must_use]
pub fn shared() -> Option<&'static PrivateDaemon> {
    SHARED
        .get_or_init(|| {
            PrivateDaemon::start_in(env!("CARGO_CRATE_NAME"), Some(&workspace_root()), Some(90))
        })
        .as_ref()
}

/// O daemon privado, JÁ AQUECIDO para uma consulta PESADA.
///
/// Existe separado de [`shared`] por uma regressão medida em 04/09/2026, e a
/// separação é o remédio, não um detalhe: o aquecimento estava dentro de
/// `shared()`, que é inicialização preguiçosa e portanto roda no PRIMEIRO
/// chamador, qualquer que ele seja. `predictive_wave_p99_guards` chama
/// `private_daemon_env()` — um getter de aparência barata — DENTRO do laço que
/// cronometra, e a primeira iteração passou a pagar os ~11s do aquecimento:
/// `D2 pre-tool-use CLI P99 = 10952ms, expected < 2_000ms`, determinístico e
/// reproduzível isolado.
///
/// A lição é geral: **custo escondido atrás de acessor preguiçoso é cobrado de
/// quem chegar primeiro** — e quem chega primeiro pode ser justamente um
/// caminho medido. O custo agora é declarado no sítio que precisa dele; quem só
/// quer o socket segue pagando nada.
#[must_use]
pub fn shared_warm() -> Option<&'static PrivateDaemon> {
    // `Once` e não `OnceLock`: o aquecimento não produz valor, só efeito — e
    // precisa acontecer uma vez mesmo que `shared()` já tenha sido inicializado
    // por outro teste do mesmo binário. Declarado ANTES do primeiro statement
    // (clippy::items_after_statements): este arquivo é incluído por caminho em
    // mais de um crate de teste, então o lint dispara em cada um deles.
    static WARMED: std::sync::Once = std::sync::Once::new();
    let daemon = shared()?;
    WARMED.call_once(|| warm_heavy_path(daemon.socket()));
    Some(daemon)
}

/// Variáveis de ambiente que apontam um `touring` spawnado para o daemon
/// privado deste processo — `Command::envs(private_daemon_env())`. Vazio
/// (fail-open, comportamento anterior) se o daemon não subiu.
#[must_use]
pub fn private_daemon_env() -> Vec<(&'static str, String)> {
    match shared() {
        Some(d) => vec![
            ("TOURING_DAEMON_SOCKET", d.socket().to_string()),
            ("TOURING_DAEMON_SOCK", d.socket().to_string()),
        ],
        None => Vec::new(),
    }
}
