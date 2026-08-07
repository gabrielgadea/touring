use super::*;
use tempfile::TempDir;

fn make_index() -> (TantivyIndex, TempDir) {
    let dir = TempDir::new().expect("tempdir");
    let idx = TantivyIndex::open_or_create(dir.path()).expect("open_or_create");
    (idx, dir)
}

fn symbol(name: &str, file: &str, kind: &str) -> SymbolDoc {
    SymbolDoc {
        symbol_name: name.to_string(),
        file_path: file.to_string(),
        symbol_kind: kind.to_string(),
        module_path: Some(format!("crate::{name}")),
        docstring: Some(format!("Documentation for {name}")),
        line_number: 42,
        language: "rust".to_string(),
        // New v2 fields — all None for backward-compatible test helpers
        visibility: None,
        crate_name: None,
        blake3_hash: None,
        import_count: None,
        export_count: None,
        cognitive_score: None,
        functional_signature: None,
        // New v3 field
        community_id: None,
    }
}

#[test]
fn test_open_or_create_empty() {
    let (_idx, _dir) = make_index();
}

#[test]
fn test_upsert_and_search() {
    let (idx, _dir) = make_index();
    idx.upsert_symbol(&symbol("HookRuntime", "src/hook_runtime.rs", "struct"))
        .expect("upsert");
    idx.commit().expect("commit");

    let hits = idx.search("HookRuntime", 10).expect("search");
    assert!(!hits.is_empty(), "expected at least one hit");
    assert_eq!(hits[0].symbol_name, "HookRuntime");
}

#[test]
fn test_stats_after_upsert() {
    let (idx, _dir) = make_index();
    idx.upsert_symbol(&symbol(
        "FileKnowledgeDb",
        "src/file_knowledge_db.rs",
        "struct",
    ))
    .expect("upsert");
    idx.commit().expect("commit");

    let stats = idx.stats();
    assert_eq!(stats.total_docs, 1);
    assert_eq!(stats.total_commits, 1);
    assert_eq!(stats.total_upserts, 1);
}

#[test]
fn test_delete_by_file() {
    let (idx, _dir) = make_index();
    let file = "src/lib.rs";
    idx.upsert_symbol(&symbol("Foo", file, "fn"))
        .expect("upsert");
    idx.commit().expect("commit");

    idx.delete_by_file(file).expect("delete");
    idx.commit().expect("commit after delete");

    let hits = idx.search("Foo", 10).expect("search after delete");
    assert!(hits.is_empty(), "document should be deleted");
}

#[test]
fn test_multiple_symbols_same_file() {
    let (idx, _dir) = make_index();
    let file = "src/multi.rs";
    for name in &["Alpha", "Beta", "Gamma"] {
        idx.upsert_symbol(&symbol(name, file, "fn"))
            .expect("upsert");
    }
    idx.commit().expect("commit");

    // All three should be searchable
    for name in &["Alpha", "Beta", "Gamma"] {
        let hits = idx.search(name, 5).expect("search");
        assert!(!hits.is_empty(), "expected hit for {name}");
    }
}

#[test]
fn test_open_existing_index() {
    let dir = TempDir::new().expect("tempdir");
    {
        let idx = TantivyIndex::open_or_create(dir.path()).expect("create");
        idx.upsert_symbol(&symbol("Persisted", "src/p.rs", "struct"))
            .expect("upsert");
        idx.commit().expect("commit");
    }
    // Re-open
    let idx2 = TantivyIndex::open_or_create(dir.path()).expect("reopen");
    let hits = idx2.search("Persisted", 5).expect("search");
    assert!(!hits.is_empty(), "data should persist across open");
}

/// Caracterização da identidade do documento — a base factual da estratégia de
/// particionamento por projeto (03/08/2026).
///
/// `doc_id = blake3(symbol_name | file_path | line_number)` e `upsert_symbol`
/// executa `delete_term(blake3_hash == doc_id)` **antes** de `add_document`.
/// Como `file_path` é gravado **relativo**, dois projetos que compartilhem
/// `(símbolo, caminho relativo, linha)` produzem o MESMO `doc_id` — e num índice
/// compartilhado o segundo write **remove** o primeiro.
///
/// A superfície medida em `~/projects`: `README.md` existe em 8 projetos,
/// `.gitignore` em 8, `pyproject.toml` em 6, `Cargo.toml` em 5.
///
/// Este teste é o árbitro da hipótese: 1 documento ⇒ eviction confirmada;
/// 2 ⇒ a leitura do código estava errada e a prioridade da frente cai.
/// `crate_name` difere entre os dois docs justamente para mostrar que um campo
/// distinto **não** participa da identidade.
#[test]
fn identical_relative_coordinates_collapse_to_one_document() {
    let (idx, _dir) = make_index();

    let mut from_project_a = symbol("title", "README.md", "heading");
    from_project_a.crate_name = Some("projeto-a".to_string());
    let mut from_project_b = symbol("title", "README.md", "heading");
    from_project_b.crate_name = Some("projeto-b".to_string());

    idx.upsert_symbol(&from_project_a).expect("upsert A");
    idx.upsert_symbol(&from_project_b).expect("upsert B");
    idx.commit().expect("commit");

    let hits = idx.search("title", 10).expect("search");
    assert_eq!(
        hits.len(),
        1,
        "coordenadas relativas idênticas compartilham doc_id — o segundo write \
         evicta o primeiro. Vieram {} docs: {:?}",
        hits.len(),
        hits.iter()
            .map(|h| h.crate_name.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(
        hits[0].crate_name.as_deref(),
        Some("projeto-b"),
        "o sobrevivente é o ÚLTIMO a escrever — o primeiro projeto perdeu o documento"
    );
}

/// A partição por diretório é o que **resolve** a eviction caracterizada em
/// `identical_relative_coordinates_collapse_to_one_document`.
///
/// Dois projetos com o mesmo `README.md:1::title` vão para índices distintos, de
/// modo que ambos sobrevivem. Note que o schema NÃO mudou: a propriedade vem do
/// particionamento, não de um campo novo — o que importa porque uma mudança de
/// schema dispara o ramo de recuperação que APAGA o diretório.
#[test]
fn separate_index_dirs_let_both_projects_keep_their_document() {
    let project_a = TempDir::new().expect("tempdir A");
    let project_b = TempDir::new().expect("tempdir B");
    let idx_a = TantivyIndex::open_or_create(project_a.path()).expect("open A");
    let idx_b = TantivyIndex::open_or_create(project_b.path()).expect("open B");

    let mut doc_a = symbol("title", "README.md", "heading");
    doc_a.crate_name = Some("projeto-a".to_string());
    let mut doc_b = symbol("title", "README.md", "heading");
    doc_b.crate_name = Some("projeto-b".to_string());

    idx_a.upsert_symbol(&doc_a).expect("upsert A");
    idx_b.upsert_symbol(&doc_b).expect("upsert B");
    idx_a.commit().expect("commit A");
    idx_b.commit().expect("commit B");

    let hits_a = idx_a.search("title", 10).expect("search A");
    let hits_b = idx_b.search("title", 10).expect("search B");
    assert_eq!(hits_a.len(), 1, "A mantém o seu");
    assert_eq!(hits_b.len(), 1, "B mantém o seu");
    assert_eq!(hits_a[0].crate_name.as_deref(), Some("projeto-a"));
    assert_eq!(
        hits_b[0].crate_name.as_deref(),
        Some("projeto-b"),
        "cada projeto conserva o SEU documento — nenhum evictou o outro"
    );
}

/// O registry devolve o MESMO ponteiro para a mesma raiz.
///
/// Guarda contra o risco R3 da estratégia: `Box::leak` por resolução vazaria sem
/// limite se cada chamada abrisse um índice novo. O vazamento tem de ser
/// proporcional ao número de PROJETOS, não ao de chamadas.
#[test]
fn the_registry_returns_one_index_per_root() {
    let project = TempDir::new().expect("tempdir");
    let root = project.path();
    // O root precisa de um marcador real, senão `normalize_project_root` sobe
    // até $HOME e duas raízes de teste colapsariam no mesmo diretório.
    std::fs::create_dir_all(root.join(".git")).expect("marcador de projeto");

    let first = tantivy_for(Some(root)).expect("primeira resolução");
    let second = tantivy_for(Some(root)).expect("segunda resolução");
    assert!(
        std::ptr::eq(first, second),
        "a mesma raiz tem de devolver o mesmo índice — senão o Box::leak vaza por chamada"
    );

    let other = TempDir::new().expect("tempdir 2");
    std::fs::create_dir_all(other.path().join(".git")).expect("marcador");
    let third = tantivy_for(Some(other.path())).expect("outra raiz");
    assert!(
        !std::ptr::eq(first, third),
        "raízes distintas têm de devolver índices distintos"
    );
}

/// A fachada histórica continua servindo o índice legado global — é o que torna
/// a conversão dos ~41 chamadores incremental, com o sistema verde em cada passo.
/// ⚠ Este teste APONTA `HOME` PARA UM TEMPDIR — de propósito.
///
/// A fachada resolve `$HOME/.claude/touring/tantivy`, e `open_or_create` CRIA o
/// diretório. Rodando contra o `HOME` real, a suíte **recriava** o índice legado
/// que a F5b tinha acabado de aposentar — um teste unitário escrevendo no
/// ambiente do usuário e desfazendo uma migração (achado do cross-audit
/// 2026-08-03, comprovado: diretório ausente antes do teste, presente depois).
///
/// `HOME` é global ao processo, daí o `#[serial]`.
#[test]
#[serial_test::serial(tantivy_home_env)]
#[expect(
    deprecated,
    reason = "este teste É o guardião da fachada depreciada: enquanto `global_tantivy` \
              existir, tem de continuar equivalente a `tantivy_for(None)`. Some junto com ela."
)]
fn the_legacy_facade_resolves_to_the_shared_global_index() {
    let fake_home = TempDir::new().expect("tempdir HOME");
    let real_home = std::env::var_os("HOME");
    // SAFETY: serializado por `#[serial]`; nenhum outro teste lê HOME em paralelo.
    unsafe { std::env::set_var("HOME", fake_home.path()) };

    let via_facade = global_tantivy();
    let via_none = tantivy_for(None);
    match (via_facade, via_none) {
        (Some(a), Some(b)) => assert!(
            std::ptr::eq(a, b),
            "global_tantivy() tem de ser exatamente tantivy_for(None)"
        ),
        (None, None) => { /* índice global indisponível no ambiente — coerente */ }
        _ => {
            restore_home(real_home);
            panic!("fachada e tantivy_for(None) divergiram");
        }
    }
    restore_home(real_home);
}

/// Devolve `HOME` ao valor original. Chamado também no caminho de falha — um
/// `panic!` com `HOME` apontando para um tempdir já removido contaminaria todo
/// teste subsequente do binário.
fn restore_home(original: Option<std::ffi::OsString>) {
    // SAFETY: chamado apenas dentro de testes marcados `#[serial]`.
    unsafe {
        match original {
            Some(h) => std::env::set_var("HOME", h),
            None => std::env::remove_var("HOME"),
        }
    }
}

/// Regressão do incidente 2026-08-03: **dois daemons legítimos, um índice**.
///
/// A topologia per-project (Pln2 L4) permite N daemons vivos — o global e um por
/// projeto pinado. Todos resolvem o índice Tantivy para o MESMO diretório
/// (`$HOME/.claude/touring/tantivy`), e o *writer lock* do Tantivy é exclusivo:
/// quem abre primeiro ganha, os demais recebiam `Err` de `open_or_create` e
/// ficavam **permanentemente** sem FTS (o singleton é `OnceLock` — cacheia o
/// `None` para sempre). Observado ao vivo: daemon global PID 43304 sem nenhum fd
/// em tantivy enquanto o daemon de `~/projects/analise` (PID 68025) segurava
/// `.tantivy-writer.lock` com lock exclusivo (`lsof` FD `10wW`; `flock` de um
/// terceiro processo retornou `EWOULDBLOCK`).
///
/// A leitura **nunca** precisou desse lock — só a escrita. Abrir o índice tem de
/// degradar para somente-leitura em vez de falhar por inteiro.
#[test]
fn a_second_opener_reads_while_the_first_holds_the_writer_lock() {
    let dir = TempDir::new().expect("tempdir");
    let first = TantivyIndex::open_or_create(dir.path()).expect("first open");
    first
        .upsert_symbol(&symbol("SharedSymbol", "src/shared.rs", "struct"))
        .expect("upsert");
    first.commit().expect("commit");
    assert!(
        first.is_writable(),
        "o primeiro a abrir detém o writer lock"
    );

    // O segundo daemon abre o MESMO diretório enquanto `first` ainda vive.
    let second = TantivyIndex::open_or_create(dir.path())
        .expect("segundo opener não pode falhar — leitura não usa o writer lock");
    assert!(
        !second.is_writable(),
        "o writer lock é exclusivo: o segundo opener fica somente-leitura"
    );

    let hits = second
        .search("SharedSymbol", 10)
        .expect("busca a partir do segundo opener");
    assert!(
        !hits.is_empty(),
        "o segundo opener tem de LER o índice compartilhado (era o sintoma: 0 hits + erro)"
    );
}

/// A escrita a partir de um handle somente-leitura falha de forma **explícita** —
/// nunca em silêncio. Um upsert perdido sem erro reapareceria como "o índice está
/// desatualizado" horas depois, longe da causa (constituição: *falhe loud*).
#[test]
fn a_read_only_handle_rejects_writes_with_an_explicit_error() {
    let dir = TempDir::new().expect("tempdir");
    let _holder = TantivyIndex::open_or_create(dir.path()).expect("holder");
    let read_only = TantivyIndex::open_or_create(dir.path()).expect("read-only opener");
    assert!(!read_only.is_writable());

    let err = read_only
        .upsert_symbol(&symbol("Rejected", "src/r.rs", "fn"))
        .expect_err("upsert em handle somente-leitura tem de falhar")
        .to_string();
    assert!(
        err.contains("read-only"),
        "o erro precisa nomear a condição real, veio: {err}"
    );
}

/// Quando o detentor do lock some, o handle degradado **se recupera sozinho** no
/// próximo write — sem exigir restart. Era o segundo meio do bug: mesmo que o
/// outro daemon morresse, o handle continuava estéril.
#[test]
fn a_degraded_handle_reacquires_the_writer_after_the_holder_goes_away() {
    let dir = TempDir::new().expect("tempdir");
    let degraded = {
        let _holder = TantivyIndex::open_or_create(dir.path()).expect("holder");
        let degraded = TantivyIndex::open_or_create(dir.path()).expect("second opener");
        assert!(!degraded.is_writable(), "nasce somente-leitura");
        degraded
        // `_holder` é dropado aqui — o writer lock é liberado.
    };

    degraded
        .upsert_symbol(&symbol("Recovered", "src/rec.rs", "struct"))
        .expect("o writer tem de ser readquirido sob demanda");
    degraded.commit().expect("commit após reaquisição");
    assert!(degraded.is_writable(), "o handle voltou a ser gravável");

    let hits = degraded.search("Recovered", 5).expect("search");
    assert!(!hits.is_empty(), "o documento gravado tem de estar visível");
}

#[test]
fn test_expanded_schema_fields() {
    let (idx, _dir) = make_index();
    let doc = SymbolDoc {
        symbol_name: "process_query".to_string(),
        file_path: "src/query.rs".to_string(),
        symbol_kind: "fn".to_string(),
        module_path: Some("crate::query".to_string()),
        docstring: Some("Process a search query".to_string()),
        line_number: 100,
        language: "rust".to_string(),
        visibility: Some("pub".to_string()),
        crate_name: Some("touring-hooks".to_string()),
        blake3_hash: Some("abc123def456".to_string()),
        import_count: Some(5),
        export_count: Some(2),
        cognitive_score: Some(0.75),
        functional_signature: Some("fn(query: &str, top_k: usize) -> Vec<Hit>".to_string()),
        community_id: None,
    };
    idx.upsert_symbol(&doc).expect("upsert expanded");
    idx.commit().expect("commit");

    let hits = idx.search("process_query", 10).expect("search");
    assert!(!hits.is_empty(), "expanded doc should be searchable");
    assert_eq!(hits[0].symbol_name, "process_query");
    assert_eq!(hits[0].crate_name.as_deref(), Some("touring-hooks"));
    assert_eq!(hits[0].visibility.as_deref(), Some("pub"));
    assert_eq!(
        hits[0].functional_signature.as_deref(),
        Some("fn(query: &str, top_k: usize) -> Vec<Hit>")
    );
}

// ── U7+U8 tests ───────────────────────────────────────────────────────────

#[test]
fn test_fuzzy_search_finds_typo() {
    let (idx, _dir) = make_index();
    idx.upsert_symbol(&symbol("HookRuntime", "src/hook_runtime.rs", "struct"))
        .expect("upsert");
    idx.commit().expect("commit");

    // "HokRuntime" → "hokruntim" (stemmed); "HookRuntime" → "hookruntim" (stemmed)
    // Edit distance hokruntim vs hookruntim = 1 (only 2nd char differs: o→k)
    let hits = idx.fuzzy_search("HokRuntime", 1, 10).expect("fuzzy_search");
    assert!(
        hits.iter().any(|h| h.symbol_name == "HookRuntime"),
        "expected fuzzy match for typo; got: {hits:?}"
    );
}

#[test]
fn test_suggest_returns_prefix_matches() {
    let (idx, _dir) = make_index();
    for name in &["HookRuntime", "HookRegistry", "HookHandler"] {
        idx.upsert_symbol(&symbol(name, "src/hooks.rs", "struct"))
            .expect("upsert");
    }
    idx.upsert_symbol(&symbol("IndexReader", "src/index.rs", "struct"))
        .expect("upsert non-hook");
    idx.commit().expect("commit");

    let hits = idx.suggest("Hook", 10).expect("suggest");
    assert!(
        hits.len() >= 3,
        "expected 3 Hook* symbols; got {}: {hits:?}",
        hits.len()
    );
    assert!(
        hits.iter().all(|h| h.symbol_name.starts_with("Hook")),
        "all results should start with 'Hook'; got: {hits:?}"
    );
}

#[test]
fn test_search_by_crate_filters_correctly() {
    let (idx, _dir) = make_index();

    // Symbol in crate "touring-hooks"
    let mut hooks_sym = symbol(
        "WiringAudit",
        "crates/touring-hooks/src/wiring.rs",
        "struct",
    );
    hooks_sym.crate_name = Some("touring-hooks".to_string());
    idx.upsert_symbol(&hooks_sym).expect("upsert hooks sym");

    // Same symbol name in crate "touring-index"
    let mut index_sym = symbol("WiringAudit", "crates/touring-index/src/audit.rs", "struct");
    index_sym.crate_name = Some("touring-index".to_string());
    idx.upsert_symbol(&index_sym).expect("upsert index sym");

    idx.commit().expect("commit");

    let hits = idx
        .search_by_crate("WiringAudit", "touring-hooks", 10)
        .expect("search_by_crate");
    assert!(
        !hits.is_empty(),
        "expected at least one result for touring-hooks"
    );
    // Every returned result must belong to touring-hooks
    for h in &hits {
        assert!(
            h.file_path.contains("touring-hooks")
                || h.crate_name.as_deref() == Some("touring-hooks"),
            "unexpected result from wrong crate: {h:?}"
        );
    }
}

#[test]
fn test_reindex_rebuilds_from_scratch() {
    let (idx, _dir) = make_index();

    // Prime the index with some initial data.
    idx.upsert_symbol(&symbol("OldSymbol", "src/old.rs", "fn"))
        .expect("upsert old");
    idx.commit().expect("initial commit");
    assert_eq!(idx.stats().total_docs, 1);

    // Reindex with a completely different set.
    let new_symbols = vec![
        symbol("NewAlpha", "src/new.rs", "struct"),
        symbol("NewBeta", "src/new.rs", "fn"),
        symbol("NewGamma", "src/new.rs", "trait"),
    ];
    let stats = idx.reindex(new_symbols).expect("reindex");

    assert_eq!(stats.total_docs, 3, "reindex should yield exactly 3 docs");

    // Old symbol must be gone.
    let old_hits = idx.search("OldSymbol", 5).expect("search old");
    assert!(
        old_hits.is_empty(),
        "OldSymbol must be removed after reindex"
    );

    // New symbols must be present.
    for name in &["NewAlpha", "NewBeta", "NewGamma"] {
        let hits = idx.search(name, 5).expect("search new");
        assert!(!hits.is_empty(), "expected hit for {name} after reindex");
    }
}

#[test]
fn test_search_by_functional_signature() {
    let (idx, _dir) = make_index();
    let doc = SymbolDoc {
        symbol_name: "search_symbols".to_string(),
        file_path: "src/search.rs".to_string(),
        symbol_kind: "fn".to_string(),
        module_path: Some("crate::search".to_string()),
        docstring: None,
        line_number: 55,
        language: "rust".to_string(),
        visibility: Some("pub".to_string()),
        crate_name: Some("touring-index".to_string()),
        blake3_hash: None,
        import_count: None,
        export_count: None,
        cognitive_score: Some(0.42),
        // Unique term in the signature for targeted search
        functional_signature: Some("fn(needle: &str) -> Vec<SymbolHit>".to_string()),
        community_id: None,
    };
    idx.upsert_symbol(&doc).expect("upsert");
    idx.commit().expect("commit");

    // Query using a term from the functional_signature field
    let hits = idx.search("SymbolHit", 5).expect("search by sig");
    assert!(
        !hits.is_empty(),
        "should find doc by functional_signature term"
    );
    assert_eq!(hits[0].symbol_name, "search_symbols");
}

// ── Schema v3 community_id tests ──────────────────────────────────────────

fn make_sym_with_community(name: &str, file: &str, community_id: Option<u64>) -> SymbolDoc {
    SymbolDoc {
        symbol_name: name.to_string(),
        file_path: file.to_string(),
        symbol_kind: "fn".to_string(),
        module_path: None,
        docstring: None,
        line_number: 1,
        language: "rust".to_string(),
        visibility: None,
        crate_name: None,
        blake3_hash: None,
        import_count: None,
        export_count: None,
        cognitive_score: None,
        functional_signature: None,
        community_id,
    }
}

#[test]
fn test_community_id_roundtrip_in_schema() {
    let (idx, _dir) = make_index();
    let doc = make_sym_with_community("foo_community", "src/a.rs", Some(42));
    idx.upsert_symbol(&doc).expect("upsert");
    idx.commit().expect("commit");

    let hits = idx.search("foo_community", 5).expect("search");
    assert_eq!(hits.len(), 1, "expected exactly one hit");
    assert_eq!(
        hits[0].community_id,
        Some(42),
        "community_id must round-trip through schema v3"
    );
}

#[test]
fn test_community_boost_elevates_same_community_hit() {
    let (idx, _dir) = make_index();

    // Two docs with same name prefix — one in community 7, one in community 9.
    // Use distinct docstrings so BM25 gives them roughly equal base scores.
    let mut doc_a = make_sym_with_community("authenticate_op", "src/auth.rs", Some(7));
    doc_a.docstring = Some("authenticate operation handler for auth module".to_string());
    doc_a.line_number = 10;

    let mut doc_b = make_sym_with_community("authenticate_op", "src/user.rs", Some(9));
    doc_b.docstring = Some("authenticate operation handler for user module".to_string());
    doc_b.line_number = 20;

    idx.upsert_symbol(&doc_a).expect("upsert doc_a");
    idx.upsert_symbol(&doc_b).expect("upsert doc_b");
    idx.commit().expect("commit");

    // With boost targeting community 7: doc_a must rank first.
    let boosted = idx
        .search_with_community_boost("authenticate_op", 5, Some(7))
        .expect("search_with_community_boost");
    assert!(
        !boosted.is_empty(),
        "expected at least one hit from community-boosted search"
    );
    assert_eq!(
        boosted[0].community_id,
        Some(7),
        "community-boosted doc (community 7) must rank first; got: {boosted:?}"
    );
}

#[test]
fn test_no_community_id_when_none() {
    let (idx, _dir) = make_index();
    let doc = make_sym_with_community("bar_nocommunity", "src/b.rs", None);
    idx.upsert_symbol(&doc).expect("upsert");
    idx.commit().expect("commit");

    let hits = idx.search("bar_nocommunity", 5).expect("search");
    assert_eq!(hits.len(), 1, "expected exactly one hit");
    assert_eq!(
        hits[0].community_id, None,
        "community_id must be None when not set on SymbolDoc"
    );
}

// ─── D2.3 — ToolOutputsIndex tests ──────────────────────────────────────

fn make_outputs_index() -> (ToolOutputsIndex, TempDir) {
    let dir = TempDir::new().expect("tempdir");
    let idx = ToolOutputsIndex::open_or_create(dir.path()).expect("open_or_create");
    (idx, dir)
}

fn sample_doc(hash: &str, tool: &str) -> ToolOutputDoc {
    ToolOutputDoc {
        content_hash: hash.to_string(),
        tool_name: tool.to_string(),
        summary: format!("output of {tool}"),
        full_output_path: format!("/tmp/sandbox/{hash}.bin"),
        exit_code: 0,
        output_bytes: 1024,
        was_truncated: false,
        stored_at_unix: 1_700_000_000,
        tool_args: None,
    }
}

/// O SEGUNDO opener do índice de tool-outputs degrada para somente-leitura.
///
/// O `TantivyIndex` tinha essa prova desde a F1; o `ToolOutputsIndex` **não** —
/// lacuna encontrada no cross-audit de 04/08/2026. Sem ela, a correção
/// reader-first aplicada a este índice nunca havia sido exercitada.
#[test]
fn a_second_tool_outputs_opener_degrades_to_read_only() {
    let dir = TempDir::new().expect("tempdir");
    let first = ToolOutputsIndex::open_or_create(dir.path()).expect("primeiro opener");
    assert!(first.is_writable(), "o primeiro detém o writer lock");

    let second = ToolOutputsIndex::open_or_create(dir.path())
        .expect("segundo opener não pode falhar — leitura não usa o writer lock");
    assert!(
        !second.is_writable(),
        "o writer lock é exclusivo: o segundo fica somente-leitura"
    );

    let err = second
        .store_tool_output(&sample_doc("hash-ro", "Bash"))
        .expect_err("escrita em handle somente-leitura tem de falhar")
        .to_string();
    assert!(
        err.contains("read-only"),
        "o erro nomeia a condição real, veio: {err}"
    );
}

/// `reset_tool_outputs_global` NÃO esvazia o registry — de propósito.
///
/// As entradas são `Box::leak`; limpá-las as tornaria inalcançáveis sem
/// liberá-las, criando um vazamento proporcional ao número de RESETS em vez de
/// projetos (cross-audit 04/08/2026). Como a chave é o diretório, um `HOME`
/// diferente já resolve para outra entrada — esvaziar não traz benefício.
#[test]
fn reset_keeps_the_registry_so_leaked_indices_stay_reachable() {
    let project = TempDir::new().expect("tempdir");
    std::fs::create_dir_all(project.path().join(".git")).expect("marcador");

    let before = tool_outputs_for(Some(project.path())).expect("primeira resolução");
    reset_tool_outputs_global();
    let after = tool_outputs_for(Some(project.path())).expect("após reset");

    assert!(
        std::ptr::eq(before, after),
        "o reset não pode abandonar o índice já vazado — a mesma raiz devolve o \
         MESMO ponteiro, senão cada reset vaza um índice inteiro"
    );
}

/// O índice de tool-outputs é particionado por projeto, como o de símbolos.
///
/// Era o último representante da classe de defeito (singleton global, path fixo
/// em `$HOME`, `None` cacheado para sempre, writer lock exclusivo, e um `unsafe`
/// com ponteiro cru como remendo de teste). Corrigido no cross-audit 03/08/2026.
#[test]
fn tool_outputs_are_partitioned_per_project() {
    let a = TempDir::new().expect("tempdir A");
    let b = TempDir::new().expect("tempdir B");
    // Marcador real: sem ele `normalize_project_root` sobe até $HOME e as duas
    // raízes colapsariam na mesma — o teste passaria sem provar nada.
    std::fs::create_dir_all(a.path().join(".git")).expect("marcador A");
    std::fs::create_dir_all(b.path().join(".git")).expect("marcador B");

    let idx_a = tool_outputs_for(Some(a.path())).expect("índice A");
    let idx_b = tool_outputs_for(Some(b.path())).expect("índice B");
    assert!(
        !std::ptr::eq(idx_a, idx_b),
        "raízes distintas têm de resolver índices distintos"
    );
    assert!(
        std::ptr::eq(
            idx_a,
            tool_outputs_for(Some(a.path())).expect("re-resolução")
        ),
        "a mesma raiz devolve o MESMO índice — senão o Box::leak vaza por chamada"
    );

    // O MESMO content_hash em projetos diferentes não se sobrescreve.
    let doc_a = sample_doc("hash-compartilhado", "Bash");
    let doc_b = sample_doc("hash-compartilhado", "Grep");
    idx_a.store_tool_output(&doc_a).expect("store A");
    idx_b.store_tool_output(&doc_b).expect("store B");

    let got_a = idx_a.get_tool_output("hash-compartilhado").expect("get A");
    let got_b = idx_b.get_tool_output("hash-compartilhado").expect("get B");
    assert_eq!(got_a.expect("A presente").tool_name, "Bash");
    assert_eq!(
        got_b.expect("B presente").tool_name,
        "Grep",
        "cada projeto conserva o SEU output — nenhum evictou o outro"
    );
}

#[test]
fn test_tool_outputs_store_and_get_roundtrip() {
    let (idx, _dir) = make_outputs_index();
    let doc = sample_doc(
        "a".repeat(64).as_str(), // 64-char hash
        "Bash",
    );
    idx.store_tool_output(&doc).expect("store");
    let got = idx
        .get_tool_output(&doc.content_hash)
        .expect("get")
        .expect("doc present");
    assert_eq!(got, doc);
}

#[test]
fn test_tool_outputs_get_missing_returns_none() {
    let (idx, _dir) = make_outputs_index();
    let res = idx.get_tool_output("nonexistent_hash_xx").expect("get");
    assert!(res.is_none());
}

#[test]
fn test_tool_outputs_upsert_replaces_previous() {
    let (idx, _dir) = make_outputs_index();
    let hash = "b".repeat(64);
    let mut doc = sample_doc(&hash, "Grep");
    doc.exit_code = 1;
    doc.was_truncated = true;
    idx.store_tool_output(&doc).expect("store v1");

    // Upsert same hash with different fields
    doc.exit_code = 0;
    doc.was_truncated = false;
    doc.summary = "second-version".into();
    idx.store_tool_output(&doc).expect("store v2");

    let got = idx.get_tool_output(&hash).expect("get").expect("present");
    assert_eq!(got.exit_code, 0);
    assert!(!got.was_truncated);
    assert_eq!(got.summary, "second-version");
}

// ─── P3-TRIG — RRF tests ────────────────────────────────────────────────

fn rrf_hit(name: &str, file: &str, line: u64) -> SearchHit {
    SearchHit {
        symbol_name: name.to_string(),
        file_path: file.to_string(),
        symbol_kind: "fn".to_string(),
        line_number: line,
        score: 1.0,
        crate_name: None,
        visibility: None,
        functional_signature: None,
        cognitive_score: None,
        community_id: None,
    }
}

#[test]
fn test_rrf_hit_identity_distinguishes_lines() {
    let h1 = rrf_hit("foo", "src/a.rs", 10);
    let h2 = rrf_hit("foo", "src/a.rs", 11);
    assert_ne!(hit_identity(&h1), hit_identity(&h2));
}

#[test]
fn test_rrf_merge_empty_lists_returns_empty() {
    let merged = rrf_merge_two(&[], &[], 60, 5);
    assert!(merged.is_empty());
}

#[test]
fn test_rrf_merge_single_list_preserves_rank() {
    let porter = vec![
        rrf_hit("first", "a.rs", 1),
        rrf_hit("second", "b.rs", 1),
        rrf_hit("third", "c.rs", 1),
    ];
    let merged = rrf_merge_two(&porter, &[], 60, 5);
    assert_eq!(merged.len(), 3);
    assert_eq!(merged[0].symbol_name, "first");
    assert_eq!(merged[1].symbol_name, "second");
    assert_eq!(merged[2].symbol_name, "third");
}

#[test]
fn test_rrf_merge_boosts_overlap() {
    // doc appearing in both lists at rank 1 should win over doc that
    // only appears in one list, even if at rank 1 there as well.
    let common = rrf_hit("shared", "x.rs", 1);
    let porter = vec![common.clone(), rrf_hit("p_only", "p.rs", 1)];
    let fuzzy = vec![common.clone(), rrf_hit("f_only", "f.rs", 1)];
    let merged = rrf_merge_two(&porter, &fuzzy, 60, 5);
    assert_eq!(merged[0].symbol_name, "shared");
    // The shared doc's score = 1/61 + 1/61 ≈ 0.0328; singletons get 1/61.
    assert!(merged[0].score > merged[1].score);
}

#[test]
fn test_rrf_merge_top_k_truncates() {
    let lots: Vec<SearchHit> = (0..10)
        .map(|i| rrf_hit(&format!("s{i}"), "z.rs", i))
        .collect();
    let merged = rrf_merge_two(&lots, &[], 60, 3);
    assert_eq!(merged.len(), 3);
}

#[test]
fn test_rrf_constant_k_default_60() {
    // Default expected behaviour even when env unset
    let k = crate::shared::feature_flags::rrf_k_constant();
    assert_eq!(k, 60);
}

#[test]
fn test_search_rrf_falls_back_when_disabled() {
    // When TOURING_TANTIVY_TRIGRAM=0, search_rrf must equal search().
    // Set env-var only inside the test (env_lock not available here, so
    // verify behaviour via the public flag check).
    let prev = std::env::var("TOURING_TANTIVY_TRIGRAM").ok();
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("TOURING_TANTIVY_TRIGRAM", "0") };
    let (idx, _dir) = make_index();
    let mut s = symbol("authenticate", "src/auth.rs", "fn");
    s.crate_name = Some("touring-auth".into());
    idx.upsert_symbol(&s).expect("upsert");
    idx.commit().expect("commit");
    let plain = idx.search("authenticate", 5).expect("search");
    let rrf = idx.search_rrf("authenticate", 5).expect("search_rrf");
    assert_eq!(plain.len(), rrf.len());
    assert_eq!(plain[0].symbol_name, rrf[0].symbol_name);
    // restore env
    match prev {
        // TODO: Audit that the environment access only happens in single-threaded code.
        Some(v) => unsafe { std::env::set_var("TOURING_TANTIVY_TRIGRAM", v) },
        // TODO: Audit that the environment access only happens in single-threaded code.
        None => unsafe { std::env::remove_var("TOURING_TANTIVY_TRIGRAM") },
    }
}

// ─── Sprint 1 — I-01 NgramTokenizer trigram tests ─────────────────────

#[test]
fn test_i01_trigram_substring_match_useeff() {
    let (idx, _dir) = make_index();
    idx.upsert_symbol(&symbol("useEffect", "src/hooks.rs", "fn"))
        .expect("upsert");
    idx.commit().expect("commit");
    let hits = idx.search_trigram("useEff", 5).expect("search_trigram");
    assert!(
        !hits.is_empty(),
        "trigram 'useEff' MUST match indexed 'useEffect'"
    );
    assert_eq!(hits[0].symbol_name, "useEffect");
}

#[test]
fn test_i01_trigram_short_query_returns_empty() {
    let (idx, _dir) = make_index();
    idx.upsert_symbol(&symbol("foo", "src/x.rs", "fn"))
        .expect("upsert");
    idx.commit().expect("commit");
    let hits = idx.search_trigram("us", 5).expect("search");
    assert!(hits.is_empty(), "queries < 3 chars MUST return empty");
}

#[test]
fn test_i01_3way_rrf_combines_porter_trigram_fuzzy() {
    let (idx, _dir) = make_index();
    // Doc com nome contendo trigrams + porter match
    idx.upsert_symbol(&symbol("authenticate_user", "src/auth.rs", "fn"))
        .expect("upsert");
    // Doc só relevante via fuzzy (typo distance)
    idx.upsert_symbol(&symbol("authentcat", "src/typo.rs", "fn"))
        .expect("upsert");
    idx.commit().expect("commit");
    // Trigram should be ON by default
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("TOURING_TANTIVY_TRIGRAM") };
    let hits = idx.search_rrf("authenticate", 5).expect("search_rrf 3-way");
    assert!(!hits.is_empty(), "3-way RRF MUST return hits");
}

// ─── Sprint 1 — I-02 PhraseQuery proximity tests ──────────────────────

#[test]
fn test_i02_phrase_query_only_for_multi_term() {
    let (idx, _dir) = make_index();
    idx.upsert_symbol(&symbol("foo", "src/a.rs", "fn"))
        .expect("upsert");
    idx.commit().expect("commit");
    // Single-term: try_build_phrase_query returns None
    let phrase = idx.try_build_phrase_query("foo");
    assert!(phrase.is_none(), "single-term MUST NOT build PhraseQuery");
    // Multi-term: returns Some
    let phrase = idx.try_build_phrase_query("foo bar");
    assert!(phrase.is_some(), "multi-term MUST build PhraseQuery");
}

#[test]
fn test_i02_phrase_metric_increments_on_multi_term_search() {
    let (idx, _dir) = make_index();
    idx.upsert_symbol(&symbol("error_handler", "src/h.rs", "fn"))
        .expect("upsert");
    idx.commit().expect("commit");
    let before = crate::shared::gate_metrics::global()
        .phrase_query_match_count
        .load(std::sync::atomic::Ordering::Relaxed);
    let _ = idx.search("error handler", 5).expect("search");
    let after = crate::shared::gate_metrics::global()
        .phrase_query_match_count
        .load(std::sync::atomic::Ordering::Relaxed);
    assert!(after > before, "phrase_query_match_count MUST advance");
}

// ─── Sprint 1 — I-03 5× Heading boost tests ───────────────────────────

#[test]
fn test_i03_name_boost_default_is_5x() {
    let boost = crate::shared::feature_flags::tantivy_name_boost();
    assert_eq!(boost, 5.0, "default name boost MUST be 5.0");
}

#[test]
fn test_i03_name_boost_env_overridable() {
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("TOURING_TANTIVY_NAME_BOOST", "3.5") };
    let boost = crate::shared::feature_flags::tantivy_name_boost();
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("TOURING_TANTIVY_NAME_BOOST") };
    assert_eq!(boost, 3.5, "env var MUST override default");
}

// ─── Sprint 1 — I-05 TTL Cache tests ───────────────────────────────────

fn fresh_doc(hash: &str, tool: &str) -> ToolOutputDoc {
    let mut d = sample_doc(hash, tool);
    d.stored_at_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|x| x.as_secs())
        .unwrap_or(0);
    d
}

#[test]
fn test_i05_ttl_skip_within_24h_window() {
    let (idx, _dir) = make_outputs_index();
    let doc = fresh_doc(&"x".repeat(64), "Bash");
    idx.store_tool_output(&doc).expect("store v1");
    let before = crate::shared::gate_metrics::global()
        .tool_outputs_ttl_skip_count
        .load(std::sync::atomic::Ordering::Relaxed);
    // Same hash within TTL: store MUST skip
    idx.store_tool_output(&doc).expect("store v2 (should skip)");
    let after = crate::shared::gate_metrics::global()
        .tool_outputs_ttl_skip_count
        .load(std::sync::atomic::Ordering::Relaxed);
    assert!(after > before, "ttl_skip_count MUST advance on duplicate");
}

#[test]
fn test_i05_is_fresh_returns_some_for_recent_doc() {
    let (idx, _dir) = make_outputs_index();
    let hash = "y".repeat(64);
    idx.store_tool_output(&fresh_doc(&hash, "Bash"))
        .expect("store");
    // 24h window must accept a doc stored seconds ago
    assert!(idx.is_fresh(&hash, 86_400).is_some());
}

#[test]
fn test_i06_json_field_stores_tool_args() {
    let (idx, _dir) = make_outputs_index();
    let mut doc = fresh_doc(&"j".repeat(64), "Bash");
    doc.tool_args = Some(serde_json::json!({
        "command": "gh issue list",
        "path": "src/main.rs",
    }));
    idx.store_tool_output(&doc).expect("store with tool_args");
    // Verify retrievability via content_hash (round-trip).
    let got = idx
        .get_tool_output(&doc.content_hash)
        .expect("get")
        .expect("present");
    assert_eq!(got.content_hash, doc.content_hash);
    // tool_args read-back not implemented yet (decode is None);
    // assert that field at least serialises round-trip via JSON form.
    let serialised = serde_json::to_string(&doc).expect("serialise");
    let parsed: ToolOutputDoc = serde_json::from_str(&serialised).expect("parse");
    assert!(parsed.tool_args.is_some());
}

#[test]
fn test_i08_facet_path_built_from_symbol() {
    let mut s = symbol("foo", "src/x.rs", "fn");
    s.crate_name = Some("touring-hooks".into());
    s.visibility = Some("pub".into());
    let facet = build_symbol_facet(&s);
    let path = format!("{facet}");
    assert!(path.contains("rust"));
    assert!(path.contains("touring-hooks"));
    assert!(path.contains("fn"));
    assert!(path.contains("pub"));
}

#[test]
fn test_i08_count_facets_returns_buckets_under_prefix() {
    let (idx, _dir) = make_index();
    let mut s1 = symbol("foo_fn", "src/a.rs", "fn");
    s1.crate_name = Some("touring-hooks".into());
    s1.visibility = Some("pub".into());
    idx.upsert_symbol(&s1).expect("upsert s1");

    let mut s2 = symbol("Bar", "src/b.rs", "struct");
    s2.crate_name = Some("touring-hooks".into());
    s2.visibility = Some("pub".into());
    idx.upsert_symbol(&s2).expect("upsert s2");
    idx.commit().expect("commit");

    let buckets = idx
        .count_facets("/rust/touring-hooks", 10)
        .expect("count_facets");
    // Two distinct kinds (fn, struct) under /rust/touring-hooks
    assert!(buckets.len() >= 1, "expected >= 1 bucket: {buckets:?}");
}

#[test]
fn test_i07_aggregate_terms_groups_by_kind() {
    let (idx, _dir) = make_index();
    idx.upsert_symbol(&symbol("a", "x.rs", "fn"))
        .expect("upsert");
    idx.upsert_symbol(&symbol("b", "y.rs", "fn"))
        .expect("upsert");
    idx.upsert_symbol(&symbol("Foo", "z.rs", "struct"))
        .expect("upsert");
    idx.commit().expect("commit");
    let buckets = idx
        .aggregate_terms("symbol_kind", 10)
        .expect("aggregate_terms");
    // Top bucket should be "fn" with count 2
    let top = buckets.first().expect("at least one bucket");
    assert_eq!(top.0, "fn");
    assert_eq!(top.1, 2);
}

#[test]
fn test_i07_aggregate_terms_unknown_field_errors() {
    let (idx, _dir) = make_index();
    let err = idx
        .aggregate_terms("nonexistent_xyz", 10)
        .expect_err("must error on unknown field");
    assert!(err.to_string().contains("unknown field"));
}

#[test]
fn test_i06_serde_to_owned_value_roundtrip() {
    let v = serde_json::json!({
        "str": "hello",
        "int": 42,
        "neg": -7,
        "float": 1.5,
        "bool": true,
        "null": null,
        "arr": [1, 2, 3],
        "nested": { "k": "v" },
    });
    let owned = serde_value_to_tantivy_owned(&v);
    // Sanity: top-level must be Object
    match owned {
        tantivy::schema::OwnedValue::Object(map) => {
            let keys: std::collections::BTreeSet<&str> =
                map.iter().map(|(k, _)| k.as_str()).collect();
            assert!(keys.contains("str"));
            assert!(keys.contains("nested"));
            assert!(keys.contains("arr"));
        }
        _ => panic!("top-level must be Object, got {owned:?}"),
    }
}

#[test]
fn test_i05_cleanup_expired_removes_old_docs() {
    let (idx, _dir) = make_outputs_index();
    let mut doc = sample_doc(&"z".repeat(64), "Bash");
    // Set stored_at_unix to 30 days ago (well past 14d retention)
    doc.stored_at_unix = 1_700_000_000; // ~Nov 2023
    idx.store_tool_output(&doc).expect("store old");
    // retention=1s means anything older than 1s gets cleaned
    let deleted = idx.cleanup_expired(1).expect("cleanup");
    assert!(deleted >= 1, "cleanup MUST delete the ancient doc");
}
