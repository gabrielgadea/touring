//! The TS/TSX/JS check driver: parse the proposed content, summarise it, run the checks, hand the
//! candidate findings (each with the [`Alvo`] it is about) to `vgp::check`, which decides severity.
//!
//! Classes (VL0 §4): `import` (module that does not exist; name the module does not export),
//! `nome` (identifier declared nowhere), `componente` (`<Comp>` neither imported nor defined),
//! `atributo` (member a provable receiver type lacks), `kwarg` (key of an object literal the target
//! type lacks) and — in `jsx.rs` — `prop` (attribute the component's props lack).
//!
//! Discipline: a check yields a finding only when the WHOLE chain closed. Every gap — unresolved
//! module, open shape, unknown receiver, narrowing in play — is an abstention, and abstentions that
//! the reader should know about travel in `Verificacao::nao_medido`. Candidates that depend on a
//! search of the project's other sources (`Requisito`) are confirmed together, in ONE walk.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use tree_sitter::Node;

use super::decls::{Decl, Param};
use super::escopo::{self, Analise, BindTipo};
use super::globais::{Busca, Mundo, Requisito, eh_global_conhecido};
use super::inferencia::Inferidor;
use super::jsdoc;
use super::modulos::{Carregador, Prova, resumir};
use super::projeto::{Projeto, Resolvido, normalizar};
use super::tipos::{Contexto, Forma, Origem, Resolvedor, Resultado};
use super::util::{conteudo_string, linha, mais_proximo, reais_de, texto, ultimo_segmento};
use crate::vgp::lang::{Verificacao, parse};
use crate::vgp::politica::{Alvo, roda_agora};
use crate::vgp::{CheckInput, Finding, is_inline};

/// Findings per file beyond which the output stops teaching and starts shouting.
const MAX_ACHADOS: usize = 60;

/// A finding before it becomes a [`Finding`]: later guards may still drop it.
pub(super) struct Candidato {
    pub classe: &'static str,
    pub nome: String,
    pub linha: usize,
    pub alvo: Alvo,
    pub motivo: String,
    pub reais: Vec<String>,
    /// What must still be confirmed by searching the project's other sources.
    pub requisito: Requisito,
}

impl Candidato {
    pub fn novo(
        classe: &'static str,
        nome: &str,
        linha: usize,
        alvo: Alvo,
        motivo: String,
        reais: Vec<String>,
    ) -> Self {
        Self {
            classe,
            nome: nome.to_string(),
            linha,
            alvo,
            motivo,
            reais,
            requisito: Requisito::Nenhum,
        }
    }

    fn exigindo(mut self, requisito: Requisito) -> Self {
        self.requisito = requisito;
        self
    }
}

/// What an `import`/`export … from`/`require` asks of the module it names.
struct Pedido {
    nome: String,
    linha: usize,
}

/// What a node cites as a module.
enum Citacao {
    Literal(String, usize, Vec<Pedido>),
    Dinamica,
    Nenhuma,
}

/// Run all the checks of `lang` on `input.content`.
pub(super) fn verificar(lang: &'static str, input: &CheckInput) -> Verificacao {
    if let Some(motivo) = arquivo_empacotado(&input.content) {
        return Verificacao::nao_medido(motivo);
    }
    let arvore = match parse(lang, &input.content) {
        Ok(a) => a,
        Err(e) => return Verificacao::nao_medido(e),
    };
    let raiz = arvore.root_node();
    if !raiz.has_error() {
        return verificar_arvore(lang, input, &input.content, raiz);
    }
    let invalida = |raiz: Node| {
        Verificacao::nao_medido(format!(
            "sintaxe `{lang}` inválida (linha {}): nada foi verificado",
            primeira_linha_com_erro(raiz)
        ))
    };
    // The grammar chokes on a bare `&` in JSX text (`<h3>Drag & drop</h3>`); the check goes on with
    // that text blanked out, but only when nothing else in the file is broken.
    let Some(limpo) = sem_texto_jsx_invalido(&input.content, raiz) else {
        return invalida(raiz);
    };
    match parse(lang, &limpo) {
        Ok(a) if !a.root_node().has_error() => verificar_arvore(lang, input, &limpo, a.root_node()),
        _ => invalida(raiz),
    }
}

/// The DSL functions the tree-sitter CLI provides to a `grammar.js`.
const DSL_TREE_SITTER: &[&str] = &[
    "alias", "blank", "choice", "field", "grammar", "optional", "prec", "repeat", "repeat1",
    "reserved", "seq", "token",
];

/// A line longer than this makes the file a minified/bundled artefact, not hand-written source.
const LINHA_MAXIMA: usize = 2000;

/// Minified and bundled files resolve their names through closures, UMD wrappers and prototype
/// patches this check cannot follow: nothing in them is proof of anything.
fn arquivo_empacotado(src: &str) -> Option<String> {
    let maior = src.lines().map(str::len).max().unwrap_or(0);
    (maior > LINHA_MAXIMA).then(|| {
        format!("arquivo minificado ou empacotado (linha de {maior} bytes): nada foi verificado")
    })
}

/// `src` with the byte ranges of every text-only `ERROR` node inside a JSX element blanked to
/// spaces (newlines kept, so offsets and lines do not move). `None` when ANY error is of another
/// kind — a missing token, or an error that holds more than loose words — because then the tree
/// around it cannot be trusted.
fn sem_texto_jsx_invalido(src: &str, raiz: Node) -> Option<String> {
    let mut faixas = Vec::new();
    let mut pilha = vec![raiz];
    while let Some(n) = pilha.pop() {
        if n.is_missing() {
            return None;
        }
        if n.is_error() {
            if !e_texto_de_jsx(src, n) {
                return None;
            }
            faixas.push(n.byte_range());
        } else if n.has_error() {
            let mut c = n.walk();
            pilha.extend(n.children(&mut c));
        }
    }
    let mut bytes = src.as_bytes().to_vec();
    for faixa in faixas {
        for b in &mut bytes[faixa] {
            if !matches!(*b, b'\n' | b'\r') {
                *b = b' ';
            }
        }
    }
    String::from_utf8(bytes).ok()
}

/// An `ERROR` node that sits directly in a JSX element and holds only loose leaf tokens, none of
/// which can open an expression or a tag.
fn e_texto_de_jsx(src: &str, erro: Node) -> bool {
    let em_jsx = erro
        .parent()
        .is_some_and(|p| matches!(p.kind(), "jsx_element" | "jsx_fragment"));
    if !em_jsx || texto(src, erro).contains(['{', '}', '<', '>']) {
        return false;
    }
    let mut c = erro.walk();
    erro.children(&mut c)
        .all(|f| f.child_count() == 0 && !f.is_error() && !f.is_missing())
}

/// Run the checks over a syntactically clean tree of `src`.
fn verificar_arvore<'t>(
    lang: &'static str,
    input: &CheckInput,
    src: &'t str,
    raiz: Node<'t>,
) -> Verificacao {
    let inline = is_inline(&input.path);
    let arquivo = caminho_efetivo(input, inline);
    let projeto = Projeto::novo(&input.project_root, &input.overlays);
    let carregador = Carregador::novo(&projeto);
    let dts = input.path.ends_with(".d.ts");
    let info = Rc::new(resumir(src, raiz, dts, lang == "javascript"));
    carregador.semear(&arquivo, info.clone());
    let eu = Rc::new(Contexto {
        caminho: arquivo.clone(),
        info,
    });
    let escopo = escopo::analisar(raiz, src);
    let resolvedor = Resolvedor::novo(&carregador);
    let mundo = Mundo::novo(&projeto, &arquivo, inline);
    let mut v = Verificador {
        lang,
        src,
        raiz,
        arquivo: &arquivo,
        dts,
        projeto: &projeto,
        carregador: &carregador,
        resolvedor,
        mundo,
        escopo: &escopo,
        eu,
        modular: false,
        roda_agora: roda_agora(
            &input.path,
            &input.project_root,
            // V-4 (31/10/2026): o HEAD separou o CONTEXTO — a política decide
            // a severidade nele (`roda_agora(path, root, contexto)`). O
            // adaptador não recebe o contexto (o `CheckInput` não o carrega),
            // então a PERGUNTA dele ("este arquivo roda sozinho?") é a da
            // execução — que é o que a `roda_agora` de 2 argumentos (a que o
            // patch do `vts` foi feito) respondia. Quem decide o MOMENTO é a
            // política, que o recebe: no contexto de escrita um fragmento em
            // `/tmp` avisa (o FRAG, 10/10/2026), na execução ele nega.
            //
            // O caminho é QUALIFICADO porque `super::tipos::Contexto` é o
            // contexto do TypeScript, não o da política.
            crate::vgp::politica::Contexto::Execucao,
        ),
        achados: Vec::new(),
        motivos: Vec::new(),
        pacotes: Vec::new(),
        vistos: HashSet::new(),
        condicoes: RefCell::new(HashMap::new()),
    };
    v.rodar();
    v.finalizar()
}

/// The file's path for resolution: inline code runs from the project root.
fn caminho_efetivo(input: &CheckInput, inline: bool) -> PathBuf {
    if inline {
        return input.project_root.join("<inline>");
    }
    let p = Path::new(&input.path);
    if p.is_relative() {
        input.project_root.join(p)
    } else {
        p.to_path_buf()
    }
}

fn primeira_linha_com_erro(raiz: Node) -> usize {
    let mut pilha = vec![raiz];
    let mut menor: Option<usize> = None;
    while let Some(n) = pilha.pop() {
        if n.is_error() || n.is_missing() {
            let l = linha(n);
            menor = Some(menor.map_or(l, |m| m.min(l)));
        }
        if n.has_error() {
            let mut c = n.walk();
            pilha.extend(n.children(&mut c));
        }
    }
    menor.unwrap_or(1)
}

/// How many package names the `nao_medido` line spells out before counting the rest.
const PACOTES_NA_LINHA: usize = 6;

/// The package a bare specifier names: `lodash/fp` → `lodash`, `@scope/pkg/sub` → `@scope/pkg`,
/// `node:fs/promises` → `node:fs`.
fn nome_do_pacote(spec: &str) -> String {
    let mut partes = spec.split('/');
    let primeiro = partes.next().unwrap_or(spec);
    match partes.next() {
        Some(segundo) if primeiro.starts_with('@') => format!("{primeiro}/{segundo}"),
        _ => primeiro.to_string(),
    }
}

/// The ONE `nao_medido` line for the external packages a file imports.
fn linha_dos_pacotes(pacotes: &[String]) -> Option<String> {
    if pacotes.is_empty() {
        return None;
    }
    let mostrados = pacotes
        .iter()
        .take(PACOTES_NA_LINHA)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    let resto = pacotes.len().saturating_sub(PACOTES_NA_LINHA);
    let cauda = if resto > 0 {
        format!(" e mais {resto}")
    } else {
        String::new()
    };
    Some(format!(
        "{} pacote(s) externo(s) importado(s), não verificados na camada 1: {mostrados}{cauda} (a camada 2 do projeto verifica)",
        pacotes.len()
    ))
}

/// What the search of the other sources decides about one candidate.
enum Decisao {
    /// The finding stands (a truncated walk may have demoted it to `Alvo::Incerto`).
    Manter(Candidato),
    /// Another source defines the name: there is nothing to report.
    Descartar,
    /// The walk was cut short, so the absence is unproven: report it as not measured, never as allow.
    SemProva(String),
}

fn decidir(
    mut c: Candidato,
    globais: &HashMap<String, Busca>,
    fontes: &HashMap<String, Busca>,
) -> Decisao {
    match c.requisito {
        Requisito::Nenhum => Decisao::Manter(c),
        Requisito::GlobalAlheio => match globais.get(&c.nome) {
            Some(Busca::NaoAchou { completo }) => {
                if !completo {
                    c.alvo = Alvo::Incerto;
                }
                Decisao::Manter(c)
            }
            _ => Decisao::Descartar,
        },
        Requisito::OutrosFontes => match fontes.get(&c.nome) {
            Some(Busca::NaoAchou { completo: true }) => Decisao::Manter(c),
            Some(Busca::NaoAchou { completo: false }) => Decisao::SemProva(c.nome),
            _ => Decisao::Descartar,
        },
    }
}

/// The state of one check.
pub(super) struct Verificador<'a, 't> {
    pub lang: &'static str,
    pub src: &'t str,
    pub raiz: Node<'t>,
    pub arquivo: &'a Path,
    pub dts: bool,
    pub projeto: &'a Projeto<'a>,
    pub carregador: &'a Carregador<'a>,
    pub resolvedor: Resolvedor<'a>,
    pub mundo: Mundo<'a>,
    pub escopo: &'a Analise<'t>,
    pub eu: Rc<Contexto>,
    /// The file is a module (import/export/`require`/CommonJS) — not a classic script whose free
    /// names the host (a `<script>` page, a runner) may provide.
    modular: bool,
    /// The code runs right now (inline, `/tmp/`): Node is the host, so nothing else provides names.
    roda_agora: bool,
    achados: Vec<Finding>,
    motivos: Vec<String>,
    /// External packages the file imports (names, in source order, once each): their members are
    /// not checked on layer 1, which must be said — a package import never vanishes silently.
    pacotes: Vec<String>,
    vistos: HashSet<(String, String)>,
    /// function node id → names that appear in conditions inside it (the narrowing guard).
    condicoes: RefCell<HashMap<usize, HashSet<String>>>,
}

impl<'a, 't> Verificador<'a, 't> {
    fn rodar(&mut self) {
        self.verificar_imports();
        if self.dts {
            self.abster("arquivo de declaração (.d.ts): só os imports foram verificados");
            return;
        }
        if self.escopo.tem_with {
            self.abster("`with` dissolve o escopo estático: nomes não verificados");
        }
        let mut candidatos = if self.executor_injeta_nomes() {
            self.abster(
                "`return` no topo: o arquivo roda dentro de um executor que injeta nomes — nomes livres não verificados",
            );
            Vec::new()
        } else {
            self.nomes_candidatos()
        };
        candidatos.extend(self.membros_ausentes());
        candidatos.extend(self.literais_ausentes());
        if self.lang == "tsx" {
            candidatos.extend(self.props_ausentes());
        }
        let (confirmados, sem_prova) = self.confirmar(candidatos);
        for c in confirmados {
            self.achar(c);
        }
        if let Some(primeiro) = sem_prova.first() {
            self.abster(format!(
                "a vizinhança não foi varrida por inteiro (orçamento de arquivos/tempo): `{primeiro}`{} não verificado",
                if sem_prova.len() > 1 { " e outros" } else { "" }
            ));
        }
    }

    fn finalizar(mut self) -> Verificacao {
        self.achados.sort_by_key(|f| f.linha);
        let mut motivos = std::mem::take(&mut self.motivos);
        motivos.extend(linha_dos_pacotes(&self.pacotes));
        let nao_medido = (!motivos.is_empty()).then(|| motivos.join("; "));
        Verificacao {
            achados: self.achados,
            nao_medido,
        }
    }

    /// Record something the check wanted to measure and could not (deduplicated, capped).
    pub fn abster(&mut self, motivo: impl Into<String>) {
        let m = motivo.into();
        if self.motivos.len() < 3 && !self.motivos.contains(&m) {
            self.motivos.push(m);
        }
    }

    /// Turn a candidate into a finding — one per (class, name), with the «você quis dizer» hint.
    pub fn achar(&mut self, c: Candidato) {
        if self.achados.len() >= MAX_ACHADOS
            || !self.vistos.insert((c.classe.to_string(), c.nome.clone()))
        {
            return;
        }
        let sugestao = match mais_proximo(&c.nome, &c.reais) {
            Some(s) => format!("{}. Você quis dizer `{s}`?", c.motivo),
            None => c.motivo,
        };
        self.achados.push(Finding::candidato(
            c.classe, &c.nome, c.linha, c.alvo, sugestao, c.reais,
        ));
    }

    /// Every node of the tree, pre-order, whose kind is in `tipos`.
    pub fn nos_de_tipo(&self, tipos: &[&str]) -> Vec<Node<'t>> {
        let mut out = Vec::new();
        let mut pilha = vec![self.raiz];
        while let Some(n) = pilha.pop() {
            if tipos.contains(&n.kind()) {
                out.push(n);
            }
            let mut c = n.walk();
            let mut filhos: Vec<Node<'t>> = n.named_children(&mut c).collect();
            filhos.reverse();
            pilha.extend(filhos);
        }
        out
    }

    pub fn inferidor(&self) -> Inferidor<'_, 't> {
        Inferidor {
            src: self.src,
            escopo: self.escopo,
            resolvedor: &self.resolvedor,
            eu: &self.eu,
            js: self.lang == "javascript",
        }
    }

    /// Settle the candidates that depend on the project's other sources, with ONE search for all:
    /// the ones that stand, and the names whose absence a truncated walk could not prove.
    fn confirmar(&self, candidatos: Vec<Candidato>) -> (Vec<Candidato>, Vec<String>) {
        let palavras = |r: Requisito| -> Vec<String> {
            let mut v: Vec<String> = candidatos
                .iter()
                .filter(|c| c.requisito == r)
                .map(|c| c.nome.clone())
                .collect();
            v.sort();
            v.dedup();
            v
        };
        let (globais, fontes) = self.mundo.buscar_varias(
            &palavras(Requisito::GlobalAlheio),
            &palavras(Requisito::OutrosFontes),
        );
        let mut confirmados = Vec::new();
        let mut sem_prova = Vec::new();
        for c in candidatos {
            match decidir(c, &globais, &fontes) {
                Decisao::Manter(c) => confirmados.push(c),
                Decisao::Descartar => {}
                Decisao::SemProva(nome) => sem_prova.push(nome),
            }
        }
        (confirmados, sem_prova)
    }

    // ------------------------------------------------------------------ import

    fn verificar_imports(&mut self) {
        let mut citados: Vec<(String, usize, Vec<Pedido>)> = Vec::new();
        let mut dinamico = false;
        let nos = self.nos_de_tipo(&[
            "import_statement",
            "export_statement",
            "import_require_clause",
            "call_expression",
        ]);
        for n in nos {
            match self.modulo_citado(n) {
                Citacao::Literal(spec, linha, pedidos) => citados.push((spec, linha, pedidos)),
                Citacao::Dinamica => dinamico = true,
                Citacao::Nenhuma => {}
            }
        }
        self.modular =
            self.eu.info.eh_modulo || self.eu.info.cjs || !citados.is_empty() || dinamico;
        if dinamico {
            self.abster("import/require dinâmico: o especificador não é uma string literal");
        }
        for (spec, linha, pedidos) in citados {
            self.checar_especificador(&spec, linha, &pedidos);
        }
    }

    /// The module a node names, if it names one.
    fn modulo_citado(&self, n: Node<'t>) -> Citacao {
        match n.kind() {
            "import_statement" => self.citacao_de_fonte(n, pedidos_de_import(n, self.src)),
            "export_statement" if n.child_by_field_name("source").is_some() => {
                self.citacao_de_fonte(n, pedidos_de_export(n, self.src))
            }
            "import_require_clause" => self.citacao_de_fonte(n, Vec::new()),
            "call_expression" => self.citacao_de_chamada(n),
            _ => Citacao::Nenhuma,
        }
    }

    fn citacao_de_fonte(&self, n: Node<'t>, pedidos: Vec<Pedido>) -> Citacao {
        let Some(fonte) = n.child_by_field_name("source") else {
            return Citacao::Nenhuma;
        };
        match conteudo_string(self.src, fonte) {
            Some(spec) => Citacao::Literal(spec, linha(fonte), pedidos),
            None => Citacao::Dinamica,
        }
    }

    /// `import('./x')` and an unshadowed `require('./x')`.
    fn citacao_de_chamada(&self, n: Node<'t>) -> Citacao {
        let Some(f) = n.child_by_field_name("function") else {
            return Citacao::Nenhuma;
        };
        let eh_require = f.kind() == "identifier"
            && texto(self.src, f) == "require"
            && self.escopo.resolver(n, "require").is_none();
        if f.kind() != "import" && !eh_require {
            return Citacao::Nenhuma;
        }
        let Some(arg) = n
            .child_by_field_name("arguments")
            .and_then(|a| a.named_child(0))
        else {
            return Citacao::Nenhuma;
        };
        match conteudo_string(self.src, arg) {
            Some(spec) => Citacao::Literal(spec, linha(arg), Vec::new()),
            None => Citacao::Dinamica,
        }
    }

    fn checar_especificador(&mut self, spec: &str, linha: usize, pedidos: &[Pedido]) {
        match self.projeto.resolver(self.arquivo, spec) {
            Resolvido::Arquivo(p) => self.checar_nomes_pedidos(&p, spec, pedidos),
            Resolvido::Inexistente => self.modulo_inexistente(spec, linha),
            // a package's members are not measured (that needs its `.d.ts`): say so, once per file
            Resolvido::Pacote => self.registrar_pacote(spec),
            // a specifier no static resolver can honour is a module this check did NOT look at
            Resolvido::Fora(motivo) => self.abster(format!("`{spec}`: {motivo}")),
        }
    }

    fn registrar_pacote(&mut self, spec: &str) {
        let nome = nome_do_pacote(spec);
        if !self.pacotes.contains(&nome) {
            self.pacotes.push(nome);
        }
    }

    fn modulo_inexistente(&mut self, spec: &str, linha: usize) {
        let dir = self.arquivo.parent().unwrap_or(Path::new("."));
        let irmaos = irmaos_em_disco(&normalizar(&dir.join(spec)));
        self.achar(Candidato::novo(
            "import",
            &ultimo_segmento(spec),
            linha,
            Alvo::Projeto,
            format!(
                "módulo `{spec}` não existe (nenhum arquivo `.ts`/`.tsx`/`.js`/… nem `index.*` casa a partir de `{}`)",
                dir.display()
            ),
            irmaos,
        ));
    }

    fn checar_nomes_pedidos(&mut self, caminho: &Path, spec: &str, pedidos: &[Pedido]) {
        if pedidos.is_empty() {
            return;
        }
        let Some(info) = self.carregador.carregar(caminho) else {
            return; // not code (json, css, image…) or over the load budget
        };
        for p in pedidos {
            match self.carregador.tem_export(caminho, &info, &p.nome, 0) {
                Prova::Sim => {}
                Prova::Incerto(m) => self.abster(format!("`{spec}`: {m}")),
                Prova::Nao => {
                    let reais = reais_de(&self.carregador.nomes_exportados(caminho, &info, 0));
                    self.achar(Candidato::novo(
                        "import",
                        &p.nome,
                        p.linha,
                        Alvo::Projeto,
                        format!("o módulo `{spec}` não exporta `{}`", p.nome),
                        reais,
                    ));
                }
            }
        }
    }

    // ------------------------------------------------------------------ nome / componente

    /// A `return` outside any function is not valid in an ES module or a classic script: the file is
    /// the body of a function some host (a workflow runner, a loader) calls — and that host decides
    /// which names exist. CommonJS is the exception (Node wraps the module in a function itself).
    fn executor_injeta_nomes(&self) -> bool {
        !(self.eu.info.cjs || self.src.contains("require("))
            && self
                .nos_de_tipo(&["return_statement"])
                .into_iter()
                .any(|n| funcao_envolvente(n).is_none())
    }

    /// The names the tree-sitter CLI injects into a `grammar.js` (its DSL).
    fn global_do_dsl(&self, nome: &str) -> bool {
        self.arquivo.file_name().is_some_and(|f| f == "grammar.js")
            && DSL_TREE_SITTER.contains(&nome)
    }

    /// Free identifiers that no table knows: they still need the «comes from elsewhere» search.
    fn nomes_candidatos(&self) -> Vec<Candidato> {
        if self.escopo.tem_with {
            return Vec::new();
        }
        self.escopo
            .livres
            .iter()
            .filter(|r| !eh_global_conhecido(&r.nome))
            .filter(|r| !self.escopo.detectados.contains(&r.nome))
            .filter(|r| !self.global_do_dsl(&r.nome))
            .filter(|r| !(self.lang == "javascript" && self.escopo.atribuidos.contains(&r.nome)))
            .map(|r| {
                let (classe, motivo) = if r.componente {
                    (
                        "componente",
                        format!(
                            "componente `<{}>` não está importado nem definido neste arquivo",
                            r.nome
                        ),
                    )
                } else {
                    (
                        "nome",
                        format!(
                            "`{}` não está declarado neste arquivo nem é um global conhecido",
                            r.nome
                        ),
                    )
                };
                // a classic script's free names may be provided by the page that loads it: never deny
                let (alvo, motivo) = if self.modular || self.roda_agora {
                    (Alvo::Projeto, motivo)
                } else {
                    (
                        Alvo::Incerto,
                        format!("{motivo} (script clássico: a página ou o executor que o carrega pode fornecê-lo)"),
                    )
                };
                Candidato::novo(classe, &r.nome, linha(r.no), alvo, motivo, Vec::new())
                    .exigindo(Requisito::GlobalAlheio)
            })
            .collect()
    }

    // ------------------------------------------------------------------ atributo

    /// `receiver.member` where the receiver's type is PROVED and closed and lacks `member`.
    fn membros_ausentes(&self) -> Vec<Candidato> {
        let inf = self.inferidor();
        let mut out = Vec::new();
        for n in self.nos_de_tipo(&["member_expression"]) {
            let (Some(obj), Some(prop)) = (
                n.child_by_field_name("object"),
                n.child_by_field_name("property"),
            ) else {
                continue;
            };
            if prop.kind() != "property_identifier" || e_alvo_de_atribuicao(n) {
                continue;
            }
            let nome = texto(self.src, prop);
            let Resultado::Forma(forma) = inf.inferir(obj, 0) else {
                continue;
            };
            if !forma.nao_tem(nome)
                || self.estreitado(obj)
                || self.remendado_neste_arquivo(&forma, nome)
            {
                continue;
            }
            let builtin = forma.origem == Origem::Builtin;
            let c = Candidato::novo(
                "atributo",
                nome,
                linha(prop),
                if builtin {
                    Alvo::Externo
                } else {
                    Alvo::Projeto
                },
                format!("`{}` não tem o membro `{nome}`", forma.nome),
                reais_de(&forma.membros),
            );
            // a builtin member can be added by a prototype patch / interface merge elsewhere
            out.push(if builtin {
                c.exigindo(Requisito::OutrosFontes)
            } else {
                c
            });
        }
        out
    }

    /// Does this very file patch the builtin (`Date.prototype.x = …`)?
    fn remendado_neste_arquivo(&self, forma: &Forma, nome: &str) -> bool {
        forma.origem == Origem::Builtin
            && (self.src.contains(&format!("prototype.{nome}"))
                || self.src.contains(&format!("prototype[\"{nome}\"]"))
                || self.src.contains(&format!("prototype['{nome}']"))
                || self
                    .src
                    .contains(&format!("defineProperty({}.prototype", forma.nome)))
    }

    /// Is the receiver variable involved in a condition of its function (`instanceof`, `in`, a type
    /// guard call, a truthiness test…)? Then TypeScript may have narrowed it past its declaration.
    fn estreitado(&self, obj: Node<'t>) -> bool {
        if obj.kind() != "identifier" {
            return false;
        }
        let nome = texto(self.src, obj);
        let escopo_fn = funcao_envolvente(obj).unwrap_or(self.raiz);
        let mut cache = self.condicoes.borrow_mut();
        let nomes = cache
            .entry(escopo_fn.id())
            .or_insert_with(|| nomes_em_condicoes(escopo_fn, self.src));
        nomes.contains(nome)
    }

    // ------------------------------------------------------------------ kwarg

    /// Keys of an object literal that the (closed, project) type it is checked against lacks.
    fn literais_ausentes(&self) -> Vec<Candidato> {
        let inf = self.inferidor();
        let mut out = Vec::new();
        for (objeto, alvo) in self.literais_com_tipo_esperado(&inf) {
            self.checar_literal(objeto, &alvo, &inf, &mut out, 0);
        }
        out
    }

    /// Object literals whose expected type is written next to them: an annotated declarator (or a
    /// JSDoc `@type`), a `satisfies`, an argument of a locally known function.
    fn literais_com_tipo_esperado(&self, inf: &Inferidor<'_, 't>) -> Vec<(Node<'t>, Resultado)> {
        let mut out = Vec::new();
        for n in self.nos_de_tipo(&[
            "variable_declarator",
            "satisfies_expression",
            "call_expression",
        ]) {
            match n.kind() {
                "variable_declarator" => out.extend(self.literal_de_declarador(n, inf)),
                "satisfies_expression" => out.extend(self.literal_de_satisfies(n, inf)),
                _ => out.extend(self.literais_em_argumentos(n, inf)),
            }
        }
        out
    }

    fn literal_de_declarador(
        &self,
        d: Node<'t>,
        inf: &Inferidor<'_, 't>,
    ) -> Option<(Node<'t>, Resultado)> {
        let valor = d
            .child_by_field_name("value")
            .filter(|v| v.kind() == "object")?;
        if let Some(t) = d.child_by_field_name("type") {
            return Some((valor, inf.de_anotacao(t)));
        }
        if self.lang != "javascript" {
            return None;
        }
        let tipo = jsdoc::tipo_da_declaracao(d.parent()?, self.src)?;
        Some((valor, inf.resolver_tipo(&tipo)))
    }

    fn literal_de_satisfies(
        &self,
        s: Node<'t>,
        inf: &Inferidor<'_, 't>,
    ) -> Option<(Node<'t>, Resultado)> {
        let (v, t) = (s.named_child(0)?, s.named_child(1)?);
        let v = if v.kind() == "parenthesized_expression" {
            v.named_child(0)?
        } else {
            v
        };
        (v.kind() == "object").then(|| (v, inf.de_anotacao(t)))
    }

    /// `f({ … })` where `f` is a top-level function/arrow of this module or a project import.
    fn literais_em_argumentos(
        &self,
        chamada: Node<'t>,
        inf: &Inferidor<'_, 't>,
    ) -> Vec<(Node<'t>, Resultado)> {
        let mut out = Vec::new();
        let (Some(f), Some(args)) = (
            chamada.child_by_field_name("function"),
            chamada.child_by_field_name("arguments"),
        ) else {
            return out;
        };
        if f.kind() != "identifier" {
            return out;
        }
        let nome = texto(self.src, f);
        let chamavel = self
            .escopo
            .resolver_no_topo(chamada, nome)
            .is_some_and(|l| {
                matches!(
                    l.tipo,
                    BindTipo::Funcao | BindTipo::Const | BindTipo::Import
                )
            });
        let Some((decl, ctx)) = chamavel.then(|| inf.funcao_chamavel(nome)).flatten() else {
            return out;
        };
        let params = params_da_decl(&decl);
        let mut c = args.walk();
        for (i, arg) in args.named_children(&mut c).enumerate() {
            let Some(Param {
                tipo: Some(t),
                rest: false,
            }) = params.get(i)
            else {
                continue;
            };
            if arg.kind() == "object" {
                out.push((arg, self.resolvedor.resolver(t, &ctx, &[], 0)));
            }
        }
        out
    }

    fn checar_literal(
        &self,
        objeto: Node<'t>,
        alvo: &Resultado,
        inf: &Inferidor<'_, 't>,
        out: &mut Vec<Candidato>,
        prof: u8,
    ) {
        let formas: Vec<&Forma> = match alvo {
            Resultado::Forma(f) => vec![f],
            Resultado::Uniao(fs) => fs.iter().collect(),
            Resultado::Desconhecido => return,
        };
        if prof > 3
            || formas
                .iter()
                .any(|f| f.aberta || f.origem != Origem::Projeto)
        {
            return;
        }
        let Some(chaves) = chaves_do_literal(objeto, self.src) else {
            return; // spread: the explicit keys are not the whole object
        };
        for (chave, no) in chaves
            .iter()
            .filter(|(c, _)| formas.iter().all(|f| f.nao_tem(c)))
        {
            out.push(Candidato::novo(
                "kwarg",
                chave,
                linha(*no),
                Alvo::Projeto,
                format!(
                    "a chave `{chave}` não existe no tipo `{}` (excesso de propriedade)",
                    formas[0].nome
                ),
                reais_de(formas.iter().flat_map(|f| f.membros.iter())),
            ));
        }
        if let [forma] = formas.as_slice() {
            self.checar_aninhados(objeto, forma, inf, out, prof);
        }
    }

    /// `{ inner: { … } }` where `inner` has a declared type.
    fn checar_aninhados(
        &self,
        objeto: Node<'t>,
        forma: &Forma,
        inf: &Inferidor<'_, 't>,
        out: &mut Vec<Candidato>,
        prof: u8,
    ) {
        let mut c = objeto.walk();
        for par in objeto.named_children(&mut c).filter(|p| p.kind() == "pair") {
            let (Some(k), Some(v)) = (
                par.child_by_field_name("key"),
                par.child_by_field_name("value"),
            ) else {
                continue;
            };
            if v.kind() != "object" || k.kind() != "property_identifier" {
                continue;
            }
            if let Some((t, ctx)) = forma.tipos.get(texto(self.src, k)) {
                let alvo = inf.resolvedor.resolver(t, ctx, &[], 0);
                self.checar_literal(v, &alvo, inf, out, prof + 1);
            }
        }
    }
}

fn params_da_decl(decl: &Decl) -> Vec<Param> {
    match decl {
        Decl::Funcao { params } => params.clone(),
        Decl::Variavel(v) => v.funcao.clone().unwrap_or_default(),
        _ => Vec::new(),
    }
}

/// `import d, { a, b as c } from …` ⇒ default + `a` + `b` (the names the module must export).
fn pedidos_de_import(n: Node, src: &str) -> Vec<Pedido> {
    let mut out = Vec::new();
    let mut c = n.walk();
    for clausula in n
        .named_children(&mut c)
        .filter(|f| f.kind() == "import_clause")
    {
        let mut cc = clausula.walk();
        for f in clausula.named_children(&mut cc) {
            match f.kind() {
                "identifier" => out.push(Pedido {
                    nome: "default".into(),
                    linha: linha(f),
                }),
                "named_imports" => out.extend(especificadores(f, "import_specifier", src)),
                _ => {}
            }
        }
    }
    out
}

/// `export { a, b as c } from …` ⇒ `a`, `b`.
fn pedidos_de_export(n: Node, src: &str) -> Vec<Pedido> {
    let mut c = n.walk();
    n.named_children(&mut c)
        .filter(|f| f.kind() == "export_clause")
        .flat_map(|cl| especificadores(cl, "export_specifier", src))
        .collect()
}

fn especificadores(pai: Node, tipo: &str, src: &str) -> Vec<Pedido> {
    let mut c = pai.walk();
    pai.named_children(&mut c)
        .filter(|s| s.kind() == tipo)
        .filter_map(|s| s.child_by_field_name("name"))
        .map(|nome| Pedido {
            nome: texto(src, nome).to_string(),
            linha: linha(nome),
        })
        .collect()
}

/// File stems next to the module that was not found — the real names a refusal can teach.
fn irmaos_em_disco(procurado: &Path) -> Vec<String> {
    let Some(pai) = procurado.parent() else {
        return Vec::new();
    };
    let Ok(entradas) = std::fs::read_dir(pai) else {
        return Vec::new();
    };
    let mut nomes: Vec<String> = entradas
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().to_string();
            let tronco = n.split('.').next().unwrap_or("").to_string();
            (!tronco.is_empty() && !tronco.starts_with('_')).then_some(tronco)
        })
        .collect();
    nomes.sort();
    nomes.dedup();
    nomes.truncate(40);
    nomes
}

/// Is `n` the left side of an assignment (an expando write: `obj.novo = 1`) or a `delete`/`++` target?
fn e_alvo_de_atribuicao(n: Node) -> bool {
    n.parent().is_some_and(|p| {
        (matches!(
            p.kind(),
            "assignment_expression" | "augmented_assignment_expression"
        ) && p
            .child_by_field_name("left")
            .is_some_and(|l| l.id() == n.id()))
            || matches!(p.kind(), "delete_expression" | "update_expression")
    })
}

const FUNCOES: &[&str] = &[
    "function_declaration",
    "function_expression",
    "function",
    "arrow_function",
    "method_definition",
    "generator_function",
    "generator_function_declaration",
];

/// The innermost function-like ancestor of `no`.
fn funcao_envolvente(no: Node) -> Option<Node> {
    let mut atual = no.parent();
    while let Some(n) = atual {
        if FUNCOES.contains(&n.kind()) {
            return Some(n);
        }
        atual = n.parent();
    }
    None
}

/// The part of `n` that is a condition, if `n` is a condition-bearing construct.
fn condicao_de<'t>(n: Node<'t>, src: &str) -> Option<Node<'t>> {
    match n.kind() {
        "if_statement" | "while_statement" | "do_statement" | "for_statement"
        | "ternary_expression" => n.child_by_field_name("condition"),
        "switch_statement" => n.child_by_field_name("value"),
        "binary_expression" => {
            let op = n
                .child_by_field_name("operator")
                .map_or("", |o| texto(src, o));
            matches!(op, "&&" | "||" | "??" | "instanceof" | "in").then_some(n)
        }
        "call_expression" => {
            let f = n
                .child_by_field_name("function")
                .map_or("", |f| texto(src, f));
            (f.starts_with("assert") || f == "invariant").then_some(n)
        }
        _ => None,
    }
}

/// Names appearing in conditions anywhere under `fn_no`.
fn nomes_em_condicoes(fn_no: Node, src: &str) -> HashSet<String> {
    let mut nomes = HashSet::new();
    let mut pilha = vec![fn_no];
    while let Some(n) = pilha.pop() {
        if let Some(c) = condicao_de(n, src) {
            coletar_identificadores(c, src, &mut nomes);
        }
        let mut cur = n.walk();
        pilha.extend(n.named_children(&mut cur));
    }
    nomes
}

fn coletar_identificadores(no: Node, src: &str, out: &mut HashSet<String>) {
    let mut pilha = vec![no];
    while let Some(n) = pilha.pop() {
        if n.kind() == "identifier" {
            out.insert(texto(src, n).to_string());
        }
        let mut c = n.walk();
        pilha.extend(n.named_children(&mut c));
    }
}

/// The explicit keys of an object literal; `None` if it spreads (`{ ...x }`).
fn chaves_do_literal<'t>(objeto: Node<'t>, src: &str) -> Option<Vec<(String, Node<'t>)>> {
    let mut chaves = Vec::new();
    let mut c = objeto.walk();
    for f in objeto.named_children(&mut c) {
        match f.kind() {
            "spread_element" => return None,
            "pair" | "method_definition" => {
                let k = f
                    .child_by_field_name("key")
                    .or_else(|| f.child_by_field_name("name"));
                if let Some((nome, no)) = k.and_then(|k| chave_estatica(k, src)) {
                    chaves.push((nome, no));
                }
            }
            "shorthand_property_identifier" => chaves.push((texto(src, f).to_string(), f)),
            _ => {}
        }
    }
    Some(chaves)
}

/// The name of a non-computed property key.
fn chave_estatica<'t>(k: Node<'t>, src: &str) -> Option<(String, Node<'t>)> {
    match k.kind() {
        "property_identifier" | "number" => Some((texto(src, k).to_string(), k)),
        "string" => conteudo_string(src, k).map(|s| (s, k)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidato(requisito: Requisito) -> Candidato {
        Candidato::novo("nome", "x", 1, Alvo::Projeto, "m".to_string(), Vec::new())
            .exigindo(requisito)
    }

    fn busca(b: Busca) -> HashMap<String, Busca> {
        [("x".to_string(), b)].into()
    }

    fn alvo(d: Decisao) -> Option<Alvo> {
        match d {
            Decisao::Manter(c) => Some(c.alvo),
            _ => None,
        }
    }

    #[test]
    fn decidir_mantem_derruba_ou_rebaixa_conforme_a_busca() {
        let vazio = HashMap::new();
        let achou = busca(Busca::Achou);
        let completo = busca(Busca::NaoAchou { completo: true });
        let truncado = busca(Busca::NaoAchou { completo: false });
        // sem requisito, o candidato fica como está
        let nenhum = decidir(candidato(Requisito::Nenhum), &vazio, &vazio);
        assert_eq!(alvo(nenhum), Some(Alvo::Projeto));
        // global alheio: achado em outra fonte cai; ausente com a varredura completa fica
        let cai = decidir(candidato(Requisito::GlobalAlheio), &achou, &vazio);
        assert!(matches!(cai, Decisao::Descartar));
        let fica = decidir(candidato(Requisito::GlobalAlheio), &completo, &vazio);
        assert_eq!(alvo(fica), Some(Alvo::Projeto));
        // varredura truncada: o achado sobrevive só como Incerto (aviso, nunca deny)
        let rebaixa = decidir(candidato(Requisito::GlobalAlheio), &truncado, &vazio);
        assert_eq!(alvo(rebaixa), Some(Alvo::Incerto));
        // outras fontes: o membro achado em outra fonte cai; só vale com a varredura completa
        let achado = decidir(candidato(Requisito::OutrosFontes), &vazio, &achou);
        assert!(matches!(achado, Decisao::Descartar));
        let ok = decidir(candidato(Requisito::OutrosFontes), &vazio, &completo);
        assert!(matches!(ok, Decisao::Manter(_)));
        // varredura truncada: a ausência não está provada — vira NÃO MEDIDO, nunca allow silencioso
        let incompleto = decidir(candidato(Requisito::OutrosFontes), &vazio, &truncado);
        assert!(matches!(incompleto, Decisao::SemProva(ref n) if n == "x"));
    }
}
