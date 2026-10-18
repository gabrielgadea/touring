//! `touring vgp check` — the VGP layer-1 motor as a pure CLI subcommand (daemonless, like
//! `touring ast polyglot-semantic`): judges ONE proposed file against the project and denies
//! invented Python API before the edit.
//!
//! Contract (juiz_motor_vgp.py, 2026-10-08-vgp-motor-base):
//!   touring vgp check --path <virtual> --content-file <proposto> --project-root <raiz>
//!                     [--disk-file <disco>] [--overlay <orig>=<arquivo>]... [--lang python]
//!                     [--contexto escrita|execucao] [-j]
//!     -> {"verdict": "deny"|"warn"|"allow"|"unknown", "layer": 1, "lang": "python",
//!         "elapsed_ms": N, "findings": [...]}
//!
//! Fail-open by construction: an unreadable content file, a missing argument, or a parse failure
//! still prints a valid JSON with verdict "unknown" — the hook channel never dies on the motor.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use touring_code::vgp::sombra::camada2_ligada;
use touring_code::vgp::{CheckInput, Contexto, check_em, lang_do_caminho};
use touring_pyright::pyright::{L2Resultado, Severidade, Verdict as VerdictL2};

/// The one subcommand. `args` starts at the token after `vgp` (["check", "--path", ...]).
pub fn run(args: &[String]) -> Result<()> {
    println!("{}", serde_json::to_string(&run_json(args))?);
    Ok(())
}

/// Everything returns a CheckOutput — including argument and IO failures (verdict "unknown").
fn run_json(args: &[String]) -> touring_code::vgp::CheckOutput {
    match parse_args(args) {
        Ok((input, camadas, contexto)) => {
            let mut out = check_em(&input, contexto);
            if camadas.contains(&2) {
                camada2(&mut out, &input, contexto);
            }
            out
        }
        Err(recusa) => touring_code::vgp::CheckOutput {
            verdict: "unknown".into(),
            layer: 1,
            // a linguagem do PEDIDO (flag, senão extensão), nunca um `python` fixo: um `.html`
            // que o motor não consegue ler não pode voltar etiquetado como outra linguagem
            lang: recusa.lang,
            elapsed_ms: 0.0,
            findings: if recusa.nao_utf8 {
                Vec::new()
            } else {
                vec![touring_code::vgp::Finding::new(
                    "erro",
                    "argumentos",
                    0,
                    "warn",
                    recusa.motivo.clone(),
                    Vec::new(),
                )]
            },
            camada2: None,
            motivo: Some(recusa.motivo),
        },
    }
}

/// Layer 2 for the input's language (`pool::checar_lang`: pyright, rust-analyzer, tsserver or deno
/// lsp), only where the project turned it on — Python by default, every other language through
/// `[vgp] camada2_langs`. Asked for and off, `camada2` says so: a layer that did not run must never
/// read as a layer that found nothing.
fn camada2(out: &mut touring_code::vgp::CheckOutput, input: &CheckInput, contexto: Contexto) {
    if !camada2_ligada(&input.lang, &input.project_root) {
        out.camada2 = Some(format!(
            "desligada: camada 2 não está ligada para `{}` neste projeto ([vgp] camada2_langs)",
            input.lang
        ));
        return;
    }
    // a CLI é um processo de confiança: liga o pool (o hook standalone nunca o faz)
    touring_pyright::pool::habilitar_neste_processo();
    let overlays: Vec<(String, String)> = input
        .overlays
        .iter()
        .map(|(origem, texto)| (origem.clone(), texto.clone()))
        .collect();
    let mut l2 = touring_pyright::pool::checar_lang(
        &input.lang,
        &input.project_root,
        &input.path,
        &input.content,
        input.disk.as_deref(),
        &overlays,
    );
    no_momento(&mut l2, input, contexto);
    fundir_camada2(out, &l2);
}

/// The severities of the moment the verdict is asked at — the layer-1 rule, applied to layer 2: a
/// fragment WRITTEN under /tmp can still be completed by the next edit; the same file asked at
/// EXECUTION keeps the /tmp bucket's verdict.
fn no_momento(l2: &mut L2Resultado, input: &CheckInput, contexto: Contexto) {
    if contexto == Contexto::Escrita {
        touring_pyright::achado::rebaixar_tmp_na_escrita(
            &mut l2.achados,
            &input.path,
            &input.project_root,
        );
    }
}

/// Why [`parse_args`] gave up on building a [`CheckInput`]. `lang` is what the file WOULD have
/// been judged as; `nao_utf8` marks a file that exists but is not text: that is "could not
/// measure" (`unknown` with a reason), not a bad argument, so it carries no `erro` finding —
/// a `warn` here would be counted as noise against a clean file.
#[derive(Debug)]
struct Recusa {
    motivo: String,
    lang: String,
    nao_utf8: bool,
}

impl Recusa {
    fn argumento(motivo: impl Into<String>, lang: &str) -> Self {
        Self {
            motivo: motivo.into(),
            lang: lang.into(),
            nao_utf8: false,
        }
    }
}

/// Read `arq` as text; `rotulo` names the flag in the reason. Non-UTF-8 content is its own
/// refusal (`InvalidData`), distinct from an IO failure.
fn ler_texto(arq: &str, rotulo: &str, lang: &str) -> std::result::Result<String, Recusa> {
    std::fs::read_to_string(arq).map_err(|e| {
        if e.kind() == std::io::ErrorKind::InvalidData {
            Recusa {
                motivo: format!("conteúdo não é UTF-8: {rotulo} {arq}"),
                lang: lang.into(),
                nao_utf8: true,
            }
        } else {
            Recusa::argumento(format!("{rotulo} ilegível {arq}: {e}"), lang)
        }
    })
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
            // the receiver class the checker named (P0B-d): `fundir_findings` compares it with
            // layer 1's closed type to corroborate
            .com_receptor(touring_pyright::pyright::classe_receptora(&a.mensagem))
        })
        .collect();
    touring_code::vgp::sombra::fundir_findings(&mut out.findings, novos);
    out.verdict = touring_code::vgp::politica::verdict_dos(&out.findings, &out.verdict);
}

#[derive(Default)]
struct Opt {
    path: Option<String>,
    content_file: Option<String>,
    root: Option<String>,
    disk_file: Option<String>,
    lang: Option<String>,
    camadas: Option<String>,
    contexto: Option<String>,
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
            "--contexto" => self.contexto = Some(value),
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

/// VL0 §2: `--lang` explícito vence; senão a linguagem sai do caminho (fonte única,
/// `lang_do_caminho`); sem extensão reconhecível (inline `<string>`, script sem sufixo) o
/// default segue python, como antes da VL0.
fn lang_efetiva(explicita: Option<String>, path: &str) -> String {
    explicita
        .or_else(|| lang_do_caminho(path).map(str::to_string))
        .unwrap_or_else(|| "python".into())
}

fn parse_args(args: &[String]) -> std::result::Result<(CheckInput, Vec<u8>, Contexto), Recusa> {
    let mut o = Opt::default();
    // sem flags lidos ainda a linguagem do pedido é a default (a mesma de `lang_efetiva`)
    let padrao = lang_efetiva(None, "<string>");
    // lidos só depois de a linguagem ser conhecida: toda recusa a leva consigo
    let mut overlays_pedidos: Vec<(String, String)> = Vec::new();
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
                let par = args
                    .get(i + 1)
                    .ok_or_else(|| Recusa::argumento("--overlay sem valor", &padrao))?;
                let (orig, arq) = par.split_once('=').ok_or_else(|| {
                    Recusa::argumento("--overlay espera <orig>=<arquivo>", &padrao)
                })?;
                overlays_pedidos.push((orig.to_string(), arq.to_string()));
                i += 2;
            }
            flag => {
                let v = args
                    .get(i + 1)
                    .ok_or_else(|| Recusa::argumento(format!("flag {flag} sem valor"), &padrao))?
                    .clone();
                o.set(flag, v);
                i += 2;
            }
        }
    }
    // a linguagem sai do pedido (flag, senão extensão) ANTES de qualquer leitura de conteúdo
    let path = o.path.unwrap_or_else(|| "<string>".into());
    let lang = lang_efetiva(o.lang, &path);
    // QUANDO o veredito é pedido (default: a escrita — o que os hooks de Write/Edit respondem)
    let contexto = match o.contexto.as_deref() {
        None => Contexto::default(),
        Some(raw) => Contexto::parse(raw).ok_or_else(|| {
            Recusa::argumento(
                format!("--contexto espera `escrita` ou `execucao`, recebeu `{raw}`"),
                &lang,
            )
        })?,
    };
    let content_file = o
        .content_file
        .ok_or_else(|| Recusa::argumento("--content-file é obrigatório", &lang))?;
    let content = ler_texto(&content_file, "content-file", &lang)?;
    let disk = o
        .disk_file
        .map(|d| ler_texto(&d, "disk-file", &lang))
        .transpose()?;
    let mut overlays = HashMap::new();
    for (orig, arq) in overlays_pedidos {
        overlays.insert(orig, ler_texto(&arq, "overlay", &lang)?);
    }
    let camadas = o
        .camadas
        .as_deref()
        .map(parse_camadas)
        .unwrap_or_else(|| vec![1]);
    Ok((
        CheckInput {
            path,
            content,
            project_root: PathBuf::from(o.root.unwrap_or_else(|| ".".into())),
            disk,
            overlays,
            lang,
        },
        camadas,
        contexto,
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
            motivo: None,
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
    fn fusao_do_cli_corrobora_o_tipo_fechado_da_camada_1_com_a_classe_da_camada_2() {
        // P0B-d (d94f8090d150): `st.st_m_size` — layer 1 knows `stat_result` is closed and warns
        // alone; layer 2 warns (`Escopo::Tipo`) naming the receiver class. The CLI's `--camadas 1,2`
        // merge must deny when the two name the SAME type, and only then.
        let input = CheckInput {
            path: "/p/a.py".into(),
            content: "from pathlib import Path\nst = Path('x').stat()\nprint(st.st_m_size)\n"
                .into(),
            project_root: PathBuf::from("/p"),
            disk: None,
            overlays: HashMap::new(),
            lang: "python".into(),
        };
        let l2 = |classe: &str| L2Resultado {
            verdict: VerdictL2::Warn,
            motivo: None,
            achados: vec![L2Achado {
                classe: "membro".into(),
                nome: "st_m_size".into(),
                linha: 3,
                escopo: Escopo::Tipo,
                severidade: Severidade::Warn,
                regra: "reportAttributeAccessIssue".into(),
                mensagem: format!(
                    "Cannot access attribute \"st_m_size\" for class \"{classe}\"\n  Attribute \"st_m_size\" is unknown"
                ),
            }],
            elapsed_ms: 10,
        };
        for (classe, esperado) in [
            ("stat_result", "deny"),
            ("os.stat_result", "deny"),
            ("Foo", "warn"),
        ] {
            let mut out = check_em(&input, Contexto::Escrita);
            assert_eq!(out.verdict, "warn", "a camada 1 sozinha só avisa");
            fundir_camada2(&mut out, &l2(classe));
            assert_eq!(out.verdict, esperado, "{classe}: {:?}", out.findings);
        }
    }

    #[test]
    fn lang_efetiva_flag_vence_o_caminho_e_o_default_e_python() {
        let l = |flag: Option<&str>, path: &str| lang_efetiva(flag.map(str::to_string), path);
        assert_eq!(l(None, "/p/a.rs"), "rust", "sem flag, sai do caminho");
        assert_eq!(l(None, "/p/a.tsx"), "tsx");
        assert_eq!(l(None, "/p/a.html"), "html");
        assert_eq!(
            l(Some("python"), "/p/a.rs"),
            "python",
            "a flag explícita vence"
        );
        assert_eq!(l(None, "<string>"), "python", "inline segue python");
        assert_eq!(l(None, "/p/script_sem_sufixo"), "python");
    }

    #[test]
    fn disk_file_chega_ao_check_para_qualquer_lang() {
        // o delta (`--disk-file`) é do `check`, não de Python: o CLI entrega o disco intacto
        let tmp = std::env::temp_dir().join("vgp-cli-disk-lang");
        std::fs::create_dir_all(&tmp).unwrap();
        let (novo, disco) = (tmp.join("novo.rs"), tmp.join("disco.rs"));
        std::fs::write(&novo, "fn a() {}\n").unwrap();
        std::fs::write(&disco, "fn b() {}\n").unwrap();
        for (path, lang) in [
            ("/p/a.rs", "rust"),
            ("/p/a.ts", "typescript"),
            ("/p/a.tsx", "tsx"),
            ("/p/a.html", "html"),
            ("/p/a.py", "python"),
        ] {
            let (input, _, _) = parse_args(&args(&[
                "check",
                "--path",
                path,
                "--content-file",
                novo.to_str().unwrap(),
                "--disk-file",
                disco.to_str().unwrap(),
                "-j",
            ]))
            .unwrap();
            assert_eq!(input.lang, lang, "{path}");
            assert_eq!(input.disk.as_deref(), Some("fn b() {}\n"), "{path}");
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn linguagem_sem_adaptador_sai_unknown_com_motivo_no_json_do_cli() {
        let tmp = std::env::temp_dir().join("vgp-cli-sem-adaptador");
        std::fs::create_dir_all(&tmp).unwrap();
        let f = tmp.join("a.go");
        std::fs::write(&f, "package main\n").unwrap();
        let out = run_json(&args(&[
            "check",
            "--path",
            "/tmp/a.go",
            "--content-file",
            f.to_str().unwrap(),
            "--lang",
            "go",
            "-j",
        ]));
        assert_eq!(out.verdict, "unknown");
        let js = serde_json::to_value(&out).unwrap();
        assert!(js["motivo"].as_str().unwrap().contains("`go`"));
        let _ = std::fs::remove_dir_all(&tmp);
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
        // FRAG: sem `--contexto` o veredito é o da ESCRITA — o nome ausente do projeto num
        // fragmento em /tmp só avisa
        assert_eq!(out.verdict, "warn");
        assert!(
            out.findings
                .iter()
                .any(|f| f.nome == "cp" && f.severidade == "warn")
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn a_camada_2_do_cli_segue_o_momento_pedido() {
        use touring_pyright::achado::{Balde, severidade};
        let achados = [Escopo::Proprio, Escopo::Externo]
            .into_iter()
            .map(|escopo| L2Achado {
                classe: "nome".into(),
                nome: format!("{escopo:?}"),
                linha: 1,
                escopo,
                severidade: severidade(Balde::Tmp, escopo),
                regra: "reportUndefinedVariable".into(),
                mensagem: "m".into(),
            })
            .collect::<Vec<_>>();
        let entrada = |path: &str| CheckInput {
            path: path.into(),
            content: String::new(),
            project_root: PathBuf::from("/home/u/proj"),
            disk: None,
            overlays: HashMap::new(),
            lang: "python".into(),
        };
        let sev = |l2: &L2Resultado| l2.achados.iter().map(|a| a.severidade).collect::<Vec<_>>();
        let mut escrita = L2Resultado::from_achados(achados.clone(), 1);
        no_momento(&mut escrita, &entrada("/tmp/frag.py"), Contexto::Escrita);
        assert_eq!(sev(&escrita), [Severidade::Warn, Severidade::Deny]);
        let mut execucao = L2Resultado::from_achados(achados.clone(), 1);
        no_momento(&mut execucao, &entrada("/tmp/frag.py"), Contexto::Execucao);
        assert_eq!(sev(&execucao), [Severidade::Deny, Severidade::Deny]);
        let mut inline = L2Resultado::from_achados(achados, 1);
        no_momento(&mut inline, &entrada("<string>"), Contexto::Escrita);
        assert_eq!(
            sev(&inline),
            [Severidade::Deny, Severidade::Deny],
            "o inline roda agora"
        );
    }

    #[test]
    fn contexto_execucao_nega_o_fragmento_em_tmp_e_contexto_invalido_e_recusado() {
        let tmp = std::env::temp_dir().join("vgp-cli-contexto");
        std::fs::create_dir_all(&tmp).unwrap();
        let f = tmp.join("a.py");
        std::fs::write(&f, "print(cp.PASTA)\n").unwrap();
        let pedido = |contexto: Option<&str>, path: &str| {
            let mut a = vec![
                "check",
                "--path",
                path,
                "--content-file",
                f.to_str().unwrap(),
                "--project-root",
                "/tmp",
                "-j",
            ];
            if let Some(c) = contexto {
                a.extend(["--contexto", c]);
            }
            run_json(&args(&a))
        };
        for c in ["execucao", "execução"] {
            let out = pedido(Some(c), "/tmp/x.py");
            assert_eq!(
                out.verdict, "deny",
                "{c}: o mesmo arquivo, agora para rodar"
            );
            assert!(
                out.findings
                    .iter()
                    .any(|f| f.nome == "cp" && f.severidade == "deny")
            );
        }
        assert_eq!(pedido(Some("escrita"), "/tmp/x.py").verdict, "warn");
        assert_eq!(
            pedido(None, "/tmp/x.py").verdict,
            "warn",
            "default = escrita"
        );
        assert_eq!(
            pedido(Some("execucao"), "/home/u/proj/src/x.py").verdict,
            "warn",
            "fora do scratch a política não muda, só o momento"
        );
        assert_eq!(pedido(Some("execucao"), "<string>").verdict, "deny");
        // contexto desconhecido: recusa com motivo (unknown), nunca um veredito inventado
        let ruim = pedido(Some("rodar"), "/tmp/x.py");
        assert_eq!(ruim.verdict, "unknown");
        assert!(
            ruim.motivo
                .as_deref()
                .is_some_and(|m| m.contains("--contexto") && m.contains("rodar")),
            "{:?}",
            ruim.motivo
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn content_file_ilegivel_e_unknown_nunca_crasha() {
        let out = run_json(&args(&["check", "--content-file", "/nao/existe.py", "-j"]));
        assert_eq!(out.verdict, "unknown");
    }

    #[test]
    fn conteudo_nao_utf8_e_unknown_com_motivo_e_a_lang_do_pedido() {
        // 1.236 dos 6.509 HTML do `analise` são Latin-1: antes voltavam como `"lang":"python"` +
        // um achado `erro` (warn) que o juiz contava como aviso num arquivo limpo
        let tmp = std::env::temp_dir().join("vgp-cli-nao-utf8");
        std::fs::create_dir_all(&tmp).unwrap();
        let f = tmp.join("latin1");
        std::fs::write(&f, b"<p>caf\xe9</p>\n").unwrap();
        for (path, lang) in [
            ("/p/a.html", "html"),
            ("/p/a.py", "python"),
            ("/p/a.tsx", "tsx"),
        ] {
            let out = run_json(&args(&[
                "check",
                "--path",
                path,
                "--content-file",
                f.to_str().unwrap(),
                "-j",
            ]));
            assert_eq!(out.verdict, "unknown", "{path}");
            assert_eq!(
                out.lang, lang,
                "{path}: a linguagem do pedido, não `python`"
            );
            let motivo = out.motivo.as_deref().unwrap_or_default();
            assert!(
                motivo.starts_with("conteúdo não é UTF-8"),
                "{path}: {motivo}"
            );
            assert!(
                out.findings.is_empty(),
                "{path}: não medir não é um achado `erro`"
            );
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn disk_file_e_overlay_nao_utf8_tambem_sao_unknown_sem_achado() {
        let tmp = std::env::temp_dir().join("vgp-cli-nao-utf8-disco");
        std::fs::create_dir_all(&tmp).unwrap();
        let (bom, ruim) = (tmp.join("novo.html"), tmp.join("disco.html"));
        std::fs::write(&bom, "<p>ok</p>\n").unwrap();
        std::fs::write(&ruim, b"<p>caf\xe9</p>\n").unwrap();
        let sobreposto = format!("a.html={}", ruim.display());
        for extra in [
            ["--disk-file", ruim.to_str().unwrap()],
            ["--overlay", sobreposto.as_str()],
        ] {
            let mut a = vec![
                "check",
                "--path",
                "/p/a.html",
                "--content-file",
                bom.to_str().unwrap(),
                "-j",
            ];
            a.extend(extra);
            let out = run_json(&args(&a));
            assert_eq!(out.verdict, "unknown", "{extra:?}");
            assert_eq!(out.lang, "html", "{extra:?}");
            assert!(out.findings.is_empty(), "{extra:?}");
            assert!(out.motivo.unwrap().starts_with("conteúdo não é UTF-8"));
        }
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn recusa_de_argumento_leva_a_lang_do_pedido_e_mantem_o_achado_erro() {
        let out = run_json(&args(&[
            "check",
            "--path",
            "/p/a.html",
            "--content-file",
            "/nao/existe.html",
            "-j",
        ]));
        assert_eq!(
            (out.verdict.as_str(), out.lang.as_str()),
            ("unknown", "html")
        );
        assert_eq!(out.findings.len(), 1);
        assert_eq!(out.findings[0].classe, "erro");
        let out = run_json(&args(&["check", "--path", "/p/a.ts", "-j"]));
        assert_eq!(
            out.lang, "typescript",
            "falta --content-file: a lang vem do caminho"
        );
        let out = run_json(&args(&["check", "--overlay", "sem-igual", "-j"]));
        assert_eq!(
            out.lang, "python",
            "antes de qualquer flag a lang é a default"
        );
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

    fn projeto_com(toml: Option<&str>, etiqueta: &str) -> PathBuf {
        let raiz =
            std::env::temp_dir().join(format!("vgp-cli-camada2-{etiqueta}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&raiz);
        std::fs::create_dir_all(raiz.join(".touring")).unwrap();
        if let Some(texto) = toml {
            std::fs::write(raiz.join(".touring/touring.toml"), texto).unwrap();
        }
        raiz
    }

    /// `touring vgp check` sobre um `.rs` do projeto, pedindo as camadas `camadas`.
    fn checar_rust(raiz: &std::path::Path, camadas: &str) -> CheckOutput {
        let proposto = raiz.join("proposto.rs");
        std::fs::write(&proposto, "fn a() {}\n").unwrap();
        run_json(&args(&[
            "check",
            "--path",
            raiz.join("src/a.rs").to_str().unwrap(),
            "--content-file",
            proposto.to_str().unwrap(),
            "--project-root",
            raiz.to_str().unwrap(),
            "--camadas",
            camadas,
            "-j",
        ]))
    }

    #[test]
    fn camada2_so_roda_na_lang_declarada_e_o_cli_diz_quando_nao_roda() {
        // padrão: só Python tem camada 2 — pedir `1,2` para Rust fica na camada 1 e DIZ por quê
        let raiz = projeto_com(None, "padrao");
        let out = checar_rust(&raiz, "1,2");
        assert_eq!(out.layer, 1, "a camada 2 não rodou para rust");
        assert!(
            out.camada2
                .as_deref()
                .is_some_and(|m| m.starts_with("desligada") && m.contains("`rust`")),
            "ausência não pode ser silêncio: {:?}",
            out.camada2
        );
        assert_eq!(
            checar_rust(&raiz, "1").camada2,
            None,
            "camada 2 não pedida: nada a dizer"
        );
        // declarada: `checar_lang("rust", …)` roda; sem Cargo.toml o verificador responde unknown + motivo
        let raiz = projeto_com(Some("[vgp]\ncamada2_langs = [\"rust\"]\n"), "rust");
        let out = checar_rust(&raiz, "1,2");
        assert_eq!(out.layer, 2, "a camada 2 de rust rodou");
        assert!(
            out.camada2
                .as_deref()
                .is_some_and(|m| m.starts_with("unknown:") && m.contains("rust-analyzer")),
            "o verificador é o de Rust (checar_lang), não o pyright: {:?}",
            out.camada2
        );
        // a lista declarada substitui o padrão: sem "rust" nela, volta a desligada
        let raiz = projeto_com(Some("[vgp]\ncamada2_langs = [\"typescript\"]\n"), "ts");
        let out = checar_rust(&raiz, "1,2");
        assert_eq!(out.layer, 1);
        assert!(
            out.camada2
                .as_deref()
                .is_some_and(|m| m.starts_with("desligada"))
        );
    }
}
