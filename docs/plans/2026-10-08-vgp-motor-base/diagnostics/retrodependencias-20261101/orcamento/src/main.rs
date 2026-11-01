use std::collections::HashMap;
use std::io::{self, BufRead};
use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use touring_pyright::achado::L2Resultado;
use touring_pyright::pyright::PyrightSession;

const BUDGET_MS: u64 = 30_000;

#[derive(Deserialize)]
struct Request {
    id: String,
    project_root: PathBuf,
    path: String,
    content: String,
    disk: Option<String>,
    overlays: Vec<(String, String)>,
}

#[derive(Serialize)]
struct Response {
    id: String,
    budget_ms: u64,
    resultado: L2Resultado,
}

fn main() {
    let stdin = io::stdin();
    let mut sessions: HashMap<PathBuf, PyrightSession> = HashMap::new();
    for line in stdin.lock().lines() {
        let line = line.expect("stdin inválido");
        if line.trim().is_empty() {
            continue;
        }
        let req: Request = serde_json::from_str(&line).expect("JSONL de entrada inválido");
        if !sessions.contains_key(&req.project_root) {
            match PyrightSession::discover(&req.project_root) {
                Ok(session) => {
                    sessions.insert(
                        req.project_root.clone(),
                        session.with_budget(Duration::from_millis(BUDGET_MS)),
                    );
                }
                Err(erro) => {
                    let response = Response {
                        id: req.id,
                        budget_ms: BUDGET_MS,
                        resultado: L2Resultado::unknown(format!("{erro}"), 0),
                    };
                    println!("{}", serde_json::to_string(&response).expect("falha ao serializar"));
                    continue;
                }
            }
        }
        let session = sessions.get_mut(&req.project_root).expect("sessão ausente");
        let overlays = req.overlays;
        let resultado = session.check(&req.path, &req.content, req.disk.as_deref(), &overlays);
        let response = Response {
            id: req.id,
            budget_ms: BUDGET_MS,
            resultado,
        };
        println!("{}", serde_json::to_string(&response).expect("falha ao serializar"));
    }
}
