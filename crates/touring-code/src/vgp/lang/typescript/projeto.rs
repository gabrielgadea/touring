//! The project as the TS/TSX/JS adapters see it: the disk laid under the entry's overlays, the
//! nearest `tsconfig.json`/`jsconfig.json` (JSONC, `extends`, `references`, `paths`, `baseUrl`) and
//! module resolution — extension probing, `.js`→`.ts` substitution, directory `index`, `paths` aliases.
//!
//! Resolution follows the TypeScript handbook (Modules › Reference/Theory, consulted 10/10/2026):
//! extension substitution `.js`→`.ts`/`.tsx`/`.d.ts`, `.mjs`→`.mts`, `.cjs`→`.cts`; extensionless and
//! directory imports; `package.json` `types`/`typings`/`main` of a directory; `paths` with a single `*`,
//! several targets tried in order, longest prefix wins, relative to `baseUrl` or to the config that
//! defines them. The resolver is deliberately PERMISSIVE — it accepts every extension the supported
//! modes accept — because a module is reported missing only when NO mode could load it.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;

use serde_json::Value;

/// Files bigger than this are never read (a generated bundle is not a module summary).
pub(super) const MAX_LEITURA: u64 = 3 * 1024 * 1024;

/// What the TS/JS config files of a project say that module resolution needs.
#[derive(Debug, Default, Clone)]
pub(super) struct ConfigTs {
    /// `compilerOptions.baseUrl`, absolute.
    pub base_url: Option<PathBuf>,
    /// `compilerOptions.paths`: pattern → absolute targets (patterns kept as written).
    pub paths: Vec<(String, Vec<PathBuf>)>,
    /// `compilerOptions.jsx`.
    pub jsx: Option<String>,
    /// `compilerOptions.jsxImportSource`.
    pub jsx_import_source: Option<String>,
    /// `compilerOptions.types` (None = every `@types/*` package is global).
    pub types: Option<Vec<String>>,
}

/// The outcome of resolving one module specifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Resolvido {
    /// A file on disk (or in the overlays).
    Arquivo(PathBuf),
    /// A relative/aliased specifier that NO resolution mode can load — provably missing.
    Inexistente,
    /// A bare package (`react`, `@scope/pkg/sub`, `node:fs`, a built-in): its members are not measured.
    Pacote,
    /// Cannot be resolved statically (URL, virtual module, absolute path, inline code): the reason.
    Fora(String),
}

const EXT_ANEXAR: &[&str] = &[
    ".ts", ".tsx", ".d.ts", ".js", ".jsx", ".mjs", ".cjs", ".mts", ".cts", ".json", ".vue",
    ".svelte", ".astro", ".node",
];

const INDEX_NOMES: &[&str] = &[
    "index.ts",
    "index.tsx",
    "index.d.ts",
    "index.js",
    "index.jsx",
    "index.mjs",
    "index.cjs",
    "index.mts",
    "index.cts",
    "index.json",
];

/// `.js`-family extension → the TypeScript files the specifier may stand for.
const SUBSTITUICOES: &[(&str, &[&str])] = &[
    (".js", &[".ts", ".tsx", ".d.ts"]),
    (".jsx", &[".tsx", ".ts"]),
    (".mjs", &[".mts", ".d.mts"]),
    (".cjs", &[".cts", ".d.cts"]),
];

/// Lexical `.`/`..` normalisation — the path may not exist, so `canonicalize` is out.
pub(super) fn normalizar(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            outro => out.push(outro.as_os_str()),
        }
    }
    out
}

fn eh_relativo(spec: &str) -> bool {
    spec == "." || spec == ".." || spec.starts_with("./") || spec.starts_with("../")
}

/// The disk laid under the entry's overlays, plus the per-call config cache.
pub(super) struct Projeto<'a> {
    pub raiz: &'a Path,
    sobrepor: &'a HashMap<String, String>,
    configs: RefCell<HashMap<PathBuf, Rc<ConfigTs>>>,
}

impl<'a> Projeto<'a> {
    pub fn novo(raiz: &'a Path, sobrepor: &'a HashMap<String, String>) -> Self {
        Self {
            raiz,
            sobrepor,
            configs: RefCell::new(HashMap::new()),
        }
    }

    /// Is `p` a file — in the overlays or on disk?
    pub fn eh_arquivo(&self, p: &Path) -> bool {
        self.sobrepor.contains_key(p.to_string_lossy().as_ref()) || p.is_file()
    }

    /// Is `p` a directory — on disk, or the parent of an overlay file?
    pub fn eh_diretorio(&self, p: &Path) -> bool {
        if p.is_dir() {
            return true;
        }
        let prefixo = format!("{}/", p.to_string_lossy());
        self.sobrepor.keys().any(|k| k.starts_with(&prefixo))
    }

    /// Content of `p` (overlay first, then disk, capped); `None` if unreadable or too big.
    pub fn ler(&self, p: &Path) -> Option<String> {
        if let Some(c) = self.sobrepor.get(p.to_string_lossy().as_ref()) {
            return Some(c.clone());
        }
        let meta = std::fs::metadata(p).ok()?;
        if !meta.is_file() || meta.len() > MAX_LEITURA {
            return None;
        }
        std::fs::read_to_string(p).ok()
    }

    /// The overlay map (read-only), for callers that scan sibling overlays.
    pub fn sobrepostos(&self) -> &HashMap<String, String> {
        self.sobrepor
    }

    // ------------------------------------------------------------------ config

    /// The config governing files in `dir`: the nearest directory (up to the project root) that has a
    /// `tsconfig.json` or `jsconfig.json`.
    pub fn config_para(&self, dir: &Path) -> Rc<ConfigTs> {
        let mut atual = Some(dir.to_path_buf());
        for _ in 0..12 {
            let Some(d) = atual else { break };
            if let Some(c) = self.configs.borrow().get(&d) {
                return c.clone();
            }
            if ["tsconfig.json", "jsconfig.json"]
                .iter()
                .any(|n| self.eh_arquivo(&d.join(n)))
            {
                let cfg = Rc::new(self.carregar_dir(&d));
                self.configs.borrow_mut().insert(d, cfg.clone());
                return cfg;
            }
            atual = if d == self.raiz {
                None
            } else {
                d.parent().map(Path::to_path_buf)
            };
        }
        Rc::new(ConfigTs::default())
    }

    fn carregar_dir(&self, dir: &Path) -> ConfigTs {
        let mut cfg = ConfigTs::default();
        for nome in [
            "tsconfig.json",
            "jsconfig.json",
            "tsconfig.app.json",
            "tsconfig.base.json",
            "tsconfig.web.json",
        ] {
            let p = dir.join(nome);
            if self.eh_arquivo(&p) {
                self.mesclar(&p, &mut cfg, 0);
            }
        }
        cfg
    }

    /// Merge one config file (and its `extends` chain / `references`) into `cfg`.
    fn mesclar(&self, arquivo: &Path, cfg: &mut ConfigTs, prof: u8) {
        if prof > 4 {
            return;
        }
        let Some(texto) = self.ler(arquivo) else {
            return;
        };
        let Ok(json) = serde_json::from_str::<Value>(&jsonc_para_json(&texto)) else {
            return;
        };
        let dir = arquivo.parent().unwrap_or(Path::new("."));
        // base first, so the child overrides
        if let Some(ext) = json.get("extends").and_then(Value::as_str)
            && eh_relativo(ext)
        {
            let mut base = normalizar(&dir.join(ext));
            if base.extension().is_none_or(|e| e != "json") {
                base.set_extension("json");
            }
            self.mesclar(&base, cfg, prof + 1);
        }
        if let Some(opts) = json.get("compilerOptions") {
            mesclar_opcoes(opts, dir, cfg);
        }
        let refs = json.get("references").and_then(Value::as_array);
        for p in refs
            .into_iter()
            .flatten()
            .filter_map(|r| r.get("path")?.as_str())
        {
            let mut alvo = normalizar(&dir.join(p));
            if self.eh_diretorio(&alvo) {
                alvo = alvo.join("tsconfig.json");
            }
            self.mesclar(&alvo, cfg, prof + 1);
        }
    }

    // ------------------------------------------------------------------ resolution

    /// Resolve `spec` as imported from the file `de`.
    pub fn resolver(&self, de: &Path, spec: &str) -> Resolvido {
        let limpo = spec.split(['?', '#']).next().unwrap_or(spec);
        let dir_de = de.parent().unwrap_or(Path::new("."));
        if let Some(fora) = especificador_fora_do_alcance(limpo) {
            return Resolvido::Fora(fora);
        }
        if eh_relativo(limpo) {
            let alvo = normalizar(&dir_de.join(limpo));
            // V-4 (31/10/2026): the allowed roots are the project root AND the
            // directory of the file being checked (a fragment in `/tmp` resolves
            // its imports next to itself). A `../..` that leaves both is not a file
            // of the project — and the refusal names the reason, never the path.
            if !crate::vgp::lang::caminho::dentro_das_raizes(&[self.raiz, dir_de], &alvo) {
                return Resolvido::Fora("caminho fora das raízes permitidas".into());
            }
            return match self.provar_caminho(&alvo) {
                Some(f) => Resolvido::Arquivo(f),
                None => Resolvido::Inexistente,
            };
        }
        self.resolver_nao_relativo(limpo, dir_de)
    }

    /// Bare specifier: `paths` alias, `baseUrl`, or a package.
    fn resolver_nao_relativo(&self, spec: &str, dir_de: &Path) -> Resolvido {
        let cfg = self.config_para(dir_de);
        if let Some(r) = self.por_paths(&cfg, spec, dir_de) {
            return r;
        }
        if let Some(base) = &cfg.base_url
            && let Some(f) = self.provar_caminho(&normalizar(&base.join(spec)))
        {
            return Resolvido::Arquivo(f);
        }
        Resolvido::Pacote
    }

    /// `paths` aliases: the longest matching pattern wins; its targets are tried in order. A pure `*`
    /// pattern never proves anything missing (TS falls through to `node_modules`).
    fn por_paths(&self, cfg: &ConfigTs, spec: &str, dir_de: &Path) -> Option<Resolvido> {
        let (peso, alvos, capt) = melhor_padrao(&cfg.paths, spec)?;
        for alvo in alvos {
            let candidato = PathBuf::from(alvo.to_string_lossy().replace('*', &capt));
            if let Some(f) = self.provar_caminho(&candidato) {
                return Some(Resolvido::Arquivo(f));
            }
        }
        if peso == 0 || self.pacote_instalado(dir_de, &nome_do_pacote(spec)) {
            return None; // catch-all, or an installed package with the same prefix wins
        }
        Some(Resolvido::Inexistente)
    }

    /// Does some `node_modules` at or above `dir` hold the package `nome`?
    pub fn pacote_instalado(&self, dir: &Path, nome: &str) -> bool {
        self.dir_do_pacote(dir, nome).is_some()
    }

    /// The directory of the installed package `nome` visible from `dir`.
    pub fn dir_do_pacote(&self, dir: &Path, nome: &str) -> Option<PathBuf> {
        dir.ancestors()
            .map(|d| d.join("node_modules").join(nome))
            .find(|c| c.is_dir())
    }

    /// The first existing file `base` can mean: itself, the TS file a `.js` specifier stands for,
    /// `base` + an extension, or the entry of the directory `base`.
    pub fn provar_caminho(&self, base: &Path) -> Option<PathBuf> {
        if self.eh_arquivo(base) {
            return Some(base.to_path_buf());
        }
        self.substituir_extensao(base)
            .or_else(|| self.anexar_extensao(base))
            .or_else(|| self.entrada_do_diretorio(base))
    }

    /// `./a.js` → `./a.ts` | `./a.tsx` | `./a.d.ts` (and the `.mjs`/`.cjs` pairs).
    fn substituir_extensao(&self, base: &Path) -> Option<PathBuf> {
        let nome = base.file_name()?.to_str()?;
        let pai = base.parent().unwrap_or(Path::new(""));
        SUBSTITUICOES
            .iter()
            .filter_map(|(de, para)| nome.strip_suffix(de).map(|raiz| (raiz, para)))
            .flat_map(|(raiz, para)| para.iter().map(move |ext| pai.join(format!("{raiz}{ext}"))))
            .find(|c| self.eh_arquivo(c))
    }

    /// `./a` → `./a.ts` | `./a.tsx` | …
    fn anexar_extensao(&self, base: &Path) -> Option<PathBuf> {
        let nome = base.file_name()?.to_str()?;
        let pai = base.parent().unwrap_or(Path::new(""));
        EXT_ANEXAR
            .iter()
            .map(|ext| pai.join(format!("{nome}{ext}")))
            .find(|c| self.eh_arquivo(c))
    }

    /// `./dir` → the `package.json` entry (`types`/`typings`/`module`/`main`) or `dir/index.*`.
    fn entrada_do_diretorio(&self, base: &Path) -> Option<PathBuf> {
        if !self.eh_diretorio(base) {
            return None;
        }
        let do_pacote = self
            .ler(&base.join("package.json"))
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .and_then(|j| {
                ["types", "typings", "module", "main"]
                    .iter()
                    .filter_map(|c| j.get(*c)?.as_str().map(|e| normalizar(&base.join(e))))
                    .filter(|alvo| alvo != base)
                    .find_map(|alvo| self.arquivo_ou_com_extensao(&alvo))
            });
        do_pacote.or_else(|| {
            INDEX_NOMES
                .iter()
                .map(|i| base.join(i))
                .find(|c| self.eh_arquivo(c))
        })
    }

    /// `base` as a file, or `base` + an extension — never a directory entry (no recursion).
    fn arquivo_ou_com_extensao(&self, base: &Path) -> Option<PathBuf> {
        if self.eh_arquivo(base) {
            return Some(base.to_path_buf());
        }
        self.anexar_extensao(base)
    }
}

/// Specifiers no static resolver can honour: the reason, or `None` when it is resolvable.
fn especificador_fora_do_alcance(spec: &str) -> Option<String> {
    if spec.is_empty() {
        Some("especificador vazio".into())
    } else if spec.starts_with('/') {
        Some("caminho absoluto (raiz do servidor ou do disco)".into())
    } else if spec.contains("://") || spec.starts_with("data:") || spec.starts_with("virtual:") {
        Some("URL ou módulo virtual".into())
    } else {
        None
    }
}

/// The longest matching `paths` pattern: (weight = chars before the `*`, targets, captured text).
fn melhor_padrao<'c>(
    paths: &'c [(String, Vec<PathBuf>)],
    spec: &str,
) -> Option<(usize, &'c [PathBuf], String)> {
    paths
        .iter()
        .filter_map(|(padrao, alvos)| {
            let capt = casar_padrao(padrao, spec)?;
            Some((
                padrao.find('*').unwrap_or(padrao.len()),
                alvos.as_slice(),
                capt,
            ))
        })
        .max_by_key(|(peso, _, _)| *peso)
}

/// Fold the `compilerOptions` keys module resolution and JSX typing need into `cfg`.
fn mesclar_opcoes(opts: &Value, dir: &Path, cfg: &mut ConfigTs) {
    let texto = |k: &str| opts.get(k).and_then(Value::as_str).map(str::to_string);
    if let Some(b) = texto("baseUrl") {
        cfg.base_url = Some(normalizar(&dir.join(b)));
    }
    cfg.jsx = texto("jsx").or(cfg.jsx.take());
    cfg.jsx_import_source = texto("jsxImportSource").or(cfg.jsx_import_source.take());
    if let Some(t) = opts.get("types").and_then(Value::as_array) {
        cfg.types = Some(
            t.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect(),
        );
    }
    if let Some(p) = opts.get("paths").and_then(Value::as_object) {
        // targets are relative to baseUrl when set, else to the config that defines them
        let ancora = cfg.base_url.clone().unwrap_or_else(|| dir.to_path_buf());
        for (padrao, alvos) in p {
            let alvos: Vec<PathBuf> = alvos
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(|t| normalizar(&ancora.join(t)))
                .collect();
            cfg.paths.retain(|(q, _)| q != padrao);
            cfg.paths.push((padrao.clone(), alvos));
        }
    }
}

/// `@scope/name/sub` → `@scope/name`; `lodash/fp` → `lodash`.
pub(super) fn nome_do_pacote(spec: &str) -> String {
    let mut partes = spec.split('/');
    let primeiro = partes.next().unwrap_or("");
    match (primeiro.starts_with('@'), partes.next()) {
        (true, Some(segundo)) => format!("{primeiro}/{segundo}"),
        _ => primeiro.to_string(),
    }
}

/// Match a `paths` pattern (at most one `*`) against `spec`: the captured text on success.
pub(super) fn casar_padrao(padrao: &str, spec: &str) -> Option<String> {
    let Some(i) = padrao.find('*') else {
        return (padrao == spec).then(String::new);
    };
    let (pre, pos) = (&padrao[..i], &padrao[i + 1..]);
    let cabe = spec.len() >= pre.len() + pos.len() && spec.starts_with(pre) && spec.ends_with(pos);
    cabe.then(|| spec[pre.len()..spec.len() - pos.len()].to_string())
}

/// JSONC → JSON: drops `//` and `/* */` comments and trailing commas, leaving strings untouched.
pub(super) fn jsonc_para_json(texto: &str) -> String {
    let b: Vec<char> = texto.chars().collect();
    let mut out = String::with_capacity(texto.len());
    let mut i = 0;
    while i < b.len() {
        i = match (b[i], b.get(i + 1)) {
            ('"', _) => copiar_string(&b, i, &mut out),
            ('/', Some('/')) => pular_linha(&b, i),
            ('/', Some('*')) => pular_bloco(&b, i),
            (',', _) if virgula_final(&b, i) => i + 1,
            (c, _) => {
                out.push(c);
                i + 1
            }
        };
    }
    out
}

/// Copy the JSON string starting at `b[i]` (a quote) to `out`; the index after its closing quote.
fn copiar_string(b: &[char], mut i: usize, out: &mut String) -> usize {
    out.push(b[i]);
    i += 1;
    while i < b.len() {
        out.push(b[i]);
        if b[i] == '\\' && i + 1 < b.len() {
            out.push(b[i + 1]);
            i += 2;
        } else if b[i] == '"' {
            return i + 1;
        } else {
            i += 1;
        }
    }
    i
}

fn pular_linha(b: &[char], mut i: usize) -> usize {
    while i < b.len() && b[i] != '\n' {
        i += 1;
    }
    i
}

fn pular_bloco(b: &[char], mut i: usize) -> usize {
    i += 2;
    while i + 1 < b.len() && !(b[i] == '*' && b[i + 1] == '/') {
        i += 1;
    }
    (i + 2).min(b.len())
}

/// Is the comma at `b[i]` the last thing before a closing `}` or `]`?
fn virgula_final(b: &[char], i: usize) -> bool {
    b[i + 1..]
        .iter()
        .find(|c| !c.is_whitespace())
        .is_some_and(|c| *c == '}' || *c == ']')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jsonc_tira_comentarios_e_virgula_final_sem_tocar_strings() {
        let j = jsonc_para_json(
            "{ // linha\n \"a\": \"http://x\", /* bloco */ \"b\": [1, 2,], \"c\": \"// nao\",\n}",
        );
        let v: Value = serde_json::from_str(&j).expect("json válido");
        assert_eq!(v["a"], "http://x");
        assert_eq!(v["b"], serde_json::json!([1, 2]));
        assert_eq!(v["c"], "// nao");
    }

    #[test]
    fn casar_padrao_pega_o_trecho_do_asterisco() {
        assert_eq!(casar_padrao("@/*", "@/a/b").as_deref(), Some("a/b"));
        assert_eq!(casar_padrao("@/*", "@x/a"), None);
        assert_eq!(casar_padrao("exato", "exato").as_deref(), Some(""));
        assert_eq!(casar_padrao("exato", "outro"), None);
        assert_eq!(casar_padrao("*", "qualquer").as_deref(), Some("qualquer"));
        assert_eq!(casar_padrao("a/*/z", "a/m/z").as_deref(), Some("m"));
        assert_eq!(casar_padrao("a/*/z", "a/m/y"), None);
    }

    #[test]
    fn nome_do_pacote_trata_escopo() {
        assert_eq!(nome_do_pacote("@scope/pkg/sub/x"), "@scope/pkg");
        assert_eq!(nome_do_pacote("lodash/fp"), "lodash");
        assert_eq!(nome_do_pacote("react"), "react");
        assert_eq!(nome_do_pacote("@solo"), "@solo");
    }

    #[test]
    fn normalizar_resolve_pontos_sem_tocar_o_disco() {
        assert_eq!(
            normalizar(Path::new("/a/b/../c/./d")),
            PathBuf::from("/a/c/d")
        );
        assert_eq!(normalizar(Path::new("/a/b/../../x")), PathBuf::from("/x"));
    }

    #[test]
    fn padrao_mais_longo_vence_e_o_asterisco_puro_pesa_zero() {
        let paths = vec![
            ("*".to_string(), vec![PathBuf::from("/r/*")]),
            ("@/*".to_string(), vec![PathBuf::from("/r/src/*")]),
            ("@/x".to_string(), vec![PathBuf::from("/r/x.ts")]),
        ];
        let (peso, alvos, capt) = melhor_padrao(&paths, "@/x").expect("casa");
        assert_eq!((peso, capt.as_str()), (3, ""), "o exato (@/x) vence o @/*");
        assert_eq!(alvos, &[PathBuf::from("/r/x.ts")]);
        let (peso, _, capt) = melhor_padrao(&paths, "@/a/b").expect("casa");
        assert_eq!((peso, capt.as_str()), (2, "a/b"));
        let (peso, _, _) = melhor_padrao(&paths, "lodash").expect("casa");
        assert_eq!(peso, 0, "só o catch-all casa");
    }
}
