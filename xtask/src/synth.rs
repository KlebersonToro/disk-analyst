//! Subcomando `synth`: gera uma árvore sintética a partir de uma descrição JSON. Só embrulha a
//! biblioteca `disk-analyst-synth`; nenhuma lógica de geração fica aqui.

use std::path::Path;

use disk_analyst_synth::{generate, load_spec};

/// Forma de uso deste subcomando, sem o prefixo `cargo xtask`.
pub const USAGE: &str = "synth <descrição.json> <destino>";

/// Executa o subcomando com os argumentos depois de `synth`. Devolve o código de saída.
pub fn run(args: &[String]) -> i32 {
    let [spec_path, dest] = args else {
        eprintln!("Uso: cargo xtask {USAGE}");
        return 2;
    };
    let spec = match load_spec(Path::new(spec_path)) {
        Ok(spec) => spec,
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    match generate(&spec, Path::new(dest)) {
        Ok(expected) => {
            println!(
                "árvore gerada em {}: {} arquivos, {} pastas (incluindo a raiz), {} bytes lógicos, {} bytes alocados",
                Path::new(dest).join("tree").display(),
                expected.files,
                expected.folders,
                expected.logical_bytes,
                expected.allocated_bytes,
            );
            0
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}
