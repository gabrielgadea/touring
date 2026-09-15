//! S2 — `DocSymbolSignalLayer`: sinais de símbolos documentais (leis, acórdãos do TCU,
//! resoluções da ANTT, processos SEI, valores, cláusulas) para documentos regulatórios em
//! Markdown, com relevância por nó do workflow — a «mana» aplicada ao documento.
//!
//! Origem: plano `docs/plans/2026-09-03-work-outer/plan-2026-09-03-substrato-de-artefato.md`
//! (S2, decisão U10) do projeto `analise`, executado em 05/09/2026. Moldado em
//! [`crate::ast_grep_signal`]: gate barato primeiro, orçamento soft, gate por `cila_level`,
//! cache auto-corretivo endereçado por conteúdo (blake3).
//!
//! # A ponte de repositórios (U10a)
//!
//! O extrator vive no projeto consumidor (`kazuba_rust_core.doc_symbols`, binding PyO3) e a
//! matriz de relevância deriva do `workflow_registry` daquele projeto. **Nenhuma dependência
//! de crate cruzada** (acoplaria dois ciclos de release — U10c, recusado no plano): o layer
//! invoca o provedor Python do projeto (`scripts/lexhub/doc_symbol_sinais.py --stdin
//! --brief`) por subprocesso, com orçamento de parede, e lê JSON tipado. Projeto sem provedor
//! ⇒ o layer não emite nada, e a decisão custa uma `is_file` (o gate barato primeiro).
//!
//! # Latência
//!
//! O subprocesso Python custa ~0,5–1,5 s a frio — inaceitável no caminho quente de
//! `pre_read`. Por isso: (1) o gate de caminho recusa tudo que não seja `.md` sob
//! `relatoria/`; (2) o cache em disco por `blake3(texto ‖ nó)` devolve em <1 ms nas leituras
//! seguintes do mesmo conteúdo; (3) o orçamento padrão é `DEFAULT_BUDGET` (medido em
//! 05/09/2026: 0,30 s por chamada, frio ou com o cache do provedor — o custo é o import do
//! registry e do binding, não a extração) — estourou, o
//! filho é morto e o layer devolve nada (nunca um resultado parcial disfarçado de completo);
//! (4) `post_edit` pré-aquece o cache com `WARM_BUDGET`. A latência medida decide U10b
//! (comando do daemon) — o plano manda medir antes de evoluir.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use crate::signal_layer::{SignalContext, SignalLayer};

// Visibilidade reduzida em 2026-09-13 (REGRA #0): DEFAULT_BUDGET, PROVIDER_REL,
// CACHE_REL e MAX_SIGNALS só têm leitores neste arquivo; o `pub` os deixava
// órfãos na auditoria de wiring sem servir a ninguém. WARM_BUDGET e PYTHON_REL
// seguem públicos porque têm consumidores fora daqui.
/// Orçamento de parede no caminho quente (`pre_read`/`pre_edit`/`pre_write`).
const DEFAULT_BUDGET: Duration = Duration::from_millis(400);
/// Orçamento de pré-aquecimento (`post_edit`): o hook não bloqueia ninguém.
const WARM_BUDGET: Duration = Duration::from_millis(3000);
/// O provedor Python do projeto consumidor (U10a), relativo à raiz do projeto.
const PROVIDER_REL: &str = "scripts/lexhub/doc_symbol_sinais.py";
/// O interpretador do projeto (o binding PyO3 vive no `.venv` dele).
const PYTHON_REL: &str = ".venv/bin/python3";
/// O cache em disco, relativo à raiz do projeto (o hook é um processo efêmero —
/// um cache em memória morreria com ele).
const CACHE_REL: &str = ".claude/touring/doc_signal_cache";
/// Teto de sinais por documento (o provedor já corta por orçamento; este é o cinto).
const MAX_SIGNALS: usize = 8;
/// Peso máximo de um sinal documental: abaixo dos sinais de topo (gotchas, blast radius),
/// ao lado do `[risk]` do ast-grep (0,85).
const PESO_MAXIMO: f32 = 0.9;
/// Passo do laço de espera pelo filho.
const PASSO_ESPERA: Duration = Duration::from_millis(5);

/// O que o provedor devolve (`--brief`), já tipado.
#[derive(Debug, Clone, PartialEq)]
pub struct DocSignals {
    /// `(utilidade, linha densa)` em ordem decrescente de utilidade.
    pub emitidos: Vec<(f32, String)>,
    /// Símbolos extraídos antes de qualquer filtro.
    pub n_simbolos: usize,
    /// Símbolos acima do piso de utilidade para o nó.
    pub candidatos: usize,
    /// `true` quando o provedor cortou pelo orçamento dele.
    pub partial: bool,
    /// `true` quando o provedor já tinha os símbolos em cache.
    pub cache_hit: bool,
}

/// Por que o provedor não respondeu — cada caso ensina a correção.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderOutcome {
    /// Sem `PROVIDER_REL` ou sem `PYTHON_REL` na raiz: projeto sem a ponte U10a.
    Unavailable,
    /// O filho passou do orçamento e foi morto.
    Timeout {
        /// O orçamento de parede que o filho estourou, em milissegundos.
        budget_ms: u64,
    },
    /// Saída não parseável ou exit ≠ 0.
    Failed(String),
}

/// `true` para o que o layer se dispõe a analisar: Markdown sob `relatoria/`.
///
/// O gate é por caminho, não por conteúdo, porque precisa custar nada: todo `.md`
/// fora de um processo (docs, planos, memórias) sai aqui sem tocar o disco.
pub fn e_documento_regulatorio(rel: &str) -> bool {
    let lower = rel.replace('\\', "/").to_ascii_lowercase();
    lower.ends_with(".md") && (lower.contains("/relatoria/") || lower.starts_with("relatoria/"))
}

/// O nó do workflow que o caminho sugere — a tabela é declarada, não inferida.
///
/// O hook não sabe em que nó a sessão está; o nome do arquivo é o melhor sinal barato.
/// Desconhecido cai em `ANALISE`, a lente mais genérica (nunca um nó inventado).
pub fn no_corrente(rel: &str) -> &'static str {
    let norm = rel.replace('\\', "/");
    let nome = norm
        .rsplit('/')
        .next()
        .unwrap_or(norm.as_str())
        .to_ascii_uppercase();
    let por_nome = [
        ("VOTO", "VOTO"),
        ("RELATORIO_ANALITICO", "RELATORIO-NARRATIVO"),
        ("RELATORIO_NARRATIVO", "RELATORIO-NARRATIVO"),
        ("ESQUELETO_NARRATIVO", "RELATORIO-NARRATIVO"),
        ("FICHA", "FICHA"),
    ];
    if let Some((_, no)) = por_nome
        .iter()
        .find(|(prefixo, _)| nome.starts_with(prefixo))
    {
        return no;
    }
    if norm.contains("/converted/") || norm.contains("/extracted/") {
        return "CONVERSAO";
    }
    "ANALISE"
}

fn par_emitido(e: &serde_json::Value) -> Option<(f32, String)> {
    let par = e.as_array()?;
    let u = par.first()?.as_f64()? as f32;
    let linha = par.get(1)?.as_str()?.to_string();
    Some((u, linha))
}

/// Lê o JSON `--brief` do provedor. `None` para qualquer forma inesperada — o layer
/// prefere emitir nada a emitir um sinal mal lido.
pub fn parse_provider_json(raw: &str) -> Option<DocSignals> {
    let v: serde_json::Value = serde_json::from_str(raw.trim()).ok()?;
    let emitidos = v
        .get("emitidos")?
        .as_array()?
        .iter()
        .filter_map(par_emitido)
        .collect();
    let n = |k: &str| v.get(k).and_then(serde_json::Value::as_u64).unwrap_or(0) as usize;
    let b = |k: &str| {
        v.get(k)
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
    };
    Some(DocSignals {
        emitidos,
        n_simbolos: n("n_simbolos"),
        candidatos: n("candidatos"),
        partial: b("partial"),
        cache_hit: b("cache_hit"),
    })
}

/// A chave do cache: conteúdo e nó — o mesmo texto lido por outro nó é outra pergunta.
pub fn cache_key(texto: &str, no: &str) -> String {
    let mut h = blake3::Hasher::new();
    h.update(texto.as_bytes());
    h.update(b"\0");
    h.update(no.as_bytes());
    h.finalize().to_hex().to_string()
}

fn cache_read(dir: &Path, key: &str) -> Option<DocSignals> {
    let raw = fs::read_to_string(dir.join(format!("{key}.json"))).ok()?;
    parse_provider_json(&raw)
}

fn cache_write(dir: &Path, key: &str, raw: &str) {
    if fs::create_dir_all(dir).is_err() {
        return;
    }
    // atômico: escreve ao lado e renomeia — um hook morto no meio não deixa JSON pela metade
    let destino = dir.join(format!("{key}.json"));
    let tmp = dir.join(format!("{key}.json.tmp"));
    if fs::write(&tmp, raw).is_ok() {
        let _ = fs::rename(&tmp, &destino);
    }
}

/// Lança o provedor e entrega o texto por stdin; o filho lê tudo antes de responder.
fn lancar_provedor(
    root: &Path,
    rel: &str,
    no: &str,
    texto: &str,
) -> Result<Child, ProviderOutcome> {
    let python = root.join(PYTHON_REL);
    let provider = root.join(PROVIDER_REL);
    if !python.is_file() || !provider.is_file() {
        return Err(ProviderOutcome::Unavailable);
    }
    let mut child = Command::new(&python)
        .arg(&provider)
        .args(["--arquivo", rel, "--no", no, "--stdin", "--brief"])
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| ProviderOutcome::Failed(format!("spawn: {e}")))?;
    if let Some(mut stdin) = child.stdin.take() {
        // um erro de escrita só significa que o filho já morreu — `esperar` reporta
        let _ = stdin.write_all(texto.as_bytes());
    }
    Ok(child)
}

/// Espera o filho até o orçamento; estourou, mata e devolve `None`.
fn esperar(child: &mut Child, budget: Duration) -> Result<Option<ExitStatus>, ProviderOutcome> {
    let inicio = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(Some(status)),
            Ok(None) if inicio.elapsed() >= budget => {
                let _ = child.kill();
                let _ = child.wait();
                return Ok(None);
            }
            Ok(None) => std::thread::sleep(PASSO_ESPERA),
            Err(e) => return Err(ProviderOutcome::Failed(format!("wait: {e}"))),
        }
    }
}

/// Invoca o provedor com orçamento de parede. O texto vai por stdin (a mutação proposta
/// pode ainda não existir em disco — S1); o caminho vai só como identidade do documento.
/// Devolve os sinais tipados e o JSON cru (o que o cache guarda).
pub fn run_provider(
    root: &Path,
    rel: &str,
    no: &str,
    texto: &str,
    budget: Duration,
) -> Result<(DocSignals, String), ProviderOutcome> {
    let mut child = lancar_provedor(root, rel, no, texto)?;
    let mut stdout = child.stdout.take();
    let leitor = std::thread::spawn(move || {
        let mut buf = String::new();
        if let Some(out) = stdout.as_mut() {
            let _ = out.read_to_string(&mut buf);
        }
        buf
    });
    let Some(status) = esperar(&mut child, budget)? else {
        // Morto pelo orçamento. Um neto do provedor (um `sleep`, um worker) pode segurar o
        // pipe de stdout aberto: esperar o leitor aqui bloquearia até ELE morrer — o teste
        // de timeout mediu 5 s onde o orçamento era 60 ms. A thread leitora fica órfã e
        // termina quando o pipe fechar; o hook não a espera.
        drop(leitor);
        return Err(ProviderOutcome::Timeout {
            budget_ms: budget.as_millis() as u64,
        });
    };
    let raw = leitor.join().unwrap_or_default();
    interpretar(status, raw)
}

/// O veredito do filho que terminou: exit ≠ 0, JSON inesperado, ou os sinais.
fn interpretar(s: ExitStatus, raw: String) -> Result<(DocSignals, String), ProviderOutcome> {
    if !s.success() {
        return Err(ProviderOutcome::Failed(format!("exit {s}")));
    }
    parse_provider_json(&raw)
        .map(|d| (d, raw))
        .ok_or_else(|| ProviderOutcome::Failed("JSON inesperado".to_string()))
}

/// Os sinais no formato do pipeline: `(peso, "[doc:<nó>] <linha densa>")`.
pub fn formatar(sinais: &DocSignals, no: &str) -> Vec<(f32, String)> {
    sinais
        .emitidos
        .iter()
        .take(MAX_SIGNALS)
        .map(|(u, linha)| {
            (
                u.clamp(0.0, 1.0) * PESO_MAXIMO,
                format!("[doc:{no}] {linha}"),
            )
        })
        .collect()
}

// ─── SignalLayer impl ─────────────────────────────────────────────────

/// Camada que injeta os símbolos documentais relevantes para o nó corrente, um por linha.
///
/// Ligada aos pipelines de `pre_read`, `pre_write` e `pre_edit`; `should_run` gata em
/// `cila_level >= 2`, como o `[risk]` do ast-grep, porque o subprocesso só se paga em
/// tarefas médias ou maiores.
pub struct DocSymbolSignalLayer {
    /// Raiz do projeto; `None` resolve pelo diretório corrente (produção).
    project_root: Option<PathBuf>,
    /// Orçamento do caminho quente; `post_edit` usa [`WARM_BUDGET`] independentemente.
    budget: Duration,
}

impl DocSymbolSignalLayer {
    /// Camada com raiz explícita (testes e atores multi-projeto).
    pub fn with_root(root: PathBuf) -> Self {
        Self {
            project_root: Some(root),
            budget: DEFAULT_BUDGET,
        }
    }

    /// Orçamento do caminho quente diferente do padrão.
    pub fn with_budget(mut self, budget: Duration) -> Self {
        self.budget = budget;
        self
    }

    fn root(&self) -> PathBuf {
        match &self.project_root {
            Some(r) => r.clone(),
            None => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        }
    }

    /// O gate barato: a ponte U10a existe neste projeto?
    pub fn provider_available(&self) -> bool {
        let root = self.root();
        root.join(PYTHON_REL).is_file() && root.join(PROVIDER_REL).is_file()
    }

    fn budget_for(&self, hook_name: &str) -> Duration {
        if hook_name == "post_edit" {
            WARM_BUDGET
        } else {
            self.budget
        }
    }

    /// Cache primeiro; só então o provedor, cujo JSON cru alimenta o cache.
    fn sinais_de(
        &self,
        root: &Path,
        rel: &str,
        no: &str,
        texto: &str,
        hook: &str,
    ) -> Option<DocSignals> {
        let chave = cache_key(texto, no);
        let cache = root.join(CACHE_REL);
        if let Some(hit) = cache_read(&cache, &chave) {
            return Some(hit);
        }
        let (sinais, raw) = run_provider(root, rel, no, texto, self.budget_for(hook)).ok()?;
        cache_write(&cache, &chave, &raw);
        Some(sinais)
    }
}

/// Camada com a raiz no diretório corrente (produção); `with_root` fixa outra.
impl Default for DocSymbolSignalLayer {
    fn default() -> Self {
        Self {
            project_root: None,
            budget: DEFAULT_BUDGET,
        }
    }
}

impl SignalLayer for DocSymbolSignalLayer {
    fn name(&self) -> &'static str {
        "doc_symbols"
    }

    fn enrich(&self, ctx: &SignalContext<'_>) -> Vec<(f32, String)> {
        if !e_documento_regulatorio(ctx.file_path) || !self.provider_available() {
            return Vec::new();
        }
        let root = self.root();
        let proposto = ctx.analysable_text();
        let do_disco;
        let texto: &str = if proposto.is_empty() {
            do_disco = fs::read_to_string(root.join(ctx.file_path)).unwrap_or_default();
            &do_disco
        } else {
            proposto
        };
        if texto.trim().is_empty() {
            return Vec::new();
        }
        let no = no_corrente(ctx.file_path);
        match self.sinais_de(&root, ctx.file_path, no, texto, ctx.hook_name) {
            Some(sinais) => formatar(&sinais, no),
            None => Vec::new(),
        }
    }

    fn should_run(&self, cila_level: usize) -> bool {
        cila_level >= 2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BRIEF: &str = r#"{"no": "VOTO", "n_simbolos": 3, "candidatos": 2, "partial": false, "cache_hit": false, "emitidos": [[0.9, "Citation(LeiFederal) Lei Federal nº 8.987/1995 @L12 c1.00 u0.90"], [0.75, "Citation(AcordaoTCU) Acórdão TCU nº 2337/2026 @L40 c0.75 u0.75"]]}"#;

    fn raiz_temp(nome: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("touring_doc_symbol_{nome}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("analise/relatoria/50500.000000-2026-00/analysis"))
            .expect("mkdir");
        dir
    }

    /// Um provedor de mentira: um shell script no lugar do `.venv/bin/python3`.
    fn provedor_falso(raiz: &Path, corpo: &str) {
        use std::os::unix::fs::PermissionsExt;
        fs::create_dir_all(raiz.join(".venv/bin")).expect("venv");
        fs::create_dir_all(raiz.join("scripts/lexhub")).expect("lexhub");
        fs::write(raiz.join(PROVIDER_REL), "# provedor de teste\n").expect("provider");
        let py = raiz.join(PYTHON_REL);
        fs::write(&py, format!("#!/bin/sh\n{corpo}\n")).expect("python falso");
        fs::set_permissions(&py, fs::Permissions::from_mode(0o755)).expect("chmod");
    }

    #[test]
    fn a_tabela_do_no_corrente_e_declarada() {
        assert_eq!(no_corrente("analise/relatoria/x/VOTO_DAA.md"), "VOTO");
        assert_eq!(
            no_corrente("analise/relatoria/x/RELATORIO_ANALITICO.md"),
            "RELATORIO-NARRATIVO"
        );
        assert_eq!(
            no_corrente("analise/relatoria/x/analysis/ESQUELETO_NARRATIVO.md"),
            "RELATORIO-NARRATIVO"
        );
        assert_eq!(
            no_corrente("analise/relatoria/x/FICHA_PROCESSUAL.md"),
            "FICHA"
        );
        assert_eq!(
            no_corrente("analise/relatoria/x/converted/SEI_x/[03]-1_Oficio.md"),
            "CONVERSAO"
        );
        assert_eq!(
            no_corrente("analise/relatoria/x/analysis/F10_overview.md"),
            "ANALISE"
        );
    }

    #[test]
    fn o_gate_de_caminho_recusa_o_que_nao_e_documento_de_processo() {
        assert!(e_documento_regulatorio("analise/relatoria/50500.1/VOTO.md"));
        assert!(e_documento_regulatorio("relatoria/x/a.md"));
        assert!(!e_documento_regulatorio("docs/plans/plano.md"));
        assert!(!e_documento_regulatorio(
            "analise/relatoria/x/analysis/computed_x.json"
        ));
        assert!(!e_documento_regulatorio(
            "crates/touring-hooks-shared/src/lib.rs"
        ));
    }

    #[test]
    fn o_json_do_provedor_e_lido_tipado_e_o_inesperado_vira_none() {
        let d = parse_provider_json(BRIEF).expect("brief válido");
        assert_eq!(d.emitidos.len(), 2);
        assert!((d.emitidos[0].0 - 0.9).abs() < 1e-6);
        assert_eq!(d.n_simbolos, 3);
        assert_eq!(d.candidatos, 2);
        assert!(!d.partial);
        assert!(parse_provider_json("não é json").is_none());
        assert!(parse_provider_json(r#"{"sem": "emitidos"}"#).is_none());
    }

    #[test]
    fn formatar_pesa_por_utilidade_e_nomeia_o_no() {
        let d = parse_provider_json(BRIEF).expect("brief");
        let s = formatar(&d, "VOTO");
        assert_eq!(s.len(), 2);
        assert!((s[0].0 - 0.81).abs() < 1e-5);
        assert!(s[0].1.starts_with("[doc:VOTO] Citation(LeiFederal)"));
    }

    #[test]
    fn a_chave_do_cache_muda_com_o_texto_e_com_o_no() {
        let a = cache_key("x", "VOTO");
        assert_ne!(a, cache_key("y", "VOTO"));
        assert_ne!(a, cache_key("x", "ANALISE"));
        assert_eq!(a, cache_key("x", "VOTO"));
    }

    #[test]
    fn sem_provedor_o_layer_emite_nada_e_nao_toca_o_disco() {
        let raiz = raiz_temp("sem_provedor");
        let rel = "analise/relatoria/50500.000000-2026-00/VOTO.md";
        fs::write(raiz.join(rel), "Lei nº 8.987/1995").expect("md");
        // A generous budget: these tests prove WHAT the layer returns, not how fast.
        // The production budget (400 ms) failed them under a parallel workspace
        // build (measured 13/09/2026); the timeout path has its own 60 ms test.
        let layer =
            DocSymbolSignalLayer::with_root(raiz.clone()).with_budget(Duration::from_secs(10));
        let ctx = SignalContext::new(rel, "").with_cila(3);
        assert!(layer.enrich(&ctx).is_empty());
        assert!(!raiz.join(CACHE_REL).exists());
    }

    #[test]
    fn com_provedor_o_layer_emite_e_a_segunda_leitura_vem_do_cache() {
        let raiz = raiz_temp("com_provedor");
        provedor_falso(&raiz, &format!("cat >/dev/null; echo '{BRIEF}'"));
        let rel = "analise/relatoria/50500.000000-2026-00/VOTO.md";
        fs::write(raiz.join(rel), "Lei nº 8.987/1995 e Acórdão 2337/2026").expect("md");
        // A generous budget: these tests prove WHAT the layer returns, not how fast.
        // The production budget (400 ms) failed them under a parallel workspace
        // build (measured 13/09/2026); the timeout path has its own 60 ms test.
        let layer =
            DocSymbolSignalLayer::with_root(raiz.clone()).with_budget(Duration::from_secs(10));
        let ctx = SignalContext::new(rel, "").with_cila(3);
        let s1 = layer.enrich(&ctx);
        assert_eq!(s1.len(), 2, "{s1:?}");
        assert!(s1[0].1.starts_with("[doc:VOTO]"));
        let entradas = fs::read_dir(raiz.join(CACHE_REL)).expect("cache").count();
        assert_eq!(entradas, 1, "uma entrada de cache após a 1ª leitura");
        // 2ª leitura: o provedor falso é trocado por um que FALHA — se o cache não
        // servir, o layer devolve vazio e o teste pega
        provedor_falso(&raiz, "exit 3");
        let s2 = layer.enrich(&ctx);
        assert_eq!(s1, s2, "a 2ª leitura veio do cache, não do provedor");
    }

    #[test]
    fn o_texto_proposto_vence_o_disco_e_o_cila_baixo_desliga() {
        let raiz = raiz_temp("proposto");
        provedor_falso(&raiz, &format!("cat >/dev/null; echo '{BRIEF}'"));
        let rel = "analise/relatoria/50500.000000-2026-00/VOTO.md";
        // sem arquivo em disco: só a proposta existe (pre_write)
        // A generous budget: these tests prove WHAT the layer returns, not how fast.
        // The production budget (400 ms) failed them under a parallel workspace
        // build (measured 13/09/2026); the timeout path has its own 60 ms test.
        let layer = DocSymbolSignalLayer::with_root(raiz).with_budget(Duration::from_secs(10));
        let ctx = SignalContext::new(rel, "Lei nº 8.987/1995 proposta").with_cila(3);
        assert_eq!(layer.enrich(&ctx).len(), 2);
        assert!(!layer.should_run(1));
        assert!(layer.should_run(2));
    }

    #[test]
    fn estourar_o_orcamento_mata_o_filho_e_emite_nada() {
        let raiz = raiz_temp("timeout");
        provedor_falso(&raiz, "cat >/dev/null; sleep 5; echo '{}'");
        let rel = "analise/relatoria/50500.000000-2026-00/VOTO.md";
        fs::write(raiz.join(rel), "Lei nº 8.987/1995").expect("md");
        let inicio = Instant::now();
        let r = run_provider(
            &raiz,
            rel,
            "VOTO",
            "Lei nº 8.987/1995",
            Duration::from_millis(60),
        );
        assert_eq!(r.err(), Some(ProviderOutcome::Timeout { budget_ms: 60 }));
        assert!(
            inicio.elapsed() < Duration::from_millis(1500),
            "o kill respeitou o orçamento"
        );
        let layer =
            DocSymbolSignalLayer::with_root(raiz.clone()).with_budget(Duration::from_millis(60));
        assert!(
            layer
                .enrich(&SignalContext::new(rel, "").with_cila(3))
                .is_empty()
        );
        assert!(!raiz.join(CACHE_REL).exists(), "timeout não grava cache");
    }

    #[test]
    fn provedor_ausente_e_falha_sao_nomeados() {
        let raiz = raiz_temp("outcomes");
        assert_eq!(
            run_provider(&raiz, "x.md", "VOTO", "t", DEFAULT_BUDGET).err(),
            Some(ProviderOutcome::Unavailable)
        );
        provedor_falso(&raiz, "cat >/dev/null; echo 'isto não é json'");
        assert!(matches!(
            run_provider(&raiz, "x.md", "VOTO", "t", DEFAULT_BUDGET).err(),
            Some(ProviderOutcome::Failed(_))
        ));
    }
}
