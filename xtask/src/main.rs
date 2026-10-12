mod check;
mod synth;

const USAGE: &str = "Uso: cargo xtask <comando>

Comandos:
  check                              roda o portão de qualidade (fmt, clippy e testes nos dois alvos)
  synth <descrição.json> <destino>   gera uma árvore sintética e o expected.json";

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let code = match args.get(1).map(String::as_str) {
        Some("check") => check::run_steps(&check::steps(), &mut check::run_process),
        Some("synth") => synth::run(&args[2..]),
        _ => {
            eprintln!("{USAGE}");
            2
        }
    };
    std::process::exit(code);
}
