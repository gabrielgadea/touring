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

/// `ETXTBSY`: o executável está aberto para escrita por alguém. Linux, `errno.h`.
const ETXTBSY: i32 = 26;

/// Quantas vezes insistir, e o passo do backoff. O pior caso soma 12 ms — ordens
/// de grandeza abaixo do orçamento de parede do provedor (400 ms em produção).
const TENTATIVAS_ETXTBSY: u32 = 3;
const PASSO_ETXTBSY: Duration = Duration::from_millis(2);

/// O `spawn`, insistindo enquanto o executável estiver ocupado para escrita.
///
/// `ETXTBSY` é transitório por definição: o kernel o devolve enquanto QUALQUER
/// processo mantém o arquivo aberto para escrita, e isso dura o tempo de fechar.
/// Acontece em produção quando um `pip install` reescreve o `.venv/bin/python3`,
/// e na suíte quando outra thread forka na janela entre o `fs::write` de um
/// fixture e o seu `exec` — o filho herda o fd por instantes.
///
/// Sem esta insistência o hook devolve vazio, que é indistinguível de "nada a
/// dizer": medido em 20/09/2026, **7 falhas em 300 execuções** do binário de
/// teste, atingindo três testes DIFERENTES do módulo — a assinatura de um
/// defeito de estado global, não de um teste. Um diagnóstico anterior leu o
/// sintoma como orçamento apertado e subiu o budget de 400 ms para 10 s
/// (13/09/2026); o tempo nunca esteve em jogo, e a suíte falhava em 0,13 s.
fn spawn_com_retry_etxtbsy(
    python: &Path,
    provider: &Path,
    root: &Path,
    rel: &str,
    no: &str,
) -> Result<Child, ProviderOutcome> {
    let mut tentativa = 0;
    loop {
        let mut cmd = Command::new(python);
        cmd.arg(provider)
            .args(["--arquivo", rel, "--no", no, "--stdin", "--brief"])
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        // Grupo próprio: sem isto, `kill` no timeout alcança só o filho direto,
        // e um neto herda o pipe de stdout — o vazamento de thread e processo
        // que o cross-audit de 20/09/2026 mediu dentro do daemon.
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }
        let resultado = cmd.spawn();
        match resultado {
            Ok(child) => return Ok(child),
            Err(e) if e.raw_os_error() == Some(ETXTBSY) && tentativa < TENTATIVAS_ETXTBSY => {
                tentativa += 1;
                std::thread::sleep(PASSO_ETXTBSY * tentativa);
            }
            // Esgotadas as tentativas, o erro viaja COM a contagem: "ainda ocupado
            // depois de 3 tentativas" é um diagnóstico, "Text file busy" sozinho
            // manda o leitor reproduzir às cegas.
            Err(e) if e.raw_os_error() == Some(ETXTBSY) => {
                return Err(ProviderOutcome::Failed(format!(
                    "spawn: {e} — ainda ocupado após {TENTATIVAS_ETXTBSY} tentativas"
                )));
            }
            Err(e) => return Err(ProviderOutcome::Failed(format!("spawn: {e}"))),
        }
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
    let mut child = spawn_com_retry_etxtbsy(&python, &provider, root, rel, no)?;
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
        // de timeout mediu 5 s onde o orçamento era 60 ms.
        //
        // Cross-audit 20/09/2026: `drop` de um `JoinHandle` DESANEXA, não cancela. Com
        // o neto vivo, a thread seguia bloqueada em `read_to_string` e o processo ficava
        // reparentado ao init — dentro de um daemon que vive DIAS, um por timeout, sem
        // teto, sem contador, sem log. O filho agora nasce em process group próprio
        // (`lancar_provedor`), então matar o GRUPO leva o neto junto e o pipe fecha
        // sozinho: a thread termina em vez de vazar.
        matar_grupo(&child);
        drop(leitor);
        return Err(ProviderOutcome::Timeout {
            budget_ms: budget.as_millis() as u64,
        });
    };
    let raw = leitor.join().unwrap_or_default();
    interpretar(status, raw)
}

/// Mata o process group do provedor — o filho E seus netos.
///
/// `child.kill()` envia o sinal só ao filho direto. Como `lancar_provedor` o põe
/// em grupo próprio (`process_group(0)`), o PID do filho É o PGID, e `killpg`
/// alcança a árvore inteira. Sem isso o neto sobrevive segurando o pipe, a
/// thread leitora nunca retorna e o processo é reparentado ao init.
///
/// Fail-open: um grupo que já morreu devolve ESRCH e não há o que fazer a
/// respeito — o caminho de timeout não pode falhar por causa da limpeza.
fn matar_grupo(child: &Child) {
    #[cfg(unix)]
    {
        let pid = child.id();
        if let Ok(pid) = i32::try_from(pid) {
            // SAFETY: `killpg` sobre um PGID que este processo criou; o pior
            // caso é ESRCH (grupo já terminou), que é ignorado de propósito.
            unsafe {
                libc::killpg(pid, libc::SIGKILL);
            }
        }
    }
    #[cfg(not(unix))]
    let _ = child;
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
        // A limpeza é só de ENTRADA, então cada execução da suíte deixa a sua raiz
        // para trás: 176 delas foram medidas em /tmp em 20/09/2026. Varrer as
        // sobras de execuções JÁ MORTAS aqui é o único ponto que roda sempre — um
        // Drop não cobre o teste que entra em pânico. Uma raiz só some se o dono
        // não existe mais: `remove_dir_all` numa raiz viva sabotaria outro
        // processo de teste rodando em paralelo.
        varrer_raizes_orfas();
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("analise/relatoria/50500.000000-2026-00/analysis"))
            .expect("mkdir");
        // A credencial que a varredura exige. Escrita DEPOIS do mkdir e antes
        // de qualquer uso: uma raiz sem ela nunca é apagada por ninguém, o que
        // significa que este arquivo é a única coisa que autoriza a remoção.
        // A credencial: `boot_id|pid|starttime`, não um texto fixo. Sem ela a
        // raiz nunca é apagada por ninguém — este arquivo é a ÚNICA coisa que
        // autoriza a remoção, e o que ele contém é o que prova de quem é.
        let identidade = identidade_do_processo(std::process::id())
            .unwrap_or_else(|| "sem-identidade".to_string());
        fs::write(dir.join(MARCADOR_AUTORIA), format!("{identidade}\n"))
            .expect("marcador de autoria");
        dir
    }

    /// O arquivo que PROVA que a raiz é nossa. Sem ele, nada é apagado.
    const MARCADOR_AUTORIA: &str = ".touring-doc-symbol-fixture";

    /// `boot_id|pid|starttime` — a identidade que distingue um processo de
    /// outro que reciclou o mesmo PID.
    ///
    /// PID sozinho é reciclável: o kernel reusa números, e uma raiz cujo dono
    /// morreu pode ter o PID de um processo VIVO e alheio — ou o contrário.
    /// `(pid, starttime)` identifica unicamente um processo dentro de um boot,
    /// e o `boot_id` fecha o caso entre boots (ele também resolve o PID
    /// namespace: um marcador escrito com o boot_id do host não casa por
    /// acidente lá dentro).
    fn identidade_do_processo(pid: u32) -> Option<String> {
        let boot = fs::read_to_string("/proc/sys/kernel/random/boot_id").ok()?;
        Some(format!("{}|{pid}|{}", boot.trim(), starttime_de(pid)?))
    }

    /// Campo 22 de `/proc/<pid>/stat`.
    ///
    /// Lido a partir do ÚLTIMO `)`: o campo 2 é o nome do executável entre
    /// parênteses e pode conter espaços e parênteses, então dividir a linha
    /// por espaços desde o início erra em qualquer processo com nome exótico.
    fn starttime_de(pid: u32) -> Option<String> {
        let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        // Depois do `)` o primeiro campo é o 3 (state), logo o 22 é o índice 19.
        stat.rsplit_once(')')?
            .1
            .split_whitespace()
            .nth(19)
            .map(str::to_string)
    }

    /// Apaga as raízes desta suíte cujo dono morreu — e só essas.
    ///
    /// Cross-audit 20/09/2026: a primeira versão usava o NOME como única
    /// credencial (`touring_doc_symbol_*_<dígitos>`), e um crítico provou em
    /// laboratório que ela apagava `touring_doc_symbol_export_2026` — cujo
    /// sufixo é uma DATA, não um PID — e um diretório de terceiro com sufixo
    /// numérico. Qualquer nome terminado em dígitos por coincidência (data,
    /// versão, contador, shard) era lido como PID.
    ///
    /// Duas defesas, e a segunda importa mais que a primeira:
    ///
    /// 1. **Marcador de autoria dentro do diretório.** Um nome é uma
    ///    coincidência possível; um arquivo que só esta suíte escreve não é.
    /// 2. **Não-saber nunca autoriza apagar.** `/proc` ausente ou ilegível
    ///    (container distroless, chroot, PID namespace distinto com `/tmp`
    ///    compartilhado) fazia TODA raiz parecer órfã — inclusive as de suítes
    ///    VIVAS, que é exatamente o flaky que o comentário anterior dizia
    ///    evitar. O desenho tratava "não consigo provar vida" como "está
    ///    morto", falhando na direção destrutiva. Agora, sem `/proc` legível,
    ///    a varredura não apaga nada e o vazamento fica — um diretório a mais
    ///    é barato; o diretório errado, não.
    ///
    /// Nota medida: `remove_dir_all` NÃO segue symlink (verificado no mesmo
    /// laboratório: o link morreu, o alvo sobreviveu), então não há cascata
    /// para fora da árvore. A varredura honra `TMPDIR`, como `raiz_temp`.
    fn varrer_raizes_orfas() {
        // Sem /proc não há prova de vida possível — e sem prova de vida não se
        // apaga. Esta é a guarda que inverte o sentido da falha.
        if !Path::new("/proc/self").exists() {
            return;
        }
        let Ok(entradas) = fs::read_dir(std::env::temp_dir()) else {
            return; // sem tmp legível não há o que varrer — nunca falha o teste
        };
        for entrada in entradas.flatten() {
            // `file_type()` do DirEntry NÃO segue symlink (`metadata()` seguiria):
            // um link apontando para fora nunca chega ao `remove_dir_all`, e um
            // arquivo regular não entra só para falhar com ENOTDIR engolido.
            if !entrada.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                continue;
            }
            let nome = entrada.file_name();
            let Some(nome) = nome.to_str() else { continue };
            if !nome.starts_with("touring_doc_symbol_") {
                continue;
            }
            let caminho = entrada.path();
            let marcador = caminho.join(MARCADOR_AUTORIA);
            // `symlink_metadata` NÃO segue o link: o marcador tem de ser um
            // arquivo regular de verdade. `is_file()` seguiria, e como estas
            // raízes vivem num diretório world-writable, um link plantado ali
            // bastaria para forjar a credencial — o vetor que a leitura do
            // marcador reintroduziria se fosse ingênua.
            let regular = fs::symlink_metadata(&marcador)
                .map(|m| m.file_type().is_file())
                .unwrap_or(false);
            if !regular {
                continue;
            }
            let Ok(conteudo) = fs::read_to_string(&marcador) else {
                continue;
            };
            let Some(identidade) = conteudo.lines().next().map(str::trim) else {
                continue;
            };
            if processo_ainda_vivo(identidade) {
                continue; // dono vivo: apagar trocaria o vazamento por um flaky
            }
            // Falha de remoção não é silêncio: uma varredura que apaga pela
            // metade tem de ser distinguível de uma que não rodou.
            if let Err(e) = fs::remove_dir_all(&caminho)
                && e.kind() != std::io::ErrorKind::NotFound
            {
                eprintln!(
                    "varrer_raizes_orfas: {} não removida: {e}",
                    caminho.display()
                );
            }
        }
    }

    /// `true` quando NÃO se pode provar que o dono morreu.
    ///
    /// Toda dúvida responde "vivo", porque a dúvida não pode autorizar um
    /// apagamento: marcador ilegível, formato inesperado, `/proc` que não
    /// responde — tudo preserva. Só duas provas matam: outro boot (e aí um
    /// /tmp em tmpfs já não existiria), ou mesmo boot com o par
    /// `(pid, starttime)` que não está mais lá.
    fn processo_ainda_vivo(identidade: &str) -> bool {
        let mut partes = identidade.split('|');
        let (Some(boot_marcado), Some(pid_txt), Some(start_marcado)) =
            (partes.next(), partes.next(), partes.next())
        else {
            return true; // formato que não entendo nunca autoriza apagar
        };
        let Ok(boot_atual) = fs::read_to_string("/proc/sys/kernel/random/boot_id") else {
            return true;
        };
        if boot_atual.trim() != boot_marcado {
            return false; // outro boot: o dono não existe mais, com certeza
        }
        let Ok(pid) = pid_txt.parse::<u32>() else {
            return true;
        };
        match starttime_de(pid) {
            // Mesmo PID, mas nasceu em outro instante: é outro processo, e o
            // dono original morreu. Sem isto, um PID reciclado preservaria
            // lixo para sempre — ou pior, um PID reciclado por um processo
            // vivo e alheio seria lido como "o meu dono".
            Some(atual) => atual == start_marcado,
            None => false, // não há processo com esse pid: morreu
        }
    }

    #[test]
    fn a_varredura_so_apaga_o_que_prova_ser_dela() {
        // Fixture de TERCEIRO, montado à mão — não pela função que cria a raiz
        // de produção. Um teste que usasse `raiz_temp` aqui traria o marcador
        // de brinde e passaria sem exercitar a credencial.
        let base = std::env::temp_dir();
        let alheio = base.join("touring_doc_symbol_export_2026");
        let _ = fs::remove_dir_all(&alheio);
        fs::create_dir_all(&alheio).expect("fixture alheio");
        fs::write(alheio.join("nao_e_meu.txt"), "conteudo de terceiro").expect("conteudo");

        // O sufixo `2026` parseia como PID e nenhum processo o tem: sob o
        // predicado antigo — só o nome — este diretório era APAGADO, provado
        // em laboratório no cross-audit de 20/09/2026.
        varrer_raizes_orfas();
        assert!(
            alheio.join("nao_e_meu.txt").exists(),
            "diretório de terceiro com sufixo numérico foi apagado: o nome não é credencial"
        );

        // E uma raiz NOSSA, cujo dono está vivo (este processo), sobrevive.
        let minha = raiz_temp("varredura_dono_vivo");
        varrer_raizes_orfas();
        assert!(
            minha.join(MARCADOR_AUTORIA).exists(),
            "a raiz do processo VIVO não pode ser varrida"
        );

        // Já uma raiz nossa cujo dono morreu (identidade de outro boot) sai.
        let morta = base.join("touring_doc_symbol_dono_morto_424242");
        let _ = fs::remove_dir_all(&morta);
        fs::create_dir_all(&morta).expect("raiz morta");
        fs::write(
            morta.join(MARCADOR_AUTORIA),
            "00000000-0000-0000-0000-000000000000|424242|1\n",
        )
        .expect("marcador de boot antigo");
        varrer_raizes_orfas();
        assert!(
            !morta.exists(),
            "raiz com marcador de OUTRO boot é lixo provado e deve sair"
        );

        let _ = fs::remove_dir_all(&alheio);
        let _ = fs::remove_dir_all(&minha);
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
        let layer =
            DocSymbolSignalLayer::with_root(raiz.clone()).with_budget(Duration::from_secs(10));
        let ctx = SignalContext::new(rel, "Lei nº 8.987/1995 proposta").with_cila(3);
        // `enrich` devolve vazio para TODA falha do provedor — a causa existe em
        // `ProviderOutcome` e morria aqui. Um `0 != 2` mandou duas sessões (13/09 e
        // 20/09) reproduzir às cegas, e o remédio de 13/09 (orçamento 400 ms → 10 s)
        // tratou tempo num defeito que não era de tempo. A mensagem só é construída
        // no caminho de falha, e uma 2ª tentativa que passa já diz "transitório".
        let sinais = layer.enrich(&ctx);
        assert_eq!(
            sinais.len(),
            2,
            "o layer devolveu vazio; 2ª chamada ao provedor: {:?}",
            run_provider(
                &raiz,
                rel,
                "VOTO",
                "Lei nº 8.987/1995 proposta",
                Duration::from_secs(10),
            )
            .err()
        );
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
