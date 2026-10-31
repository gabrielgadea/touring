//! Small pure helpers shared by the TS/TSX/JS adapter modules: node text, whole-word search,
//! edit distance for the «você quis dizer» hint.

use tree_sitter::Node;

/// The source text of `no`.
pub(super) fn texto<'s>(src: &'s str, no: Node) -> &'s str {
    src.get(no.start_byte()..no.end_byte()).unwrap_or("")
}

/// 1-based line of the start of `no`.
pub(super) fn linha(no: Node) -> usize {
    no.start_position().row + 1
}

/// Does `no` have a child (named or anonymous token) of exactly this kind? Tokens such as `default`,
/// `const`, `type` or `=` are anonymous nodes whose kind is their own text.
pub(super) fn tem_token(no: Node, tipo: &str) -> bool {
    let mut c = no.walk();
    no.children(&mut c).any(|f| f.kind() == tipo)
}

/// The value of a string literal node (`'./a'`, `"./a"`, a template with no substitution).
/// `None` for anything dynamic — a template with `${}`, a string with escapes, a non-string node.
pub(super) fn conteudo_string(src: &str, no: Node) -> Option<String> {
    match no.kind() {
        "string" => {
            let t = texto(src, no);
            if t.len() < 2 || t.contains('\\') {
                return None;
            }
            Some(t[1..t.len() - 1].to_string())
        }
        "template_string" => {
            let t = texto(src, no);
            if t.len() < 2 || t.contains("${") || t.contains('\\') {
                return None;
            }
            Some(t[1..t.len() - 1].to_string())
        }
        _ => None,
    }
}

fn eh_id(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b >= 0x80
}

/// Does `palavra` occur in `texto` as a whole identifier (neither side touches an identifier char)?
pub(super) fn palavra_presente(texto: &str, palavra: &str) -> bool {
    if palavra.is_empty() {
        return false;
    }
    let bytes = texto.as_bytes();
    let mut de = 0;
    while let Some(i) = texto[de..].find(palavra) {
        let ini = de + i;
        let fim = ini + palavra.len();
        let antes = ini == 0 || !eh_id(bytes[ini - 1]);
        let depois = fim >= bytes.len() || !eh_id(bytes[fim]);
        if antes && depois {
            return true;
        }
        de = ini + palavra.len().max(1);
        if de >= texto.len() {
            break;
        }
    }
    false
}

/// Levenshtein distance (chars).
pub(super) fn distancia(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut linha: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut anterior = linha[0];
        linha[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let tmp = linha[j + 1];
            linha[j + 1] = if ca == cb {
                anterior
            } else {
                1 + anterior.min(linha[j]).min(linha[j + 1])
            };
            anterior = tmp;
        }
    }
    linha[b.len()]
}

/// The real name closest to `nome` (edit distance 1..=2), if any — the refusal teaches the way.
pub(super) fn mais_proximo<'a>(
    nome: &str,
    reais: impl IntoIterator<Item = &'a String>,
) -> Option<String> {
    reais
        .into_iter()
        .map(|r| (distancia(nome, r), r))
        .filter(|(d, _)| *d > 0 && *d <= 2)
        .min_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(b.1)))
        .map(|(_, r)| r.clone())
}

/// A bounded, sorted copy of `nomes` for `Finding::reais`.
pub(super) fn reais_de<'a>(nomes: impl IntoIterator<Item = &'a String>) -> Vec<String> {
    let mut v: Vec<String> = nomes.into_iter().cloned().collect();
    v.sort();
    v.dedup();
    v.truncate(40);
    v
}

/// The last path segment of a module specifier, query/hash stripped: `./a/b?raw` → `b`.
pub(super) fn ultimo_segmento(spec: &str) -> String {
    let limpo = spec.split(['?', '#']).next().unwrap_or(spec);
    limpo
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .unwrap_or(limpo)
        .to_string()
}

/// Test support shared by the unit and the end-to-end tests.
#[cfg(test)]
pub(super) mod apoio {
    use std::path::PathBuf;

    /// Where the temp projects live. The severity policy treats `/tmp/` as «the code runs right now»
    /// (it denies there, and the neighbourhood walk is flat there), so a project that is meant to
    /// get the ordinary behaviour lives under the cargo target dir
    /// (`<target>/<profile>/deps/<test binary>` → `<target>/tmp`) instead.
    pub fn base_dos_projetos() -> PathBuf {
        let exe = std::env::current_exe().expect("caminho do binário de teste");
        let alvo = exe.ancestors().nth(3).expect("diretório target");
        let base = alvo.join("tmp").join("vgp-vts");
        assert!(
            !base.starts_with("/tmp"),
            "o target dir está sob /tmp ({}): a política nega ali — defina CARGO_TARGET_DIR fora de /tmp",
            base.display()
        );
        std::fs::create_dir_all(&base).expect("mkdir base");
        base
    }

    /// An empty temp project directory outside `/tmp`.
    pub fn dir_temporario() -> tempfile::TempDir {
        tempfile::Builder::new()
            .prefix("p-")
            .tempdir_in(base_dos_projetos())
            .expect("tmp")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palavra_presente_respeita_a_borda_do_identificador() {
        assert!(palavra_presente("const a = foo;", "foo"));
        assert!(palavra_presente("foo", "foo"));
        assert!(palavra_presente("x.foo()", "foo"));
        assert!(!palavra_presente("foobar", "foo"));
        assert!(!palavra_presente("_foo", "foo"));
        assert!(!palavra_presente("$foo", "foo"));
        assert!(!palavra_presente("foo1", "foo"));
        assert!(palavra_presente("foobar foo", "foo"), "segunda ocorrência");
        assert!(!palavra_presente("", "foo"));
        assert!(!palavra_presente("foo", ""));
    }

    #[test]
    fn distancia_e_sugestao() {
        assert_eq!(distancia("getTime", "getTim"), 1);
        assert_eq!(distancia("a", "a"), 0);
        let reais = vec!["getTime".to_string(), "setTime".to_string()];
        assert_eq!(mais_proximo("getTim", &reais).as_deref(), Some("getTime"));
        assert_eq!(mais_proximo("zzzzzz", &reais), None);
        assert_eq!(
            mais_proximo("getTime", &reais).as_deref(),
            Some("setTime"),
            "o mesmo nome não é sugestão de si"
        );
        assert_eq!(
            mais_proximo("getTime", &["getTime".to_string()]),
            None,
            "distância 0 não é sugestão"
        );
    }

    #[test]
    fn ultimo_segmento_tira_query_e_barra() {
        assert_eq!(ultimo_segmento("./a/b"), "b");
        assert_eq!(
            ultimo_segmento("./__vgpModAusente1?raw"),
            "__vgpModAusente1"
        );
        assert_eq!(ultimo_segmento("../x/"), "x");
        assert_eq!(ultimo_segmento("pkg"), "pkg");
    }
}
