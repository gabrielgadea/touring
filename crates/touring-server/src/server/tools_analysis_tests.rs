
use super::*;

#[test]
fn current_uid_is_stable() {
    // uid never changes mid-process; calling twice must return the same value.
    assert_eq!(current_uid(), current_uid());
}

#[test]
fn daemon_socket_path_format() {
    let path = daemon_socket_path();
    let s = path.to_string_lossy();
    assert!(s.starts_with("/tmp/touring-daemon-"));
    assert!(s.ends_with(".sock"));
}
