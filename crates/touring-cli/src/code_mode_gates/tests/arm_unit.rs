//! The A/B arm and its evidence — route offers, durable counts, the policy, the r3 shadow —
//! moved verbatim with the code (they are about the arm, not about gate verdicts).

use super::support::tmpdir;

/// R2 (29/08): the durable evidence accumulates on DISK — 2 bumps add up, a
/// new family coexists, a corrupted file restarts from zeros without a panic
/// (unreadable evidence is nonexistent evidence, never an invented value).
#[test]
#[serial_test::serial(gate_metrics)]
fn evidencia_duravel_acumula_em_disco_e_tolera_corrupcao() {
    use crate::code_mode_gates::arm::{bump_durable_evidence, read_durable_evidence};
    let tmp = tempfile::tempdir().expect("tempdir");
    bump_durable_evidence(tmp.path(), "s3", "denied");
    bump_durable_evidence(tmp.path(), "s3", "denied");
    bump_durable_evidence(tmp.path(), "s3", "first_passed");
    bump_durable_evidence(tmp.path(), "pillar", "emitted");
    let v = read_durable_evidence(tmp.path());
    assert_eq!(v.pointer("/s3/denied").and_then(|x| x.as_u64()), Some(2));
    assert_eq!(
        v.pointer("/s3/first_passed").and_then(|x| x.as_u64()),
        Some(1)
    );
    assert_eq!(
        v.pointer("/pillar/emitted").and_then(|x| x.as_u64()),
        Some(1)
    );
    // corruption: the read returns an empty object; the next bump restarts from 1
    let path = tmp
        .path()
        .join(".claude/touring/durable_gate_evidence.json");
    std::fs::write(&path, "{ nao-e-json").expect("write");
    assert!(
        read_durable_evidence(tmp.path())
            .as_object()
            .is_some_and(|o| o.is_empty())
    );
    bump_durable_evidence(tmp.path(), "s3", "denied");
    let v2 = read_durable_evidence(tmp.path());
    assert_eq!(v2.pointer("/s3/denied").and_then(|x| x.as_u64()), Some(1));
}

/// The pure predicate: closed or without a 1st → FirstPass; open turn with a 1st →
/// Fold accumulating the command. Separating decision from state is what makes the
/// gate easier to test than to work around.
#[test]
fn rota_tomada_e_rota_recusada_tem_vereditos_opostos() {
    use crate::code_mode_gates::arm::classify_route_outcome;
    // Running the program IS the route. The VALUE carries the economy (P5b): a shell
    // around one operation is worth 0.5; what matters here is that it is positive,
    // whereas refusing is worth 0.0.
    assert_eq!(
        classify_route_outcome("touring run --lang bash --code 'ls'"),
        Some(0.5)
    );
    assert_eq!(
        classify_route_outcome("touring run --lang bash --code 'cat /a/x.rs /a/y.rs /a/z.rs'"),
        Some(1.0),
        "o programa que funde vale a rota inteira"
    );
    // Relaxing the gate is refusing it.
    assert_eq!(
        classify_route_outcome("TOURING_CODE_MODE=native grep -rn foo ."),
        Some(0.0)
    );
    assert_eq!(classify_route_outcome("TOURING_GATE_OK=1 cat x"), Some(0.0));
}

/// The failure this catches: reading "did something else" as a failure. Absence of
/// signal is unknown — crediting 0.0 here would punish the arm for silence.
#[test]
fn desfecho_desconhecido_nao_vira_veredito() {
    use crate::code_mode_gates::arm::classify_route_outcome;
    assert_eq!(classify_route_outcome("cargo test -p touring-cli"), None);
    assert_eq!(classify_route_outcome(""), None);
}

/// Order: having relaxed the gate on the way does not undo having run the program.
#[test]
fn relaxar_o_gate_e_ainda_assim_rodar_conta_como_rota_tomada() {
    use crate::code_mode_gates::arm::classify_route_outcome;
    let v = classify_route_outcome("TOURING_CODE_MODE=native touring run --lang bash --code 'ls'")
        .expect("rodar o programa é um desfecho legível");
    assert!(
        v > 0.0,
        "rodar o programa domina o token de relaxamento — senão o braço \
         aprenderia o oposto do que houve (o valor exato é a ECONOMIA, \
         coberta por outro teste)"
    );
}

/// The failure this catches — and which existed for a few minutes today: a
/// PostToolUse of a tool WITHOUT a command (a `Read`) consumed the offer without a
/// verdict, and the route taken right afterwards found nothing to credit.
#[test]
fn desfecho_ilegivel_nao_consome_a_oferta() {
    use crate::code_mode_gates::{RouteOffer, claim_route_reward, turn_ledger_insert_for_test};
    let raiz = std::path::Path::new("/tmp/p2-teste-ordem");
    let payload = serde_json::json!({"session_id": "sessao-ordem"});
    turn_ledger_insert_for_test(
        raiz,
        &payload,
        RouteOffer {
            mode: "code".into(),
            offered_secs: 7,
        },
    );

    // A `Read` carries no command: nothing to classify.
    assert!(claim_route_reward(raiz, &payload, "").is_none());
    // …and the offer MUST still be there for the real verdict.
    let (offer, value) = claim_route_reward(raiz, &payload, "touring run --lang bash --code 'ls'")
        .expect("a oferta sobreviveu ao desfecho ilegível");
    assert_eq!(offer.mode, "code");
    assert_eq!(
        value, 0.5,
        "programa de um alvo: rota tomada, economia nula"
    );
    // And now it goes away.
    assert!(claim_route_reward(raiz, &payload, "touring run --code 'x'").is_none());
}

/// A claimed offer goes away: two readers never credit the same
/// decision twice (the discipline of the decision and case ledgers).
#[test]
fn a_oferta_de_rota_e_reivindicada_uma_unica_vez() {
    use crate::code_mode_gates::{RouteOffer, take_route_offer, turn_ledger_insert_for_test};
    let raiz = std::path::Path::new("/tmp/p2-teste-rota");
    let payload = serde_json::json!({"session_id": "sessao-p2"});
    turn_ledger_insert_for_test(
        raiz,
        &payload,
        RouteOffer {
            mode: "code".into(),
            offered_secs: 42,
        },
    );
    let primeira = take_route_offer(raiz, &payload);
    assert_eq!(primeira.map(|o| o.mode), Some("code".to_string()));
    assert!(
        take_route_offer(raiz, &payload).is_none(),
        "a segunda leitura não pode reencontrar a oferta já creditada"
    );
}

/// The invariant that protects Gabriel: the policy fills the space the
/// human left open — it never overrides what he declared. A
/// policy that beat `touring.toml` would not be learning, it would be
/// disobeying.
///
/// Tested on the PURE PREDICATE: the previous version mutated
/// `TOURING_CODE_MODE_ARM_ARMED` in the process, and the var came to be read
/// also by the fusion path — the mutation leaked into parallel tests and
/// brought down a neighbor (`--test-threads=1` passed; the signature of global
/// state).
#[test]
fn declaracao_humana_vence_a_politica_mesmo_armada() {
    use crate::code_mode_gates::CodeModePresentation;
    use crate::code_mode_gates::arm::resolve_with_policy;
    // evidence that, by itself, would make the policy choose `code`
    let counts = [(20, 4), (20, 10), (20, 20)];
    assert_eq!(
        resolve_with_policy(Some(CodeModePresentation::Native), true, counts),
        CodeModePresentation::Native,
        "o `touring.toml` declarou `native`; a política não pode passar por cima"
    );
    // no declaration, armed and with evidence: the policy chooses
    assert_eq!(
        resolve_with_policy(None, true, counts),
        CodeModePresentation::Code
    );
    // no declaration and DISARMED: default, no signal read
    assert_eq!(
        resolve_with_policy(None, false, counts),
        CodeModePresentation::Both
    );
}

/// Thin evidence promotes nothing — the whole point of "measured promotion".
#[test]
fn programa_de_um_alvo_e_uma_operacao_nao_fundiu_nada() {
    use crate::code_mode_gates::arm::{ProgramShape, program_shape};
    assert_eq!(program_shape("cat /a/b/c.rs"), ProgramShape::Trivial);
    assert_eq!(
        program_shape("sed -n 1,20p /a/b/c.rs"),
        ProgramShape::Trivial
    );
}

#[test]
fn programa_que_toca_varios_alvos_ou_operacoes_e_fusao() {
    use crate::code_mode_gates::arm::{ProgramShape, program_shape};
    // three targets, one operation
    assert!(matches!(
        program_shape("cat /a/x.rs /a/y.rs /a/z.rs"),
        ProgramShape::Fused(n) if n >= 3
    ));
    // one target, three operations
    assert!(matches!(
        program_shape("grep a /f/g.rs; wc -l /f/g.rs; sed -n 1p /f/g.rs"),
        ProgramShape::Fused(n) if n >= 3
    ));
}

#[test]
fn a_aspa_que_fecha_e_a_primeira_nao_a_ultima() {
    use crate::code_mode_gates::arm::{ProgramShape, extract_run_body, program_shape};
    // The REAL case that broke the live measurement on 26/08: the outer command
    // carried more quotes after the body, and `rfind` swallowed the whole pipe.
    let cmd = "touring run --lang bash --code 'cat /a/b/Cargo.toml' 2>/dev/null \
               | python3 -c \"print('x')\"";
    assert_eq!(extract_run_body(cmd), Some("cat /a/b/Cargo.toml"));
    assert_eq!(
        program_shape(extract_run_body(cmd).unwrap()),
        ProgramShape::Trivial,
        "um arquivo, uma operação — o pipe externo não é parte do programa"
    );
}

/// `2>/dev/null` is shell plumbing, not an inspection target. Counting it
/// made a one-file program look like it touched two.
#[test]
fn dispositivos_nao_contam_como_alvo() {
    use crate::code_mode_gates::arm::{ProgramShape, program_shape};
    assert_eq!(
        program_shape("cat /a/b.rs 2>/dev/null"),
        ProgramShape::Trivial
    );
    assert_eq!(
        program_shape("cat /a/b.rs >/dev/null"),
        ProgramShape::Trivial
    );
}

#[test]
fn o_corpo_do_programa_e_extraido_de_ambas_as_aspas() {
    use crate::code_mode_gates::arm::extract_run_body;
    assert_eq!(
        extract_run_body("touring run --lang bash --code 'cat /a/b'"),
        Some("cat /a/b")
    );
    assert_eq!(
        extract_run_body("touring run --lang python --code \"print(1)\""),
        Some("print(1)")
    );
    // without --code there is nothing to classify — never guess
    assert_eq!(extract_run_body("touring run --file x.py"), None);
}

/// The point of P5b: obeying is not saving. A shell around a single call
/// is worth only half — and NOT zero, because under `code` the atomic one is denied and the
/// shell is mandatory: the cost belongs to the presentation, not to the model.
#[test]
fn casca_sobre_chamada_unica_vale_metade_da_rota_que_funde() {
    use crate::code_mode_gates::arm::classify_route_outcome;
    assert_eq!(
        classify_route_outcome("touring run --lang bash --code 'cat /a/b/c.rs'"),
        Some(0.5)
    );
    assert_eq!(
        classify_route_outcome(
            "touring run --lang bash --code 'grep a /f/g.rs; wc -l /f/h.rs; cat /f/i.rs'"
        ),
        Some(1.0)
    );
}

#[test]
#[serial_test::serial(gate_metrics)]
fn a_evidencia_do_braco_sobrevive_ao_processo() {
    use crate::code_mode_gates::arm::{ArmAxis, arm_counts_durable, bump_arm, read_arm_file};
    let proj = tmpdir("arm-duravel");
    std::fs::create_dir_all(&proj).expect("mkdir");

    for _ in 0..3 {
        bump_arm(&proj, "code", ArmAxis::Offered);
    }
    bump_arm(&proj, "code", ArmAxis::Followed);
    bump_arm(&proj, "code", ArmAxis::Economical);

    // DIRECT read of the file — no cache, which is what a new process
    // (restarted daemon) would do. In-memory counters went from 2 to 0
    // in two minutes today; the decision cannot depend on them.
    let do_disco = read_arm_file(&proj);
    assert_eq!(
        do_disco[2],
        (3, 1, 1),
        "o braço `code` tem de estar no disco"
    );
    assert_eq!(arm_counts_durable(&proj)[2], (3, 1));

    let _ = std::fs::remove_dir_all(&proj);
}

/// The durable (per-project file) and the volatile (process counter)
/// count the SAME decision.
///
/// **Serial by necessity, not by habit.** The `tmpdir` isolates the durable,
/// but `code_mode_arm_counts()` is a SINGLE process counter: the test
/// compares a global delta with a local value, and any other test that
/// calls `bump_arm` between the two reads breaks the equality. Measured on
/// 27/08: serial 458/458, parallel failing 1 in 3.
///
/// It is the exception to the structural isolation that the rest of this module uses (key
/// with `project_root`, see the S3 burst tests): there a key exists
/// that separates; here the resource is global by design, and pretending it is not would be
/// hiding the race instead of declaring it.
#[test]
#[serial_test::serial(gate_metrics)]
fn bump_arm_mantem_duravel_e_volatil_em_sincronia() {
    use crate::code_mode_gates::arm::{ArmAxis, bump_arm, read_arm_file};
    use touring_foundation::gate_metrics_snapshot::code_mode_arm_counts;
    let proj = tmpdir("arm-sincronia");
    std::fs::create_dir_all(&proj).expect("mkdir");

    // index 2 = `code` in the order (native, both, code).
    let antes = code_mode_arm_counts()[2];
    bump_arm(&proj, "code", ArmAxis::Offered);
    bump_arm(&proj, "code", ArmAxis::Offered);
    bump_arm(&proj, "code", ArmAxis::Followed);
    let depois = code_mode_arm_counts()[2];
    let disco = read_arm_file(&proj)[2];

    assert_eq!(disco.0, 2, "durável: 2 ofertas");
    assert_eq!(disco.1, 1, "durável: 1 tomada");
    // `>=`, not `==`, and the reason matters: the durable is PER PROJECT (isolated
    // by the tmpdir above) while the volatile is a SINGLE process
    // counter, incremented by any path that offers a route —
    // including the S3 burst tests, which produce denies. Requiring
    // equality was requiring EXCLUSIVITY over a shared resource, and
    // the test passed by scheduling luck (it failed 1 in 3).
    //
    // What the assertion protects still stands: if the volatile stopped
    // following the durable, the delta would be SMALLER than the disk and this fails.
    // The opposite case — the volatile moving more than the durable — is expected here
    // and is exactly what the equality could not distinguish from a bug.
    assert!(
        depois.0 - antes.0 >= disco.0,
        "o volátil não acompanhou o durável nas ofertas: {} < {}",
        depois.0 - antes.0,
        disco.0
    );
    assert!(
        depois.1 - antes.1 >= disco.1,
        "o volátil não acompanhou o durável nas tomadas: {} < {}",
        depois.1 - antes.1,
        disco.1
    );

    // Economy is durable-only by design: there is no corresponding atom,
    // so the volatile must NOT move when only the economy is recorded.
    let pre_eco = code_mode_arm_counts()[2];
    bump_arm(&proj, "code", ArmAxis::Economical);
    assert_eq!(
        code_mode_arm_counts()[2],
        pre_eco,
        "economia não tem par volátil; nada no gate-metrics pode mudar"
    );
    assert_eq!(read_arm_file(&proj)[2].2, 1, "economia foi ao disco");

    let _ = std::fs::remove_dir_all(&proj);
}

/// An unknown arm name cannot fall into a bucket: counting wrong is
/// worse than not counting, because the policy would believe the count.
#[test]
#[serial_test::serial(gate_metrics)]
fn braco_desconhecido_nao_e_contado() {
    use crate::code_mode_gates::arm::{ArmAxis, bump_arm, read_arm_file};
    let proj = tmpdir("arm-desconhecido");
    std::fs::create_dir_all(&proj).expect("mkdir");
    bump_arm(&proj, "agressivo", ArmAxis::Offered);
    assert_eq!(read_arm_file(&proj), [(0, 0, 0); 3]);
    let _ = std::fs::remove_dir_all(&proj);
}

/// Unreadable file ⇒ zeros ⇒ quiet policy. Evidence that cannot be
/// read is evidence that does not exist; inventing a choice would be worse.
#[test]
fn evidencia_corrompida_deixa_a_politica_calada() {
    use crate::code_mode_gates::arm::{arm_choice_from_counts, read_arm_file};
    let proj = tmpdir("arm-corrompido");
    std::fs::create_dir_all(proj.join(".claude/touring")).expect("mkdir");
    std::fs::write(proj.join(".claude/touring/code_mode_arm.json"), "{nao json").expect("write");
    assert_eq!(read_arm_file(&proj), [(0, 0, 0); 3]);
    assert_eq!(arm_choice_from_counts([(0, 0); 3]), None);
    let _ = std::fs::remove_dir_all(&proj);
}

#[test]
fn abaixo_do_piso_de_amostra_a_politica_se_cala() {
    use crate::code_mode_gates::arm::arm_choice_from_counts;
    assert_eq!(arm_choice_from_counts([(19, 19), (19, 0), (19, 5)]), None);
    assert_eq!(arm_choice_from_counts([(0, 0), (0, 0), (0, 0)]), None);
}

/// A single arm with a sample is not a choice: it is the only one observed.
/// Calling that learning would be confirming the current configuration.
#[test]
fn um_braco_sozinho_nao_conta_como_comparacao() {
    use crate::code_mode_gates::arm::arm_choice_from_counts;
    assert_eq!(arm_choice_from_counts([(100, 90), (3, 3), (0, 0)]), None);
}

#[test]
fn diferenca_dentro_do_ruido_nao_e_evidencia() {
    use crate::code_mode_gates::arm::arm_choice_from_counts;
    // 90% vs 88%: real difference, but smaller than the margin — silence.
    assert_eq!(arm_choice_from_counts([(0, 0), (100, 90), (100, 88)]), None);
    // An exact tie can never be broken by the vector order.
    assert_eq!(arm_choice_from_counts([(20, 10), (20, 10), (20, 10)]), None);
}

#[test]
fn com_dois_bracos_medidos_vence_a_maior_taxa() {
    use crate::code_mode_gates::CodeModePresentation;
    use crate::code_mode_gates::arm::arm_choice_from_counts;
    // native 50%, both 90% → both.
    assert_eq!(
        arm_choice_from_counts([(20, 10), (20, 18), (0, 0)]),
        Some(CodeModePresentation::Both)
    );
    // code 100% beats both 80% — a clear gap, not a margin boundary.
    // (An assertion exactly AT the threshold would depend on the IEEE754 representation
    // of the subtraction and would fail for a reason that has nothing to do with the policy.)
    assert_eq!(
        arm_choice_from_counts([(0, 0), (20, 16), (20, 20)]),
        Some(CodeModePresentation::Code)
    );
}

// ── P3 — replay harness of the research loop ──────────────────────────────────
//
// It is NOT an assertion: it is a deterministic EXPERIMENT over a FROZEN corpus
// (`eval/autoresearch/corpus.json`, extracted from real transcripts). It uses the
// REAL predicates (`scan_class_of`, `command_similarity`) and not a copy —
// a verifier that reimplemented the rule would measure the copy, not the gate.
//
// Runs on demand:
//   cargo test -p touring-cli --lib autoresearch_replay -- --ignored --nocapture
mod autoresearch_replay {
    use crate::code_mode_gates::{command_similarity, scan_class_of};

    /// A sweep per threshold over the frozen corpus.
    ///
    /// Simulates with LOCAL state (one map per session), never the process's
    /// global caches: an experiment that wrote to the live ledger would contaminate the
    /// system it measures.
    fn varre(sessoes: &[Vec<String>], limiar: f64) -> (usize, usize, f64) {
        let (mut inspecoes, mut colapsadas, mut soma_sim) = (0usize, 0usize, 0.0f64);
        for cmds in sessoes {
            let mut ultima_por_classe: std::collections::HashMap<&str, &str> =
                std::collections::HashMap::new();
            for cmd in cmds {
                let Some(classe) = scan_class_of(cmd) else {
                    continue;
                };
                inspecoes += 1;
                if let Some(anterior) = ultima_por_classe.get(classe) {
                    let sim = command_similarity(anterior, cmd);
                    if sim >= limiar {
                        colapsadas += 1;
                        soma_sim += sim;
                    }
                }
                ultima_por_classe.insert(classe, cmd);
            }
        }
        let media = if colapsadas > 0 {
            soma_sim / colapsadas as f64
        } else {
            0.0
        };
        (inspecoes, colapsadas, media)
    }

    #[test]
    #[ignore = "experimento sob demanda — precisa do corpus extraído"]
    fn varre_o_limiar_de_similaridade_sobre_corpus_congelado() {
        let raiz = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("eval/autoresearch/corpus.json");
        let Ok(txt) = std::fs::read_to_string(&raiz) else {
            println!(
                "CORPUS AUSENTE em {} — rode eval/autoresearch/extract_corpus.py",
                raiz.display()
            );
            return;
        };
        let v: serde_json::Value = serde_json::from_str(&txt).expect("corpus json");
        let sessoes: Vec<Vec<String>> = v["sessions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| {
                s.as_array()
                    .unwrap()
                    .iter()
                    .map(|c| c.as_str().unwrap_or("").to_string())
                    .collect()
            })
            .collect();

        println!("corpus: {} sessões, meta={}", sessoes.len(), v["meta"]);
        println!(
            "{:>7} {:>12} {:>12} {:>10}",
            "limiar", "inspeções", "colapsadas", "sim_média"
        );
        for passo in 0..=10 {
            let limiar = passo as f64 / 10.0;
            let (insp, col, media) = varre(&sessoes, limiar);
            let pct = if insp > 0 {
                100.0 * col as f64 / insp as f64
            } else {
                0.0
            };
            println!(
                "{limiar:>7.1} {insp:>12} {col:>12} {media:>10.3}   ({pct:.1}% das inspeções)"
            );
        }
        // The scalar that a campaign would read. DECLARED as a proxy: maximizing it
        // alone pushes the threshold to 0 (collapse everything), so the keep/discard
        // over it NEEDS a human gate — which is exactly the partition by the
        // verifier's frontier that the plan requires.
        let (_, col, _) = varre(&sessoes, crate::code_mode_gates::G1_SIMILARITY_AT);
        println!("METRIC={col}");
    }
}

// ── r3 shadow A2 (S-8, plan 2026-10-02-jev-harness-r3) ───────────────────────────
//
// The shadow only OBSERVES the lifecycle of the route offer. These tests prove the three
// obligations: flag OFF does nothing, the production decision is the same with the flag on
// (even when the emission FAILS), and each offer closes with ONE line. Each test runs
// on its own thread with a per-thread sink (`test_sink`) — never the real journal, never a
// global shared with the parallel neighbor.
mod r3_shadow_a2 {
    use crate::code_mode_gates::arm::r3_a2::{Registry, pending_for, test_sink};
    use crate::code_mode_gates::{claim_route_reward, record_route_offer, session_key};
    use serde_json::{Value, json};
    use std::cell::RefCell;
    use std::path::Path;
    use std::rc::Rc;
    use touring_foundation::code_mode::CodeModePresentation;
    use touring_hook_runtime::r3_shadow::{dropped_event_count, emit_r3_event_at, session_hash};

    type Lines = Rc<RefCell<Vec<Value>>>;

    /// A project dir whose `.touring/touring.toml` says `[shadow] r3 = <flag>` (`None` ⇒ no file).
    fn project(flag: Option<bool>) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        if let Some(on) = flag {
            let toml = dir.path().join(".touring/touring.toml");
            std::fs::create_dir_all(toml.parent().expect("parent")).expect("mkdir");
            std::fs::write(&toml, format!("[shadow]\nr3 = {on}\n")).expect("write toml");
        }
        dir
    }

    /// Every event this thread emits lands here instead of the real journal.
    fn capture() -> Lines {
        let lines: Lines = Rc::default();
        let sink = Rc::clone(&lines);
        test_sink::install(move |e| {
            sink.borrow_mut().push(e.clone());
            true
        });
        lines
    }

    /// An env var in the test environment would override the toml and void the premise.
    fn env_flag_is_set() -> bool {
        std::env::var_os(touring_hook_runtime::r3_shadow::ENV_FLAG).is_some()
    }

    /// These tests assume the shadow flag is NOT forced by the environment. Fail LOUDLY
    /// otherwise: a guard that returns early (`if env_flag_is_set() { return; }`) turns the
    /// observe-only proof into a green no-op when the shell carries `TOURING_R3_SHADOW`
    /// (cross-audit 03/10/2026, finding C: with the flag forced, 11/11 passed without testing).
    fn require_flag_unset() {
        assert!(
            !env_flag_is_set(),
            "run these tests with TOURING_R3_SHADOW unset"
        );
    }

    /// Production's own sequence: offer, then these PostToolUse commands (`""` = a tool
    /// without a command, e.g. a `Read`).
    const SEQ: [&str; 5] = [
        "",
        "ls -la",
        "touring run --lang bash --code 'ls'",
        "touring run --code 'x'",
        "TOURING_CODE_MODE=native cat /a/b",
    ];

    /// What production decided for each command: `(mode, value)` or `None`.
    fn drive(root: &Path, session: &str) -> Vec<Option<(String, f64)>> {
        let payload = json!({"session_id": session});
        record_route_offer(root, session, CodeModePresentation::Code);
        SEQ.iter()
            .map(|cmd| claim_route_reward(root, &payload, cmd).map(|(o, v)| (o.mode, v)))
            .collect()
    }

    #[test]
    fn r3_flag_off_does_no_io() {
        require_flag_unset();
        for flag in [None, Some(false)] {
            let root = project(flag);
            let lines = capture();
            drive(root.path(), "r3-off");
            assert!(
                lines.borrow().is_empty(),
                "flag {flag:?}: no line may be written"
            );
            assert!(
                !pending_for(root.path(), "r3-off"),
                "flag {flag:?}: the shadow must not even record the offer"
            );
        }
    }

    #[test]
    fn r3_closed_offer_emits_exactly_one_line() {
        require_flag_unset();
        let root = project(Some(true));
        let lines = capture();
        let payload = json!({"session_id": "r3-one-line"});
        record_route_offer(root.path(), "r3-one-line", CodeModePresentation::Code);
        assert!(lines.borrow().is_empty(), "opening an offer writes nothing");
        claim_route_reward(root.path(), &payload, "ls -la");
        assert!(
            lines.borrow().is_empty(),
            "a Bash without a verdict does not close it"
        );

        claim_route_reward(root.path(), &payload, "touring run --lang bash --code 'ls'");
        assert_eq!(lines.borrow().len(), 1, "closed ⇒ exactly one line");
        let e = lines.borrow()[0].clone();
        assert_eq!(e["kind"], "a2_offer_closed");
        assert_eq!(e["use_case"], "a2_outcome");
        assert_eq!(e["closed_by"], "consumed");
        assert_eq!(e["mode"], "code");
        assert_eq!(
            e["baseline_value"], 0.5,
            "the verdict production returned, verbatim"
        );
        assert_eq!(e["baseline_followed"], true);
        assert_eq!(
            e["cmd_heads"],
            json!(["ls -la", "touring run --lang bash --code 'ls'"])
        );
        assert_eq!(e["session_hash"], session_hash("r3-one-line").as_str());

        // Nothing is pending any more: the rest of the sequence adds no line.
        for cmd in SEQ {
            claim_route_reward(root.path(), &payload, cmd);
        }
        assert_eq!(lines.borrow().len(), 1, "a closed offer never emits twice");
    }

    #[test]
    fn r3_window_closes_after_k_bash_without_a_verdict() {
        require_flag_unset();
        let root = project(Some(true));
        let lines = capture();
        let payload = json!({"session_id": "r3-window"});
        record_route_offer(root.path(), "r3-window", CodeModePresentation::Both);
        for cmd in ["ls", "pwd"] {
            claim_route_reward(root.path(), &payload, cmd);
        }
        assert!(lines.borrow().is_empty(), "2 of K=3: still open");
        claim_route_reward(root.path(), &payload, "git status");
        assert_eq!(lines.borrow().len(), 1);
        let e = lines.borrow()[0].clone();
        assert_eq!(e["closed_by"], "window");
        assert_eq!(e["mode"], "both");
        assert!(e["baseline_value"].is_null(), "no verdict ⇒ null, never 0");
        assert_eq!(e["baseline_followed"], false);
        assert_eq!(e["cmd_heads"], json!(["ls", "pwd", "git status"]));

        // The shadow's window is NOT production's: `None` never consumed the offer, so a
        // `touring run` after the window is still credited — and adds no second line.
        let credited =
            claim_route_reward(root.path(), &payload, "touring run --lang bash --code 'ls'");
        assert!(credited.is_some(), "production still holds the offer");
        assert_eq!(lines.borrow().len(), 1);
    }

    #[test]
    fn r3_replaced_offer_closes_with_one_line() {
        require_flag_unset();
        let root = project(Some(true));
        let lines = capture();
        let payload = json!({"session_id": "r3-replaced"});
        record_route_offer(root.path(), "r3-replaced", CodeModePresentation::Code);
        claim_route_reward(root.path(), &payload, "ls");
        record_route_offer(root.path(), "r3-replaced", CodeModePresentation::Code);
        assert_eq!(
            lines.borrow().len(),
            1,
            "the replaced offer closes immediately"
        );
        let first = lines.borrow()[0].clone();
        assert_eq!(first["closed_by"], "replaced");
        assert!(first["baseline_value"].is_null());
        assert_eq!(first["cmd_heads"], json!(["ls"]));

        claim_route_reward(root.path(), &payload, "touring run --lang bash --code 'ls'");
        assert_eq!(lines.borrow().len(), 2, "the new offer closes on its own");
        assert_eq!(lines.borrow()[1]["closed_by"], "consumed");
        assert_eq!(
            lines.borrow()[1]["cmd_heads"],
            json!(["touring run --lang bash --code 'ls'"])
        );
    }

    #[test]
    fn r3_claim_route_reward_is_identical_with_the_flag_on() {
        require_flag_unset();
        let off = project(Some(false));
        let on = project(Some(true));
        let lines = capture();
        let without = drive(off.path(), "r3-same");
        let with = drive(on.path(), "r3-same");
        assert_eq!(without, with, "the shadow must not change one decision");
        assert!(
            !lines.borrow().is_empty(),
            "sanity: the ON run really did observe"
        );
        assert_eq!(
            without,
            vec![None, None, Some(("code".to_string(), 0.5)), None, None],
            "and those are production's own decisions"
        );
    }

    #[test]
    fn r3_emission_failure_never_changes_the_return() {
        require_flag_unset();
        let off = project(Some(false));
        let on = project(Some(true));
        // A REAL failing write — the emitter's own swallow-and-count path, not a stub:
        // `/proc` cannot hold the journal's parent directory.
        test_sink::install(|e| {
            emit_r3_event_at(Path::new("/proc/r3-shadow-probe/r3_events.jsonl"), e)
        });
        let before = dropped_event_count();
        let without = drive(off.path(), "r3-fail");
        let with = drive(on.path(), "r3-fail");
        assert_eq!(without, with, "a failed emission must not touch the return");
        assert!(
            dropped_event_count() > before,
            "the failure is counted, never silent"
        );
    }

    fn strings_in(v: &Value, out: &mut Vec<String>) {
        match v {
            Value::String(s) => out.push(s.clone()),
            Value::Array(a) => a.iter().for_each(|x| strings_in(x, out)),
            Value::Object(m) => m.values().for_each(|x| strings_in(x, out)),
            _ => {}
        }
    }

    #[test]
    fn r3_event_lines_never_carry_text_over_80_chars() {
        require_flag_unset();
        let root = project(Some(true));
        let lines = capture();
        let payload = json!({"session_id": "r3-long"});
        record_route_offer(root.path(), "r3-long", CodeModePresentation::Code);
        let long = format!("echo {}", "x".repeat(300));
        claim_route_reward(root.path(), &payload, &long);
        claim_route_reward(root.path(), &payload, "pwd");
        claim_route_reward(root.path(), &payload, &"ç".repeat(200));
        assert_eq!(lines.borrow().len(), 1);
        let mut all = Vec::new();
        strings_in(&lines.borrow()[0], &mut all);
        assert!(!all.is_empty());
        assert!(
            all.iter().all(|s| s.chars().count() <= 80),
            "S6: no string over 80 chars in any key: {all:?}"
        );
        assert_eq!(
            lines.borrow()[0]["cmd_heads"][0]
                .as_str()
                .map(|s| s.chars().count()),
            Some(80)
        );
    }

    #[test]
    fn r3_event_names_the_session_not_the_agent() {
        require_flag_unset();
        let root = project(Some(true));
        let lines = capture();
        let payload = json!({"session_id": "sess-a", "agent_id": "ag1"});
        let actor = session_key(&payload);
        assert!(
            actor.contains("agent:ag1"),
            "premise: the actor key carries the agent"
        );
        record_route_offer(root.path(), &actor, CodeModePresentation::Code);
        claim_route_reward(root.path(), &payload, "touring run --lang bash --code 'ls'");
        assert_eq!(lines.borrow().len(), 1);
        assert_eq!(
            lines.borrow()[0]["session_hash"],
            session_hash("sess-a").as_str()
        );
    }

    #[test]
    fn r3_actors_do_not_close_each_others_offers() {
        require_flag_unset();
        let root = project(Some(true));
        let lines = capture();
        record_route_offer(root.path(), "r3-a", CodeModePresentation::Code);
        record_route_offer(root.path(), "r3-b", CodeModePresentation::Code);
        claim_route_reward(
            root.path(),
            &json!({"session_id": "r3-a"}),
            "touring run --lang bash --code 'ls'",
        );
        assert_eq!(lines.borrow().len(), 1);
        assert_eq!(
            lines.borrow()[0]["session_hash"],
            session_hash("r3-a").as_str()
        );
        assert!(
            pending_for(root.path(), "r3-b"),
            "the other actor's offer is untouched"
        );
    }

    #[test]
    fn r3_pending_offers_are_capped_oldest_first() {
        let reg = Registry::new();
        let mut sink = |_: &Value| {};
        for i in 0..1030u64 {
            reg.offer(&format!("k{i}"), "s", "code", i, &mut sink);
        }
        assert_eq!(reg.len(), 1024, "the registry never grows past its cap");
        assert!(!reg.is_pending("k0"), "the oldest went first");
        assert!(reg.is_pending("k1029"), "the newest is kept");
    }

    /// Cross-audit 02/10/2026 (finding 7): the capacity eviction used to `map.remove` the oldest offer WITHOUT a
    /// line, so S3 ("every eligible offer leaves exactly one line") silently undercounted past 1024 pending offers.
    #[test]
    fn r3_capacity_eviction_still_leaves_one_line_per_offer() {
        let reg = Registry::new();
        let lines = std::cell::RefCell::new(Vec::<Value>::new());
        let mut sink = |e: &Value| lines.borrow_mut().push(e.clone());
        for i in 0..1030u64 {
            reg.offer(&format!("k{i}"), "s", "code", i, &mut sink);
        }
        let lines = lines.into_inner();
        // 1030 offers, 1024 still pending: the 6 oldest were displaced, and EACH of them closed exactly one line.
        assert_eq!(
            lines.len(),
            6,
            "every displaced offer must leave one line (S3 counts lines)"
        );
        assert!(
            lines
                .iter()
                .all(|e| e["kind"] == "a2_offer_closed" && e["closed_by"] == "replaced"),
            "a displaced offer closes as `replaced` (a newer offer took its place in the registry)"
        );
        assert_eq!(reg.len(), 1024);
    }
}
