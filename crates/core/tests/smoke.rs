#[test]
fn version_matches_package_version() {
    assert_eq!(disk_analyst_core::version(), env!("CARGO_PKG_VERSION"));
}
