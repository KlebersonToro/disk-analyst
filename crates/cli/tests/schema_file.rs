use std::fs;
use std::path::PathBuf;

fn repo_file(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}

#[test]
fn published_schema_is_identical_to_the_contract() {
    let contract = "specs/001-directory-scan-cli/contracts/scan-report-v1.schema.json";
    let published = "docs/schemas/scan-report-v1.schema.json";

    let contract_bytes = fs::read(repo_file(contract))
        .unwrap_or_else(|e| panic!("não foi possível ler {contract}: {e}"));
    let published_bytes = fs::read(repo_file(published))
        .unwrap_or_else(|e| panic!("não foi possível ler {published}: {e}"));

    assert!(
        contract_bytes == published_bytes,
        "{published} diverge de {contract}: atualize {published} com `cp {contract} {published}`"
    );
}
