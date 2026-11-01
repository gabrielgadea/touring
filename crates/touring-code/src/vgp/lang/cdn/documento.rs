//! The html document as the transversal checks read it: every element with its attributes (and
//! the line each value starts on), plus the bodies of `<script>` and `<style>`.
//!
//! Shared by `cdn` (URLs in `src`/`href`, inline module imports, `@import`) and `classe_css`
//! (`class` attributes, `<style>` bodies, `<link rel=stylesheet>`). It reads the tree-sitter-html
//! parse — so a URL in a comment or in running text is never mistaken for a resource — and keeps
//! owned strings, because the tree does not outlive the call.

use tree_sitter::Node;

use crate::vgp::lang::parse;

/// One `name="value"` of a start tag. `nome` is lower-cased (HTML attribute names are
/// case-insensitive); `valor` is the text between the quotes, verbatim (entities not decoded).
#[derive(Debug, Clone)]
pub(crate) struct Atributo {
    pub nome: String,
    pub valor: String,
    /// 1-based line the value starts on.
    pub linha: usize,
}

/// A start tag (or a self-closing tag) with its attributes.
#[derive(Debug, Clone)]
pub(crate) struct Elemento {
    /// Lower-cased tag name.
    pub tag: String,
    pub atributos: Vec<Atributo>,
}

impl Elemento {
    /// The attribute called `nome` (lower-case), if the tag has it.
    pub fn atributo(&self, nome: &str) -> Option<&Atributo> {
        self.atributos.iter().find(|a| a.nome == nome)
    }
}

/// The text of a `<script>` or `<style>` element.
#[derive(Debug, Clone)]
pub(crate) struct Bloco {
    /// Index into [`Documento::elementos`] of the element's start tag.
    pub elemento: usize,
    pub texto: String,
    /// 1-based line the text starts on (its first line may be the tag's own).
    pub linha: usize,
}

/// What the checks need of one html document, in document order.
#[derive(Debug, Default)]
pub(crate) struct Documento {
    pub elementos: Vec<Elemento>,
    pub scripts: Vec<Bloco>,
    pub estilos: Vec<Bloco>,
    /// Is there a `<head>` / `<html>` tag — a fragment has neither.
    pub tem_head: bool,
    pub tem_html: bool,
}

impl Documento {
    /// The start tag a [`Bloco`] belongs to.
    pub fn elemento_de(&self, bloco: &Bloco) -> &Elemento {
        &self.elementos[bloco.elemento]
    }
}

/// Does a `<script type=…>` hold JavaScript (as opposed to JSON, a template, an import map…)?
/// `tipo` is lower-cased and trimmed; an absent type means JavaScript.
pub(crate) fn e_script_js(tipo: &str) -> bool {
    tipo.is_empty()
        || tipo == "module"
        || tipo.contains("javascript")
        || tipo.contains("ecmascript")
        || tipo == "text/babel"
        || tipo == "text/jsx"
}

fn texto<'a>(no: Node, fonte: &'a str) -> &'a str {
    no.utf8_text(fonte.as_bytes()).unwrap_or("")
}

/// Parse `fonte` as html. `Err` carries the reason when the grammar could not run at all
/// (an erroneous document still parses: the grammar recovers).
pub(crate) fn ler(fonte: &str) -> Result<Documento, String> {
    let arvore = parse("html", fonte)?;
    let mut doc = Documento::default();
    // explicit stack: a pathologically nested document must not overflow the hook's stack
    let mut pilha = vec![arvore.root_node()];
    while let Some(no) = pilha.pop() {
        match no.kind() {
            "start_tag" | "self_closing_tag" => {
                colher_elemento(&mut doc, no, fonte);
            }
            "script_element" | "style_element" => colher_bloco(&mut doc, no, fonte),
            "comment" => {}
            _ => {
                // reversed, so the stack pops the children in document order
                let total = u32::try_from(no.child_count()).unwrap_or(u32::MAX);
                pilha.extend((0..total).rev().filter_map(|i| no.child(i)));
            }
        }
    }
    Ok(doc)
}

fn colher_elemento(doc: &mut Documento, no: Node, fonte: &str) -> usize {
    let mut tag = String::new();
    let mut atributos = Vec::new();
    let mut cursor = no.walk();
    for filho in no.children(&mut cursor) {
        match filho.kind() {
            "tag_name" => tag = texto(filho, fonte).to_ascii_lowercase(),
            "attribute" => atributos.push(colher_atributo(filho, fonte)),
            _ => {}
        }
    }
    match tag.as_str() {
        "head" => doc.tem_head = true,
        "html" => doc.tem_html = true,
        _ => {}
    }
    doc.elementos.push(Elemento { tag, atributos });
    doc.elementos.len() - 1
}

fn colher_atributo(no: Node, fonte: &str) -> Atributo {
    let mut nome = String::new();
    let mut valor = String::new();
    let mut linha = no.start_position().row + 1;
    let mut cursor = no.walk();
    for filho in no.children(&mut cursor) {
        match filho.kind() {
            "attribute_name" => nome = texto(filho, fonte).to_ascii_lowercase(),
            "attribute_value" => {
                valor = texto(filho, fonte).to_string();
                linha = filho.start_position().row + 1;
            }
            "quoted_attribute_value" => {
                let mut interno = filho.walk();
                let valor_no = filho
                    .children(&mut interno)
                    .find(|n| n.kind() == "attribute_value");
                match valor_no {
                    Some(v) => {
                        valor = texto(v, fonte).to_string();
                        linha = v.start_position().row + 1;
                    }
                    None => linha = filho.start_position().row + 1,
                }
            }
            _ => {}
        }
    }
    Atributo { nome, valor, linha }
}

fn colher_bloco(doc: &mut Documento, no: Node, fonte: &str) {
    let e_estilo = no.kind() == "style_element";
    let mut elemento = None;
    let mut cursor = no.walk();
    for filho in no.children(&mut cursor) {
        match filho.kind() {
            "start_tag" => elemento = Some(colher_elemento(doc, filho, fonte)),
            "raw_text" => {
                // a zero-width `raw_text` (`<script src=…></script>`) is no block: it has no text
                if let Some(elemento) = elemento
                    && filho.end_byte() > filho.start_byte()
                {
                    let bloco = Bloco {
                        elemento,
                        texto: texto(filho, fonte).to_string(),
                        linha: filho.start_position().row + 1,
                    };
                    if e_estilo {
                        doc.estilos.push(bloco);
                    } else {
                        doc.scripts.push(bloco);
                    }
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colhe_elementos_atributos_e_linhas_em_ordem_de_documento() {
        let doc = ler("<!DOCTYPE html>\n<html>\n<head>\n<link rel=\"stylesheet\" href='a.css'>\n</head>\n<body class=x\n id=\"b\"></body>\n</html>\n").unwrap();
        let tags: Vec<&str> = doc.elementos.iter().map(|e| e.tag.as_str()).collect();
        assert_eq!(tags, ["html", "head", "link", "body"]);
        assert!(doc.tem_html && doc.tem_head);
        let link = &doc.elementos[2];
        assert_eq!(link.atributo("rel").unwrap().valor, "stylesheet");
        let href = link.atributo("href").unwrap();
        assert_eq!((href.valor.as_str(), href.linha), ("a.css", 4));
        let body = &doc.elementos[3];
        assert_eq!(body.atributo("class").unwrap().valor, "x");
        assert_eq!(body.atributo("id").unwrap().linha, 7);
    }

    #[test]
    fn nomes_de_tag_e_atributo_sao_minusculos_e_o_valor_vazio_existe() {
        let doc = ler("<SCRIPT SRC=\"\" Async></SCRIPT>").unwrap();
        let s = &doc.elementos[0];
        assert_eq!(s.tag, "script");
        assert_eq!(s.atributo("src").unwrap().valor, "");
        assert_eq!(s.atributo("async").unwrap().valor, "");
    }

    #[test]
    fn blocos_de_script_e_style_levam_o_texto_a_linha_e_o_elemento() {
        let doc = ler("<style>\n.a { color: red }\n</style>\n<script type=\"module\">\nlet x;\n</script>\n<script src=\"a.js\"></script>").unwrap();
        assert_eq!(doc.estilos.len(), 1);
        assert_eq!(doc.estilos[0].linha, 1, "o texto começa na linha da tag");
        assert!(doc.estilos[0].texto.contains(".a { color: red }"));
        assert_eq!(
            doc.scripts.len(),
            1,
            "<script src> sem corpo não gera bloco"
        );
        let el = doc.elemento_de(&doc.scripts[0]);
        assert_eq!(el.atributo("type").unwrap().valor, "module");
        assert_eq!(doc.scripts[0].linha, 4);
    }

    #[test]
    fn comentario_e_texto_corrido_nao_viram_elementos() {
        let doc = ler("<!-- <script src=\"x.js\"></script> -->\n<p>veja <b>isto</b></p>").unwrap();
        let tags: Vec<&str> = doc.elementos.iter().map(|e| e.tag.as_str()).collect();
        assert_eq!(tags, ["p", "b"]);
        assert!(doc.scripts.is_empty());
    }

    #[test]
    fn fragmento_nao_tem_head_nem_html() {
        let doc = ler("<div class=\"a\"></div>").unwrap();
        assert!(!doc.tem_head && !doc.tem_html);
    }

    #[test]
    fn aninhamento_profundo_nao_estoura_a_pilha() {
        let fonte = format!("{}x{}", "<div>".repeat(5000), "</div>".repeat(5000));
        let doc = ler(&fonte).unwrap();
        assert!(doc.elementos.len() >= 1000);
    }
}
