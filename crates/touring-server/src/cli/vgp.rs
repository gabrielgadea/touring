//! `touring vgp check` — the VGP layer-1 motor as a pure CLI subcommand (daemonless, like
//! `touring ast polyglot-semantic`): judges ONE proposed file against the project and denies
//! invented Python API before the edit.
//!
//! Contract (juiz_motor_vgp.py, 2026-10-08-vgp-motor-base):
//!   touring vgp check --path <virtual> --content-file <proposto> --project-root <raiz>
//!                     [--disk-file <disco>] [--overlay <orig>=<arquivo>]... [--lang python] [-j]
//!     -> {"verdict": "deny"|"warn"|"allow"|"unknown", "layer": 1, "lang": "python",
//!         "elapsed_ms": N, "findings": [...]}
//!
//! Fail-open by construction: an unreadable content file, a missing argument, or a parse failure
//! still prints a valid JSON with verdict "unknown" — the hook channel never dies on the motor.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use touring_code::vgp::{CheckInput, check};
use touring_pyright::pyright::{L2Resultado, Severidade, Verdict as VerdictL2};

/// The one subcommand. `args` starts at the token after `vgp` (["check", "--path", ...]).
pub fn run(args: &[String]) -> Result<()> {
    println!("{}", serde_json::to_string(&run_json(args))?);
    Ok(())
}

/// Everything returns a CheckOutput — including argument and IO failures (verdict "unknown").
fn run_json(args: &[String]) -> touring_code::vgp::CheckOutput {
    match parse_args(args) {
        Ok((input, camadas)) => {
            let mut out = check(&input);
            if camadas.contains(&2) && input.lang == "python" {
                // a CLI é um processo de confiança: liga o pool (o hook standalone nunca o faz)
                touring_pyright::pool::habilitar_neste_processo();
                let overlays: Vec<(String, String)> = input
                    .overlays
                    .iter()
                    .map(|(origem, texto)| (origem.clone(), texto.clone()))
                    .collect();
                let l2 = touring_pyright::pool::checar(
                    &input.project_root,
                    &input.path,
                    &input.content,
                    input.disk.as_deref(),
                    &overlays,
                );
                fundir_camada2(&mut out, &l2);
            }
            out
        }
        Err(motivo) => touring_code::vgp::CheckOutput {
            verdict: "unknown".into(),
            layer: 1,
            lang: "python".into(),
            elapsed_ms: 0.0,
            findings: vec![touring_code::vgp::Finding::new(
                "erro",
                "argumentos",
                0,
                "warn",
                motivo,
                Vec::new(),
            )],
            camada2: None,
        },
    }
}

/// Merge a layer-2 result into the layer-1 output: findings deduped by (nome, linha) with the
/// STRONGER severity winning, elapsed summed, `layer` raised. An `unknown` layer-2 annotates
/// `camada2` and leaves the layer-1 answer standing — fail-open, never an implicit allow.
fn fundir_camada2(out: &mut touring_code::vgp::CheckOutput, l2: &L2Resultado) {
    out.layer = 2;
    out.elapsed_ms += l2.elapsed_ms as f64;
    if l2.verdict == VerdictL2::Unknown {
        out.camada2 = Some(format!(
            "unknown: {}",
            l2.motivo
                .clone()
                .unwrap_or_else(|| "sem motivo declarado".to_string())
        ));
        return;
    }
    let novos: Vec<touring_code::vgp::Finding> = l2
        .achados
        .iter()
        .map(|a| {
            touring_code::vgp::Finding::new(
                &a.classe,
                &a.nome,
                a.linha as usize,
                match a.severidade {
                    Severidade::Deny => "deny",
                    Severidade::Warn => "warn",
                },
                a.mensagem.clone(),
                Vec::new(),
            )
        })
        .collect();
    touring_code::vgp::sombra::fundir_findings(&mut out.findings, novos);
    out.verdict = touring_code::vgp::checks::verdict_dos(&out.findings, &out.verdict);
}

#[derive(Default)]
struct Opt {
    path: Option<String>,
    content_file: Option<String>,
    root: Option<String>,
    disk_file: Option<String>,
    lang: Option<String>,
    camadas: Option<String>,
}

impl Opt {
    fn set(&mut self, flag: &str, value: String) {
        match flag {
            "--path" => self.path = Some(value),
            "--content-file" => self.content_file = Some(value),
            "--project-root" => self.root = Some(value),
            "--disk-file" => self.disk_file = Some(value),
            "--lang" => self.lang = Some(value),
            "--camadas" => self.camadas = Some(value),
            _ => {}
        }
    }
}

/// "1,2" → [1, 2]; ausente/vazia → [1] (a camada 1 é sempre a base).
fn parse_camadas(raw: &str) -> Vec<u8> {
    let v: Vec<u8> = raw
        .split(',')
        .filter_map(|p| p.trim().parse().ok())
        .collect();
    if v.is_empty() { vec![1] } else { v }
}

fn parse_args(args: &[String]) -> std::result::Result<(CheckInput, Vec<u8>), String> {
    let mut o = Opt::default();
    let mut overlays = HashMap::new();
    // o handler recebe o argv INTEIRO (["touring", "vgp", "check", …]): os flags começam depois
    // do posicional "check" (ou do primeiro token com "--", quando "check" não vem)
    let mut i = args
        .iter()
        .position(|s| s == "check")
        .map(|p| p + 1)
        .or_else(|| args.iter().position(|s| s.starts_with("--")))
        .unwrap_or(args.len());
    while i < args.len() {
        match args[i].as_str() {
            "-j" | "--json" | "--brief" | "--full" => i += 1,
            "--overlay" => {
                let par = args.get(i + 1).ok_or("--overlay sem valor")?;
                let (orig, arq) = par
                    .split_once('=')
                    .ok_or_else(|| "--overlay espera <orig>=<arquivo>".to_string())?;
                overlays.insert(
                    orig.to_string(),
                    std::fs::read_to_string(arq)
                        .map_err(|e| format!("overlay ilegível {arq}: {e}"))?,
                );
                i += 2;
            }
            flag => {
                let v = args
                    .get(i + 1)
                    .ok_or_else(|| format!("flag {flag} sem valor"))?
                    .clone();
                o.set(flag, v);
                i += 2;
            }
        }
    }
    let content_file = o.content_file.ok_or("--content-file é obrigatório")?;
    let content = std::fs::read_to_string(&content_file)
        .map_err(|e| format!("content-file ilegível {content_file}: {e}"))?;
    let disk = match o.disk_file {
        Some(d) => {
            Some(std::fs::read_to_string(&d).map_err(|e| format!("disk-file ilegível {d}: {e}"))?)
        }
        None => None,
    };
    let camadas = o
        .camadas
        .as_deref()
        .map(parse_camadas)
        .unwrap_or_else(|| vec![1]);
    Ok((
        CheckInput {
            path: o.path.unwrap_or_else(|| "<string>".into()),
            content,
            project_root: PathBuf::from(o.root.unwrap_or_else(|| ".".into())),
            disk,
            overlays,
            lang: o.lang.unwrap_or_else(|| "python".into()),
        },
        camadas,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use touring_code::vgp::{CheckOutput, Finding};
    use touring_pyright::pyright::{Escopo, L2Achado};

    fn args(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| s.to_string()).collect()
    }

    fn out_base(verdict: &str, findings: Vec<Finding>) -> CheckOutput {
        CheckOutput {
            verdict: verdict.into(),
            layer: 1,
            lang: "python".into(),
            elapsed_ms: 5.0,
            findings,
            camada2: None,
        }
    }

    #[test]
    fn camada2_desconhecida_anota_e_preserva_a_camada1() {
        let mut out = out_base(
            "warn",
            vec![Finding::new("nome", "x", 1, "warn", "s".into(), Vec::new())],
        );
        let l2 = L2Resultado::unknown("pyright indisponível", 40);
        fundir_camada2(&mut out, &l2);
        assert_eq!(out.layer, 2);
        assert_eq!(
            out.camada2.as_deref(),
            Some("unknown: pyright indisponível")
        );
        assert_eq!(out.findings.len(), 1, "findings da camada 1 ficam de pé");
        assert_eq!(
            out.verdict, "warn",
            "unknown da L2 nunca derruba o veredito da L1"
        );
        assert_eq!(out.elapsed_ms, 45.0);
    }

    #[test]
    fn camada2_deny_escala_o_veredito_e_ensina_a_mensagem() {
        let mut out = out_base("allow", Vec::new());
        let l2 = L2Resultado {
            verdict: VerdictL2::Deny,
            motivo: None,
            achados: vec![L2Achado {
                classe: "atributo".into(),
                nome: "caminho_inventado".into(),
                linha: 3,
                escopo: Escopo::Externo,
                severidade: Severidade::Deny,
                regra: "reportAttributeAccessIssue".into(),
                mensagem: "Cannot access attribute".into(),
            }],
            elapsed_ms: 40,
        };
        fundir_camada2(&mut out, &l2);
        assert_eq!(out.verdict, "deny");
        assert_eq!(out.findings[0].severidade, "deny");
        assert_eq!(out.findings[0].sugestao, "Cannot access attribute");
        assert_eq!(out.elapsed_ms, 45.0);
    }

    #[test]
    fn fusao_por_nome_e_linha_vence_a_severidade_mais_forte() {
        let mut out = out_base(
            "warn",
            vec![Finding::new(
                "atributo",
                "x",
                3,
                "warn",
                "l1".into(),
                Vec::new(),
            )],
        );
        let l2 = L2Resultado {
            verdict: VerdictL2::Deny,
            motivo: None,
            achados: vec![
                L2Achado {
                    classe: "atributo".into(),
                    nome: "x".into(),
                    linha: 3,
                    escopo: Escopo::Projeto,
                    severidade: Severidade::Deny,
                    regra: "r".into(),
                    mensagem: "l2 mais forte".into(),
                },
                L2Achado {
                    classe: "atributo".into(),
                    nome: "y".into(),
                    linha: 7,
                    escopo: Escopo::Projeto,
                    severidade: Severidade::Warn,
                    regra: "r".into(),
                    mensagem: "linha nova entra".into(),
                },
            ],
            elapsed_ms: 10,
        };
        fundir_camada2(&mut out, &l2);
        assert_eq!(out.findings.len(), 2);
        let x = out.findings.iter().find(|f| f.nome == "x").unwrap();
        assert_eq!(x.severidade, "deny");
        assert_eq!(x.sugestao, "l2 mais forte");
        assert!(out.findings.iter().any(|f| f.nome == "y" && f.linha == 7));
    }

    #[test]
    fn parse_camadas_default_e_lista() {
        assert_eq!(parse_camadas("1"), vec![1]);
        assert_eq!(parse_camadas("1,2"), vec![1, 2]);
        assert_eq!(
            parse_camadas(""),
            vec![1],
            "vazia cai no default da camada 1"
        );
        assert_eq!(parse_camadas("lixo"), vec![1]);
    }

    #[test]
    fn chamada_minima_devolve_json_com_verdict() {
        let tmp = std::env::temp_dir().join("vgp-cli-t");
        std::fs::create_dir_all(&tmp).unwrap();
        let f = tmp.join("a.py");
        std::fs::write(&f, "print(cp.PASTA)\n").unwrap();
        let out = run_json(&args(&[
            "touring",
            "vgp",
            "check",
            "--path",
            "/tmp/x.py",
            "--content-file",
            f.to_str().unwrap(),
            "--project-root",
            "/tmp",
            "-j",
        ]));
        assert_eq!(out.verdict, "deny");
        assert!(
            out.findings
                .iter()
                .any(|f| f.nome == "cp" && f.severidade == "deny")
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn content_file_ilegivel_e_unknown_nunca_crasha() {
        let out = run_json(&args(&["check", "--content-file", "/nao/existe.py", "-j"]));
        assert_eq!(out.verdict, "unknown");
    }

    #[test]
    fn overlay_alimenta_a_resolucao_de_modulos() {
        let tmp = std::env::temp_dir().join("vgp-cli-ovl");
        std::fs::create_dir_all(&tmp).unwrap();
        let prog = tmp.join("prog.py");
        std::fs::write(&prog, "from scripts.memoria import g\nprint(g.Recibo)\n").unwrap();
        let ovl = tmp.join("ovl.py");
        // `g` precisa ser nome de topo do módulo: desde o conserto do from-import sem alias o
        // motor checa o nome importado (antes passava em silêncio — o teste codificava o bug)
        std::fs::write(&ovl, "class Recibo:\n    chave: str\ng = 1\n").unwrap();
        let raiz = tmp.join("raiz");
        std::fs::create_dir_all(raiz.join("scripts")).unwrap();
        let out = run_json(&args(&[
            "check",
            "--path",
            "/tmp/x.py",
            "--content-file",
            prog.to_str().unwrap(),
            "--project-root",
            raiz.to_str().unwrap(),
            "--overlay",
            &format!(
                "{}={}",
                raiz.join("scripts/memoria.py").display(),
                ovl.display()
            ),
            "-j",
        ]));
        assert_eq!(out.verdict, "allow");
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
