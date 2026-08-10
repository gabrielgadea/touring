//! Bilingual (pt-BR / en) tokenizer for intent matching over mined prose.
//!
//! The portfolio must answer intents the way its owner actually phrases them.
//! Measured on 2026-08-08: `touring search-tools "gerar PDF profissional"` →
//! `No matching tool`, while the English phrasing of the *same* intent ranked a
//! result. The corpus is overwhelmingly English (docstrings, `//!` headers) and
//! the intents are frequently Portuguese, so lexical BM25 alone never meets.
//!
//! The fix is symmetric normalization: **both** the corpus and the query are
//! folded to a canonical English term set, so "mapa" (doc) and "map" (query)
//! meet in the middle. This is deliberately a small curated table rather than a
//! translation model — it is total, offline, deterministic, and costs
//! microseconds. Terms outside the table pass through unchanged, which keeps
//! identifiers, acronyms and jargon (PDF, SIMD, tantivy) intact.

/// Minimum length of a token kept after normalization.
const MIN_TOKEN_LEN: usize = 2;

/// Fold pt-BR diacritics to ASCII so "gráfico" and "grafico" are one term.
///
/// Deliberately covers only the Portuguese range — this is a lexical fold for
/// matching, not a general Unicode normalizer.
#[must_use]
pub fn fold_accents(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'ñ' => 'n',
            other => other,
        })
        .collect()
}

/// True for tokens that carry no intent signal in either language.
fn is_stopword(w: &str) -> bool {
    matches!(
        w,
        // English
        "the" | "a" | "an" | "of" | "to" | "for" | "in" | "on" | "and" | "or" | "is" | "are"
            | "with" | "how" | "do" | "does" | "did" | "me" | "my" | "want" | "need" | "all"
            | "that" | "this" | "it" | "be" | "can" | "from" | "by" | "as" | "at" | "into"
            // Portuguese (accent-folded). "do"/"as" are already covered above —
            // the two stopword sets overlap and the match must not repeat arms.
            | "o" | "os" | "um" | "uma" | "uns" | "umas" | "de" | "da" | "dos"
            | "das" | "em" | "no" | "na" | "nos" | "nas" | "por" | "para" | "pra" | "com"
            | "sem" | "que" | "ou" | "se" | "ao" | "aos" | "sao" | "ser" | "estar" | "esta"
            | "isso" | "isto" | "esse" | "essa" | "este" | "meu" | "minha" | "seu" | "sua"
            | "qual" | "quais" | "como" | "onde" | "quando" | "porque" | "mais" | "menos"
            | "muito" | "ja" | "ainda" | "tambem" | "sobre" | "entre" | "ate" | "antes"
            | "depois" | "todo" | "toda" | "todos" | "todas" | "quero" | "preciso" | "fazer"
    )
}

/// pt-BR → canonical English term. Keys are already accent-folded and lowercase.
///
/// Curated for the domains this workspace actually works in (code intelligence,
/// document generation, data pipelines). Growth is expected and cheap; an
/// unmapped term simply passes through.
const PT_TO_EN: &[(&str, &str)] = &[
    // verbs
    ("gerar", "generate"),
    ("gera", "generate"),
    ("geracao", "generate"),
    ("criar", "create"),
    ("cria", "create"),
    ("criacao", "create"),
    ("buscar", "search"),
    ("busca", "search"),
    ("procurar", "search"),
    ("pesquisar", "search"),
    ("encontrar", "find"),
    ("achar", "find"),
    ("ler", "read"),
    ("leitura", "read"),
    ("escrever", "write"),
    ("escrita", "write"),
    ("validar", "validate"),
    ("validacao", "validate"),
    ("verificar", "verify"),
    ("testar", "test"),
    ("converter", "convert"),
    ("conversao", "convert"),
    ("extrair", "extract"),
    ("extracao", "extract"),
    ("analisar", "analyze"),
    ("analise", "analysis"),
    ("medir", "measure"),
    ("medicao", "measure"),
    ("calcular", "compute"),
    ("indexar", "index"),
    ("listar", "list"),
    ("mostrar", "show"),
    ("exibir", "show"),
    ("enviar", "send"),
    ("baixar", "download"),
    ("carregar", "load"),
    ("salvar", "save"),
    ("exportar", "export"),
    ("importar", "import"),
    ("renderizar", "render"),
    ("formatar", "format"),
    ("comparar", "compare"),
    ("agrupar", "group"),
    ("ordenar", "sort"),
    ("filtrar", "filter"),
    ("contar", "count"),
    ("resumir", "summarize"),
    ("traduzir", "translate"),
    ("limpar", "clean"),
    ("corrigir", "fix"),
    ("consertar", "fix"),
    ("implementar", "implement"),
    ("refatorar", "refactor"),
    ("documentar", "document"),
    ("publicar", "publish"),
    ("instalar", "install"),
    ("atualizar", "update"),
    ("remover", "remove"),
    ("apagar", "delete"),
    ("mapear", "map"),
    ("desenhar", "draw"),
    ("plotar", "plot"),
    ("preencher", "fill"),
    ("assinar", "sign"),
    ("juntar", "merge"),
    ("dividir", "split"),
    // nouns
    ("mapa", "map"),
    ("grafico", "chart"),
    ("grafo", "graph"),
    ("relatorio", "report"),
    ("planilha", "spreadsheet"),
    ("documento", "document"),
    ("arquivo", "file"),
    ("pasta", "directory"),
    ("diretorio", "directory"),
    ("tabela", "table"),
    ("imagem", "image"),
    ("figura", "figure"),
    ("pagina", "page"),
    ("modelo", "model"),
    ("consulta", "query"),
    ("banco", "database"),
    ("dados", "data"),
    ("resumo", "summary"),
    ("teste", "test"),
    ("codigo", "code"),
    ("texto", "text"),
    ("linha", "line"),
    ("coluna", "column"),
    ("saida", "output"),
    ("entrada", "input"),
    ("chave", "key"),
    ("valor", "value"),
    ("caminho", "path"),
    ("projeto", "project"),
    ("versao", "version"),
    ("ambiente", "environment"),
    ("configuracao", "config"),
    ("biblioteca", "library"),
    ("ferramenta", "tool"),
    ("fluxo", "flow"),
    ("etapa", "step"),
    ("fase", "phase"),
    ("regra", "rule"),
    ("padrao", "pattern"),
    ("exemplo", "example"),
    ("grafica", "graphic"),
    ("formulario", "form"),
    ("cabecalho", "header"),
    ("rodape", "footer"),
    ("modelos", "model"),
    ("apresentacao", "presentation"),
    ("slide", "slide"),
    // adjectives / qualifiers
    ("profissional", "professional"),
    ("novo", "new"),
    ("nova", "new"),
    ("antigo", "old"),
    ("rapido", "fast"),
    ("seguro", "safe"),
    ("completo", "complete"),
    ("simples", "simple"),
];

/// Strip a single plural `s` when doing so is lexically safe.
///
/// Conservative on purpose: words ending in `ss` (process, class) or `us`
/// (status, bonus) keep their suffix, and the stem must stay at least 3 chars.
fn depluralize(w: &str) -> &str {
    if w.len() > 3 && w.ends_with('s') && !w.ends_with("ss") && !w.ends_with("us") {
        &w[..w.len() - 1]
    } else {
        w
    }
}

/// Normalize one raw word to its canonical term, or `None` if it carries no signal.
///
/// Pipeline: lowercase → accent-fold → stopword drop → length floor →
/// pt→en mapping (direct, then de-pluralized) → light English de-pluralization.
#[must_use]
pub fn normalize_term(raw: &str) -> Option<String> {
    let folded = fold_accents(&raw.to_lowercase());
    let cleaned: String = folded
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
        .collect();
    let w = cleaned.trim_matches(|c| c == '_' || c == '-');
    if w.len() < MIN_TOKEN_LEN || is_stopword(w) {
        return None;
    }
    // Direct pt→en hit.
    if let Some((_, en)) = PT_TO_EN.iter().find(|(pt, _)| *pt == w) {
        return Some((*en).to_string());
    }
    // Portuguese plural ("mapas" → "mapa" → "map").
    let singular = depluralize(w);
    if singular != w
        && let Some((_, en)) = PT_TO_EN.iter().find(|(pt, _)| *pt == singular)
    {
        return Some((*en).to_string());
    }
    if is_stopword(singular) {
        return None;
    }
    Some(singular.to_string())
}

/// Tokenize free text into canonical, deduplicated-by-position terms.
///
/// Applied identically to the corpus and to the query — that symmetry is what
/// makes a Portuguese intent reach an English docstring.
#[must_use]
pub fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-'))
        .filter(|s| !s.is_empty())
        .filter_map(normalize_term)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_portuguese_diacritics() {
        assert_eq!(fold_accents("gráfico relatório versão ção"), "grafico relatorio versao cao");
    }

    #[test]
    fn portuguese_intent_reaches_english_corpus() {
        // The exact failure measured on 2026-08-08 against `touring search-tools`.
        let query = tokenize("gerar PDF profissional");
        let doc = tokenize("Generate a professional PDF report from HTML");
        for t in ["generate", "professional", "pdf"] {
            assert!(query.contains(&t.to_string()), "query missing {t}: {query:?}");
            assert!(doc.contains(&t.to_string()), "doc missing {t}: {doc:?}");
        }
    }

    #[test]
    fn map_intent_meets_in_the_middle() {
        assert!(tokenize("gerar um mapa").contains(&"map".to_string()));
        assert!(tokenize("generate a map").contains(&"map".to_string()));
        // And the noise word "um" is dropped, not indexed.
        assert!(!tokenize("gerar um mapa").contains(&"um".to_string()));
    }

    #[test]
    fn stopwords_dropped_in_both_languages() {
        let t = tokenize("How do I generate the report for a project?");
        assert!(!t.contains(&"how".to_string()) && !t.contains(&"the".to_string()));
        let p = tokenize("Como eu faço para gerar o relatório de um projeto?");
        assert!(!p.contains(&"como".to_string()) && !p.contains(&"de".to_string()));
        // Both phrasings converge on the same content terms.
        for term in ["generate", "report", "project"] {
            assert!(t.contains(&term.to_string()), "en missing {term}: {t:?}");
            assert!(p.contains(&term.to_string()), "pt missing {term}: {p:?}");
        }
    }

    #[test]
    fn depluralize_is_conservative() {
        assert_eq!(depluralize("maps"), "map");
        assert_eq!(depluralize("process"), "process", "ss must survive");
        assert_eq!(depluralize("status"), "status", "us must survive");
        assert_eq!(depluralize("css"), "css");
        assert_eq!(depluralize("is"), "is", "too short to strip");
    }

    #[test]
    fn portuguese_plural_maps_to_english_singular() {
        assert_eq!(normalize_term("mapas").as_deref(), Some("map"));
        assert_eq!(normalize_term("relatórios").as_deref(), Some("report"));
    }

    #[test]
    fn unknown_terms_pass_through_untouched() {
        // Identifiers, acronyms and jargon must not be mangled.
        for t in ["tantivy", "simd", "rkyv", "bm25", "fastembed"] {
            assert_eq!(normalize_term(t).as_deref(), Some(t), "mangled {t}");
        }
    }

    #[test]
    fn punctuation_and_short_tokens_are_dropped() {
        let t = tokenize("a, b. c! generate-map_v2");
        assert!(!t.contains(&"a".to_string()));
        assert!(t.iter().any(|w| w.contains("generate-map_v2")));
    }
}
