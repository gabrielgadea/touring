//! What the project around a file says about its classes: whether a utility-class generator or a
//! CSS-in-JS library is in play (then the classes are not all written in sheets: "unmeasured",
//! never a guess), which stylesheets the project owns, and what its entry `index.html` loads.

use std::path::{Path, PathBuf};

/// Config files whose presence means a utility-class generator builds the classes.
const CONFIGS_DE_GERADOR: [(&str, &str); 12] = [
    ("tailwind.config.js", "Tailwind"),
    ("tailwind.config.cjs", "Tailwind"),
    ("tailwind.config.mjs", "Tailwind"),
    ("tailwind.config.ts", "Tailwind"),
    ("uno.config.ts", "UnoCSS"),
    ("uno.config.js", "UnoCSS"),
    ("uno.config.mjs", "UnoCSS"),
    ("unocss.config.ts", "UnoCSS"),
    ("unocss.config.js", "UnoCSS"),
    ("windi.config.ts", "Windi CSS"),
    ("windi.config.js", "Windi CSS"),
    ("panda.config.ts", "Panda CSS"),
];

/// Dependencies (exact names) that generate classes or define them from JavaScript.
const DEPENDENCIAS_EXATAS: [&str; 13] = [
    "tailwindcss",
    "unocss",
    "windicss",
    "twind",
    "daisyui",
    "styled-components",
    "styled-jsx",
    "goober",
    "jss",
    "react-jss",
    "linaria",
    "aphrodite",
    "glamor",
];

/// Dependency scopes that do the same (`@emotion/react`, `@unocss/preset-uno`, …).
const ESCOPOS_DE_GERADOR: [&str; 8] = [
    "@tailwindcss/",
    "@unocss/",
    "@twind/",
    "@emotion/",
    "@stitches/",
    "@vanilla-extract/",
    "@linaria/",
    "@pandacss/",
];

/// Directories never walked: dependencies, build output, caches, VCS.
const DIRETORIOS_IGNORADOS: [&str; 11] = [
    "node_modules",
    "dist",
    "build",
    "out",
    "target",
    "coverage",
    "vendor",
    "venv",
    "env",
    "__pycache__",
    "storybook-static",
];

/// How far the walk over the project's sheets may go.
#[derive(Debug, Clone, Copy)]
pub(super) struct Limites {
    /// Directory entries visited.
    pub entradas: usize,
    /// Stylesheets collected.
    pub folhas: usize,
}

impl Default for Limites {
    fn default() -> Self {
        Self {
            entradas: 20_000,
            folhas: 200,
        }
    }
}

/// Levels walked up from a file looking for its package.
const MAX_NIVEIS_ACIMA: usize = 16;

/// Is `nome` a dependency that generates classes or defines them from JavaScript?
fn e_gerador(nome: &str) -> bool {
    DEPENDENCIAS_EXATAS.contains(&nome) || ESCOPOS_DE_GERADOR.iter().any(|e| nome.starts_with(e))
}

/// The generator named in a `package.json`'s dependency tables, if any.
fn gerador_do_package_json(texto: &str) -> Option<String> {
    let json: serde_json::Value = serde_json::from_str(texto).ok()?;
    ["dependencies", "devDependencies", "peerDependencies"]
        .iter()
        .filter_map(|tabela| json.get(tabela)?.as_object())
        .flat_map(|deps| deps.keys())
        .find(|nome| e_gerador(nome))
        .cloned()
}

/// A generator marker in the directory `dir`: a config file, or a dependency of its
/// `package.json`. The second element says whether `dir` is a package root.
fn gerador_na_pasta(dir: &Path) -> (Option<String>, bool) {
    for (arquivo, nome) in CONFIGS_DE_GERADOR {
        if dir.join(arquivo).is_file() {
            return (
                Some(format!("{nome} ({})", dir.join(arquivo).display())),
                true,
            );
        }
    }
    let manifesto = dir.join("package.json");
    let Ok(texto) = std::fs::read_to_string(&manifesto) else {
        return (None, false);
    };
    let achado =
        gerador_do_package_json(&texto).map(|dep| format!("{dep} ({})", manifesto.display()));
    (achado, true)
}

/// The utility-class generator / CSS-in-JS library that governs files in `dir`: looks in `dir`
/// and its ancestors up to the project root, and stops at the first directory that is a package
/// (has a `package.json`) — a report living outside `frontend/` is not governed by
/// `frontend/package.json`.
pub(super) fn gerador_do_pacote(dir: &Path, raiz: &Path) -> Option<String> {
    let dentro = dir.starts_with(raiz);
    let mut atual = dir.to_path_buf();
    for _ in 0..MAX_NIVEIS_ACIMA {
        let (achado, e_pacote) = gerador_na_pasta(&atual);
        if achado.is_some() || e_pacote || !dentro || atual == raiz {
            return achado;
        }
        if !atual.pop() {
            return None;
        }
    }
    None
}

/// The stylesheets the project owns (CSS modules, minified copies excluded: modules are scoped,
/// not global). `Err` when the project is too big to walk within `limites` — a partial list
/// would make "no sheet defines it" a lie.
pub(super) fn folhas_do_projeto(raiz: &Path, limites: Limites) -> Result<Vec<PathBuf>, String> {
    let mut folhas = Vec::new();
    let mut pilha = vec![raiz.to_path_buf()];
    let mut visitadas = 0usize;
    while let Some(dir) = pilha.pop() {
        let Ok(entradas) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entrada in entradas.filter_map(Result::ok) {
            visitadas += 1;
            if visitadas > limites.entradas {
                return Err(format!(
                    "projeto grande demais para varrer as folhas (mais de {} entradas)",
                    limites.entradas
                ));
            }
            let nome = entrada.file_name().to_string_lossy().into_owned();
            let caminho = entrada.path();
            let Ok(tipo) = entrada.file_type() else {
                continue;
            };
            if tipo.is_dir() {
                if !nome.starts_with('.') && !DIRETORIOS_IGNORADOS.contains(&nome.as_str()) {
                    pilha.push(caminho);
                }
            } else if tipo.is_file() && nome.ends_with(".css") && !nome.ends_with(".module.css") {
                folhas.push(caminho);
                if folhas.len() > limites.folhas {
                    return Err(format!(
                        "projeto com folhas demais para ler (mais de {})",
                        limites.folhas
                    ));
                }
            }
        }
    }
    folhas.sort();
    Ok(folhas)
}

/// The entry documents of a web project, if the root has one.
pub(super) fn html_de_entrada(raiz: &Path) -> Vec<PathBuf> {
    ["index.html", "public/index.html", "src/index.html"]
        .iter()
        .map(|rel| raiz.join(rel))
        .filter(|p| p.is_file())
        .collect()
}

/// Resolve a package-style CSS import (`lib-ui/dist/ui.css`, `@/styles/x.css`) from the
/// directory `dir`: `node_modules` of each ancestor up to the root, and for the `@/` alias the
/// conventional `src/` (then the root).
pub(super) fn resolver_pacote(alvo: &str, dir: &Path, raiz: &Path) -> Option<PathBuf> {
    if let Some(resto) = alvo.strip_prefix("@/") {
        return [raiz.join("src").join(resto), raiz.join(resto)]
            .into_iter()
            // V-4 (31/10/2026): the alias is CONTENT — a `..` that leaves the
            // project root is not a file of the project.
            .filter(|c| crate::vgp::lang::caminho::dentro_das_raizes(&[raiz], c))
            .find(|c| c.is_file());
    }
    let sem_til = alvo.strip_prefix('~').unwrap_or(alvo);
    let mut atual = dir.to_path_buf();
    for _ in 0..MAX_NIVEIS_ACIMA {
        let candidato = atual.join("node_modules").join(sem_til);
        // V-4 (31/10/2026): the package name is CONTENT — it may not leave the roots.
        if crate::vgp::lang::caminho::dentro_das_raizes(&[raiz], &candidato) && candidato.is_file()
        {
            return Some(candidato);
        }
        if atual == raiz || !atual.pop() {
            break;
        }
    }
    let da_raiz = raiz.join("node_modules").join(sem_til);
    (crate::vgp::lang::caminho::dentro_das_raizes(&[raiz], &da_raiz) && da_raiz.is_file())
        .then_some(da_raiz)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn escreve(raiz: &Path, rel: &str, conteudo: &str) {
        let alvo = raiz.join(rel);
        std::fs::create_dir_all(alvo.parent().unwrap()).unwrap();
        std::fs::write(alvo, conteudo).unwrap();
    }

    #[test]
    fn varredura_estourada_e_erro_e_nunca_uma_lista_parcial() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..6 {
            escreve(dir.path(), &format!("pasta/{i}.txt"), "x");
        }
        let curto = Limites {
            entradas: 3,
            folhas: 10,
        };
        assert!(
            folhas_do_projeto(dir.path(), curto)
                .unwrap_err()
                .contains("grande demais")
        );
        for i in 0..4 {
            escreve(dir.path(), &format!("css/{i}.css"), ".a{}");
        }
        let poucas = Limites {
            entradas: 1000,
            folhas: 2,
        };
        assert!(
            folhas_do_projeto(dir.path(), poucas)
                .unwrap_err()
                .contains("folhas demais")
        );
    }

    #[test]
    fn varre_so_css_global_e_ignora_modulos_ocultas_e_build() {
        let dir = tempfile::tempdir().unwrap();
        for rel in [
            "src/app.css",
            "src/fundo/b.css",
            "src/Card.module.css",
            "node_modules/x/n.css",
            ".git/g.css",
            "dist/d.css",
            "src/leia.txt",
        ] {
            escreve(dir.path(), rel, ".a{}");
        }
        let achadas = folhas_do_projeto(dir.path(), Limites::default()).unwrap();
        let nomes: Vec<String> = achadas
            .iter()
            .map(|p| p.strip_prefix(dir.path()).unwrap().display().to_string())
            .collect();
        assert_eq!(nomes, vec!["src/app.css", "src/fundo/b.css"]);
    }

    #[test]
    fn dependencia_de_gerador_por_nome_exato_ou_escopo() {
        for dep in [
            "tailwindcss",
            "unocss",
            "styled-components",
            "@emotion/react",
            "@unocss/preset-uno",
            "@vanilla-extract/css",
            "@tailwindcss/vite",
        ] {
            assert!(e_gerador(dep), "{dep}");
        }
        // only known names and scopes count: a package that merely contains the word does not
        for dep in [
            "react",
            "bootstrap",
            "@types/node",
            "tailwindcss-animate",
            "my-unocss-fork",
        ] {
            assert!(!e_gerador(dep), "{dep}");
        }
    }

    #[test]
    fn o_gerador_para_no_primeiro_pacote_e_dependencia_so_vale_no_json_valido() {
        let dir = tempfile::tempdir().unwrap();
        escreve(
            dir.path(),
            "app/package.json",
            r#"{"dependencies": {"react": "18"}}"#,
        );
        escreve(dir.path(), "tailwind.config.js", "x");
        // app/ is its own package without a generator: the root config does not govern it
        let app = dir.path().join("app/src");
        assert_eq!(gerador_do_pacote(&app, dir.path()), None);
        // package.json that is not JSON never invents a generator
        escreve(dir.path(), "outro/package.json", "{ tailwindcss");
        assert_eq!(
            gerador_do_pacote(&dir.path().join("outro"), dir.path()),
            None
        );
        // outside the project root only the file's own directory is looked at
        assert_eq!(
            gerador_do_pacote(Path::new("/tmp/xyz-inexistente"), dir.path()),
            None
        );
    }
}
