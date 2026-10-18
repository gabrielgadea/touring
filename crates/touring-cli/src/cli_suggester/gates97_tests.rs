//! Gates da 30.4.97 — G10ROTA e G7G3G9, sobre os casos REAIS do `analise`.
//!
//! Fontes (transcrições em `~/.claude/projects/-home-gabrielgadea-projects-analise/`):
//! - G10ROTA: sessão `05166acc-ff08-4644-8481-ab8c2316c4a4`, linhas 5786-5798,
//!   10/10/2026 01:39Z (09/10 22:39 BRT). A 5ª `python-inline` seriada ESCREVIA
//!   `RETOMADA.md` (`Path.write_text`); o G10 nomeou `touring run --lang python`, que o X6
//!   nega sob Sandboxed (`file-write`); a rota que funcionou foi
//!   `TOURING_TRUSTED_OK=1 .touring/bin/touring run --allow-forbidden --lang python`.
//! - G7G3G9: a mesma sessão, linhas 5441-5448 (Read #3 de `RETOMADA.md` negado pelo G7 →
//!   edição por python), e a sessão `2e4db816` do projeto touring, linhas 14641-14650:
//!   `Edit` → «File has been modified since read…» → `Read` → G7 «3ª leitura» negado.

use super::*;
use serde_json::json;
use serial_test::serial;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

fn bash(cmd: &str) -> Value {
    json!({ "command": cmd })
}

fn gate(proj: &Path, sess: &str, tool: &str, input: &Value) -> Option<String> {
    code_mode_gates_with(proj, sess, tool, input, &GateOverrides::default())
}

fn is_deny(resp: &Option<String>) -> bool {
    resp.as_deref()
        .and_then(|r| serde_json::from_str::<Value>(r).ok())
        .is_some_and(|v| v["hookSpecificOutput"]["permissionDecision"] == "deny")
}

fn reason(resp: &Option<String>) -> String {
    let v: Value = serde_json::from_str(resp.as_deref().expect("a resposta existe")).expect("json");
    v["hookSpecificOutput"]["permissionDecisionReason"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

fn escopo_code(root: &Path) {
    std::fs::create_dir_all(root.join(".touring")).expect("mkdir");
    std::fs::write(
        root.join(".touring/touring.toml"),
        "[code_mode]\nmode = \"code\"\n",
    )
    .expect("write");
}

// ── G10ROTA ──────────────────────────────────────────────────────────────────

const RETOMADA_REL: &str = "docs/plans/2026-10-06-geopolitica-logistica-eua/RETOMADA.md";

/// O comando NEGADO do caso real, verbatim (transcrição 05166acc, linha 5786).
const COMANDO_REAL_G10: &str = "TOURING_CODE_MODE=native .venv/bin/python3 - <<'EOF'\nimport pathlib\np = pathlib.Path(\"docs/plans/2026-10-06-geopolitica-logistica-eua/RETOMADA.md\")\nt = p.read_text(encoding=\"utf-8\")\nvelho = \"> Estado 10/10 ~00:30:\"\nassert t.count(velho) == 1\np.write_text(t.replace(velho, \"> Estado 09/10 22:38 (o relógio da máquina; eu tinha escrito «10/10 ~00:30» — errado):\"), encoding=\"utf-8\")\nprint(\"ok\")\nEOF";

/// As 4 chamadas que o antecederam na janela de 600s: edições python do mesmo documento.
fn escritores_anteriores(n: usize) -> Vec<String> {
    (1..=n)
        .map(|i| {
            format!(
                ".venv/bin/python3 -c 'import pathlib; p = pathlib.Path(\"{RETOMADA_REL}\"); p.write_text(p.read_text() + \"# passo {i}\")'"
            )
        })
        .collect()
}

fn projeto_g10() -> tempfile::TempDir {
    let tmp = tempfile::tempdir().expect("tempdir");
    escopo_code(tmp.path());
    let doc = tmp.path().join(RETOMADA_REL);
    std::fs::create_dir_all(doc.parent().expect("pai")).expect("mkdir docs");
    std::fs::write(&doc, "> Estado 10/10 ~00:30: pendente\n").expect("RETOMADA.md");
    tmp
}

/// Leva a rajada ao deny do G10: 4 escritores passam, o `ultimo` é a 5ª chamada.
fn quinta_chamada(proj: &Path, sess: &str, ultimo: &str) -> Option<String> {
    for cmd in escritores_anteriores(4) {
        assert!(
            gate(proj, sess, "Bash", &bash(&cmd)).is_none(),
            "as 4 primeiras passam intactas: {cmd}"
        );
    }
    gate(proj, sess, "Bash", &bash(ultimo))
}

/// O caso real: a 5ª python-inline ESCREVE. O executor nega `write_text` no perfil
/// Sandboxed, então a rota `touring run --lang python` falha — o deny tem de nomear a
/// que funciona (Trusted, por-comando, só dentro do workspace), com o comando pronto.
#[test]
#[serial(gate_metrics, t3_env)]
fn g10_corpo_que_escreve_nomeia_a_rota_trusted() {
    let tmp = projeto_g10();
    let resp = quinta_chamada(tmp.path(), "g97-g10-trusted", COMANDO_REAL_G10);
    assert!(is_deny(&resp), "a 5ª chamada seriada nega: {resp:?}");
    let r = reason(&resp);
    assert!(
        r.contains("TOURING_TRUSTED_OK=1 touring run --allow-forbidden --lang python --code '"),
        "o deny tem de nomear a rota Trusted com o comando pronto: {r}"
    );
    assert!(
        !r.contains("touring run --lang python --code"),
        "a rota Sandboxed NÃO executa um corpo que escreve (X6 nega file-write): {r}"
    );
    assert!(
        r.contains("write_text"),
        "o corpo viaja verbatim na rota: {r}"
    );
    assert!(
        r.contains("workspace"),
        "a rota é válida só dentro do workspace (o Landlock concede apenas ele): {r}"
    );
}

/// Destino FORA do workspace: nenhuma rota do sandbox escreve ali (a Trusted compartilha o
/// Landlock). O deny diz isso e NÃO promete uma rota que falha.
#[test]
#[serial(gate_metrics, t3_env)]
fn g10_destino_fora_do_workspace_nao_promete_rota() {
    let tmp = projeto_g10();
    let fora = tempfile::tempdir().expect("tempdir de fora");
    let destino = fora.path().join("saida.md");
    let cmd = format!(
        ".venv/bin/python3 -c 'import pathlib; pathlib.Path(\"{}\").write_text(\"x\")'",
        destino.display()
    );
    let resp = quinta_chamada(tmp.path(), "g97-g10-fora", &cmd);
    assert!(is_deny(&resp), "a 5ª chamada seriada nega: {resp:?}");
    let r = reason(&resp);
    assert!(
        r.contains(&destino.display().to_string()),
        "o deny cita o destino REAL: {r}"
    );
    assert!(
        r.contains("nenhuma rota"),
        "o deny diz que nenhuma rota de sandbox escreve ali: {r}"
    );
    assert!(
        !r.contains("TOURING_TRUSTED_OK=1 touring run"),
        "não promete a rota Trusted para um destino que o Landlock nega: {r}"
    );
    assert!(
        !r.contains("touring run --lang python --code"),
        "nem a Sandboxed: {r}"
    );
    assert!(
        r.contains("TOURING_GATE_OK=1"),
        "a saída que funciona é o bypass por-comando, nativo: {r}"
    );
}

/// Controle positivo: um corpo de LEITURA segue com a rota Sandboxed — o executor o admite.
#[test]
#[serial(gate_metrics, t3_env)]
fn g10_corpo_de_leitura_segue_com_a_rota_sandboxed() {
    let tmp = projeto_g10();
    let cmd = ".venv/bin/python3 -c 'import pathlib; print(pathlib.Path(\"docs\").exists())'";
    let resp = quinta_chamada(tmp.path(), "g97-g10-leitura", cmd);
    assert!(is_deny(&resp), "a 5ª chamada seriada nega: {resp:?}");
    let r = reason(&resp);
    assert!(
        r.contains("touring run --lang python --code '"),
        "leitura: a rota Sandboxed é a certa: {r}"
    );
    assert!(
        !r.contains("TOURING_TRUSTED_OK"),
        "leitura não pede elevação: {r}"
    );
}

// ── G7G3G9 ───────────────────────────────────────────────────────────────────

/// O mundo de UM arquivo e UMA sessão: os gates do hook + o que o harness do Claude Code
/// exige do `Edit` (um `Read` BEM-SUCEDIDO depois da última modificação do disco).
struct Mundo {
    proj: PathBuf,
    sess: String,
    fp: PathBuf,
    tick: u64,
    /// O `readFileState` do harness está em dia com o disco.
    fresco: bool,
    /// Algum `Read` BEM-SUCEDIDO já aconteceu nesta sessão: o G3 conhece o arquivo e não fala mais
    /// dele (o ledger do G3 não olha o mtime — só o harness o faz).
    leu: bool,
}

#[derive(Clone, Copy, Debug)]
enum Op {
    Read,
    Edit,
    /// Escrita FORA da ferramenta Edit: python, linter, outra sessão.
    Modify,
    /// `touring run` citando o arquivo (reseta o G7).
    Run,
    /// `sed -i` cego sobre o arquivo: o G9 nega e NOMEIA o `Edit` como a rota (o arquivo não muda).
    SedCego,
}

static SESSOES: AtomicU64 = AtomicU64::new(0);
/// Um `sed -i` distinto por chamada: o G6 nega a repetição idêntica através de uma época, e o que
/// este modelo mede é o G9, não o G6.
static SEDS: AtomicU64 = AtomicU64::new(0);

/// Grava `conteudo` e fixa o mtime em `base + tick` s — dois `tocar` nunca empatam.
fn tocar(fp: &Path, conteudo: &str, tick: u64) {
    std::fs::write(fp, conteudo).expect("write");
    let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000 + tick);
    std::fs::File::options()
        .write(true)
        .open(fp)
        .expect("open")
        .set_modified(t)
        .expect("set_modified");
}

impl Mundo {
    fn novo(proj: &Path, fp: &Path) -> Self {
        let n = SESSOES.fetch_add(1, Ordering::Relaxed);
        tocar(fp, "base", 0);
        Self {
            proj: proj.to_path_buf(),
            sess: format!("g97-mundo-{n}"),
            fp: fp.to_path_buf(),
            tick: 0,
            fresco: false,
            leu: false,
        }
    }

    fn gate(&self, tool: &str, input: &Value) -> Option<String> {
        gate(&self.proj, &self.sess, tool, input)
    }

    fn le(&mut self) -> Option<String> {
        let r = self.gate("Read", &json!({ "file_path": self.fp, "limit": 8 }));
        if !is_deny(&r) {
            self.fresco = true;
            self.leu = true;
        }
        r
    }

    /// `Some(true)` = o Edit TEVE SUCESSO (hook liberou E o harness o aceita).
    fn edita(&mut self) -> (Option<String>, bool) {
        let r = self.gate(
            "Edit",
            &json!({ "file_path": self.fp, "old_string": "a", "new_string": "b" }),
        );
        // A rota «Edit direto» que o G7 e o G9 nomeiam promete que o G3 a aceita: depois de um
        // `Read` o G3 fica MUDO (nem advisory) — reclamar de Edit-sem-Read de um arquivo lido é
        // o gate rodando contra a rota que os outros dois apontam.
        if self.leu {
            assert!(
                !r.as_deref()
                    .is_some_and(|x| x.contains("[G3 edit-sem-read]")),
                "o G3 não fala de um arquivo que a sessão já leu: {r:?}"
            );
        }
        let ok = !is_deny(&r) && self.fresco;
        if ok {
            self.tick += 1;
            tocar(&self.fp, "editado", self.tick);
        }
        (r, ok)
    }

    fn aplica(&mut self, op: Op) {
        match op {
            Op::Read => {
                let _ = self.le();
            }
            Op::Edit => {
                let _ = self.edita();
            }
            Op::Modify => {
                self.tick += 1;
                tocar(&self.fp, "externo", self.tick);
                self.fresco = false;
            }
            Op::Run => {
                let cmd = format!(
                    "touring run --lang python --args '[\"{}\"]' --file r1.py",
                    self.fp.display()
                );
                let _ = self.gate("Bash", &bash(&cmd));
            }
            Op::SedCego => {
                let sed = |m: &Self| {
                    let n = SEDS.fetch_add(1, Ordering::Relaxed);
                    m.gate(
                        "Bash",
                        &bash(&format!("sed -i 's/a/b{n}/' {}", m.fp.display())),
                    )
                };
                let mut r = sed(self);
                // O G5 (telemetria) fala PRIMEIRO na primeira ação Bash depois de uma rajada de
                // ≥3 edits — um advisory único que zera a rajada; o G9 responde na chamada seguinte.
                if r.as_deref().is_some_and(|x| x.contains("[G5 telemetria]")) {
                    r = sed(self);
                }
                assert!(is_deny(&r), "`sed -i` cego é negado pelo G9: {r:?}");
                let motivo = reason(&r);
                // D8: o deny nomeia a rota de edição COM o arquivo real (nunca `<o arquivo>`).
                assert!(
                    motivo.contains(&format!("Edit tool em `{}`", self.fp.display())),
                    "o deny do G9 nomeia o Edit como a rota: {motivo}"
                );
            }
        }
    }
}

const OPS: [Op; 5] = [Op::Read, Op::Edit, Op::Modify, Op::Run, Op::SedCego];

fn mundo_apos(proj: &Path, fp: &Path, prefixo: &[Op]) -> Mundo {
    let mut m = Mundo::novo(proj, fp);
    for op in prefixo {
        m.aplica(*op);
    }
    m
}

/// Existe, a partir deste estado, uma rota de edição que o hook não nega e que o harness
/// aceita: o `Edit` direto, ou um `Read` liberado seguido do `Edit`?
fn existe_rota_de_edicao(proj: &Path, fp: &Path, prefixo: &[Op]) -> bool {
    let mut direto = mundo_apos(proj, fp, prefixo);
    if direto.edita().1 {
        return true;
    }
    let mut via_read = mundo_apos(proj, fp, prefixo);
    !is_deny(&via_read.le()) && via_read.edita().1
}

fn percorre(proj: &Path, fp: &Path, prefixo: &mut Vec<Op>, fundo: usize, falhas: &mut Vec<String>) {
    if !existe_rota_de_edicao(proj, fp, prefixo) {
        falhas.push(format!("{prefixo:?}"));
    }
    if fundo == 0 {
        return;
    }
    for op in OPS {
        prefixo.push(op);
        percorre(proj, fp, prefixo, fundo - 1, falhas);
        prefixo.pop();
    }
}

fn arquivo_de_edicao() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("tempdir");
    escopo_code(tmp.path());
    let fp = tmp.path().join(RETOMADA_REL);
    std::fs::create_dir_all(fp.parent().expect("pai")).expect("mkdir");
    std::fs::write(&fp, "base").expect("write");
    (tmp, fp)
}

/// A sequência EXATA do caso real (05166acc 5441-5448 e 2e4db816 14641-14650): duas leituras,
/// o arquivo muda por fora da ferramenta Edit (python), o Edit acusa «modified since read» e a
/// releitura que o harness exige NÃO pode ser negada pelo G7 — reler um arquivo que MUDOU é o
/// oposto de programa-faltando.
#[test]
#[serial(gate_metrics, t3_env)]
fn g7_releitura_depois_de_modificacao_externa_nao_e_negada() {
    let (tmp, fp) = arquivo_de_edicao();
    let s = "g97-g7-real";
    let ler = || json!({ "file_path": fp, "limit": 8 });
    tocar(&fp, "> Estado 10/10 ~00:30: pendente\n", 1);
    assert!(
        gate(tmp.path(), s, "Read", &ler()).is_none(),
        "1ª leitura: silêncio"
    );
    let segunda = gate(tmp.path(), s, "Read", &ler());
    assert!(
        segunda.is_some() && !is_deny(&segunda),
        "2ª: advisory, nunca deny"
    );
    // python reescreve o documento (write_text): o mtime avança, o harness fica obsoleto
    tocar(&fp, "> Estado 09/10 22:38: corrigido\n", 2);
    let terceira = gate(tmp.path(), s, "Read", &ler());
    assert!(
        !is_deny(&terceira),
        "o arquivo MUDOU desde a última leitura: a releitura que o harness exige é liberada: {terceira:?}"
    );
}

/// Controle positivo: o arquivo NÃO mudou — a 3ª leitura é re-inspeção de verdade e o G7
/// segue negando (o gate continua existindo para a rajada que o motivou).
#[test]
#[serial(gate_metrics, t3_env)]
fn g7_terceira_leitura_do_arquivo_inalterado_segue_negada() {
    let (tmp, fp) = arquivo_de_edicao();
    let s = "g97-g7-controle";
    let ler = || json!({ "file_path": fp });
    let _ = gate(tmp.path(), s, "Read", &ler());
    let _ = gate(tmp.path(), s, "Read", &ler());
    let terceira = gate(tmp.path(), s, "Read", &ler());
    assert!(
        is_deny(&terceira),
        "inalterado: a re-inspeção segue negada: {terceira:?}"
    );
}

/// D8: o deny do G7 NOMEIA a rota de edição, e a promessa é verdadeira — o Edit seguinte passa
/// em todos os gates, e se o arquivo mudar a releitura é liberada.
#[test]
#[serial(gate_metrics, t3_env)]
fn g7_deny_nomeia_a_rota_de_edicao_e_a_promessa_cumpre() {
    let (tmp, fp) = arquivo_de_edicao();
    let s = "g97-g7-rota";
    let ler = || json!({ "file_path": fp });
    let _ = gate(tmp.path(), s, "Read", &ler());
    let _ = gate(tmp.path(), s, "Read", &ler());
    let negado = gate(tmp.path(), s, "Read", &ler());
    assert!(is_deny(&negado));
    let r = reason(&negado);
    assert!(
        r.contains("Edit direto") && r.contains("a releitura é liberada"),
        "o deny nomeia a rota de edição e a condição da releitura: {r}"
    );
    let edit = json!({ "file_path": fp, "old_string": "a", "new_string": "b" });
    assert!(
        !is_deny(&gate(tmp.path(), s, "Edit", &edit)),
        "a rota nomeada (Edit direto) passa nos gates"
    );
    tocar(&fp, "mudou por fora", 7);
    assert!(
        !is_deny(&gate(tmp.path(), s, "Read", &ler())),
        "a releitura prometida é liberada de fato"
    );
}

/// D8, o lado negativo: sem carimbo (arquivo que não se deixa medir) a afirmação «o arquivo não
/// mudou» não tem lastro — o deny segue, mas NÃO promete a rota de edição.
#[test]
#[serial(gate_metrics, t3_env)]
fn g7_deny_sem_medicao_nao_promete_rota_de_edicao() {
    let tmp = tempfile::tempdir().expect("tempdir");
    escopo_code(tmp.path());
    let fp = tmp.path().join("nao-existe/ainda.md");
    let s = "g97-g7-sem-medicao";
    let ler = || json!({ "file_path": fp });
    let _ = gate(tmp.path(), s, "Read", &ler());
    let _ = gate(tmp.path(), s, "Read", &ler());
    let negado = gate(tmp.path(), s, "Read", &ler());
    assert!(is_deny(&negado), "a 3ª leitura segue negada: {negado:?}");
    let r = reason(&negado);
    assert!(
        !r.contains("Edit direto"),
        "sem medição o deny não afirma que o arquivo não mudou: {r}"
    );
}

/// O invariante da composição G7×G3×G9: para TODO estado alcançável por sequências de até 5
/// operações {Read, Edit, Modify, Run, SedCego} sobre o arquivo, existe uma rota de edição que
/// algum gate nomeia (Edit direto, ou Read→Edit) e que nenhum gate nega nem o harness recusa —
/// e o `sed -i` cego (G9) nega NOMEANDO essa rota (o `Edit` sobre o arquivo real), enquanto o G3
/// fica mudo diante de um arquivo que a sessão já leu.
#[test]
#[serial(gate_metrics, t3_env)]
fn composicao_g7_g3_g9_sempre_deixa_uma_rota_de_edicao() {
    let (tmp, fp) = arquivo_de_edicao();
    let mut falhas = Vec::new();
    percorre(tmp.path(), &fp, &mut Vec::new(), 5, &mut falhas);
    assert!(
        falhas.is_empty(),
        "{} estado(s) sem rota de edição — o círculo; o menor: {:?}",
        falhas.len(),
        falhas.iter().min_by_key(|f| f.len())
    );
}

// ── Ordem dos gates do Bash: o deny aplicável vence o advisory ──────────────────────────────────
//
// Invariante (coordenadora, 10/10/2026): todo deny aplicável vence todo advisory, qualquer que
// seja a ordem de registro dos gates. O G5 (telemetria) respondia advisory na 1ª ação Bash depois
// de ≥3 edits e curto-circuitava a cadeia: o deny do G9 nunca era avaliado e o `sed -i` cego
// executava (achado do gates-97, `logs/p4/green_cli.log`). O advisory não se perde: vai junto no
// texto do deny.

/// Lê o arquivo (registra no G3) e faz `n` edições pelo gate: cada uma avança a rajada do G5.
fn rajada_de_edits(proj: &Path, fp: &Path, s: &str, n: usize) {
    let _ = gate(proj, s, "Read", &json!({ "file_path": fp, "limit": 8 }));
    for _ in 0..n {
        let edit = json!({ "file_path": fp, "old_string": "a", "new_string": "b" });
        let r = gate(proj, s, "Edit", &edit);
        assert!(!is_deny(&r), "as edições passam nos gates: {r:?}");
    }
}

/// O caso: 3 edits e, na 1ª ação Bash, um `sed -i` cego. Hoje o G5 fala primeiro e o G9 não é
/// avaliado; o deny aplicável tem de vencer, levando o advisory no texto.
#[test]
#[serial(gate_metrics, t3_env)]
fn g5_advisory_nao_engole_o_deny_do_g9() {
    let (tmp, fp) = arquivo_de_edicao();
    let s = "g97-ordem-g5-g9";
    rajada_de_edits(tmp.path(), &fp, s, 3);
    let r = gate(
        tmp.path(),
        s,
        "Bash",
        &bash(&format!("sed -i 's/a/b/' {}", fp.display())),
    );
    assert!(
        is_deny(&r),
        "o deny do G9 vence o advisory do G5 (hoje: advisory e o sed executa): {r:?}"
    );
    let motivo = reason(&r);
    assert!(
        motivo.contains("[G9 escrita-cega-inline]")
            && motivo.contains(&format!("Edit tool em `{}`", fp.display())),
        "o deny é o do G9, com a rota real: {motivo}"
    );
    assert!(
        motivo.contains("[G5 telemetria]"),
        "o advisory vai junto no texto do deny: {motivo}"
    );
}

/// Controle: depois de 3 edits uma Bash inofensiva segue com o advisory do G5, sem deny.
#[test]
#[serial(gate_metrics, t3_env)]
fn g5_bash_inofensiva_depois_de_3_edits_segue_com_o_advisory() {
    let (tmp, fp) = arquivo_de_edicao();
    let s = "g97-ordem-g5-controle";
    rajada_de_edits(tmp.path(), &fp, s, 3);
    let r = gate(tmp.path(), s, "Bash", &bash("echo ok"));
    assert!(r.is_some() && !is_deny(&r), "só o advisory: {r:?}");
    assert!(
        r.as_deref().unwrap_or_default().contains("[G5 telemetria]"),
        "o advisory do G5 segue: {r:?}"
    );
}

/// Controle: `sed -i` sem rajada de edits antes segue com o deny do G9, sem texto de G5.
#[test]
#[serial(gate_metrics, t3_env)]
fn g9_sed_i_sem_edits_antes_segue_negado_sem_texto_de_g5() {
    let (tmp, fp) = arquivo_de_edicao();
    let r = gate(
        tmp.path(),
        "g97-ordem-g9-controle",
        "Bash",
        &bash(&format!("sed -i 's/a/b/' {}", fp.display())),
    );
    assert!(is_deny(&r), "{r:?}");
    let motivo = reason(&r);
    assert!(motivo.contains("[G9 escrita-cega-inline]"), "{motivo}");
    assert!(!motivo.contains("[G5 telemetria]"), "{motivo}");
}

/// A MESMA forma em outro ponto da cadeia Bash: o advisory de rajada do S3 (2ª inspeção da
/// classe) respondia antes do G6 — a 3ª repetição byte-idêntica, que o G6 nega, saía advisory.
/// Sequência: `cat a` (G6 vê), run, `cat a` (G6 avisa), run, `cat b`, `cat a` (S3 n=2 e G6 nega).
#[test]
#[serial(gate_metrics, t3_env)]
fn s3_advisory_nao_engole_o_deny_do_g6() {
    let tmp = tempfile::tempdir().expect("tempdir");
    escopo_code(tmp.path());
    let s = "g97-ordem-s3-g6";
    let p = tmp.path();
    let cat_a = bash("cat docs/g6a.md");
    let rota = |n: u8| bash(&format!("touring run --lang bash --code 'echo r{n}'"));
    assert!(gate(p, s, "Bash", &cat_a).is_none(), "1ª vista: silêncio");
    assert!(gate(p, s, "Bash", &rota(1)).is_none());
    let segunda = gate(p, s, "Bash", &cat_a);
    assert!(
        segunda.is_some() && !is_deny(&segunda),
        "2ª idêntica: advisory do G6: {segunda:?}"
    );
    assert!(gate(p, s, "Bash", &rota(2)).is_none());
    assert!(gate(p, s, "Bash", &bash("cat docs/g6b.md")).is_none());
    let terceira = gate(p, s, "Bash", &cat_a);
    assert!(
        is_deny(&terceira),
        "3ª idêntica: o deny do G6 vence o advisory de rajada do S3: {terceira:?}"
    );
    let motivo = reason(&terceira);
    assert!(motivo.contains("[G6 redundant-exact-call]"), "{motivo}");
    assert!(
        motivo.contains("[CODE MODE · rajada]"),
        "o advisory do S3 vai junto no texto do deny: {motivo}"
    );
}

/// Controle (ordem D do Gabriel, 09/10/2026): a 2ª inspeção da classe SÓ AVISA — o G1, que dispara
/// na 2ª quando os comandos são quase iguais, não volta a decidir essa chamada. Os dois `grep`
/// compartilham 3 de 4 tokens, o suficiente para o G1 reescrever se ele fosse avaliado.
#[test]
#[serial(gate_metrics, t3_env)]
fn s3_segunda_inspecao_parecida_segue_so_com_o_advisory() {
    let tmp = tempfile::tempdir().expect("tempdir");
    escopo_code(tmp.path());
    let s = "g97-ordem-s3-controle";
    assert!(gate(tmp.path(), s, "Bash", &bash("grep -n alfa docs/g6a.md")).is_none());
    let r = gate(tmp.path(), s, "Bash", &bash("grep -n alfa docs/g6b.md"));
    assert!(r.is_some() && !is_deny(&r), "só o advisory: {r:?}");
    let texto = r.as_deref().unwrap_or_default();
    assert!(texto.contains("[CODE MODE · rajada]"), "{r:?}");
    assert!(
        !texto.contains("updatedInput"),
        "o G1 não reescreve a 2ª inspeção — a rajada só avisa: {r:?}"
    );
}

// ── G10 write→run: a MESMA falha no segundo deny do G10 ─────────────────────

/// O par write→run nomeia `touring run --file <script>`; o executor julga o CONTEÚDO do script.
/// Um script que escreve não roda no Sandboxed (X6 `file-write`) — a rota `--file` nomeada
/// falharia exatamente como a do exec-burst.
fn par_com_scripts(sess: &str, scripts: &[&Path], root: &Path, ultimo: &Path) -> Option<String> {
    let ovr = GateOverrides::default();
    for s in scripts {
        written_scripts_ledger().insert(write_run_key(root, sess, &s.display().to_string()), ());
    }
    let primeiro = scripts[0];
    assert!(
        write_run_pair_gate_with(root, sess, &format!("python3 {}", primeiro.display()), &ovr)
            .is_none(),
        "o 1º par passa: criar e testar UM script é legítimo"
    );
    write_run_pair_gate_with(root, sess, &format!("python3 {}", ultimo.display()), &ovr)
}

#[test]
#[serial(gate_metrics, t3_env)]
fn g10_par_write_run_de_script_que_escreve_nomeia_a_rota_trusted() {
    let tmp = tempfile::tempdir().expect("tempdir");
    escopo_code(tmp.path());
    let le = tmp.path().join("le.py");
    let escreve = tmp.path().join("escreve.py");
    std::fs::write(
        &le,
        "import pathlib\nprint(pathlib.Path(\"RETOMADA.md\").exists())\n",
    )
    .expect("le.py");
    std::fs::write(
        &escreve,
        "import pathlib\npathlib.Path(\"RETOMADA.md\").write_text(\"novo\")\n",
    )
    .expect("escreve.py");
    let deny = par_com_scripts("g97-par-escreve", &[&le, &escreve], tmp.path(), &escreve)
        .expect("o 2º par nega");
    let esperado = format!(
        "TOURING_TRUSTED_OK=1 touring run --allow-forbidden --lang python --file {} --timeout-ms 60000",
        escreve.display()
    );
    assert!(
        deny.contains(&esperado),
        "o script ESCREVE: a rota `--file` do Sandboxed falharia; a Trusted executa: {deny}"
    );
    assert!(
        !deny.contains(&format!(
            "touring run --lang python --file {}",
            escreve.display()
        )),
        "a rota Sandboxed não pode ser nomeada para um script que escreve: {deny}"
    );
}

/// Controle positivo: um script de LEITURA segue com a rota Sandboxed de sempre.
#[test]
#[serial(gate_metrics, t3_env)]
fn g10_par_write_run_de_script_de_leitura_segue_sandboxed() {
    let tmp = tempfile::tempdir().expect("tempdir");
    escopo_code(tmp.path());
    let a = tmp.path().join("a.py");
    let b = tmp.path().join("b.py");
    for p in [&a, &b] {
        std::fs::write(
            p,
            "import pathlib\nprint(pathlib.Path(\"RETOMADA.md\").exists())\n",
        )
        .expect("script");
    }
    let deny = par_com_scripts("g97-par-leitura", &[&a, &b], tmp.path(), &b).expect("2º par nega");
    assert!(
        deny.contains(&format!(
            "touring run --lang python --file {} --timeout-ms 60000",
            b.display()
        )),
        "{deny}"
    );
    assert!(!deny.contains("TOURING_TRUSTED_OK"), "{deny}");
}

/// Destino fora do workspace: nenhuma rota; o deny diz isso e não nomeia comando algum.
#[test]
#[serial(gate_metrics, t3_env)]
fn g10_par_write_run_com_destino_fora_nao_promete_rota() {
    let tmp = tempfile::tempdir().expect("tempdir");
    escopo_code(tmp.path());
    let fora = tempfile::tempdir().expect("tempdir de fora");
    let destino = fora.path().join("saida.md");
    let a = tmp.path().join("a.py");
    let b = tmp.path().join("b.py");
    std::fs::write(&a, "print(1)\n").expect("a.py");
    std::fs::write(
        &b,
        format!(
            "import pathlib\npathlib.Path(\"{}\").write_text(\"x\")\n",
            destino.display()
        ),
    )
    .expect("b.py");
    let deny = par_com_scripts("g97-par-fora", &[&a, &b], tmp.path(), &b).expect("2º par nega");
    assert!(deny.contains("nenhuma rota"), "{deny}");
    assert!(deny.contains(&destino.display().to_string()), "{deny}");
    assert!(!deny.contains("TOURING_TRUSTED_OK=1 touring run"), "{deny}");
    assert!(!deny.contains("touring run --lang python --file"), "{deny}");
}
