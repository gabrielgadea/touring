//! Type resolution: turn a [`TipoExpr`] (or a declaration) into a [`Forma`] — the member set an
//! access or an object literal is judged against.
//!
//! A `Forma` is trusted only when CLOSED. Everything the resolver cannot see through — an `extends`
//! of something unresolved, an index signature, a mapped type, a generic parameter, a union with an
//! unresolvable member — opens the shape (`aberta`) or yields `Desconhecido`, and an open shape
//! never produces a finding. Builtin types come from the generated lib tables; project types from
//! the declarations of the module that declares them, found through `import`s and re-exports.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;

use super::decls::{ClasseDecl, Decl, Heranca, ObjTipo, TipoExpr};
use super::modulos::{Carregador, Importado, MAX_PROFUNDIDADE, ModuloInfo};
use super::tabelas::TIPOS_BUILTIN;

/// Members every object has (`Object.prototype`).
const OBJETO: &[&str] = &[
    "constructor",
    "toString",
    "toLocaleString",
    "valueOf",
    "hasOwnProperty",
    "isPrototypeOf",
    "propertyIsEnumerable",
    "__proto__",
];

/// Where a type's names are written: the module that declares it.
#[derive(Debug)]
pub(super) struct Contexto {
    pub caminho: PathBuf,
    pub info: Rc<ModuloInfo>,
}

/// Is the shape a project declaration or a closed builtin table?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Origem {
    Projeto,
    Builtin,
}

/// The member set of a type.
#[derive(Debug, Clone)]
pub(super) struct Forma {
    pub nome: String,
    pub membros: HashSet<String>,
    /// Declared types of members (for `a.b.c` chains), with the context their names live in.
    pub tipos: HashMap<String, (TipoExpr, Rc<Contexto>)>,
    /// The member set is not closed: nothing absent from it is proof.
    pub aberta: bool,
    pub origem: Origem,
}

impl Forma {
    fn vazia(nome: &str, origem: Origem) -> Self {
        Self {
            nome: nome.to_string(),
            membros: OBJETO.iter().map(|s| (*s).to_string()).collect(),
            tipos: HashMap::new(),
            aberta: false,
            origem,
        }
    }

    fn mesclar(&mut self, outra: Forma) {
        self.membros.extend(outra.membros);
        for (k, v) in outra.tipos {
            self.tipos.entry(k).or_insert(v);
        }
        self.aberta |= outra.aberta;
    }

    /// Does the shape provably LACK `nome`? Only a closed shape can say so.
    pub fn nao_tem(&self, nome: &str) -> bool {
        !self.aberta && !self.membros.contains(nome)
    }
}

/// What a type expression resolves to.
#[derive(Debug, Clone)]
pub(super) enum Resultado {
    Forma(Forma),
    /// A union of resolvable shapes (`null`/`undefined` members dropped).
    Uniao(Vec<Forma>),
    Desconhecido,
}

/// Resolves type names through a module's declarations, imports and re-exports.
pub(super) struct Resolvedor<'a> {
    pub carregador: &'a Carregador<'a>,
}

/// The builtin type `nome` (`Date`, `Array`, `String`…), if the generated tables have it.
pub(super) fn forma_builtin(nome: &str) -> Option<Forma> {
    let (_, membros) = TIPOS_BUILTIN.iter().find(|(n, _)| *n == nome)?;
    let mut f = Forma::vazia(nome, Origem::Builtin);
    f.membros.extend(membros.iter().map(|s| (*s).to_string()));
    Some(f)
}

fn prim_para_builtin(p: &str) -> Option<&'static str> {
    match p {
        "string" => Some("String"),
        "number" => Some("Number"),
        "boolean" => Some("Boolean"),
        "bigint" => Some("BigInt"),
        _ => None,
    }
}

impl<'a> Resolvedor<'a> {
    pub fn novo(carregador: &'a Carregador<'a>) -> Self {
        Self { carregador }
    }

    /// Resolve `t` as written in `ctx`. `livres` are the generic parameter names in scope.
    pub fn resolver(
        &self,
        t: &TipoExpr,
        ctx: &Rc<Contexto>,
        livres: &[String],
        prof: u8,
    ) -> Resultado {
        if prof >= MAX_PROFUNDIDADE {
            return Resultado::Desconhecido;
        }
        match t {
            TipoExpr::Ref { nome, qual, args } => {
                self.resolver_referencia(nome, qual, args, ctx, livres, prof)
            }
            TipoExpr::Obj(o) => Resultado::Forma(self.forma_de_corpo("{…}", o, ctx)),
            TipoExpr::Uniao(v) => self.resolver_uniao(v, ctx, livres, prof),
            TipoExpr::Interseccao(v) => self.resolver_interseccao(v, ctx, livres, prof),
            TipoExpr::Array(_) => {
                forma_builtin("Array").map_or(Resultado::Desconhecido, Resultado::Forma)
            }
            TipoExpr::Prim(p) => prim_para_builtin(p)
                .and_then(forma_builtin)
                .map_or(Resultado::Desconhecido, Resultado::Forma),
            TipoExpr::Literal | TipoExpr::Outro => Resultado::Desconhecido,
        }
    }

    fn resolver_uniao(
        &self,
        v: &[TipoExpr],
        ctx: &Rc<Contexto>,
        livres: &[String],
        prof: u8,
    ) -> Resultado {
        let mut formas = Vec::new();
        for t in v {
            if matches!(t, TipoExpr::Prim(p) if p == "null" || p == "undefined" || p == "void") {
                continue;
            }
            match self.resolver(t, ctx, livres, prof + 1) {
                Resultado::Forma(f) => formas.push(f),
                Resultado::Uniao(fs) => formas.extend(fs),
                Resultado::Desconhecido => return Resultado::Desconhecido,
            }
        }
        match formas.len() {
            0 => Resultado::Desconhecido,
            1 => Resultado::Forma(formas.remove(0)),
            _ => Resultado::Uniao(formas),
        }
    }

    fn resolver_interseccao(
        &self,
        v: &[TipoExpr],
        ctx: &Rc<Contexto>,
        livres: &[String],
        prof: u8,
    ) -> Resultado {
        let mut total = Forma::vazia("&", Origem::Projeto);
        for t in v {
            match self.resolver(t, ctx, livres, prof + 1) {
                Resultado::Forma(f) => total.mesclar(f),
                _ => total.aberta = true,
            }
        }
        Resultado::Forma(total)
    }

    fn resolver_referencia(
        &self,
        nome: &str,
        qual: &[String],
        args: &[TipoExpr],
        ctx: &Rc<Contexto>,
        livres: &[String],
        prof: u8,
    ) -> Resultado {
        if livres.iter().any(|p| p == nome) && qual.is_empty() {
            return Resultado::Desconhecido; // a generic parameter
        }
        if qual.is_empty() {
            // utility types that keep (a superset of) the keys of their first argument
            if matches!(
                nome,
                "Partial" | "Required" | "Readonly" | "NonNullable" | "Pick" | "Omit"
            ) && !ctx.info.decls.contains_key(nome)
                && let Some(primeiro) = args.first()
            {
                return self.resolver(primeiro, ctx, livres, prof + 1);
            }
            if let Some(r) = self.resolver_local(nome, ctx, prof) {
                return r;
            }
            return forma_builtin(nome).map_or(Resultado::Desconhecido, Resultado::Forma);
        }
        self.resolver_qualificada(nome, qual, ctx, prof)
    }

    /// `Nome` declared in `ctx` or imported into it.
    fn resolver_local(&self, nome: &str, ctx: &Rc<Contexto>, prof: u8) -> Option<Resultado> {
        if let Some(decls) = ctx.info.decls.get(nome) {
            let tipos: Vec<&Decl> = decls.iter().filter(|d| eh_decl_de_tipo(d)).collect();
            if !tipos.is_empty() {
                return Some(self.formar(nome, &tipos, ctx, prof));
            }
        }
        let importado = ctx.info.imports.get(nome)?;
        let (spec, original) = match importado {
            Importado::Nomeado(s, o) => (s.as_str(), o.as_str()),
            Importado::Padrao(s) => (s.as_str(), "default"),
            Importado::Namespace(_) => return Some(Resultado::Desconhecido),
        };
        Some(self.resolver_em_outro_modulo(ctx, spec, original, prof))
    }

    /// `ns.Nome` where `ns` is a namespace import of a project module.
    fn resolver_qualificada(
        &self,
        nome: &str,
        qual: &[String],
        ctx: &Rc<Contexto>,
        prof: u8,
    ) -> Resultado {
        let [ns] = qual else {
            return Resultado::Desconhecido;
        };
        match ctx.info.imports.get(ns) {
            Some(Importado::Namespace(spec)) => {
                self.resolver_em_outro_modulo(ctx, spec, nome, prof)
            }
            _ => Resultado::Desconhecido,
        }
    }

    fn resolver_em_outro_modulo(
        &self,
        ctx: &Rc<Contexto>,
        spec: &str,
        original: &str,
        prof: u8,
    ) -> Resultado {
        let Some((caminho, info)) = self.carregador.resolver_e_carregar(&ctx.caminho, spec) else {
            return Resultado::Desconhecido;
        };
        let Some((caminho2, info2, local)) =
            self.carregador
                .seguir_export(&caminho, &info, original, prof + 1)
        else {
            return Resultado::Desconhecido;
        };
        let Some(decls) = info2.decls.get(&local) else {
            return Resultado::Desconhecido;
        };
        let tipos: Vec<&Decl> = decls.iter().filter(|d| eh_decl_de_tipo(d)).collect();
        if tipos.is_empty() {
            return Resultado::Desconhecido;
        }
        let novo = Rc::new(Contexto {
            caminho: caminho2,
            info: info2.clone(),
        });
        self.formar(&local, &tipos, &novo, prof + 1)
    }

    /// Merge the declarations of one name into a [`Resultado`].
    fn formar(&self, nome: &str, decls: &[&Decl], ctx: &Rc<Contexto>, prof: u8) -> Resultado {
        if let [Decl::Alias { params, tipo }] = decls {
            return match self.resolver(tipo, ctx, params, prof + 1) {
                // `type P = { … }` / `type P = A & B`: the refusal names the alias, not «{…}»
                Resultado::Forma(mut f) if f.nome == "{…}" || f.nome == "&" => {
                    f.nome = nome.to_string();
                    Resultado::Forma(f)
                }
                outro => outro,
            };
        }
        let mut total = Forma::vazia(nome, Origem::Projeto);
        for d in decls {
            match d {
                Decl::Interface {
                    params,
                    estende,
                    corpo,
                } => {
                    total.mesclar(self.forma_de_corpo(nome, corpo, ctx));
                    for e in estende {
                        self.somar_base(&mut total, e, ctx, params, prof);
                    }
                }
                Decl::Classe(c) => self.somar_classe(&mut total, c, ctx, prof),
                // an alias merged with something else, or an enum: not a member-checkable shape
                _ => total.aberta = true,
            }
        }
        Resultado::Forma(total)
    }

    fn somar_classe(&self, total: &mut Forma, c: &ClasseDecl, ctx: &Rc<Contexto>, prof: u8) {
        let corpo = ObjTipo {
            membros: c.membros.clone(),
            aberto: c.aberta,
        };
        total.mesclar(self.forma_de_corpo(&total.nome.clone(), &corpo, ctx));
        match &c.estende {
            Heranca::Nenhuma => {}
            Heranca::Opaca => total.aberta = true,
            Heranca::Tipo(t) => self.somar_base(total, t, ctx, &c.params, prof),
        }
    }

    /// Fold a base type (`extends X`) into `total`; an unresolvable base opens the shape.
    fn somar_base(
        &self,
        total: &mut Forma,
        base: &TipoExpr,
        ctx: &Rc<Contexto>,
        livres: &[String],
        prof: u8,
    ) {
        match self.resolver(base, ctx, livres, prof + 1) {
            Resultado::Forma(f) => total.mesclar(f),
            Resultado::Uniao(_) | Resultado::Desconhecido => total.aberta = true,
        }
    }

    fn forma_de_corpo(&self, nome: &str, corpo: &ObjTipo, ctx: &Rc<Contexto>) -> Forma {
        let mut f = Forma::vazia(nome, Origem::Projeto);
        f.aberta = corpo.aberto;
        for m in &corpo.membros {
            f.membros.insert(m.nome.clone());
            if let Some(t) = &m.tipo {
                f.tipos.insert(m.nome.clone(), (t.clone(), ctx.clone()));
            }
        }
        f
    }

    /// The shape of the INSTANCES of the class `nome` declared in `ctx`.
    pub fn forma_da_classe(&self, nome: &str, ctx: &Rc<Contexto>, prof: u8) -> Resultado {
        let Some(decls) = ctx.info.decls.get(nome) else {
            return Resultado::Desconhecido;
        };
        let classes: Vec<&Decl> = decls
            .iter()
            .filter(|d| matches!(d, Decl::Classe(_) | Decl::Interface { .. }))
            .collect();
        if classes.is_empty() {
            return Resultado::Desconhecido;
        }
        self.formar(nome, &classes, ctx, prof)
    }
}

fn eh_decl_de_tipo(d: &Decl) -> bool {
    matches!(
        d,
        Decl::Interface { .. } | Decl::Alias { .. } | Decl::Classe(_)
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vgp::lang::parse;
    use crate::vgp::lang::typescript::modulos::resumir;
    use crate::vgp::lang::typescript::projeto::Projeto;
    use std::collections::HashMap as Mapa;
    use std::path::Path;

    fn ctx_de(src: &str, caminho: &str) -> Rc<Contexto> {
        let arvore = parse("typescript", src).expect("parse");
        Rc::new(Contexto {
            caminho: PathBuf::from(caminho),
            info: Rc::new(resumir(src, arvore.root_node(), false, false)),
        })
    }

    fn forma(src: &str, nome: &str) -> Forma {
        let sobrepor = Mapa::new();
        let projeto = Projeto::novo(Path::new("/p"), &sobrepor);
        let car = Carregador::novo(&projeto);
        let ctx = ctx_de(src, "/p/a.ts");
        let r = Resolvedor::novo(&car);
        match r.resolver(
            &TipoExpr::Ref {
                nome: nome.into(),
                qual: vec![],
                args: vec![],
            },
            &ctx,
            &[],
            0,
        ) {
            Resultado::Forma(f) => f,
            outro => panic!("esperava Forma, veio {outro:?}"),
        }
    }

    #[test]
    fn interface_fechada_nao_tem_o_que_nao_declarou() {
        let f = forma("interface P { a: number; b(): void }", "P");
        assert!(!f.aberta);
        assert!(!f.nao_tem("a") && !f.nao_tem("b"));
        assert!(f.nao_tem("c"));
        assert!(
            !f.nao_tem("toString"),
            "membros de Object valem para qualquer objeto"
        );
    }

    #[test]
    fn extends_resolvido_soma_e_nao_resolvido_abre() {
        let f = forma("interface A { x: 1 } interface B extends A { y: 2 }", "B");
        assert!(!f.nao_tem("x") && !f.nao_tem("y") && f.nao_tem("z"));
        let f = forma("interface B extends Externa { y: 2 }", "B");
        assert!(f.aberta && !f.nao_tem("qualquer"));
    }

    #[test]
    fn indice_e_mapeado_abrem() {
        let f = forma("interface A { [k: string]: unknown; x: 1 }", "A");
        assert!(f.aberta);
    }

    #[test]
    fn alias_de_objeto_e_interseccao() {
        let f = forma("type O = { a: 1 } & { b: 2 };", "O");
        assert!(!f.nao_tem("a") && !f.nao_tem("b") && f.nao_tem("c"));
        let f = forma("type O = { a: 1 } & Desconhecida;", "O");
        assert!(f.aberta);
    }

    #[test]
    fn utilitarios_mantem_as_chaves_do_primeiro_argumento() {
        let f = forma("interface A { x: 1 } type P = Partial<A>;", "P");
        assert!(!f.nao_tem("x") && f.nao_tem("y"));
    }

    #[test]
    fn classe_com_base_externa_fica_aberta_e_com_base_local_soma() {
        let f = forma("class A { a = 1 } class B extends A { b = 2 }", "B");
        assert!(!f.nao_tem("a") && !f.nao_tem("b") && f.nao_tem("c"));
        let f = forma("class B extends React.Component { b = 2 }", "B");
        assert!(f.aberta);
    }

    #[test]
    fn parametro_generico_e_desconhecido() {
        let sobrepor = Mapa::new();
        let projeto = Projeto::novo(Path::new("/p"), &sobrepor);
        let car = Carregador::novo(&projeto);
        let ctx = ctx_de("interface A<T> { x: T }", "/p/a.ts");
        let r = Resolvedor::novo(&car);
        let t = TipoExpr::Ref {
            nome: "T".into(),
            qual: vec![],
            args: vec![],
        };
        assert!(matches!(
            r.resolver(&t, &ctx, &["T".to_string()], 0),
            Resultado::Desconhecido
        ));
    }

    #[test]
    fn uniao_descarta_null_e_resolve_o_resto() {
        let f = forma("interface A { x: 1 } type U = A | null | undefined;", "U");
        assert!(!f.nao_tem("x"));
    }

    #[test]
    fn builtin_vem_da_tabela_gerada() {
        let f = forma_builtin("Date").expect("Date");
        assert!(!f.nao_tem("getTime") && f.nao_tem("vgpPropAusente"));
        assert!(f.origem == Origem::Builtin && !f.aberta);
        assert!(forma_builtin("NaoExiste").is_none());
    }

    #[test]
    fn import_de_modulo_do_projeto_resolve_a_interface() {
        let tmp = tempfile::tempdir().expect("tmp");
        let raiz = tmp.path();
        std::fs::write(
            raiz.join("tipos.ts"),
            "export interface Opts { a: number }\n",
        )
        .expect("w");
        let sobrepor = Mapa::new();
        let projeto = Projeto::novo(raiz, &sobrepor);
        let car = Carregador::novo(&projeto);
        let arq = raiz.join("a.ts");
        let src = "import { Opts } from './tipos';";
        let ctx = ctx_de(src, arq.to_str().expect("utf8"));
        let r = Resolvedor::novo(&car);
        let t = TipoExpr::Ref {
            nome: "Opts".into(),
            qual: vec![],
            args: vec![],
        };
        match r.resolver(&t, &ctx, &[], 0) {
            Resultado::Forma(f) => assert!(!f.nao_tem("a") && f.nao_tem("b")),
            outro => panic!("{outro:?}"),
        }
    }
}
