//! Class `recurso`: local `src` / `href` that do not exist on disk.

use std::collections::HashMap;

use super::*;

/// V-4 (31/10/2026): a referência RELATIVA que sai das raízes permitidas não é um
/// arquivo faltando — a classe se abstém, e o caminho não é ecoado.
#[test]
fn uma_referencia_fora_das_raizes_e_recusada_sem_o_caminho() {
    let s = Site::novo();
    let v = s.mede(
        "index.html",
        &pagina("<img src=\"../../../../etc/passwd\">"),
    );
    assert!(v.achados.is_empty(), "achados inesperados: {:?}", v.achados);
    let motivo = v.nao_medido.expect("a classe se abstém");
    assert!(motivo.contains("raízes permitidas"), "{motivo}");
    assert!(
        !motivo.contains("passwd"),
        "o caminho não é ecoado: {motivo}"
    );
}

#[test]
fn recurso_relativo_inexistente_e_achado_com_linha_e_nunca_nega() {
    let s = Site::novo();
    let v = s.mede("index.html", &pagina("<img src=\"img/logo.png\">"));
    ve(&v, "recurso", &["img/logo.png"]);
    let f = &v.achados[0];
    assert_eq!(f.linha, 7);
    assert_eq!(
        f.alvo,
        Alvo::Projeto,
        "o disco decide: o arquivo é do PROJETO e não existe (o warn vem da classe consultiva)"
    );
    assert!(f.sugestao.contains("img/logo.png"), "{}", f.sugestao);
    assert_eq!(v.nao_medido, None);
}

#[test]
fn recurso_existente_passa_limpo() {
    let s = Site::novo();
    s.escreve("img/logo.png", "x");
    limpo(&s.mede("index.html", &pagina("<img src=\"img/logo.png\">")));
}

#[test]
fn recurso_ignora_query_e_fragmento_mas_nao_o_arquivo() {
    let s = Site::novo();
    s.escreve("a.png", "x");
    s.escreve("app.js", "x");
    let v = s.mede(
        "index.html",
        &pagina("<img src=\"a.png?v=3#x\"><script src=\"app.js?t=1\"></script>"),
    );
    limpo(&v);
    let v = s.mede("index.html", &pagina("<img src=\"b.png?v=3#x\">"));
    ve(&v, "recurso", &["b.png"]);
}

#[test]
fn recurso_decodifica_percentual_e_entidades() {
    let s = Site::novo();
    s.escreve("meu arquivo.png", "x");
    s.escreve("a&b.png", "x");
    let v = s.mede(
        "index.html",
        &pagina("<img src=\"meu%20arquivo.png\"><img src=\"a&amp;b.png\">"),
    );
    limpo(&v);
    let v = s.mede("index.html", &pagina("<img src=\"outro%20arquivo.png\">"));
    ve(&v, "recurso", &["outro%20arquivo.png"]);
}

#[test]
fn recurso_resolve_relativo_ao_arquivo_e_nao_a_raiz_do_projeto() {
    let s = Site::novo();
    s.escreve("css/a.css", "x");
    let html = pagina("<link rel=\"stylesheet\" href=\"../css/a.css\">");
    limpo(&s.mede("pages/index.html", &html));
    // a mesma referência, de um arquivo na raiz, sobe para fora do projeto: não existe lá
    let v = s.mede("index.html", &html);
    ve(&v, "recurso", &["../css/a.css"]);
    // `css/a.css` só existe a partir da raiz, não a partir de `pages/`
    let v = s.mede(
        "pages/index.html",
        &pagina("<link rel=\"stylesheet\" href=\"css/a.css\">"),
    );
    ve(&v, "recurso", &["css/a.css"]);
}

#[test]
fn recurso_raiz_relativo_procura_ancestrais_e_raizes_web_e_erra_como_incerto() {
    let s = Site::novo();
    s.escreve("site/css/a.css", "x");
    s.escreve("public/img/b.png", "x");
    s.escreve("site/pages/deep/c.js", "x");
    let corpo = "<link rel=\"stylesheet\" href=\"/css/a.css\"><img src=\"/img/b.png\">\
                 <script src=\"/pages/deep/c.js\"></script>";
    // `site/` é a raiz web do documento (um ancestral); `public/` é uma raiz web do projeto
    limpo(&s.mede("site/pages/deep/index.html", &pagina(corpo)));
    let v = s.mede(
        "site/pages/deep/index.html",
        &pagina("<img src=\"/nada/aqui.png\">"),
    );
    ve(&v, "recurso", &["/nada/aqui.png"]);
    assert_eq!(
        v.achados[0].alvo,
        Alvo::Incerto,
        "onde a página é servida o arquivo não diz: nunca deny"
    );
}

#[test]
fn recurso_absoluto_do_sistema_de_arquivos_conta() {
    let s = Site::novo();
    s.escreve("assets/x.png", "x");
    let abs = s.caminho("assets/x.png");
    limpo(&s.mede("index.html", &pagina(&format!("<img src=\"{abs}\">"))));
}

#[test]
fn recurso_fora_de_escopo_nao_vira_achado_nem_nao_medido() {
    let s = Site::novo();
    let corpo = "<img src=\"http://x.org/a.png\"><script src=\"https://cdn.x/a.js\"></script>\
        <script src=\"//cdn.x/b.js\"></script><img src=\"data:image/png;base64,AAAA\">\
        <a href=\"mailto:a@b.c\">m</a><a href=\"tel:123\">t</a><a href=\"javascript:void(0)\">j</a>\
        <iframe src=\"about:blank\"></iframe><img src=\"\"><img src=\"   \"><a href=\"?q=1\">q</a>\
        <img src=\"?v=1\">";
    limpo(&s.mede("index.html", &pagina(corpo)));
}

#[test]
fn recurso_dinamico_ou_de_template_e_nao_medido_com_o_motivo() {
    let s = Site::novo();
    for ref_ in [
        "{{ url_for('static', filename='a.png') }}",
        "{% static 'a.png' %}",
        "<%= asset('a.png') %>",
        "<?php echo $x ?>",
        "${base}/a.png",
        "#{img}",
    ] {
        let v = s.mede("index.html", &pagina(&format!("<img src=\"{ref_}\">")));
        assert!(v.achados.is_empty(), "{ref_}: {:?}", v.achados);
        assert!(
            motivo(&v).contains("dinâmico"),
            "{ref_}: motivo {:?}",
            v.nao_medido
        );
    }
    // um recurso real quebrado ao lado de um dinâmico: o achado vale e o motivo viaja junto
    let v = s.mede(
        "index.html",
        &pagina("<img src=\"{{ x }}\"><img src=\"falta.png\">"),
    );
    ve(&v, "recurso", &["falta.png"]);
    assert!(v.nao_medido.is_some());
}

#[test]
fn recurso_link_so_conta_para_rel_de_arquivo() {
    let s = Site::novo();
    let outros = "<link rel=\"canonical\" href=\"pagina\"><link rel=\"alternate\" href=\"feed.xml\">\
        <link rel=\"dns-prefetch\" href=\"cdn\"><link rel=\"next\" href=\"p2.html\"><link href=\"x.css\">";
    limpo(&s.mede("index.html", &pagina(outros)));
    let arquivos = "<link rel=\"stylesheet\" href=\"a.css\"><link rel=\"icon\" href=\"f.ico\">\
        <link rel=\"preload\" href=\"w.woff2\" as=\"font\"><link rel=\"shortcut icon\" href=\"g.ico\">\
        <link rel=\"manifest\" href=\"m.json\"><link rel=\"modulepreload\" href=\"m.js\">";
    let v = s.mede("index.html", &pagina(arquivos));
    ve(
        &v,
        "recurso",
        &["a.css", "f.ico", "g.ico", "m.js", "m.json", "w.woff2"],
    );
}

#[test]
fn recurso_cobre_as_tags_que_carregam_arquivo() {
    let s = Site::novo();
    let corpo = "<script src=\"s.js\"></script><iframe src=\"f.html\"></iframe>\
        <video src=\"v.mp4\" poster=\"p.jpg\"></video><audio src=\"a.mp3\"></audio>\
        <source src=\"so.webm\"><track src=\"t.vtt\"><embed src=\"e.swf\"><object data=\"o.pdf\"></object>\
        <input type=\"image\" src=\"i.png\"><img src=\"m.png\" srcset=\"m1.png 1x, m2.png 2x\">";
    let v = s.mede("index.html", &pagina(corpo));
    ve(
        &v,
        "recurso",
        &[
            "a.mp3", "e.swf", "f.html", "i.png", "m.png", "m1.png", "m2.png", "o.pdf", "p.jpg",
            "s.js", "so.webm", "t.vtt", "v.mp4",
        ],
    );
    // `<input>` que não é imagem não carrega `src`
    limpo(&s.mede("index.html", &pagina("<input type=\"text\" src=\"x.png\">")));
}

#[test]
fn recurso_srcset_nao_quebra_data_url_com_virgula() {
    let s = Site::novo();
    s.escreve("b.png", "x");
    let v = s.mede(
        "index.html",
        &pagina("<img srcset=\"data:image/png;base64,AAAA 1x, b.png 2x\">"),
    );
    limpo(&v);
}

#[test]
fn recurso_link_so_conta_quando_o_alvo_parece_arquivo() {
    let s = Site::novo();
    let rotas = "<a href=\"/sobre\">s</a><a href=\"docs/\">d</a><a href=\"contato\">c</a>\
        <a href=\"https://x.org/p.html\">e</a>";
    limpo(&s.mede("index.html", &pagina(rotas)));
    let corpo = "<a href=\"guia.html\">g</a><a href=\"rel.pdf\">r</a>";
    ve(
        &s.mede("index.html", &pagina(corpo)),
        "recurso",
        &["guia.html", "rel.pdf"],
    );
    s.escreve("guia.html", "x");
    s.escreve("rel.pdf", "x");
    limpo(&s.mede("index.html", &pagina(corpo)));
}

#[test]
fn recurso_base_externa_nao_resolve_e_base_local_muda_o_diretorio() {
    let s = Site::novo();
    let com_base = |base: &str, ref_: &str| {
        format!(
            "<!DOCTYPE html><html><head><base href=\"{base}\"></head><body>\
             <img src=\"{ref_}\"></body></html>"
        )
    };
    let v = s.mede("index.html", &com_base("https://cdn.x/", "a.png"));
    assert!(v.achados.is_empty());
    assert!(motivo(&v).contains("<base"), "{:?}", v.nao_medido);

    s.escreve("sub/a.png", "x");
    limpo(&s.mede("index.html", &com_base("sub/", "a.png")));
    ve(
        &s.mede("index.html", &com_base("sub/", "b.png")),
        "recurso",
        &["b.png"],
    );
}

#[test]
fn recurso_ensina_o_nome_proximo_inclusive_na_caixa() {
    let s = Site::novo();
    s.escreve("img/Logo.PNG", "x");
    s.escreve("img/fundo-azul.png", "x");
    let v = s.mede(
        "index.html",
        &pagina("<img src=\"img/logo.png\"><img src=\"img/fundo-azu.png\">"),
    );
    let por_nome = |n: &str| v.achados.iter().find(|f| f.nome == n).expect(n);
    assert!(
        por_nome("img/logo.png")
            .sugestao
            .contains("Você quis dizer `img/Logo.PNG`")
    );
    assert!(
        por_nome("img/fundo-azu.png")
            .sugestao
            .contains("`img/fundo-azul.png`")
    );
    assert_eq!(
        por_nome("img/logo.png").reais.first().map(String::as_str),
        Some("Logo.PNG")
    );
}

#[test]
fn recurso_diretorio_inexistente_e_dito() {
    let s = Site::novo();
    let v = s.mede("index.html", &pagina("<img src=\"nao/existe/a.png\">"));
    ve(&v, "recurso", &["nao/existe/a.png"]);
    assert!(
        v.achados[0].sugestao.contains("não existe"),
        "{}",
        v.achados[0].sugestao
    );
}

#[test]
fn recurso_repetido_vira_um_achado_na_primeira_linha() {
    let s = Site::novo();
    let corpo = "<img src=\"falta.png\">\n<img src=\"falta.png\">\n<img src=\"falta.png\">";
    let v = s.mede("index.html", &pagina(corpo));
    ve(&v, "recurso", &["falta.png"]);
    assert_eq!(v.achados[0].linha, 7);
    assert!(
        v.achados[0].sugestao.contains("+2"),
        "{}",
        v.achados[0].sugestao
    );
}

#[test]
fn recurso_barra_invertida_espacos_e_quebra_de_linha_no_valor() {
    let s = Site::novo();
    s.escreve("img/a.png", "x");
    limpo(&s.mede("index.html", &pagina("<img src=\"  img\\a.png \">")));
    limpo(&s.mede("index.html", &pagina("<img src=\"img/\na.png\">")));
}

#[test]
fn recurso_conhece_o_overlay_do_instante() {
    let s = Site::novo();
    let mut input = s.entrada("index.html", &pagina("<img src=\"img/a.png\">"));
    assert_eq!(
        Html.verificar(&input).achados.len(),
        1,
        "sem overlay o arquivo falta"
    );
    input
        .overlays
        .insert(s.caminho("img/a.png"), "x".to_string());
    limpo(&Html.verificar(&input));
}

#[test]
fn recurso_em_caminho_virtual_e_nao_medido_mas_ancora_continua() {
    let s = Site::novo();
    let mut input = s.entrada(
        "x.html",
        &pagina("<img src=\"a.png\"><a href=\"#nada\">n</a>"),
    );
    input.path = "<string>".into();
    let v = Html.verificar(&input);
    ve(&v, "recurso", &[]);
    ve(&v, "ancora", &["nada"]);
    assert!(motivo(&v).contains("<string>"), "{:?}", v.nao_medido);
}

#[test]
fn recurso_nunca_nega_em_html_nem_onde_o_codigo_roda_agora() {
    // spec §5.1: deny exige que NENHUMA edição seguinte possa tornar o código certo; em HTML a
    // próxima escrita cria o arquivo (o `index.html` sai antes do `style.css`)
    let html = pagina("<img src=\"falta.png\">");
    for caminho in [
        "/home/u/proj/site/a.html",
        "/tmp/vhtml-x/a.html",
        "/home/u/proj/.cache/a.html",
    ] {
        assert_eq!(julga(caminho, &html).verdict, "warn", "{caminho}");
    }
    let raiz = pagina("<img src=\"/falta/a.png\">");
    assert_eq!(julga("/tmp/vhtml-x/a.html", &raiz).verdict, "warn");
}

#[test]
fn recurso_ja_ausente_no_disco_continua_warn_e_sem_prefixo_de_delta() {
    let html = pagina("<img src=\"falta.png\">");
    let out = check(&CheckInput {
        path: "/tmp/vhtml-x/a.html".into(),
        content: html.clone(),
        project_root: "/home/u/proj".into(),
        disk: Some(html),
        overlays: HashMap::new(),
        lang: "html".into(),
    });
    assert_eq!(out.verdict, "warn");
    assert!(
        !out.findings[0].sugestao.starts_with("pré-existente"),
        "o delta só reescreve deny, e HTML não nega: {}",
        out.findings[0].sugestao
    );
}

#[test]
fn recurso_ignora_a_caixa_de_tag_e_de_atributo() {
    let s = Site::novo();
    let v = s.mede(
        "index.html",
        &pagina("<IMG SRC=\"falta.png\"><Link REL=\"StyleSheet\" HREF=\"a.css\">"),
    );
    ve(&v, "recurso", &["a.css", "falta.png"]);
}

#[test]
fn recurso_nome_e_o_token_como_escrito_sem_query_e_sem_fragmento() {
    // convenção da coordenadora (10/10): o `nome` é a referência exatamente como escrita no
    // atributo — prefixo de caminho incluído — sem `?query` e sem `#fragmento`; a existência no
    // disco é testada sobre esse token, nunca sobre o valor inteiro
    let s = Site::novo();
    s.escreve("shared/foo.png", "x");
    let html = |ref_: &str| pagina(&format!("<img src=\"{ref_}\">"));
    // existe: a query e o fragmento não entram na existência
    limpo(&s.mede("a/b/index.html", &html("../../shared/foo.png?v=2#x")));
    // ausente: o nome é o token inteiro, com o prefixo, sem o sufixo
    let v = s.mede(
        "a/b/index.html",
        &html("../../shared/foo.vgpAusente0c2dcab442d3.png?v=2#x"),
    );
    ve(
        &v,
        "recurso",
        &["../../shared/foo.vgpAusente0c2dcab442d3.png"],
    );
    // relativo ao arquivo × raiz-relativo: o nome guarda a barra inicial e o `./`
    let v = s.mede("a/b/index.html", &html("./nada.png"));
    ve(&v, "recurso", &["./nada.png"]);
    assert_eq!(v.achados[0].alvo, Alvo::Projeto);
    let v = s.mede("a/b/index.html", &html("/shared/nada.png#x"));
    ve(&v, "recurso", &["/shared/nada.png"]);
    assert_eq!(v.achados[0].alvo, Alvo::Incerto);
    // a barra invertida fica como foi escrita (só a resolução a lê como `/`)
    let v = s.mede("index.html", &html("img\\nada.png"));
    ve(&v, "recurso", &["img\\nada.png"]);
}

#[test]
fn recurso_fragmento_de_template_nao_e_recurso_dinamico() {
    // `#sec-{{ n }}` aponta para dentro do documento: é da classe `ancora`, que conta a sua
    let s = Site::novo();
    let v = s.mede("a.html", &pagina("<a href=\"#sec-{{ n }}\">x</a>"));
    assert!(
        !motivo(&v).contains("valor dinâmico ou de template"),
        "{:?}",
        v.nao_medido
    );
    assert!(
        motivo(&v).contains("id com valor de template"),
        "{:?}",
        v.nao_medido
    );
}
