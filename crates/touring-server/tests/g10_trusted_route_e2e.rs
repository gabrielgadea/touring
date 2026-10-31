//! G10ROTA — o comando que o deny do G10 nomeia EXECUTA e FAZ a ação (D8: o texto promete, o
//! executor cumpre).
//!
//! Caso real (analise-78, transcrição `05166acc-ff08-4644-8481-ab8c2316c4a4`, linhas 5786-5798,
//! 09/10/2026 22:39 BRT): a 5ª `python-inline` seriada ESCREVIA `RETOMADA.md`. O G10 nomeou
//! `touring run --lang python --code …`; o perfil Sandboxed nega `write_text` (X6 `file-write`) e
//! a rota falha. A que funcionou foi `TOURING_TRUSTED_OK=1 touring run --allow-forbidden …`.
//!
//! Este teste fecha o laço pelas duas pontas, com o hook REAL (`cli_suggester::run`) e o binário
//! `touring` REAL: (1) a rota antiga FALHA e não altera o arquivo (a premissa do defeito); (2) o
//! comando que o deny nomeia EXECUTA e altera o arquivo; (3) para um destino fora do workspace o
//! deny não promete rota — e a melhor candidata, executada, de fato NÃO escreve ali.

use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};
use touring_hooks::cli_suggester;
use touring_hooks::runtime::HookRuntime;

const TOURING: &str = env!("CARGO_BIN_EXE_touring");
const RETOMADA_REL: &str = "docs/plans/2026-10-06-geopolitica-logistica-eua/RETOMADA.md";

/// O comando NEGADO do caso real, verbatim (transcrição 05166acc, linha 5786).
const COMANDO_REAL: &str = "TOURING_CODE_MODE=native .venv/bin/python3 - <<'EOF'\nimport pathlib\np = pathlib.Path(\"docs/plans/2026-10-06-geopolitica-logistica-eua/RETOMADA.md\")\nt = p.read_text(encoding=\"utf-8\")\nvelho = \"> Estado 10/10 ~00:30:\"\nassert t.count(velho) == 1\np.write_text(t.replace(velho, \"> Estado 09/10 22:38 (o relógio da máquina; eu tinha escrito «10/10 ~00:30» — errado):\"), encoding=\"utf-8\")\nprint(\"ok\")\nEOF";

fn projeto() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir_all(tmp.path().join(".claude/data")).expect("mkdir .claude/data");
    std::fs::create_dir_all(tmp.path().join(".touring")).expect("mkdir .touring");
    std::fs::write(
        tmp.path().join(".touring/touring.toml"),
        "[code_mode]\nmode = \"code\"\n",
    )
    .expect("touring.toml");
    let doc = tmp.path().join(RETOMADA_REL);
    std::fs::create_dir_all(doc.parent().expect("pai")).expect("mkdir docs");
    std::fs::write(&doc, "> Estado 10/10 ~00:30: pendente\n").expect("RETOMADA.md");
    tmp
}

fn hook(rt: &HookRuntime, sessao: &str, cmd: &str) -> String {
    cli_suggester::run(
        rt,
        &json!({
            "tool_name": "Bash",
            "session_id": sessao,
            "tool_input": { "command": cmd }
        }),
    )
}

fn razao_do_deny(saida: &str) -> Option<String> {
    let v: Value = serde_json::from_str(saida).ok()?;
    let h = v.get("hookSpecificOutput")?;
    if h.get("permissionDecision")?.as_str() != Some("deny") {
        return None;
    }
    h.get("permissionDecisionReason")?
        .as_str()
        .map(str::to_owned)
}

/// As 4 chamadas que antecedem a 5ª na janela de 600s: escritores python do mesmo documento.
fn rajada_ate_o_deny(rt: &HookRuntime, sessao: &str, quinta: &str) -> Option<String> {
    for i in 1..=4 {
        let c = format!(
            ".venv/bin/python3 -c 'import pathlib; p = pathlib.Path(\"{RETOMADA_REL}\"); p.write_text(p.read_text() + \"# passo {i}\")'"
        );
        assert!(
            razao_do_deny(&hook(rt, sessao, &c)).is_none(),
            "as 4 primeiras passam: {c}"
        );
    }
    razao_do_deny(&hook(rt, sessao, quinta))
}

/// O comando que o deny nomeia: do primeiro `TOURING_TRUSTED_OK=1 touring run` até o fim do
/// programa (que termina antes da frase que o G10 sempre acrescenta).
fn comando_trusted_nomeado(razao: &str) -> String {
    let ini = razao
        .find("TOURING_TRUSTED_OK=1 touring run")
        .unwrap_or_else(|| panic!("o deny não nomeia a rota Trusted: {razao}"));
    let fim = razao[ini..]
        .find("\nSó o comando negado roda")
        .map_or(razao.len(), |i| ini + i);
    razao[ini..fim].trim_end().to_string()
}

/// Roda `comando` num shell, com `touring` no PATH, dentro do projeto e SEM daemon: o socket
/// aponta para um arquivo que não existe (o cliente falha aberto, sem autostart).
fn roda(projeto: &Path, comando: &str) -> (i32, String) {
    let bin_dir = Path::new(TOURING).parent().expect("dir do binário");
    let path = format!(
        "{}:{}",
        bin_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    // T-2 (31/10/2026): HOME TEMPORÁRIO.
    //
    // O `touring run` journala em `$HOME/.claude/touring/run_journal.jsonl` — o
    // caminho vem do HOME (não existe variável de ambiente para o path do
    // journal), então sem isto o e2e escreve no journal REAL do usuário. O
    // `TOURING_REPEAT_GATE=off` (28/10, `b6f747c8`) só desligava o gate de
    // repetição: o REGISTRO continuava indo para o arquivo de verdade.
    let home = projeto.join("home-temp");
    std::fs::create_dir_all(&home).expect("HOME temporário");
    let out = Command::new("bash")
        .arg("-c")
        .arg(comando)
        .current_dir(projeto)
        .env("HOME", &home)
        .env("PATH", path)
        .env("TOURING_DAEMON_SOCKET", projeto.join("sem-daemon.sock"))
        .env("TOURING_DAEMON_SOCK", projeto.join("sem-daemon.sock"))
        .env_remove("CLAUDE_PROJECT_DIR")
        .env_remove("TOURING_PROJECT_ROOT")
        // Hermetic against the user's run journal (28/10/2026): the P2-v2 repeat gate reads
        // `$HOME/.claude/touring/run_journal.jsonl`, so once ANY earlier green run of this
        // very test journaled the program, every later run was `repeat_blocked` (exit 2) and
        // the named route looked broken. This test is about the G10 route; the repeat gate
        // has its own suite. `off` keeps it out of the way without touching the command.
        .env("TOURING_REPEAT_GATE", "off")
        .output()
        .expect("bash");
    (
        out.status.code().unwrap_or(-1),
        format!(
            "{}\n{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        ),
    )
}

fn corpo_python_do_comando_real() -> &'static str {
    let ini = COMANDO_REAL.find("import pathlib").expect("corpo");
    let fim = COMANDO_REAL.rfind("\nEOF").expect("fim do heredoc");
    &COMANDO_REAL[ini..fim]
}

#[test]
fn o_comando_que_o_deny_do_g10_nomeia_executa_e_escreve() {
    let tmp = projeto();
    let rt = HookRuntime::new(tmp.path()).expect("hook runtime");
    let doc = tmp.path().join(RETOMADA_REL);

    // (1) A premissa do defeito: a rota Sandboxed (a que o G10 nomeava) FALHA e não escreve.
    let antiga = format!(
        "touring run --lang python --code '{}'",
        corpo_python_do_comando_real().replace('\'', "'\\''")
    );
    let (rc_antiga, saida_antiga) = roda(tmp.path(), &antiga);
    assert!(
        saida_antiga.contains("file-write") || rc_antiga != 0,
        "a rota Sandboxed devia ser recusada pelo X6: rc={rc_antiga} {saida_antiga}"
    );
    assert_eq!(
        std::fs::read_to_string(&doc).expect("lê"),
        "> Estado 10/10 ~00:30: pendente\n",
        "a rota antiga não escreveu nada"
    );

    // (2) O deny real nomeia a rota Trusted — e ELA executa e faz a ação.
    let razao = rajada_ate_o_deny(&rt, "g97-e2e-g10", COMANDO_REAL).expect("a 5ª nega");
    let nomeado = comando_trusted_nomeado(&razao);
    let (rc, saida) = roda(tmp.path(), &nomeado);
    assert_eq!(
        rc, 0,
        "o comando nomeado executa: {saida}\ncomando: {nomeado}"
    );
    let depois = std::fs::read_to_string(&doc).expect("lê");
    assert!(
        depois.contains(
            "> Estado 09/10 22:38 (o relógio da máquina; eu tinha escrito «10/10 ~00:30» — errado):"
        ),
        "o comando nomeado FEZ a ação (a substituição do caso real): {depois}"
    );
}

/// T-2 (31/10/2026) — o e2e NÃO pode escrever no journal real.
///
/// O `touring run` journala em `$HOME/.claude/touring/run_journal.jsonl`, e o
/// caminho vem do HOME (não existe variável de ambiente para ele). Com o HOME
/// real herdado, cada rodada do e2e escrevia no journal do usuário — e o
/// `TOURING_REPEAT_GATE=off` (28/10, `b6f747c8`) só desligava o gate, não a
/// escrita.
///
/// A prova é de CONTEÚDO, não de mtime: o journal real é COMPARTILHADO (outra
/// sessão pode escrever nele no mesmo instante), então comparar mtime/tamanho
/// seria instável. O que tem de ser verdade é: as linhas do NOSSO run estão no
/// journal TEMPORÁRIO e NENHUMA delas no journal real.
#[test]
fn o_e2e_nao_escreve_no_journal_real() {
    let tmp = projeto();
    let journal_real = std::env::var("HOME")
        .map(|h| Path::new(&h).join(".claude/touring/run_journal.jsonl"))
        .unwrap_or_else(|_| Path::new("/nonexistent/run_journal.jsonl").to_path_buf());
    let antes = std::fs::read(&journal_real).unwrap_or_default();
    let meta_antes = std::fs::metadata(&journal_real)
        .ok()
        .map(|m| (m.len(), m.modified().ok()));

    // O `run_id` do registro é único por execução, então cada linha nossa é
    // única — a prova por conteúdo não pode acusar o journal real por
    // coincidência (dois runs do MESMO programa, no mesmo segundo, com a mesma
    // duração, ainda teriam `run_id` diferente).
    let (rc, saida) = roda(tmp.path(), "touring run --lang python --code 'print(1)'");
    assert_eq!(rc, 0, "o run tem de passar: {saida}");

    // (1) o journal TEMPORÁRIO recebeu o registro — o mecanismo está em jogo.
    let temp = std::fs::read_to_string(
        tmp.path()
            .join("home-temp/.claude/touring/run_journal.jsonl"),
    )
    .expect("o journal temporário recebeu o registro");
    assert!(
        temp.lines().any(|l| !l.trim().is_empty()),
        "o run não journalou no HOME temporário"
    );

    // (2) NENHUMA das nossas linhas está no journal real.
    let depois = std::fs::read(&journal_real).unwrap_or_default();
    for linha in temp.lines().filter(|l| !l.trim().is_empty()) {
        assert!(
            !depois.windows(linha.len()).any(|w| w == linha.as_bytes()),
            "o e2e escreveu no journal real: {linha}"
        );
    }

    // (3) os números pedidos (tamanho/mtime), para o relatório.
    let meta_depois = std::fs::metadata(&journal_real)
        .ok()
        .map(|m| (m.len(), m.modified().ok()));
    eprintln!(
        "journal real: bytes {} -> {} | mtime {:?} -> {:?}",
        antes.len(),
        depois.len(),
        meta_antes.map(|m| m.1),
        meta_depois.map(|m| m.1),
    );
}

#[test]
fn destino_fora_do_workspace_nenhuma_rota_escreve_de_fato() {
    let tmp = projeto();
    let rt = HookRuntime::new(tmp.path()).expect("hook runtime");
    let fora = tempfile::tempdir().expect("tempdir de fora");
    let destino = fora.path().join("saida.md");
    let corpo = format!(
        "import pathlib; pathlib.Path(\"{}\").write_text(\"x\")",
        destino.display()
    );
    let cmd = format!(".venv/bin/python3 -c '{corpo}'");

    // O deny não promete a rota Trusted…
    let razao = rajada_ate_o_deny(&rt, "g97-e2e-g10-fora", &cmd).expect("a 5ª nega");
    assert!(razao.contains("nenhuma rota"), "{razao}");
    assert!(
        !razao.contains("TOURING_TRUSTED_OK=1 touring run"),
        "{razao}"
    );

    // …e a sua premissa é verdadeira: a melhor candidata, executada, NÃO escreve fora.
    let melhor = format!(
        "TOURING_TRUSTED_OK=1 touring run --allow-forbidden --lang python --code '{corpo}'"
    );
    let (_rc, saida) = roda(tmp.path(), &melhor);
    assert!(
        !destino.exists(),
        "a rota Trusted escreveu FORA do workspace — o Landlock não confina e o deny mente: {saida}"
    );
}
