//! Local type inference for receivers: what is the type of the expression left of a `.`?
//!
//! Only what can be PROVED from the file and the project's declarations is answered: `new C()` of a
//! builtin or project class, literals, an identifier bound once (a `const`, a parameter or variable with
//! a type annotation — or a JSDoc `@type`/`@param` in JavaScript —, a `let` nobody reassigns), `this`
//! inside a class, and member chains through declared member types. Anything else — calls, awaits,
//! destructuring, reassigned variables, unions the checks cannot narrow — is `Desconhecido`, and an
//! unknown receiver never yields a finding.

use std::rc::Rc;

use tree_sitter::Node;

use super::decls::{Decl, TipoExpr, baixar_tipo};
use super::escopo::{Analise, BindTipo};
use super::jsdoc;
use super::modulos::Importado;
use super::tipos::{Contexto, Resolvedor, Resultado, forma_builtin};
use super::util::{tem_token, texto};

const MAX_PROF: u8 = 6;

/// Infers receiver types over one tree.
pub(super) struct Inferidor<'a, 't> {
    pub src: &'t str,
    pub escopo: &'a Analise<'t>,
    pub resolvedor: &'a Resolvedor<'a>,
    /// The module under check (its declarations and imports).
    pub eu: &'a Rc<Contexto>,
    /// Plain JavaScript: types may come from JSDoc.
    pub js: bool,
}

fn builtin(nome: &str) -> Resultado {
    forma_builtin(nome).map_or(Resultado::Desconhecido, Resultado::Forma)
}

/// The builtin type of a literal expression kind.
fn tipo_do_literal(kind: &str) -> Option<&'static str> {
    match kind {
        "string" | "template_string" => Some("String"),
        "number" => Some("Number"),
        "true" | "false" => Some("Boolean"),
        "array" => Some("Array"),
        "regex" => Some("RegExp"),
        _ => None,
    }
}

impl<'a, 't> Inferidor<'a, 't> {
    /// The type of the expression `no`.
    pub fn inferir(&self, no: Node<'t>, prof: u8) -> Resultado {
        if prof > MAX_PROF {
            return Resultado::Desconhecido;
        }
        let k = no.kind();
        if let Some(b) = tipo_do_literal(k) {
            return builtin(b);
        }
        match k {
            "parenthesized_expression" | "non_null_expression" => no
                .named_child(0)
                .map_or(Resultado::Desconhecido, |f| self.inferir(f, prof + 1)),
            "as_expression" => self.de_assercao(no),
            "new_expression" => self.de_new(no),
            "identifier" => self.de_identificador(no, prof),
            "this" => self.de_this(no),
            "member_expression" => self.de_membro(no, prof),
            _ => Resultado::Desconhecido,
        }
    }

    /// The type written in an annotation node, resolved in this module.
    pub fn de_anotacao(&self, tipo: Node<'t>) -> Resultado {
        self.resolver_tipo(&baixar_tipo(tipo, self.src))
    }

    /// A lowered type, resolved in this module.
    pub fn resolver_tipo(&self, t: &TipoExpr) -> Resultado {
        self.resolvedor.resolver(t, self.eu, &[], 0)
    }

    /// `expr as T` has type `T` (`as const` and unresolvable `T` are unknown).
    fn de_assercao(&self, no: Node<'t>) -> Resultado {
        match no.named_child(1) {
            Some(tipo) if texto(self.src, tipo) != "const" => self.de_anotacao(tipo),
            _ => Resultado::Desconhecido,
        }
    }

    fn de_new(&self, no: Node<'t>) -> Resultado {
        let Some(ctor) = no
            .child_by_field_name("constructor")
            .filter(|c| c.kind() == "identifier")
        else {
            return Resultado::Desconhecido;
        };
        let nome = texto(self.src, ctor);
        match self.escopo.resolver(no, nome).map(|l| l.tipo) {
            None => builtin(nome),
            Some(BindTipo::Classe) if self.escopo.resolver_no_topo(no, nome).is_some() => {
                self.resolvedor.forma_da_classe(nome, self.eu, 0)
            }
            Some(BindTipo::Import) => self.classe_importada(nome),
            Some(_) => Resultado::Desconhecido,
        }
    }

    /// `import { C } from './c'` ⇒ the instance shape of `C` declared there.
    fn classe_importada(&self, local: &str) -> Resultado {
        let Some((caminho, info, local2)) = self.seguir_importado(local) else {
            return Resultado::Desconhecido;
        };
        let ctx = Rc::new(Contexto { caminho, info });
        self.resolvedor.forma_da_classe(&local2, &ctx, 0)
    }

    /// Follow the `import` binding `local` to the module that declares it.
    fn seguir_importado(
        &self,
        local: &str,
    ) -> Option<(std::path::PathBuf, Rc<super::modulos::ModuloInfo>, String)> {
        let (spec, original) = match self.eu.info.imports.get(local)? {
            Importado::Nomeado(s, o) => (s.as_str(), o.as_str()),
            Importado::Padrao(s) => (s.as_str(), "default"),
            Importado::Namespace(_) => return None,
        };
        let car = self.resolvedor.carregador;
        let (caminho, info) = car.resolver_e_carregar(&self.eu.caminho, spec)?;
        car.seguir_export(&caminho, &info, original, 0)
    }

    fn de_identificador(&self, no: Node<'t>, prof: u8) -> Resultado {
        let nome = texto(self.src, no);
        let Some(lig) = self.escopo.resolver(no, nome) else {
            return Resultado::Desconhecido;
        };
        match lig.tipo {
            BindTipo::Param => self.de_parametro(lig.no, nome),
            BindTipo::Const | BindTipo::Let | BindTipo::Var => {
                let reatribuido =
                    lig.tipo != BindTipo::Const && self.escopo.atribuidos.contains(nome);
                let desestruturado = lig
                    .no
                    .child_by_field_name("name")
                    .is_some_and(|n| n.kind() != "identifier");
                if reatribuido || desestruturado {
                    return Resultado::Desconhecido;
                }
                self.de_declarador(lig.no, prof)
            }
            _ => Resultado::Desconhecido,
        }
    }

    /// A parameter's annotation (TS) or its JSDoc `@param` (JS).
    fn de_parametro(&self, param: Node<'t>, nome: &str) -> Resultado {
        if let Some(t) = param.child_by_field_name("type") {
            return self.de_anotacao(t);
        }
        if self.js
            && let Some(t) = jsdoc::tipo_do_parametro(param, nome, self.src)
        {
            return self.resolver_tipo(&t);
        }
        Resultado::Desconhecido
    }

    /// `const x: T = …` / `/** @type {T} */ const x = …` / `const x = <inferable>`.
    fn de_declarador(&self, d: Node<'t>, prof: u8) -> Resultado {
        if let Some(t) = d.child_by_field_name("type") {
            return self.de_anotacao(t);
        }
        if self.js
            && let Some(t) = d
                .parent()
                .and_then(|decl| jsdoc::tipo_da_declaracao(decl, self.src))
        {
            return self.resolver_tipo(&t);
        }
        d.child_by_field_name("value")
            .map_or(Resultado::Desconhecido, |v| self.inferir(v, prof + 1))
    }

    /// `this` inside an instance method/constructor/field initializer of a top-level class.
    fn de_this(&self, no: Node<'t>) -> Resultado {
        match classe_dona_do_this(no) {
            Some(classe) => match classe.child_by_field_name("name") {
                Some(nome) => self
                    .resolvedor
                    .forma_da_classe(texto(self.src, nome), self.eu, 0),
                None => Resultado::Desconhecido,
            },
            None => Resultado::Desconhecido,
        }
    }

    /// `a.b`: the declared type of member `b` of the shape of `a`.
    fn de_membro(&self, no: Node<'t>, prof: u8) -> Resultado {
        let (Some(obj), Some(prop)) = (
            no.child_by_field_name("object"),
            no.child_by_field_name("property"),
        ) else {
            return Resultado::Desconhecido;
        };
        let Resultado::Forma(forma) = self.inferir(obj, prof + 1) else {
            return Resultado::Desconhecido;
        };
        match forma.tipos.get(texto(self.src, prop)) {
            Some((t, ctx)) => self.resolvedor.resolver(t, ctx, &[], 0),
            None => Resultado::Desconhecido,
        }
    }

    /// The declaration of a top-level function (or arrow `const`) callable as `nome` from this
    /// module, with the context its parameter types are written in.
    pub fn funcao_chamavel(&self, nome: &str) -> Option<(Decl, Rc<Contexto>)> {
        if let Some(ds) = self.eu.info.decls.get(nome) {
            return unica_funcao(ds).map(|d| (d, self.eu.clone()));
        }
        let (caminho, info, local) = self.seguir_importado(nome)?;
        let decl = unica_funcao(info.decls.get(&local)?)?;
        Some((decl, Rc::new(Contexto { caminho, info })))
    }
}

/// The class a `this` at `no` refers to: inside an instance method, constructor or field
/// initializer (arrows keep `this`; a nested `function`, a static member or an object-literal
/// method change it).
fn classe_dona_do_this(no: Node) -> Option<Node> {
    let mut atual = no.parent();
    while let Some(n) = atual {
        match n.kind() {
            "method_definition"
                if tem_token(n, "static")
                    || n.parent().is_none_or(|p| p.kind() != "class_body") =>
            {
                return None;
            }
            "public_field_definition" | "field_definition" if tem_token(n, "static") => {
                return None;
            }
            "function_declaration"
            | "function_expression"
            | "function"
            | "generator_function"
            | "generator_function_declaration"
            | "class" => return None,
            "class_declaration" | "abstract_class_declaration" => return Some(n),
            _ => {}
        }
        atual = n.parent();
    }
    None
}

/// The only function-like declaration among `ds` (overloads and merges are ambiguous: `None`).
fn unica_funcao(ds: &[Decl]) -> Option<Decl> {
    let eh_funcao = |d: &&Decl| {
        matches!(d, Decl::Funcao { .. })
            || matches!(d, Decl::Variavel(v) if v.funcao.is_some() || v.tipo.is_some())
    };
    let funcoes: Vec<&Decl> = ds.iter().filter(eh_funcao).collect();
    match funcoes.as_slice() {
        [uma] if ds.len() == 1 => Some((*uma).clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vgp::lang::parse;
    use crate::vgp::lang::typescript::escopo::analisar;
    use crate::vgp::lang::typescript::modulos::{Carregador, resumir};
    use crate::vgp::lang::typescript::projeto::Projeto;
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};

    /// Infer the type of the receiver of the LAST `.membro` access in `src`.
    fn receptor(lang: &str, src: &str) -> Resultado {
        let arvore = parse(lang, src).expect("parse");
        let esc = analisar(arvore.root_node(), src);
        let sobrepor = HashMap::new();
        let projeto = Projeto::novo(Path::new("/p"), &sobrepor);
        let car = Carregador::novo(&projeto);
        let info = Rc::new(resumir(
            src,
            arvore.root_node(),
            false,
            lang == "javascript",
        ));
        let eu = Rc::new(Contexto {
            caminho: PathBuf::from("/p/a.ts"),
            info,
        });
        let res = Resolvedor::novo(&car);
        let inf = Inferidor {
            src,
            escopo: &esc,
            resolvedor: &res,
            eu: &eu,
            js: lang == "javascript",
        };
        let mut ultimo = None;
        let mut pilha = vec![arvore.root_node()];
        while let Some(n) = pilha.pop() {
            if n.kind() == "member_expression"
                && ultimo.is_none_or(|u: Node| n.start_byte() > u.start_byte())
            {
                ultimo = Some(n);
            }
            let mut c = n.walk();
            pilha.extend(n.named_children(&mut c));
        }
        let m = ultimo.expect("member_expression");
        inf.inferir(m.child_by_field_name("object").expect("object"), 0)
    }

    fn nome_da_forma(r: &Resultado) -> Option<String> {
        match r {
            Resultado::Forma(f) => Some(f.nome.clone()),
            _ => None,
        }
    }

    #[test]
    fn new_de_builtin_e_literais() {
        assert_eq!(
            nome_da_forma(&receptor("typescript", "new Date().foo;")).as_deref(),
            Some("Date")
        );
        assert_eq!(
            nome_da_forma(&receptor("typescript", "'abc'.foo;")).as_deref(),
            Some("String")
        );
        assert_eq!(
            nome_da_forma(&receptor("typescript", "[1, 2].foo;")).as_deref(),
            Some("Array")
        );
        assert_eq!(
            nome_da_forma(&receptor("typescript", "(1).foo;")).as_deref(),
            Some("Number")
        );
        assert_eq!(
            nome_da_forma(&receptor("typescript", "(new Date()).foo;")).as_deref(),
            Some("Date")
        );
    }

    #[test]
    fn new_de_nome_ligado_localmente_nao_e_o_builtin() {
        assert!(
            nome_da_forma(&receptor(
                "typescript",
                "import { Date } from 'luxon'; new Date().foo;"
            ))
            .is_none()
        );
        assert!(
            nome_da_forma(&receptor(
                "typescript",
                "function f(Date: any) { new Date().foo; }"
            ))
            .is_none()
        );
    }

    #[test]
    fn identificador_const_anotado_e_inferido() {
        assert_eq!(
            nome_da_forma(&receptor("typescript", "const d = new Date(); d.foo;")).as_deref(),
            Some("Date")
        );
        assert_eq!(
            nome_da_forma(&receptor(
                "typescript",
                "interface P { a: 1 } function f(p: P) { p.foo; }"
            ))
            .as_deref(),
            Some("P")
        );
        assert_eq!(
            nome_da_forma(&receptor(
                "typescript",
                "interface P { a: 1 } const p: P = make(); p.foo;"
            ))
            .as_deref(),
            Some("P")
        );
    }

    #[test]
    fn variavel_reatribuida_ou_desestruturada_e_desconhecida() {
        assert!(
            nome_da_forma(&receptor(
                "typescript",
                "let d = new Date(); d = outro(); d.foo;"
            ))
            .is_none()
        );
        assert!(nome_da_forma(&receptor("typescript", "const { d } = x; d.foo;")).is_none());
        assert!(nome_da_forma(&receptor("typescript", "let n = 1; n++; n.foo;")).is_none());
        // desestruturar um valor TIPADO não devolve o tipo do todo para o pedaço
        assert!(
            nome_da_forma(&receptor(
                "typescript",
                "const x = new Date(); const { y } = x; y.foo;"
            ))
            .is_none()
        );
    }

    #[test]
    fn this_em_classe_de_topo_mas_nao_em_funcao_nem_estatico() {
        let ok = receptor("typescript", "class K { a = 1; m() { this.foo; } }");
        assert_eq!(nome_da_forma(&ok).as_deref(), Some("K"));
        let arrow = receptor("typescript", "class K { m() { [1].map(x => this.foo); } }");
        assert_eq!(nome_da_forma(&arrow).as_deref(), Some("K"));
        assert!(
            nome_da_forma(&receptor(
                "typescript",
                "class K { static m() { this.foo; } }"
            ))
            .is_none()
        );
        assert!(
            nome_da_forma(&receptor(
                "typescript",
                "class K { m() { function g() { this.foo; } } }"
            ))
            .is_none()
        );
        assert!(
            nome_da_forma(&receptor("typescript", "const o = { m() { this.foo; } };")).is_none()
        );
        // num campo estático, `this` é o construtor da classe, não a instância
        assert!(
            nome_da_forma(&receptor("typescript", "class K { static s = this.foo; }")).is_none()
        );
    }

    #[test]
    fn cadeia_de_membros_usa_o_tipo_declarado() {
        let r = receptor(
            "typescript",
            "interface Cfg { nome: string } class K { cfg: Cfg = { nome: 'x' }; m() { this.cfg.foo; } }",
        );
        assert_eq!(nome_da_forma(&r).as_deref(), Some("Cfg"));
    }

    #[test]
    fn assercao_de_tipo_vale_e_const_nao() {
        assert_eq!(
            nome_da_forma(&receptor(
                "typescript",
                "interface P { a: 1 } (x as P).foo;"
            ))
            .as_deref(),
            Some("P")
        );
        assert!(nome_da_forma(&receptor("typescript", "(x as const).foo;")).is_none());
    }

    #[test]
    fn jsdoc_param_e_type_dao_o_tipo_em_javascript() {
        let src = "/** @typedef {{ a: number }} P */\n/**\n * @param {P} p\n */\nfunction f(p) { p.foo; }\n";
        assert_eq!(
            nome_da_forma(&receptor("javascript", src)).as_deref(),
            Some("P")
        );
        let src =
            "/** @typedef {{ a: number }} P */\n/** @type {P} */\nconst p = make();\np.foo;\n";
        assert_eq!(
            nome_da_forma(&receptor("javascript", src)).as_deref(),
            Some("P")
        );
        // sem JSDoc nada é inferido de um parâmetro
        assert!(nome_da_forma(&receptor("javascript", "function f(p) { p.foo; }")).is_none());
    }
}
