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

/// Every heredoc body in a program. Whether it is code or data is decided by
/// the command that consumes it, never by this reader.
pub(super) fn heredoc_bodies(command: &str) -> Vec<String> {
    let lines: Vec<&str> = command.split('\n').collect();
    let mut bodies = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let Some((delimiter, strip_tabs)) = heredoc_delimiter(lines[i]) else {
            i += 1;
            continue;
        };
        let mut body = String::new();
        i += 1;
        while i < lines.len() {
            let compared = if strip_tabs {
                lines[i].trim_start_matches('\t')
            } else {
                lines[i]
            };
            if compared == delimiter {
                break;
            }
            body.push_str(lines[i]);
            body.push('\n');
            i += 1;
        }
        bodies.push(body);
        i += 1;
    }
    bodies
}

fn heredoc_delimiter(line: &str) -> Option<(String, bool)> {
    let bytes = line.as_bytes();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if bytes[i] == b'<' && bytes[i + 1] == b'<' && bytes.get(i + 2) != Some(&b'<') {
            let mut j = i + 2;
            let strip_tabs = bytes.get(j) == Some(&b'-');
            if strip_tabs {
                j += 1;
            }
            while bytes.get(j).is_some_and(|b| matches!(b, b' ' | b'\t')) {
                j += 1;
            }
            let rest = &line[j..];
            let delimiter = rest
                .split(|c: char| c.is_whitespace() || ";&|<>()".contains(c))
                .next()?
                .trim_matches(['\'', '"']);
            if !delimiter.is_empty() {
                return Some((delimiter.to_owned(), strip_tabs));
            }
        }
        i += 1;
    }
    None
}

/// Blank only heredoc bodies attached to a direct shell that already has a
/// program (`-c` or a script file). The general lexer stays conservative; this
/// mask is the git-specific proof that the body is input, not another program.
pub(super) fn mask_script_heredocs(command: &str) -> String {
    let lines: Vec<&str> = command.split('\n').collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let Some((delimiter, strip_tabs)) = heredoc_delimiter(line) else {
            out.push(line);
            i += 1;
            continue;
        };
        let prefix = line.find("<<").map_or(line, |at| &line[..at]);
        let is_script_input = super::super::lexer::shell_pipelines(prefix)
            .into_iter()
            .flatten()
            .any(|words| {
                let Some((program, args)) = super::super::lexer::command_of(&words) else {
                    return false;
                };
                let name = program.rsplit('/').next().unwrap_or(program);
                if !matches!(
                    name,
                    "bash"
                        | "sh"
                        | "zsh"
                        | "dash"
                        | "ksh"
                        | "ash"
                        | "fish"
                        | "mksh"
                        | "rbash"
                        | "csh"
                        | "tcsh"
                ) {
                    return false;
                }
                let (payload, reads_stdin) = super::nested::shell_invocation(args);
                payload.is_some() || !reads_stdin
            });
        out.push(line);
        i += 1;
        while i < lines.len() {
            let compared = if strip_tabs {
                lines[i].trim_start_matches('\t')
            } else {
                lines[i]
            };
            if compared == delimiter {
                break;
            }
            out.push(if is_script_input { "" } else { lines[i] });
            i += 1;
        }
        if i < lines.len() {
            out.push(lines[i]);
            i += 1;
        }
    }
    out.join("\n")
}
