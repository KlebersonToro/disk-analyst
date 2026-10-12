// A T020 só entrega o parse dos argumentos; quem o consome é o `main` da T022.
#[allow(
    dead_code,
    reason = "consumido pelo main e pelo relatório de texto nas T021/T022"
)]
mod args;

fn main() {}
