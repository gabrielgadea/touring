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
    let out = Command::new("bash")
        .arg("-c")
        .arg(comando)
        .current_dir(projeto)
        .env("PATH", path)
        .env("TOURING_DAEMON_SOCKET", projeto.join("sem-daemon.sock"))
        .env("TOURING_DAEMON_SOCK", projeto.join("sem-daemon.sock"))
        .env_remove("CLAUDE_PROJECT_DIR")
        .env_remove("TOURING_PROJECT_ROOT")
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
