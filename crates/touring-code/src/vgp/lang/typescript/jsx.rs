//! JSX attribute checks (`prop`): the attribute a component's `Props` — or a React intrinsic element —
//! does not have.
//!
//! Two claims, two strengths. An attribute on a PROJECT component is judged against that component's
//! `Props` shape, resolved through the project's own declarations; the shape must be closed (an
//! `extends React.HTMLAttributes<…>` that cannot be followed opens it, and an open shape says
//! nothing). An attribute on an intrinsic element (`<div …>`) is judged against the union of every
//! attribute the React types declare — a closed table generated from `@types/react` — and is only
//! `Incerto`: JSX libraries augment those interfaces (`css=`, `tw=`), so the motor can warn but not
//! deny. Hyphenated attributes (`data-*`, `aria-*`) are never judged, as in TypeScript.

use tree_sitter::Node;

use super::analise::{Candidato, Verificador};
use super::decls::{Decl, TipoExpr, VarDecl};
use super::projeto::ConfigTs;
use super::tabelas::ATRIBUTOS_REACT;
use super::tipos::{Origem, Resultado};
use super::util::{linha, texto};
use crate::vgp::politica::Alvo;

/// `React.FC<P>` and friends: the types whose first type argument is the props.
const TIPOS_DE_COMPONENTE: &[&str] = &["FC", "FunctionComponent", "VFC", "VoidFunctionComponent"];

/// Attributes every React element accepts through `JSX.IntrinsicAttributes`.
const ATRIBUTOS_UNIVERSAIS: &[&str] = &["key", "ref"];

impl<'a, 't> Verificador<'a, 't> {
    /// Attributes on JSX elements that provably do not exist.
    pub(super) fn props_ausentes(&self) -> Vec<Candidato> {
        let react = self.projeto_usa_react();
        let mut out = Vec::new();
        for el in self.nos_de_tipo(&["jsx_opening_element", "jsx_self_closing_element"]) {
            let Some(nome) = el.child_by_field_name("name") else {
                continue;
            };
            if nome.kind() != "identifier" {
                continue; // `<ns.Comp>`, `<svg:rect>`
            }
            let tag = texto(self.src, nome);
            let (atributos, tem_spread) = atributos_do_elemento(el, self.src);
            if tag.contains('-') {
                continue; // custom element
            }
            if tag.starts_with(char::is_lowercase) {
                if react {
                    self.props_intrinsecas(tag, &atributos, &mut out);
                }
            } else if !tem_spread {
                self.props_de_componente(tag, el, &atributos, &mut out);
            }
        }
        out
    }

    /// Does the project type JSX as React (so the React attribute table applies)?
    fn projeto_usa_react(&self) -> bool {
        let cfg: &ConfigTs = self.mundo.config();
        let dir = self.arquivo.parent().unwrap_or(std::path::Path::new("."));
        let fonte_ok = cfg
            .jsx_import_source
            .as_deref()
            .is_none_or(|s| s == "react");
        let jsx_ok = cfg
            .jsx
            .as_deref()
            .is_none_or(|j| j.starts_with("react") || j == "preserve");
        fonte_ok && jsx_ok && self.projeto.pacote_instalado(dir, "react")
    }

    fn props_intrinsecas(
        &self,
        tag: &str,
        atributos: &[(String, Node<'t>)],
        out: &mut Vec<Candidato>,
    ) {
        for (nome, no) in atributos {
            if nome.contains('-') || ATRIBUTOS_REACT.binary_search(&nome.as_str()).is_ok() {
                continue;
            }
            out.push(Candidato::novo(
                "prop",
                nome,
                linha(*no),
                Alvo::Incerto,
                format!(
                    "`<{tag}>` não tem o atributo `{nome}` nos tipos do React (HTML/SVG/eventos)"
                ),
                Vec::new(),
            ));
        }
    }

    fn props_de_componente(
        &self,
        tag: &str,
        el: Node<'t>,
        atributos: &[(String, Node<'t>)],
        out: &mut Vec<Candidato>,
    ) {
        if self.escopo.resolver_no_topo(el, tag).is_none() {
            return; // undeclared (the `componente` check owns it) or shadowed by a local binding
        }
        let inf = self.inferidor();
        let Some((decl, ctx)) = inf.funcao_chamavel(tag) else {
            return;
        };
        let Some(tipo) = tipo_das_props(&decl) else {
            return;
        };
        let Resultado::Forma(forma) = self.resolvedor.resolver(&tipo, &ctx, &[], 0) else {
            return;
        };
        if forma.aberta || forma.origem != Origem::Projeto {
            return;
        }
        for (nome, no) in atributos {
            if nome.contains('-')
                || ATRIBUTOS_UNIVERSAIS.contains(&nome.as_str())
                || !forma.nao_tem(nome)
            {
                continue;
            }
            out.push(Candidato::novo(
                "prop",
                nome,
                linha(*no),
                Alvo::Projeto,
                format!(
                    "o componente `<{tag}>` não tem a prop `{nome}` em `{}`",
                    forma.nome
                ),
                super::util::reais_de(&forma.membros),
            ));
        }
    }
}

/// The declared props type of a component: first parameter's annotation, `FC<P>`, `forwardRef<El, P>`.
fn tipo_das_props(decl: &Decl) -> Option<TipoExpr> {
    match decl {
        Decl::Funcao { params } => params.first()?.tipo.clone(),
        Decl::Variavel(v) => tipo_das_props_de_variavel(v),
        _ => None,
    }
}

fn tipo_das_props_de_variavel(v: &VarDecl) -> Option<TipoExpr> {
    if let Some(t) = v
        .funcao
        .as_ref()
        .and_then(|ps| ps.first())
        .and_then(|p| p.tipo.clone())
    {
        return Some(t);
    }
    if let Some(TipoExpr::Ref { nome, args, .. }) = &v.tipo
        && TIPOS_DE_COMPONENTE.contains(&nome.as_str())
    {
        return args.first().cloned();
    }
    match &v.embrulho {
        Some((quem, targs)) if quem == "forwardRef" => targs.get(1).cloned(),
        _ => None,
    }
}

/// The plain attributes of a JSX element (name, node) and whether any attribute is a spread.
fn atributos_do_elemento<'t>(el: Node<'t>, src: &str) -> (Vec<(String, Node<'t>)>, bool) {
    let mut atributos = Vec::new();
    let mut spread = false;
    let mut c = el.walk();
    for a in el.children_by_field_name("attribute", &mut c) {
        match a.kind() {
            "jsx_attribute" => {
                if let Some(n) = a.named_child(0)
                    && n.kind() == "property_identifier"
                {
                    atributos.push((texto(src, n).to_string(), n));
                }
            }
            _ => spread = true,
        }
    }
    (atributos, spread)
}
