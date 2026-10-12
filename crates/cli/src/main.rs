//! `disk-analyst`: a CLI (T022). Parse dos argumentos, varredura e relatório de texto.
//!
//! Contrato (contracts/cli.md): stdout recebe SÓ o relatório (ou a ajuda e a versão); tudo o mais
//! (avisos, erros) vai para stderr, em português. Códigos de saída: 0 sucesso, 2 erro de uso
//! (opção inválida, caminho inexistente ou raiz ilegível, `--engine mft`), 1 erro de E/S ao
//! escrever a saída. O relatório NÃO usa `print!`/`println!`, que dão pânico com o pipe fechado:
//! a escrita passa por um `BufWriter` e o erro vira código 1.
//!
//! Com `--json`, stdout recebe SÓ o documento do esquema v1 (T025/T026) e o aviso de `--progress` não
//! é emitido (o contrato diz que ele é ignorado nesse modo); o aviso de `--stats` continua, porque
//! o `--stats` vai para stderr e é ortogonal. Erro de varredura com `--json`: código 2, mensagem em
//! stderr e stdout VAZIO (nada de JSON parcial).
//!
//! Ainda sem `--progress` (US5), `--stats` (T039) e Ctrl+C (T037).

mod args;
mod text;

use std::io::{self, BufWriter, Write};
use std::process::ExitCode;

use disk_analyst_core::control::{CancelToken, ScanCounters};
use disk_analyst_core::json::{ToolInfo, write_json};
use disk_analyst_core::report::build_report;
use disk_analyst_core::scan::{ScanOptions, scan};

use args::{Command, Engine, HELP, ScanArgs};

const EXIT_IO_ERROR: u8 = 1;
const EXIT_USAGE: u8 = 2;

fn main() -> ExitCode {
    match args::parse(std::env::args_os().skip(1)) {
        Ok(Command::Help) => write_stdout(|out| out.write_all(HELP.as_bytes())),
        Ok(Command::Version) => {
            write_stdout(|out| writeln!(out, "disk-analyst {}", env!("CARGO_PKG_VERSION")))
        }
        Ok(Command::Scan(args)) => run_scan(&args),
        Err(error) => usage_error(&error.to_string()),
    }
}

/// Uma linha em stderr. Ignora falha de escrita: `eprintln!` daria pânico com o stderr fechado.
fn stderr_line(message: &str) {
    let _ = writeln!(io::stderr(), "{message}");
}

/// Erro de uso: mensagem, dica de `--help` e código 2.
fn usage_error(message: &str) -> ExitCode {
    stderr_line(&format!("erro: {message}"));
    stderr_line("use --help para ver o uso");
    ExitCode::from(EXIT_USAGE)
}

/// Escreve em stdout por um `BufWriter` com lock e faz o flush. Falha de escrita (por exemplo,
/// pipe fechado) vira código 1 e um aviso em stderr, sem pânico.
fn write_stdout(
    write: impl FnOnce(&mut BufWriter<io::StdoutLock<'static>>) -> io::Result<()>,
) -> ExitCode {
    let mut out = BufWriter::new(io::stdout().lock());
    match write(&mut out).and_then(|()| out.flush()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            stderr_line(&format!("erro ao escrever a saída: {error}"));
            ExitCode::from(EXIT_IO_ERROR)
        }
    }
}

fn run_scan(args: &ScanArgs) -> ExitCode {
    // `--progress` é ignorado com `--json` (contrato), então nem o aviso sai: stderr fica limpo.
    if args.progress && !args.json {
        stderr_line("aviso: --progress ainda não disponível; ignorado");
    }
    if args.stats {
        stderr_line("aviso: --stats ainda não disponível; ignorado");
    }
    match args.engine {
        // Os dois usam a varredura de diretórios; `mft` nem chega aqui (erro de uso no parse).
        Engine::Auto | Engine::Dir => {}
    }

    let tree = match scan(
        &args.path,
        &ScanOptions::default(),
        &CancelToken::new(),
        &ScanCounters::default(),
    ) {
        Ok(tree) => tree,
        Err(error) => {
            stderr_line(&format!("erro: {error}"));
            return ExitCode::from(EXIT_USAGE);
        }
    };

    let report = build_report(&tree, args.top.get());
    if args.json {
        let tool = ToolInfo {
            name: "disk-analyst",
            version: env!("CARGO_PKG_VERSION"),
        };
        write_stdout(|out| write_json(out, &report, &tree, &tool))
    } else {
        write_stdout(|out| text::write_text(out, &report, &tree))
    }
}
