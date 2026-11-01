//! Class `ancora`: ids referenced and missing, ids declared twice.

use super::*;

#[test]
fn ancora_href_para_id_ausente_e_achado_e_presente_nao() {
    let s = Site::novo();
    let v = s.mede(
        "a.html",
        &pagina("<a href=\"#capa\">c</a>\n<h1 id=\"capitulo\">x</h1>"),
    );
    ve(&v, "ancora", &["capa"]);
    assert_eq!(v.achados[0].linha, 7);
    assert_eq!(
        v.achados[0].alvo,
        Alvo::Projeto,
        "o disco decide: o id é do PROJETO e não existe (o warn vem da classe consultiva)"
    );
    assert_eq!(v.nao_medido, None);
    limpo(&s.mede(
        "a.html",
        &pagina("<a href=\"#capa\">c</a><h1 id=\"capa\">x</h1>"),
    ));
}

#[test]
fn ancora_aceita_a_name_e_conta_id_com_caixa_exata() {
    let s = Site::novo();
    limpo(&s.mede(
        "a.html",
        &pagina("<a href=\"#velha\">v</a><a name=\"velha\"></a>"),
    ));
    let v = s.mede(
        "a.html",
        &pagina("<a href=\"#Foo\">f</a><p id=\"foo\"></p>"),
    );
    ve(&v, "ancora", &["Foo"]);
    assert!(
        v.achados[0].sugestao.contains("`foo`"),
        "{}",
        v.achados[0].sugestao
    );
}

#[test]
fn ancora_fragmento_percentual_casa_com_id_decodificado() {
    let s = Site::novo();
    limpo(&s.mede(
        "a.html",
        &pagina("<a href=\"#caf%C3%A9\">c</a><p id=\"café\"></p>"),
    ));
}

#[test]
fn ancora_label_for_e_aria_checam_cada_id_da_lista() {
    let s = Site::novo();
    let v = s.mede(
        "a.html",
        &pagina(
            "<label for=\"nome\">n</label><input id=\"nome\">\
             <label for=\"fantasma\">f</label>\
             <div aria-labelledby=\"t1 t2\" aria-describedby=\"d1\"></div>\
             <h2 id=\"t1\"></h2><p id=\"d1\"></p>",
        ),
    );
    ve(&v, "ancora", &["fantasma", "t2"]);
}

#[test]
fn ancora_use_do_svg_aponta_para_id() {
    let s = Site::novo();
    let v = s.mede(
        "a.html",
        &pagina(
            "<svg><use href=\"#ico\"/><use xlink:href=\"#ico2\"/></svg>\
             <svg><symbol id=\"ico\"></symbol></svg>",
        ),
    );
    ve(&v, "ancora", &["ico2"]);
}

#[test]
fn ancora_navegacao_especial_nao_e_id() {
    let s = Site::novo();
    let corpo = "<a href=\"#\">a</a><a href=\"#top\">t</a><a href=\"#TOP\">t</a>\
        <a href=\"#!/rota\">r</a><a href=\"#/rota\">r</a><a href=\"#:~:text=abc\">x</a>\
        <a href=\"outra.html#sec\">o</a>";
    let v = s.mede("a.html", &pagina(corpo));
    ve(&v, "ancora", &[]);
}

#[test]
fn ancora_id_so_em_comentario_ou_template_conta_como_existente() {
    let s = Site::novo();
    // conservador por desenho: o `id=` lexical em comentário evita um falso "ausente"
    limpo(&s.mede(
        "a.html",
        &pagina("<a href=\"#c\">c</a><!-- <div id=\"c\"></div> -->"),
    ));
    limpo(&s.mede(
        "a.html",
        &pagina("<a href=\"#t\">t</a><template><p id=\"t\"></p></template>"),
    ));
}

#[test]
fn ancora_id_duplicado_e_incerto_e_ignora_template() {
    let s = Site::novo();
    let v = s.mede(
        "a.html",
        &pagina("<p id=\"x\"></p>\n<p id=\"x\"></p>\n<p id=\"y\"></p>"),
    );
    ve(&v, "ancora", &["x"]);
    assert_eq!(
        v.achados[0].alvo,
        Alvo::Incerto,
        "duplicado é lint: nunca deny"
    );
    assert_eq!(v.achados[0].linha, 8, "a linha da segunda ocorrência");
    assert!(v.achados[0].sugestao.contains("duplicado"));
    // dentro de <template> o conteúdo é inerte: repetir o id lá e no documento é válido
    limpo(&s.mede(
        "a.html",
        &pagina("<p id=\"x\"></p><template><p id=\"x\"></p></template>"),
    ));
    // `<template id>` não está DENTRO de um template
    let v = s.mede(
        "a.html",
        &pagina("<template id=\"r\"></template><template id=\"r\"></template>"),
    );
    ve(&v, "ancora", &["r"]);
}

#[test]
fn ancora_nunca_nega_em_html_nem_onde_o_codigo_roda_agora() {
    // spec §5.1: o deny de «roda agora» é das classes de SÍMBOLO; em HTML a próxima escrita cria
    // o id (o `index.html` sai antes do arquivo que o declara). Quem rebaixa é a política
    // (`CLASSES_CONSULTIVAS`), não o rótulo do adaptador: o alvo segue `Projeto`
    let html = pagina("<a href=\"#capa\">c</a>");
    for caminho in [
        "/home/u/proj/a.html",
        "/tmp/vhtml-x/a.html",
        "/home/u/proj/.cache/a.html",
        "<string>",
    ] {
        assert_eq!(julga(caminho, &html).verdict, "warn", "{caminho}");
    }
    let duplicado = pagina("<p id=\"x\"></p><p id=\"x\"></p>");
    assert_eq!(julga("/tmp/vhtml-x/a.html", &duplicado).verdict, "warn");
}

#[test]
fn ancora_script_externo_ou_dinamico_vira_nota_na_sugestao() {
    let s = Site::novo();
    let sem_script = s.mede("a.html", &pagina("<a href=\"#capa\">c</a>"));
    assert!(!sem_script.achados[0].sugestao.contains("script"));
    assert_eq!(sem_script.achados[0].alvo, Alvo::Projeto);
    let externo = s.mede(
        "a.html",
        &pagina("<script src=\"https://cdn.x/app.js\"></script><a href=\"#capa\">c</a>"),
    );
    assert_eq!(
        externo.achados[0].alvo,
        Alvo::Incerto,
        "um script externo pode criar o id: incerteza de verdade"
    );
    assert!(
        externo.achados[0].sugestao.contains("script"),
        "um script externo pode criar o id: {}",
        externo.achados[0].sugestao
    );
}

#[test]
fn ancora_pagina_que_herda_ou_inclui_nao_e_medida() {
    let s = Site::novo();
    for marca in [
        "{% extends 'base.html' %}",
        "{% include 'topo.html' %}",
        "<?php include 'a.php'; ?>",
        "<!--#include virtual=\"a.html\" -->",
    ] {
        let v = s.mede(
            "a.html",
            &pagina(&format!("{marca}<a href=\"#capa\">c</a>")),
        );
        ve(&v, "ancora", &[]);
        assert!(v.nao_medido.is_some(), "{marca}");
    }
}

#[test]
fn ancora_fragmento_sem_pagina_nao_e_medido_mas_pagina_com_body_e() {
    let s = Site::novo();
    let v = s.mede("parcial.html", "<nav><a href=\"#secao\">s</a></nav>\n");
    ve(&v, "ancora", &[]);
    assert!(motivo(&v).contains("fragmento"), "{:?}", v.nao_medido);
    let v = s.mede(
        "p.html",
        "<html><body><a href=\"#secao\">s</a></body></html>",
    );
    ve(&v, "ancora", &["secao"]);
}

#[test]
fn ancora_ids_de_template_e_referencia_de_template_nao_sao_medidos() {
    let s = Site::novo();
    let v = s.mede(
        "a.html",
        &pagina("<h2 id=\"sec-{{ n }}\">x</h2><a href=\"#sec-1\">1</a>"),
    );
    ve(&v, "ancora", &[]);
    assert!(v.nao_medido.is_some());
    let v = s.mede(
        "a.html",
        &pagina("<a href=\"#sec-{{ n }}\">1</a><a href=\"#falta\">f</a>"),
    );
    ve(&v, "ancora", &["falta"]);
    assert!(v.nao_medido.is_some());
}

#[test]
fn ancora_base_href_tira_o_direito_de_julgar_href_de_fragmento() {
    let s = Site::novo();
    let html = "<!DOCTYPE html><html><head><base href=\"/app/\"></head><body>\
                <a href=\"#capa\">c</a><label for=\"nada\">n</label></body></html>";
    ve(&s.mede("a.html", html), "ancora", &["nada"]);
}

#[test]
fn ancora_sugere_o_id_parecido() {
    let s = Site::novo();
    let corpo =
        "<a href=\"#conclusoa\">i</a><h1 id=\"conclusao\">x</h1><h1 id=\"introducao\">y</h1>";
    let v = s.mede("a.html", &pagina(corpo));
    assert!(
        v.achados[0]
            .sugestao
            .contains("Você quis dizer `conclusao`"),
        "{}",
        v.achados[0].sugestao
    );
    assert!(v.achados[0].reais.contains(&"conclusao".to_string()));
}

#[test]
fn ancora_repetida_vira_um_achado() {
    let s = Site::novo();
    let corpo = "<a href=\"#f\">1</a>\n<a href=\"#f\">2</a><label for=\"f\">3</label>";
    let v = s.mede("a.html", &pagina(corpo));
    ve(&v, "ancora", &["f"]);
    assert!(
        v.achados[0].sugestao.contains("mais 2"),
        "{}",
        v.achados[0].sugestao
    );
}

#[test]
fn ancora_label_for_de_template_e_nao_medido_e_o_resto_continua() {
    // só a classe `ancora` vê o `for=`: o motivo precisa ser dela
    let s = Site::novo();
    let v = s.mede(
        "a.html",
        &pagina("<label for=\"campo-{{ n }}\">c</label><a href=\"#falta\">f</a>"),
    );
    ve(&v, "ancora", &["falta"]);
    assert!(
        motivo(&v).contains("id com valor de template"),
        "{:?}",
        v.nao_medido
    );
}
