//! Where a name the file does not declare can still come from: the runtime's globals (a closed,
//! generated table), the project's other scripts and declaration files, the `@types/*` packages and
//! the project's own TypeScript `lib`. A name is reported undeclared only when NONE of them has it.
//!
//! The search is lazy, batched and bounded: it runs only for the words that survived the lexical
//! scopes and the table (a handful per file), walks outward from the file's directory ONCE for all
//! of them, and says whether it covered the whole neighbourhood (`completo`) or ran out of budget —
//! a truncated search proves less, and the caller lowers its claim accordingly.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::projeto::{ConfigTs, Projeto};
use super::tabelas::GLOBAIS;
use super::util::palavra_presente;

/// Is `nome` one of the runtime globals (ES + DOM + Node + test runners)?
pub(super) fn eh_global_conhecido(nome: &str) -> bool {
    GLOBAIS.binary_search(&nome).is_ok() || GLOBAIS_DE_SCRIPT.contains(&nome)
}

/// What script hosts provide beyond the ES/DOM/Node tables: AMD loaders and worker scripts.
const GLOBAIS_DE_SCRIPT: &[&str] = &["define", "importScripts", "requirejs"];

/// The result of looking for a word in the project's other sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Busca {
    /// Some other source mentions the word.
    Achou,
    /// Nothing mentions it; `completo`: the whole neighbourhood was read (not cut by the budget).
    NaoAchou { completo: bool },
}

/// What a candidate needs confirmed before it becomes a finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Requisito {
    /// Nothing: the candidate stands on what the file proves.
    Nenhum,
    /// The name must not come from anywhere else: overlays, `@types`, the TS `lib`, other sources.
    GlobalAlheio,
    /// No other project source may mention the word (prototype patches, declaration merging).
    OutrosFontes,
}

const SKIP_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    "target",
    ".venv",
    "venv",
    "__pycache__",
    "coverage",
    ".next",
    ".nuxt",
    ".svelte-kit",
    ".turbo",
    ".cache",
    ".idea",
    ".vscode",
];

const EXT_LIDAS: &[&str] = &[
    "ts", "tsx", "js", "jsx", "mjs", "cjs", "mts", "cts", "html", "htm", "vue", "svelte", "astro",
];

const MAX_ARQUIVOS: usize = 2500;
const MAX_BYTES: u64 = 32 * 1024 * 1024;
const PRAZO: Duration = Duration::from_millis(120);
const MARCADORES_DE_RAIZ: &[&str] = &["package.json", "tsconfig.json", "jsconfig.json"];
/// How many directories above a marker-less file are searched (flat) for the page that loads it.
const NIVEIS_DA_PAGINA: usize = 2;

/// The cost ceiling of one neighbourhood walk.
struct Orcamento {
    arquivos: usize,
    bytes: u64,
    inicio: Instant,
    estourou: bool,
}

impl Orcamento {
    fn novo() -> Self {
        Self {
            arquivos: 0,
            bytes: 0,
            inicio: Instant::now(),
            estourou: false,
        }
    }

    /// May one more file be read? Marks the walk as truncated when not.
    fn permite(&mut self) -> bool {
        if self.arquivos >= MAX_ARQUIVOS || self.bytes >= MAX_BYTES || self.inicio.elapsed() > PRAZO
        {
            self.estourou = true;
        }
        !self.estourou
    }
}

/// The search context of one check.
pub(super) struct Mundo<'a> {
    projeto: &'a Projeto<'a>,
    arquivo: &'a Path,
    inline: bool,
    config: std::rc::Rc<ConfigTs>,
}

impl<'a> Mundo<'a> {
    pub fn novo(projeto: &'a Projeto<'a>, arquivo: &'a Path, inline: bool) -> Self {
        let dir = arquivo.parent().unwrap_or(Path::new("."));
        let config = projeto.config_para(dir);
        Self {
            projeto,
            arquivo,
            inline,
            config,
        }
    }

    pub fn config(&self) -> &ConfigTs {
        &self.config
    }

    /// Confirm many words at once. `globais` need the full «comes from elsewhere» search; `fontes`
    /// only the «mentioned by another project source» one. One neighbourhood walk serves both.
    pub fn buscar_varias(
        &self,
        globais: &[String],
        fontes: &[String],
    ) -> (HashMap<String, Busca>, HashMap<String, Busca>) {
        // the cheap sources first: a word they have needs no walk
        let ja_g: HashSet<&str> = globais
            .iter()
            .filter(|w| self.nas_sobreposicoes(w) || self.nos_tipos_globais(w))
            .map(String::as_str)
            .collect();
        let ja_f: HashSet<&str> = fontes
            .iter()
            .filter(|w| self.nas_sobreposicoes(w))
            .map(String::as_str)
            .collect();
        let pendentes: HashSet<&str> = globais
            .iter()
            .map(String::as_str)
            .filter(|w| !ja_g.contains(w))
            .chain(
                fontes
                    .iter()
                    .map(String::as_str)
                    .filter(|w| !ja_f.contains(w)),
            )
            .collect();
        let (achadas, completo) = if pendentes.is_empty() || self.inline {
            (HashSet::new(), true)
        } else {
            self.varrer_vizinhanca(&pendentes)
        };
        let montar = |palavras: &[String], ja: &HashSet<&str>| -> HashMap<String, Busca> {
            palavras
                .iter()
                .map(|w| {
                    let b = if ja.contains(w.as_str()) {
                        Busca::Achou
                    } else {
                        resultado(&achadas, w, completo)
                    };
                    (w.clone(), b)
                })
                .collect()
        };
        (montar(globais, &ja_g), montar(fontes, &ja_f))
    }

    fn nas_sobreposicoes(&self, nome: &str) -> bool {
        let eu = self.arquivo.to_string_lossy();
        self.projeto
            .sobrepostos()
            .iter()
            .any(|(k, v)| k.as_str() != eu.as_ref() && palavra_presente(v, nome))
    }

    /// The nearest ancestor (up to the project root) that marks a package; else the file's directory.
    fn raiz_de_busca(&self) -> PathBuf {
        let inicio = self
            .arquivo
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf();
        if self.arquivo.starts_with("/tmp") {
            return inicio;
        }
        for d in inicio.ancestors() {
            if MARCADORES_DE_RAIZ
                .iter()
                .any(|m| self.projeto.eh_arquivo(&d.join(m)))
            {
                return d.to_path_buf();
            }
            if d == self.projeto.raiz {
                break;
            }
        }
        inicio
    }

    /// Walk outward from the file's directory, reading each source once; the words found and
    /// whether the walk covered the whole neighbourhood.
    fn varrer_vizinhanca(&self, pendentes: &HashSet<&str>) -> (HashSet<String>, bool) {
        let inicio = self
            .arquivo
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf();
        let raiz = self.raiz_de_busca();
        let so_vizinhos = self.arquivo.starts_with("/tmp");
        let mut visto: HashSet<PathBuf> = HashSet::new();
        let mut achadas: HashSet<String> = HashSet::new();
        let mut orc = Orcamento::novo();
        for d in inicio.ancestors() {
            self.varrer_dir(
                d,
                pendentes,
                &mut achadas,
                &mut visto,
                &mut orc,
                so_vizinhos,
            );
            if orc.estourou || achadas.len() == pendentes.len() || so_vizinhos || d == raiz {
                break;
            }
        }
        // No package marker bounded the walk: the page that loads a classic script usually sits a
        // directory or two above it, so read (flat) the pages of those two levels.
        if raiz == inicio && !so_vizinhos && !orc.estourou && achadas.len() < pendentes.len() {
            for d in inicio
                .ancestors()
                .skip(1)
                .take(NIVEIS_DA_PAGINA)
                .take_while(|d| d.starts_with(self.projeto.raiz))
            {
                self.varrer_dir(d, pendentes, &mut achadas, &mut visto, &mut orc, true);
            }
        }
        (achadas, !orc.estourou)
    }

    /// Read the sources under `dir` (not revisiting `visto`), collecting the pending words they mention.
    fn varrer_dir(
        &self,
        dir: &Path,
        pendentes: &HashSet<&str>,
        achadas: &mut HashSet<String>,
        visto: &mut HashSet<PathBuf>,
        orc: &mut Orcamento,
        plano: bool,
    ) {
        let mut pilha = vec![dir.to_path_buf()];
        while let Some(d) = pilha.pop() {
            if !visto.insert(d.clone()) {
                continue;
            }
            let Ok(entradas) = std::fs::read_dir(&d) else {
                continue;
            };
            for e in entradas.flatten() {
                let Ok(tipo) = e.file_type() else { continue };
                if tipo.is_dir()
                    && !tipo.is_symlink()
                    && !plano
                    && !SKIP_DIRS.iter().any(|s| e.file_name() == **s)
                {
                    pilha.push(e.path());
                } else if tipo.is_file() && self.arquivo_candidato(&e.path()) {
                    if !orc.permite() {
                        return;
                    }
                    self.ler_e_marcar(&e.path(), pendentes, achadas, orc);
                }
            }
        }
    }

    fn arquivo_candidato(&self, p: &Path) -> bool {
        p != self.arquivo
            && p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| EXT_LIDAS.contains(&e))
    }

    fn ler_e_marcar(
        &self,
        p: &Path,
        pendentes: &HashSet<&str>,
        achadas: &mut HashSet<String>,
        orc: &mut Orcamento,
    ) {
        orc.arquivos += 1;
        let Some(texto) = self.projeto.ler(p) else {
            return;
        };
        orc.bytes += texto.len() as u64;
        let paginas = e_pagina_html(p).then(|| radicais_de_scripts(&texto));
        for w in pendentes {
            if achadas.contains(*w) {
                continue;
            }
            let chave = chave_de_biblioteca(w);
            let por_nome = palavra_presente(&texto, w);
            let por_script = paginas.as_ref().is_some_and(|rs| rs.contains(&chave));
            if por_nome || por_script {
                achadas.insert((*w).to_string());
            }
        }
    }

    // ------------------------------------------------------------------ declaration packages

    /// Is `nome` in a global-capable declaration package: the `@types/*` the config enables, or the
    /// project's TypeScript `lib`?
    fn nos_tipos_globais(&self, nome: &str) -> bool {
        let dir = self.arquivo.parent().unwrap_or(Path::new("."));
        self.nos_tipos(dir, nome) || self.nas_libs_do_typescript(dir, nome)
    }

    fn nos_tipos(&self, dir: &Path, nome: &str) -> bool {
        let Some(types) = dir
            .ancestors()
            .map(|d| d.join("node_modules").join("@types"))
            .find(|c| c.is_dir())
        else {
            return false;
        };
        let pacotes: Vec<PathBuf> = match &self.config.types {
            Some(lista) => lista
                .iter()
                .map(|t| types.join(t.split('/').next().unwrap_or(t)))
                .collect(),
            None => std::fs::read_dir(&types)
                .map(|r| r.flatten().map(|e| e.path()).collect())
                .unwrap_or_default(),
        };
        pacotes.iter().any(|p| self.pacote_menciona(p, nome))
    }

    /// The top-level `*.d.ts` of a declaration package mention `nome`?
    fn pacote_menciona(&self, pacote: &Path, nome: &str) -> bool {
        let Ok(entradas) = std::fs::read_dir(pacote) else {
            return false;
        };
        entradas
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.to_string_lossy().ends_with(".d.ts"))
            .any(|p| {
                self.projeto
                    .ler(&p)
                    .is_some_and(|t| palavra_presente(&t, nome))
            })
    }

    fn nas_libs_do_typescript(&self, dir: &Path, nome: &str) -> bool {
        let Some(ts) = self.projeto.dir_do_pacote(dir, "typescript") else {
            return false;
        };
        let Ok(entradas) = std::fs::read_dir(ts.join("lib")) else {
            return false;
        };
        entradas
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("lib.") && n.ends_with(".d.ts"))
            })
            .any(|p| {
                self.projeto
                    .ler(&p)
                    .is_some_and(|t| palavra_presente(&t, nome))
            })
    }
}

fn resultado(achadas: &HashSet<String>, palavra: &str, completo: bool) -> Busca {
    if achadas.contains(palavra) {
        Busca::Achou
    } else {
        Busca::NaoAchou { completo }
    }
}

/// Is `p` an HTML page (the place `<script src>` tags name the libraries that provide globals)?
fn e_pagina_html(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e == "html" || e == "htm")
}

/// The comparison key of a library name: lowercase letters and digits only
/// (`maplibre-gl` and `maplibregl`, `crypto-js` and `CryptoJS` are the same key).
fn chave_de_biblioteca(nome: &str) -> String {
    nome.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// The library stems of every `<script … src="…">` in an HTML page: the last path segment without
/// query, extension, `.min`/`.umd`/`.esm`-style suffixes and version, as a [`chave_de_biblioteca`].
fn radicais_de_scripts(html: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut resto = html;
    while let Some(i) = resto.find("<script") {
        resto = &resto[i + "<script".len()..];
        let fim = resto.find('>').unwrap_or(resto.len());
        let tag = &resto[..fim];
        if let Some(j) = tag.find("src=") {
            let valor = tag[j + 4..].trim_start_matches(['"', '\'']);
            let url = valor.split(['"', '\'']).next().unwrap_or("");
            if let Some(radical) = radical_do_script(url) {
                out.insert(radical);
            }
        }
        resto = &resto[fim.min(resto.len())..];
    }
    out
}

fn radical_do_script(url: &str) -> Option<String> {
    let sem_consulta = url.split(['?', '#']).next()?;
    let ultimo = sem_consulta.trim_end_matches('/').rsplit('/').next()?;
    // `maplibre-gl.js` → `maplibre-gl`; `d3.v7.min.js` → `d3`; `jquery-3.6.0.min.js` → `jquery`
    let primeiro = ultimo.split('@').next()?.split('.').next()?;
    let radical = match primeiro.rsplit_once('-') {
        Some((base, versao))
            if !versao.is_empty() && versao.chars().all(|c| c.is_ascii_digit()) =>
        {
            base
        }
        _ => primeiro,
    };
    let chave = chave_de_biblioteca(radical);
    (!chave.is_empty()).then_some(chave)
}

#[cfg(test)]
mod tests {
    use super::super::util::apoio;
    use super::*;

    fn um(mundo: &Mundo, w: &str) -> Busca {
        mundo.buscar_varias(&[w.to_string()], &[]).0[w]
    }

    fn so_fontes(mundo: &Mundo, w: &str) -> Busca {
        mundo.buscar_varias(&[], &[w.to_string()]).1[w]
    }

    #[test]
    fn tabela_ordenada_e_com_os_globais_de_sempre() {
        assert!(
            GLOBAIS.windows(2).all(|w| w[0] < w[1]),
            "binary_search exige a tabela ordenada"
        );
        for n in [
            "console",
            "document",
            "window",
            "process",
            "setTimeout",
            "Promise",
            "describe",
            "Deno",
            "Buffer",
        ] {
            assert!(eh_global_conhecido(n), "{n}");
        }
        assert!(!eh_global_conhecido("vgpAusenteXYZ"));
    }

    #[test]
    fn vizinhos_com_a_palavra_valem_como_definicao() {
        let tmp = apoio::dir_temporario();
        let raiz = tmp.path();
        std::fs::write(raiz.join("package.json"), "{}").expect("w");
        std::fs::create_dir_all(raiz.join("src")).expect("d");
        std::fs::write(raiz.join("src/a.js"), "usar(helperGlobal);\n").expect("w");
        std::fs::write(raiz.join("src/b.js"), "function helperGlobal() {}\n").expect("w");
        let sobrepor = HashMap::new();
        let projeto = Projeto::novo(raiz, &sobrepor);
        let arq = raiz.join("src/a.js");
        let mundo = Mundo::novo(&projeto, &arq, false);
        assert_eq!(um(&mundo, "helperGlobal"), Busca::Achou);
        assert_eq!(
            um(&mundo, "ninguemDefine"),
            Busca::NaoAchou { completo: true }
        );
    }

    #[test]
    fn uma_varredura_serve_varias_palavras_e_dois_requisitos() {
        let tmp = apoio::dir_temporario();
        let raiz = tmp.path();
        std::fs::write(raiz.join("package.json"), "{}").expect("w");
        std::fs::write(raiz.join("a.js"), "x;\n").expect("w");
        std::fs::write(raiz.join("b.js"), "function presente() {}\n").expect("w");
        let sobrepor = HashMap::new();
        let projeto = Projeto::novo(raiz, &sobrepor);
        let arq = raiz.join("a.js");
        let mundo = Mundo::novo(&projeto, &arq, false);
        let (g, f) = mundo.buscar_varias(
            &["presente".to_string(), "ausenteG".to_string()],
            &["presente".to_string(), "ausenteF".to_string()],
        );
        assert_eq!(g["presente"], Busca::Achou);
        assert_eq!(g["ausenteG"], Busca::NaoAchou { completo: true });
        assert_eq!(f["presente"], Busca::Achou);
        assert_eq!(f["ausenteF"], Busca::NaoAchou { completo: true });
    }

    #[test]
    fn a_busca_acha_a_palavra_inteira_e_nao_um_pedaco_de_outra() {
        let tmp = apoio::dir_temporario();
        let raiz = tmp.path();
        std::fs::write(raiz.join("package.json"), "{}").expect("w");
        std::fs::write(raiz.join("b.js"), "var meuGlobalDepois; var inteira;\n").expect("w");
        std::fs::write(raiz.join("a.js"), "x;\n").expect("w");
        let sobrepor = HashMap::new();
        let projeto = Projeto::novo(raiz, &sobrepor);
        let arq = raiz.join("a.js");
        let mundo = Mundo::novo(&projeto, &arq, false);
        // `Global` só aparece DENTRO de `meuGlobalDepois`: não é menção
        assert_eq!(um(&mundo, "Global"), Busca::NaoAchou { completo: true });
        assert_eq!(um(&mundo, "inteira"), Busca::Achou);
    }

    #[test]
    fn script_src_de_pagina_html_nomeia_a_biblioteca_do_global() {
        let tmp = apoio::dir_temporario();
        let raiz = tmp.path();
        std::fs::write(raiz.join("package.json"), "{}").expect("w");
        std::fs::write(
            raiz.join("pagina.html"),
            "<script src=\"https://unpkg.com/maplibre-gl@3.6.2/dist/maplibre-gl.js\"></script>\n\
             <script src='vendor/crypto-js.min.js?v=3'></script><script defer src=\"gsap.umd.js\"></script>",
        )
        .expect("w");
        std::fs::write(raiz.join("a.js"), "x;\n").expect("w");
        let sobrepor = HashMap::new();
        let projeto = Projeto::novo(raiz, &sobrepor);
        let arq = raiz.join("a.js");
        let mundo = Mundo::novo(&projeto, &arq, false);
        assert_eq!(um(&mundo, "maplibregl"), Busca::Achou);
        assert_eq!(um(&mundo, "CryptoJS"), Busca::Achou);
        assert_eq!(um(&mundo, "gsap"), Busca::Achou);
        // o que nenhuma tag de script nomeia continua ausente
        assert_eq!(um(&mundo, "vgpAusente"), Busca::NaoAchou { completo: true });
        // e a mesma tag em um arquivo que NÃO é página não conta como menção de biblioteca
        std::fs::write(
            raiz.join("notas.js"),
            "const t = '<script src=\"outra-lib.js\">';\n",
        )
        .expect("w");
        assert_eq!(um(&mundo, "outraLib"), Busca::NaoAchou { completo: true });
    }

    #[test]
    fn script_sem_marcador_acha_a_pagina_ate_dois_niveis_acima_e_nao_mais() {
        let tmp = apoio::dir_temporario();
        let raiz = tmp.path();
        std::fs::create_dir_all(raiz.join("p/q/r/s")).expect("d");
        std::fs::write(raiz.join("p/q/r/s/app.js"), "x;\n").expect("w");
        std::fs::write(raiz.join("p/q/r/um.html"), "<p>nivelUm()</p>\n").expect("w");
        std::fs::write(raiz.join("p/q/dois.html"), "<p>nivelDois()</p>\n").expect("w");
        std::fs::write(raiz.join("p/tres.html"), "<p>nivelTres()</p>\n").expect("w");
        let sobrepor = HashMap::new();
        let projeto = Projeto::novo(raiz, &sobrepor);
        let arq = raiz.join("p/q/r/s/app.js");
        let mundo = Mundo::novo(&projeto, &arq, false);
        assert_eq!(um(&mundo, "nivelUm"), Busca::Achou);
        assert_eq!(um(&mundo, "nivelDois"), Busca::Achou);
        assert_eq!(um(&mundo, "nivelTres"), Busca::NaoAchou { completo: true });
        // a raiz do projeto limita a subida: acima dela nada é lido
        let raiz_menor = raiz.join("p/q/r");
        let projeto = Projeto::novo(&raiz_menor, &sobrepor);
        let mundo = Mundo::novo(&projeto, &arq, false);
        assert_eq!(um(&mundo, "nivelUm"), Busca::Achou);
        assert_eq!(um(&mundo, "nivelDois"), Busca::NaoAchou { completo: true });
    }

    #[test]
    fn pagina_htm_e_consulta_com_ponto_tambem_nomeiam_a_biblioteca() {
        let tmp = apoio::dir_temporario();
        let raiz = tmp.path();
        std::fs::write(raiz.join("package.json"), "{}").expect("w");
        std::fs::write(
            raiz.join("legado.htm"),
            "<script src=\"x/maplibre-gl.js\"></script>\n\
             <script src=\"https://cdn/lib/crypto-js?v=1.2.3\"></script>",
        )
        .expect("w");
        std::fs::write(raiz.join("a.js"), "x;\n").expect("w");
        let sobrepor = HashMap::new();
        let projeto = Projeto::novo(raiz, &sobrepor);
        let arq = raiz.join("a.js");
        let mundo = Mundo::novo(&projeto, &arq, false);
        assert_eq!(um(&mundo, "maplibregl"), Busca::Achou);
        // o nome `CryptoJS` NÃO aparece no texto da página: só a chave do radical (sem `?v=1.2.3`) o acha
        assert_eq!(um(&mundo, "CryptoJS"), Busca::Achou);
    }

    #[test]
    fn radical_do_script_tira_consulta_versao_e_sufixos() {
        assert_eq!(
            radical_do_script("https://cdn/x/d3.v7.min.js"),
            Some("d3".into())
        );
        assert_eq!(
            radical_do_script("lib/vue@3.4.0/dist/vue.global.prod.js?x=1"),
            Some("vue".into())
        );
        assert_eq!(
            radical_do_script("v/jquery-3.6.0.min.js"),
            Some("jquery".into())
        );
        assert_eq!(
            radical_do_script("v/crypto-js.min.js"),
            Some("cryptojs".into())
        );
        assert_eq!(
            radical_do_script("https://cdn/pkg@1.2.3"),
            Some("pkg".into())
        );
        assert_eq!(
            radical_do_script("a/b/mermaid.esm.min.mjs#h"),
            Some("mermaid".into())
        );
        assert_eq!(radical_do_script(""), None);
    }

    #[test]
    fn node_modules_e_o_proprio_arquivo_nao_contam() {
        let tmp = apoio::dir_temporario();
        let raiz = tmp.path();
        std::fs::write(raiz.join("package.json"), "{}").expect("w");
        std::fs::create_dir_all(raiz.join("node_modules/x")).expect("d");
        std::fs::write(raiz.join("node_modules/x/i.js"), "var soAqui;\n").expect("w");
        std::fs::write(raiz.join("a.js"), "soAqui; euMesmo;\n").expect("w");
        let sobrepor = HashMap::new();
        let projeto = Projeto::novo(raiz, &sobrepor);
        let arq = raiz.join("a.js");
        let mundo = Mundo::novo(&projeto, &arq, false);
        assert_eq!(um(&mundo, "soAqui"), Busca::NaoAchou { completo: true });
        assert_eq!(um(&mundo, "euMesmo"), Busca::NaoAchou { completo: true });
    }

    #[test]
    fn globais_de_pacote_types_e_lib_do_typescript() {
        let tmp = apoio::dir_temporario();
        let raiz = tmp.path();
        std::fs::write(raiz.join("package.json"), "{}").expect("w");
        std::fs::create_dir_all(raiz.join("node_modules/@types/meu")).expect("d");
        std::fs::write(
            raiz.join("node_modules/@types/meu/index.d.ts"),
            "declare var globalDoPacote: number;\n",
        )
        .expect("w");
        std::fs::create_dir_all(raiz.join("node_modules/typescript/lib")).expect("d");
        std::fs::write(
            raiz.join("node_modules/typescript/lib/lib.dom.d.ts"),
            "declare var globalDaLib: number;\n",
        )
        .expect("w");
        std::fs::write(raiz.join("a.ts"), "x;\n").expect("w");
        let sobrepor = HashMap::new();
        let projeto = Projeto::novo(raiz, &sobrepor);
        let arq = raiz.join("a.ts");
        let mundo = Mundo::novo(&projeto, &arq, false);
        assert_eq!(um(&mundo, "globalDoPacote"), Busca::Achou);
        assert_eq!(um(&mundo, "globalDaLib"), Busca::Achou);
        // o requisito `fontes` NÃO olha @types nem a lib: um membro de Date não é "mencionado" por elas
        assert_eq!(
            so_fontes(&mundo, "globalDaLib"),
            Busca::NaoAchou { completo: true }
        );
    }

    #[test]
    fn tsconfig_types_restringe_os_pacotes_globais() {
        let tmp = apoio::dir_temporario();
        let raiz = tmp.path();
        std::fs::write(
            raiz.join("tsconfig.json"),
            r#"{ "compilerOptions": { "types": ["node"] } }"#,
        )
        .expect("w");
        std::fs::create_dir_all(raiz.join("node_modules/@types/node")).expect("d");
        std::fs::create_dir_all(raiz.join("node_modules/@types/outro")).expect("d");
        std::fs::write(
            raiz.join("node_modules/@types/node/index.d.ts"),
            "declare var doNode: number;\n",
        )
        .expect("w");
        std::fs::write(
            raiz.join("node_modules/@types/outro/index.d.ts"),
            "declare var doOutro: number;\n",
        )
        .expect("w");
        std::fs::write(raiz.join("a.ts"), "x;\n").expect("w");
        let sobrepor = HashMap::new();
        let projeto = Projeto::novo(raiz, &sobrepor);
        let arq = raiz.join("a.ts");
        let mundo = Mundo::novo(&projeto, &arq, false);
        assert_eq!(um(&mundo, "doNode"), Busca::Achou);
        assert_eq!(
            um(&mundo, "doOutro"),
            Busca::NaoAchou { completo: true },
            "`types: [\"node\"]` deixa de fora os outros @types"
        );
    }

    #[test]
    fn inline_so_olha_sobreposicoes_e_tipos() {
        let sobrepor: HashMap<String, String> =
            [("/p/outro.js".to_string(), "var visivel;".to_string())].into();
        let projeto = Projeto::novo(Path::new("/p"), &sobrepor);
        let arq = PathBuf::from("<string>");
        let mundo = Mundo::novo(&projeto, &arq, true);
        assert_eq!(um(&mundo, "visivel"), Busca::Achou);
        assert_eq!(um(&mundo, "invisivel"), Busca::NaoAchou { completo: true });
    }

    #[test]
    fn orcamento_estourado_marca_a_busca_como_incompleta() {
        let mut o = Orcamento::novo();
        o.arquivos = MAX_ARQUIVOS;
        assert!(!o.permite());
        assert!(o.estourou);
    }
}
