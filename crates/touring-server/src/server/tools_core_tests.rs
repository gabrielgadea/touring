use super::*;

#[test]
fn default_entity_db_path_honors_identity_dir_env() {
    let _env = crate::cli::ENV_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let tmp = std::env::temp_dir().join("touring-test-identity");
    // AUDITED (2026-08-12): env mutation serialized (ENV_LOCK/#[serial] guard at fn/mod) — edition-2024 unsafe.
    unsafe { std::env::set_var("TOURING_IDENTITY_DIR", &tmp) };
    let path = default_entity_db_path().expect("path should resolve");
    assert!(path.starts_with(&tmp));
    assert_eq!(
        path.file_name().and_then(|f| f.to_str()),
        Some("registry.db")
    );
    // AUDITED (2026-08-12): env mutation serialized (ENV_LOCK/#[serial] guard at fn/mod) — edition-2024 unsafe.
    unsafe { std::env::remove_var("TOURING_IDENTITY_DIR") };
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn default_entity_db_path_falls_back_to_data_dir() {
    let _env = crate::cli::ENV_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    // AUDITED (2026-08-12): env mutation serialized (ENV_LOCK/#[serial] guard at fn/mod) — edition-2024 unsafe.
    unsafe { std::env::remove_var("TOURING_IDENTITY_DIR") };
    let tmp = std::env::temp_dir().join("touring-test-data");
    // AUDITED (2026-08-12): env mutation serialized (ENV_LOCK/#[serial] guard at fn/mod) — edition-2024 unsafe.
    unsafe { std::env::set_var("TOURING_DATA_DIR", &tmp) };
    let path = default_entity_db_path().expect("path should resolve");
    assert!(path.starts_with(&tmp));
    // AUDITED (2026-08-12): env mutation serialized (ENV_LOCK/#[serial] guard at fn/mod) — edition-2024 unsafe.
    unsafe { std::env::remove_var("TOURING_DATA_DIR") };
    let _ = std::fs::remove_dir_all(&tmp);
}
