//! Cost probe of the layer-2 verifiers (VL0 §6 / proposta §7 risk): time until the first diagnostic,
//! warm latency and the peak resident memory of the verifier's whole process tree.
//!
//! `#[ignore]`: it starts a real verifier on a real project and is meant to be run alone, under
//! `hw-slot`, on a machine that is not busy. Parameters come from the environment:
//!
//! ```text
//! VGP_L2_SONDA_SERVIDOR  rust-analyzer | tsserver | deno | pyright
//! VGP_L2_SONDA_RAIZ      the project root
//! VGP_L2_SONDA_ARQUIVO   an existing file of the project, relative to the root
//! VGP_L2_SONDA_N         warm checks after the first measured one (default 15)
//! VGP_L2_SONDA_OUT       a file that receives the JSON result (also printed)
//! ```
//!
//! The file is checked as it is on disk (`content == disk`), so nothing is invented and nothing is
//! written: the probe measures what the layer costs, not what it finds. The first call only primes a
//! slow verifier in the background (`unknown: aquecendo`); "time to first diagnostic" is the wall time
//! from that call until the first measured result.

mod common;

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

use common::{LIMITE_DE_AQUECIMENTO, encerrar_tudo_e_esperar_os_filhos};
use serde_json::json;
use touring_pyright::achado::Verdict;
use touring_pyright::pool;
use touring_pyright::servidor::Servidor;

fn env(nome: &str) -> Option<String> {
    std::env::var(nome).ok().filter(|v| !v.is_empty())
}

/// `(pid → ppid)` of every process, read from `/proc/<pid>/stat`.
fn arvore_de_processos() -> HashMap<u32, u32> {
    let mut mapa = HashMap::new();
    let Ok(dir) = fs::read_dir("/proc") else {
        return mapa;
    };
    for entrada in dir.flatten() {
        let Some(pid) = entrada
            .file_name()
            .to_str()
            .and_then(|n| n.parse::<u32>().ok())
        else {
            continue;
        };
        let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) else {
            continue;
        };
        let Some(depois) = stat.rfind(')').map(|i| &stat[i + 1..]) else {
            continue;
        };
        if let Some(ppid) = depois
            .split_whitespace()
            .nth(1)
            .and_then(|p| p.parse().ok())
        {
            mapa.insert(pid, ppid);
        }
    }
    mapa
}

/// `VmRSS` of `pid` in KiB.
fn rss_kib(pid: u32) -> Option<u64> {
    let status = fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let linha = status.lines().find(|l| l.starts_with("VmRSS:"))?;
    linha.split_whitespace().nth(1)?.parse().ok()
}

/// Every descendant of this test process (children, grandchildren, …).
fn descendentes() -> Vec<u32> {
    let eu = std::process::id();
    let mapa = arvore_de_processos();
    let mut achados = Vec::new();
    let mut fronteira = vec![eu];
    while let Some(pai) = fronteira.pop() {
        for (&pid, &ppid) in &mapa {
            if ppid == pai {
                achados.push(pid);
                fronteira.push(pid);
            }
        }
    }
    achados
}

/// The hottest package/core temperature in °C, from `/sys/class/hwmon`.
fn temperatura_c() -> Option<f64> {
    let mut maxima: Option<f64> = None;
    for hwmon in fs::read_dir("/sys/class/hwmon").ok()?.flatten() {
        for arquivo in fs::read_dir(hwmon.path()).ok()?.flatten() {
            let nome = arquivo.file_name();
            let nome = nome.to_string_lossy();
            if nome.starts_with("temp")
                && nome.ends_with("_input")
                && let Some(milli) = fs::read_to_string(arquivo.path())
                    .ok()
                    .and_then(|t| t.trim().parse::<f64>().ok())
            {
                maxima = Some(maxima.map_or(milli / 1000.0, |m| m.max(milli / 1000.0)));
            }
        }
    }
    maxima
}

fn percentil(ordenado: &[u64], p: usize) -> u64 {
    ordenado[(ordenado.len() * p / 100).min(ordenado.len() - 1)]
}

#[test]
#[ignore = "starts a real verifier on a real project; run alone, under hw-slot"]
fn cost_of_one_verifier_on_one_project() {
    let Some(nome) = env("VGP_L2_SONDA_SERVIDOR") else {
        eprintln!(
            "sonda de custo: defina VGP_L2_SONDA_SERVIDOR/RAIZ/ARQUIVO para medir (nada a fazer)"
        );
        return;
    };
    let servidor = Servidor::do_nome(&nome).expect("a known verifier name");
    let raiz = PathBuf::from(env("VGP_L2_SONDA_RAIZ").expect("VGP_L2_SONDA_RAIZ"));
    let arquivo = env("VGP_L2_SONDA_ARQUIVO").expect("VGP_L2_SONDA_ARQUIVO");
    let n: usize = env("VGP_L2_SONDA_N")
        .and_then(|v| v.parse().ok())
        .unwrap_or(15);
    let conteudo = fs::read_to_string(raiz.join(&arquivo)).expect("the probe file must exist");

    pool::habilitar_neste_processo();
    let parar = Arc::new(AtomicBool::new(false));
    let pico_total = Arc::new(AtomicU64::new(0));
    let pico_maior = Arc::new(AtomicU64::new(0));
    let amostrador = {
        let (parar, pico_total, pico_maior) = (
            Arc::clone(&parar),
            Arc::clone(&pico_total),
            Arc::clone(&pico_maior),
        );
        std::thread::spawn(move || {
            while !parar.load(Ordering::Relaxed) {
                let rss: Vec<u64> = descendentes().into_iter().filter_map(rss_kib).collect();
                pico_total.fetch_max(rss.iter().sum(), Ordering::Relaxed);
                pico_maior.fetch_max(rss.iter().copied().max().unwrap_or(0), Ordering::Relaxed);
                std::thread::sleep(Duration::from_millis(50));
            }
        })
    };

    let temp_antes = temperatura_c();
    let inicio = Instant::now();
    let mut chamadas_ate_o_primeiro = 0;
    let primeiro = loop {
        chamadas_ate_o_primeiro += 1;
        let r = pool::checar_em(servidor, &raiz, &arquivo, &conteudo, Some(&conteudo), &[]);
        let aquecendo = r.motivo.as_deref().is_some_and(|m| m.contains("aquecendo"));
        if !aquecendo || inicio.elapsed() > LIMITE_DE_AQUECIMENTO {
            break r;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let ate_o_primeiro_ms = inicio.elapsed().as_millis();
    let temp_depois_do_frio = temperatura_c();

    let mut quentes: Vec<u64> = Vec::new();
    for _ in 0..n {
        let t0 = Instant::now();
        let r = pool::checar_em(servidor, &raiz, &arquivo, &conteudo, Some(&conteudo), &[]);
        assert_ne!(
            r.verdict,
            Verdict::Unknown,
            "a warm check must be measured: {r:#?}"
        );
        quentes.push(u64::try_from(t0.elapsed().as_millis()).unwrap_or(u64::MAX));
    }
    quentes.sort_unstable();
    let rss_final: u64 = descendentes().into_iter().filter_map(rss_kib).sum();
    let temp_depois = temperatura_c();

    parar.store(true, Ordering::Relaxed);
    amostrador.join().expect("sampler thread");
    let encerrou = encerrar_tudo_e_esperar_os_filhos();

    let resultado = json!({
        "servidor": servidor.nome(),
        "raiz": raiz,
        "arquivo": arquivo,
        "bytes_do_arquivo": conteudo.len(),
        "primeiro_resultado": format!("{:?}", primeiro.verdict),
        "motivo_do_primeiro": primeiro.motivo,
        "chamadas_ate_o_primeiro": chamadas_ate_o_primeiro,
        "ate_o_primeiro_diagnostico_ms": ate_o_primeiro_ms,
        "verificacao_do_primeiro_ms": primeiro.elapsed_ms,
        "quentes_n": quentes.len(),
        "quentes_ms": {
            "min": quentes.first(), "p50": percentil(&quentes, 50), "p95": percentil(&quentes, 95), "max": quentes.last(),
        },
        "rss_pico_da_arvore_mib": pico_total.load(Ordering::Relaxed) / 1024,
        "rss_do_maior_processo_pico_mib": pico_maior.load(Ordering::Relaxed) / 1024,
        "rss_final_da_arvore_mib": rss_final / 1024,
        "temperatura_c": {"antes": temp_antes, "depois_do_frio": temp_depois_do_frio, "depois": temp_depois},
        "encerrou_sem_orfaos": encerrou,
    });
    println!("SONDA {resultado}");
    if let Some(saida) = env("VGP_L2_SONDA_OUT") {
        fs::write(
            saida,
            serde_json::to_string_pretty(&resultado).expect("serialize"),
        )
        .expect("write the result");
    }
    assert!(encerrou, "the verifier left a process behind");
}
