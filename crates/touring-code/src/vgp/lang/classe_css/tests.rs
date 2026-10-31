//! Tests of the `classe_css` transversal check (VCSS): html `class=` and JSX `className=`
//! against the stylesheets the file loads. Every case builds a tiny project in a temp dir, so the
//! sheet resolution (relative, root-relative, `@import`, `node_modules`) runs on a real disk.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use super::*;
use crate::vgp::lang::Transversal;
use crate::vgp::lang::cdn::cache::{Cache, Registro, Veredito};
use crate::vgp::politica::Alvo;
use crate::vgp::{CheckInput, Finding};

fn escreve(raiz: &Path, rel: &str, conteudo: &str) {
    let alvo = raiz.join(rel);
    std::fs::create_dir_all(alvo.parent().unwrap()).unwrap();
    std::fs::write(alvo, conteudo).unwrap();
}

fn entrada(raiz: &Path, rel: &str, lang: &str, content: &str) -> CheckInput {
    CheckInput {
        path: raiz.join(rel).to_string_lossy().into_owned(),
        content: content.into(),
        project_root: raiz.to_path_buf(),
        disk: None,
        overlays: HashMap::new(),
        lang: lang.into(),
    }
}

/// A full html page: `cabeca` goes in `<head>` (starting at line 4), `corpo` in `<body>`.
fn pagina(cabeca: &str, corpo: &str) -> String {
    format!(
        "<!DOCTYPE html>\n<html>\n<head>\n{cabeca}\n</head>\n<body>\n{corpo}\n</body>\n</html>\n"
    )
}

/// The check with an EMPTY CDN cache: the tests never read the machine's real one.
fn medir_isolado(input: &CheckInput) -> Verificacao {
    ClasseCss.medir_com_cache(input, Arc::new(Cache::default()))
}

fn medir_html(raiz: &Path, html: &str) -> Verificacao {
    medir_isolado(&entrada(raiz, "index.html", "html", html))
}

fn medir_jsx(raiz: &Path, lang: &str, rel: &str, fonte: &str) -> Verificacao {
    medir_isolado(&entrada(raiz, rel, lang, fonte))
}

/// A CDN cache that knows ONE stylesheet and the classes it defines.
fn cache_com_css(url: &str, classes: &[&str]) -> Arc<Cache> {
    let mut cache = Cache::default();
    cache.inserir(
        url,
        Registro {
            classes: Some(classes.iter().map(|c| c.to_string()).collect()),
            ..Registro::novo(Veredito::Existe)
        },
    );
    Arc::new(cache)
}

/// (name, line) of every finding, in order.
fn achados(v: &Verificacao) -> Vec<(String, usize)> {
    v.achados
        .iter()
        .map(|f: &Finding| (f.nome.clone(), f.linha))
        .collect()
}

fn par(nome: &str, linha: usize) -> (String, usize) {
    (nome.into(), linha)
}

// ------------------------------------------------------------------ html: o caso do corpus

#[test]
fn classe_fora_do_style_inline_e_achado_com_voce_quis_dizer() {
    let dir = tempfile::tempdir().unwrap();
    let html = pagina(
        "<style>.meta { color: red } .header { margin: 0 }</style>",
        "<div class=\"header metaq\"></div>",
    );
    let v = medir_html(dir.path(), &html);
    assert_eq!(achados(&v), vec![par("metaq", 7)]);
    assert!(v.nao_medido.is_none(), "{:?}", v.nao_medido);
    let f = &v.achados[0];
    assert_eq!(f.classe, "classe_css");
    assert_eq!(f.alvo, Alvo::Projeto);
    assert!(f.sugestao.contains("você quis dizer"), "{}", f.sugestao);
    assert!(f.sugestao.contains("`meta`"), "{}", f.sugestao);
    assert_eq!(f.reais.first().map(String::as_str), Some("meta"));
}

#[test]
fn severidade_e_sempre_aviso_pela_politica_mesmo_em_codigo_que_roda_agora() {
    let dir = tempfile::tempdir().unwrap();
    let html = pagina("<style>.a{}</style>", "<p class=\"b\"></p>");
    let v = medir_html(dir.path(), &html);
    let sev = crate::vgp::politica::severidade(&v.achados[0].classe, v.achados[0].alvo, true);
    assert_eq!(sev, "warn");
}

#[test]
fn toda_classe_definida_nao_acha_nada_e_mede() {
    let dir = tempfile::tempdir().unwrap();
    let html = pagina(
        "<style>.a { } .b { }</style>",
        "<div class=\"a b\"></div><p class=\"b\"></p><span></span>",
    );
    let v = medir_html(dir.path(), &html);
    assert!(v.achados.is_empty());
    assert!(v.nao_medido.is_none(), "{:?}", v.nao_medido);
}

#[test]
fn as_formas_de_seletor_que_definem_uma_classe() {
    let dir = tempfile::tempdir().unwrap();
    let css = "@media (min-width: 1px) { .em-media { } }\n\
               .a .descendente { }\n\
               div.com-tag { }\n\
               .hover:hover { }\n\
               .antes::before { }\n\
               :is(.dentro-de-is, .outra) { }\n\
               :not(.dentro-de-not) { }\n\
               .sm\\:flex { }\n\
               .encadeada.segunda { }\n\
               .pai > .filho + .irmao ~ .primo { }\n\
               @supports (display: grid) { .em-supports { } }\n\
               @layer base { .em-layer { } }\n\
               [data-x] .com-atributo { }\n\
               .a, .b { }\n";
    let usadas = "em-media descendente com-tag hover antes dentro-de-is outra dentro-de-not \
                  sm:flex encadeada segunda pai filho irmao primo em-supports em-layer \
                  com-atributo a b";
    let html = pagina(
        &format!("<style>{css}</style>"),
        &format!("<div class=\"{usadas}\"></div>"),
    );
    let v = medir_html(dir.path(), &html);
    assert!(
        v.achados.is_empty(),
        "classes definidas viraram achado: {:?}",
        achados(&v)
    );
}

#[test]
fn a_pseudo_classe_nao_define_uma_classe_de_mesmo_nome() {
    // `:hover` is a pseudo-class, not the class `hover`
    let dir = tempfile::tempdir().unwrap();
    let html = pagina(
        "<style>a:hover { color: red }</style>",
        "<b class=\"hover\"></b>",
    );
    assert_eq!(
        achados(&medir_html(dir.path(), &html)),
        vec![par("hover", 7)]
    );
}

#[test]
fn caixa_importa_e_cada_classe_e_um_achado_so_na_primeira_linha() {
    let dir = tempfile::tempdir().unwrap();
    let html = pagina(
        "<style>.btn { }</style>",
        "<a class=\"Btn\"></a>\n<a class=\"Btn btn\"></a>",
    );
    assert_eq!(achados(&medir_html(dir.path(), &html)), vec![par("Btn", 7)]);
}

#[test]
fn a_linha_e_a_do_token_num_atributo_de_varias_linhas() {
    let dir = tempfile::tempdir().unwrap();
    let html = pagina(
        "<style>.ok { }</style>",
        "<div class=\"ok\n   nova\n   outra\"></div>",
    );
    assert_eq!(
        achados(&medir_html(dir.path(), &html)),
        vec![par("nova", 8), par("outra", 9)]
    );
}

#[test]
fn sem_sugestao_proxima_nao_inventa_voce_quis_dizer() {
    let dir = tempfile::tempdir().unwrap();
    let html = pagina("<style>.alpha { }</style>", "<i class=\"zzzzzz\"></i>");
    let v = medir_html(dir.path(), &html);
    assert_eq!(v.achados.len(), 1);
    assert!(!v.achados[0].sugestao.contains("você quis dizer"));
    assert!(v.achados[0].reais.is_empty());
}

// ------------------------------------------------------------------ html: o que são "folhas carregadas"

#[test]
fn link_local_define_as_classes_e_resolve_relativo_ao_documento() {
    let dir = tempfile::tempdir().unwrap();
    escreve(dir.path(), "css/site.css", ".card { } .titulo { }");
    let html = pagina(
        "<link rel=\"stylesheet\" href=\"css/site.css?v=3\">",
        "<div class=\"card cardd titulo\"></div>",
    );
    let v = medir_html(dir.path(), &html);
    assert_eq!(achados(&v), vec![par("cardd", 7)]);
    assert!(v.nao_medido.is_none());
}

#[test]
fn link_com_caminho_absoluto_resolve_na_raiz_do_projeto_ou_em_public() {
    let dir = tempfile::tempdir().unwrap();
    escreve(dir.path(), "public/static/site.css", ".so-em-public { }");
    escreve(dir.path(), "global.css", ".na-raiz { }");
    let html = pagina(
        "<link rel=\"stylesheet\" href=\"/static/site.css\">\n<link rel=\"stylesheet\" href=\"/global.css\">",
        "<p class=\"so-em-public na-raiz outra\"></p>",
    );
    let v = medir_html(dir.path(), &html);
    assert_eq!(achados(&v), vec![par("outra", 8)]);
    assert!(v.nao_medido.is_none(), "{:?}", v.nao_medido);
}

#[test]
fn rel_alternate_stylesheet_conta_e_rel_preload_nao() {
    let dir = tempfile::tempdir().unwrap();
    escreve(dir.path(), "a.css", ".da-alternativa { }");
    escreve(dir.path(), "b.css", ".so-preload { }");
    let html = pagina(
        "<link rel=\"alternate stylesheet\" href=\"a.css\">\n<link rel=\"preload\" as=\"style\" href=\"b.css\">\n<style>.x{}</style>",
        "<p class=\"da-alternativa so-preload\"></p>",
    );
    assert_eq!(
        achados(&medir_html(dir.path(), &html)),
        vec![par("so-preload", 9)]
    );
}

#[test]
fn folha_local_inexistente_e_nao_medido_e_nenhum_achado() {
    let dir = tempfile::tempdir().unwrap();
    let html = pagina(
        "<link rel=\"stylesheet\" href=\"nao_existe.css\">\n<style>.a{}</style>",
        "<p class=\"qualquer\"></p>",
    );
    let v = medir_html(dir.path(), &html);
    assert!(
        v.achados.is_empty(),
        "a folha ausente poderia definir a classe"
    );
    let motivo = v.nao_medido.expect("folha não resolvida = não medido");
    assert!(motivo.contains("nao_existe.css"), "{motivo}");
    assert!(motivo.contains("não resolvida"), "{motivo}");
}

#[test]
fn folha_remota_e_nao_medido_porque_a_camada_um_nao_usa_rede() {
    let dir = tempfile::tempdir().unwrap();
    let html = pagina(
        "<link rel=\"stylesheet\" href=\"https://cdn.exemplo.com/lib.css\">\n<style>.a{}</style>",
        "<p class=\"b\"></p>",
    );
    let v = medir_html(dir.path(), &html);
    assert!(v.achados.is_empty());
    let motivo = v.nao_medido.unwrap();
    assert!(
        motivo.contains("remota") && motivo.contains("cdn.exemplo.com"),
        "{motivo}"
    );
}

#[test]
fn google_fonts_css_so_traz_font_face_e_nao_atrapalha_mas_o_icon_traz_classe() {
    let dir = tempfile::tempdir().unwrap();
    let fontes = pagina(
        "<link rel=\"stylesheet\" href=\"https://fonts.googleapis.com/css2?family=Roboto\">\n<style>.a{}</style>",
        "<p class=\"b\"></p>",
    );
    let v = medir_html(dir.path(), &fontes);
    assert_eq!(achados(&v), vec![par("b", 8)]);
    assert!(v.nao_medido.is_none(), "{:?}", v.nao_medido);

    let icones = pagina(
        "<link rel=\"stylesheet\" href=\"https://fonts.googleapis.com/icon?family=Material+Icons\">\n<style>.a{}</style>",
        "<p class=\"b material-icons\"></p>",
    );
    let v = medir_html(dir.path(), &icones);
    assert!(v.achados.is_empty());
    assert!(v.nao_medido.unwrap().contains("remota"));
}

#[test]
fn import_local_e_seguido_com_ciclo_protegido_e_import_remoto_nao_resolve() {
    let dir = tempfile::tempdir().unwrap();
    escreve(dir.path(), "a.css", "@import \"b.css\"; .da-a { }");
    escreve(dir.path(), "b.css", "@import url('a.css'); .da-b { }");
    let html = pagina(
        "<link rel=\"stylesheet\" href=\"a.css\">",
        "<p class=\"da-a da-b nenhuma\"></p>",
    );
    let v = medir_html(dir.path(), &html);
    assert_eq!(achados(&v), vec![par("nenhuma", 7)]);

    escreve(
        dir.path(),
        "r.css",
        "@import url(https://x.exemplo/y.css); .da-r { }",
    );
    let html = pagina(
        "<link rel=\"stylesheet\" href=\"r.css\">",
        "<p class=\"da-r outra\"></p>",
    );
    let v = medir_html(dir.path(), &html);
    assert!(v.achados.is_empty());
    assert!(v.nao_medido.unwrap().contains("x.exemplo"));
}

#[test]
fn seletor_de_atributo_class_define_por_prefixo_sufixo_ou_trecho() {
    let dir = tempfile::tempdir().unwrap();
    let css =
        "[class^=\"icon-\"] { } [class$=\"-btn\"] { } [class*=\"col-\"] { } [class~=\"exato\"] { }";
    let html = pagina(
        &format!("<style>{css}</style>"),
        "<p class=\"icon-home save-btn col-md-6 exato fora\"></p>",
    );
    assert_eq!(
        achados(&medir_html(dir.path(), &html)),
        vec![par("fora", 7)]
    );
}

#[test]
fn gancho_de_js_do_proprio_documento_nao_e_classe_sem_definicao() {
    let dir = tempfile::tempdir().unwrap();
    let html = pagina(
        "<style>.a{}</style>",
        "<div class=\"a gancho1 gancho2 gancho3 gancho4 sem-gancho\"></div>\n\
         <script>\n\
         document.querySelector('.gancho1').click();\n\
         el.classList.add('gancho2', \"x\");\n\
         document.getElementsByClassName('gancho3');\n\
         el.className = 'gancho4 outro';\n\
         </script>",
    );
    assert_eq!(
        achados(&medir_html(dir.path(), &html)),
        vec![par("sem-gancho", 7)]
    );
}

#[test]
fn prefixo_js_e_convencao_de_gancho_e_nao_e_cobrado() {
    let dir = tempfile::tempdir().unwrap();
    let html = pagina(
        "<style>.a{}</style>",
        "<button class=\"a js-toggle js_open\"></button>",
    );
    assert!(medir_html(dir.path(), &html).achados.is_empty());
}

// ------------------------------------------------------------------ html: quando NÃO dá para medir

#[test]
fn tailwind_no_documento_e_nao_medido_nunca_chute() {
    let dir = tempfile::tempdir().unwrap();
    for (nome, html) in [
        (
            "play cdn",
            pagina(
                "<script src=\"https://cdn.tailwindcss.com\"></script>\n<style>.a{}</style>",
                "<p class=\"flex p-4 desconhecida\"></p>",
            ),
        ),
        (
            "@tailwind na folha",
            pagina(
                "<style>@tailwind base;\n.a{}</style>",
                "<p class=\"flex desconhecida\"></p>",
            ),
        ),
        (
            "@apply na folha",
            pagina(
                "<style>.a { @apply p-4; }</style>",
                "<p class=\"p-4 desconhecida\"></p>",
            ),
        ),
        (
            "unocss runtime",
            pagina(
                "<script src=\"https://cdn.jsdelivr.net/npm/@unocss/runtime\"></script>\n<style>.a{}</style>",
                "<p class=\"desconhecida\"></p>",
            ),
        ),
    ] {
        let v = medir_html(dir.path(), &html);
        assert!(v.achados.is_empty(), "{nome}: {:?}", achados(&v));
        let motivo = v
            .nao_medido
            .unwrap_or_else(|| panic!("{nome}: devia ser não medido"));
        assert!(
            motivo.to_lowercase().contains("tailwind") || motivo.to_lowercase().contains("unocss"),
            "{nome}: {motivo}"
        );
    }
}

#[test]
fn projeto_com_tailwind_na_pasta_do_documento_e_nao_medido() {
    let dir = tempfile::tempdir().unwrap();
    escreve(dir.path(), "tailwind.config.js", "module.exports = {}");
    let html = pagina("<style>.a{}</style>", "<p class=\"b\"></p>");
    let v = medir_html(dir.path(), &html);
    assert!(v.achados.is_empty());
    assert!(v.nao_medido.unwrap().contains("tailwind.config.js"));
}

#[test]
fn tailwind_de_outro_pacote_nao_contamina_um_html_fora_dele() {
    // a report under data/ sits outside frontend/ (which has the Tailwind package.json)
    let dir = tempfile::tempdir().unwrap();
    escreve(
        dir.path(),
        "frontend/package.json",
        r#"{"devDependencies": {"tailwindcss": "^3"}}"#,
    );
    let html = pagina("<style>.a{}</style>", "<p class=\"b\"></p>");
    let v = medir_isolado(&entrada(dir.path(), "data/relatorio.html", "html", &html));
    assert_eq!(achados(&v), vec![par("b", 7)]);
    assert!(v.nao_medido.is_none());
}

#[test]
fn package_json_com_tailwind_ou_css_in_js_no_pacote_do_documento_e_nao_medido() {
    for dep in [
        "tailwindcss",
        "styled-components",
        "@emotion/react",
        "unocss",
        "@stitches/react",
    ] {
        let dir = tempfile::tempdir().unwrap();
        escreve(
            dir.path(),
            "package.json",
            &format!(r#"{{"dependencies": {{"{dep}": "1.0.0"}}}}"#),
        );
        let html = pagina("<style>.a{}</style>", "<p class=\"b\"></p>");
        let v = medir_html(dir.path(), &html);
        assert!(v.achados.is_empty(), "{dep}");
        assert!(v.nao_medido.unwrap().contains(dep), "{dep}");
    }
}

#[test]
fn package_json_sem_gerador_nao_atrapalha() {
    let dir = tempfile::tempdir().unwrap();
    escreve(
        dir.path(),
        "package.json",
        r#"{"dependencies": {"react": "18"}}"#,
    );
    let html = pagina("<style>.a{}</style>", "<p class=\"b\"></p>");
    assert_eq!(achados(&medir_html(dir.path(), &html)), vec![par("b", 7)]);
}

#[test]
fn template_com_heranca_ou_include_e_nao_medido() {
    let dir = tempfile::tempdir().unwrap();
    for marca in [
        "{% extends \"base.html\" %}",
        "{% include 'estilos.html' %}",
        "<?php include 'estilos.php'; ?>",
        "@extends('layout')",
    ] {
        let html = pagina(
            &format!("<style>.a{{}}</style>\n{marca}"),
            "<p class=\"b\"></p>",
        );
        let v = medir_html(dir.path(), &html);
        assert!(v.achados.is_empty(), "{marca}");
        assert!(v.nao_medido.is_some(), "{marca}");
    }
}

#[test]
fn fragmento_sem_head_nem_html_e_nao_medido() {
    let dir = tempfile::tempdir().unwrap();
    let v = medir_html(dir.path(), "<style>.a{}</style><div class=\"b\">oi</div>");
    assert!(v.achados.is_empty());
    assert!(v.nao_medido.unwrap().contains("fragmento"));
}

#[test]
fn pagina_sem_nenhuma_folha_e_nao_medido_nao_ha_contra_o_que_medir() {
    let dir = tempfile::tempdir().unwrap();
    let html = pagina("<title>x</title>", "<p class=\"b\"></p>");
    let v = medir_html(dir.path(), &html);
    assert!(v.achados.is_empty());
    assert!(v.nao_medido.unwrap().contains("nenhuma folha"));
}

#[test]
fn folha_injetada_por_script_e_nao_medido() {
    let dir = tempfile::tempdir().unwrap();
    for js in [
        "const l = document.createElement('link'); document.head.append(l);",
        "document.styleSheets[0].insertRule('.x{}');",
        "document.adoptedStyleSheets = [new CSSStyleSheet()];",
    ] {
        let html = pagina(
            "<style>.a{}</style>",
            &format!("<p class=\"b\"></p><script>{js}</script>"),
        );
        let v = medir_html(dir.path(), &html);
        assert!(v.achados.is_empty(), "{js}");
        assert!(v.nao_medido.unwrap().contains("script"), "{js}");
    }
}

#[test]
fn atributo_com_marcador_de_template_e_pulado_e_o_resto_da_pagina_segue() {
    let dir = tempfile::tempdir().unwrap();
    let html = pagina(
        "<style>.a{}</style>",
        "<p class=\"a {{ cls }}\"></p>\n<p class=\"<?= $c ?>\"></p>\n<p class=\"x ${y}\"></p>\n<p class=\"{% if z %}b{% endif %}\"></p>\n<p class=\"de-verdade\"></p>",
    );
    assert_eq!(
        achados(&medir_html(dir.path(), &html)),
        vec![par("de-verdade", 11)]
    );
}

#[test]
fn documento_sem_classe_nenhuma_nem_olha_para_as_folhas() {
    let dir = tempfile::tempdir().unwrap();
    let html = pagina(
        "<link rel=\"stylesheet\" href=\"nao_existe.css\">",
        "<p>oi</p>",
    );
    let v = medir_html(dir.path(), &html);
    assert!(v.achados.is_empty());
    assert!(v.nao_medido.is_none(), "nada a medir não é 'não medido'");
}

#[test]
fn folha_via_overlay_vale_como_o_arquivo_do_instante() {
    let dir = tempfile::tempdir().unwrap();
    let html = pagina(
        "<link rel=\"stylesheet\" href=\"so_no_overlay.css\">",
        "<p class=\"vem-do-overlay outra\"></p>",
    );
    let mut input = entrada(dir.path(), "index.html", "html", &html);
    input.overlays.insert(
        dir.path()
            .join("so_no_overlay.css")
            .to_string_lossy()
            .into_owned(),
        ".vem-do-overlay { }".into(),
    );
    assert_eq!(achados(&medir_isolado(&input)), vec![par("outra", 7)]);
}

// ------------------------------------------------------------------ jsx / tsx

fn projeto_com_css() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    escreve(
        dir.path(),
        "src/app.css",
        ".btn { } .btn-primary { } .titulo { }",
    );
    dir
}

#[test]
fn jsx_classe_que_nenhuma_folha_do_projeto_define_e_achado() {
    let dir = projeto_com_css();
    let fonte =
        "export const A = () => (\n  <button className=\"btn btn-primry\">ok</button>\n);\n";
    let v = medir_jsx(dir.path(), "tsx", "src/A.tsx", fonte);
    assert_eq!(achados(&v), vec![par("btn-primry", 2)]);
    assert!(v.nao_medido.is_none(), "{:?}", v.nao_medido);
    assert!(v.achados[0].sugestao.contains("`btn-primary`"));
    assert_eq!(v.achados[0].alvo, Alvo::Projeto);
}

#[test]
fn jsx_aceita_literal_em_chaves_template_sem_substituicao_e_class() {
    let dir = projeto_com_css();
    let fonte = "const a = <i className={'novo1'} />;\n\
                 const b = <i className={\"novo2\"} />;\n\
                 const c = <i className={`novo3`} />;\n\
                 const d = <i class=\"novo4\" />;\n";
    for lang in ["tsx", "javascript"] {
        let v = medir_jsx(dir.path(), lang, "src/A.tsx", fonte);
        assert_eq!(
            achados(&v),
            vec![
                par("novo1", 1),
                par("novo2", 2),
                par("novo3", 3),
                par("novo4", 4)
            ],
            "{lang}"
        );
    }
}

#[test]
fn jsx_dinamico_e_pulado_sem_chute_e_sem_derrubar_o_resto() {
    let dir = projeto_com_css();
    let fonte = "const a = <i className={cls} />;\n\
                 const b = <i className={ativo ? 'on' : 'off'} />;\n\
                 const c = <i className={`ghost ${tam}`} />;\n\
                 const d = <i className={clsx('x', y)} />;\n\
                 const e = <i className={styles.caixa} />;\n\
                 const f = <i className=\"btn\" />;\n\
                 const g = <i className=\"so-este\" />;\n";
    let v = medir_jsx(dir.path(), "tsx", "src/A.tsx", fonte);
    assert_eq!(achados(&v), vec![par("so-este", 7)]);
    assert!(v.nao_medido.is_none(), "{:?}", v.nao_medido);
}

#[test]
fn jsx_classe_so_definida_em_css_module_nao_conta_como_global() {
    let dir = projeto_com_css();
    escreve(dir.path(), "src/Card.module.css", ".caixa { }");
    let v = medir_jsx(
        dir.path(),
        "tsx",
        "src/A.tsx",
        "import styles from './Card.module.css';\nconst a = <i className=\"caixa\" />;\n",
    );
    assert_eq!(achados(&v), vec![par("caixa", 2)]);
    assert!(
        v.nao_medido.is_none(),
        "o módulo resolveu: {:?}",
        v.nao_medido
    );
}

#[test]
fn jsx_import_de_css_que_nao_resolve_e_nao_medido() {
    let dir = projeto_com_css();
    for import in [
        "import './nao_existe.css';",
        "import 'pacote-nao-instalado/dist/x.css';",
        "import './tema.scss';",
        "import './tema.less';",
    ] {
        let fonte = format!("{import}\nconst a = <i className=\"qualquer\" />;\n");
        let v = medir_jsx(dir.path(), "tsx", "src/A.tsx", &fonte);
        assert!(v.achados.is_empty(), "{import}");
        assert!(v.nao_medido.is_some(), "{import}");
    }
}

#[test]
fn jsx_css_de_pacote_do_node_modules_conta_quando_esta_instalado() {
    let dir = projeto_com_css();
    escreve(
        dir.path(),
        "node_modules/lib-ui/dist/ui.css",
        ".ui-botao { }",
    );
    let fonte = "import 'lib-ui/dist/ui.css';\nconst a = <i className=\"ui-botao ui-botaoo\" />;\n";
    let v = medir_jsx(dir.path(), "tsx", "src/A.tsx", fonte);
    assert_eq!(achados(&v), vec![par("ui-botaoo", 2)]);
    assert!(v.nao_medido.is_none(), "{:?}", v.nao_medido);
}

#[test]
fn jsx_nao_varre_node_modules_nem_pastas_ocultas_nem_build() {
    let dir = projeto_com_css();
    escreve(
        dir.path(),
        "node_modules/x/estilo.css",
        ".so-no-node-modules { }",
    );
    escreve(dir.path(), ".oculta/estilo.css", ".so-na-oculta { }");
    escreve(dir.path(), "dist/estilo.css", ".so-no-dist { }");
    let fonte = "const a = <i className=\"so-no-node-modules so-na-oculta so-no-dist\" />;\n";
    let v = medir_jsx(dir.path(), "tsx", "src/A.tsx", fonte);
    assert_eq!(
        achados(&v),
        vec![
            par("so-no-node-modules", 1),
            par("so-na-oculta", 1),
            par("so-no-dist", 1)
        ]
    );
}

#[test]
fn jsx_projeto_sem_nenhuma_folha_e_nao_medido() {
    let dir = tempfile::tempdir().unwrap();
    let v = medir_jsx(
        dir.path(),
        "tsx",
        "src/A.tsx",
        "const a = <i className=\"qualquer\" />;\n",
    );
    assert!(v.achados.is_empty());
    assert!(v.nao_medido.unwrap().contains("nenhuma folha"));
}

#[test]
fn jsx_projeto_com_gerador_de_utilitarios_ou_css_in_js_e_nao_medido() {
    for (arquivo, conteudo, nome) in [
        (
            "tailwind.config.ts",
            "export default {}",
            "tailwind.config.ts",
        ),
        ("uno.config.ts", "export default {}", "uno.config.ts"),
        (
            "package.json",
            r#"{"dependencies":{"styled-components":"6"}}"#,
            "styled-components",
        ),
        (
            "package.json",
            r#"{"devDependencies":{"@tailwindcss/vite":"4"}}"#,
            "@tailwindcss/vite",
        ),
    ] {
        let dir = projeto_com_css();
        escreve(dir.path(), arquivo, conteudo);
        let v = medir_jsx(
            dir.path(),
            "tsx",
            "src/A.tsx",
            "const a = <i className=\"desconhecida\" />;\n",
        );
        assert!(v.achados.is_empty(), "{arquivo}");
        assert!(v.nao_medido.unwrap().contains(nome), "{arquivo}");
    }
}

#[test]
fn jsx_html_de_entrada_com_folha_remota_e_nao_medido() {
    let dir = projeto_com_css();
    escreve(
        dir.path(),
        "index.html",
        "<html><head><link rel=\"stylesheet\" href=\"https://cdn.exemplo.com/ui.css\"></head></html>",
    );
    let v = medir_jsx(
        dir.path(),
        "tsx",
        "src/A.tsx",
        "const a = <i className=\"desconhecida\" />;\n",
    );
    assert!(v.achados.is_empty());
    assert!(v.nao_medido.unwrap().contains("cdn.exemplo.com"));
}

#[test]
fn jsx_prefixo_js_e_nome_so_de_gancho_nao_sao_cobrados() {
    let dir = projeto_com_css();
    let v = medir_jsx(
        dir.path(),
        "tsx",
        "src/A.tsx",
        "const a = <i className=\"btn js-abrir\" />;\n",
    );
    assert!(v.achados.is_empty());
}

#[test]
fn jsx_arquivo_sem_classname_nem_toca_no_disco() {
    let dir = tempfile::tempdir().unwrap();
    let v = medir_jsx(dir.path(), "tsx", "src/A.tsx", "export const x = 1;\n");
    assert!(v.achados.is_empty());
    assert!(v.nao_medido.is_none());
}

// ------------------------------------------------------------------ folhas de CDN, pelo cache

const BOOTSTRAP: &str = "https://cdn.jsdelivr.net/npm/bootstrap@4.3.1/dist/css/bootstrap.min.css";

fn pagina_com_bootstrap(corpo: &str) -> String {
    pagina(
        &format!("<link rel=\"stylesheet\" href=\"{BOOTSTRAP}\">\n<style>.local {{ }}</style>"),
        corpo,
    )
}

#[test]
fn folha_de_cdn_no_cache_com_classes_define_e_o_typo_e_achado() {
    let dir = tempfile::tempdir().unwrap();
    let html = pagina_com_bootstrap("<p class=\"btn btn-primry container local\"></p>");
    let cache = cache_com_css(BOOTSTRAP, &["btn", "btn-primary", "container"]);
    let v = ClasseCss.medir_com_cache(&entrada(dir.path(), "index.html", "html", &html), cache);
    assert_eq!(achados(&v), vec![par("btn-primry", 8)]);
    assert!(v.nao_medido.is_none(), "{:?}", v.nao_medido);
    assert!(v.achados[0].sugestao.contains("`btn-primary`"));
}

#[test]
fn folha_de_cdn_fora_do_cache_e_nao_medido_e_nomeia_o_resolvedor() {
    let dir = tempfile::tempdir().unwrap();
    let html = pagina_com_bootstrap("<p class=\"btn\"></p>");
    let v = medir_isolado(&entrada(dir.path(), "index.html", "html", &html));
    assert!(v.achados.is_empty());
    let motivo = v.nao_medido.unwrap();
    assert!(motivo.contains(BOOTSTRAP), "{motivo}");
    assert!(
        motivo.contains("vgp::lang::cdn::resolvedor::preencher"),
        "{motivo}"
    );
}

#[test]
fn folha_de_cdn_que_o_cache_conhece_sem_classes_ou_com_404_e_nao_medido() {
    let dir = tempfile::tempdir().unwrap();
    let html = pagina_com_bootstrap("<p class=\"btn\"></p>");
    let sem_classes = {
        let mut c = Cache::default();
        c.inserir(BOOTSTRAP, Registro::novo(Veredito::Existe));
        Arc::new(c)
    };
    let v = ClasseCss.medir_com_cache(
        &entrada(dir.path(), "index.html", "html", &html),
        sem_classes,
    );
    assert!(v.achados.is_empty());
    assert!(v.nao_medido.unwrap().contains("não tem as classes"));

    let morta = {
        let mut c = Cache::default();
        c.inserir(BOOTSTRAP, Registro::novo(Veredito::NaoExiste));
        Arc::new(c)
    };
    let v = ClasseCss.medir_com_cache(&entrada(dir.path(), "index.html", "html", &html), morta);
    assert!(v.achados.is_empty());
    assert!(v.nao_medido.unwrap().contains("404"));
}

#[test]
fn padroes_de_atributo_da_folha_de_cdn_tambem_definem() {
    let dir = tempfile::tempdir().unwrap();
    let html = pagina_com_bootstrap("<i class=\"icon-home fora\"></i>");
    let mut cache = Cache::default();
    cache.inserir(
        BOOTSTRAP,
        Registro {
            classes: Some(Vec::new()),
            padroes: vec![("^=".into(), "icon-".into())],
            ..Registro::novo(Veredito::Existe)
        },
    );
    let v = ClasseCss.medir_com_cache(
        &entrada(dir.path(), "index.html", "html", &html),
        Arc::new(cache),
    );
    assert_eq!(achados(&v), vec![par("fora", 8)]);
}

#[test]
fn import_absoluto_de_cdn_dentro_de_folha_local_segue_o_cache_e_o_relativo_nao_resolve() {
    let dir = tempfile::tempdir().unwrap();
    escreve(
        dir.path(),
        "site.css",
        &format!("@import url(\"{BOOTSTRAP}\");\n.local {{ }}"),
    );
    let html = pagina(
        "<link rel=\"stylesheet\" href=\"site.css\">",
        "<p class=\"btn local ausente\"></p>",
    );
    let cache = cache_com_css(BOOTSTRAP, &["btn"]);
    let v = ClasseCss.medir_com_cache(&entrada(dir.path(), "index.html", "html", &html), cache);
    assert_eq!(achados(&v), vec![par("ausente", 7)]);

    // a relative @import INSIDE a remote sheet is relative to its URL: not resolved in layer 1
    let mut cache = Cache::default();
    cache.inserir(
        BOOTSTRAP,
        Registro {
            classes: Some(vec!["btn".into()]),
            imports: vec!["./outra.css".into()],
            ..Registro::novo(Veredito::Existe)
        },
    );
    let v = ClasseCss.medir_com_cache(
        &entrada(dir.path(), "index.html", "html", &html),
        Arc::new(cache),
    );
    assert!(v.achados.is_empty());
    assert!(v.nao_medido.unwrap().contains("relativo"));
}

#[test]
fn ler_folha_traz_classes_ordenadas_padroes_e_imports_para_o_cache() {
    let folha = ler_folha(
        "@import \"base.css\"; .b { } .a:hover { } [class^=\"x-\"] { } /* @import \"comentado.css\"; */",
    )
    .unwrap();
    assert_eq!(folha.classes, vec!["a".to_string(), "b".to_string()]);
    assert_eq!(folha.padroes, vec![("^=".to_string(), "x-".to_string())]);
    assert_eq!(folha.imports, vec!["base.css".to_string()]);
}

#[test]
fn folha_com_erro_de_sintaxe_nao_perde_classe_definida_a_rede_lexica_cobre() {
    // a rule the grammar cannot close must not make `.depois` look undefined
    let dir = tempfile::tempdir().unwrap();
    let html = pagina(
        "<style>.antes { color: ; } .@@@ { } .depois { }</style>",
        "<p class=\"antes depois\"></p>",
    );
    assert!(medir_html(dir.path(), &html).achados.is_empty());
}

// ------------------------------------------------------------------ o registro

#[test]
fn o_transversal_cobre_html_tsx_e_javascript_e_nao_ts_puro() {
    let mut langs = ClasseCss.langs().to_vec();
    langs.sort_unstable();
    assert_eq!(langs, vec!["html", "javascript", "tsx"]);
    assert_eq!(ClasseCss.nome(), "classe_css");
}

#[test]
fn linguagem_fora_do_alcance_nao_mede_nada() {
    let dir = tempfile::tempdir().unwrap();
    let v = medir_isolado(&entrada(dir.path(), "a.py", "python", "class = 'x'"));
    assert!(v.achados.is_empty());
    assert!(v.nao_medido.is_none());
}

// ------------------------------------------------------------------ peças puras

#[test]
fn distancia_de_edicao_e_a_de_levenshtein() {
    for (a, b, d) in [
        ("", "", 0),
        ("meta", "metaq", 1),
        ("kitten", "sitting", 3),
        ("abc", "", 3),
        ("flaw", "lawn", 2),
        ("áé", "ae", 2),
    ] {
        assert_eq!(distancia(a, b), d, "{a} {b}");
    }
}

#[test]
fn desescapa_traduz_escapes_de_seletor_css() {
    for (css, classe) in [
        ("sm\\:flex", "sm:flex"),
        ("w-1\\/2", "w-1/2"),
        ("\\31 0", "10"),
        ("a\\.b", "a.b"),
        ("simples", "simples"),
    ] {
        assert_eq!(folhas::desescapa(css), classe, "{css}");
    }
}

#[test]
fn classe_so_lexica_conta_quando_a_gramatica_de_css_engasga() {
    // lixo no meio da folha: a gramática perde ou engole seletores; a rede léxica (`.nome`) segura
    let quebrados = [
        ".ok { } @@@ .so-lexico { }",
        ".ok { color: red; } !!! .so-lexico { }",
        ".ok { } ;;; ::: .so-lexico { }",
        ".ok { } }} .so-lexico { }",
        ".ok { } @media ( { .so-lexico { } }",
        ".ok { } <<<< .so-lexico { }",
        ".ok { content: \"aberta\n} .so-lexico { }",
        ".ok { width: calc(1px + ; }\n.so-lexico { }",
        ".ok { } @unknown-rule ((( ; .so-lexico { }",
        ".ok { } .tmp\\ .so-lexico { }",
        ".ok { background: url(\"x.png }\n.so-lexico { }",
        ".ok { } %%% { } .so-lexico { }",
    ];
    for css in quebrados {
        let dir = tempfile::tempdir().unwrap();
        let html = pagina(
            &format!("<style>{css}</style>"),
            "<p class=\"ok so-lexico\"></p>",
        );
        let v = medir_html(dir.path(), &html);
        assert!(v.achados.is_empty(), "`{css}`: {:?}", achados(&v));
    }
}

// ------------------------------------------------------------------ saída de editor rico (contenteditable)

const FOLHA_DO_DOC: &str = "<style>.Texto { }</style>";

/// Documento exportado de um editor rico: os elementos que o editor injeta levam `contenteditable`
/// e as classes dele (`minutaAncora`, `ancoraSei`) são estilizadas pela folha do EDITOR, que o
/// arquivo exportado não traz.
fn documento_de_editor() -> String {
    pagina(
        FOLHA_DO_DOC,
        "<p class=\"Texto\"><span class=\"minutaAncora\" contenteditable=\"false\">\
         <a class=\"ancoraSei\" contenteditable=\"false\">1</a></span></p>",
    )
}

#[test]
fn documento_de_editor_rico_com_classe_sem_folha_se_abstem_e_diz_porque() {
    let dir = tempfile::tempdir().unwrap();
    let v = medir_html(dir.path(), &documento_de_editor());
    assert!(v.achados.is_empty(), "{:?}", achados(&v));
    let motivo = v
        .nao_medido
        .expect("a abstenção tem de aparecer, nunca sumir");
    assert!(
        motivo.contains(
            "classes de saída de editor rico (contenteditable): a folha do editor não está no arquivo"
        ),
        "{motivo}"
    );
    // a abstenção é um número: quantas classes ficaram sem avaliação, e quais
    assert!(motivo.contains("2 classe(s)"), "{motivo}");
    assert!(
        motivo.contains("minutaAncora") && motivo.contains("ancoraSei"),
        "{motivo}"
    );
}

#[test]
fn sem_contenteditable_a_classe_sem_folha_segue_avisando() {
    let dir = tempfile::tempdir().unwrap();
    let html = pagina(
        FOLHA_DO_DOC,
        "<p class=\"Texto\"><span class=\"minutaAncora\"><a class=\"ancoraSei\">1</a></span></p>",
    );
    let v = medir_html(dir.path(), &html);
    assert_eq!(
        achados(&v),
        vec![par("minutaAncora", 7), par("ancoraSei", 7)]
    );
    assert!(v.nao_medido.is_none(), "{:?}", v.nao_medido);
}

#[test]
fn documento_de_editor_sem_classe_indefinida_nao_se_abstem() {
    let dir = tempfile::tempdir().unwrap();
    let html = pagina(
        FOLHA_DO_DOC,
        "<p class=\"Texto\" contenteditable=\"true\">texto</p>",
    );
    let v = medir_html(dir.path(), &html);
    assert!(v.achados.is_empty());
    assert!(
        v.nao_medido.is_none(),
        "só se abstém onde avisaria: {:?}",
        v.nao_medido
    );
}

#[test]
fn contenteditable_vale_em_qualquer_caixa_e_sem_valor() {
    let dir = tempfile::tempdir().unwrap();
    let html = pagina(
        FOLHA_DO_DOC,
        "<div CONTENTEDITABLE class=\"naoexiste\">a</div>",
    );
    let v = medir_html(dir.path(), &html);
    assert!(v.achados.is_empty(), "{:?}", achados(&v));
    assert!(v.nao_medido.is_some());
}

#[test]
fn a_abstencao_e_por_documento_um_typo_fora_da_subarvore_editavel_tambem_se_cala() {
    // limite MEDIDO da regra por documento (a variante por subárvore deixaria 36 avisos nos
    // negativos reais porque `dark-mode-color-*` está fora de qualquer elemento editável)
    let dir = tempfile::tempdir().unwrap();
    let html = pagina(
        FOLHA_DO_DOC,
        "<div contenteditable=\"true\"><b class=\"Texto\">a</b></div><p class=\"Textoq\">fora</p>",
    );
    let v = medir_html(dir.path(), &html);
    assert!(v.achados.is_empty(), "{:?}", achados(&v));
    assert!(v.nao_medido.is_some());
}
