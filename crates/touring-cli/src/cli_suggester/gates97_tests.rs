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

// ── S-13 (B2): toda negação nomeia o próximo comando ─────────────────────────
//
// Medido no corpus de transcrições antes do deploy de 27/10/2026: depois de um deny do G6 o
// modelo repetia a chamada byte-idêntica em 21 de 24 casos (duas cadeias de 11-12 cópias numa
// só sessão — o texto dele reconhecia o gate e a chamada seguinte saía com os mesmos bytes), e
// depois do G3 cerca de 1 em 4 era negado de novo. G10/G11 recuperavam ≥ 96%: a razão deles
// já trazia o comando pronto. A rubrica: (1) a invariante, (2) o próximo passo como comando,
// (3) o que segue proibido, (4) o prefixo `[G<n> nome]` intacto.

/// Os trechos entre crases de uma razão.
fn backtick_spans(reason: &str) -> Vec<&str> {
    reason.split('`').skip(1).step_by(2).collect()
}

/// A razão carrega um comando concreto para a chamada seguinte, em uma destas formas: a linha
/// pronta (indentada, abrindo por um verbo executável), uma chamada entre crases com argumento,
/// uma invocação do sandbox com flags, ou o token de bypass SEGUIDO do comando que ele prefixa.
/// Menção não conta: `touring run` sem argumento e «prefixe TOURING_GATE_OK=1 (contado…)» são a
/// regra, não o comando.
fn names_next_command(reason: &str) -> bool {
    reason.contains("touring run --")
        || has_ready_line(reason)
        || has_call_in_backticks(reason)
        || has_bypass_followed_by_command(reason)
}

/// Os verbos com que abre uma chamada que o modelo executa, e quantas palavras ela tem no mínimo
/// (`touring run` sozinho é menção; `touring run --lang bash` é chamada).
const CALL_VERBS: [(&str, usize); 5] = [
    ("touring ", 3),
    ("Read ", 2),
    ("Edit ", 2),
    ("set -o pipefail", 3),
    (GATE_BYPASS_TOKEN, 2),
];

fn opens_a_call(text: &str) -> bool {
    CALL_VERBS
        .iter()
        .any(|(verb, words)| text.starts_with(verb) && text.split_whitespace().count() >= *words)
}

/// Uma linha indentada em 2 espaços cujo conteúdo abre uma chamada — a convenção das razões que
/// já traziam o comando (`\n  touring run …`, `\n  set -o pipefail; …`).
fn has_ready_line(reason: &str) -> bool {
    reason.lines().any(|l| {
        l.strip_prefix("  ")
            .is_some_and(|rest| opens_a_call(rest.trim_start()))
    })
}

/// Um trecho entre crases que abre uma chamada.
fn has_call_in_backticks(reason: &str) -> bool {
    backtick_spans(reason).iter().any(|span| opens_a_call(span))
}

/// O token de bypass com um comando de verdade depois dele (letra, dígito, caminho ou `$`) —
/// nunca `(contado como bypassed)`.
fn has_bypass_followed_by_command(reason: &str) -> bool {
    reason.match_indices(GATE_BYPASS_TOKEN).any(|(i, _)| {
        reason[i + GATE_BYPASS_TOKEN.len()..]
            .strip_prefix(' ')
            .and_then(|rest| rest.chars().next())
            .is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '/' | '_' | '~' | '$'))
    })
}

/// A razão PRÓPRIA do gate: o merge do veredito anexa os avisos de outros gates depois do
/// separador, e um `touring run` de um aviso não é o próximo passo da negação.
fn razao_propria(reason: &str) -> String {
    reason
        .split(touring_hook_runtime::verdict::ALSO_NON_BLOCKING)
        .next()
        .unwrap_or_default()
        .trim_end()
        .to_string()
}

/// Um arquivo de 200 linhas no escopo `code`, com a linha 120 única — grande o bastante para o
/// G3 devolver uma janela de leitura.
fn projeto_com_arquivo_grande() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().expect("tempdir");
    escopo_code(tmp.path());
    let fp = tmp.path().join("grande.rs");
    let corpo: String = (1..=200).map(|i| format!("linha {i}\n")).collect();
    std::fs::write(&fp, corpo).expect("grande.rs");
    (tmp, fp)
}

/// Leva o G6 ao deny com a sequência do teste do S3: o `touring run` no meio zera a rajada de
/// inspeção e deixa o G6 decidir sozinho. Devolve o advisory da 2ª cópia e o deny da 3ª.
fn g6_ate_o_deny(p: &Path, s: &str, cmd: &str) -> (Option<String>, Option<String>) {
    let igual = bash(cmd);
    let rota = |n: u8| bash(&format!("touring run --lang bash --code 'echo r{n}'"));
    assert!(gate(p, s, "Bash", &igual).is_none(), "1ª vista: silêncio");
    assert!(gate(p, s, "Bash", &rota(1)).is_none());
    let advisory = gate(p, s, "Bash", &igual);
    assert!(
        advisory.is_some() && !is_deny(&advisory),
        "2ª idêntica: advisory: {advisory:?}"
    );
    assert!(gate(p, s, "Bash", &rota(2)).is_none());
    assert!(gate(p, s, "Bash", &bash("echo outra-coisa")).is_none());
    (advisory, gate(p, s, "Bash", &igual))
}

/// Todo deny que a cadeia produz para G1..G11, dirigido como os testes acima. G4/G5 são
/// telemetria e G8 reescreve — não negam. O rótulo é `G<n>: ramo`.
fn toda_razao_de_deny() -> Vec<(&'static str, String)> {
    let mut out: Vec<(&'static str, String)> = Vec::new();
    let mut push = |rotulo: &'static str, resp: Option<String>| {
        assert!(
            is_deny(&resp),
            "{rotulo}: o driver tem de produzir um deny: {resp:?}"
        );
        out.push((rotulo, razao_propria(&reason(&resp))));
    };

    // G1 — o deny clássico, atrás do kill switch de reescrita.
    {
        let tmp = tempfile::tempdir().expect("tempdir");
        let ovr = GateOverrides {
            g1_rewrite_disabled: true,
            g1_precision: Some(None),
            ..Default::default()
        };
        for cmd in ["grep -n alpha src/a.rs", "grep -rn beta crates/"] {
            let _ = burst_gate_with(tmp.path(), "s13-g1", cmd, &Value::Null, &ovr);
        }
        let resp = burst_gate_with(
            tmp.path(),
            "s13-g1",
            "grep -l gamma docs/x.md",
            &Value::Null,
            &ovr,
        );
        push("G1: rajada de inspeção", resp);
    }
    // G2 — `$?` depois de pipe, atrás do kill switch de reescrita.
    {
        let tmp = tempfile::tempdir().expect("tempdir");
        let ovr = GateOverrides {
            g2_rewrite_disabled: true,
            ..Default::default()
        };
        let cmd = bash("cargo test 2>&1 | tail -3; echo EXIT=$?");
        let resp = code_mode_gates_with(tmp.path(), "s13-g2", "Bash", &cmd, &ovr);
        push("G2: exit através de pipe", resp);
    }
    // G3 — o 2º Edit seguido sem Read.
    {
        let (tmp, fp) = projeto_com_arquivo_grande();
        let edit = json!({ "file_path": fp, "old_string": "linha 120", "new_string": "x" });
        let _ = gate(tmp.path(), "s13-g3", "Edit", &edit);
        push(
            "G3: edit sem read",
            gate(tmp.path(), "s13-g3", "Edit", &edit),
        );
    }
    // G6 — a 3ª cópia byte-idêntica.
    {
        let tmp = tempfile::tempdir().expect("tempdir");
        escopo_code(tmp.path());
        let (_, deny) = g6_ate_o_deny(tmp.path(), "s13-g6", "cat docs/g6a.md");
        push("G6: chamada redundante", deny);
    }
    // G7 — a 3ª leitura do mesmo arquivo.
    {
        let (tmp, fp) = projeto_com_arquivo_grande();
        let le = json!({ "file_path": fp });
        let _ = gate(tmp.path(), "s13-g7", "Read", &le);
        let _ = gate(tmp.path(), "s13-g7", "Read", &le);
        push("G7: re-inspeção", gate(tmp.path(), "s13-g7", "Read", &le));
    }
    // G9 — `sed -i` cego.
    {
        let tmp = tempfile::tempdir().expect("tempdir");
        let cmd = bash("sed -i 's/old/new/' src/foo.rs");
        push(
            "G9: escrita cega inline",
            gate(tmp.path(), "s13-g9", "Bash", &cmd),
        );
    }
    // G10 — a 5ª execução seriada: rota Sandboxed e destino fora do workspace (sem rota).
    {
        let tmp = projeto_g10();
        let leitura =
            ".venv/bin/python3 -c 'import pathlib; print(pathlib.Path(\"docs\").exists())'";
        push(
            "G10: exec-burst com rota",
            quinta_chamada(tmp.path(), "s13-g10-rota", leitura),
        );
    }
    {
        let tmp = projeto_g10();
        let fora = tempfile::tempdir().expect("tempdir de fora");
        let cmd = format!(
            ".venv/bin/python3 -c 'import pathlib; pathlib.Path(\"{}\").write_text(\"x\")'",
            fora.path().join("saida.md").display()
        );
        push(
            "G10: exec-burst sem rota",
            quinta_chamada(tmp.path(), "s13-g10-fora", &cmd),
        );
    }
    // G10 — o par write→run: script de leitura (rota nomeada) e destino fora (sem rota).
    {
        let tmp = tempfile::tempdir().expect("tempdir");
        escopo_code(tmp.path());
        let (a, b) = (tmp.path().join("a.py"), tmp.path().join("b.py"));
        for p in [&a, &b] {
            std::fs::write(p, "print(1)\n").expect("script");
        }
        push(
            "G10: par write→run com rota",
            par_com_scripts("s13-par-rota", &[&a, &b], tmp.path(), &b),
        );
    }
    {
        let tmp = tempfile::tempdir().expect("tempdir");
        escopo_code(tmp.path());
        let fora = tempfile::tempdir().expect("tempdir de fora");
        let (a, b) = (tmp.path().join("a.py"), tmp.path().join("b.py"));
        std::fs::write(&a, "print(1)\n").expect("a.py");
        std::fs::write(
            &b,
            format!(
                "import pathlib\npathlib.Path(\"{}\").write_text(\"x\")\n",
                fora.path().join("saida.md").display()
            ),
        )
        .expect("b.py");
        push(
            "G10: par write→run sem rota",
            par_com_scripts("s13-par-fora", &[&a, &b], tmp.path(), &b),
        );
    }
    // G11 — o 2º bypass colado no 1º.
    {
        let tmp = tempfile::tempdir().expect("tempdir");
        let s = "s13-g11";
        let _ = gate(tmp.path(), s, "Bash", &bash("TOURING_GATE_OK=1 echo a"));
        push(
            "G11: orçamento de bypass",
            gate(tmp.path(), s, "Bash", &bash("TOURING_GATE_OK=1 echo b")),
        );
    }
    out
}

/// O teste estrutural da S-13: nenhuma razão de deny termina sem dizer o que fazer a seguir.
#[test]
#[serial(gate_metrics, t3_env)]
fn test_every_gate_reason_names_the_next_command() {
    let razoes = toda_razao_de_deny();
    for gate_id in ["G1", "G2", "G3", "G6", "G7", "G9", "G10", "G11"] {
        assert!(
            razoes
                .iter()
                .any(|(rotulo, _)| rotulo.split(':').next() == Some(gate_id)),
            "{gate_id} sem driver: o teste estrutural teria um buraco"
        );
    }
    let prefixo_perdido: Vec<String> = razoes
        .iter()
        .filter(|(rotulo, r)| {
            let id = rotulo.split(':').next().unwrap_or_default();
            !r.starts_with(&format!("[{id} "))
        })
        .map(|(rotulo, r)| format!("{rotulo}: {r}"))
        .collect();
    assert!(
        prefixo_perdido.is_empty(),
        "o prefixo estruturado `[G<n> nome]` é casado por parsers e testes: {prefixo_perdido:#?}"
    );
    let sem_comando: Vec<String> = razoes
        .iter()
        .filter(|(_, r)| !names_next_command(r))
        .map(|(rotulo, r)| format!("{rotulo}: {r}"))
        .collect();
    assert!(
        sem_comando.is_empty(),
        "negação que não nomeia o próximo comando ensina nada ({} de {}): {sem_comando:#?}",
        sem_comando.len(),
        razoes.len()
    );
}

/// O texto do advisory (`additionalContext`) de uma resposta.
fn contexto(resp: &Option<String>) -> String {
    let v: Value = serde_json::from_str(resp.as_deref().expect("a resposta existe")).expect("json");
    v["hookSpecificOutput"]["additionalContext"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

/// G6 — o deny e o advisory entregam a linha de bypass do PRÓPRIO comando, e a linha funciona:
/// colada de volta na cadeia ela passa (D8 — a frase promete o que o executor cumpre).
#[test]
#[serial(gate_metrics, t3_env)]
fn g6_entrega_a_linha_de_bypass_do_proprio_comando_e_ela_passa() {
    let tmp = tempfile::tempdir().expect("tempdir");
    escopo_code(tmp.path());
    let (p, s) = (tmp.path(), "s13-g6-linha");
    let cmd = "cat docs/g6a.md";
    let (advisory, deny) = g6_ate_o_deny(p, s, cmd);
    let linha = format!("{GATE_BYPASS_TOKEN} {cmd}");

    let aviso = contexto(&advisory);
    assert!(aviso.starts_with("[G6 redundant-exact-call] "), "{aviso}");
    assert!(
        aviso.contains(&format!("\n  {linha}\n")),
        "o advisory traz a linha pronta: {aviso}"
    );
    assert!(aviso.contains("PRÓXIMA idêntica será negada"), "{aviso}");

    assert!(is_deny(&deny), "{deny:?}");
    let razao = razao_propria(&reason(&deny));
    assert!(razao.starts_with("[G6 redundant-exact-call] "), "{razao}");
    assert!(
        razao.contains(&format!("\n  {linha}\n")),
        "a linha pronta, sozinha: {razao}"
    );
    for parte in [
        "(1) o dado já basta",
        "(2) precisa de OUTRO recorte",
        "(3) o estado mudou",
    ] {
        assert!(razao.contains(parte), "falta `{parte}`: {razao}");
    }
    assert!(
        razao.contains("conta no G11"),
        "o custo do bypass é dito: {razao}"
    );
    assert!(
        razao.contains("Proibido: reemitir este comando sem mudar nada"),
        "{razao}"
    );
    assert!(
        !razao.contains("\nComando:"),
        "a linha pronta já carrega o comando inteiro: {razao}"
    );

    let passou = gate(p, s, "Bash", &bash(&linha));
    assert!(
        passou.is_none(),
        "a linha que o deny entrega tem de passar: {passou:?}"
    );
}

/// G6 — comando que não cabe numa linha (várias linhas, ou longo demais) não ganha linha pronta
/// quebrada: a razão manda prefixar o comando mostrado, verbatim, e o mostra.
#[test]
#[serial(gate_metrics, t3_env)]
fn g6_comando_que_nao_cabe_numa_linha_nao_ganha_linha_pronta() {
    let longo = format!("echo {}", "x".repeat(READY_LINE_MAX_CHARS + 1));
    for (i, cmd) in ["echo um\necho dois".to_string(), longo].iter().enumerate() {
        let tmp = tempfile::tempdir().expect("tempdir");
        escopo_code(tmp.path());
        let (_, deny) = g6_ate_o_deny(tmp.path(), &format!("s13-g6-multi-{i}"), cmd);
        let razao = razao_propria(&reason(&deny));
        assert!(
            !razao.contains(&format!("\n  {GATE_BYPASS_TOKEN} ")),
            "{razao}"
        );
        assert!(
            razao.contains(&format!("com `{GATE_BYPASS_TOKEN} `")),
            "{razao}"
        );
        assert!(
            razao.contains("\nComando: echo "),
            "o comando aparece uma vez: {razao}"
        );
    }
}

#[test]
fn ready_bypass_line_recusa_o_que_nao_cabe_numa_linha() {
    assert_eq!(
        ready_bypass_line("  cargo test -p x \n").as_deref(),
        Some("TOURING_GATE_OK=1 cargo test -p x")
    );
    assert_eq!(ready_bypass_line("   "), None);
    assert_eq!(ready_bypass_line("a\nb"), None);
    assert_eq!(
        ready_bypass_line(&"y".repeat(READY_LINE_MAX_CHARS + 1)),
        None
    );
    assert!(ready_bypass_line(&"y".repeat(READY_LINE_MAX_CHARS)).is_some());
}

/// G3 — o advisory e o deny entregam o `Read` com a janela do `old_string` do próprio Edit, e o
/// Read entregue destrava o Edit (o gate fica mudo — a frase cumpre o que promete).
#[test]
#[serial(gate_metrics, t3_env)]
fn g3_entrega_o_read_com_a_janela_do_old_string_e_o_read_destrava_o_edit() {
    let (tmp, fp) = projeto_com_arquivo_grande();
    let (p, s) = (tmp.path(), "s13-g3-janela");
    let edit = json!({ "file_path": fp, "old_string": "linha 120", "new_string": "x" });
    let leitura = format!("Read {} offset=115 limit=11", fp.display());

    let advisory = gate(p, s, "Edit", &edit);
    let aviso = contexto(&advisory);
    assert!(aviso.starts_with("[G3 edit-sem-read] "), "{aviso}");
    assert!(aviso.contains("(advisory 1/1"), "{aviso}");
    assert!(aviso.contains(&format!("`{leitura}`")), "{aviso}");
    assert!(aviso.contains("cat/sed/grep via Bash não"), "{aviso}");

    let deny = gate(p, s, "Edit", &edit);
    assert!(is_deny(&deny), "{deny:?}");
    let razao = reason(&deny);
    assert!(
        razao.starts_with("[G3 edit-sem-read] 2º Edit sem Read"),
        "{razao}"
    );
    assert!(
        razao.contains(&format!("`{leitura}`, e só então o Edit")),
        "{razao}"
    );
    assert!(
        razao.contains("cat/sed/grep via Bash e o Read de OUTRO arquivo não registram"),
        "{razao}"
    );
    assert!(razao.contains("Proibido até lá"), "{razao}");

    let le = gate(
        p,
        s,
        "Read",
        &json!({ "file_path": fp, "offset": 115, "limit": 11 }),
    );
    assert!(!is_deny(&le), "o Read entregue passa: {le:?}");
    assert!(
        gate(p, s, "Edit", &edit).is_none(),
        "depois do Read entregue o Edit passa em silêncio"
    );
}

/// G3 — sem janela derivável (arquivo curto, `old_string` ausente), o Read é o do arquivo todo.
#[test]
#[serial(gate_metrics, t3_env)]
fn g3_sem_janela_derivavel_entrega_o_read_do_arquivo_todo() {
    let tmp = tempfile::tempdir().expect("tempdir");
    escopo_code(tmp.path());
    let curto = tmp.path().join("curto.rs");
    std::fs::write(&curto, "fn a() {}\n").expect("curto.rs");
    let (grande_tmp, grande) = projeto_com_arquivo_grande();
    let casos = [
        (tmp.path(), curto, "fn a"),
        (grande_tmp.path(), grande, "texto que não existe no arquivo"),
    ];
    for (i, (p, fp, old)) in casos.iter().enumerate() {
        let s = format!("s13-g3-simples-{i}");
        let edit = json!({ "file_path": fp, "old_string": old, "new_string": "x" });
        let _ = gate(p, &s, "Edit", &edit);
        let razao = reason(&gate(p, &s, "Edit", &edit));
        assert!(
            razao.contains(&format!("`Read {}`, e só então", fp.display())),
            "{razao}"
        );
        assert!(!razao.contains("offset="), "{razao}");
    }
}

#[test]
fn g3_read_window_cobre_o_old_string_com_contexto() {
    let (tmp, fp) = projeto_com_arquivo_grande();
    let (p, f) = (tmp.path(), fp.to_str().expect("utf8"));
    assert_eq!(g3_read_window(p, f, "linha 120"), Some((115, 11)));
    // multilinha: 2 linhas + 5 de contexto de cada lado
    assert_eq!(g3_read_window(p, f, "linha 50\nlinha 51"), Some((45, 12)));
    // perto do começo o offset trava em 1
    assert_eq!(g3_read_window(p, f, "linha 2\n"), Some((1, 7)));
    // caminho relativo ao projeto
    assert_eq!(g3_read_window(p, "grande.rs", "linha 120"), Some((115, 11)));
    // sem o que janelar
    assert_eq!(g3_read_window(p, f, ""), None);
    assert_eq!(g3_read_window(p, f, "não existe"), None);
    assert_eq!(g3_read_window(p, "nao-existe.rs", "linha 120"), None);
    // a janela que é o arquivo inteiro não economiza nada
    let tudo: String = (1..=200).map(|i| format!("linha {i}\n")).collect();
    assert_eq!(g3_read_window(p, f, &tudo), None);
}

/// G10 sem rota — a saída que funciona é a linha de bypass do PRÓPRIO comando negado, não a
/// regra solta; e ela passa a cadeia.
#[test]
#[serial(gate_metrics, t3_env)]
fn g10_sem_rota_entrega_a_linha_de_bypass_do_proprio_comando() {
    let tmp = projeto_g10();
    let fora = tempfile::tempdir().expect("tempdir de fora");
    let cmd = format!(
        ".venv/bin/python3 -c 'import pathlib; pathlib.Path(\"{}\").write_text(\"x\")'",
        fora.path().join("saida.md").display()
    );
    let sess = "s13-g10-linha";
    let resp = quinta_chamada(tmp.path(), sess, &cmd);
    assert!(is_deny(&resp), "{resp:?}");
    let r = reason(&resp);
    let linha = format!("{GATE_BYPASS_TOKEN} {cmd}");
    assert!(
        r.contains(&format!("\n  {linha}")),
        "a linha pronta do comando negado: {r}"
    );
    assert!(
        gate(tmp.path(), sess, "Bash", &bash(&linha)).is_none(),
        "a linha entregue passa a cadeia"
    );
    // comando de várias linhas: sem linha quebrada, a regra sobre o comando negado
    let multi = bypass_next_step("python3 - <<'EOF'\nprint(1)\nEOF");
    assert!(
        multi.contains("prefixe o comando negado, verbatim"),
        "{multi}"
    );
    assert!(!multi.contains("\n  "), "{multi}");
}
