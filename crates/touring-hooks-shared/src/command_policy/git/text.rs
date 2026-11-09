//! String and heredoc readers shared by the nested-code scanners.

/// Command/process substitutions in one simple command. Arithmetic `$((…))` is
/// deliberately not code for this scanner.
pub(super) fn substitutions(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 1 < bytes.len() {
        let marker = match (bytes[i], bytes[i + 1]) {
            (b'$', b'(') | (b'<', b'(') | (b'>', b'(') => Some('('),
            (b'`', _) => Some('`'),
            _ => None,
        };
        let Some(marker) = marker else {
            i += 1;
            continue;
        };
        if marker == '(' && bytes[i] == b'$' && bytes.get(i + 2) == Some(&b'(') {
            i += 2;
            continue;
        }
        if marker == '`' {
            if let Some(end) = text[i + 1..].find('`') {
                out.push(text[i + 1..i + 1 + end].to_owned());
                i += end + 2;
            } else {
                break;
            }
            continue;
        }
        let start = i + 1;
        let region = call_region(text, start, true);
        if region.len() < text.len() - start - 1 {
            out.push(region.to_owned());
            i = start + region.len() + 2;
        } else {
            break;
        }
    }
    out
}

/// The inside of a balanced call, or the rest of a statement for the exec
/// functions that also accept their program without parentheses.
pub(super) fn call_region(code: &str, start: usize, parens: bool) -> &str {
    if !parens {
        return code[start..]
            .split([';', '\n'])
            .next()
            .unwrap_or(&code[start..]);
    }
    let bytes = code.as_bytes();
    let mut depth = 0;
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'\'' | b'"' | b'`' => {
                i = skip_string(code, i);
                continue;
            }
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return &code[start + 1..i];
                }
            }
            _ => {}
        }
        i += 1;
    }
    &code[start + 1..]
}

fn skip_string(code: &str, start: usize) -> usize {
    let bytes = code.as_bytes();
    let quote = bytes[start];
    let mut i = start + 1;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
        } else if bytes[i] == quote {
            return i + 1;
        } else {
            i += 1;
        }
    }
    bytes.len()
}

/// String literals in an interpreter call, with escapes decoded just enough to
/// reconstruct the program handed to a shell. Other Unicode is preserved.
pub(super) fn string_literals(region: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = region.chars().peekable();
    while let Some(c) = chars.next() {
        if !matches!(c, '\'' | '"' | '`') {
            continue;
        }
        let quote = c;
        let mut value = String::new();
        while let Some(c) = chars.next() {
            match c {
                '\\' => match chars.next() {
                    Some('n' | 't') => value.push(' '),
                    Some(other) => value.push(other),
                    None => {}
                },
                c if c == quote => break,
                other => value.push(other),
            }
        }
        out.push(value);
    }
    out
}

pub(super) fn backticks(code: &str) -> Vec<String> {
    code.split('`')
        .skip(1)
        .step_by(2)
        .map(str::to_owned)
        .collect()
}
