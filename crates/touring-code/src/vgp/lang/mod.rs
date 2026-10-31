//! Language adapters of the VGP motor (VL0 §2-§3) — one file per adapter, one line per adapter
//! in `REGISTRO`.
//!
//! The contract an adapter keeps ([`Adaptador`]): it MEASURES the proposed content — findings
//! with the [`Alvo`](super::politica::Alvo) each is about, or the reason it could not measure
//! ([`Verificacao`]). It does not decide verdicts, severities or deltas: [`super::check`] does,
//! once, for all languages (enforcement lives in the executor, D8). It is pure: no daemon, no
//! process spawn (layer 2 lives elsewhere).
//!
//! **Adding or replacing an adapter touches ONE file** (`lang/<lang>.rs`): every language of the
//! §2 table already has its module, its registry line and a stub that answers `unknown` with
//! "adaptador `lang` ainda não implementado". The agent writing the Rust adapter replaces the
//! body of `rust.rs` and nothing else — no shared line to merge. Cross-language checks (a CDN
//! `src` in html and in a JS `import`; a CSS class used from html and from JSX) are
//! [`Transversal`]s in their own file (`cdn.rs`, `classe_css.rs`), registered in
//! `TRANSVERSAIS` the same way.
//!
//! Grammar access is centralised here too ([`gramatica`], [`parse`]): TypeScript and TSX are two
//! grammars (`<T>expr` is a cast in `.ts` and an unclosed JSX element in `.tsx`), and the
//! shared `ast::Lang::TypeScript` (TSX for everything) must not be used by an adapter.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;

use tree_sitter::{Language, Parser, Tree};

use super::{CheckInput, Finding};

pub mod cdn;
pub mod classe_css;
pub mod css;
pub mod html;
pub mod javascript;
pub mod python;
pub mod rust;
pub mod tsx;
pub mod typescript;

/// What an adapter reports about one content: the findings it could prove, and — when part of
/// the content (or all of it) was out of its reach — why. `nao_medido` set with no findings is
/// the `unknown` verdict; with findings, the findings stand and the reason travels beside them.
#[derive(Debug)]
pub struct Verificacao {
    /// Findings with their [`Alvo`](super::politica::Alvo), built with [`Finding::candidato`].
    pub achados: Vec<Finding>,
    /// Why something could not be measured (parse failure, unresolved sheet, offline CDN…).
    pub nao_medido: Option<String>,
}

impl Verificacao {
    /// The content was measured; `achados` is everything the adapter could prove.
    pub fn medido(achados: Vec<Finding>) -> Self {
        Self {
            achados,
            nao_medido: None,
        }
    }

    /// Nothing could be measured, and here is why (`unknown`, never `allow`).
    pub fn nao_medido(motivo: impl Into<String>) -> Self {
        Self {
            achados: Vec::new(),
            nao_medido: Some(motivo.into()),
        }
    }

    /// The answer of a registered adapter whose body is not written yet.
    pub fn nao_implementado(lang: &str) -> Self {
        Self::nao_medido(format!("adaptador {lang} ainda não implementado"))
    }
}

/// One language's layer-1 checks.
pub trait Adaptador: Sync {
    /// The `lang` value of VL0 §2 this adapter answers for.
    fn lang(&self) -> &'static str;
    /// Measure the content in `input.content`. The delta against `input.disk` is applied by
    /// [`super::check`] (it calls this again on the disk content), not here.
    fn verificar(&self, input: &CheckInput) -> Verificacao;
}

/// A check that crosses languages and so belongs to none of the adapters (the CDN pin of an
/// html `<script src>` and of a JS `import 'https://…'`; the CSS class of html and of JSX).
/// It runs after the adapter of each language in [`Transversal::langs`], on the same content.
/// Returning no findings is NOT an `allow` claim: what it could not measure comes back in
/// [`Verificacao::nao_medido`], and the motor joins that reason to the adapter's.
pub trait Transversal: Sync {
    /// Name for diagnostics.
    fn nome(&self) -> &'static str;
    /// The `lang` values this check runs for.
    fn langs(&self) -> &'static [&'static str];
    /// Findings over `input.content`, with their [`Alvo`](super::politica::Alvo), plus the reason
    /// for whatever stayed unmeasured ([`Verificacao::nao_medido`]).
    fn verificar(&self, input: &CheckInput) -> Verificacao;
}

/// The registry: ONE line per language (trait objects — the set is open and each adapter lives in
/// its own file; dispatch costs one call per check). `Sync` is the supertrait that lets the
/// adapters sit in a `static`.
static REGISTRO: &[&dyn Adaptador] = &[
    &python::Python,
    &rust::Rust,
    &typescript::TypeScript,
    &tsx::Tsx,
    &javascript::JavaScript,
    &html::Html,
    &css::Css,
];

/// The transversal checks, one line each.
static TRANSVERSAIS: &[&dyn Transversal] = &[&cdn::Cdn, &classe_css::ClasseCss];

/// The adapter registered for `lang`, if any.
pub fn adaptador_de(lang: &str) -> Option<&'static dyn Adaptador> {
    REGISTRO.iter().copied().find(|a| a.lang() == lang)
}

/// The transversal checks that run for `lang`.
pub fn transversais_de(lang: &str) -> Vec<&'static dyn Transversal> {
    TRANSVERSAIS
        .iter()
        .copied()
        .filter(|t| t.langs().contains(&lang))
        .collect()
}

/// Every `lang` with a registered adapter, in registry order.
pub fn langs_registradas() -> Vec<&'static str> {
    REGISTRO.iter().map(|a| a.lang()).collect()
}

/// The language of a path by its extension (VL0 §2) — the single source hooks, CLI and judge
/// derive from. `None` for a path with no known extension (an inline `<string>`, `Makefile`,
/// `.md`, a dotfile named `.py`).
pub fn lang_do_caminho(path: &str) -> Option<&'static str> {
    let ext = Path::new(path).extension()?.to_str()?.to_ascii_lowercase();
    EXTENSOES
        .iter()
        .find(|(e, _)| *e == ext)
        .map(|(_, lang)| *lang)
}

/// VL0 §2: extension (lowercase, no dot) → `lang`. The one table; add a row here and nowhere else.
const EXTENSOES: &[(&str, &str)] = &[
    ("py", "python"),
    ("pyi", "python"),
    ("rs", "rust"),
    ("ts", "typescript"),
    ("mts", "typescript"),
    ("cts", "typescript"),
    ("tsx", "tsx"),
    ("js", "javascript"),
    ("mjs", "javascript"),
    ("cjs", "javascript"),
    ("jsx", "javascript"),
    ("html", "html"),
    ("htm", "html"),
    ("css", "css"),
];

/// The tree-sitter grammar of a VL0 §2 `lang` (`None` for any other string). TypeScript and
/// TSX are distinct grammars (`tree_sitter_typescript::LANGUAGE_TYPESCRIPT` / `LANGUAGE_TSX`);
/// `javascript` already includes JSX.
pub fn gramatica(lang: &str) -> Option<Language> {
    Some(match lang {
        "python" => tree_sitter_python::LANGUAGE.into(),
        "rust" => tree_sitter_rust::LANGUAGE.into(),
        "typescript" => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        "tsx" => tree_sitter_typescript::LANGUAGE_TSX.into(),
        "javascript" => tree_sitter_javascript::LANGUAGE.into(),
        "html" => tree_sitter_html::LANGUAGE.into(),
        "css" => tree_sitter_css::LANGUAGE.into(),
        _ => return None,
    })
}

thread_local! {
    /// One parser per (thread, lang): `Parser` is mutable state, `Language` is `Sync`.
    static PARSERS: RefCell<HashMap<String, Parser>> = RefCell::new(HashMap::new());
}

/// Parse `source` with the grammar of `lang`, bounded in time like every parse of the crate
/// (a grammar stuck in error recovery must not hold a hook). `Err` carries the reason — an
/// adapter turns it into [`Verificacao::nao_medido`]. Note a returned tree may still contain
/// ERROR nodes: the grammars recover; judging them is the adapter's call.
pub fn parse(lang: &str, source: &str) -> Result<Tree, String> {
    PARSERS.with(|cell| {
        let mut parsers = cell.borrow_mut();
        if !parsers.contains_key(lang) {
            let gram = gramatica(lang).ok_or_else(|| format!("sem gramática para `{lang}`"))?;
            let mut p = Parser::new();
            p.set_language(&gram).map_err(|e| {
                format!("gramática `{lang}` recusada pelo runtime tree-sitter: {e}")
            })?;
            parsers.insert(lang.to_string(), p);
        }
        let parser = parsers
            .get_mut(lang)
            .ok_or_else(|| format!("parser de `{lang}` ausente"))?;
        crate::ast::parser::parse_bounded(parser, source, None)
            .ok_or_else(|| format!("parse de `{lang}` não terminou no orçamento"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vgp::politica::Alvo;
    use crate::vgp::{CheckOutput, Contexto, check, check_em, julgar};
    use std::collections::{HashMap, HashSet};
    use std::path::PathBuf;

    fn entrada(path: &str, content: &str, lang: &str) -> CheckInput {
        CheckInput {
            path: path.into(),
            content: content.into(),
            project_root: PathBuf::from("/home/u/proj"),
            disk: None,
            overlays: HashMap::new(),
            lang: lang.into(),
        }
    }

    #[test]
    fn lang_do_caminho_tabela_do_paragrafo_2() {
        let tabela: &[(&str, Option<&str>)] = &[
            ("a.py", Some("python")),
            ("a.pyi", Some("python")),
            ("src/lib.rs", Some("rust")),
            ("a.ts", Some("typescript")),
            ("a.mts", Some("typescript")),
            ("a.cts", Some("typescript")),
            ("types/a.d.ts", Some("typescript")),
            ("a.tsx", Some("tsx")),
            ("a.js", Some("javascript")),
            ("a.mjs", Some("javascript")),
            ("a.cjs", Some("javascript")),
            ("a.jsx", Some("javascript")),
            ("a.html", Some("html")),
            ("a.htm", Some("html")),
            ("a.css", Some("css")),
            ("/abs/Dir.py/A.PY", Some("python")),
            ("A.HTML", Some("html")),
            // desconhecidas → None
            ("README.md", None),
            ("a.json", None),
            ("a.scss", None),
            ("a.txt", None),
            ("Makefile", None),
            ("<string>", None),
            ("<stdin>", None),
            (".py", None),
            ("dir.py/arquivo_sem_extensao", None),
            ("", None),
        ];
        for (caminho, esperado) in tabela {
            assert_eq!(lang_do_caminho(caminho), *esperado, "{caminho:?}");
        }
    }

    #[test]
    fn toda_extensao_da_tabela_leva_a_uma_lang_com_adaptador_e_gramatica() {
        let langs: HashSet<&str> = EXTENSOES.iter().map(|(_, l)| *l).collect();
        assert_eq!(langs.len(), 7, "as sete linguagens do §2: {langs:?}");
        for lang in langs {
            assert!(
                adaptador_de(lang).is_some(),
                "`{lang}` na tabela de extensões mas sem linha no REGISTRO"
            );
            assert!(gramatica(lang).is_some(), "`{lang}` sem gramática");
        }
        for adaptador in langs_registradas() {
            assert!(
                EXTENSOES.iter().any(|(_, l)| *l == adaptador),
                "`{adaptador}` registrada mas nenhuma extensão leva a ela"
            );
        }
    }

    #[test]
    fn registro_sem_lang_duplicada_e_cada_adaptador_responde_pela_sua() {
        let langs = langs_registradas();
        let unicas: HashSet<&&str> = langs.iter().collect();
        assert_eq!(
            unicas.len(),
            langs.len(),
            "lang duplicada no REGISTRO: {langs:?}"
        );
        for lang in &langs {
            assert_eq!(adaptador_de(lang).map(|a| a.lang()), Some(*lang));
        }
    }

    #[test]
    fn linguagem_sem_adaptador_e_unknown_com_motivo_nunca_allow() {
        let out: CheckOutput = check(&entrada("/tmp/a.go", "package main\n", "go"));
        assert_eq!(out.verdict, "unknown");
        assert!(out.findings.is_empty());
        let motivo = out.motivo.expect("o unknown carrega o motivo");
        assert!(
            motivo.contains("`go`") && motivo.contains("sem adaptador"),
            "{motivo}"
        );
        // idem para uma string vazia e para a grafia errada de uma lang válida
        for lang in ["", "Python", "ts"] {
            let o = check(&entrada("/tmp/x", "x", lang));
            assert_eq!(o.verdict, "unknown", "lang `{lang}`");
            assert!(o.motivo.is_some());
        }
    }

    #[test]
    fn gramaticas_aceitas_pelo_runtime_e_ts_tsx_sao_distintas() {
        for lang in [
            "python",
            "rust",
            "typescript",
            "tsx",
            "javascript",
            "html",
            "css",
        ] {
            let amostra = match lang {
                "python" => "x = 1\n",
                "rust" => "fn main() {}\n",
                "html" => "<p id=\"a\">oi</p>\n",
                "css" => ".a { color: red; }\n",
                _ => "const x = 1;\n",
            };
            let tree = parse(lang, amostra).unwrap_or_else(|e| panic!("{lang}: {e}"));
            assert!(!tree.root_node().has_error(), "{lang} deve parsear limpo");
        }
        assert!(gramatica("go").is_none());
        // `<T>expr` é asserção de tipo em .ts e elemento JSX não fechado em .tsx
        let cast = "const y = <number>x;\n";
        assert!(!parse("typescript", cast).unwrap().root_node().has_error());
        assert!(parse("tsx", cast).unwrap().root_node().has_error());
        // e o JSX só existe na gramática TSX
        let jsx = "const el = <div className=\"a\" />;\n";
        assert!(!parse("tsx", jsx).unwrap().root_node().has_error());
        assert!(parse("typescript", jsx).unwrap().root_node().has_error());
    }

    #[test]
    fn parse_de_lang_desconhecida_e_erro_com_motivo() {
        let e = parse("cobol", "x").unwrap_err();
        assert!(e.contains("cobol"), "{e}");
    }

    #[test]
    fn na_escrita_so_o_inline_roda_agora_o_fragmento_em_tmp_ou_cache_apenas_avisa() {
        // FRAG (10/10): um arquivo escrito em /tmp ou no .cache ainda pode ser completado pela
        // edição seguinte (o caso `trecho_g3.py`, um trecho para colar) — o nome ausente é do
        // PROJETO e só vira deny onde nenhuma edição seguinte conserta: o inline (§5.1).
        let src = "print(nome_que_nao_existe)\n";
        let veredito = |path: &str| check(&entrada(path, src, "python")).verdict;
        assert_eq!(veredito("/home/u/proj/src/a.py"), "warn");
        assert_eq!(veredito("/home/u/proj/.cache/scratch/a.py"), "warn");
        assert_eq!(veredito("/tmp/frag.py"), "warn", "fragmento em /tmp");
        assert_eq!(veredito("<string>"), "deny");
        assert_eq!(veredito("<stdin>"), "deny");
        assert_eq!(
            veredito("/home/u/outro/.cache/a.py"),
            "warn",
            "o .cache de outro projeto"
        );
    }

    #[test]
    fn na_execucao_o_scratch_roda_agora_e_o_python_nega_o_simbolo_do_projeto_ali() {
        // o MESMO fragmento, agora pedido para rodar: o /tmp e o .cache da raiz negam; um
        // arquivo comum segue só avisando (a política não muda, só o momento em que ela vale)
        let src = "print(nome_que_nao_existe)\n";
        let veredito =
            |path: &str| check_em(&entrada(path, src, "python"), Contexto::Execucao).verdict;
        assert_eq!(veredito("/tmp/frag.py"), "deny");
        assert_eq!(veredito("/home/u/proj/.cache/scratch/a.py"), "deny");
        assert_eq!(veredito("<string>"), "deny");
        assert_eq!(veredito("/home/u/proj/src/a.py"), "warn");
        assert_eq!(veredito("/home/u/outro/.cache/a.py"), "warn");
    }

    #[test]
    fn alvo_externo_inexistente_nega_ja_na_escrita_em_tmp_p1_nao_muda() {
        // `unicodedata.combending` não existe na stdlib: nenhuma edição seguinte do PROJETO o conserta — P1
        // nega em qualquer balde, inclusive o fragmento em /tmp na escrita
        let src = "import unicodedata\nprint(unicodedata.combending('a'))\n";
        for path in [
            "/tmp/x.py",
            "/home/u/proj/.cache/x.py",
            "/home/u/proj/src/x.py",
        ] {
            let out = check(&entrada(path, src, "python"));
            assert_eq!(out.verdict, "deny", "{path}: {:?}", out.findings);
            assert!(out.findings.iter().any(|f| f.nome.contains("combending")));
        }
    }

    // ---------------------------------------------------------------- política e delta, em QUALQUER lang

    /// Adaptador de teste que responde por QUALQUER `lang`: reporta o que o conteúdo pede, sem
    /// gramática — prova que a política e o delta moram no `check`, não em Python. Conteúdo com
    /// `quebra` reporta todos os achados; `quebra só <nome>` reporta apenas o de nome `<nome>`
    /// (o "disco" que já tinha UM dos problemas); `ilegivel` não mede.
    struct Falso(&'static str, Vec<(&'static str, &'static str, Alvo)>);

    impl Adaptador for Falso {
        fn lang(&self) -> &'static str {
            self.0
        }
        fn verificar(&self, input: &CheckInput) -> Verificacao {
            if input.content.contains("quebra") {
                let so = input.content.split("só ").nth(1).map(str::trim);
                Verificacao::medido(
                    self.1
                        .iter()
                        .filter(|(_, n, _)| so.is_none_or(|s| s == *n))
                        .map(|(c, n, a)| Finding::candidato(c, n, 1, *a, "s".into(), Vec::new()))
                        .collect(),
                )
            } else if input.content.contains("ilegivel") {
                Verificacao::nao_medido("parse falhou")
            } else {
                Verificacao::medido(Vec::new())
            }
        }
    }

    fn julga(
        input: &CheckInput,
        achados: Vec<(&'static str, &'static str, Alvo)>,
    ) -> (String, Vec<Finding>, Option<String>) {
        julgar(input, &Falso("falso", achados), &[], Contexto::Escrita)
    }

    #[test]
    fn regra_de_politica_vale_para_adaptador_de_qualquer_lang() {
        // P1 externo nega no arquivo comum; P2 projeto só onde roda; P3 incerto nunca
        let achados = vec![
            ("atributo", "ext", Alvo::Externo),
            ("nome", "proj", Alvo::Projeto),
            ("atributo", "inc", Alvo::Incerto),
            ("classe_css", "gancho", Alvo::Externo),
        ];
        let arquivo = entrada("/home/u/proj/src/a.xyz", "quebra", "falso");
        let (v, fs, _) = julga(&arquivo, achados.clone());
        let sev = |n: &str| fs.iter().find(|f| f.nome == n).unwrap().severidade.clone();
        assert_eq!(v, "deny");
        assert_eq!(sev("ext"), "deny");
        assert_eq!(sev("proj"), "warn");
        assert_eq!(sev("inc"), "warn");
        assert_eq!(sev("gancho"), "warn", "classe consultiva nunca nega");

        // na ESCRITA o /tmp é um arquivo como outro: o símbolo do projeto ainda pode nascer
        let tmp = entrada("/tmp/a.xyz", "quebra", "falso");
        let (_, fs, _) = julga(&tmp, achados.clone());
        let sev = |n: &str| fs.iter().find(|f| f.nome == n).unwrap().severidade.clone();
        assert_eq!(sev("ext"), "deny", "P1 não muda: externo nega em /tmp");
        assert_eq!(sev("proj"), "warn", "projeto só avisa na escrita em /tmp");
        assert_eq!(sev("inc"), "warn", "incerto não nega nem em /tmp");

        // na EXECUÇÃO o mesmo arquivo roda agora: o símbolo do projeto nega, o incerto não
        let (v, fs, _) = julgar(
            &tmp,
            &Falso("falso", achados.clone()),
            &[],
            Contexto::Execucao,
        );
        let sev = |n: &str| fs.iter().find(|f| f.nome == n).unwrap().severidade.clone();
        assert_eq!(v, "deny");
        assert_eq!(sev("proj"), "deny", "projeto nega em /tmp na execução");
        assert_eq!(sev("inc"), "warn", "incerto não nega nem na execução");
        let arquivo = entrada("/home/u/proj/src/a.xyz", "quebra", "falso");
        let (_, fs, _) = julgar(
            &arquivo,
            &Falso("falso", achados.clone()),
            &[],
            Contexto::Execucao,
        );
        let sev = |n: &str| fs.iter().find(|f| f.nome == n).unwrap().severidade.clone();
        assert_eq!(sev("proj"), "warn", "fora do scratch a política não muda");

        // o inline roda agora, em qualquer lang
        let inline = entrada("<string>", "quebra", "falso");
        let (_, fs, _) = julga(&inline, achados);
        let sev = |n: &str| fs.iter().find(|f| f.nome == n).unwrap().severidade.clone();
        assert_eq!(sev("proj"), "deny", "projeto nega no inline");
        assert_eq!(sev("inc"), "warn", "incerto não nega nem no inline");
    }

    #[test]
    fn adaptador_nao_decide_veredito_so_os_achados_decidem() {
        // sem achados medidos o veredito é allow; não medido é unknown; nunca deny sem achado
        let (v, fs, m) = julga(&entrada("/tmp/a.xyz", "limpo", "falso"), Vec::new());
        assert_eq!((v.as_str(), fs.len(), m), ("allow", 0, None));
        let (v, fs, m) = julga(&entrada("/tmp/a.xyz", "ilegivel", "falso"), Vec::new());
        assert_eq!((v.as_str(), fs.len()), ("unknown", 0));
        assert_eq!(m.as_deref(), Some("parse falhou"));
    }

    #[test]
    fn delta_vale_para_qualquer_lang_e_so_rebaixa_o_que_o_disco_ja_tinha() {
        let achados = vec![
            ("atributo", "velho", Alvo::Externo),
            ("atributo", "novo", Alvo::Externo),
        ];
        // o "disco" também quebra: o Falso o vê com os MESMOS achados → ambos pré-existentes
        let mut input = entrada("/home/u/proj/a.xyz", "quebra", "falso");
        input.disk = Some("quebra (versão do disco)".into());
        let (v, fs, _) = julga(&input, achados.clone());
        assert_eq!(v, "warn", "tudo já estava no disco: nada é introduzido");
        assert!(
            fs.iter()
                .all(|f| f.severidade == "warn" && f.sugestao.starts_with("pré-existente"))
        );
        // o disco limpo → nada pré-existente → o deny fica
        input.disk = Some("limpo".into());
        let (v, fs, _) = julga(&input, achados.clone());
        assert_eq!(v, "deny");
        assert!(fs.iter().all(|f| f.severidade == "deny"));
        // o disco ilegível (parse falhou) não suprime nada
        input.disk = Some("ilegivel".into());
        let (v, _, _) = julga(&input, achados);
        assert_eq!(v, "deny", "sem baseline legível o delta não suprime");
    }

    #[test]
    fn delta_vale_para_toda_lang_registrada_o_pre_existente_no_disco_nunca_vira_deny() {
        // `--disk-file` (delta, §3 passo 4): o despacho é um só — `julgar` — e aqui é provado para
        // CADA lang do REGISTRO com um adaptador de teste que responde por ela. Um adaptador real
        // nasce com o delta porque o delta não mora nele.
        for lang in langs_registradas() {
            let achados = vec![
                ("atributo", "velho", Alvo::Externo),
                ("atributo", "novo", Alvo::Externo),
            ];
            let adaptador = Falso(lang, achados);
            let mut input = entrada("/home/u/proj/src/a.xyz", "quebra", lang);
            // o disco já tinha UM dos dois: só o introduzido nega
            input.disk = Some("quebra só velho".into());
            let (v, fs, _) = julgar(&input, &adaptador, &[], Contexto::Escrita);
            let sev = |n: &str| fs.iter().find(|f| f.nome == n).unwrap().severidade.clone();
            assert_eq!(sev("velho"), "warn", "{lang}: pré-existente não nega");
            assert_eq!(sev("novo"), "deny", "{lang}: o introduzido nega");
            assert_eq!(v, "deny", "{lang}");
            // o disco igual ao proposto: nada é introduzido, o veredito cai para warn
            input.disk = Some("quebra".into());
            let (v, fs, _) = julgar(&input, &adaptador, &[], Contexto::Escrita);
            assert_eq!(v, "warn", "{lang}: tudo pré-existente");
            assert!(fs.iter().all(|f| f.severidade == "warn"), "{lang}");
            // sem `--disk-file` (arquivo novo): nada a rebaixar
            input.disk = None;
            assert_eq!(
                julgar(&input, &adaptador, &[], Contexto::Escrita).0,
                "deny",
                "{lang}"
            );
        }
    }

    // ---------------------------------------------------------------- transversais

    struct Marca;
    impl Transversal for Marca {
        fn nome(&self) -> &'static str {
            "marca"
        }
        fn langs(&self) -> &'static [&'static str] {
            &["falso"]
        }
        fn verificar(&self, _: &CheckInput) -> Verificacao {
            Verificacao::medido(vec![Finding::candidato(
                "cdn",
                "x",
                3,
                Alvo::Externo,
                "404".into(),
                Vec::new(),
            )])
        }
    }

    #[test]
    fn transversal_roda_depois_do_adaptador_e_passa_pela_mesma_politica() {
        let input = entrada("/home/u/proj/a.xyz", "limpo", "falso");
        let (v, fs, _) = julgar(
            &input,
            &Falso("falso", Vec::new()),
            &[&Marca],
            Contexto::Escrita,
        );
        assert_eq!(v, "deny", "cdn 404 (alvo externo) nega pela política comum");
        assert_eq!(fs.len(), 1);
        assert_eq!(fs[0].classe, "cdn");
    }

    /// Transversal que só se abstém: não acha nada e diz por quê.
    struct SoAbstem(&'static str);
    impl Transversal for SoAbstem {
        fn nome(&self) -> &'static str {
            "abstem"
        }
        fn langs(&self) -> &'static [&'static str] {
            &["falso"]
        }
        fn verificar(&self, _: &CheckInput) -> Verificacao {
            Verificacao::nao_medido(self.0)
        }
    }

    #[test]
    fn transversal_que_nao_mediu_faz_o_veredito_sair_unknown_com_o_motivo_dele() {
        let input = entrada("/home/u/proj/a.xyz", "limpo", "falso");
        // adaptador limpo + transversal que se abstém: nunca `allow`
        let (v, fs, motivo) = julgar(
            &input,
            &Falso("falso", Vec::new()),
            &[&SoAbstem("folha remota fora do cache")],
            Contexto::Escrita,
        );
        assert_eq!(v, "unknown");
        assert!(fs.is_empty());
        assert_eq!(motivo.as_deref(), Some("folha remota fora do cache"));
        // a abstenção convive com o achado de outro transversal: o achado decide, o motivo fica
        let (v, fs, motivo) = julgar(
            &input,
            &Falso("falso", Vec::new()),
            &[&Marca, &SoAbstem("folha remota fora do cache")],
            Contexto::Escrita,
        );
        assert_eq!(v, "deny");
        assert_eq!(fs.len(), 1);
        assert_eq!(motivo.as_deref(), Some("folha remota fora do cache"));
    }

    #[test]
    fn motivos_do_adaptador_e_dos_transversais_se_juntam_na_ordem_sem_perder_nenhum() {
        let input = entrada("/home/u/proj/a.xyz", "ilegivel", "falso");
        let (v, _, motivo) = julgar(
            &input,
            &Falso("falso", Vec::new()),
            &[&SoAbstem("um"), &Marca, &SoAbstem("dois")],
            Contexto::Escrita,
        );
        assert_eq!(v, "deny");
        assert_eq!(motivo.as_deref(), Some("parse falhou; um; dois"));
    }

    #[test]
    fn transversal_que_mediu_tudo_nao_acrescenta_motivo() {
        let input = entrada("/home/u/proj/a.xyz", "limpo", "falso");
        let (v, _, motivo) = julgar(
            &input,
            &Falso("falso", Vec::new()),
            &[&Marca],
            Contexto::Escrita,
        );
        assert_eq!(v, "deny");
        assert_eq!(motivo, None);
    }

    #[test]
    fn transversais_registradas_so_rodam_nas_langs_que_declaram() {
        assert!(transversais_de("python").is_empty());
        let html: Vec<_> = transversais_de("html").iter().map(|t| t.nome()).collect();
        assert!(html.contains(&"cdn"), "{html:?}");
        assert!(html.contains(&"classe_css"), "{html:?}");
        assert!(transversais_de("rust").is_empty());
    }
}
