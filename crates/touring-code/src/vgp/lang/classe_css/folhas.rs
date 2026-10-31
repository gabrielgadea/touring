//! Reading stylesheets for the `classe_css` check: the classes a sheet defines, the sheets it
//! `@import`s, and the markers of a utility-class generator. Pure over a [`Leitor`] (overlays
//! first, then the disk, then the CDN cache for remote sheets) — no network: a remote sheet the
//! cache does not know is reported as unresolved, never fetched.

use std::cell::OnceCell;
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::time::SystemTime;

use parking_lot::Mutex;
use regex::Regex;
use tree_sitter::Node;

use crate::vgp::lang::cdn::cache::{self, Cache, Veredito};
use crate::vgp::lang::parse;

/// Largest stylesheet read; a bigger file is reported instead of parsed.
const MAX_BYTES_FOLHA: u64 = 4 * 1024 * 1024;

/// Deepest `@import` chain followed.
const MAX_PROFUNDIDADE: usize = 8;

/// Most sheets read for one check — a pathological import fan-out must not hold a hook.
const MAX_FOLHAS: usize = 256;

/// Parsed sheets the process remembers (daemon: every edit re-reads the same project sheets).
const MAX_MEMO: usize = 512;

/// How the unresolved-remote reason names the network side that fills the CDN cache.
const RESOLVEDOR: &str = "vgp::lang::cdn::resolvedor::preencher";

/// Sheet extensions that need a preprocessor: the class exists only after compiling.
const PREPROCESSADORES: [&str; 5] = [".scss", ".sass", ".less", ".styl", ".stylus"];

/// Directories (under the project root) a root-relative `/x.css` may be served from.
const RAIZES_WEB: [&str; 5] = ["", "public", "static", "www", "assets"];

static IMPORT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?s)@import\s+(?:url\(\s*)?(?:"([^"]*)"|'([^']*)'|([^\s)"';]+))"#)
        .expect("regex constante")
});

static PADRAO_CLASSE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"^\[\s*class\s*([~|^$*]?=)\s*(?:"([^"]*)"|'([^']*)'|([^\s\]]+))"#)
        .expect("regex constante")
});

/// A CSS comment (`/* … */`), stripped before any text scan so a commented-out `@import` or
/// `@apply` is not read.
static COMENTARIO: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)/\*.*?\*/").expect("regex constante"));

/// `.name` anywhere in CSS text: the lexical net under the grammar (see [`ler_css`]).
static CLASSE_LEXICA: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\.(-?[_A-Za-z][\w-]*)").expect("regex constante"));

static GERADOR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"@(?:tailwind|apply|theme|utility|plugin|config|source|custom-variant)\b|@import\s+(?:url\()?\s*["']tailwindcss"#,
    )
    .expect("regex constante")
});

/// (mtime, length) of a file on disk — the memo key.
type Assinatura = (SystemTime, u64);

/// Path → (signature when parsed, parsed sheet).
type Memo = HashMap<PathBuf, (Assinatura, Arc<Definicoes>)>;

static MEMO: LazyLock<Mutex<Memo>> = LazyLock::new(|| Mutex::new(HashMap::new()));

/// What one stylesheet says about classes.
#[derive(Debug, Default, Clone)]
pub(super) struct Definicoes {
    /// Class names with a selector somewhere in the sheet (escapes decoded).
    pub classes: HashSet<String>,
    /// `[class*="x"]`-style selectors: (operator, value). Any class they match is styled.
    pub padroes: Vec<(String, String)>,
    /// `@import` targets, as written.
    pub imports: Vec<String>,
    /// A utility-class generator marker in the text (`@tailwind`, `@apply`, …).
    pub gerador: Option<&'static str>,
}

/// Decode CSS escapes in a class name: `\:` → `:`, `\31 0` → `10`.
pub(super) fn desescapa(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut chars = css.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.peek().copied() {
            Some(h) if h.is_ascii_hexdigit() => {
                let mut hex = String::new();
                while hex.len() < 6 && chars.peek().is_some_and(char::is_ascii_hexdigit) {
                    hex.extend(chars.next());
                }
                if chars.peek().is_some_and(|w| w.is_whitespace()) {
                    chars.next();
                }
                out.push(
                    u32::from_str_radix(&hex, 16)
                        .ok()
                        .and_then(char::from_u32)
                        .unwrap_or('\u{FFFD}'),
                );
            }
            Some(outro) => {
                out.push(outro);
                chars.next();
            }
            None => {}
        }
    }
    out
}

/// Does the attribute-selector operator `op` with `valor` style the class `classe`?
fn padrao_casa(op: &str, valor: &str, classe: &str) -> bool {
    match op {
        "=" | "~=" => classe == valor,
        "|=" => classe == valor || classe.starts_with(&format!("{valor}-")),
        "^=" => classe.starts_with(valor),
        "$=" => classe.ends_with(valor),
        "*=" => classe.contains(valor),
        _ => false,
    }
}

impl Definicoes {
    /// Does the sheet style `classe` — by a class selector or by a `[class…=…]` pattern?
    pub fn define(&self, classe: &str) -> bool {
        self.classes.contains(classe)
            || self
                .padroes
                .iter()
                .any(|(op, valor)| padrao_casa(op, valor, classe))
    }
}

/// The class names of a `class_selector` node (`.a`; a chain `.a.b` is nested selectors).
fn colher_classe(no: Node, texto: &str, def: &mut Definicoes) {
    let mut filhos = no.walk();
    for filho in no.children(&mut filhos) {
        if filho.kind() == "class_name"
            && let Ok(nome) = filho.utf8_text(texto.as_bytes())
        {
            def.classes.insert(desescapa(nome));
        }
    }
}

/// `[class*="x"]` and friends.
fn colher_padrao(no: Node, texto: &str, def: &mut Definicoes) {
    if let Ok(t) = no.utf8_text(texto.as_bytes())
        && let Some(c) = PADRAO_CLASSE.captures(t)
    {
        let valor = c
            .get(2)
            .or(c.get(3))
            .or(c.get(4))
            .map_or("", |m| m.as_str());
        def.padroes.push((c[1].to_string(), valor.to_string()));
    }
}

/// Every `@import` target of the (comment-free) text, as written.
fn imports_do_texto(limpo: &str) -> Vec<String> {
    IMPORT
        .captures_iter(limpo)
        .filter_map(|c| {
            let alvo = c.get(1).or(c.get(2)).or(c.get(3))?.as_str();
            (!alvo.is_empty()).then(|| alvo.to_string())
        })
        .collect()
}

/// Parse CSS text. `Err` only when the grammar could not run.
///
/// Classes come from the grammar (`class_selector`, so a pseudo-class like `:hover` is not a
/// class). When the grammar reports a syntax error — a CSS feature it does not know, a broken
/// rule — the lexical scan (`.name` anywhere in the text) is ADDED: a sheet the grammar only half
/// understood must not make a defined class look undefined. `@import`s and generator markers are
/// read lexically from the comment-free text: they must not depend on the grammar at all.
pub(super) fn ler_css(texto: &str) -> Result<Definicoes, String> {
    let arvore = parse("css", texto)?;
    let limpo = COMENTARIO.replace_all(texto, " ");
    let mut def = Definicoes::default();
    let mut cursor = arvore.walk();
    'varredura: loop {
        let no = cursor.node();
        match no.kind() {
            "class_selector" => colher_classe(no, texto, &mut def),
            "attribute_selector" => colher_padrao(no, texto, &mut def),
            _ => {}
        }
        if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                break 'varredura;
            }
        }
    }
    if arvore.root_node().has_error() {
        def.classes.extend(
            CLASSE_LEXICA
                .captures_iter(&limpo)
                .map(|c| desescapa(&c[1])),
        );
    }
    def.imports = imports_do_texto(&limpo);
    def.gerador = GERADOR.is_match(&limpo).then_some("Tailwind");
    Ok(def)
}

/// Where sheets come from: the overlays of the entry (module files at its instant) first, then
/// the disk; remote sheets are looked up in the CDN cache, loaded on first use.
pub(super) struct Leitor<'a> {
    overlays: &'a HashMap<String, String>,
    cdn: OnceCell<Arc<Cache>>,
}

impl<'a> Leitor<'a> {
    /// A reader whose CDN cache is the default one ([`cache::padrao`]), read on first need.
    pub fn novo(overlays: &'a HashMap<String, String>) -> Self {
        Self {
            overlays,
            cdn: OnceCell::new(),
        }
    }

    /// A reader with the CDN cache injected (tests).
    pub fn com_cache(overlays: &'a HashMap<String, String>, cache: Arc<Cache>) -> Self {
        Self {
            overlays,
            cdn: OnceCell::from(cache),
        }
    }

    fn cache_cdn(&self) -> &Cache {
        self.cdn.get_or_init(|| cache::padrao().0)
    }

    pub fn existe(&self, caminho: &Path) -> bool {
        self.overlays.contains_key(&*caminho.to_string_lossy()) || caminho.is_file()
    }

    pub fn ler(&self, caminho: &Path) -> Result<String, String> {
        if let Some(texto) = self.overlays.get(&*caminho.to_string_lossy()) {
            return Ok(texto.clone());
        }
        let tamanho = std::fs::metadata(caminho)
            .map_err(|e| format!("{}: {e}", caminho.display()))?
            .len();
        if tamanho > MAX_BYTES_FOLHA {
            return Err(format!(
                "{}: {tamanho} bytes, acima do limite de {MAX_BYTES_FOLHA}",
                caminho.display()
            ));
        }
        std::fs::read_to_string(caminho).map_err(|e| format!("{}: {e}", caminho.display()))
    }

    /// The parsed sheet at `caminho`: from the overlay, else from the process memo when the file
    /// has not changed (mtime and length), else read and parsed now.
    fn definicoes(&self, caminho: &Path) -> Result<Arc<Definicoes>, String> {
        if self.overlays.contains_key(&*caminho.to_string_lossy()) {
            return ler_css(&self.ler(caminho)?).map(Arc::new);
        }
        let assinatura = std::fs::metadata(caminho)
            .ok()
            .and_then(|m| Some((m.modified().ok()?, m.len())));
        if let Some(a) = assinatura
            && let Some((guardada, def)) = MEMO.lock().get(caminho)
            && *guardada == a
        {
            return Ok(Arc::clone(def));
        }
        let def = Arc::new(ler_css(&self.ler(caminho)?)?);
        if let Some(a) = assinatura {
            let mut memo = MEMO.lock();
            if memo.len() >= MAX_MEMO {
                memo.clear();
            }
            memo.insert(caminho.to_path_buf(), (a, Arc::clone(&def)));
        }
        Ok(def)
    }
}

/// `a/./b/../c` → `a/c`, lexically (overlay keys are absolute and already normalised).
pub(super) fn normaliza_caminho(caminho: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for componente in caminho.components() {
        match componente {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            outro => out.push(outro.as_os_str()),
        }
    }
    out
}

/// How a `href` / `@import` target resolved.
pub(super) enum Resolucao {
    /// A file the [`Leitor`] can read.
    Local(PathBuf),
    /// `http(s)://` or `//host`: not fetched in layer 1 (the CDN cache may know it).
    Remota,
    /// Cannot be read, with why.
    Ilegivel(String),
}

/// Remote sheets that only declare `@font-face` — they define no class, so they do not make the
/// document unmeasurable. (`fonts.googleapis.com/icon` DOES define `.material-icons`: not here.)
fn so_declara_fontes(url: &str) -> bool {
    let sem_esquema = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .or_else(|| url.strip_prefix("//"))
        .unwrap_or(url);
    ["fonts.googleapis.com/css", "fonts.bunny.net/css"]
        .iter()
        .any(|p| {
            sem_esquema
                .strip_prefix(p)
                .is_some_and(|resto| resto.is_empty() || resto.starts_with(['/', '?', '2']))
        })
}

/// Is `alvo` an absolute URL (`http(s)://` or `//host`)?
fn e_url_absoluta(alvo: &str) -> bool {
    alvo.starts_with("http://") || alvo.starts_with("https://") || alvo.starts_with("//")
}

/// The files a local `href` may name: root-relative (`/x.css`) tries the usual web roots,
/// anything else is relative to the document's directory.
fn candidatos_locais(limpo: &str, base: &Path, raiz: &Path) -> Vec<PathBuf> {
    let candidatos = match limpo.strip_prefix('/') {
        Some(abs) => RAIZES_WEB
            .iter()
            .map(|d| normaliza_caminho(&raiz.join(d).join(abs)))
            .collect(),
        None => vec![normaliza_caminho(&base.join(limpo))],
    };
    // V-4 (31/10/2026): the CONTENT names the target — only what is inside the
    // allowed roots (the project root and the document's own directory) is a
    // candidate. A `..` that leaves them is not a file of the project.
    candidatos
        .into_iter()
        .filter(|c| crate::vgp::lang::caminho::dentro_das_raizes(&[raiz, base], c))
        .collect()
}

/// Why a target cannot even be looked up on the disk, if so.
pub(super) fn recusa_previa(limpo: &str) -> Option<String> {
    if limpo.is_empty() {
        return Some("href vazio".into());
    }
    PREPROCESSADORES
        .iter()
        .any(|e| limpo.ends_with(e))
        .then(|| format!("`{limpo}` é pré-processador: a classe só existe depois de compilar"))
}

/// Resolve `href` against the directory `base` of the document that wrote it.
pub(super) fn resolver(href: &str, base: &Path, raiz: &Path, leitor: &Leitor) -> Resolucao {
    let href = href.trim();
    if e_url_absoluta(href) {
        return Resolucao::Remota;
    }
    if href.starts_with("data:") {
        return Resolucao::Ilegivel("URI data:".into());
    }
    let limpo = href.split(['?', '#']).next().unwrap_or(href);
    if let Some(motivo) = recusa_previa(limpo) {
        return Resolucao::Ilegivel(motivo);
    }
    let candidatos = candidatos_locais(limpo, base, raiz);
    match candidatos.iter().find(|c| leitor.existe(c)) {
        Some(achado) => Resolucao::Local(achado.clone()),
        None => Resolucao::Ilegivel(format!(
            "arquivo inexistente: {}",
            candidatos
                .first()
                .map_or_else(|| limpo.to_string(), |c| c.display().to_string())
        )),
    }
}

/// The frame of one sheet being added: who wrote it, where it lives, how deep the chain is.
#[derive(Clone, Copy)]
struct Contexto<'a> {
    origem: &'a str,
    base: &'a Path,
    raiz: &'a Path,
    leitor: &'a Leitor<'a>,
    profundidade: usize,
}

/// Everything the loaded sheets say, merged — plus why some sheet could not be read.
#[derive(Default)]
pub(super) struct Folhas {
    fontes: Vec<Arc<Definicoes>>,
    /// Sheets read (inline `<style>` included).
    pub lidas: usize,
    /// A sheet that was referenced and could not be read: it may define ANY class.
    pub nao_resolvidas: Vec<String>,
    /// The first utility-class generator marker seen in a sheet.
    pub gerador: Option<&'static str>,
    /// Local sheets already followed (an `@import` cycle ends here).
    visitados: HashSet<PathBuf>,
    /// Remote sheets already followed.
    remotas: HashSet<String>,
}

impl Folhas {
    /// Does any loaded sheet style `classe`?
    pub fn define(&self, classe: &str) -> bool {
        self.fontes.iter().any(|d| d.define(classe))
    }

    /// Every defined class (for the "did you mean" search).
    pub fn classes(&self) -> impl Iterator<Item = &String> {
        self.fontes.iter().flat_map(|d| d.classes.iter())
    }

    /// Count a sheet as read without taking its classes (a CSS module: scoped, not global).
    pub fn conta_sem_absorver(&mut self) {
        self.lidas += 1;
    }

    /// Take a parsed sheet in and follow what it `@import`s.
    fn absorve(&mut self, def: Arc<Definicoes>, ctx: &Contexto, remota: bool) {
        self.lidas += 1;
        self.gerador = self.gerador.or(def.gerador);
        let imports = def.imports.clone();
        self.fontes.push(def);
        for alvo in imports {
            if remota {
                self.seguir_import_de_remota(&alvo, ctx);
            } else {
                self.seguir_import(&alvo, ctx);
            }
        }
    }

    /// Read `texto` (the content of `origem`, living in `base`) and, recursively, the sheets it
    /// `@import`s. A target that cannot be read lands in [`Folhas::nao_resolvidas`].
    pub fn adicionar_texto(
        &mut self,
        texto: &str,
        origem: &str,
        base: &Path,
        raiz: &Path,
        leitor: &Leitor,
    ) {
        let ctx = Contexto {
            origem,
            base,
            raiz,
            leitor,
            profundidade: 0,
        };
        match ler_css(texto) {
            Ok(def) => self.absorve(Arc::new(def), &ctx, false),
            Err(e) => self.nao_resolvidas.push(format!("folha {origem}: {e}")),
        }
    }

    fn seguir_import(&mut self, alvo: &str, ctx: &Contexto) {
        match resolver(alvo, ctx.base, ctx.raiz, ctx.leitor) {
            Resolucao::Remota if so_declara_fontes(alvo) => {}
            Resolucao::Remota => self.adicionar_remota(alvo, ctx),
            Resolucao::Ilegivel(motivo) => self.nao_resolvidas.push(format!(
                "@import `{alvo}` em {} não resolvida: {motivo}",
                ctx.origem
            )),
            Resolucao::Local(caminho) => {
                if !self.visitados.insert(caminho.clone()) {
                    return;
                }
                if ctx.profundidade >= MAX_PROFUNDIDADE || self.lidas >= MAX_FOLHAS {
                    self.nao_resolvidas.push(format!(
                        "@import `{alvo}` em {}: cadeia de imports longa demais",
                        ctx.origem
                    ));
                    return;
                }
                self.adicionar_local(&caminho, ctx.raiz, ctx.leitor, ctx.profundidade + 1);
            }
        }
    }

    /// A relative `@import` inside a remote sheet is relative to the sheet's URL, which layer 1
    /// does not resolve; an absolute one goes through the CDN cache like any remote sheet.
    fn seguir_import_de_remota(&mut self, alvo: &str, ctx: &Contexto) {
        if !e_url_absoluta(alvo) {
            self.nao_resolvidas.push(format!(
                "@import relativo `{alvo}` dentro da folha remota {}: a camada 1 não resolve URLs relativas",
                ctx.origem
            ));
        } else if !so_declara_fontes(alvo) {
            self.adicionar_remota(alvo, ctx);
        }
    }

    /// A remote sheet, read from the CDN cache: the classes the network resolver recorded when it
    /// downloaded the file. Anything the cache cannot answer is unresolved, naming the resolver.
    fn adicionar_remota(&mut self, url: &str, ctx: &Contexto) {
        if !self.remotas.insert(cache::normaliza(url)) {
            return;
        }
        if ctx.profundidade >= MAX_PROFUNDIDADE || self.lidas >= MAX_FOLHAS {
            self.nao_resolvidas.push(format!(
                "folha remota `{url}` ({}): cadeia de imports longa demais",
                ctx.origem
            ));
            return;
        }
        let Some(registro) = ctx.leitor.cache_cdn().buscar(url) else {
            self.nao_resolvidas.push(format!(
                "folha remota `{url}` ({}) fora do cache de CDN: a camada 1 não usa rede — `{RESOLVEDOR}` a baixa e grava as classes",
                ctx.origem
            ));
            return;
        };
        match (registro.verdict, &registro.classes) {
            (Veredito::Existe, Some(classes)) => {
                let def = Definicoes {
                    classes: classes.iter().cloned().collect(),
                    padroes: registro.padroes.clone(),
                    imports: registro.imports.clone(),
                    gerador: None,
                };
                let fundo = Contexto {
                    profundidade: ctx.profundidade + 1,
                    ..*ctx
                };
                self.absorve(Arc::new(def), &fundo, true);
            }
            (Veredito::Existe, None) => self.nao_resolvidas.push(format!(
                "folha remota `{url}` ({}): o cache não tem as classes dela (não é .css, ou foi gravada sem elas); `{RESOLVEDOR}` as grava",
                ctx.origem
            )),
            (outro, _) => self.nao_resolvidas.push(format!(
                "folha remota `{url}` ({}): o cache registra {}, não há classes para ler",
                ctx.origem,
                outro.rotulo()
            )),
        }
    }

    /// Read the file at `caminho` (memoized by mtime and length) and add it.
    fn adicionar_local(
        &mut self,
        caminho: &Path,
        raiz: &Path,
        leitor: &Leitor,
        profundidade: usize,
    ) {
        let nome = caminho.display().to_string();
        match leitor.definicoes(caminho) {
            Ok(def) => {
                let ctx = Contexto {
                    origem: &nome,
                    base: caminho.parent().unwrap_or(raiz),
                    raiz,
                    leitor,
                    profundidade,
                };
                self.absorve(def, &ctx, false);
            }
            Err(e) => self
                .nao_resolvidas
                .push(format!("folha {nome} não resolvida: {e}")),
        }
    }

    /// Read the sheet `href` (written in a document living in `base`) and add it. Remote and
    /// unreadable targets are resolved through the CDN cache or land in
    /// [`Folhas::nao_resolvidas`].
    pub fn adicionar_href(
        &mut self,
        href: &str,
        origem: &str,
        base: &Path,
        raiz: &Path,
        leitor: &Leitor,
    ) {
        let ctx = Contexto {
            origem,
            base,
            raiz,
            leitor,
            profundidade: 0,
        };
        match resolver(href, base, raiz, leitor) {
            Resolucao::Remota if so_declara_fontes(href) => {}
            Resolucao::Remota => self.adicionar_remota(href, &ctx),
            Resolucao::Ilegivel(motivo) => self
                .nao_resolvidas
                .push(format!("folha `{href}` ({origem}) não resolvida: {motivo}")),
            Resolucao::Local(caminho) => {
                if self.visitados.insert(caminho.clone()) {
                    self.adicionar_local(&caminho, raiz, leitor, 0);
                }
            }
        }
    }

    /// Read the file at `caminho` and add it (with its `@import`s).
    pub fn adicionar_arquivo(&mut self, caminho: &Path, raiz: &Path, leitor: &Leitor) {
        if self.visitados.insert(caminho.to_path_buf()) {
            self.adicionar_local(caminho, raiz, leitor, 0);
        }
    }
}
