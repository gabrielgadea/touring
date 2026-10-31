//! DOM XSS sinks fed by untrusted data (CWE-79) — the code judge's own detector.
//!
//! D-4 "F21INNER" (31/10/2026, plan `docs/plans/2026-10-31-f21inner/PLANO.md`). The
//! shared `XssPattern` only knows PAYLOADS (`<script`, a `javascript:` URL, an inline
//! handler), so `el.innerHTML = location.hash` — no payload anywhere in the source —
//! scored 1.000 on F2.1. This is intra-function taint analysis over the JS/TS family
//! (`javascript`, `typescript`; `.jsx`/`.tsx` arrive as those two):
//!
//! | Part | What counts |
//! |---|---|
//! | **Sink** | `x.innerHTML = e`, `x.outerHTML = e` (also `+=`), `x.insertAdjacentHTML(pos, e)` (the 2nd argument), `document.write(e)` / `writeln`, React `dangerouslySetInnerHTML={{ __html: e }}` (and the object form) |
//! | **Source** | a parameter of the innermost enclosing function, `location.search/hash/href/pathname/host/hostname`, `document.URL/documentURI/cookie/referrer`, `window.name`, `event.data`, `fetch(...)`, `.responseText`, `URLSearchParams`, `req|request.query/body/params/headers/cookies` |
//! | **Propagation** | `const|let|var x = e`, `x = e`, `x += e`, destructuring, `obj.k = e` (taints `obj`); a plain re-assignment from a clean expression CLEARS the name |
//! | **Sanitizer** | a call whose name contains `sanitize`/`sanitise`/`escape` (`DOMPurify.sanitize`, `escapeHtml`, `_.escape`; NOT `unescape`, NOT `escapeRegExp`), a numeric coercion (`Number`, `parseInt`, `parseFloat`, `Boolean`), and the `textContent` round trip (`t.textContent = s; … t.innerHTML`) |
//! | **Safe** | a string literal, a template literal without `${}`, a constant concatenation, anything that holds no tainted name |
//!
//! **Limits, on purpose**: intra-file and intra-function. A value that crosses a call
//! (`el.innerHTML = build(x)` with `build` returning `location.hash`), a module-level
//! tainted variable read from inside a function, and a name captured by a closure are
//! NOT followed — the sink is judged only by what its own function can prove. No
//! `for (x of tainted)` binding, no `$(..).html(..)`, no `v-html`, no bracket access
//! (`el["innerHTML"]`), no HTML `<script>` bodies. A file above [`MAX_SOURCE_BYTES`]
//! is not analyzed.
//!
//! **A warning, not a verdict.** Findings leave through
//! [`super::security::SecurityReport::advisories`]: they never enter `vuln_matches`,
//! the F2.1 score or the P0 block — a false positive here would freeze every
//! Write/Edit machine-wide (the gate is P0). Promotion is a separate decision of
//! Gabriel, after the census (criterion in the plan).
//!
//! Spans start at the sink TOKEN (`innerHTML`), found on the code mask — a sink quoted
//! in a comment or a string is text, never a finding.

use std::collections::{HashMap, HashSet};

use memchr::{memchr_iter, memmem};
use touring_offensive::vuln::VulnMatch;

/// Pattern name of a tainted DOM sink (distinct from every registry name).
pub(crate) const DOM_XSS: &str = "DomXss";

/// Informational only: advisories are never summed into a score.
const DOM_XSS_SEVERITY: f32 = 6.1;
/// Above this the file is skipped (the arm is O(n) in memory: one slot per byte).
const MAX_SOURCE_BYTES: usize = 4 * 1024 * 1024;
/// An expression longer than this is cut: a runaway statement is not worth following.
const MAX_EXPR_BYTES: usize = 4096;
/// Bytes scanned backwards looking for a TypeScript annotation.
const MAX_ANNOTATION_BYTES: usize = 256;
const NO_PARTNER: usize = usize::MAX;
const NO_OWNER: u32 = u32::MAX;
/// Template literals nest through `${}`; deeper than this is cut.
const MAX_TEMPLATE_NEST: u8 = 8;

/// A sink worth the lexer: files without one of these needles are never lexed.
const SINK_NEEDLES: [&[u8]; 5] = [
    b"innerHTML",
    b"outerHTML",
    b"insertAdjacentHTML",
    b"document.write",
    b"dangerouslySetInnerHTML",
];

/// Identifiers that open a parenthesis without being a method or function name.
const NOT_A_METHOD: [&[u8]; 9] = [
    b"if",
    b"for",
    b"while",
    b"switch",
    b"catch",
    b"with",
    b"function",
    b"return",
    b"await",
];

/// Properties of `location` an attacker controls.
const LOCATION_PROPS: [&[u8]; 6] = [
    b"search",
    b"hash",
    b"href",
    b"pathname",
    b"host",
    b"hostname",
];
const DOCUMENT_PROPS: [&[u8]; 4] = [b"URL", b"documentURI", b"cookie", b"referrer"];
const REQUEST_PROPS: [&[u8]; 5] = [b"query", b"body", b"params", b"headers", b"cookies"];

/// Every tainted DOM sink in `source` (see the module doc for what counts); empty for
/// any language outside the JS/TS family.
pub(crate) fn detect(source: &str, lang: &str) -> Vec<VulnMatch> {
    let Some(syntax) = js_syntax(lang) else {
        return Vec::new();
    };
    if source.len() > MAX_SOURCE_BYTES
        || !SINK_NEEDLES
            .iter()
            .any(|needle| memmem::find(source.as_bytes(), needle).is_some())
    {
        return Vec::new();
    }
    let mask = super::code_regions::code_mask(source, syntax);
    let file = File::new(source.as_bytes(), &mask);
    let assigns = file.assignments();
    let mut sinks = file.sinks();
    sinks.sort_by_key(|sink| sink.at);
    let mut states: HashMap<Option<usize>, RegionState> = HashMap::new();
    let mut found = Vec::new();
    for sink in sinks {
        let region = file.region_of(sink.at);
        let state = states
            .entry(region)
            .or_insert_with(|| RegionState::new(&file, region));
        let in_region = assigns.get(&region).map_or(&[][..], Vec::as_slice);
        state.advance(&file, in_region, sink.at);
        let expr = text(&file.src[sink.expr.0..sink.expr.1]);
        if is_tainted(&expr, &state.tainted, &state.text_content) {
            found.push(VulnMatch::new(
                DOM_XSS.into(),
                (sink.at, sink.at + sink.len),
                DOM_XSS_SEVERITY,
                79,
            ));
        }
    }
    found
}

/// The lexer dialect of `lang`, or `None` outside the JS/TS family.
fn js_syntax(lang: &str) -> Option<&'static str> {
    match lang {
        "javascript" | "js" | "jsx" | "mjs" | "cjs" => Some("javascript"),
        "typescript" | "ts" | "tsx" => Some("typescript"),
        _ => None,
    }
}

// ── byte helpers ─────────────────────────────────────────────────────────────

fn is_ident(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b == b'$'
}

/// A byte a TypeScript type annotation may hold (`Promise<string[]>`, `A | B`).
fn is_type_byte(b: u8) -> bool {
    is_ident(b) || b.is_ascii_whitespace() || b"<>[]|&.,?".contains(&b)
}

fn skip_ws(b: &[u8], mut i: usize) -> usize {
    while b.get(i).is_some_and(u8::is_ascii_whitespace) {
        i += 1;
    }
    i
}

/// The index just past the last non-whitespace byte before `end`.
fn skip_ws_back(b: &[u8], mut end: usize) -> usize {
    while end > 0 && b[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    end
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn trim(mut b: &[u8]) -> &[u8] {
    while let Some((first, rest)) = b.split_first() {
        if !first.is_ascii_whitespace() {
            break;
        }
        b = rest;
    }
    while let Some((last, rest)) = b.split_last() {
        if !last.is_ascii_whitespace() {
            break;
        }
        b = rest;
    }
    b
}

/// Offsets of `word` in `hay` that are not part of a longer identifier.
fn word_hits<'a>(hay: &'a [u8], word: &'a [u8]) -> impl Iterator<Item = usize> + 'a {
    memmem::find_iter(hay, word).filter(move |&at| {
        (at == 0 || !is_ident(hay[at - 1]))
            && hay.get(at + word.len()).is_none_or(|&b| !is_ident(b))
    })
}

/// Partner of every bracket in the (string- and comment-free) mask, `NO_PARTNER` if
/// the bracket never closes; one pass, so the later lookups are O(1).
fn bracket_partners(m: &[u8]) -> Vec<usize> {
    let mut partner = vec![NO_PARTNER; m.len()];
    let mut stack: Vec<usize> = Vec::new();
    for (i, &c) in m.iter().enumerate() {
        match c {
            b'(' | b'[' | b'{' => stack.push(i),
            b')' | b']' | b'}' => {
                if let Some(&open) = stack.last()
                    && matches!((m[open], c), (b'(', b')') | (b'[', b']') | (b'{', b'}'))
                {
                    stack.pop();
                    partner[open] = i;
                    partner[i] = open;
                }
            }
            _ => {}
        }
    }
    partner
}

/// Ranges of `t` between `sep` bytes that sit at bracket depth 0. `angle` also counts
/// `<`/`>` as brackets (TypeScript generics in a parameter list), except the `>` of `=>`.
fn split_top_level(t: &[u8], sep: u8, angle: bool) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut start = 0usize;
    for (i, &c) in t.iter().enumerate() {
        match c {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth -= 1,
            b'<' if angle => depth += 1,
            b'>' if angle && (i == 0 || t[i - 1] != b'=') => depth -= 1,
            _ if c == sep && depth <= 0 => {
                out.push((start, i));
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push((start, t.len()));
    out
}

/// End of the expression that starts at `start`: the first depth-0 `;`, closing
/// bracket, `,` (when `stop_at_comma`) or newline that really ends the statement.
fn expr_end(m: &[u8], start: usize, stop_at_comma: bool) -> usize {
    let limit = start.saturating_add(MAX_EXPR_BYTES).min(m.len());
    let mut depth = 0usize;
    let mut i = start;
    while i < limit {
        match m[i] {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                if depth == 0 {
                    return i;
                }
                depth -= 1;
            }
            b';' if depth == 0 => return i,
            b',' if depth == 0 && stop_at_comma => return i,
            b'\n' if depth == 0 && statement_ends_at(m, i) => return i,
            _ => {}
        }
        i += 1;
    }
    limit
}

/// Whether the newline at `nl` ends the statement (JS automatic semicolon insertion,
/// approximated): it does not when the line ends in an operator or the next one starts
/// with a continuation.
fn statement_ends_at(m: &[u8], nl: usize) -> bool {
    let before = skip_ws_back(m, nl);
    let after = skip_ws(m, nl);
    let ends_open = before > 0 && b"+-*/%&|^?:,=(<>.".contains(&m[before - 1]);
    let continues = m.get(after).is_some_and(|c| b"+-*/%&|^?:.,".contains(c));
    !(ends_open || continues)
}

// ── functions and their parameters ───────────────────────────────────────────

/// One function-like body (`function`, arrow, method) and its parameter names.
struct Func {
    body: (usize, usize),
    params: Vec<String>,
}

/// Names a binding list introduces: `a, { b, c: d = 1 }: T, ...rest` → `a b d rest`.
fn param_names(list: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    for (s, e) in split_top_level(list, b',', true) {
        names_in_binding(&list[s..e], false, &mut out);
    }
    out
}

/// Names one binding element introduces. `in_object` reads `key: value` as a rename
/// (inside a `{ … }` pattern); at the top level a `:` is a TypeScript annotation.
fn names_in_binding(item: &[u8], in_object: bool, out: &mut Vec<String>) {
    let item = trim(item);
    let item = item.strip_prefix(b"...").map_or(item, trim);
    let Some(&first) = item.first() else {
        return;
    };
    if first == b'{' || first == b'[' {
        let inner = bracket_inner(item);
        for (s, e) in split_top_level(inner, b',', true) {
            names_in_binding(&inner[s..e], first == b'{', out);
        }
        return;
    }
    if !is_ident_start(first) {
        return;
    }
    let name_len = item.iter().take_while(|&&b| is_ident(b)).count();
    let name = &item[..name_len];
    let rest = trim(&item[name_len..]);
    if in_object && rest.first() == Some(&b':') {
        names_in_binding(&rest[1..], false, out);
        return;
    }
    let modifier = [
        &b"public"[..],
        b"private",
        b"protected",
        b"readonly",
        b"override",
    ]
    .contains(&name);
    if modifier && rest.first().is_some_and(|&b| is_ident_start(b)) {
        names_in_binding(rest, false, out);
    } else if name != b"this" {
        out.push(text(name));
    }
}

/// The text between the bracket that opens `s` and its partner (to the end if unclosed).
fn bracket_inner(s: &[u8]) -> &[u8] {
    let mut depth = 0usize;
    for (i, &c) in s.iter().enumerate() {
        match c {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return &s[1..i];
                }
            }
            _ => {}
        }
    }
    &s[1..]
}

/// The `{` that opens a function body at `from`, past an optional TypeScript return
/// type (`): Promise<void> {`).
fn body_open(m: &[u8], from: usize) -> Option<usize> {
    let mut i = skip_ws(m, from);
    match m.get(i)? {
        b'{' => Some(i),
        b':' => {
            i += 1;
            while let Some(&c) = m.get(i) {
                match c {
                    b'{' => return Some(i),
                    c if is_type_byte(c) => i += 1,
                    _ => return None,
                }
            }
            None
        }
        _ => None,
    }
}

/// A function whose parameter list opens at `open_paren` and whose braces follow.
fn func_after_params(m: &[u8], partner: &[usize], open_paren: usize) -> Option<Func> {
    let close = partner[open_paren];
    if close == NO_PARTNER {
        return None;
    }
    let open = body_open(m, close + 1)?;
    let end = partner[open];
    (end != NO_PARTNER).then(|| Func {
        body: (open, end),
        params: param_names(&m[open_paren + 1..close]),
    })
}

/// The arrow function whose `=>` sits at `arrow`.
fn arrow_at(m: &[u8], partner: &[usize], arrow: usize) -> Option<Func> {
    let end = skip_ws_back(m, arrow);
    let params = arrow_params(m, partner, end)?;
    let brace = skip_ws(m, arrow + 2);
    let body = if m.get(brace) == Some(&b'{') && partner[brace] != NO_PARTNER {
        (brace, partner[brace])
    } else {
        (arrow + 2, expr_end(m, arrow + 2, true))
    };
    Some(Func { body, params })
}

/// Parameter names of the arrow whose parameter list ends at `end` (just before `=>`):
/// `(a, b) =>`, `x =>`, and `(a): T =>` with a return annotation.
fn arrow_params(m: &[u8], partner: &[usize], end: usize) -> Option<Vec<String>> {
    if end > 0 && m[end - 1] == b')' {
        return list_before(m, partner, end);
    }
    let mut j = end;
    while j > 0 && end - j < MAX_ANNOTATION_BYTES && is_type_byte(m[j - 1]) {
        j -= 1;
    }
    if j > 0 && m[j - 1] == b':' {
        let k = skip_ws_back(m, j - 1);
        if k > 0 && m[k - 1] == b')' {
            return list_before(m, partner, k);
        }
    }
    let start = (0..end).rev().take_while(|&i| is_ident(m[i])).last()?;
    Some(vec![text(&m[start..end])])
}

/// The parameter names of the `( … )` whose closing paren is at `close_end - 1`.
fn list_before(m: &[u8], partner: &[usize], close_end: usize) -> Option<Vec<String>> {
    let open = partner[close_end - 1];
    (open != NO_PARTNER).then(|| param_names(&m[open + 1..close_end - 1]))
}

/// Every function-like body in the mask.
fn find_functions(m: &[u8], partner: &[usize]) -> Vec<Func> {
    let mut out = Vec::new();
    for at in word_hits(m, b"function") {
        let mut i = skip_ws(m, at + b"function".len());
        if m.get(i) == Some(&b'*') {
            i = skip_ws(m, i + 1);
        }
        while m.get(i).is_some_and(|&c| is_ident(c)) {
            i += 1;
        }
        i = skip_ws(m, i);
        if m.get(i) == Some(&b'(') {
            out.extend(func_after_params(m, partner, i));
        }
    }
    out.extend(memmem::find_iter(m, b"=>").filter_map(|at| arrow_at(m, partner, at)));
    // Methods: `name(params) {` — the keyword forms above and control flow excluded.
    for (open, &close) in partner.iter().enumerate() {
        if m[open] != b'(' || close == NO_PARTNER {
            continue;
        }
        let name_end = skip_ws_back(m, open);
        let name_start = (0..name_end).rev().take_while(|&i| is_ident(m[i])).last();
        let Some(name_start) = name_start else {
            continue;
        };
        if !NOT_A_METHOD.contains(&&m[name_start..name_end]) {
            out.extend(func_after_params(m, partner, open));
        }
    }
    out
}

/// Innermost function index per byte (`NO_OWNER` outside every function). Spans nest or
/// are disjoint, so painting the widest first leaves the innermost on top.
fn paint_owners(len: usize, funcs: &[Func]) -> Vec<u32> {
    let mut owner = vec![NO_OWNER; len];
    let mut order: Vec<usize> = (0..funcs.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(funcs[i].body.1.saturating_sub(funcs[i].body.0)));
    for i in order {
        let (start, end) = funcs[i].body;
        let id = u32::try_from(i).unwrap_or(NO_OWNER);
        for slot in &mut owner[start.min(len)..end.min(len)] {
            *slot = id;
        }
    }
    owner
}

// ── the file: sinks and assignments ──────────────────────────────────────────

struct File<'a> {
    src: &'a [u8],
    m: &'a [u8],
    partner: Vec<usize>,
    funcs: Vec<Func>,
    owner: Vec<u32>,
}

/// A DOM sink: the token's offset and length, and the byte range of the HTML expression.
struct Sink {
    at: usize,
    len: usize,
    expr: (usize, usize),
}

impl<'a> File<'a> {
    fn new(src: &'a [u8], m: &'a [u8]) -> Self {
        let partner = bracket_partners(m);
        let funcs = find_functions(m, &partner);
        let owner = paint_owners(m.len(), &funcs);
        Self {
            src,
            m,
            partner,
            funcs,
            owner,
        }
    }

    /// The innermost function containing `at`; `None` is module level.
    fn region_of(&self, at: usize) -> Option<usize> {
        match self.owner.get(at) {
            Some(&id) if id != NO_OWNER => Some(id as usize),
            _ => None,
        }
    }

    /// `(open, close)` of the call whose name ends at `name_end`, if one follows.
    fn call_after(&self, name_end: usize) -> Option<(usize, usize)> {
        let open = skip_ws(self.m, name_end);
        (self.m.get(open) == Some(&b'(') && self.partner[open] != NO_PARTNER)
            .then(|| (open, self.partner[open]))
    }

    fn sinks(&self) -> Vec<Sink> {
        let mut out = Vec::new();
        self.html_assignments(&mut out);
        self.adjacent_html(&mut out);
        self.document_writes(&mut out);
        self.react_inner_html(&mut out);
        out
    }

    /// `x.innerHTML = e` / `x.outerHTML = e` / `+=`.
    fn html_assignments(&self, out: &mut Vec<Sink>) {
        let m = self.m;
        for name in [&b"innerHTML"[..], b"outerHTML"] {
            for at in word_hits(m, name) {
                if at == 0 || m[at - 1] != b'.' {
                    continue;
                }
                let after = skip_ws(m, at + name.len());
                let value_at = match (m.get(after), m.get(after + 1)) {
                    (Some(b'='), next) if next != Some(&b'=') => after + 1,
                    (Some(b'+'), Some(b'=')) => after + 2,
                    _ => continue,
                };
                // No whitespace skip: the mask blanks a leading string literal, and
                // skipping "whitespace" there would jump over the whole expression.
                out.push(Sink {
                    at,
                    len: name.len(),
                    expr: (value_at, expr_end(m, value_at, true)),
                });
            }
        }
    }

    /// `x.insertAdjacentHTML(position, e)` — only the 2nd argument is HTML.
    fn adjacent_html(&self, out: &mut Vec<Sink>) {
        let name = b"insertAdjacentHTML";
        for at in word_hits(self.m, name) {
            if at == 0 || self.m[at - 1] != b'.' {
                continue;
            }
            let Some((open, close)) = self.call_after(at + name.len()) else {
                continue;
            };
            let args = split_top_level(&self.m[open + 1..close], b',', false);
            if let Some(&(s, e)) = args.get(1) {
                out.push(Sink {
                    at,
                    len: name.len(),
                    expr: (open + 1 + s, open + 1 + e),
                });
            }
        }
    }

    /// `document.write(e)` / `document.writeln(e)` — every argument is written.
    fn document_writes(&self, out: &mut Vec<Sink>) {
        for name in [&b"document.write"[..], b"document.writeln"] {
            for at in memmem::find_iter(self.m, name) {
                if at > 0 && is_ident(self.m[at - 1]) {
                    continue;
                }
                if let Some((open, close)) = self.call_after(at + name.len()) {
                    out.push(Sink {
                        at,
                        len: name.len(),
                        expr: (open + 1, close),
                    });
                }
            }
        }
    }

    /// `dangerouslySetInnerHTML={{ __html: e }}` (JSX) and `dangerouslySetInnerHTML: { __html: e }`.
    fn react_inner_html(&self, out: &mut Vec<Sink>) {
        let (m, name) = (self.m, b"dangerouslySetInnerHTML");
        for at in word_hits(m, name) {
            let sep_at = skip_ws(m, at + name.len());
            let Some(&sep) = m.get(sep_at).filter(|&&c| c == b'=' || c == b':') else {
                continue;
            };
            if let Some(expr) = self.react_value(sep, sep_at + 1) {
                out.push(Sink {
                    at,
                    len: name.len(),
                    expr,
                });
            }
        }
    }

    /// The HTML expression of a `dangerouslySetInnerHTML` whose value starts at `from`
    /// (just past its `=` or `:`).
    fn react_value(&self, sep: u8, from: usize) -> Option<(usize, usize)> {
        let m = self.m;
        let value = skip_ws(m, from);
        let is_brace = m.get(value) == Some(&b'{') && self.partner[value] != NO_PARTNER;
        if sep == b':' {
            return Some(if is_brace {
                self.html_in_object(value)
            } else {
                (from, expr_end(m, from, true))
            });
        }
        // JSX attribute: `={ expression }` — the expression is the object literal or a variable.
        if !is_brace {
            return None;
        }
        let inner = skip_ws(m, value + 1);
        if m.get(inner) == Some(&b'{') && self.partner[inner] != NO_PARTNER {
            return Some(self.html_in_object(inner));
        }
        Some((value + 1, self.partner[value]))
    }

    /// The `__html` value of the object literal that opens at `open` (the whole object
    /// when it has no such key — a spread, say).
    fn html_in_object(&self, open: usize) -> (usize, usize) {
        let (m, close) = (self.m, self.partner[open]);
        let inside = &m[open + 1..close];
        match word_hits(inside, b"__html").next() {
            Some(key) => {
                let after = skip_ws(m, open + 1 + key + b"__html".len());
                if m.get(after) == Some(&b':') {
                    (after + 1, expr_end(m, after + 1, true).min(close))
                } else {
                    (open + 1 + key, open + 1 + key + b"__html".len())
                }
            }
            None => (open + 1, close),
        }
    }

    /// Every assignment in the file, grouped by the function that owns it, in order.
    fn assignments(&self) -> HashMap<Option<usize>, Vec<Assign>> {
        let mut by_region: HashMap<Option<usize>, Vec<Assign>> = HashMap::new();
        for at in memchr_iter(b'=', self.m) {
            if let Some(assign) = self.assignment_at(at) {
                by_region
                    .entry(self.region_of(at))
                    .or_default()
                    .push(assign);
            }
        }
        by_region
    }

    /// The assignment whose `=` sits at `eq`, or `None` for a comparison, an arrow,
    /// a compound operator that cannot carry a string, or an unreadable target.
    fn assignment_at(&self, eq: usize) -> Option<Assign> {
        let m = self.m;
        let (lhs_end, compound) = assignment_operator(m, eq)?;
        let target = self.lhs_target(skip_ws_back(m, lhs_end))?;
        let (kind, names) = classify(target, compound)?;
        // The right-hand side starts right after `=`: the mask blanks a leading string
        // literal, so skipping whitespace there would jump over the whole expression.
        Some(Assign {
            at: eq,
            kind,
            names,
            rhs: (eq + 1, expr_end(m, eq + 1, true)),
        })
    }

    /// What the left-hand side that ends at `k` assigns to.
    fn lhs_target(&self, k: usize) -> Option<Target> {
        let m = self.m;
        let last = *m.get(k.checked_sub(1)?)?;
        if last == b'}' || last == b']' {
            let open = self.partner[k - 1];
            if open == NO_PARTNER {
                return None;
            }
            let before = skip_ws_back(m, open);
            let indexes = last == b']'
                && before > 0
                && (is_ident(m[before - 1]) || matches!(m[before - 1], b')' | b']'));
            if indexes {
                return self.member_chain(before, Some(String::new()));
            }
            let mut names = Vec::new();
            names_in_binding(&m[open..k], false, &mut names);
            return Some(Target::Pattern(names));
        }
        if !is_ident(last) {
            return None;
        }
        self.typed_declaration(k)
            .or_else(|| self.member_chain(k, None))
    }

    /// `const x: T = …` — the identifier before `=` is the type; the name precedes the colon.
    fn typed_declaration(&self, k: usize) -> Option<Target> {
        let m = self.m;
        let mut j = k;
        while j > 0 && k - j < MAX_ANNOTATION_BYTES && is_type_byte(m[j - 1]) {
            j -= 1;
        }
        if j == 0 || m[j - 1] != b':' {
            return None;
        }
        let name_end = skip_ws_back(m, j - 1);
        let name_start = (0..name_end).rev().take_while(|&i| is_ident(m[i])).last()?;
        let keyword_end = skip_ws_back(m, name_start);
        let keyword_start = (0..keyword_end)
            .rev()
            .take_while(|&i| is_ident(m[i]))
            .last()?;
        [&b"const"[..], b"let", b"var"]
            .contains(&&m[keyword_start..keyword_end])
            .then(|| Target::Plain(text(&m[name_start..name_end])))
    }

    /// Walk `a.b[c].d` backwards from `k`: the plain name, or the root and last segment.
    /// `last` pre-seeds the last segment when the walk starts past an index (`a[i] =`).
    fn member_chain(&self, mut k: usize, mut last: Option<String>) -> Option<Target> {
        let m = self.m;
        let mut segments = usize::from(last.is_some());
        loop {
            k = skip_ws_back(m, k);
            let c = *m.get(k.checked_sub(1)?)?;
            if c == b']' || c == b')' {
                k = self.group_open(k)?;
                segments += 1;
                last.get_or_insert_with(String::new);
                continue;
            }
            let start = ident_start_before(m, k)?;
            let name = text(&m[start..k]);
            segments += 1;
            last.get_or_insert_with(|| name.clone());
            if let Some(dot) = dot_before(m, start) {
                k = dot;
                continue;
            }
            return Some(if segments == 1 {
                Target::Plain(name)
            } else {
                Target::Member {
                    root: name,
                    last: last.unwrap_or_default(),
                }
            });
        }
    }

    /// The opening bracket of the group that closes at `k - 1`.
    fn group_open(&self, k: usize) -> Option<usize> {
        let open = self.partner[k - 1];
        (open != NO_PARTNER).then_some(open)
    }
}

/// Start of the identifier that ends at `end`, `None` when no identifier does.
fn ident_start_before(m: &[u8], end: usize) -> Option<usize> {
    (0..end).rev().take_while(|&i| is_ident(m[i])).last()
}

/// Where the walk resumes after a `.` (or `?.`) that sits before `start`.
fn dot_before(m: &[u8], start: usize) -> Option<usize> {
    let before = skip_ws_back(m, start);
    if before == 0 || m[before - 1] != b'.' {
        return None;
    }
    let dot = before - 1;
    Some(if dot > 0 && m[dot - 1] == b'?' {
        dot - 1
    } else {
        dot
    })
}

/// `(end of the left-hand side, is it a compound operator)` for the `=` at `eq`; `None`
/// when that `=` belongs to a comparison, an arrow or an arithmetic compound operator.
/// `+= |= ||= ??= &&=` can only ADD taint, so they count as compound.
fn assignment_operator(m: &[u8], eq: usize) -> Option<(usize, bool)> {
    if matches!(m.get(eq + 1), Some(b'=' | b'>')) || (eq > 0 && b"=!<>-*/%^".contains(&m[eq - 1])) {
        return None;
    }
    let mut lhs_end = eq;
    while lhs_end > 0 && b"+|?&".contains(&m[lhs_end - 1]) {
        lhs_end -= 1;
    }
    Some((lhs_end, lhs_end != eq))
}

/// How a target moves taint, and the names it moves it to.
fn classify(target: Target, compound: bool) -> Option<(Kind, Vec<String>)> {
    Some(match target {
        Target::Plain(name) => (if compound { Kind::Weak } else { Kind::Strong }, vec![name]),
        Target::Pattern(names) => (Kind::Strong, names),
        Target::Member { root, last } => match last.as_str() {
            // The sink itself is judged where it stands; it spreads nothing.
            "innerHTML" | "outerHTML" => return None,
            "textContent" | "innerText" => (Kind::TextContent, vec![root]),
            _ => (Kind::Weak, vec![root]),
        },
    })
}

/// What an assignment writes to.
enum Target {
    Plain(String),
    Pattern(Vec<String>),
    Member { root: String, last: String },
}

/// How an assignment moves taint.
enum Kind {
    /// `x = e`: the name becomes exactly as tainted as `e`.
    Strong,
    /// `x += e`, `o.k = e`: only ADDS taint.
    Weak,
    /// `n.textContent = e`: `n` now holds escaped text, whatever `e` was.
    TextContent,
}

struct Assign {
    at: usize,
    kind: Kind,
    names: Vec<String>,
    rhs: (usize, usize),
}

/// The taint state of one function (or module level) as the sweep reaches each sink.
struct RegionState {
    next: usize,
    tainted: HashSet<String>,
    text_content: HashSet<String>,
}

impl RegionState {
    /// Parameters of the function are tainted from the start.
    fn new(file: &File<'_>, region: Option<usize>) -> Self {
        Self {
            next: 0,
            tainted: region
                .map(|i| file.funcs[i].params.iter().cloned().collect())
                .unwrap_or_default(),
            text_content: HashSet::new(),
        }
    }

    /// Apply every assignment of the region that sits before `upto`, in order.
    fn advance(&mut self, file: &File<'_>, assigns: &[Assign], upto: usize) {
        while let Some(assign) = assigns.get(self.next).filter(|a| a.at < upto) {
            self.next += 1;
            if matches!(assign.kind, Kind::TextContent) {
                self.text_content.extend(assign.names.iter().cloned());
                continue;
            }
            let rhs = text(&file.src[assign.rhs.0..assign.rhs.1]);
            let tainted = is_tainted(&rhs, &self.tainted, &self.text_content);
            for name in &assign.names {
                if tainted {
                    self.tainted.insert(name.clone());
                } else if matches!(assign.kind, Kind::Strong) {
                    self.tainted.remove(name);
                }
            }
        }
    }
}

// ── taint of one expression ──────────────────────────────────────────────────

/// Whether the expression `expr` carries untrusted data given the `tainted` names.
fn is_tainted(expr: &str, tainted: &HashSet<String>, text_content: &HashSet<String>) -> bool {
    let flat = strip_text_content_reads(
        strip_neutralized_calls(flatten(expr.as_bytes())),
        text_content,
    );
    has_direct_source(&flat) || mentions_tainted(&flat, tainted)
}

/// The expression as code only: comments and string literals become a space, a template
/// literal keeps just its `${…}` holes (a template without one flattens to nothing).
fn flatten(b: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(b.len());
    flatten_code(b, &mut 0, &mut out, false, 0);
    out
}

fn flatten_code(b: &[u8], i: &mut usize, out: &mut Vec<u8>, in_hole: bool, nest: u8) {
    let mut depth = 0usize;
    while *i < b.len() {
        let c = b[*i];
        match c {
            b'/' if b.get(*i + 1) == Some(&b'/') => {
                while *i < b.len() && b[*i] != b'\n' {
                    *i += 1;
                }
                out.push(b' ');
            }
            b'/' if b.get(*i + 1) == Some(&b'*') => {
                *i += 2;
                while *i < b.len() && !b[*i..].starts_with(b"*/") {
                    *i += 1;
                }
                *i = (*i + 2).min(b.len());
                out.push(b' ');
            }
            b'\'' | b'"' => {
                skip_quoted(b, i, c);
                out.push(b' ');
            }
            b'`' => {
                *i += 1;
                flatten_template(b, i, out, nest);
                out.push(b' ');
            }
            b'{' if in_hole => {
                depth += 1;
                out.push(c);
                *i += 1;
            }
            b'}' if in_hole => {
                *i += 1;
                if depth == 0 {
                    return;
                }
                depth -= 1;
                out.push(c);
            }
            _ => {
                out.push(c);
                *i += 1;
            }
        }
    }
}

/// Past a single-line string literal opened at `*i` with quote `q`.
fn skip_quoted(b: &[u8], i: &mut usize, q: u8) {
    *i += 1;
    while *i < b.len() {
        match b[*i] {
            b'\\' => *i += 2,
            c if c == q => {
                *i += 1;
                return;
            }
            b'\n' => return,
            _ => *i += 1,
        }
    }
}

/// Past a template literal (after its opening backtick), emitting only its holes.
fn flatten_template(b: &[u8], i: &mut usize, out: &mut Vec<u8>, nest: u8) {
    while *i < b.len() {
        match b[*i] {
            b'\\' => *i += 2,
            b'`' => {
                *i += 1;
                return;
            }
            b'$' if b.get(*i + 1) == Some(&b'{') && nest < MAX_TEMPLATE_NEST => {
                *i += 2;
                out.push(b' ');
                flatten_code(b, i, out, true, nest + 1);
                out.push(b' ');
            }
            _ => *i += 1,
        }
    }
}

/// Whether a call named `name` (its last path segment, any case) launders its argument:
/// a sanitizer / escaper (NOT the inverse `unescape`, NOT regex-escaping) or a numeric coercion.
fn neutralizes(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if matches!(
        lower.as_str(),
        "number" | "parseint" | "parsefloat" | "boolean"
    ) {
        return true;
    }
    let names_a_filter = ["sanitize", "sanitise", "escape"]
        .iter()
        .any(|k| lower.contains(k));
    names_a_filter && !lower.contains("unescape") && !lower.contains("regex")
}

/// The first neutralizing call at or after `from`: `(path start, end past its ")")`.
fn find_neutralized_call(flat: &[u8], from: usize) -> Option<(usize, usize)> {
    for open in memchr_iter(b'(', &flat[from..]).map(|p| p + from) {
        let name_end = skip_ws_back(flat, open);
        let name_start = (0..name_end)
            .rev()
            .take_while(|&i| is_ident(flat[i]))
            .last();
        let Some(name_start) = name_start else {
            continue;
        };
        if !neutralizes(&text(&flat[name_start..name_end])) {
            continue;
        }
        let mut path_start = name_start;
        while path_start > 0 && (is_ident(flat[path_start - 1]) || flat[path_start - 1] == b'.') {
            path_start -= 1;
        }
        let mut depth = 0usize;
        let mut end = flat.len();
        for (i, &c) in flat.iter().enumerate().skip(open) {
            match c {
                b'(' => depth += 1,
                b')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = i + 1;
                        break;
                    }
                }
                _ => {}
            }
        }
        return Some((path_start, end));
    }
    None
}

/// `flat` without any sanitizer / escaper / numeric-coercion call (arguments included).
fn strip_neutralized_calls(mut flat: Vec<u8>) -> Vec<u8> {
    let mut from = 0;
    while let Some((start, end)) = find_neutralized_call(&flat, from) {
        let _ = flat.splice(start..end, *b" ");
        from = start;
    }
    flat
}

/// `flat` without `n.innerHTML` reads for every `n` that had `n.textContent = …` — the
/// browser escaped it on the way in.
fn strip_text_content_reads(mut flat: Vec<u8>, text_content: &HashSet<String>) -> Vec<u8> {
    for node in text_content {
        let needle = format!("{node}.innerHTML");
        loop {
            let hit = word_hits(&flat, needle.as_bytes()).next();
            let Some(at) = hit else {
                break;
            };
            let _ = flat.splice(at..at + needle.len(), *b" ");
        }
    }
    flat
}

/// Whether `flat` names a source that is untrusted by itself (no variable involved).
fn has_direct_source(flat: &[u8]) -> bool {
    member_source(flat, b"location", &LOCATION_PROPS)
        || member_source(flat, b"document", &DOCUMENT_PROPS)
        || member_source(flat, b"req", &REQUEST_PROPS)
        || member_source(flat, b"request", &REQUEST_PROPS)
        || member_source(flat, b"event", &[b"data"])
        || word_hits(flat, b"URLSearchParams").next().is_some()
        || window_name(flat)
        || fetch_call(flat)
        || memmem::find(flat, b".responseText").is_some()
}

/// `root.prop` with `prop` among `props`, `root` not the tail of a longer identifier.
fn member_source(flat: &[u8], root: &[u8], props: &[&[u8]]) -> bool {
    memmem::find_iter(flat, root).any(|at| {
        let after = at + root.len();
        let boundary = at == 0 || !is_ident(flat[at - 1]);
        let prop_start = after + 1;
        boundary && flat.get(after) == Some(&b'.') && props.contains(&ident_at(flat, prop_start))
    })
}

fn ident_at(b: &[u8], start: usize) -> &[u8] {
    let rest = b.get(start..).unwrap_or_default();
    &rest[..rest.iter().take_while(|&&c| is_ident(c)).count()]
}

fn window_name(flat: &[u8]) -> bool {
    word_hits(flat, b"window.name").next().is_some()
}

/// A call to `fetch` — a bare `fetch(` or `window.fetch(`, not `prefetch(` or `x.fetch(`.
fn fetch_call(flat: &[u8]) -> bool {
    word_hits(flat, b"fetch").any(|at| {
        let method = at > 0 && flat[at - 1] == b'.' && !flat[..at].ends_with(b"window.");
        !method && flat.get(skip_ws(flat, at + 5)) == Some(&b'(')
    })
}

/// Whether any identifier of `flat` that is a variable (not a `.property`, not an
/// object-literal key) is among the `tainted` names.
fn mentions_tainted(flat: &[u8], tainted: &HashSet<String>) -> bool {
    if tainted.is_empty() {
        return false;
    }
    let mut i = 0;
    while i < flat.len() {
        if !is_ident(flat[i]) || (i > 0 && is_ident(flat[i - 1])) {
            i += 1;
            continue;
        }
        let end = i + flat[i..].iter().take_while(|&&c| is_ident(c)).count();
        if is_ident_start(flat[i])
            && !is_member_or_key(flat, i, end)
            && tainted.contains(&text(&flat[i..end]))
        {
            return true;
        }
        i = end;
    }
    false
}

/// `x` in `.x` / `?.x` (a property) or in `{ x: …` / `, x: …` (an object key); a spread
/// `...x` is a variable.
fn is_member_or_key(flat: &[u8], start: usize, end: usize) -> bool {
    let before = skip_ws_back(flat, start);
    let property = before > 0 && flat[before - 1] == b'.' && !flat[..before].ends_with(b"...");
    let after = skip_ws(flat, end);
    let key =
        flat.get(after) == Some(&b':') && before > 0 && matches!(flat[before - 1], b'{' | b',');
    property || key
}

#[cfg(test)]
mod tests {
    use super::*;

    fn js(src: &str) -> usize {
        detect(src, "javascript").len()
    }

    fn ts(src: &str) -> usize {
        detect(src, "typescript").len()
    }

    #[test]
    fn every_source_reaches_every_kind_of_sink() {
        let cases = [
            // a function parameter
            "function r(el, html) { el.innerHTML = html; }",
            "const r = (el, html) => { el.outerHTML = html; };",
            "function r(box, html) { box.insertAdjacentHTML('beforeend', html); }",
            "function r(html) { document.write(html); }",
            "function r(html) { document.writeln('<p>' + html + '</p>'); }",
            // location.*
            "el.innerHTML = location.hash;",
            "el.innerHTML = window.location.search.slice(1);",
            "el.outerHTML = '<a>' + document.location.href + '</a>';",
            // document.*
            "el.innerHTML = document.URL;",
            "el.insertAdjacentHTML('afterend', document.cookie);",
            "document.write(document.referrer);",
            // window.name, event.data
            "el.innerHTML = window.name;",
            "window.addEventListener('message', function (event) { box.innerHTML = event.data; });",
            "box.innerHTML = event.data;",
            // fetch(...).then results and a response read
            "fetch(u).then(r => r.json()).then(d => { el.innerHTML = d.html; });",
            "async function f(u) { const res = await fetch(u); const j = await res.json(); el.innerHTML = j.html; }",
            "xhr.onload = function () { el.innerHTML = xhr.responseText; };",
            // URLSearchParams and req.*
            "const p = new URLSearchParams(raw); const n = p.get('n'); el.innerHTML = `<h1>${n}</h1>`;",
            "el.innerHTML = req.query.name;",
            "el.innerHTML = request.body.html;",
            // React, JSX and the object form
            "function Post({ body }) { return <div dangerouslySetInnerHTML={{ __html: body }} />; }",
            "const Post = (props) => <p dangerouslySetInnerHTML={{ __html: props.html }} />;",
            "const mk = (h) => React.createElement('p', { dangerouslySetInnerHTML: { __html: h } });",
            // propagation: chains, +=, destructuring, member assignment
            "function f(a) { const b = a.trim(); let c = '<i>' + b; el.innerHTML = c; }",
            "function f(items) { let html = ''; html += items[0]; el.innerHTML = html; }",
            "function f(props) { const { label } = props; el.innerHTML = label; }",
            "function f(a) { const o = {}; o.html = a; el.innerHTML = o.html; }",
            "function f(a) { el.innerHTML = a ? a : ''; }",
            // methods and arrow bodies
            "class V { render(data) { this.el.innerHTML = data; } }",
            "const set = (x) => el.innerHTML = x;",
            // a multi-line expression
            "function f(a) {\n  el.innerHTML = '<b>'\n    + a\n    + '</b>';\n}",
            // `+=` onto the sink itself
            "function f(a) { el.innerHTML += a; }",
            // a lookalike of a sanitizer is not one
            "function f(x) { el.innerHTML = unescape(x); }",
            "function f(x) { el.innerHTML = escapeRegExp(x); }",
        ];
        for src in cases {
            assert_eq!(js(src), 1, "{src}");
        }
    }

    #[test]
    fn typescript_annotations_do_not_hide_the_parameters() {
        for src in [
            "function r(el: HTMLElement, html: string): void { el.innerHTML = html; }",
            "const r = (el: HTMLElement, html: string): void => { el.innerHTML = html; };",
            "class V { render(data: Record<string, string>): void { this.el.innerHTML = data.x; } }",
            "function r({ html }: { html: string }) { el.innerHTML = html; }",
            "function f(a: string) { const b: string = a; el.innerHTML = b; }",
        ] {
            assert_eq!(ts(src), 1, "{src}");
        }
    }

    #[test]
    fn a_sanitizer_in_the_flow_clears_the_taint() {
        for src in [
            "function f(html) { el.innerHTML = DOMPurify.sanitize(html); }",
            "function f(html) { el.innerHTML = sanitizeHtml(html); }",
            "function f(html) { el.innerHTML = escapeHtml(html); }",
            "function f(html) { el.innerHTML = _.escape(html); }",
            "function f(name) { el.innerHTML = `<p>${escapeHtml(name)}</p>`; }",
            "function f(a, b) { el.innerHTML = '<b>' + sanitize(a) + sanitize(b) + '</b>'; }",
            "function f(html) { html = DOMPurify.sanitize(html); el.innerHTML = html; }",
            "function f(html) { const clean = DOMPurify.sanitize(html); el.innerHTML = clean; }",
            "function f(n) { el.innerHTML = Number(n); }",
            "function f(n) { el.innerHTML = '<i>' + parseInt(n, 10) + '</i>'; }",
            "el.innerHTML = DOMPurify.sanitize(location.hash);",
            "box.insertAdjacentHTML('beforeend', DOMPurify.sanitize(document.cookie));",
            "function P({ body }) { return <div dangerouslySetInnerHTML={{ __html: DOMPurify.sanitize(body) }} />; }",
            // the textContent path: the sink is not HTML, and the round trip escapes
            "function f(html) { el.textContent = html; }",
            "function f(t, s) { t.textContent = s; el.innerHTML = t.innerHTML; }",
            "function f(s) { const t = document.createElement('div'); t.textContent = s; el.innerHTML = t.innerHTML; }",
        ] {
            assert_eq!(js(src), 0, "{src}");
        }
    }

    #[test]
    fn literals_and_constants_are_safe() {
        for src in [
            "el.innerHTML = '<b>hi</b>';",
            "el.innerHTML = \"<b>hi</b>\";",
            "el.innerHTML = `<b>hi</b>`;",
            "el.innerHTML = '';",
            "el.innerHTML = '<b>' + 'x' + '</b>';",
            "const T = '<li>'; el.innerHTML = T + 'x';",
            "function f() { el.innerHTML = '<p>fixed</p>'; }",
            "function f(el) { el.innerHTML = ''; }",
            "function f(h) { h = '<b>'; el.innerHTML = h; }",
            "function f(h) { const local = '<hr>'; el.innerHTML = local; }",
            "el.insertAdjacentHTML('beforeend', '<hr>');",
            "document.write('<p>fixed</p>');",
            "function P() { return <div dangerouslySetInnerHTML={{ __html: '<b>x</b>' }} />; }",
            // the position argument of insertAdjacentHTML is not HTML
            "function f(pos) { el.insertAdjacentHTML(pos, '<hr>'); }",
        ] {
            assert_eq!(js(src), 0, "{src}");
        }
    }

    #[test]
    fn quoted_reads_and_lookalikes_are_not_sinks() {
        for src in [
            "// el.innerHTML = location.hash;",
            "/* el.innerHTML = location.hash; */ var a = 1;",
            "const s = \"el.innerHTML = location.hash\";",
            "const s = `el.innerHTML = ${location.hash}`;",
            "const h = el.innerHTML;",
            "if (el.innerHTML == location.hash) { go(); }",
            "if (el.innerHTML === location.hash) { go(); }",
            "el.myinnerHTML = location.hash;",
            "el.innerHTMLCache = location.hash;",
            "innerHTML = location.hash;",
            "const x = window.location.hash;",
            "el.textContent = location.hash;",
            "el.setAttribute('title', location.hash);",
        ] {
            assert_eq!(js(src), 0, "{src}");
        }
    }

    #[test]
    fn only_the_js_family_is_read() {
        let src = "el.innerHTML = location.hash;";
        assert_eq!(detect(src, "javascript").len(), 1);
        assert_eq!(detect(src, "jsx").len(), 1);
        assert_eq!(detect(src, "tsx").len(), 1);
        for lang in ["python", "rust", "go", "shell", "html", "java", ""] {
            assert!(detect(src, lang).is_empty(), "{lang}");
        }
    }

    /// NOT detected, asserting today's behaviour: the arm is intra-function and
    /// intra-file. Each case is a real DOM XSS the plan lists as a known false negative.
    #[test]
    fn cross_function_taint_is_documented_as_not_detected() {
        for src in [
            // a call result: `getInput` returns the source, the sink function sees a call
            "function getInput() { return location.hash; }\nfunction show(el) { el.innerHTML = getInput(); }",
            // a module-level variable read from inside a function
            "const q = location.search;\nfunction show(el) { el.innerHTML = q; }",
            // a closure over the outer function's variable
            "function outer() { const q = location.hash; return () => { el.innerHTML = q; }; }",
            // a value that crossed a call before it was stored
            "function readHash() { return location.hash; }\nconst h = readHash();\nel.innerHTML = h;",
            // bracket access
            "el['innerHTML'] = location.hash;",
        ] {
            assert_eq!(js(src), 0, "now a known false negative: {src}");
        }
    }

    #[test]
    fn the_span_is_the_sink_token_and_each_sink_reports_once() {
        let src = "function f(a) {\n  one.innerHTML = a;\n  two.innerHTML = a;\n}\n";
        let found = detect(src, "javascript");
        assert_eq!(found.len(), 2);
        for m in &found {
            assert_eq!(&src[m.span.0..m.span.1], "innerHTML");
            assert_eq!((m.pattern_name.as_str(), m.cwe_id), (DOM_XSS, 79));
        }
        assert!(found[0].span.0 < found[1].span.0);
    }

    #[test]
    fn a_sink_belongs_to_its_innermost_function_only() {
        // The outer parameter is a closure capture for the arrow: not followed.
        let outer_param = "function f(a) { return () => { el.innerHTML = a; }; }";
        assert_eq!(js(outer_param), 0);
        // The arrow's own parameter is.
        let own_param = "function f(a) { return (b) => { el.innerHTML = b; }; }";
        assert_eq!(js(own_param), 1);
        // A clean outer sink is not tainted by a tainted inner one.
        let separate =
            "function f(a) { const g = (b) => { x.innerHTML = b; }; y.innerHTML = 'ok'; }";
        assert_eq!(js(separate), 1);
    }

    #[test]
    fn control_flow_parentheses_are_not_methods() {
        // `if (a) {` must not become a function whose parameter list is `a`: here `a` is
        // NOT a parameter of anything, so the sink is clean.
        for src in [
            "function f() { if (a) { el.innerHTML = a; } }",
            "function f() { while (a) { el.innerHTML = a; } }",
            "function f() { for (k of a) { el.innerHTML = k; } }",
            "function f() { switch (a) { default: el.innerHTML = a; } }",
            "function f() { try { go(); } catch (a) { el.innerHTML = a; } }",
        ] {
            assert_eq!(js(src), 0, "{src}");
        }
        let src = "function f(a) { while (a) { el.innerHTML = a; } }";
        assert_eq!(js(src), 1, "`a` is still the parameter of f");
    }

    #[test]
    fn a_file_without_a_sink_needle_is_never_lexed() {
        assert!(detect("const a = location.hash; render(a);", "javascript").is_empty());
        assert!(detect("", "javascript").is_empty());
    }

    #[test]
    fn param_names_read_defaults_destructuring_modifiers_and_annotations() {
        let names = |s: &str| param_names(s.as_bytes());
        assert_eq!(names("a, b"), ["a", "b"]);
        assert_eq!(names("a = 1, b = f(x, y)"), ["a", "b"]);
        assert_eq!(
            names("{ a, b: c, d = 1 }, [e, f]"),
            ["a", "c", "d", "e", "f"]
        );
        assert_eq!(names("...rest: string[]"), ["rest"]);
        assert_eq!(
            names("private readonly x: Map<string, number>, y?: T"),
            ["x", "y"]
        );
        assert_eq!(names("this: Window, cb = () => 1"), ["cb"]);
        assert!(names("").is_empty());
    }
}
