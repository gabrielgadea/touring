//! The A/B arm and its evidence: route offers, durable counts, and the policy that — only when
//! ARMED, and never over a human declaration — picks the presentation the evidence favours.

use serde_json::Value;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

use super::*;

pub(in crate::code_mode_gates) mod r3_a2;

/// A rota escrita que a apresentação entregou ao modelo, com o braço que a produziu.
///
/// Contadores de FREQUÊNCIA dizem com que assiduidade um gate age e nada sobre
/// se agir funcionou — telemetria sem consumidor de aprendizado (medido
/// 25/08/2026: zero políticas os liam; os contadores do T3-B eram o caso
/// exemplar, e o gate inteiro saiu no S10). Esta é a metade que faltava: a
/// apresentação vigente vira o braço, e o comando seguinte vira a recompensa.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteOffer {
    /// `native` | `code` | `both` — a apresentação em vigor quando a rota saiu.
    pub mode: String,
    /// Quando foi oferecida (secs desde o UNIX_EPOCH).
    pub offered_secs: u64,
}


pub(super) fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}


/// O que um programa de code mode COMPROU em round-trips.
///
/// `adoption_ratio` responde "usou a ferramenta certa?" e nada sobre "a chamada
/// valia um round-trip?" — mede o CANAL, não a ECONOMIA (achado de Gabriel,
/// 26/08/2026: `scan_class_of` não reconhece `touring run`, então N programas
/// diferentes e triviais somam N adoções e zero avisos).
///
/// Um programa que inspeciona UM alvo com UMA operação não fundiu nada: é uma
/// chamada atômica com uma casca. Isso não é indisciplina do modelo — sob a
/// apresentação `code` a chamada atômica é NEGADA, então a casca é obrigatória.
/// Por isso a economia é lida como CUSTO DA APRESENTAÇÃO e não como falta do
/// modelo: é o número que impede o braço `code` de parecer ótimo só porque
/// todos obedecem, quando cada obediência custa um round-trip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProgramShape {
    /// Uma operação sobre um alvo — a casca não comprou nada.
    Trivial,
    /// Fundiu `n` operações/alvos numa chamada só.
    Fused(usize),
}


/// Verbos de inspeção reconhecidos no corpo, em qualquer linguagem.
pub(super) const INSPECTION_VERBS: [&str; 12] = [
    "grep", "rg", "cat", "head", "tail", "sed", "awk", "ls", "wc", "find", "stat", "jq",
];


/// Extrai o corpo de `touring run … --code '<corpo>'`.
///
/// Sem o corpo não há o que classificar, e `None` mantém o comportamento
/// anterior — jamais adivinhar a forma de um programa que não se leu.
pub(crate) fn extract_run_body(cmd: &str) -> Option<&str> {
    let idx = cmd.find("--code")?;
    let resto = &cmd[idx + "--code".len()..];
    let resto = resto.trim_start();
    let aspa = resto.chars().next()?;
    if aspa != '\'' && aspa != '"' {
        return None;
    }
    let corpo = &resto[aspa.len_utf8()..];
    // A PRIMEIRA aspa igual fecha, não a última. `rfind` capturava além do
    // corpo quando o comando externo trazia mais aspas depois — medido ao vivo
    // em 26/08: `… --code 'cat X' 2>/dev/null | python3 -c "…print('…')"`
    // devolvia um corpo que incluía o pipe inteiro, e o `/dev/null` de dentro
    // dele contava como um segundo alvo: um programa trivial era classificado
    // como fusão. Em shell, string entre aspas simples não contém a própria
    // aspa, então a primeira É a de fechamento.
    let fim = corpo.find(aspa)?;
    Some(&corpo[..fim])
}


/// Classifica o corpo pelo que ele funde. Puro — mais fácil de testar do que
/// de contornar.
///
/// Dois eixos, e o MAIOR decide: quantas operações de inspeção o corpo executa,
/// e quantos alvos distintos ele toca. Um `grep` sobre três arquivos fundiu
/// três; três `sed` sobre o mesmo arquivo fundiram três. Um de cada não fundiu
/// nada.
pub(crate) fn program_shape(body: &str) -> ProgramShape {
    // Casar PALAVRA, não substring: `body.matches("rg")` conta 1 dentro de
    // "Cargo.toml", e `cat /a/Cargo.toml` — um arquivo, uma operação — era
    // classificado como fusão de duas. O teste pegou; a medição ao vivo teria
    // levado semanas para revelar, porque o número errado é plausível.
    let ops = body
        .split(|c: char| c.is_whitespace() || matches!(c, ';' | '|' | '&' | '(' | ')' | '"' | '\''))
        .filter(|t| INSPECTION_VERBS.contains(&t.trim_start_matches("$(")))
        .count()
        // `open(` é chamada de função (Python), não token isolado.
        + body.matches("open(").count();
    let alvos: std::collections::BTreeSet<&str> = body
        .split(|c: char| c.is_whitespace() || c == '"' || c == '\'' || c == '(' || c == ')')
        .filter(|t| t.contains('/') && t.len() > 3 && !t.starts_with('-'))
        // Dispositivos e descritores não são alvos de inspeção: `2>/dev/null`
        // é plumbing do shell, e contá-lo fazia um programa de um arquivo
        // parecer que tocava dois (medido ao vivo em 26/08).
        .filter(|t| {
            !t.trim_start_matches(['>', '<', '&', '1', '2'])
                .starts_with("/dev/")
        })
        .collect();
    let fundido = ops.max(alvos.len());
    if fundido <= 1 {
        ProgramShape::Trivial
    } else {
        ProgramShape::Fused(fundido)
    }
}


/// O que o modelo fez com a rota que recebeu — puro, para ser mais fácil de
/// testar do que de contornar.
///
/// `Some(1.0)` rodou o programa; `Some(0.0)` recusou a rota com um token de
/// relaxamento; `None` fez outra coisa — e isso NÃO é veredito. Ausência de
/// sinal é desconhecido, jamais zero: a mesma leitura fail-closed que o nó
/// `loop` aplica a `NEW_FINDINGS`.
///
/// A ordem importa: `TOURING_CODE_MODE=native touring run …` **rodou** o
/// programa. Ter relaxado o gate no caminho não desfaz o fato de a rota ter
/// sido tomada.
pub fn classify_route_outcome(cmd: &str) -> Option<f64> {
    // `TOURING_T3_FUSE_DISABLED=1` saiu com o T3-B (S10, 27/08): reconhecer um
    // kill switch que não desliga nada ensinaria o modelo a digitá-lo.
    const BYPASS: [&str; 3] = [
        "TOURING_CODE_MODE=native",
        "TOURING_GATE_OK=1",
        "TOURING_CODE_GATES_DISABLED=1",
    ];
    if cmd.contains("touring run ") || cmd.contains("touring exec ") {
        // A rota foi tomada — mas COMPROU alguma coisa? Um programa que
        // inspeciona um alvo com uma operação é uma chamada atômica com casca:
        // o canal está certo e a economia é nula. Creditar 1.0 aqui ensinaria
        // ao braço que `code` é ótimo porque todos obedecem, quando cada
        // obediência custou um round-trip (achado de Gabriel, 26/08).
        //
        // 0.5 e não 0.0: sob `code` a chamada atômica é NEGADA, então a casca é
        // obrigatória. Punir como recusa culparia o modelo pela política — o
        // custo é da apresentação, e é dela que o número tem de falar.
        return Some(match extract_run_body(cmd).map(program_shape) {
            Some(ProgramShape::Trivial) => 0.5,
            _ => 1.0,
        });
    }
    if BYPASS.iter().any(|t| cmd.contains(t)) {
        return Some(0.0);
    }
    None
}


/// Nome estável do braço. É a chave dos contadores e da memória — mudá-lo
/// renomeia o braço e zera a evidência acumulada sem avisar ninguém.
pub(crate) fn presentation_label(p: CodeModePresentation) -> &'static str {
    p.label()
}


/// Registra que a apresentação acabou de entregar uma rota escrita.
///
/// Escritor ÚNICO do campo `route`: dois sítios gravando a mesma oferta seriam
/// duas versões da mesma decisão, e a primeira divergência entre eles só
/// apareceria como um braço aprendendo o oposto do que aconteceu — a classe de
/// `comentario-afirma-simetria-inexistente`.
pub(crate) fn record_route_offer(project_root: &Path, session: &str, mode: CodeModePresentation) {
    let label = presentation_label(mode);
    let key = format!("{}\u{2}{session}", project_root.display());
    let offered_secs = now_secs();
    route_offers().insert(
        key,
        RouteOffer {
            mode: label.to_string(),
            offered_secs,
        },
    );
    // `bump_arm` é o escritor único: ele grava a vista durável E a volátil.
    bump_arm(project_root, label, ArmAxis::Offered);
    // r3 shadow (observe-only, default OFF): fecha como `replaced` a oferta que ainda
    // estava pendente nesta sessão e abre a nova. Nada aqui altera o que foi gravado acima.
    r3_a2::on_offer(project_root, session, label, offered_secs);
}


/// Onde vive a evidência DURÁVEL do braço, por projeto.
///
/// Medido em 26/08/2026: `t3_turn_first_passed_count` caiu de 2 para 0 em dois
/// minutos, sem nada acontecer além de um restart de daemon. Os contadores de
/// `gate-metrics` são telemetria de processo — e uma política com piso de
/// amostra que lesse dali NUNCA alcançaria o piso entre deploys: existiria,
/// estaria armada, e não escolheria nada. Exatamente o modo de falha que este
/// plano inteiro cataloga. A decisão lê disco; a telemetria segue observando.
/// Ofertas de rota pendentes, por (projeto, sessão).
///
/// Cache PRÓPRIO e não um campo do `TurnBurst`: guardá-la lá fazia
/// `record_route_offer` chamar `get(...).unwrap_or_default()` num deny
/// code-mode onde o turno muitas vezes NÃO existe — e o default tem
/// `first_passed = false`, então a oferta inseria um turno falso e a fusão T3
/// seguinte nunca disparava. Os testes pegaram isso como vítima alternando
/// entre dois vizinhos, a assinatura de estado global compartilhado.
///
/// A separação também é semântica: a oferta sobrevive ao `turn_gate_close` de
/// propósito (o fim do turno é quando o desfecho fica observável, não quando se
/// esquece dele) — sinal de que nunca foi estado de turno.
pub(super) fn route_offers() -> &'static moka::sync::Cache<String, RouteOffer> {
    static C: OnceLock<moka::sync::Cache<String, RouteOffer>> = OnceLock::new();
    C.get_or_init(|| {
        moka::sync::Cache::builder()
            .max_capacity(1024)
            .time_to_live(Duration::from_secs(60))
            .build()
    })
}


pub(super) fn arm_store_path(project_root: &Path) -> std::path::PathBuf {
    project_root.join(".claude/touring/code_mode_arm.json")
}


/// Evidência DURÁVEL das famílias de medição sem arquivo próprio — S3 (rajada
/// de inspeção) e pillar-induction. R2 (29/08, ordem de Gabriel): os KPIs
/// `inspect_burst_share` e `pillar_induction_ratio` liam contadores de
/// processo que zeram a cada restart do daemon (3 deploys num dia = 3 apagões
/// da amostra) — a mesma classe que fez a decisão do braço ler
/// `code_mode_arm.json`. Mesmo contrato do arm: leitura ilegível ⇒ zeros (o
/// consumidor fica calado), falha de escrita ⇒ warn, nunca silêncio.
pub(super) fn durable_evidence_path(project_root: &Path) -> std::path::PathBuf {
    project_root.join(".claude/touring/durable_gate_evidence.json")
}


/// O arquivo inteiro como JSON; ausente/corrompido ⇒ objeto vazio (evidência
/// que não se pode ler é evidência que não existe).
pub(crate) fn read_durable_evidence(project_root: &Path) -> serde_json::Value {
    std::fs::read_to_string(durable_evidence_path(project_root))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| serde_json::json!({}))
}


/// `+1` em `<family>.<field>`, read-modify-write no arquivo durável. Chamado
/// ao lado do contador volátil correspondente — o vivo alimenta o
/// `gate-metrics` da sessão, o durável alimenta o KPI que precisa de DIAS.
pub(crate) fn bump_durable_evidence(project_root: &Path, family: &str, field: &str) {
    let mut v = read_durable_evidence(project_root);
    let Some(obj) = v.as_object_mut() else { return };
    let fam = obj
        .entry(family.to_string())
        .or_insert_with(|| serde_json::json!({}));
    if !fam.is_object() {
        *fam = serde_json::json!({});
    }
    let Some(f) = fam.as_object_mut() else { return };
    let n = f.get(field).and_then(Value::as_u64).unwrap_or(0);
    f.insert(field.to_string(), serde_json::json!(n + 1));
    let path = durable_evidence_path(project_root);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(&path, v.to_string()) {
        tracing::warn!(
            target: "touring::kpi",
            "durable gate evidence write failed at {}: {e}",
            path.display()
        );
    }
}


/// TTL curto: `code_mode_presentation` roda em todo PreToolUse, e ler o arquivo
/// a cada chamada trocaria um problema de durabilidade por um de latência.
pub(super) fn arm_cache() -> &'static moka::sync::Cache<String, ArmCounts> {
    static C: OnceLock<moka::sync::Cache<String, ArmCounts>> = OnceLock::new();
    C.get_or_init(|| {
        moka::sync::Cache::builder()
            .max_capacity(64)
            .time_to_live(Duration::from_secs(5))
            .build()
    })
}


pub(super) const ARM_NAMES: [&str; 3] = ["native", "both", "code"];


pub(super) fn arm_index(mode: &str) -> Option<usize> {
    ARM_NAMES.iter().position(|n| *n == mode)
}


/// Lê o arquivo. Ausente, ilegível ou corrompido ⇒ zeros: evidência que não se
/// pode ler é evidência que não existe, e zeros deixam a política calada (que é
/// o comportamento seguro), em vez de inventar uma escolha.
/// `(oferecidas, tomadas, economicas)` por braço.
///
/// Três números e não dois porque obediência e economia são eixos DIFERENTES:
/// `tomadas/oferecidas` responde "seguiram a rota?" (o canal) e
/// `economicas/tomadas` responde "a rota comprou round-trips?" (a economia).
/// Fundi-los num só faria um braço obedecido e caro parecer excelente — o
/// defeito que Gabriel apontou na `adoption_ratio`.
pub(crate) type ArmCounts = [(u64, u64, u64); 3];


pub(super) fn read_arm_file(project_root: &Path) -> ArmCounts {
    let mut out = [(0u64, 0u64, 0u64); 3];
    let Ok(txt) = std::fs::read_to_string(arm_store_path(project_root)) else {
        return out;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&txt) else {
        return out;
    };
    for (i, nome) in ARM_NAMES.iter().enumerate() {
        if let Some(par) = v.get(*nome) {
            out[i].0 = par.get("offered").and_then(Value::as_u64).unwrap_or(0);
            out[i].1 = par.get("followed").and_then(Value::as_u64).unwrap_or(0);
            out[i].2 = par.get("economical").and_then(Value::as_u64).unwrap_or(0);
        }
    }
    out
}


pub(super) fn write_arm_file(project_root: &Path, counts: &ArmCounts) {
    let path = arm_store_path(project_root);
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let mut obj = serde_json::Map::new();
    for (i, nome) in ARM_NAMES.iter().enumerate() {
        obj.insert(
            (*nome).to_string(),
            serde_json::json!({
                "offered": counts[i].0,
                "followed": counts[i].1,
                "economical": counts[i].2,
            }),
        );
    }
    // Falha de escrita NÃO pode ser silenciosa. Se o diretório não for
    // gravável, a evidência fica em zero para sempre e a política se cala —
    // indistinguível de "ainda não há amostra". Um `warn` é a diferença entre
    // um bug diagnosticável e mais um mecanismo que existe e nunca dispara.
    if let Err(e) = std::fs::write(&path, serde_json::Value::Object(obj).to_string()) {
        tracing::warn!(
            path = %path.display(),
            error = %e,
            "P2: evidência do braço code-mode não pôde ser persistida — a política \
             ficará calada e isso NÃO é ausência de amostra"
        );
    }
}


/// Evidência durável do projeto, com cache curto.
pub(crate) fn arm_counts_full(project_root: &Path) -> ArmCounts {
    let key = project_root.display().to_string();
    if let Some(c) = arm_cache().get(&key) {
        return c;
    }
    let counts = read_arm_file(project_root);
    arm_cache().insert(key, counts);
    counts
}


/// O par `(oferecidas, tomadas)` que a POLÍTICA lê. A economia é reportada
/// (KPI) mas ainda não escolhe braço: mudar o critério de escolha é decisão de
/// Gabriel, não efeito colateral de ter passado a medir.
pub(crate) fn arm_counts_durable(project_root: &Path) -> [(u64, u64); 3] {
    let full = arm_counts_full(project_root);
    [
        (full[0].0, full[0].1),
        (full[1].0, full[1].1),
        (full[2].0, full[2].1),
    ]
}


/// Incrementa um eixo do braço e persiste.
///
/// **Orçamento**: este arquivo declara `< 1ms (scan de bytes + moka)` para o
/// caminho quente, e aqui há I/O de disco. Os dois únicos chamadores estão
/// DENTRO de ramos de deny (a fusão T3 e o deny code-mode), então a leitura +
/// escrita de ~200 bytes é paga só quando um gate age — nunca nas chamadas que
/// passam, que são a esmagadora maioria. O orçamento do caminho comum segue
/// intacto; quem paga é o evento raro que produz a evidência.
///
/// Relê o arquivo antes de somar (não o cache): dois daemons per-project ou
/// duas sessões no mesmo projeto somariam sobre uma cópia velha e uma das duas
/// contagens sumiria. Continua havendo uma janela de corrida entre a leitura e
/// a escrita — aceitável para um contador de evidência, e declarada aqui em vez
/// de descoberta depois como número que não bate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ArmAxis {
    /// A apresentação entregou uma rota.
    Offered,
    /// O modelo tomou a rota (o canal).
    Followed,
    /// A rota tomada fundiu round-trips (a economia).
    Economical,
}


pub(crate) fn bump_arm(project_root: &Path, mode: &str, eixo: ArmAxis) {
    let Some(i) = arm_index(mode) else { return };
    let mut counts = read_arm_file(project_root);
    // Escritor ÚNICO da contagem do braço. A evidência DURÁVEL (o arquivo, que
    // sobrevive ao restart e alimenta a política) e a VOLÁTIL (os átomos que
    // `gate-metrics -j` publica) são duas VISTAS DA MESMA decisão — não dois
    // registros independentes. Até 26/08 elas eram mantidas por dois call
    // sites adjacentes: funcionava por vizinhança, não por construção, e um
    // terceiro ponto de oferta que chamasse só um dos dois faria as vistas
    // divergirem em silêncio (a política aprendendo de um número que o
    // operador não vê em `gate-metrics`, ou o inverso). É a mesma regra que a
    // docstring de `record_route_offer` já impunha ao campo `route`, aplicada
    // ao vizinho que ela não cobria. Provado por
    // `bump_arm_mantem_duravel_e_volatil_em_sincronia`.
    match eixo {
        ArmAxis::Offered => {
            counts[i].0 = counts[i].0.saturating_add(1);
            touring_foundation::gate_metrics_snapshot::record_code_mode_arm_offered(mode);
        }
        ArmAxis::Followed => {
            counts[i].1 = counts[i].1.saturating_add(1);
            touring_foundation::gate_metrics_snapshot::record_code_mode_arm_followed(mode);
        }
        // Economia não tem contador atômico correspondente: `gate-metrics`
        // publica canal (oferecida/tomada), e a economia é derivada no KPI a
        // partir da forma do programa. Sem par volátil, nada a sincronizar.
        ArmAxis::Economical => counts[i].2 = counts[i].2.saturating_add(1),
    }
    write_arm_file(project_root, &counts);
    arm_cache().insert(project_root.display().to_string(), counts);
}


/// P2 (decisão (b), 26/08/2026) — a política só age quando ARMADA.
///
/// Default-OFF, como o `f7_actuator_armed`: sem a env, nenhum sinal é lido e o
/// comportamento é byte-idêntico ao de antes. Armar é decisão humana, e é o
/// gate de promoção que o survey MSR exige além da fronteira do verificador —
/// aqui o sinal (o modelo seguiu a rota?) é produzido pelo próprio sistema
/// dirigido, então autonomia sem humano degradaria com a iteração.
pub(super) fn code_mode_arm_armed() -> bool {
    std::env::var("TOURING_CODE_MODE_ARM_ARMED").is_ok_and(|v| v != "0")
}


/// A precedência final, PURA: declaração humana > política armada > default.
///
/// Extraída de `code_mode_presentation` porque a invariante que mais importa
/// aqui — "a política nunca sobrescreve o que o humano declarou" — só era
/// testável mutando `TOURING_CODE_MODE_ARM_ARMED` no processo. E a var passou a
/// ser lida também pelo caminho da fusão, então essa mutação vazava para testes
/// concorrentes: `turn_gate_ignora_o_que_nao_e_fanout` falhava em paralelo e
/// passava com `--test-threads=1` (a assinatura de estado global). Um predicado
/// puro é mais fácil de testar do que de contornar, e não tem vítima.
pub(crate) fn resolve_with_policy(
    declarado: Option<CodeModePresentation>,
    armada: bool,
    counts: [(u64, u64); 3],
) -> CodeModePresentation {
    if let Some(d) = declarado {
        return d;
    }
    if armada && let Some(escolhido) = arm_choice_from_counts(counts) {
        return escolhido;
    }
    CodeModePresentation::Both
}


/// Piso de amostra por braço — COMPARTILHADO com o KPI que reporta a evidência.
///
/// Uma cópia do número em cada lado divergiria no primeiro ajuste, e aí o
/// painel diria "pronto para promover" enquanto a política ainda se cala (ou o
/// contrário). O juiz e o medidor têm de ler o mesmo predicado.
pub(crate) const ARM_MIN_SAMPLE: u64 = 20;


/// Escolhe o braço pela evidência medida — ou `None` quando ela é fina.
///
/// Puro, para ser mais fácil de testar do que de contornar. Duas regras:
///
/// * **Piso de amostra** (`ARM_MIN_SAMPLE`): abaixo dele "a melhor taxa" é
///   ruído amostral, e promover ruído é exatamente o que `default-OFF +
///   promoção medida` existe para impedir.
/// * **Comparação exige ao menos DOIS braços** com amostra. Um único braço
///   acima do piso não é uma escolha: é o único que existe, e "escolher" o
///   único observado seria confirmar a configuração vigente chamando isso de
///   aprendizado.
///
/// Exploração — oferecer deliberadamente um braço sub-amostrado para aprender
/// sobre ele — NÃO está aqui de propósito: significaria degradar a
/// apresentação para colher dado, o que é outra decisão humana.
pub(crate) fn arm_choice_from_counts(counts: [(u64, u64); 3]) -> Option<CodeModePresentation> {
    let arms = [
        CodeModePresentation::Native,
        CodeModePresentation::Both,
        CodeModePresentation::Code,
    ];
    let mut elegiveis: Vec<(f64, CodeModePresentation)> = Vec::new();
    for (i, (offered, followed)) in counts.iter().enumerate() {
        if *offered >= ARM_MIN_SAMPLE {
            elegiveis.push((*followed as f64 / *offered as f64, arms[i]));
        }
    }
    if elegiveis.len() < 2 {
        return None;
    }
    elegiveis.sort_by(|a, b| b.0.total_cmp(&a.0));
    let (melhor, vice) = (elegiveis[0], elegiveis[1]);
    // Margem mínima: uma diferença dentro do ruído NÃO é evidência. Sem ela,
    // `max_by` desempataria pela ordem do vetor e a apresentação mudaria por
    // acaso — trocar o gate de um projeto no desempate é o oposto de "promoção
    // por evidência medida". Empate ⇒ silêncio ⇒ o default declarado prevalece.
    const ARM_MIN_MARGIN: f64 = 0.05;
    if melhor.0 - vice.0 < ARM_MIN_MARGIN {
        return None;
    }
    Some(melhor.1)
}



// ── The feedback adapters: from the post-tool payload to the arm ─────────────────────────

/// Reivindica a rota pendente deste (projeto, sessão) — UMA vez.
///
/// Mesma disciplina dos ledgers de decisão e de casos: uma oferta reivindicada
/// some, para que duas leituras nunca creditem a mesma decisão duas vezes. A
/// sessão sai da MESMA `session_key` do pre e do close, ou pre e post estariam
/// falando de turnos diferentes com a mesma cara.
pub fn take_route_offer(project_root: &Path, payload: &Value) -> Option<RouteOffer> {
    let key = format!("{}\u{2}{}", project_root.display(), session_key(payload));
    let offer = route_offers().get(&key)?;
    route_offers().invalidate(&key);
    Some(offer)
}

/// Semeia uma oferta de rota — só para teste, pela MESMA chave do runtime.
///
/// Um teste que montasse a chave por conta própria provaria que a sua fórmula
/// funciona, não que a do gate funciona.
#[cfg(test)]
pub fn turn_ledger_insert_for_test(project_root: &Path, payload: &Value, offer: RouteOffer) {
    let key = format!("{}\u{2}{}", project_root.display(), session_key(payload));
    route_offers().insert(key, offer);
}

/// Reivindica a rota pendente **apenas quando há veredito** — o par
/// (braço, recompensa) pronto para o depósito.
///
/// Classificar antes de reivindicar não é preferência de estilo: uma oferta
/// reivindicada some. Com a ordem invertida, o primeiro PostToolUse de QUALQUER
/// ferramenta — um `Read`, que nem carrega comando — consumia a oferta sem
/// veredito, e o `touring run` logo em seguida já não achava nada para creditar.
/// Deixar a regra no chamador a perderia no segundo chamador; aqui ela é
/// estrutural.
pub fn claim_route_reward(
    project_root: &Path,
    payload: &Value,
    cmd: &str,
) -> Option<(RouteOffer, f64)> {
    let decision = claim_route_reward_decision(project_root, payload, cmd);
    // r3 shadow (observe-only, default OFF): a decisão acima já está tomada e é devolvida
    // por valor, intocada — o shadow só a OBSERVA para fechar o ciclo de vida da oferta.
    r3_a2::on_outcome(
        project_root,
        payload,
        cmd,
        decision.as_ref().map(|(_, v)| *v),
    );
    decision
}

/// A decisão de produção de [`claim_route_reward`] — o corpo original, sem o shadow.
fn claim_route_reward_decision(
    project_root: &Path,
    payload: &Value,
    cmd: &str,
) -> Option<(RouteOffer, f64)> {
    let value = classify_route_outcome(cmd)?;
    let offer = take_route_offer(project_root, payload)?;
    if value > 0.0 {
        bump_arm(project_root, &offer.mode, ArmAxis::Followed);
        // 1.0 é o programa que FUNDIU; 0.5 é a casca sobre uma chamada só.
        if value >= 1.0 {
            bump_arm(project_root, &offer.mode, ArmAxis::Economical);
        }
    }
    Some((offer, value))
}

