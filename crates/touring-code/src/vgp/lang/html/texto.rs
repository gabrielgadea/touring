//! Text helpers of the html adapter: character references, percent-decoding, template markers,
//! the lexical `id=` scan and the edit distance behind "você quis dizer".

use std::borrow::Cow;

/// Markers of server-side or client-side templating: a value carrying one is not a literal
/// reference, so the adapter cannot say whether it resolves (`unknown`, never a finding).
const MARCADORES_DE_TEMPLATE: &[&str] = &[
    "{{", "{%", "<%", "<?", "${", "{$", "#{", "%7B%7B", "%7b%7b", "%7B%25", "%7b%25",
];

/// Does `s` carry a templating marker (`{{x}}`, `{% x %}`, `<%= x %>`, `<?php`, `${x}`…)?
pub(super) fn tem_template(s: &str) -> bool {
    MARCADORES_DE_TEMPLATE.iter().any(|m| s.contains(m))
}

/// Decode the HTML character references an attribute value may carry (`&amp;`, `&#38;`,
/// `&#x26;`, `&nbsp;`…). A reference the adapter does not know stays verbatim.
pub(super) fn decodificar_entidades(s: &str) -> Cow<'_, str> {
    if !s.contains('&') {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    let mut resto = s;
    while let Some(i) = resto.find('&') {
        out.push_str(&resto[..i]);
        let depois = &resto[i + 1..];
        match entidade(depois) {
            Some((ch, consumidos)) => {
                out.push(ch);
                resto = &depois[consumidos..];
            }
            None => {
                out.push('&');
                resto = depois;
            }
        }
    }
    out.push_str(resto);
    Cow::Owned(out)
}

/// The character a reference names and how many bytes of `s` (the text after the `&`) it used.
fn entidade(s: &str) -> Option<(char, usize)> {
    let fim = s.find(';').filter(|f| (1..=32).contains(f))?;
    let corpo = &s[..fim];
    let ch = match corpo.strip_prefix('#') {
        Some(num) => referencia_numerica(num)?,
        None => referencia_nomeada(corpo)?,
    };
    Some((ch, fim + 1))
}

/// `&#38;` / `&#x26;` — the digits after the `#`.
fn referencia_numerica(num: &str) -> Option<char> {
    let cod = match num.strip_prefix(['x', 'X']) {
        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
        None => num.parse().ok()?,
    };
    char::from_u32(cod)
}

/// The few named references that appear in URLs and ids.
fn referencia_nomeada(nome: &str) -> Option<char> {
    Some(match nome {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{a0}',
        _ => return None,
    })
}

/// `s` with its `%XX` escapes decoded, when it has any and the result is valid UTF-8; `None`
/// otherwise (nothing to decode, or bytes that are not text).
pub(super) fn decodificar_percentual(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut mudou = false;
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2]))
        {
            out.push(h * 16 + l);
            mudou = true;
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    if !mudou {
        return None;
    }
    String::from_utf8(out).ok()
}

fn hex(b: u8) -> Option<u8> {
    char::from(b).to_digit(16).map(|d| d as u8)
}

/// Every value of an `id=` attribute written in `texto`, in source order — a LEXICAL scan, so it
/// also sees what the parser dropped in an error region, inside a comment, a `<textarea>` or a
/// template string. Over-approximating the ids that exist is the safe side for "id missing".
pub(super) fn ids_no_texto(texto: &str) -> Vec<&str> {
    let b = texto.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < b.len() {
        let inicio_de_palavra = i == 0 || !eh_de_nome(b[i - 1]);
        if inicio_de_palavra
            && b[i] | 0x20 == b'i'
            && b[i + 1] | 0x20 == b'd'
            && let Some((valor, fim)) = valor_do_atributo(texto, i + 2)
        {
            out.push(valor);
            i = fim;
            continue;
        }
        i += 1;
    }
    out
}

/// A character that can be part of an attribute or identifier name: `data-id`, `valid`, `x.id`
/// are not an `id=` attribute.
fn eh_de_nome(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b':' | b'.')
}

/// After `id`: optional blanks, `=`, optional blanks, then a quoted or bare value.
fn valor_do_atributo(texto: &str, de: usize) -> Option<(&str, usize)> {
    let b = texto.as_bytes();
    let i = pular_brancos(b, de);
    if b.get(i) != Some(&b'=') {
        return None;
    }
    let i = pular_brancos(b, i + 1);
    let (inicio, fim) = match b.get(i)? {
        q @ (b'"' | b'\'') => {
            let ini = i + 1;
            (ini, ini + b[ini..].iter().position(|c| c == q)?)
        }
        _ => (i, fim_do_valor_sem_aspas(b, i)),
    };
    let valor = texto.get(inicio..fim)?;
    (!valor.is_empty()).then_some((valor, fim))
}

fn pular_brancos(b: &[u8], mut i: usize) -> usize {
    while i < b.len() && b[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}

/// A bare attribute value ends at the first blank or `>`.
fn fim_do_valor_sem_aspas(b: &[u8], de: usize) -> usize {
    b[de..]
        .iter()
        .position(|c| c.is_ascii_whitespace() || *c == b'>')
        .map_or(b.len(), |p| de + p)
}

/// Edit distance over characters (insert, delete, substitute).
pub(super) fn distancia(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            cur.push(
                (prev[j] + usize::from(ca != cb))
                    .min(cur[j] + 1)
                    .min(prev[j + 1] + 1),
            );
        }
        prev = cur;
    }
    prev[b.len()]
}

/// How many distances "você quis dizer" computes at most (a document with a hundred thousand ids
/// must not turn a typo into a quadratic scan).
const TETO_DE_CANDIDATOS: usize = 2000;

/// The candidates closest to `alvo` (at most `n`), nearest first: edit distance up to a quarter of
/// the length (at least 2), or equal ignoring case. Never `alvo` itself. Candidates whose length
/// is too far from `alvo`'s are skipped before any distance is computed.
pub(super) fn mais_proximos<'a>(
    alvo: &str,
    candidatos: impl Iterator<Item = &'a str>,
    n: usize,
) -> Vec<String> {
    let tamanho = alvo.chars().count();
    let limite = (tamanho / 4).max(2);
    let mut achados: Vec<(usize, &str)> = candidatos
        .filter(|c| *c != alvo && c.chars().count().abs_diff(tamanho) <= limite)
        .take(TETO_DE_CANDIDATOS)
        .filter_map(|c| {
            if c.eq_ignore_ascii_case(alvo) {
                return Some((0, c));
            }
            let d = distancia(alvo, c);
            (d <= limite).then_some((d, c))
        })
        .collect();
    achados.sort();
    achados
        .into_iter()
        .take(n)
        .map(|(_, c)| c.to_string())
        .collect()
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn entidades_numericas_nomeadas_e_desconhecidas() {
        assert_eq!(decodificar_entidades("a&amp;b"), "a&b");
        assert_eq!(decodificar_entidades("&#38;&#x26;&#X26;"), "&&&");
        assert_eq!(decodificar_entidades("&lt;&gt;&quot;&apos;"), "<>\"'");
        assert_eq!(decodificar_entidades("a&nbsp;b"), "a\u{a0}b");
        // desconhecida, sem ponto e vírgula e fora de faixa ficam como estão
        assert_eq!(decodificar_entidades("&foo; & &amp"), "&foo; & &amp");
        assert_eq!(decodificar_entidades("&#99999999999;"), "&#99999999999;");
        assert!(matches!(decodificar_entidades("sem"), Cow::Borrowed(_)));
    }

    #[test]
    fn percentual_so_decodifica_utf8_valido() {
        assert_eq!(decodificar_percentual("a%20b").as_deref(), Some("a b"));
        assert_eq!(decodificar_percentual("caf%C3%A9").as_deref(), Some("café"));
        assert_eq!(decodificar_percentual("sem-escape"), None);
        assert_eq!(decodificar_percentual("100%"), None, "% solto não é escape");
        assert_eq!(decodificar_percentual("a%2"), None);
        assert_eq!(decodificar_percentual("%ZZ"), None);
        assert_eq!(decodificar_percentual("%FF"), None, "byte que não é UTF-8");
        assert_eq!(decodificar_percentual("%41%").as_deref(), Some("A%"));
    }

    #[test]
    fn marcadores_de_template() {
        for t in [
            "{{x}}", "{% if %}", "<%= x %>", "<?php", "${x}", "{$x}", "#{x}", "%7B%7Bx",
        ] {
            assert!(tem_template(t), "{t}");
        }
        for t in ["a.png", "img/{x.png", "a#b", "a$b", "100%"] {
            assert!(!tem_template(t), "{t}");
        }
    }

    #[test]
    fn ids_no_texto_le_atributo_id_e_so_ele() {
        let t = "<div id=\"a\"> <p ID = 'b'> <i id=c> data-id=\"nao\" valid=\"nao\" x.id=\"nao\" \
                 <span id=\"\"> <b id=\"d";
        assert_eq!(ids_no_texto(t), vec!["a", "b", "c"]);
        assert_eq!(ids_no_texto("el.innerHTML = '<a id=\"x\">'"), vec!["x"]);
        assert!(ids_no_texto("id").is_empty());
        assert!(
            ids_no_texto("\"id\": \"x\"").is_empty(),
            "JSON não é atributo"
        );
    }

    #[test]
    fn distancia_e_proximos() {
        assert_eq!(distancia("kitten", "sitting"), 3);
        assert_eq!(distancia("", "abc"), 3);
        assert_eq!(distancia("abc", "abc"), 0);
        let c = ["introducao", "conclusao", "Conclusao", "x"];
        assert_eq!(
            mais_proximos("conclusoa", c.iter().copied(), 5),
            vec!["conclusao"],
            "transposição = 2 edições; `Conclusao` seria 3"
        );
        assert_eq!(
            mais_proximos("conclusao", c.iter().copied(), 1),
            vec!["Conclusao"]
        );
        assert!(mais_proximos("zzzzzz", c.iter().copied(), 5).is_empty());
        assert!(
            mais_proximos("ab", ["ab"].iter().copied(), 5).is_empty(),
            "nunca o próprio"
        );
    }
}
