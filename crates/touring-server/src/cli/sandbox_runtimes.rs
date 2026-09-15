//! `touring sandbox-runtimes` — preflight e provisionamento dos runtimes do
//! sandbox CEG (RUN-1, cross-audit 27/08).
//!
//! Dois subcomandos:
//! - `status`: para cada linguagem anunciada, o runtime RESOLVIDO ou AUSENTE
//!   (fail-LOUD na apresentação — o last-resort do resolver esconderia a
//!   ausência e um spawn morreria ENOENT genérico).
//! - `setup-venv`: cria/atualiza o venv gerenciado em
//!   `~/.claude/touring/sandbox-venv` com as bibliotecas de agente
//!   (pandas/pydantic/httpx). Roda no HOST (a rede do sandbox é deny-all no
//!   kernel); o sandbox o monta via PYTHONPATH, read-only por construção (fora
//!   dos write roots do Landlock).

use anyhow::{Context, bail};
use std::path::PathBuf;
use touring_ceg::gateway::sandbox_executor::detect_language_runtimes;

fn venv_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".claude/touring/sandbox-venv"))
}

/// Pacotes de agente provisionados no venv gerenciado. Sem rede no sandbox,
/// este é o único canal — e é por isso que o setup roda no host.
const VENV_PACKAGES: &[&str] = &["pandas", "pydantic", "httpx"];

/// Handler do subcomando `touring sandbox-runtimes` (`status` | `setup-venv`).
pub fn sandbox_runtimes(args: &[String]) -> anyhow::Result<()> {
    // args = [<bin>, "sandbox-runtimes", ...rest] (padrão da command_table —
    // ver run.rs). Anclamos no nome do comando, não numa posição fixa.
    let rest: Vec<&str> = args
        .iter()
        .map(String::as_str)
        .skip_while(|a| *a != "sandbox-runtimes")
        .skip(1)
        .collect();
    let json = rest.iter().any(|a| *a == "--json" || *a == "-j");
    match rest.first() {
        None | Some(&"status") => status(json),
        Some(&"setup-venv") => setup_venv(),
        Some(other) => {
            bail!("sandbox-runtimes: subcomando desconhecido '{other}' (status|setup-venv)")
        }
    }
}

fn status(json: bool) -> anyhow::Result<()> {
    let rows = detect_language_runtimes();
    let presentes = rows.iter().filter(|(_, _, r)| r.is_some()).count();
    if json {
        let venv = venv_dir().map(|d| d.join("lib"));
        let venv_ok = venv.as_ref().is_some_and(|lib| {
            std::fs::read_dir(lib).is_ok_and(|mut rd| {
                rd.any(|e| e.is_ok_and(|e| e.path().join("site-packages").is_dir()))
            })
        });
        let out = serde_json::json!({
            "runtimes": rows.iter().map(|(nome, _lang, rt)| {
                serde_json::json!({"language": nome, "runtime": rt.as_ref().map(|p| p.display().to_string())})
            }).collect::<Vec<_>>(),
            "present": presentes,
            "total": rows.len(),
            "sandbox_venv": if venv_ok { "presente" } else { "ausente (rode: touring sandbox-runtimes setup-venv)" },
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }
    println!("touring sandbox-runtimes — preflight dos runtimes do sandbox CEG\n");
    for (nome, _lang, rt) in &rows {
        match rt {
            Some(p) => println!("  {nome:<8} OK       {}", p.display()),
            None => println!("  {nome:<8} AUSENTE  (nenhum candidato no PATH)"),
        }
    }
    let venv = venv_dir().map(|d| d.join("lib"));
    let venv_ok = venv.as_ref().is_some_and(|lib| {
        std::fs::read_dir(lib).is_ok_and(|mut rd| {
            rd.any(|e| e.is_ok_and(|e| e.path().join("site-packages").is_dir()))
        })
    });
    println!();
    if venv_ok {
        println!(
            "  venv     OK       {}",
            venv_dir().unwrap_or_default().display()
        );
    } else {
        println!("  venv     AUSENTE  (rode: touring sandbox-runtimes setup-venv)");
    }
    println!(
        "\n{presentes}/{} linguagens com runtime resolvido",
        rows.len()
    );
    Ok(())
}

fn setup_venv() -> anyhow::Result<()> {
    let dir = venv_dir().context("HOME ausente")?;
    let python = "python3";
    if !dir.join("bin/python").exists() {
        println!("criando venv em {}…", dir.display());
        let st = std::process::Command::new(python)
            .args(["-m", "venv", &dir.display().to_string()])
            .status()
            .context("python3 -m venv")?;
        if !st.success() {
            bail!("python3 -m venv falhou ({st})");
        }
    }
    let pip = dir.join("bin/pip");
    println!(
        "instalando/atualizando {} (rede do HOST)…",
        VENV_PACKAGES.join(", ")
    );
    let out = std::process::Command::new(&pip)
        .args(["install", "--quiet", "--disable-pip-version-check"])
        .args(VENV_PACKAGES)
        .output()
        .context("pip install")?;
    if !out.status.success() {
        eprintln!("{}", String::from_utf8_lossy(&out.stderr));
        bail!("pip install falhou ({})", out.status);
    }
    // Relatório honesto: o que de fato importa na versão do python do sandbox.
    let report = std::process::Command::new(dir.join("bin/python"))
        .args([
            "-c",
            "import importlib,sys;print('python',sys.version.split()[0]);\
             [print(m, getattr(importlib.import_module(m),'__version__','?')) for m in ['pandas','pydantic','httpx']]",
        ])
        .output()
        .context("verificação pós-instalação")?;
    print!("{}", String::from_utf8_lossy(&report.stdout));
    if !report.status.success() {
        eprintln!("{}", String::from_utf8_lossy(&report.stderr));
        bail!("verificação pós-instalação falhou");
    }
    println!("venv pronto; o sandbox o monta via PYTHONPATH (read-only por Landlock).");
    Ok(())
}
