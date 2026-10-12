mod check;

const USAGE: &str = "Uso: cargo xtask <comando>

Comandos:
  check    roda o portão de qualidade (fmt, clippy e testes nos dois alvos)";

fn main() {
    let code = match std::env::args().nth(1).as_deref() {
        Some("check") => check::run_steps(&check::steps(), &mut check::run_process),
        _ => {
            eprintln!("{USAGE}");
            2
        }
    };
    std::process::exit(code);
}
