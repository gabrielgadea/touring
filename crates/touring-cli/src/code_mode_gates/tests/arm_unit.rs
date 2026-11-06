//! The A/B arm and its evidence — route offers, durable counts, the policy, the r3 shadow —
//! moved verbatim with the code (they are about the arm, not about gate verdicts).

use super::support::tmpdir;

/// R2 (29/08): a evidência durável acumula em DISCO — 2 bumps somam, uma
/// família nova coexiste, arquivo corrompido recomeça de zeros sem panic
/// (evidência ilegível é evidência inexistente, jamais um valor inventado).
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
    // corrupção: leitura volta objeto vazio; o próximo bump recomeça de 1
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

/// O predicado puro: closed ou sem 1ª → FirstPass; turno aberto com 1ª →
/// Fold acumulando o comando. Separar decisão de estado é o que deixa o
/// gate mais fácil de testar do que de contornar.
#[test]
fn rota_tomada_e_rota_recusada_tem_vereditos_opostos() {
    use crate::code_mode_gates::arm::classify_route_outcome;
    // Rodar o programa É a rota. O VALOR carrega a economia (P5b): uma casca
    // sobre uma operação vale 0.5; o que importa aqui é que é positivo,
    // enquanto recusar vale 0.0.
    assert_eq!(
        classify_route_outcome("touring run --lang bash --code 'ls'"),
        Some(0.5)
    );
    assert_eq!(
        classify_route_outcome("touring run --lang bash --code 'cat /a/x.rs /a/y.rs /a/z.rs'"),
        Some(1.0),
        "o programa que funde vale a rota inteira"
    );
    // Relaxar o gate é recusá-la.
    assert_eq!(
        classify_route_outcome("TOURING_CODE_MODE=native grep -rn foo ."),
        Some(0.0)
    );
    assert_eq!(classify_route_outcome("TOURING_GATE_OK=1 cat x"), Some(0.0));
}

/// A falha que isto pega: ler "fez outra coisa" como fracasso. Ausência de
/// sinal é desconhecido — creditar 0.0 aqui puniria o braço por silêncio.
#[test]
fn desfecho_desconhecido_nao_vira_veredito() {
    use crate::code_mode_gates::arm::classify_route_outcome;
    assert_eq!(classify_route_outcome("cargo test -p touring-cli"), None);
    assert_eq!(classify_route_outcome(""), None);
}

/// Ordem: ter relaxado o gate no caminho não desfaz ter rodado o programa.
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

/// A falha que isto pega — e que existiu por alguns minutos hoje: um
/// PostToolUse de ferramenta SEM comando (um `Read`) consumia a oferta sem
/// veredito, e a rota tomada logo depois já não achava nada para creditar.
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

    // Um `Read` não carrega comando: nada a classificar.
    assert!(claim_route_reward(raiz, &payload, "").is_none());
    // …e a oferta TEM de continuar lá para o veredito real.
    let (offer, value) = claim_route_reward(raiz, &payload, "touring run --lang bash --code 'ls'")
        .expect("a oferta sobreviveu ao desfecho ilegível");
    assert_eq!(offer.mode, "code");
    assert_eq!(
        value, 0.5,
        "programa de um alvo: rota tomada, economia nula"
    );
    // E agora sim ela some.
    assert!(claim_route_reward(raiz, &payload, "touring run --code 'x'").is_none());
}

/// Uma oferta reivindicada some: dois leitores nunca creditam a mesma
/// decisão duas vezes (a disciplina dos ledgers de decisão e de casos).
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

/// A invariante que protege Gabriel: a política preenche o espaço que o
/// humano deixou aberto — nunca sobrescreve o que ele declarou. Uma
/// política que vencesse o `touring.toml` não estaria aprendendo, estaria
/// desobedecendo.
///
/// Testado no PREDICADO PURO: a versão anterior mutava
/// `TOURING_CODE_MODE_ARM_ARMED` no processo, e a var passou a ser lida
/// também pelo caminho da fusão — a mutação vazava para testes paralelos e
/// derrubava um vizinho (`--test-threads=1` passava; a assinatura de estado
/// global).
#[test]
fn declaracao_humana_vence_a_politica_mesmo_armada() {
    use crate::code_mode_gates::CodeModePresentation;
    use crate::code_mode_gates::arm::resolve_with_policy;
    // evidência que, sozinha, faria a política escolher `code`
    let counts = [(20, 4), (20, 10), (20, 20)];
    assert_eq!(
        resolve_with_policy(Some(CodeModePresentation::Native), true, counts),
        CodeModePresentation::Native,
        "o `touring.toml` declarou `native`; a política não pode passar por cima"
    );
    // sem declaração, armada e com evidência: a política escolhe
    assert_eq!(
        resolve_with_policy(None, true, counts),
        CodeModePresentation::Code
    );
    // sem declaração e DESARMADA: default, nenhum sinal lido
    assert_eq!(
        resolve_with_policy(None, false, counts),
        CodeModePresentation::Both
    );
}

/// Evidência fina não promove nada — o ponto inteiro de "promoção medida".
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
    // três alvos, uma operação
    assert!(matches!(
        program_shape("cat /a/x.rs /a/y.rs /a/z.rs"),
        ProgramShape::Fused(n) if n >= 3
    ));
    // um alvo, três operações
    assert!(matches!(
        program_shape("grep a /f/g.rs; wc -l /f/g.rs; sed -n 1p /f/g.rs"),
        ProgramShape::Fused(n) if n >= 3
    ));
}

#[test]
fn a_aspa_que_fecha_e_a_primeira_nao_a_ultima() {
    use crate::code_mode_gates::arm::{ProgramShape, extract_run_body, program_shape};
    // O caso REAL que quebrou a medição ao vivo em 26/08: o comando externo
    // trazia mais aspas depois do corpo, e `rfind` engolia o pipe inteiro.
    let cmd = "touring run --lang bash --code 'cat /a/b/Cargo.toml' 2>/dev/null \
               | python3 -c \"print('x')\"";
    assert_eq!(extract_run_body(cmd), Some("cat /a/b/Cargo.toml"));
    assert_eq!(
        program_shape(extract_run_body(cmd).unwrap()),
        ProgramShape::Trivial,
        "um arquivo, uma operação — o pipe externo não é parte do programa"
    );
}

/// `2>/dev/null` é plumbing do shell, não um alvo de inspeção. Contá-lo
/// fazia um programa de um arquivo parecer que tocava dois.
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
    // sem --code não há o que classificar — jamais adivinhar
    assert_eq!(extract_run_body("touring run --file x.py"), None);
}

/// O ponto do P5b: obedecer não é economizar. Uma casca sobre uma chamada
/// só vale metade — e NÃO zero, porque sob `code` a atômica é negada e a
/// casca é obrigatória: o custo é da apresentação, não do modelo.
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

    // Leitura DIRETA do arquivo — sem cache, que é o que um processo novo
    // (daemon reiniciado) faria. Contadores em memória zeraram de 2 para 0
    // em dois minutos hoje; a decisão não pode depender deles.
    let do_disco = read_arm_file(&proj);
    assert_eq!(
        do_disco[2],
        (3, 1, 1),
        "o braço `code` tem de estar no disco"
    );
    assert_eq!(arm_counts_durable(&proj)[2], (3, 1));

    let _ = std::fs::remove_dir_all(&proj);
}

/// O durável (arquivo por projeto) e o volátil (contador de processo)
/// contam a MESMA decisão.
///
/// **Serial por necessidade, não por hábito.** O `tmpdir` isola o durável,
/// mas `code_mode_arm_counts()` é um contador ÚNICO do processo: o teste
/// compara um delta global com um valor local, e qualquer outro teste que
/// chame `bump_arm` entre as duas leituras quebra a igualdade. Medido em
/// 27/08: serial 458/458, paralelo falhando 1 em 3.
///
/// É a exceção ao isolamento estrutural que o resto deste módulo usa (chave
/// com `project_root`, ver os testes de rajada do S3): ali existe uma chave
/// que separa; aqui o recurso é global por design, e fingir que não é seria
/// esconder a corrida em vez de declará-la.
#[test]
#[serial_test::serial(gate_metrics)]
fn bump_arm_mantem_duravel_e_volatil_em_sincronia() {
    use crate::code_mode_gates::arm::{ArmAxis, bump_arm, read_arm_file};
    use touring_foundation::gate_metrics_snapshot::code_mode_arm_counts;
    let proj = tmpdir("arm-sincronia");
    std::fs::create_dir_all(&proj).expect("mkdir");

    // índice 2 = `code` na ordem (native, both, code).
    let antes = code_mode_arm_counts()[2];
    bump_arm(&proj, "code", ArmAxis::Offered);
    bump_arm(&proj, "code", ArmAxis::Offered);
    bump_arm(&proj, "code", ArmAxis::Followed);
    let depois = code_mode_arm_counts()[2];
    let disco = read_arm_file(&proj)[2];

    assert_eq!(disco.0, 2, "durável: 2 ofertas");
    assert_eq!(disco.1, 1, "durável: 1 tomada");
    // `>=`, não `==`, e a razão importa: o durável é POR PROJETO (isolado
    // pelo tmpdir acima) enquanto o volátil é um contador ÚNICO do
    // processo, incrementado por qualquer caminho que ofereça uma rota —
    // inclusive os testes de rajada do S3, que produzem denies. Exigir
    // igualdade era exigir EXCLUSIVIDADE sobre um recurso compartilhado, e
    // o teste passava por sorte de escalonamento (falhava 1 em 3).
    //
    // O que a asserção protege continua de pé: se o volátil deixasse de
    // acompanhar o durável, o delta seria MENOR que o disco e isto reprova.
    // O caso oposto — o volátil andar mais que o durável — é esperado aqui
    // e é justamente o que a igualdade não sabia distinguir de um bug.
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

    // Economia é durável-only por design: não existe átomo correspondente,
    // então o volátil NÃO pode mexer quando só a economia é registrada.
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

/// Um nome de braço desconhecido não pode cair num balde: contar errado é
/// pior que não contar, porque a política acreditaria na contagem.
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

/// Arquivo ilegível ⇒ zeros ⇒ política calada. Evidência que não se pode
/// ler é evidência que não existe; inventar uma escolha seria pior.
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

/// Um único braço com amostra não é uma escolha: é o único observado.
/// Chamar isso de aprendizado seria confirmar a configuração vigente.
#[test]
fn um_braco_sozinho_nao_conta_como_comparacao() {
    use crate::code_mode_gates::arm::arm_choice_from_counts;
    assert_eq!(arm_choice_from_counts([(100, 90), (3, 3), (0, 0)]), None);
}

#[test]
fn diferenca_dentro_do_ruido_nao_e_evidencia() {
    use crate::code_mode_gates::arm::arm_choice_from_counts;
    // 90% vs 88%: diferença real, mas menor que a margem — silêncio.
    assert_eq!(arm_choice_from_counts([(0, 0), (100, 90), (100, 88)]), None);
    // Empate exato jamais pode desempatar pela ordem do vetor.
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
    // code 100% supera both 80% — folga clara, não fronteira de margem.
    // (Uma asserção exatamente NO limiar dependeria da representação IEEE754
    // da subtração e falharia por motivo nenhum a ver com a política.)
    assert_eq!(
        arm_choice_from_counts([(0, 0), (20, 16), (20, 20)]),
        Some(CodeModePresentation::Code)
    );
}

// ── P3 — harness de replay do research loop ──────────────────────────────────
//
// NÃO é uma asserção: é um EXPERIMENTO determinístico sobre corpus CONGELADO
// (`eval/autoresearch/corpus.json`, extraído de transcripts reais). Usa os
// predicados REAIS (`scan_class_of`, `command_similarity`) e não uma cópia —
// um verificador que reimplementasse a regra mediria a cópia, não o gate.
//
// Roda sob demanda:
//   cargo test -p touring-cli --lib autoresearch_replay -- --ignored --nocapture
mod autoresearch_replay {
    use crate::code_mode_gates::{command_similarity, scan_class_of};

    /// Uma varredura por limiar sobre o corpus congelado.
    ///
    /// Simula com estado LOCAL (um mapa por sessão), nunca os caches globais do
    /// processo: um experimento que escrevesse no ledger vivo contaminaria o
    /// sistema que ele mede.
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
        // O escalar que uma campanha leria. DECLARADO como proxy: maximizá-lo
        // sozinho empurra o limiar para 0 (colapsar tudo), então o keep/discard
        // sobre ele PRECISA de gate humano — que é exatamente a partição pela
        // fronteira do verificador que o plano exige.
        let (_, col, _) = varre(&sessoes, crate::code_mode_gates::G1_SIMILARITY_AT);
        println!("METRIC={col}");
    }
}

// ── r3 shadow A2 (S-8, plan 2026-10-02-jev-harness-r3) ───────────────────────────
//
// O shadow só OBSERVA o ciclo de vida da oferta de rota. Estes testes provam as três
// obrigações: flag OFF não faz nada, a decisão de produção é a mesma com a flag ligada
// (inclusive quando a emissão FALHA), e cada oferta fecha com UMA linha. Cada teste roda
// na própria thread com um sink por-thread (`test_sink`) — nunca o journal real, nunca um
// global compartilhado com o vizinho paralelo.
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
