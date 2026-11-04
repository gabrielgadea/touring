//! Mundo fechado para os testes E2E que rodam `touring doctor -j`.
//!
//! **Por que existe.** O `doctor` lê o estado do projeto do CWD do CLIENTE:
//! `check_project_db`, `check_wiring_diagnostic` e `check_index_generation` abrem
//! `<cwd>/.claude/touring/{symbols,knowledge}.db`. Sob `cargo test` o CWD é o diretório do pacote
//! (`crates/touring-hooks`), cujo `.claude/touring/*.db` é estado de runtime — ignorado pelo git e
//! fora do índice desde 18/10/2026 (decisão D4). Os testes passavam nas máquinas onde esse
//! diretório existia por acidente (um índice antigo, copiado ou deixado por outra execução) e
//! reprovavam num clone limpo: `project_db`, `wiring_diagnostic` e `index_generation` com
//! `status: missing`. A prova de que o produto está ligado e saudável não pode depender do que a
//! máquina guardou.
//!
//! **O que faz.** [`DoctorWorld`] monta um projeto num tempdir, semeia o estado que o `doctor`
//! lê com os PRÓPRIOS construtores do produto (`SymbolStore::new`, `FileKnowledgeDB::new` — o
//! mesmo schema que o daemon cria, nunca um SQLite escrito à mão que envelhece) e sobe um daemon
//! privado fechado nesse mundo ([`PrivateDaemon::start_isolated`]), de modo que o cliente, o
//! daemon, o `HOME` e o CWD apontam para o tempdir e nada toca o daemon vivo do usuário.
//!
//! **Entrada residual do ambiente.** `check_circuit_breaker` lê `/tmp/touring-circuit-<uid>.state`,
//! um caminho fixo que nenhuma variável redireciona; um disjuntor ABERTO na máquina continua
//! reprovando o `doctor` — e é esse o sinal que o teste deve dar.
//!
//! Incluído por caminho (`wave24_hook_integration_e2e`) e por `mod common;` (as demais suítes):
//! em ambos os casos `super::private_daemon` é o irmão com o daemon privado.

use super::private_daemon::{PrivateDaemon, ProjectPin};
use std::path::{Path, PathBuf};

/// Quantas linhas de `wiring_map` a semeadura grava — e, portanto, o que o `doctor` deve contar.
pub const SEEDED_WIRING_ROWS: u64 = 1;

/// Cria `<root>/.claude/touring/{symbols,knowledge}.db` com o schema do produto e UMA linha de
/// wiring (um símbolo público sem consumidor).
///
/// **Por que a linha.** Os dois bancos também nascem dentro do daemon quando uma requisição de
/// hook instancia o runtime do projeto (`HookRuntime::new`: `FileKnowledgeDB::new` e
/// `SymbolStore::new`, `hook_runtime.rs`); a sonda `doctor` sozinha não o instancia (medido: sem
/// a semeadura, `project_db` reporta `missing`), mas um mundo que passasse a exercitar hooks
/// passaria em `project_db` sem semeadura nenhuma — «o arquivo existe» não prova a semeadura.
/// A contagem do `doctor` prova: `wiring_diagnostic` reporta `rows=1` só se leu ESTE
/// `knowledge.db` semeado; um banco recém-criado reporta `rows=0`, e o do repositório, milhares.
pub fn seed_project_state(root: &Path) {
    let dir = root.join(".claude").join("touring");
    std::fs::create_dir_all(&dir).expect("create <root>/.claude/touring");
    drop(
        touring_code::ast::store::SymbolStore::new(&dir.join("symbols.db"))
            .expect("create symbols.db with the product's constructor"),
    );
    let knowledge = touring_hooks::knowledge::FileKnowledgeDB::new(&dir.join("knowledge.db"))
        .expect("create knowledge.db with the product's constructor");
    knowledge
        .register_pub_symbol(
            "crates/seeded/src/lib.rs",
            "SeededSymbol",
            "struct",
            "public",
        )
        .expect("register the seeded wiring row");
    let orphans = knowledge.orphan_symbols().expect("query the seeded row");
    assert_eq!(
        orphans.len() as u64,
        SEEDED_WIRING_ROWS,
        "the seeded wiring row must pass the write gate: {orphans:?}"
    );
    drop(knowledge);
}

/// O resultado de um `touring doctor -j` dentro do mundo.
pub struct DoctorRun {
    /// Código de saída do cliente (`-1` se terminou por sinal).
    pub code: i32,
    /// O array JSON de checks, na ordem em que o `doctor` os emite.
    pub checks: Vec<serde_json::Value>,
}

/// Um projeto semeado + um daemon privado fechado nele. O `Drop` encerra o daemon pelo PID que
/// este teste criou (REGRA #19) antes de o diretório que ele serve desaparecer.
pub struct DoctorWorld {
    // `daemon` ANTES de `_dir`: os campos caem na ordem de declaração.
    daemon: PrivateDaemon,
    _dir: tempfile::TempDir,
    root: PathBuf,
}

impl DoctorWorld {
    /// `None` quando os binários `touring`/`touring-daemon` não foram compilados ou o daemon não
    /// ficou pronto: um SKIP anunciado no stderr pelo chamador, nunca um passe que pareça prova.
    #[must_use]
    pub fn start(tag: &str) -> Option<Self> {
        let dir = tempfile::Builder::new()
            .prefix("touring-e2edoctor-")
            .tempdir()
            .expect("create the hermetic temp dir");
        let root = dir.path().join("project");
        let home = dir.path().join("home");
        std::fs::create_dir_all(&root).expect("create the hermetic project root");
        std::fs::create_dir_all(&home).expect("create the hermetic HOME");
        // O estado entra ANTES do daemon subir: ele o abre no boot.
        seed_project_state(&root);
        let daemon = PrivateDaemon::start_isolated(
            tag,
            ProjectPin {
                root: root.clone(),
                home,
            },
            Some(90),
        )?;
        Some(Self {
            daemon,
            _dir: dir,
            root,
        })
    }

    /// Raiz do projeto deste mundo (CWD dos clientes).
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Roda `touring doctor -j` pinado a este mundo. Falha ALTO se o cliente não emitir um array
    /// JSON: dentro de um mundo inteiramente nosso isso é defeito, não «lacuna de infraestrutura».
    #[must_use]
    pub fn doctor(&self) -> DoctorRun {
        let mut cmd = self
            .daemon
            .client_command("touring")
            .expect("`touring` binary vanished after the daemon started");
        // O daemon deste mundo embute em CPU (`start_isolated`); o cliente declara a mesma
        // política, senão o `gpu_embeddings` lê «CPU sem falha de CUDA» como `degraded` numa
        // máquina com GPU — uma verdade sobre a configuração do teste, não sobre o produto.
        let out = cmd
            .env("TOURING_EMBED_DEVICE", "cpu")
            .args(["doctor", "-j"])
            .output()
            .expect("spawn `touring doctor -j`");
        let code = out.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        let json: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_else(|e| {
            panic!(
                "`doctor -j` emitted no JSON ({e}); exit {code}\nstdout: {stdout}\nstderr: {stderr}"
            )
        });
        let checks = json
            .as_array()
            .unwrap_or_else(|| panic!("`doctor -j` must return an array, got: {json}"))
            .clone();
        DoctorRun { code, checks }
    }

    /// Prova que o `doctor` leu o estado DESTE mundo e não o de outro diretório. Duas provas, e a
    /// segunda é a que carrega a semeadura:
    ///
    /// 1. `project_db` é `ok` e o caminho que ele relata mora sob a raiz do tempdir;
    /// 2. `wiring_diagnostic` conta exatamente [`SEEDED_WIRING_ROWS`] linha(s). O daemon cria os
    ///    bancos sozinho, então a existência do arquivo (1) não distingue um mundo semeado de um
    ///    vazio; a contagem distingue (vazio: `rows=0`; banco do repositório: milhares).
    pub fn assert_state_was_read_from_this_world(&self, checks: &[serde_json::Value]) {
        let check = |name: &str| {
            checks
                .iter()
                .find(|c| c.get("name").and_then(serde_json::Value::as_str) == Some(name))
                .unwrap_or_else(|| panic!("`doctor -j` has no {name} check: {checks:?}"))
        };
        let field = |c: &serde_json::Value, key: &str| {
            c.get(key)
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string()
        };

        let project_db = check("project_db");
        assert_eq!(
            field(project_db, "status"),
            "ok",
            "project_db must see the seeded symbols.db: {project_db}"
        );
        let detail = field(project_db, "detail");
        assert!(
            detail.starts_with(&self.root.to_string_lossy().into_owned()),
            "project_db read {detail:?}, not a database under this world's root {}",
            self.root.display()
        );

        let wiring = check("wiring_diagnostic");
        let wiring_detail = field(wiring, "detail");
        let rows = wiring_detail
            .split_whitespace()
            .find_map(|word| word.strip_prefix("rows="));
        assert_eq!(
            rows,
            Some(SEEDED_WIRING_ROWS.to_string().as_str()),
            "wiring_diagnostic must count exactly the seeded wiring row of this world \
             (an unseeded world reads rows=0, the repository's database thousands): {wiring}"
        );
    }
}
