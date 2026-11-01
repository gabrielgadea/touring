//! Class `ancora` from the inline `<script>`: `getElementById` / `querySelector('#x')`, and the
//! ids a script creates.

use super::*;

#[test]
fn script_get_element_by_id_ausente_e_incerto_e_presente_nao() {
    let s = Site::novo();
    let v = s.mede(
        "a.html",
        &com_script(
            "var a = document.getElementById('painel');",
            "<div id=\"outro\"></div>",
        ),
    );
    ve(&v, "ancora", &["painel"]);
    assert_eq!(v.achados[0].alvo, Alvo::Incerto);
    assert!(v.achados[0].sugestao.contains("getElementById('painel')"));
    limpo(&s.mede(
        "a.html",
        &com_script(
            "document.getElementById('painel').focus();",
            "<div id=\"painel\"></div>",
        ),
    ));
}

#[test]
fn script_linha_do_lookup_e_absoluta_no_documento() {
    let s = Site::novo();
    // linha 7 = <div>, 8 = <script>, 9 = primeira linha do JS, 10 = a chamada
    let v = s.mede(
        "a.html",
        &com_script("var ok = 1;\ndocument.getElementById('x');", "<div></div>"),
    );
    assert_eq!(v.achados[0].linha, 10);
}

#[test]
fn script_query_selector_checa_so_ids_obrigatorios_do_seletor() {
    let s = Site::novo();
    let js = "document.querySelector('#a .classe');\
              document.querySelectorAll('div#b > p');\
              document.querySelector('#c, #d');\
              document.querySelector(':not(#e)');\
              document.querySelector('[data-x=\"#f\"]');\
              document.querySelector('.so-classe');";
    let v = s.mede("a.html", &com_script(js, "<p></p>"));
    ve(&v, "ancora", &["a", "b"]);
    limpo(&s.mede(
        "a.html",
        &com_script("document.querySelector('#a .k');", "<div id=\"a\"></div>"),
    ));
}

#[test]
fn script_so_olha_document_e_literais() {
    let s = Site::novo();
    let js = "el.querySelector('#nada1');\
              shadow.getElementById('nada2');\
              document.getElementById(nome);\
              document.getElementById(`x${n}`);\
              document.getElementById('a\\u0062');\
              $('#nada3');";
    limpo(&s.mede("a.html", &com_script(js, "<p></p>")));
    let v = s.mede(
        "a.html",
        &com_script("window.document.getElementById('w');", "<p></p>"),
    );
    ve(&v, "ancora", &["w"]);
}

#[test]
fn script_id_criado_pelo_proprio_script_nao_e_ausente() {
    let s = Site::novo();
    for js in [
        "var d = document.createElement('div'); d.id = 'novo'; document.getElementById('novo');",
        "d.setAttribute('id', 'novo'); document.getElementById('novo');",
        "box.innerHTML = '<span id=\"novo\"></span>'; document.getElementById('novo');",
        "var cfg = { id: 'novo' }; document.getElementById('novo');",
    ] {
        limpo(&s.mede("a.html", &com_script(js, "<p></p>")));
    }
    // e o href para um id criado por script também passa
    limpo(&s.mede(
        "a.html",
        &com_script("d.id = 'novo';", "<a href=\"#novo\">n</a>"),
    ));
}

#[test]
fn script_template_html_declara_ids_e_json_nao() {
    let s = Site::novo();
    let tpl = pagina(
        "<script type=\"text/template\"><div id=\"linha\"></div></script>\
         <a href=\"#linha\">l</a>",
    );
    limpo(&s.mede("a.html", &tpl));
    let json = pagina(
        "<script type=\"application/json\">{\"id\": \"linha\", \"x\": \"document.getElementById('z')\"}</script>\
         <a href=\"#linha\">l</a>",
    );
    ve(&s.mede("a.html", &json), "ancora", &["linha"]);
}

#[test]
fn script_cria_id_por_dado_dinamico_desliga_lookups_e_avisa_na_ancora() {
    let s = Site::novo();
    for js in [
        "el.id = prefixo + n; document.getElementById('x');",
        "el.setAttribute('id', gerar()); document.getElementById('x');",
        "box.innerHTML = montar(); document.getElementById('x');",
        "document.write(html); document.getElementById('x');",
        "box.insertAdjacentHTML('beforeend', montar()); document.getElementById('x');",
    ] {
        let v = s.mede("a.html", &com_script(js, "<p></p>"));
        ve(&v, "ancora", &[]);
        assert!(motivo(&v).contains("dinâmicos"), "{js}: {:?}", v.nao_medido);
    }
    // o href de fragmento continua julgado, como Incerto, e a sugestão avisa que o script pode
    // criar o id
    let v = s.mede(
        "a.html",
        &com_script("el.id = n;", "<a href=\"#falta\">f</a>"),
    );
    ve(&v, "ancora", &["falta"]);
    assert_eq!(v.achados[0].alvo, Alvo::Incerto);
    assert!(
        v.achados[0].sugestao.contains("script"),
        "{}",
        v.achados[0].sugestao
    );
}

#[test]
fn script_com_erro_de_sintaxe_e_nao_medido() {
    let s = Site::novo();
    let v = s.mede(
        "a.html",
        &com_script(
            "document.getElementById('x'; ((",
            "<a href=\"#falta\">f</a>",
        ),
    );
    assert!(motivo(&v).contains("erro de sintaxe"), "{:?}", v.nao_medido);
    assert!(!v.achados.iter().any(|f| f.nome == "x"));
    let falta = v
        .achados
        .iter()
        .find(|f| f.nome == "falta")
        .expect("o href continua medido");
    assert_eq!(
        falta.alvo,
        Alvo::Incerto,
        "um script ilegível pode criar o id"
    );
    assert!(
        falta.sugestao.contains("script"),
        "um script ilegível pode criar o id: {}",
        falta.sugestao
    );
}

#[test]
fn script_com_src_ignora_o_corpo_e_tipo_desconhecido_e_dinamico() {
    let s = Site::novo();
    let v = s.mede(
        "a.html",
        &pagina("<script src=\"https://cdn.x/a.js\">document.getElementById('x');</script>"),
    );
    ve(&v, "ancora", &[]);
    let v = s.mede(
        "a.html",
        &pagina("<script type=\"text/coffeescript\">$('#x')</script><a href=\"#falta\">f</a>"),
    );
    assert_eq!(v.achados[0].alvo, Alvo::Incerto);
    assert!(v.achados[0].sugestao.contains("script"));
}

#[test]
fn script_o_corpo_de_style_nao_e_javascript() {
    let s = Site::novo();
    // o `raw_text` de <style> é CSS: lido como JS daria erro de sintaxe, um motivo de `nao_medido`
    // e o achado rebaixado a `Incerto` com a nota de script na sugestão
    let v = s.mede(
        "a.html",
        &pagina("<style>body { color: red; }</style><a href=\"#falta\">f</a>"),
    );
    ve(&v, "ancora", &["falta"]);
    assert_eq!(v.achados[0].alvo, Alvo::Projeto);
    assert!(!v.achados[0].sugestao.contains("script"));
    assert_eq!(v.nao_medido, None);
}

#[test]
fn script_mesmo_id_procurado_duas_vezes_e_no_href_vira_um_achado() {
    let s = Site::novo();
    let v = s.mede(
        "a.html",
        &com_script(
            "document.getElementById('x');\ndocument.getElementById('x');",
            "<a href=\"#x\">x</a>",
        ),
    );
    ve(&v, "ancora", &["x"]);
}
