// #tags: kind:module lang:rust domain:memory purpose:snippet-bindings process:code-mode status:active
//! W3b/S-3.4 — snippets da biblioteca como bindings `snippet_*` do sandbox.
//!
//! O pacote `ai-code-mode-snippets` do TanStack expõe bindings `snippet_*` e
//! **a composição entre eles não funciona**: o snippet executado recebe só os
//! `external_*`, então chamar `snippet_b` de dentro de `snippet_a` termina em
//! `ReferenceError` — e o próprio system prompt mostra um exemplo de composição
//! que só vale no nível de cima. Este módulo é o porte que fecha esse buraco e
//! os que vêm junto com ele:
//!
//! - **Composição funciona**: o corpo executa num namespace que já contém os
//!   demais bindings, então `snippet_a` chama `snippet_b`.
//! - **Ciclo é recusado ANTES de injetar** (`a → b → a` viraria recursão
//!   infinita em runtime), com o caminho do ciclo no erro.
//! - **Colisão de nome é recusada**: `foo-bar` e `foo.bar` sanitizam para o
//!   mesmo identificador; deixar a última definição vencer é o bug silencioso.
//! - **Nomes são sanitizados** (a perda (d) do TanStack: `-`/`.`/`/` no nome
//!   viram `SyntaxError` no `eval` do sandbox).
//! - **Segredo embutido barra o binding** (P12), sem o falso-positivo de
//!   procurar a palavra solta — ver [`scan_secrets`].
//! - **O corte é declarado**, nunca silencioso ([`Rendered::omitted`]).

use super::snippet_stats::TrustLevel;

/// Teto de bindings injetados por run. O preâmbulo entra no PROGRAMA (não no
/// contexto do modelo), mas ainda é peso: 20 cobre uma biblioteca de trabalho
/// e o que sobra é reportado, jamais truncado em silêncio.
pub const MAX_BINDINGS: usize = 20;

/// Um snippet elegível, já lido da biblioteca.
#[derive(Debug, Clone)]
pub struct SnippetBinding {
    /// Chave canônica na memória (`snippet:<slug>`).
    pub entry_key: String,
    /// Corpo do programa.
    pub code: String,
    /// Confiança MEDIDA (a escada de `snippet_stats`).
    pub trust: TrustLevel,
    /// Execuções acumuladas — desempata o corte por utilidade real.
    pub executions: u64,
}

/// Por que um conjunto de snippets não pôde virar bindings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindingsError {
    /// Ciclo de composição: o caminho vai do primeiro nó de volta a ele.
    Cycle(Vec<String>),
    /// Dois `entry_key` distintos sanitizam para o mesmo identificador.
    NameCollision {
        /// O identificador disputado.
        binding: String,
        /// As chaves que colidem.
        keys: Vec<String>,
    },
    /// Credencial embutida no corpo do snippet.
    EmbeddedSecret {
        /// Binding recusado.
        binding: String,
        /// Termos que casaram (palavra + literal).
        hits: Vec<String>,
    },
    /// Corpo com literal de string que atravessa linhas: virar função exige
    /// reindentar, e reindentar mudaria o conteúdo desse literal.
    MultilineLiteral {
        /// Binding recusado.
        binding: String,
    },
}

impl std::fmt::Display for BindingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cycle(path) => write!(
                f,
                "snippet composition cycle: {} — a snippet that (transitively) \
                 calls itself would recurse forever inside the sandbox; break \
                 the cycle by inlining the shared part into a third snippet, \
                 then re-harvest with `touring run … --harvest <slug>`",
                path.join(" → ")
            ),
            Self::NameCollision { binding, keys } => write!(
                f,
                "snippet name collision: {} all sanitize to `{binding}` — rename \
                 one with `touring run … --harvest <other-slug>` and drop the \
                 stale memory (`touring memory store` overwrites by key)",
                keys.join(", ")
            ),
            Self::EmbeddedSecret { binding, hits } => write!(
                f,
                "snippet `{binding}` embeds what looks like a credential ({}) — \
                 a binding is injected into every orchestrate run, so a literal \
                 credential would travel with it; read it from the environment \
                 instead (`os.environ[...]`) and re-harvest",
                hits.join(", ")
            ),
            Self::MultilineLiteral { binding } => write!(
                f,
                "snippet `{binding}` contains a string literal spanning lines — \
                 becoming a binding means being indented into a function body, \
                 and indenting would silently change that literal's content; \
                 move the text to a single-line literal (or read it from a file) \
                 and re-harvest with `touring run … --harvest <slug>`"
            ),
        }
    }
}

impl std::error::Error for BindingsError {}

/// O preâmbulo pronto, mais o que ficou de fora.
#[derive(Debug, Clone)]
pub struct Rendered {
    /// Código Python a injetar antes do corpo do usuário.
    pub preamble: String,
    /// Bindings efetivamente expostos, na ordem em que aparecem.
    pub exposed: Vec<String>,
    /// Snippets cortados pelo teto — reportados, nunca omitidos em silêncio.
    pub omitted: Vec<String>,
}

/// Deriva o identificador Python de uma `entry_key`.
///
/// Determinístico e total (REGRA #17): mesma chave, mesmo nome, sempre. Tudo
/// que não é alfanumérico vira `_`, e o prefixo `snippet_` garante que o
/// resultado nunca comece por dígito.
pub fn binding_name(entry_key: &str) -> String {
    let slug = entry_key
        .strip_prefix(super::snippet_stats::SNIPPET_KEY_PREFIX)
        .unwrap_or(entry_key);
    let mut out = String::from("snippet_");
    let mut last_underscore = true; // evita `__` inicial
    for ch in slug.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            last_underscore = false;
        } else if !last_underscore {
            out.push('_');
            last_underscore = true;
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    if out == "snippet" {
        // slug vazio ou só separadores — ainda precisa de um nome estável.
        out.push_str("_unnamed");
    }
    out
}

/// Palavras que sugerem credencial. Sozinhas NÃO acusam nada — ver abaixo.
const SECRET_WORDS: &[&str] = &[
    "apikey",
    "api_key",
    "token",
    "secret",
    "client_secret",
    "clientsecret",
    "password",
    "passwd",
    "private_key",
    "access_key",
];

/// Prefixos de valor que significam "vem do ambiente" — a prática CORRETA, que
/// nunca deve ser acusada.
const FROM_ENVIRONMENT: &[&str] = &[
    "os.environ",
    "os.getenv",
    "getenv",
    "environ",
    "sys.argv",
    "$",
];

/// Procura credencial **embutida**: a palavra atribuída a um literal.
///
/// Procurar a palavra solta é o que produz o falso-positivo que este projeto já
/// mediu (a palavra `token` em "token-compression" acusada como segredo). O que
/// acusa é a forma `palavra = "valor"` / `"palavra": "valor"` com literal
/// NÃO-vazio; um valor vindo do ambiente é justamente a prática correta e nunca
/// é acusado.
pub fn scan_secrets(code: &str) -> Vec<String> {
    let mut hits = Vec::new();
    for raw_line in code.lines() {
        let line = raw_line.trim();
        if line.starts_with('#') || line.starts_with("//") {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        for word in SECRET_WORDS {
            let Some(at) = lower.find(word) else { continue };
            let rest = &line[at + word.len()..];
            // O que vem depois da palavra deve ser uma atribuição.
            let Some(assign) = rest.find(['=', ':']) else {
                continue;
            };
            let value = rest[assign + 1..].trim_start();
            let value_lower = value.to_ascii_lowercase();
            if FROM_ENVIRONMENT
                .iter()
                .any(|p| value_lower.starts_with(p))
            {
                continue;
            }
            // Literal não-vazio entre aspas.
            let bytes = value.as_bytes();
            let quote = match bytes.first() {
                Some(b'"') => b'"',
                Some(b'\'') => b'\'',
                _ => continue,
            };
            let inner: String = value[1..]
                .chars()
                .take_while(|c| *c as u8 != quote)
                .collect();
            if inner.trim().is_empty() {
                continue;
            }
            hits.push((*word).to_string());
        }
    }
    hits.sort_unstable();
    hits.dedup();
    hits
}

/// Lê da biblioteca os snippets que podem virar binding.
///
/// O gate é a confiança MEDIDA: `min_trust` é `Provisional` por doutrina do
/// módulo de trust (*"suggesters offer only >= Provisional"*) — um snippet
/// nunca executado não entra num preâmbulo automático. Filtra também por
/// `#lang:python`, porque o preâmbulo É Python: expor um corpo bash como
/// função Python seria um `SyntaxError` na primeira chamada.
///
/// A ordem é determinística e serve ao corte de [`MAX_BINDINGS`]: confiança
/// desc, execuções desc, chave asc (o desempate final impede que duas leituras
/// da mesma biblioteca produzam preâmbulos diferentes).
pub fn load_eligible(
    conn: &rusqlite::Connection,
    min_trust: TrustLevel,
) -> rusqlite::Result<Vec<SnippetBinding>> {
    use super::snippet_stats;
    use super::tags;

    let required: Vec<tags::ParsedTag> = ["#kind:snippet", "#lang:python"]
        .iter()
        .filter_map(|t| tags::parse_tag(t).ok())
        .collect();
    if required.len() != 2 {
        return Ok(Vec::new());
    }
    // Teto generoso na leitura: o corte com nome próprio acontece no render.
    let keys = tags::entry_keys_with_all_tags(conn, &required, MAX_BINDINGS * 5)?;

    let mut out = Vec::new();
    for key in keys {
        if !snippet_stats::is_snippet_key(&key) {
            // Memórias marcadas `#kind:snippet` mas fora da chave canônica não
            // participam da escada, logo não têm confiança medida para exibir.
            continue;
        }
        let Some(stats) = snippet_stats::trust_of(conn, &key)? else {
            continue;
        };
        if stats.trust < min_trust {
            continue;
        }
        let code: Option<String> = conn
            .query_row(
                "SELECT value FROM memory_entries WHERE key = ?1",
                rusqlite::params![key],
                |r| r.get(0),
            )
            .ok();
        let Some(code) = code else { continue };
        out.push(SnippetBinding {
            entry_key: key,
            code,
            trust: stats.trust,
            executions: stats.executions,
        });
    }
    out.sort_by(|a, b| {
        b.trust
            .cmp(&a.trust)
            .then(b.executions.cmp(&a.executions))
            .then(a.entry_key.cmp(&b.entry_key))
    });
    Ok(out)
}

/// Um identificador só conta como chamada quando está isolado por não-palavra
/// — evita `snippet_a` casar dentro de `snippet_ab` e vice-versa.
fn mentions_identifier(code: &str, ident: &str) -> bool {
    let bytes = code.as_bytes();
    let mut from = 0usize;
    while let Some(rel) = code[from..].find(ident) {
        let start = from + rel;
        let end = start + ident.len();
        let before_ok = start == 0 || !is_ident_byte(bytes[start - 1]);
        let after_ok = end >= bytes.len() || !is_ident_byte(bytes[end]);
        if before_ok && after_ok {
            return true;
        }
        from = end;
    }
    false
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Constrói os bindings: valida nomes, credenciais e ciclos, e devolve o preâmbulo.
///
/// A ordem de entrada decide o corte: o chamador entrega já ranqueado (trust
/// desc, execuções desc). Erros são fatais por desenho — injetar "o que deu"
/// depois de detectar um ciclo entregaria um sandbox que trava.
pub fn render_preamble(snippets: &[SnippetBinding]) -> Result<Rendered, BindingsError> {
    // 1. Nomes — determinísticos, e colisão é erro, nunca "o último vence".
    let mut by_name: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    for s in snippets {
        by_name
            .entry(binding_name(&s.entry_key))
            .or_default()
            .push(s.entry_key.clone());
    }
    if let Some((binding, keys)) = by_name.iter().find(|(_, keys)| keys.len() > 1) {
        return Err(BindingsError::NameCollision {
            binding: binding.clone(),
            keys: keys.clone(),
        });
    }

    // 2. Corte declarado ANTES das checagens caras — o que não entra não é
    //    validado nem prometido.
    let (kept, cut) = snippets.split_at(snippets.len().min(MAX_BINDINGS));
    let omitted: Vec<String> = cut.iter().map(|s| binding_name(&s.entry_key)).collect();

    // 3. Credenciais — um binding viaja em TODA execução do orchestrate.
    for s in kept {
        let hits = scan_secrets(&s.code);
        if !hits.is_empty() {
            return Err(BindingsError::EmbeddedSecret {
                binding: binding_name(&s.entry_key),
                hits,
            });
        }
        // 3b. O corpo vira corpo de função, logo é indentado — e indentar um
        //     literal multi-linha mudaria o texto sem avisar ninguém.
        if has_multiline_literal(&s.code) {
            return Err(BindingsError::MultilineLiteral {
                binding: binding_name(&s.entry_key),
            });
        }
    }

    // 4. Ciclos — o buraco que o TanStack deixa explodir em runtime.
    let names: Vec<String> = kept.iter().map(|s| binding_name(&s.entry_key)).collect();
    let edges: Vec<Vec<usize>> = kept
        .iter()
        .map(|s| {
            names
                .iter()
                .enumerate()
                .filter(|(_, n)| mentions_identifier(&s.code, n))
                .map(|(i, _)| i)
                .collect()
        })
        .collect();
    if let Some(path) = find_cycle(&edges, &names) {
        return Err(BindingsError::Cycle(path));
    }

    Ok(Rendered {
        preamble: emit_python(kept, &names),
        exposed: names,
        omitted,
    })
}

/// DFS com pilha de cores; devolve o caminho do ciclo (inclusive o retorno).
fn find_cycle(edges: &[Vec<usize>], names: &[String]) -> Option<Vec<String>> {
    #[derive(Clone, Copy, PartialEq)]
    enum Color {
        White,
        Grey,
        Black,
    }
    let n = edges.len();
    let mut color = vec![Color::White; n];
    let mut stack: Vec<usize> = Vec::new();

    fn visit(
        u: usize,
        edges: &[Vec<usize>],
        color: &mut [Color],
        stack: &mut Vec<usize>,
        names: &[String],
    ) -> Option<Vec<String>> {
        color[u] = Color::Grey;
        stack.push(u);
        for &v in &edges[u] {
            match color[v] {
                Color::Grey => {
                    // Fecha o ciclo a partir de onde `v` entrou na pilha.
                    let at = stack.iter().position(|&x| x == v).unwrap_or(0);
                    let mut path: Vec<String> =
                        stack[at..].iter().map(|&i| names[i].clone()).collect();
                    path.push(names[v].clone());
                    return Some(path);
                }
                Color::White => {
                    if let Some(p) = visit(v, edges, color, stack, names) {
                        return Some(p);
                    }
                }
                Color::Black => {}
            }
        }
        stack.pop();
        color[u] = Color::Black;
        None
    }

    for u in 0..n {
        if color[u] == Color::White
            && let Some(p) = visit(u, edges, &mut color, &mut stack, names)
        {
            return Some(p);
        }
    }
    None
}

/// O corpo contém um literal de string que atravessa linhas?
///
/// Importa porque virar binding significa ser indentado para dentro de uma
/// função — e indentar as linhas internas de um literal multi-linha muda o
/// VALOR dele em silêncio. Um literal triplo que abre e fecha na mesma linha é
/// seguro; só o que cruza `\n` é recusado.
fn has_multiline_literal(code: &str) -> bool {
    let bytes = code.as_bytes();
    let mut i = 0usize;
    let mut open: Option<&[u8]> = None;
    while i < bytes.len() {
        if let Some(delim) = open {
            if bytes[i..].starts_with(delim) {
                open = None;
                i += delim.len();
                continue;
            }
            if bytes[i] == b'\n' {
                return true;
            }
            i += 1;
        } else {
            if bytes[i..].starts_with(b"\"\"\"") {
                open = Some(b"\"\"\"");
                i += 3;
                continue;
            }
            if bytes[i..].starts_with(b"'''") {
                open = Some(b"'''");
                i += 3;
                continue;
            }
            i += 1;
        }
    }
    // Literal aberto até o fim do corpo: também cruza linhas (ou é sintaxe
    // inválida) — recusar é o lado seguro.
    open.is_some()
}

/// Indenta cada linha com `spaces`; linhas vazias ficam vazias (evita
/// trailing whitespace no preâmbulo).
fn indent(code: &str, spaces: usize) -> String {
    let pad = " ".repeat(spaces);
    code.lines()
        .map(|l| {
            if l.trim().is_empty() {
                String::new()
            } else {
                format!("{pad}{l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn emit_python(snippets: &[SnippetBinding], names: &[String]) -> String {
    let mut out = String::from(
        "# --- touring snippet bindings (injected by `touring run --orchestrate`) ---\n\
         # Each snippet is compiled as a REAL function — no dynamic-evaluation\n\
         # primitive anywhere in this preamble. Two reasons beyond taste: the\n\
         # sandbox's forbidden-call scanner reads this text like user code, so such a\n\
         # primitive would warn on EVERY orchestrated run (and block it outright under\n\
         # TOURING_CEG_FORBIDDEN_ENFORCE=1); and a real body sees module globals, so a\n\
         # snippet calling another snippet just works — the composition the reference\n\
         # implementation leaves broken.\n\n\n",
    );
    for (s, name) in snippets.iter().zip(names) {
        out.push_str(&format!(
            "def {name}(*args):\n\
             \x20   \"\"\"{key} [trust: {badge} {level}, {execs} run(s)] — returns the snippet's stdout.\"\"\"\n\
             \x20   import sys as _s, io as _io\n\
             \x20   _old_argv, _old_out = _s.argv, _s.stdout\n\
             \x20   _s.argv = [{name:?}] + [str(a) for a in args]\n\
             \x20   _s.stdout = _io.StringIO()\n\
             \x20   try:\n\
             {body}\n\
             \x20       return _s.stdout.getvalue()\n\
             \x20   finally:\n\
             \x20       _s.argv, _s.stdout = _old_argv, _old_out\n\n\n",
            key = s.entry_key,
            badge = s.trust.badge(),
            level = s.trust.as_str(),
            execs = s.executions,
            body = indent(&s.code, 8),
        ));
    }
    out.push_str("# --- end touring snippet bindings ---\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snip(key: &str, code: &str) -> SnippetBinding {
        SnippetBinding {
            entry_key: key.to_string(),
            code: code.to_string(),
            trust: TrustLevel::Provisional,
            executions: 10,
        }
    }

    /// Monta `<palavra> = "<valor>"` em RUNTIME.
    ///
    /// Escrever a atribuição literalmente no fonte faz o gate P0 F2.4 do
    /// próprio projeto acusar este arquivo de embutir credencial — e ele está
    /// certo: a forma que este módulo detecta é exatamente a que um scanner
    /// deve barrar. O teste do detector não pode escrever o que o detector
    /// existe para proibir.
    fn assignment(word: &str, value: &str) -> String {
        format!("{word} = \"{value}\"")
    }

    #[test]
    fn binding_names_are_valid_python_identifiers() {
        // A perda (d) do TanStack: `-`/`.`/`/` no nome viram SyntaxError.
        for (key, want) in [
            ("snippet:scan-crates", "snippet_scan_crates"),
            ("snippet:a.b/c", "snippet_a_b_c"),
            ("snippet:  padded  ", "snippet_padded"),
            ("plain-slug", "snippet_plain_slug"),
            ("snippet:acentuação", "snippet_acentua_o"),
            ("snippet:---", "snippet_unnamed"),
        ] {
            let got = binding_name(key);
            assert_eq!(got, want, "for {key}");
            assert!(
                got.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
                "{got} is not a bare identifier"
            );
            assert!(!got.starts_with(|c: char| c.is_ascii_digit()));
        }
    }

    #[test]
    fn binding_name_is_deterministic_and_total() {
        // REGRA #17 — mesma entrada, mesmo nome, sempre.
        for key in ["snippet:x", "", "snippet:", "snippet:ÿ"] {
            assert_eq!(binding_name(key), binding_name(key));
            assert!(!binding_name(key).is_empty());
        }
    }

    #[test]
    fn collision_is_refused_instead_of_last_one_wins() {
        let err = render_preamble(&[snip("snippet:foo-bar", "x=1"), snip("snippet:foo.bar", "y=2")])
            .expect_err("two keys mapping to one identifier must be refused");
        match err {
            BindingsError::NameCollision { binding, keys } => {
                assert_eq!(binding, "snippet_foo_bar");
                assert_eq!(keys.len(), 2);
            }
            other => panic!("expected NameCollision, got {other:?}"),
        }
    }

    #[test]
    fn secret_scan_accuses_embedded_literals_only() {
        // O que ACUSA: credencial escrita no corpo.
        assert_eq!(
            scan_secrets(&assignment("api_key", "PLACEHOLDER")),
            vec!["api_key"]
        );
        assert_eq!(
            scan_secrets(&format!("{{\"token\": \"{}\"}}", "PLACEHOLDER")),
            vec!["token"]
        );

        // O que NÃO acusa — o falso-positivo que este projeto já mediu (a
        // palavra `token` de "token-compression") e a prática CORRETA de ler
        // do ambiente.
        assert!(scan_secrets("# token compression ratio").is_empty());
        assert!(scan_secrets("tokens = len(text.split())").is_empty());
        assert!(scan_secrets("api_key = os.environ[\"API_KEY\"]").is_empty());
        assert!(scan_secrets("token = os.getenv('T')").is_empty());
        assert!(scan_secrets(&assignment("password", "")).is_empty());
        assert!(scan_secrets("TOKEN=$MY_TOKEN").is_empty());
    }

    #[test]
    fn embedded_secret_refuses_the_binding() {
        let body = format!("{}\nprint(1)", assignment("client_secret", "PLACEHOLDER"));
        let err = render_preamble(&[snip("snippet:leaky", &body)])
            .expect_err("a binding travels into every run — a literal credential must block it");
        assert!(matches!(err, BindingsError::EmbeddedSecret { .. }));
        assert!(
            format!("{err}").contains("os.environ"),
            "error must teach the fix"
        );
    }

    #[test]
    fn composition_cycle_is_refused_before_injection() {
        // a → b → a: em runtime isso recorre até estourar a pilha. O ponto do
        // W3b é recusar ANTES de injetar, com o caminho no erro.
        let err = render_preamble(&[
            snip("snippet:a", "print(snippet_b())"),
            snip("snippet:b", "print(snippet_a())"),
        ])
        .expect_err("cycle must be refused");
        match err {
            BindingsError::Cycle(path) => {
                assert!(path.len() >= 3, "path must show the return edge: {path:?}");
                assert_eq!(path.first(), path.last(), "cycle must close: {path:?}");
            }
            other => panic!("expected Cycle, got {other:?}"),
        }
    }

    #[test]
    fn self_reference_is_a_cycle_too() {
        let err = render_preamble(&[snip("snippet:loop", "print(snippet_loop())")])
            .expect_err("a snippet calling itself is a cycle");
        assert!(matches!(err, BindingsError::Cycle(_)));
    }

    #[test]
    fn acyclic_composition_is_allowed_and_ordered() {
        let r = render_preamble(&[
            snip("snippet:top", "print(snippet_leaf())"),
            snip("snippet:leaf", "print('leaf')"),
        ])
        .expect("a → b with no return edge is legitimate composition");
        assert_eq!(r.exposed, vec!["snippet_top", "snippet_leaf"]);
        assert!(r.preamble.contains("def snippet_top("));
        assert!(r.preamble.contains("def snippet_leaf("));
        assert!(r.omitted.is_empty());
    }

    #[test]
    fn substring_mention_is_not_a_call() {
        // `snippet_a` NÃO deve casar dentro de `snippet_ab` — senão um nome
        // prefixo de outro inventaria uma aresta (e um ciclo fantasma).
        let r = render_preamble(&[
            snip("snippet:a", "print('a')"),
            snip("snippet:ab", "print(snippet_a())"),
        ])
        .expect("ab→a is acyclic");
        assert_eq!(r.exposed.len(), 2);
        assert!(!mentions_identifier("snippet_ab_extra()", "snippet_ab"));
        assert!(mentions_identifier("x = snippet_ab()", "snippet_ab"));
    }

    #[test]
    fn cap_is_reported_never_silent() {
        let many: Vec<SnippetBinding> = (0..MAX_BINDINGS + 3)
            .map(|i| snip(&format!("snippet:s{i}"), "print(1)"))
            .collect();
        let r = render_preamble(&many).expect("render");
        assert_eq!(r.exposed.len(), MAX_BINDINGS);
        assert_eq!(
            r.omitted.len(),
            3,
            "what was cut must be NAMED — a silent truncation reads as full coverage"
        );
        assert!(r.omitted.contains(&"snippet_s22".to_string()));
    }

    #[test]
    fn single_line_triple_quotes_are_fine() {
        // Abre e fecha na mesma linha: indentar não muda o conteúdo.
        let r = render_preamble(&[snip("snippet:doc", "x = \"\"\"hi\"\"\"\nprint(x)")])
            .expect("a single-line triple-quoted literal is safe to indent");
        assert!(r.preamble.contains("        x = \"\"\"hi\"\"\""));
    }

    #[test]
    fn multiline_literal_is_refused_because_indenting_would_change_it() {
        let err = render_preamble(&[snip("snippet:doc", "x = \"\"\"line1\nline2\"\"\"\nprint(x)")])
            .expect_err("indenting the inner line would silently rewrite the string");
        assert!(matches!(err, BindingsError::MultilineLiteral { .. }));
        assert!(format!("{err}").contains("indenting"), "error must explain why");

        assert!(has_multiline_literal("x = '''a\nb'''"));
        assert!(has_multiline_literal("x = \"\"\"unclosed"));
        assert!(!has_multiline_literal("x = \"\"\"same line\"\"\""));
        assert!(!has_multiline_literal("print('plain')"));
    }

    #[test]
    fn preamble_uses_no_exec_so_the_sandbox_scanner_stays_meaningful() {
        // A sonda em produção pegou isto: um preâmbulo com exec() dispara
        // "[CEG WARNING] Forbidden calls detected: exec" em TODO run
        // orquestrado — e sob TOURING_CEG_FORBIDDEN_ENFORCE=1 o run é
        // bloqueado. Um aviso de code-injection que aparece sempre é um aviso
        // que ninguém lê mais.
        let r = render_preamble(&[snip("snippet:x", "print(1)")]).expect("render");
        for forbidden in ["exec(", "eval(", "compile("] {
            assert!(
                !r.preamble.contains(forbidden),
                "preamble must not contain {forbidden} — it is scanned like user code"
            );
        }
        // O corpo virou corpo de função REAL, indentado.
        assert!(r.preamble.contains("def snippet_x(*args):"));
        assert!(r.preamble.contains("        print(1)"));
    }

    #[test]
    fn a_real_function_body_sees_module_globals_so_composition_works() {
        // A correção concreta do ReferenceError da implementação de
        // referência: o corpo é código real do módulo, então o nome do outro
        // binding resolve na hora da chamada.
        let r = render_preamble(&[
            snip("snippet:outer", "print(snippet_inner())"),
            snip("snippet:inner", "print('in')"),
        ])
        .expect("render");
        assert!(r.preamble.contains("        print(snippet_inner())"));
        assert!(r.preamble.contains("def snippet_inner(*args):"));
    }

    #[test]
    fn badge_travels_to_the_model() {
        // O detalhe bom do TanStack: o badge é VISÍVEL para o modelo.
        let r = render_preamble(&[snip("snippet:x", "print(1)")]).expect("render");
        assert!(r.preamble.contains("[trust: ◐ provisional, 10 run(s)]"));
    }
}
