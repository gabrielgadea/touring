//! The rows of the gate scenario table (see `gate_scenarios`). Each row names the old tests it
//! subsumes in `covers`; the sequences, the sessions and the expectations are the ones those
//! tests drove — restated as data against the public entry point.

use super::scenarios::*;
use serde_json::json;

const MARK: &str = "[CODE MODE · rajada]";
const BYPASS: &str = "TOURING_GATE_OK=1";
const RETOMADA: &str = "docs/plans/2026-10-06-geopolitica-logistica-eua/RETOMADA.md";
const PYTEST_R9: &str = "touring run --lang bash --timeout-ms 120000 --code '";
const SEIS: [&str; 6] = [
    "grep -n alpha src/a.rs",
    "grep -rn beta crates/",
    "grep -l gamma docs/x.md",
    "grep -c delta README.md",
    "grep -w epsilon lib/zeta.py",
    "grep -i omega tests/eta.toml",
];

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}
fn insp(i: usize) -> String {
    format!("rg -n 'p{i}' src/f{i}.rs")
}
fn pytest(i: usize) -> String {
    format!("pytest tests/test_{i}.py -x")
}
fn run_route(tag: &str) -> String {
    format!("touring run --lang bash --code 'echo {tag}'")
}
fn edit_ab(fp: &str) -> serde_json::Value {
    json!({ "file_path": fp, "old_string": "a", "new_string": "b" })
}

/// Every scenario of the table.
pub(crate) fn scenarios() -> Vec<Scn> {
    let mut v = Vec::new();
    v.extend(g2_g9_rows());
    v.extend(g10_rows());
    v.extend(g6_rows());
    v.extend(g1_rows());
    v.extend(g3_g7_m3_g5_e3_rows());
    v.extend(g3_bash_evidence_rows());
    v.extend(s3_rows());
    v.extend(g11_rows());
    v.extend(actor_rows());
    v.extend(s24_rows());
    v.extend(g10_route_rows());
    v.extend(order_rows());
    v.extend(reason_rows());
    v
}

// ── Retro 11/11/2026 (c): a leitura por Bash do arquivo EXATO vale como Read para o G3 ───────
//
// Medido antes do deploy de 27/10: em 37 de 40 negações do G3 a sessão já tinha olhado o
// arquivo por Bash, e o harness do Claude Code deixa o Edit rodar sem Read dentro do
// diretório de trabalho. O G3 existe contra `edit_string_not_found` (um old_string chutado), e
// um `grep -n`/`sed -n` do arquivo exato entrega a linha exata que será editada.

fn g3_bash_evidence_rows() -> Vec<Scn> {
    let a = "{ROOT}/src/a.rs";
    let corpo = "fn main() {}\nlet needle = 1;\n";
    // Todos leem o MESMO arquivo, nas formas que o modelo usa de verdade.
    let leituras: [(&str, &str); 9] = [
        ("grep_n", "grep -n needle {ROOT}/src/a.rs"),
        ("rg_n", "rg -n needle {ROOT}/src/a.rs"),
        ("sed_n", "sed -n '1,5p' {ROOT}/src/a.rs"),
        ("cat", "cat {ROOT}/src/a.rs"),
        ("head", "head -n 20 {ROOT}/src/a.rs"),
        ("tail", "tail -n 20 {ROOT}/src/a.rs"),
        ("relative", "grep -n needle src/a.rs"),
        ("cd_then_sed", "cd {ROOT}/src && sed -n '1,5p' a.rs"),
        ("pipeline", "grep -n needle {ROOT}/src/a.rs | head -5"),
    ];
    let mut rows: Vec<Scn> = leituras
        .iter()
        .map(|(nome, cmd)| {
            let sess = leak(format!("retro-c-{nome}"));
            scn(
                leak(format!("g3_a_bash_read_counts_as_a_read__{nome}")),
                &[],
            )
            .file(a, corpo)
            .bash(sess, cmd, any())
            .edit(sess, a, pass())
            .edit(sess, a, pass())
        })
        .collect();
    // Controles negativos: o que NÃO é a leitura do arquivo exato segue como antes —
    // advisory no 1º Edit, deny no 2º.
    let nao_contam: [(&str, &str); 5] = [
        ("a_directory", "grep -rn needle {ROOT}/src"),
        ("another_file", "grep -n needle {ROOT}/src/b.rs"),
        ("a_glob", "grep -n needle {ROOT}/src/*.rs"),
        ("not_a_reader", "ls -l {ROOT}/src/a.rs"),
        (
            "an_in_place_write",
            "sed -i.bak 's/needle/pin/' {ROOT}/src/a.rs",
        ),
    ];
    rows.extend(nao_contam.iter().map(|(nome, cmd)| {
        let sess = leak(format!("retro-c-neg-{nome}"));
        scn(
            leak(format!(
                "g3_a_bash_that_does_not_read_the_exact_file_does_not_count__{nome}"
            )),
            &[],
        )
        .file(a, corpo)
        .file("{ROOT}/src/b.rs", corpo)
        .bash(sess, cmd, any())
        .edit(sess, a, advisory(&["[G3 "]))
        .edit(sess, a, deny(&[]).gate("G3"))
    }));
    // Um Bash NEGADO não rodou: não leu nada.
    rows.push(
        scn("g3_a_denied_bash_read_does_not_count", &[])
            .cfg(Cfg {
                g1_rewrite_disabled: true,
                ..Cfg::default()
            })
            .file(a, corpo)
            .bash("retro-c-denied", "grep -n x1 {ROOT}/src/b.rs", any())
            .bash("retro-c-denied", "grep -n x2 {ROOT}/src/c.rs", any())
            .bash(
                "retro-c-denied",
                "grep -n needle {ROOT}/src/a.rs",
                deny(&[]).gate("G1"),
            )
            .edit("retro-c-denied", a, advisory(&["[G3 "]))
            .edit("retro-c-denied", a, deny(&[]).gate("G3")),
    );
    // A leitura é da SESSÃO (do ator): outra sessão não herda.
    rows.push(
        scn("g3_a_bash_read_does_not_cross_sessions", &[])
            .file(a, corpo)
            .bash("retro-c-sa", "grep -n needle {ROOT}/src/a.rs", any())
            .edit("retro-c-sb", a, advisory(&["[G3 "]))
            .edit("retro-c-sa", a, pass()),
    );
    rows
}

// ── G2 / G8 / G9 ─────────────────────────────────────────────────────────────────────────

fn g2_g9_rows() -> Vec<Scn> {
    let pipe = "cargo test 2>&1 | tail -3; echo EXIT=$?";
    let piped = format!("set -o pipefail; {pipe}");
    vec![
        scn("g2_rewrites_by_default", &["g2_reescreve_por_default_e_nega_sob_fallback"])
            .bash("s1", pipe, rewrite(&[&piped]).upd("/command", json!(piped))),
        scn("g2_denies_under_the_fallback_switch", &[])
            .cfg(Cfg {
                g2_rewrite_disabled: true,
                ..Cfg::default()
            })
            .bash("s1", pipe, deny(&[&piped]).gate("G2")),
        scn("g2_rewrite_keeps_the_other_call_fields", &["g2_reescrita_preserva_os_campos_da_chamada"])
            .call(
                "s1",
                "Bash",
                json!({"command": pipe, "timeout": 600000, "run_in_background": true, "description": "d"}),
                rewrite(&[])
                    .upd("/command", json!(piped))
                    .upd("/timeout", json!(600000))
                    .upd("/run_in_background", json!(true))
                    .upd("/description", json!("d")),
            ),
        scn(
            "g8_rewrite_keeps_the_budget_and_the_call_fields",
            &["g8_reescrita_preserva_o_orcamento_e_os_campos_da_chamada"],
        )
        .call(
            "s1",
            "Bash",
            json!({
                "command": r#"for f in a.rs b.rs; do grep -n "fn main" $f; done"#,
                "timeout": 300000,
                "description": "laço"
            }),
            rewrite(&["--timeout-ms 295000", "for f in a.rs b.rs"])
                .upd("/timeout", json!(300000))
                .upd("/description", json!("laço")),
        ),
        scn("g2_bypass_token_passes", &["g2_bypass_token_passa_e_conta"])
            .bash("s1", "TOURING_GATE_OK=1 cargo test | tail -1; echo $?", pass()),
        scn("g2_heredoc_is_data_never_denied", &["g2_heredoc_e_dado_nunca_nega"])
            .bash("s1", "cat >> t.py <<'EOF'\nassert exit_pipe('a | b; echo $?')\nEOF", pass()),
        scn("g9_sed_i_denies_with_derived_routes", &["g9_sed_i_nega_com_rotas_derivadas"])
            .bash(
                "g9-a",
                "sed -i 's/old/new/' src/foo.rs",
                deny(&["Edit tool", "src/foo.rs", "sed -i '\\''s/old/new/'\\'' src/foo.rs"])
                    .without(&["<o arquivo>"])
                    .gate("G9"),
            )
            .call(
                "g9-a",
                "Edit",
                json!({"file_path": "src/foo.rs", "old_string": "old", "new_string": "new"}),
                any(),
            ),
        scn("g9_covers_the_variants_and_spares_the_reads", &["g9_cobre_as_variantes_e_poupa_as_leituras"])
            .each(0, 6, |s, i| {
                let cmd = [
                    "sed --in-place 's/a/b/' f.rs",
                    "sed -i.bak 's/a/b/' f.rs",
                    "awk -i inplace '{print}' f.txt",
                    "perl -pi -e 's/a/b/' f.txt",
                    "cat f | sed -i 's/a/b/'",
                    "cd x && sed -i 's/a/b/' f",
                ][i];
                s.bash("g9-b", cmd, speaks(&["[G9 "]))
            })
            .each(0, 4, |s, i| {
                let cmd = [
                    "sed -n '5p' f.rs",
                    "sed 's/a/b/' f.rs",
                    "echo \"rode sed -i aqui\"",
                    "cat >> s.sh <<'EOF'\nsed -i 's/a/b/' f\nEOF",
                ][i];
                s.bash("g9-b", cmd, pass())
            }),
        scn("g9_per_command_bypass_passes", &["g9_bypass_por_comando_passa"])
            .bash("g9-c", "TOURING_GATE_OK=1 sed -i 's/a/b/' f.rs", pass()),
    ]
}

// ── G10 exec-burst ───────────────────────────────────────────────────────────────────────

fn g10_rows() -> Vec<Scn> {
    vec![
        scn(
            "g10_fifth_serial_call_denies_with_the_program_and_the_run_resets",
            &["g10_quinta_chamada_nega_com_o_programa_e_run_zera"],
        )
        .each(1, 5, |s, i| s.bash("g10-a", &pytest(i), pass()))
        .bash(
            "g10-a",
            &pytest(5),
            deny(&[
                PYTEST_R9,
                &pytest(1),
                &pytest(2),
                &pytest(3),
                &pytest(4),
                &pytest(5),
            ])
            .gate("G10"),
        )
        .bash("g10-a", &pytest(6), pass()),
        scn(
            "g10_a_run_in_the_middle_zeroes_the_burst",
            &["g10_touring_run_no_meio_zera_a_rajada"],
        )
        .each(1, 5, |s, i| s.bash("g10-b", &pytest(i), any()))
        .bash("g10-b", "touring run --lang bash --code 'ls'", any())
        .each(11, 15, |s, i| s.bash("g10-b", &pytest(i), pass())),
        scn(
            "g10_per_command_bypass_passes",
            &["g10_bypass_por_comando_passa"],
        )
        .each(1, 5, |s, i| s.bash("g10-c", &pytest(i), any()))
        .bash(
            "g10-c",
            "TOURING_GATE_OK=1 pytest tests/test_5.py -x",
            pass(),
        ),
        scn(
            "g10_deny_carries_the_instantiated_prior_art",
            &["s6_deny_g10_carrega_o_prior_art_instanciado"],
        )
        .cfg(Cfg {
            portfolio_scratch: true,
            ..Cfg::default()
        })
        .each(1, 5, |s, i| s.bash("g10-prior", &pytest(i), any()))
        .bash(
            "g10-prior",
            &pytest(5),
            deny(&[PYTEST_R9, "prior art", "python3 ~/x/rodar_suite.py --fast"]),
        ),
    ]
}

// ── G6 ───────────────────────────────────────────────────────────────────────────────────

fn g6_rows() -> Vec<Scn> {
    let rg = "rg -n 'padrao' src/lib.rs";
    vec![
        scn(
            "g6_escalates_advisory_then_deny",
            &["g6_escala_advisory_e_depois_deny"],
        )
        .bash("s1", rg, pass())
        .bash("s1", rg, advisory(&["G6"]))
        .bash("s1", rg, deny(&[]).gate("G6")),
        scn(
            "g6_live_state_allowlist_never_fires",
            &["g6_allowlist_de_estado_vivo_nunca_dispara"],
        )
        .each(0, 4, |s, _| s.bash("s1", "touring doctor -j", pass())),
        scn(
            "g6_another_project_does_not_inherit_the_counter",
            &["g6_projeto_diferente_nao_herda_contador"],
        )
        .bash("s1", "rg -n 'x' src/main.rs", pass())
        .bash("s1", "rg -n 'x' src/main.rs", speaks(&[]))
        .bash_on(1, "s1", "rg -n 'x' src/main.rs", pass()),
        scn(
            "g6_another_session_does_not_inherit_the_counter",
            &["g6_outra_sessao_nao_herda_contador"],
        )
        .bash("sa", "rg -n 'y' src/main.rs", pass())
        .bash("sb", "rg -n 'y' src/main.rs", pass())
        .bash("sc", "rg -n 'y' src/main.rs", pass())
        .bash("sa", "rg -n 'y' src/main.rs", speaks(&[])),
        scn(
            "g6_a_different_command_in_the_same_session_is_a_new_call",
            &["g6_comando_diferente_na_mesma_sessao_e_chamada_nova"],
        )
        .bash("s1", "cargo check -p alfa", pass())
        .bash("s1", "cargo check -p beta", pass())
        .bash("s1", "cargo check -p alfa", speaks(&[])),
        scn(
            "kill_switch_turns_every_gate_off",
            &["kill_switch_injetado_desliga_os_gates"],
        )
        .cfg(Cfg {
            kill_switch: true,
            ..Cfg::default()
        })
        .each(0, 3, |s, _| s.bash("s1", rg, pass())),
        scn("kill_switch_control_the_repeat_fires_without_it", &[])
            .bash("s2", rg, pass())
            .bash("s2", rg, speaks(&[])),
        scn(
            "g6_a_mutation_in_the_middle_resets_the_repeat",
            &["g6_mutacao_no_meio_reseta_a_repeticao"],
        )
        .bash("s1", "sed -n '10,20p' src/lib.rs", pass())
        .read("s1", "src/lib.rs", pass())
        .call("s1", "Edit", json!({"file_path": "src/lib.rs"}), pass())
        .bash("s1", "sed -n '10,20p' src/lib.rs", pass()),
    ]
}

// ── G1 ───────────────────────────────────────────────────────────────────────────────────

fn g1_rows() -> Vec<Scn> {
    let budget = |name: &'static str, base: usize, input: serde_json::Value, want: W| {
        // The three variants are the three assertions of ONE old test.
        let covers: &'static [&'static str] = if base == 0 {
            &["g1_reescrita_preserva_o_orcamento_e_os_campos_da_chamada"]
        } else {
            &[]
        };
        scn(name, covers)
            .each(base, base + 2, |s, i| s.bash("s-orc", &insp(i), any()))
            .call("s-orc", "Bash", input, want)
    };
    let kill = Cfg {
        g1_rewrite_disabled: true,
        ..Cfg::default()
    };
    let demoted = |p: (f64, u64)| Cfg {
        g1_precision: Some(p),
        ..Cfg::default()
    };
    let mut rows = vec![
        scn(
            "g1_second_passes_third_rewrites_with_the_burst_in_the_program",
            &["g1_segunda_passa_terceira_reescreve_com_a_rajada_no_programa"],
        )
        .bash("s1", &insp(0), pass())
        .bash("s1", &insp(1), pass())
        .bash(
            "s1",
            &insp(2),
            rewrite(&[
                "touring run --lang bash",
                "p0",
                "src/f0.rs",
                "p2",
                "src/f2.rs",
            ]),
        ),
        budget(
            "g1_rewrite_keeps_the_calls_timeout_and_fields",
            0,
            json!({"command": "rg -n 'p2' src/f2.rs", "timeout": 600000, "description": "espera"}),
            rewrite(&["--timeout-ms 595000"])
                .upd("/timeout", json!(600000))
                .upd("/description", json!("espera")),
        ),
        budget(
            "g1_rewrite_default_budget_is_the_bash_default",
            10,
            json!({"command": "rg -n 'p12' src/f12.rs"}),
            rewrite(&["--timeout-ms 115000"]),
        ),
        budget(
            "g1_rewrite_in_background_takes_the_run_ceiling",
            20,
            json!({"command": "rg -n 'p22' src/f22.rs", "run_in_background": true}),
            rewrite(&["--timeout-ms 600000"]).upd("/run_in_background", json!(true)),
        ),
        scn(
            "g1_a_third_of_a_different_class_passes",
            &["g1_terceira_de_classe_diferente_passa"],
        )
        .each(0, 2, |s, i| s.bash("s1", &insp(i), any()))
        .bash("s1", "cat src/lib.rs", pass()),
        scn(
            "g1_another_project_does_not_inherit_the_burst",
            &["g1_projeto_diferente_nao_herda_rajada"],
        )
        .each(0, 2, |s, i| s.bash("s1", &insp(i), any()))
        .bash_on(1, "s1", "rg -n 'x' src/y.rs", pass()),
        scn(
            "g1_the_bypass_token_resets_the_window",
            &["g1_bypass_token_reseta_a_janela"],
        )
        .each(0, 2, |s, i| s.bash("s1", &insp(i), any()))
        .bash("s1", "TOURING_GATE_OK=1 rg -n 'w' src/z.rs", pass())
        .each(10, 12, |s, i| s.bash("s1", &insp(i), pass())),
        scn(
            "g1_autodemote_without_data_denies_the_burst",
            &["g1_autodemote_e_puro_e_exige_volume"],
        )
        .each(0, 2, |s, i| s.bash("s1", &insp(i), any()))
        .bash("s1", &insp(2), rewrite(&[])),
        scn("g1_autodemote_high_precision_still_rewrites", &[])
            .cfg(demoted((0.9, 200)))
            .each(0, 2, |s, i| s.bash("s1", &insp(i), any()))
            .bash("s1", &insp(2), rewrite(&[])),
        scn("g1_autodemote_low_volume_never_demotes", &[])
            .cfg(demoted((0.5, 50)))
            .each(0, 2, |s, i| s.bash("s1", &insp(i), any()))
            .bash("s1", &insp(2), rewrite(&[])),
        scn(
            "g1_autodemote_low_precision_with_volume_is_an_advisory",
            &[],
        )
        .cfg(demoted((0.5, 150)))
        .each(0, 2, |s, i| s.bash("s1", &insp(i), any()))
        .bash("s1", &insp(2), advisory(&["AUTODEMOVIDO"])),
        scn(
            "g1_continuation_after_a_deny_is_read_on_the_next_call",
            &["g1_continuation_check_fecha_o_ab"],
        )
        .cfg(kill.clone())
        .each(0, 2, |s, i| s.bash("sess-cont", &insp(i), any()))
        .bash("sess-cont", &insp(2), deny(&[]).gate("G1"))
        .bash("sess-cont", "rg -n 'de novo' src/a.rs", any()),
        scn(
            "g1_similar_second_call_names_the_similarity",
            &["a_razao_declarada_e_a_do_gatilho_que_disparou"],
        )
        .bash("g1-sim-3", "grep alfa /x/a.rs", pass())
        .bash(
            "g1-sim-3",
            "grep alfa /x/b.rs",
            rewrite(&["dos tokens da inspeção anterior"]).without(&["rajadas ≥3"]),
        ),
        scn(
            "g1_a_near_identical_second_is_already_acted_on",
            &["a_segunda_quase_identica_ja_e_negada"],
        )
        .bash("g1-sim-1", "grep alfa /x/a.rs", pass())
        .bash(
            "g1-sim-1",
            "grep alfa /x/b.rs",
            rewrite(&[
                "touring run --lang bash --timeout-ms 115000 --code",
                "/x/a.rs",
                "/x/b.rs",
            ]),
        ),
        scn(
            "g1_a_different_second_of_the_same_class_still_passes",
            &["segunda_da_mesma_classe_mas_diferente_ainda_passa"],
        )
        .bash("g1-sim-2", "grep alfa /x/a.rs", pass())
        .bash(
            "g1-sim-2",
            "grep beta_totalmente_outro /y/z/outro_arquivo.toml",
            pass(),
        ),
        // ── actor isolation + the burst rewrite (g1_no_replay_tests / actor_isolation_tests) ──
        scn(
            "g1_rewrites_the_burst_into_one_sandbox_run",
            &["g1_rewrites_the_burst_into_one_sandbox_run"],
        )
        .bash("v0e1-rewrite", SEIS[0], any())
        .bash("v0e1-rewrite", SEIS[1], any())
        .bash(
            "v0e1-rewrite",
            SEIS[2],
            rewrite(&[
                "touring run --lang bash --timeout-ms 115000 --code '",
                SEIS[0],
                SEIS[1],
                SEIS[2],
            ]),
        ),
        scn(
            "g1_rewrite_disabled_falls_back_to_the_classic_deny",
            &["g1_rewrite_disabled_falls_back_to_the_classic_deny"],
        )
        .cfg(kill)
        .bash("v0e1-killswitch", SEIS[0], any())
        .bash("v0e1-killswitch", SEIS[1], any())
        .bash("v0e1-killswitch", SEIS[2], deny(&[]).gate("G1")),
        scn(
            "g1_rewrite_runs_only_the_triggering_command",
            &["g1_rewrite_runs_only_the_triggering_command"],
        )
        .bash("g1-only-trigger", SEIS[0], any())
        .bash("g1-only-trigger", SEIS[1], any())
        .bash(
            "g1-only-trigger",
            SEIS[2],
            rewrite(&["# já executado (não repetir): grep -n alpha src/a.rs"]).code(&[SEIS[2]]),
        ),
        scn(
            "g1_rewrite_tells_the_model_what_happened",
            &["g1_rewrite_tells_the_model_what_happened"],
        )
        .bash("g1-tells-model", SEIS[0], any())
        .bash("g1-tells-model", SEIS[1], any())
        .bash(
            "g1-tells-model",
            SEIS[2],
            rewrite(&["[G1", "NÃO rodaram de novo"]),
        ),
        scn(
            "g1_rewrite_consumes_the_burst",
            &["g1_rewrite_consumes_the_burst"],
        )
        .bash("g1-consumes", SEIS[0], any())
        .bash("g1-consumes", SEIS[1], any())
        .bash("g1-consumes", SEIS[2], rewrite(&[]))
        .bash("g1-consumes", "grep -c delta README.md", not_rewrite()),
        scn(
            "a_touring_run_zeroes_the_g1_window",
            &["touring_run_zeroes_the_g1_window"],
        )
        .bash("g1-touring-run", SEIS[0], any())
        .bash("g1-touring-run", SEIS[1], any())
        .bash(
            "g1-touring-run",
            "touring run --lang bash --code 'true'",
            any(),
        )
        .bash("g1-touring-run", SEIS[2], not_rewrite()),
        scn(
            "g1_never_drops_an_oversized_triggering_command",
            &["g1_never_drops_an_oversized_triggering_command"],
        )
        .bash("g1-oversized", SEIS[0], any())
        .bash("g1-oversized", SEIS[1], any())
        .bash(
            "g1-oversized",
            &format!("grep -n '{}' src/a.rs", "x".repeat(3000)),
            if_rewrite(&[&"x".repeat(3000)]),
        ),
        scn(
            "g1_fires_once_per_lot",
            &[
                "g1_fires_once_per_lot",
                "g1_count_reason_counts_since_last_fire",
            ],
        )
        .each(0, 6, |s, i| {
            let want = match i {
                2 => rewrite(&["3ª inspeção da classe `grep` desde o último disparo"]),
                5 => rewrite(&[
                    "# já executado (não repetir): grep -c delta README.md",
                    "# já executado (não repetir): grep -w epsilon lib/zeta.py",
                    "3ª inspeção da classe `grep` desde o último disparo",
                ])
                .without(&["grep -n alpha src/a.rs"])
                .code(&[SEIS[5]]),
                _ => pass(),
            };
            s.bash("g1-once-per-lot", SEIS[i], want)
        }),
        scn(
            "g1_advisory_says_this_command_runs_native",
            &["g1_advisory_says_this_command_runs_native"],
        )
        .cfg(demoted((0.0, 10_000)))
        .bash("g1-advisory-text", SEIS[0], any())
        .bash("g1-advisory-text", SEIS[1], any())
        .bash(
            "g1-advisory-text",
            SEIS[2],
            advisory(&[
                "NATIVO",
                "próxima rajada",
                "[G1 rajada — AUTODEMOVIDO a advisory: precisão viva abaixo de 0.7]",
                "touring run --lang bash",
            ])
            .without(&["rode-o"]),
        ),
        scn(
            "g1_advisory_branch_also_consumes_the_lot",
            &["g1_advisory_branch_also_consumes_the_lot"],
        )
        .cfg(demoted((0.0, 10_000)))
        .bash("g1-advisory-consumes", SEIS[0], pass())
        .bash("g1-advisory-consumes", SEIS[1], pass())
        .bash("g1-advisory-consumes", SEIS[2], advisory(&["AUTODEMOVIDO"]))
        .bash("g1-advisory-consumes", SEIS[3], pass()),
    ];
    rows.shrink_to_fit();
    rows
}

// ── G3 / G7 / M3 / G5 / E3 ───────────────────────────────────────────────────────────────

fn g3_g7_m3_g5_e3_rows() -> Vec<Scn> {
    let skeleton = Cfg {
        r1_skeleton: Some("/r1/r1_varredura_agregado.py"),
        ..Cfg::default()
    };
    let fp = "crates/x/src/lib.rs";
    let g7_msg = "crates/z/src/alvo.rs";
    let reset_fp = "crates/z/src/mod.rs";
    let base = |name: &'static str, covers: &'static [&'static str]| scn(name, covers);
    vec![
        base("g3_one_advisory_then_deny_and_a_read_resets", &["g3_um_advisory_depois_deny_e_read_reseta"])
            .edit("sess-g3", "src/a.rs", advisory(&["[G3 "]))
            .edit("sess-g3", "src/a.rs", deny(&[]).gate("G3"))
            .read("sess-g3", "src/b.rs", pass())
            .edit("sess-g3", "src/b.rs", pass()),
        base("g3_a_new_session_starts_from_zero", &["g3_sessao_nova_zera"])
            .edit("sess-a", "src/x.rs", any())
            .edit("sess-a", "src/x.rs", any())
            .edit("sess-b", "src/x.rs", advisory(&["1/1"])),
        base("g7_second_advises_third_denies_distinct_files_nothing", &["g7_segunda_advisory_terceira_deny_arquivos_distintos_nada"])
            .cfg(skeleton.clone())
            .read("sess-g7", fp, pass())
            .read("sess-g7", fp, advisory(&["G7", fp, "r1_varredura_agregado"]))
            .read("sess-g7", fp, deny(&[]).gate("G7"))
            .read("sess-g7", "crates/y/src/lib.rs", pass()),
        base("m3_the_tenth_distinct_file_advises_delegation_once", &["m3_decimo_arquivo_distinto_aconselha_delegacao_uma_vez"])
            .each(0, 9, |s, i| s.read("sess-m3", &format!("src/m3_{i}.rs"), pass()))
            .read("sess-m3", "src/m3_9.rs", advisory(&["M3 delegação", "subagentes read-only"]))
            .read("sess-m3", "src/m3_10.rs", pass()),
        base("m3_a_reread_is_not_a_distinct_file", &["m3_releitura_nao_conta_distinto"])
            .each(0, 8, |s, i| s.read("sess-m3-r", &format!("src/r{i}.rs"), pass()))
            .read("sess-m3-r", "src/r0.rs", advisory(&["G7"]).without(&["M3"]))
            .read("sess-m3-r", "src/r8.rs", pass())
            .read("sess-m3-r", "src/r9.rs", speaks(&["M3 delegação"])),
        base("g3_a_creating_write_does_not_count_and_counts_as_a_read", &["g3_write_de_criacao_nao_conta_e_vale_como_read"])
            .each(0, 3, |s, i| {
                let f = ["docs/a.md", "docs/b.md", "docs/c.md"][i];
                s.call("sess-g3w", "Write", json!({"file_path": f, "content": "conteudo novo"}), pass())
            })
            .edit("sess-g3w", "docs/a.md", pass())
            .edit("sess-g3w", "src/nunca_visto.rs", advisory(&["1/1"])),
        base("g7_a_run_citing_the_file_resets", &["g7_touring_run_citando_o_arquivo_reseta"])
            .read("sess-g7r", reset_fp, any())
            .read("sess-g7r", reset_fp, any())
            .bash(
                "sess-g7r",
                &format!("touring run --lang python --args '[\"{reset_fp}\"]' --file r1.py"),
                any(),
            )
            .read("sess-g7r", reset_fp, pass()),
        base("g7_the_reset_works_with_the_path_quoted_inside_code", &["g7_reset_funciona_com_o_path_citado_dentro_de_codigo"])
            .read("sess-g7-code", reset_fp, any())
            .read("sess-g7-code", reset_fp, any())
            .bash(
                "sess-g7-code",
                &format!(
                    "touring run --lang python --code 'import pathlib; print(pathlib.Path(\"{reset_fp}\").exists())'"
                ),
                any(),
            )
            .read("sess-g7-code", reset_fp, pass()),
        base("g7_message_names_the_query_when_there_is_no_skeleton", &["g7_mensagem_nomeia_o_r1_real_ou_a_query_nunca_placeholder"])
            .read("sess-g7-msg", g7_msg, any())
            .read(
                "sess-g7-msg",
                g7_msg,
                advisory(&["touring memory query"])
                    .without(&["<r1_varredura_agregado.py>", "--file r1_varredura_agregado.py"]),
            ),
        base("g7_message_names_the_real_skeleton_when_it_exists", &[])
            .cfg(Cfg {
                r1_skeleton: Some("/h/projects/touring/docs/r1_varredura_agregado.py"),
                ..Cfg::default()
            })
            .read("sess-g7-msg2", g7_msg, any())
            .read(
                "sess-g7-msg2",
                g7_msg,
                advisory(&["projects/touring", "r1_varredura_agregado.py"])
                    .without(&["<r1_varredura_agregado.py>", "--file r1_varredura_agregado.py "]),
            ),
        base("g5_a_single_advisory_at_the_end_of_an_unvalidated_burst", &["g5_advisory_unico_no_fim_da_rajada_sem_validacao"])
            .each(0, 3, |s, i| {
                let f = ["a.rs", "b.rs", "c.rs"][i];
                s.read("sess-g5", f, pass()).edit("sess-g5", f, pass())
            })
            .bash("sess-g5", "echo done", speaks(&["G5", "3 edits"]))
            .bash("sess-g5", "echo again", pass()),
        base("g5_a_validation_ends_the_burst_without_advisory", &["g5_validacao_encerra_sem_advisory"])
            .each(0, 3, |s, i| {
                let f = ["d.rs", "e.rs", "f.rs"][i];
                s.read("sess-g5ok", f, pass()).edit("sess-g5ok", f, pass())
            })
            .bash("sess-g5ok", "cargo check -p x", pass()),
        // 11/11/2026: four `.md` edits ended in "[G5 telemetria] rajada de 4 edits terminou
        // sem build/teste" — prose has nothing to build or test, so it is no burst.
        base("g5_prose_edits_are_not_a_burst", &[])
            .each(0, 5, |s, i| {
                let f = ["docs/a.md", "docs/b.json", "Cargo.toml", "ci.yaml", "notes.txt"][i];
                s.read("sess-g5prose", f, pass()).edit("sess-g5prose", f, pass())
            })
            .bash("sess-g5prose", "echo done", quiet("[G5 telemetria]")),
        base("g5_only_code_edits_count_toward_the_burst", &[])
            .each(0, 5, |s, i| {
                let f = ["x1.rs", "docs/x2.md", "x3.py", "x4.yml", "x5.ts"][i];
                s.read("sess-g5mix", f, pass()).edit("sess-g5mix", f, pass())
            })
            .bash("sess-g5mix", "echo done", speaks(&["G5", "rajada de 3 edits"])),
        base("g5_two_code_edits_among_prose_stay_below_the_burst", &[])
            .each(0, 5, |s, i| {
                let f = ["y1.rs", "docs/y2.md", "y3.py", "y4.toml", "y5.json"][i];
                s.read("sess-g5low", f, pass()).edit("sess-g5low", f, pass())
            })
            .bash("sess-g5low", "echo done", quiet("[G5 telemetria]")),
        base("e3_a_counterfactual_comment_advises_and_a_run_id_silences", &["e3_contrafactual_em_comentario_gera_advisory_e_run_id_silencia"])
            .read("sess-e3", "m.rs", pass())
            .call(
                "sess-e3",
                "Edit",
                json!({"file_path": "m.rs", "new_string": "// sandboxar este nó quebraria a escrita do marcador\nlet x = 1;"}),
                speaks(&["contrafactual"]),
            )
            .read("sess-e3", "m.rs", any())
            .call(
                "sess-e3",
                "Edit",
                json!({"file_path": "m.rs", "new_string": "// quebraria sem pipefail — provado em run-1787618052969\nlet x = 1;"}),
                pass(),
            ),
    ]
}

// ── S3: the inspection burst (scope `code`) ──────────────────────────────────────────────

fn s3_rows() -> Vec<Scn> {
    let burst = |name: &'static str, covers: &'static [&'static str]| scn(name, covers).code();
    let mut rows = vec![
        burst(
            "s3_the_second_inspection_warns_without_denying",
            &["rajada_de_inspecao_avisa_na_segunda_sem_negar"],
        )
        .bash("s3-burst-1", "grep -rn alfa src/", pass())
        .bash(
            "s3-burst-1",
            "grep -rn beta src/",
            advisory(&[MARK, "grep -rn alfa src/", "grep -rn beta src/", "300"]),
        ),
        burst(
            "s3_one_warning_per_burst_and_a_run_rearms",
            &["o_aviso_sai_uma_vez_por_rajada_e_touring_run_rearma"],
        )
        .bash("s3-burst-2", "cat a.md", pass())
        .bash("s3-burst-2", "cat b.md", advisory(&[MARK]))
        .bash("s3-burst-2", "cat c.md", quiet(MARK))
        .bash("s3-burst-2", "cat d.md", quiet(MARK))
        .bash("s3-burst-2", "touring run --lang bash --code 'ls'", any())
        .bash("s3-burst-2", "cat e.md", quiet(MARK))
        .bash("s3-burst-2", "cat f.md", advisory(&[MARK])),
        burst(
            "s3_distinct_classes_do_not_form_a_burst",
            &["classes_distintas_nao_formam_rajada"],
        )
        .bash("s3-burst-3", "grep -rn x src/", pass())
        .bash("s3-burst-3", "cat README.md", quiet(MARK))
        .bash("s3-burst-3", "ls -la src/", quiet(MARK)),
        scn(
            "s3_a_scope_without_a_declaration_does_not_collapse",
            &["escopo_sem_declaracao_nao_colapsa_rajada"],
        )
        .bash("s3-burst-sem-escopo", "grep -rn x src/", pass())
        .bash("s3-burst-sem-escopo", "grep -rn y src/", quiet(MARK)),
        burst(
            "s3_a_mutation_and_a_build_never_enter_the_burst",
            &["mutacao_e_build_nao_entram_na_rajada"],
        )
        .bash("s3-burst-mutacao", "cargo build", quiet(MARK))
        .bash("s3-burst-mutacao", "cargo build", quiet(MARK))
        .bash("s3-burst-mutacao", "cat > out.txt <<EOF", quiet(MARK))
        .bash("s3-burst-mutacao", "cat >> out.txt <<EOF", quiet(MARK)),
        burst(
            "s3_read_only_python_inline_falls_in_the_inspection_burst",
            &["python_inline_readonly_cai_na_rajada_de_inspecao"],
        )
        .bash(
            "s3-pyinline-1",
            "python3 - <<'EOF'\nimport json\nprint(json.load(open('a.json')))\nEOF",
            pass(),
        )
        .bash(
            "s3-pyinline-1",
            "python3 -c 'import re; print(re.findall(\"x\", open(\"b.txt\").read()))'",
            advisory(&[MARK, "--lang python", "json.load", "re.findall"]),
        ),
        burst(
            "s3_a_writing_python_inline_stays_out_of_the_burst",
            &["python_inline_escritor_fica_fora_da_rajada_de_inspecao"],
        )
        .bash(
            "s3-pyinline-2",
            "python3 - <<'EOF'\nopen('out.txt', 'w').write('x')\nEOF",
            pass(),
        )
        .bash(
            "s3-pyinline-2",
            "python3 -c 'import shutil; shutil.copy(\"a\", \"b\")'",
            quiet(MARK),
        ),
        burst(
            "s3_the_burst_leaves_durable_evidence_on_disk",
            &["rajada_de_inspecao_grava_evidencia_duravel_no_disco"],
        )
        .bash("r2-duravel-1", "grep -rn a src/", pass())
        .bash("r2-duravel-1", "grep -rn b src/", speaks(&[MARK]))
        .bash("r2-duravel-1", "grep -rn c src/", any())
        .json_at(
            ".claude/touring/durable_gate_evidence.json",
            "/s3/first_passed",
            Some(1),
        )
        .json_at(
            ".claude/touring/durable_gate_evidence.json",
            "/s3/advised",
            Some(1),
        )
        .json_at(
            ".claude/touring/durable_gate_evidence.json",
            "/s3/in_burst",
            Some(1),
        )
        .json_at(
            ".claude/touring/durable_gate_evidence.json",
            "/s3/denied",
            None,
        ),
    ];
    for (class, a, b) in [
        ("sed-n", "sed -n 1,20p a.rs", "sed -n 30,50p b.rs"),
        ("ls", "ls -la src/", "ls -la crates/"),
        ("wc", "wc -l a.rs", "wc -l b.rs"),
    ] {
        let name = leak(format!("s3_the_class_{class}_now_collapses_in_a_burst"));
        let sess = leak(format!("s3-antes-isenta-{class}"));
        rows.push(
            burst(name, &["classes_antes_isentas_agora_colapsam_em_rajada"])
                .bash(sess, a, pass())
                .bash(sess, b, advisory(&[MARK])),
        );
    }
    rows
}

// ── G11: the bypass budget ───────────────────────────────────────────────────────────────

fn g11_rows() -> Vec<Scn> {
    let by = |c: &str| format!("{BYPASS} echo {c}");
    let mut rows = vec![
        scn("g11_the_first_bypass_passes", &["the_first_bypass_passes"]).bash("t", &by("a"), pass()),
        scn("g11_two_in_a_row_are_denied_by_the_strict_rule", &["two_in_a_row_are_denied_by_the_strict_rule"])
            .bash("t", &by("a"), pass())
            .bash(
                "t",
                &by("b"),
                deny(&["[G11 orçamento de bypass] dois bypasses seguidos: 2 seguido(s), 2 desde a última rota, janela 600s"])
                    .gate("G11"),
            ),
        scn("g11_an_interleaved_pair_is_not_a_strict_run", &["an_interleaved_pair_is_not_a_strict_run", "a_neutral_command_breaks_only_the_strict_run"])
            .bash("t", &by("a"), pass())
            .bash("t", "true", any())
            .bash("t", &by("b"), pass()),
        scn("g11_the_third_without_the_route_is_denied_even_when_spaced", &["the_third_without_the_route_is_denied_even_when_spaced"])
            .bash("t", &by("a"), pass())
            .bash("t", "true", any())
            .bash("t", &by("b"), pass())
            .bash("t", "true", any())
            .bash(
                "t",
                &by("c"),
                deny(&["[G11 orçamento de bypass] terceiro bypass sem usar a rota: 1 seguido(s), 3 desde a última rota"])
                    .gate("G11"),
            ),
        scn("g11_the_route_credits_both_counts_back", &["the_route_credits_both_counts_back"])
            .bash("t", &by("a"), pass())
            .bash("t", "true", any())
            .bash("t", &by("b"), pass())
            .bash("t", "touring run --lang bash --code 'echo r'", any())
            .bash("t", "true", any())
            .bash("t", &by("c"), pass())
            .bash("t", "true", any())
            .bash("t", &by("d"), pass())
            .bash("t", "true", any())
            .bash("t", &by("e"), deny(&["terceiro bypass sem usar a rota"])),
        scn("g11_an_echo_that_mentions_the_route_does_not_credit", &["the_sanctioned_route_is_recognised", "path_qualified_touring_run_is_the_sanctioned_route"])
            .bash("t", &by("a"), pass())
            .bash("t", "true", any())
            .bash("t", &by("b"), pass())
            .bash("t", "echo touring running", any())
            .bash("t", "touring index find X", any())
            .bash("t", "true", any())
            .bash("t", &by("c"), deny(&["terceiro bypass sem usar a rota"])),
    ];
    for (i, form) in [
        "touring run --lang bash --file x.sh",
        "  touring run --lang python --code 'x'",
        ".touring/bin/touring run --lang bash --file x.sh",
        "/home/u/.local/bin/touring run --code 'ls'",
    ]
    .into_iter()
    .enumerate()
    {
        let name = leak(format!("g11_the_route_form_{i}_is_sanctioned"));
        rows.push(
            scn(
                name,
                &[
                    "the_sanctioned_route_is_recognised",
                    "path_qualified_touring_run_is_the_sanctioned_route",
                ],
            )
            .bash("t", &by("a"), pass())
            .bash("t", "true", any())
            .bash("t", &by("b"), pass())
            .bash("t", form, any())
            .bash("t", "true", any())
            .bash("t", &by("c"), pass())
            .bash("t", "true", any())
            .bash("t", &by("d"), pass()),
        );
    }
    rows
}

// ── Actor isolation (S23) ────────────────────────────────────────────────────────────────

fn actor_rows() -> Vec<Scn> {
    let probe = "/tmp/s23_g7_probe.py";
    let same = "/tmp/s23_g7_same.py";
    let trio = "/tmp/s11_trio_g7.py";
    vec![
        scn(
            "g7_counter_is_isolated_per_session_and_agent",
            &["g7_counter_isolated_per_session_and_agent"],
        )
        .read("owner", probe, any())
        .read("owner", probe, any())
        .read("other", probe, pass())
        .read("agent", probe, pass()),
        scn(
            "g7_third_read_by_the_same_actor_is_still_denied",
            &["g7_third_read_same_actor_still_denied"],
        )
        .read("me", same, any())
        .read("me", same, any())
        .read("me", same, deny(&[]).gate("G7")),
        scn(
            "a_real_denial_carries_the_gate_id_the_tool_trio_reads",
            &["real_gate_denial_is_read_by_the_tool_trio"],
        )
        .read("me", trio, pass())
        .read("me", trio, any())
        .read("me", trio, deny(&["[G7 "]).gate("G7")),
        scn(
            "the_burst_window_is_isolated_per_agent",
            &["burst_window_isolated_per_agent"],
        )
        .bash("owner", "grep -n alpha src/a.rs", any())
        .bash("owner", "grep -rn beta crates/", any())
        .bash("agent", "grep -c gamma README.md", pass())
        .bash("owner", "grep -l delta docs/x.md", rewrite(&[])),
        scn(
            "the_g1_burst_window_is_per_session",
            &["g1_burst_window_is_per_session"],
        )
        .bash("a", "grep -n alpha src/a.rs", any())
        .bash("a", "grep -rn beta crates/", any())
        .bash("b", "grep -c gamma README.md", pass())
        .bash("b", "grep -l delta docs/x.md", any())
        .bash(
            "b",
            "grep -n epsilon src/e.rs",
            rewrite(&[]).without(&["alpha", "beta"]),
        ),
        scn(
            "the_bypass_budget_is_isolated_per_actor",
            &["bypass_budget_isolated_per_actor"],
        )
        .bash("a", &format!("{BYPASS} ls /tmp"), any())
        .bash("a", &format!("{BYPASS} ls /var"), any())
        .bash("b", &format!("{BYPASS} ls /etc"), pass()),
    ]
}

// ── S24: the route is capability-aware and reads shell, not bytes ────────────────────────

fn s24_rows() -> Vec<Scn> {
    let outside = |name: &'static str, covers: &'static [&'static str]| {
        scn(name, covers).symlink("{ROOT}/.touring/bin/touring", "/root")
    };
    vec![
        outside(
            "s24_a_loop_outside_the_sandbox_capability_is_not_rewritten",
            &["code_mode_does_not_rewrite_commands_outside_sandbox_capability"],
        )
        .bash(
            "s24-rewrite",
            "for s in a b c; do .touring/bin/touring decompose ready $s; done",
            not_rewrite(),
        ),
        outside(
            "s24_a_burst_of_unreachable_commands_is_not_fused",
            &["burst_of_unreachable_commands_is_not_fused_toward_the_sandbox"],
        )
        .bash("s24-burst", "cat .touring/bin/touring", pass())
        .bash("s24-burst", "cat .touring/bin/touring x", pass())
        .bash("s24-burst", "cat .touring/bin/touring y", pass()),
        scn(
            "s24_a_burst_inside_the_project_is_still_fused",
            &["code_mode_still_fuses_grep_burst_inside_project"],
        )
        .bash("s24-control", "grep -n alpha src/a.rs", any())
        .bash("s24-control", "grep -rn beta crates/", any())
        .bash("s24-control", "grep -l gamma docs/x.md", rewrite(&[])),
        scn(
            "s24_the_bypass_token_inside_a_heredoc_is_data",
            &["bypass_token_inside_heredoc_is_data_not_a_bypass"],
        )
        .each(0, 3, |s, _| {
            s.bash(
                "s24-heredoc",
                &format!("cat > t.rs <<'EOF'\nlet x = \"{BYPASS} ls\";\nEOF"),
                quiet("[G11 "),
            )
        }),
    ]
}

// ── G10 route (the real `analise` cases), G7×G3×G9, write→run ────────────────────────────

fn writers_before(s: Scn, sess: &'static str) -> Scn {
    s.each(1, 5, |s, i| {
        s.bash(
            sess,
            &format!(
                ".venv/bin/python3 -c 'import pathlib; p = pathlib.Path(\"{RETOMADA}\"); p.write_text(p.read_text() + \"# passo {i}\")'"
            ),
            pass(),
        )
    })
}

fn project_g10(name: &'static str, covers: &'static [&'static str]) -> Scn {
    scn(name, covers).code().file(
        &format!("{{ROOT}}/{RETOMADA}"),
        "> Estado 10/10 ~00:30: pendente\n",
    )
}

const COMANDO_REAL_G10: &str = "TOURING_CODE_MODE=native .venv/bin/python3 - <<'EOF'\nimport pathlib\np = pathlib.Path(\"docs/plans/2026-10-06-geopolitica-logistica-eua/RETOMADA.md\")\nt = p.read_text(encoding=\"utf-8\")\nvelho = \"> Estado 10/10 ~00:30:\"\nassert t.count(velho) == 1\np.write_text(t.replace(velho, \"> Estado 09/10 22:38 (o relógio da máquina; eu tinha escrito «10/10 ~00:30» — errado):\"), encoding=\"utf-8\")\nprint(\"ok\")\nEOF";

fn edit_file() -> Scn {
    scn("", &[])
        .code()
        .file(&format!("{{ROOT}}/{RETOMADA}"), "base")
}

/// A source file for the rows about the G5 edit burst, which counts CODE edits only.
const CODE_TARGET: &str = "crates/x/src/lib.rs";

fn edit_code_file() -> Scn {
    scn("", &[])
        .code()
        .file(&format!("{{ROOT}}/{CODE_TARGET}"), "base")
}

fn g10_route_rows() -> Vec<Scn> {
    let fp = format!("{{ROOT}}/{RETOMADA}");
    let leitura = ".venv/bin/python3 -c 'import pathlib; print(pathlib.Path(\"docs\").exists())'";
    let fora_cmd =
        ".venv/bin/python3 -c 'import pathlib; pathlib.Path(\"{OUT}/saida.md\").write_text(\"x\")'";
    let rd = |sess: &'static str, s: Scn, want: W| {
        s.call(sess, "Read", json!({"file_path": fp.clone()}), want)
    };
    let mut rows = vec![
        writers_before(
            project_g10(
                "g10_a_body_that_writes_names_the_trusted_route",
                &["g10_corpo_que_escreve_nomeia_a_rota_trusted"],
            ),
            "g97-g10-trusted",
        )
        .bash(
            "g97-g10-trusted",
            COMANDO_REAL_G10,
            deny(&[
                "TOURING_TRUSTED_OK=1 touring run --allow-forbidden --lang python --code '",
                "write_text",
                "workspace",
            ])
            .without(&["touring run --lang python --code"])
            .gate("G10"),
        ),
        writers_before(
            project_g10(
                "g10_a_destination_outside_the_workspace_promises_no_route",
                &["g10_destino_fora_do_workspace_nao_promete_rota"],
            ),
            "g97-g10-fora",
        )
        .bash(
            "g97-g10-fora",
            fora_cmd,
            deny(&["{OUT}/saida.md", "nenhuma rota", "TOURING_GATE_OK=1"]).without(&[
                "TOURING_TRUSTED_OK=1 touring run",
                "touring run --lang python --code",
            ]),
        ),
        writers_before(
            project_g10(
                "g10_a_read_body_keeps_the_sandboxed_route",
                &["g10_corpo_de_leitura_segue_com_a_rota_sandboxed"],
            ),
            "g97-g10-leitura",
        )
        .bash(
            "g97-g10-leitura",
            leitura,
            deny(&["touring run --lang python --code '"]).without(&["TOURING_TRUSTED_OK"]),
        ),
        writers_before(
            project_g10(
                "g10_without_a_route_the_reason_hands_over_the_bypass_line_and_it_passes",
                &["g10_sem_rota_entrega_a_linha_de_bypass_do_proprio_comando"],
            ),
            "s13-g10-linha",
        )
        .bash(
            "s13-g10-linha",
            fora_cmd,
            deny(&[&format!("\n  {BYPASS} {fora_cmd}")]),
        )
        .bash("s13-g10-linha", &format!("{BYPASS} {fora_cmd}"), pass()),
    ];
    // G7 × the external modification the harness demands a re-read for.
    rows.push({
        let s = edit_file().touch(&fp, "> Estado 10/10 ~00:30: pendente\n", 1);
        let read8 = json!({"file_path": fp, "limit": 8});
        rename(
            s.call("g97-g7-real", "Read", read8.clone(), pass())
                .call("g97-g7-real", "Read", read8.clone(), advisory(&[]))
                .touch(&fp, "> Estado 09/10 22:38: corrigido\n", 2)
                .call("g97-g7-real", "Read", read8, not_deny()),
            "g7_a_reread_after_an_external_modification_is_not_denied",
            &["g7_releitura_depois_de_modificacao_externa_nao_e_negada"],
        )
    });
    rows.push(rename(
        rd(
            "g97-g7-controle",
            rd(
                "g97-g7-controle",
                rd("g97-g7-controle", edit_file(), any()),
                any(),
            ),
            deny(&[]).gate("G7"),
        ),
        "g7_a_third_read_of_an_unchanged_file_is_still_denied",
        &["g7_terceira_leitura_do_arquivo_inalterado_segue_negada"],
    ));
    rows.push(rename(
        rd(
            "g97-g7-rota",
            rd("g97-g7-rota", rd("g97-g7-rota", edit_file(), any()), any()),
            deny(&["Edit direto", "a releitura é liberada"]).gate("G7"),
        )
        .call("g97-g7-rota", "Edit", edit_ab(&fp), not_deny())
        .touch(&fp, "mudou por fora", 7)
        .call("g97-g7-rota", "Read", json!({"file_path": fp}), not_deny()),
        "g7_the_deny_names_the_edit_route_and_the_promise_holds",
        &["g7_deny_nomeia_a_rota_de_edicao_e_a_promessa_cumpre"],
    ));
    rows.push(rename(
        {
            let ghost = "{ROOT}/nao-existe/ainda.md";
            let r =
                |s: Scn, w: W| s.call("g97-g7-sem-medicao", "Read", json!({"file_path": ghost}), w);
            r(
                r(r(scn("", &[]).code(), any()), any()),
                deny(&[]).without(&["Edit direto"]).gate("G7"),
            )
        },
        "g7_a_deny_without_a_measurement_promises_no_edit_route",
        &["g7_deny_sem_medicao_nao_promete_rota_de_edicao"],
    ));
    // write→run pair
    let script = |name: &'static str,
                  covers: &'static [&'static str],
                  sess: &'static str,
                  a: (&str, &str),
                  b: (&str, &str),
                  want: W| {
        scn(name, covers)
            .code()
            .file(a.0, a.1)
            .file(b.0, b.1)
            .bash(
                sess,
                &format!("cat > {} <<'PYEOF'\nprint(1)\nPYEOF", a.0),
                any(),
            )
            .bash(
                sess,
                &format!("cat > {} <<'PYEOF'\nprint(2)\nPYEOF", b.0),
                any(),
            )
            .bash(sess, &format!("python3 {}", a.0), pass())
            .bash(sess, &format!("python3 {}", b.0), want)
    };
    let reader = "import pathlib\nprint(pathlib.Path(\"RETOMADA.md\").exists())\n";
    rows.push(script(
        "g10_a_writing_script_pair_names_the_trusted_route",
        &["g10_par_write_run_de_script_que_escreve_nomeia_a_rota_trusted"],
        "g97-par-escreve",
        ("{ROOT}/le.py", reader),
        ("{ROOT}/escreve.py", "import pathlib\npathlib.Path(\"RETOMADA.md\").write_text(\"novo\")\n"),
        deny(&["TOURING_TRUSTED_OK=1 touring run --allow-forbidden --lang python --file {ROOT}/escreve.py --timeout-ms 60000"])
            .without(&["touring run --lang python --file {ROOT}/escreve.py"])
            .gate("G10"),
    ));
    rows.push(script(
        "g10_a_reading_script_pair_keeps_the_sandboxed_route",
        &["g10_par_write_run_de_script_de_leitura_segue_sandboxed"],
        "g97-par-leitura",
        ("{ROOT}/a.py", reader),
        ("{ROOT}/b.py", reader),
        deny(&["touring run --lang python --file {ROOT}/b.py --timeout-ms 60000"])
            .without(&["TOURING_TRUSTED_OK"]),
    ));
    rows.push(script(
        "g10_a_pair_with_a_destination_outside_promises_no_route",
        &["g10_par_write_run_com_destino_fora_nao_promete_rota"],
        "g97-par-fora",
        ("{ROOT}/a.py", "print(1)\n"),
        (
            "{ROOT}/b.py",
            "import pathlib\npathlib.Path(\"{OUT}/saida.md\").write_text(\"x\")\n",
        ),
        deny(&["nenhuma rota", "{OUT}/saida.md"]).without(&[
            "TOURING_TRUSTED_OK=1 touring run",
            "touring run --lang python --file",
        ]),
    ));
    rows.push(
        scn(
            "s5_the_second_pair_denies_with_the_own_file",
            &["s5_par_write_run_nega_no_segundo_par_com_o_proprio_arquivo"],
        )
        .bash(
            "sess-s5",
            "cat > /tmp/s5/a.py <<'PYEOF'\nprint(1)\nPYEOF",
            any(),
        )
        .bash("sess-s5", "python3 /tmp/s5/a.py", pass())
        .bash(
            "sess-s5",
            "cat > /tmp/s5/b.py <<'PYEOF'\nprint(2)\nPYEOF\npython3 /tmp/s5/b.py",
            deny(&["write→run", "touring run --lang python --file /tmp/s5/b.py"]).gate("G10"),
        )
        .bash("sess-s5", "python3 /tmp/s5/alheio.py", pass()),
    );
    rows.push(
        scn(
            "s5_the_pair_window_does_not_leak_between_sessions",
            &["s5_janela_do_par_nao_vaza_entre_sessoes"],
        )
        .bash(
            "sess-A",
            "cat > /tmp/s5x/c.py <<'PYEOF'\nprint(1)\nPYEOF",
            any(),
        )
        .each(0, 3, |s, i| {
            s.bash("sess-B", &format!("python3 /tmp/s5x/c.py {i}"), pass())
        }),
    );
    rows
}

/// Name a scenario assembled with an anonymous constructor.
fn rename(mut s: Scn, name: &'static str, covers: &'static [&'static str]) -> Scn {
    s.name = name;
    s.covers = covers;
    s
}

// ── Order of the chain: an applicable deny beats an advisory ─────────────────────────────

fn order_rows() -> Vec<Scn> {
    let fp = format!("{{ROOT}}/{RETOMADA}");
    let code_fp = format!("{{ROOT}}/{CODE_TARGET}");
    let burst_of_edits = |s: Scn, sess: &'static str, fp: &str| {
        s.call(sess, "Read", json!({"file_path": fp, "limit": 8}), any())
            .each(0, 3, |s, _| s.call(sess, "Edit", edit_ab(fp), not_deny()))
    };
    let g6 = |s: Scn, sess: &'static str| {
        s.bash(sess, "cat docs/g6a.md", pass())
            .bash(sess, &run_route("r1"), pass())
            .bash(sess, "cat docs/g6a.md", advisory(&[]))
            .bash(sess, &run_route("r2"), pass())
    };
    vec![
        // The G5 burst counts edits of CODE only (11/11/2026): these two rows drive it on a
        // code file — on the plan's `.md` the advisory these rows are about never speaks.
        burst_of_edits(edit_code_file(), "g97-ordem-g5-g9", &code_fp)
            .bash(
                "g97-ordem-g5-g9",
                &format!("sed -i 's/a/b/' {code_fp}"),
                deny(&[
                    "[G9 escrita-cega-inline]",
                    &format!("Edit tool em `{code_fp}`"),
                    "[G5 telemetria]",
                ])
                .gate("G9"),
            )
            .named(
                "g5_an_advisory_does_not_swallow_the_g9_deny",
                &["g5_advisory_nao_engole_o_deny_do_g9"],
            ),
        burst_of_edits(edit_code_file(), "g97-ordem-g5-controle", &code_fp)
            .bash(
                "g97-ordem-g5-controle",
                "echo ok",
                advisory(&["[G5 telemetria]"]),
            )
            .named(
                "g5_a_harmless_bash_after_3_edits_keeps_the_advisory",
                &["g5_bash_inofensiva_depois_de_3_edits_segue_com_o_advisory"],
            ),
        edit_file()
            .bash(
                "g97-ordem-g9-controle",
                &format!("sed -i 's/a/b/' {fp}"),
                deny(&["[G9 escrita-cega-inline]"]).without(&["[G5 telemetria]"]),
            )
            .named(
                "g9_sed_i_without_edits_before_has_no_g5_text",
                &["g9_sed_i_sem_edits_antes_segue_negado_sem_texto_de_g5"],
            ),
        g6(scn("", &[]).code(), "g97-ordem-s3-g6")
            .bash("g97-ordem-s3-g6", "cat docs/g6b.md", pass())
            .bash(
                "g97-ordem-s3-g6",
                "cat docs/g6a.md",
                deny(&["[G6 redundant-exact-call]", MARK]).gate("G6"),
            )
            .named(
                "s3_an_advisory_does_not_swallow_the_g6_deny",
                &["s3_advisory_nao_engole_o_deny_do_g6"],
            ),
        scn("", &[])
            .code()
            .bash("g97-ordem-s3-controle", "grep -n alfa docs/g6a.md", pass())
            .bash(
                "g97-ordem-s3-controle",
                "grep -n alfa docs/g6b.md",
                advisory(&[MARK]).without(&["updatedInput"]),
            )
            .named(
                "s3_a_similar_second_inspection_only_advises",
                &["s3_segunda_inspecao_parecida_segue_so_com_o_advisory"],
            ),
    ]
}

impl Scn {
    fn named(mut self, name: &'static str, covers: &'static [&'static str]) -> Self {
        self.name = name;
        self.covers = covers;
        self
    }
}

// ── S-13: every denial names the next command ────────────────────────────────────────────

fn reason_rows() -> Vec<Scn> {
    let linha = format!("{BYPASS} cat docs/g6a.md");
    let g6_to_deny = |name: &'static str,
                      covers: &'static [&'static str],
                      sess: &'static str,
                      cmd: &str,
                      adv: W,
                      den: W| {
        scn(name, covers)
            .code()
            .bash(sess, cmd, pass())
            .bash(sess, &run_route("r1"), pass())
            .bash(sess, cmd, adv)
            .bash(sess, &run_route("r2"), pass())
            .bash(sess, "echo outra-coisa", pass())
            .bash(sess, cmd, den)
    };
    let grande = |name: &'static str,
                  covers: &'static [&'static str],
                  sess: &'static str,
                  old: &'static str,
                  short: bool| {
        let s = scn(name, covers).code();
        let s = if short {
            s.file("{ROOT}/curto.rs", "fn a() {}\n")
        } else {
            s.file(
                "{ROOT}/grande.rs",
                &(1..=200)
                    .map(|i| format!("linha {i}\n"))
                    .collect::<String>(),
            )
        };
        s.sess_edit(
            sess,
            if short {
                "{ROOT}/curto.rs"
            } else {
                "{ROOT}/grande.rs"
            },
            old,
        )
    };
    let long = format!("echo {}", "x".repeat(601));
    let mut rows = vec![
        g6_to_deny(
            "g6_hands_over_the_bypass_line_of_the_own_command_and_it_passes",
            &["g6_entrega_a_linha_de_bypass_do_proprio_comando_e_ela_passa"],
            "s13-g6-linha",
            "cat docs/g6a.md",
            advisory(&[
                "[G6 redundant-exact-call] ",
                &format!("\n  {linha}\n"),
                "PRÓXIMA idêntica será negada",
            ]),
            deny(&[
                "[G6 redundant-exact-call] ",
                &format!("\n  {linha}\n"),
                "(1) o dado já basta",
                "(2) precisa de OUTRO recorte",
                "(3) o estado mudou",
                "conta no G11",
                "Proibido: reemitir este comando sem mudar nada",
            ])
            .without(&["\nComando:"])
            .gate("G6"),
        )
        .bash("s13-g6-linha", &linha, pass()),
        g6_to_deny(
            "g6_a_multi_line_command_gets_no_broken_ready_line",
            &["g6_comando_que_nao_cabe_numa_linha_nao_ganha_linha_pronta"],
            "s13-g6-multi-0",
            "echo um\necho dois",
            advisory(&[]),
            deny(&[&format!("com `{BYPASS} `"), "\nComando: echo "])
                .without(&[&format!("\n  {BYPASS} ")]),
        ),
        g6_to_deny(
            "g6_a_command_too_long_for_one_line_gets_no_ready_line",
            &[],
            "s13-g6-multi-1",
            &long,
            advisory(&[]),
            deny(&[&format!("com `{BYPASS} `"), "\nComando: echo "])
                .without(&[&format!("\n  {BYPASS} ")]),
        ),
        scn(
            "g3_hands_over_the_read_window_and_the_read_unlocks_the_edit",
            &["g3_entrega_o_read_com_a_janela_do_old_string_e_o_read_destrava_o_edit"],
        )
        .code()
        .file(
            "{ROOT}/grande.rs",
            &(1..=200)
                .map(|i| format!("linha {i}\n"))
                .collect::<String>(),
        )
        .call(
            "s13-g3-janela",
            "Edit",
            json!({"file_path": "{ROOT}/grande.rs", "old_string": "linha 120", "new_string": "x"}),
            advisory(&[
                "[G3 edit-sem-read] ",
                "(advisory 1/1",
                "`Read {ROOT}/grande.rs offset=115 limit=11`",
                "via Bash sobre este MESMO arquivo",
            ])
            .without(&["cat/sed/grep via Bash não"]),
        )
        .call(
            "s13-g3-janela",
            "Edit",
            json!({"file_path": "{ROOT}/grande.rs", "old_string": "linha 120", "new_string": "x"}),
            deny(&[
                "[G3 edit-sem-read] 2º Edit sem Read",
                "`Read {ROOT}/grande.rs offset=115 limit=11`, e só então o Edit",
                "o Read ou o grep de OUTRO arquivo não registra",
                "Proibido até lá",
            ])
            .without(&["cat/sed/grep via Bash e o Read de OUTRO arquivo não registram"])
            .gate("G3"),
        )
        .call(
            "s13-g3-janela",
            "Read",
            json!({"file_path": "{ROOT}/grande.rs", "offset": 115, "limit": 11}),
            not_deny(),
        )
        .call(
            "s13-g3-janela",
            "Edit",
            json!({"file_path": "{ROOT}/grande.rs", "old_string": "linha 120", "new_string": "x"}),
            pass(),
        ),
    ];
    rows.push(
        grande(
            "g3_a_short_file_hands_over_the_whole_file_read",
            &["g3_sem_janela_derivavel_entrega_o_read_do_arquivo_todo"],
            "s13-g3-simples-0",
            "fn a",
            true,
        )
        .sess_edit_want(
            "s13-g3-simples-0",
            "{ROOT}/curto.rs",
            "fn a",
            deny(&["`Read {ROOT}/curto.rs`, e só então"]).without(&["offset="]),
        ),
    );
    rows.push(
        grande(
            "g3_a_missing_old_string_hands_over_the_whole_file_read",
            &[],
            "s13-g3-simples-1",
            "texto que não existe no arquivo",
            false,
        )
        .sess_edit_want(
            "s13-g3-simples-1",
            "{ROOT}/grande.rs",
            "texto que não existe no arquivo",
            deny(&["`Read {ROOT}/grande.rs`, e só então"]).without(&["offset="]),
        ),
    );
    rows
}

impl Scn {
    /// An Edit whose `old_string` is `old` (the first of a pair; result ignored).
    fn sess_edit(self, sess: &'static str, fp: &str, old: &str) -> Self {
        self.call(
            sess,
            "Edit",
            json!({"file_path": fp, "old_string": old, "new_string": "x"}),
            any(),
        )
    }
    fn sess_edit_want(self, sess: &'static str, fp: &str, old: &str, want: W) -> Self {
        self.call(
            sess,
            "Edit",
            json!({"file_path": fp, "old_string": old, "new_string": "x"}),
            want,
        )
    }
}
