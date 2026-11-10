//! Positive controls for the F2.1 precision relaxations reviewed in task 4b.
// touring-quality:allow-attack-fixture
// touring-quality:allow-secrets

use touring_analysis::quality::SecurityAnalyzer;

fn assert_detected(pattern: &str, lang: &str, cases: &[&str]) {
    let missed: Vec<_> = cases
        .iter()
        .filter(|source| {
            !SecurityAnalyzer::new()
                .analyze(source, lang)
                .vuln_matches
                .iter()
                .any(|m| m.pattern_name == pattern)
        })
        .collect();
    assert!(missed.is_empty(), "{pattern} missed {missed:#?}");
}

fn assert_not_detected(pattern: &str, lang: &str, cases: &[&str]) {
    let hits: Vec<_> = cases
        .iter()
        .filter(|source| {
            SecurityAnalyzer::new()
                .analyze(source, lang)
                .vuln_matches
                .iter()
                .any(|m| m.pattern_name == pattern)
        })
        .collect();
    assert!(hits.is_empty(), "{pattern} false positives {hits:#?}");
}

/// 0abafc64: a constant module specifier is exempt; input-built paths are not.
#[test]
fn commit_0abafc64_module_specifier_neighbors() {
    assert_detected(
        "PathTraversal",
        "typescript",
        &[
            "const m = require(\"../../\" + userInput);\n",
            "const m = await import(`../../plugins/${name}.js`);\n",
            "const s = fs.readFileSync(\"../../etc/passwd\", \"utf8\");\n",
            "const m = require.resolve(\"../../etc/passwd\");\n",
        ],
    );
}

/// 0abafc64: a constant <script> tag is exempt; runtime formatting is not.
#[test]
fn commit_0abafc64_script_tag_neighbors() {
    assert_detected(
        "XSS",
        "python",
        &[
            "def page(user):\n    return f\"<script>var u = '{user}';</script>\"\n",
            "def page(user):\n    return \"<script>var u = '\" + user + \"';</script>\"\n",
            "def page(user):\n    return \"<script>var u = '{}';</script>\".format(user)\n",
        ],
    );
}

/// Regression: percent formatting reaches the script sink at runtime.
#[test]
fn regression_0abafc64_percent_format_script_sink() {
    assert_detected(
        "XSS",
        "python",
        &[
            "def page(user):\n    return \"<script>var u = '%s';</script>\" % user\n",
            "def page(user):\n    return \"<script>var u = '%s';</script>\"%user\n",
            "def page(user):\n    return \"\"\"<script>var u = '%s';</script>\"\"\" % (user,)\n",
        ],
    );
}

/// A percent sign used for arithmetic or prose is not a formatting operator.
#[test]
fn constant_script_with_percent_remains_exempt() {
    assert_not_detected(
        "XSS",
        "html",
        &[
            "<html><script>var remainder = 100 % 3;</script></html>\n",
            "<html><script>var label = '100%';</script></html>\n",
        ],
    );
}

/// Regression: an HTML template is not a static HTML document.
#[test]
fn regression_0abafc64_interpolated_html_template_script() {
    assert_detected(
        "XSS",
        "html",
        &[
            "<html><script>var u = '{{ user }}';</script></html>\n",
            "<html>\n<script>\nvar u = '{{ user }}';\n</script>\n</html>\n",
        ],
    );
}

/// Runtime output AFTER the closing script tag is outside its sink.
#[test]
fn constant_html_script_before_template_output_remains_exempt() {
    assert_not_detected(
        "XSS",
        "html",
        &["<html><script>var a = 1;</script>{{ user }}</html>\n"],
    );
}

/// 461cfa77: the literal must be the whole module specifier.
#[test]
fn commit_461cfa77_whole_specifier_neighbors() {
    assert_detected(
        "PathTraversal",
        "typescript",
        &[
            "const m = require(\"../../\" + userInput);\n",
            "const m = await import(\"../../\" + name);\n",
            "const m = require('../../' + userInput);\n",
            "import x from \"../../\" + y;\n",
            "const m = require(\"../../etc/passwd\", options);\n",
        ],
    );
}

/// 49fb4552: only bare SCREAMING_SNAKE names are treated as constants.
#[test]
fn commit_49fb4552_spliced_constant_neighbors() {
    assert_detected(
        "XSS",
        "python",
        &[
            "tag = f'<script src=\"{GSAP_CDN_URL}\"{sri_attr}></script>'\n",
            "tag = '<script src=\"' + url + '\"></script>'\n",
            "tag = '<script src=\"' + CDN.url + '\"></script>'\n",
            "tag = f'<script src=\"{URL()}\"></script>'\n",
            "tag = '<script src=\"{u}\">'.format(u=URL)\n",
            "tag = '<script src=\"' + CDN_URL + user_input + '\"></script>'\n",
        ],
    );
}
