use super::*;

#[test]
fn default_entity_db_path_honors_identity_dir_env() {
    let tmp = std::env::temp_dir().join("touring-test-identity");
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("TOURING_IDENTITY_DIR", &tmp) };
    let path = default_entity_db_path().expect("path should resolve");
    assert!(path.starts_with(&tmp));
    assert_eq!(
        path.file_name().and_then(|f| f.to_str()),
        Some("registry.db")
    );
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("TOURING_IDENTITY_DIR") };
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn default_entity_db_path_falls_back_to_data_dir() {
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("TOURING_IDENTITY_DIR") };
    let tmp = std::env::temp_dir().join("touring-test-data");
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("TOURING_DATA_DIR", &tmp) };
    let path = default_entity_db_path().expect("path should resolve");
    assert!(path.starts_with(&tmp));
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var("TOURING_DATA_DIR") };
    let _ = std::fs::remove_dir_all(&tmp);
}
