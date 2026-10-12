//! Portão único de qualidade (`cargo xtask check`): fmt, clippy e testes nos dois alvos.

use std::process::Command;

const WINDOWS_TARGET: &str = "x86_64-pc-windows-gnu";

/// Código de saída usado quando o processo termina sem código (morto por sinal).
const EXIT_NO_CODE: i32 = 1;

/// Um passo do portão: programa e argumentos.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub program: String,
    pub args: Vec<String>,
}

impl Step {
    pub fn new(program: &str, args: &[&str]) -> Self {
        Self {
            program: program.to_owned(),
            args: args.iter().map(|a| (*a).to_owned()).collect(),
        }
    }

    /// Linha do comando, para o log em stderr.
    pub fn label(&self) -> String {
        std::iter::once(self.program.as_str())
            .chain(self.args.iter().map(String::as_str))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// Os passos do portão, na ordem de execução.
pub fn steps() -> Vec<Step> {
    let clippy = ["clippy", "--workspace", "--all-targets"];
    let deny = ["--", "-D", "warnings"];
    let with = |base: &[&str], extra: &[&str], tail: &[&str]| {
        let args: Vec<&str> = [base, extra, tail].concat();
        Step::new("cargo", &args)
    };
    vec![
        Step::new("cargo", &["fmt", "--check"]),
        with(&clippy, &[], &deny),
        with(&clippy, &["--target", WINDOWS_TARGET], &deny),
        with(&["test", "--workspace"], &[], &[]),
        with(&["test", "--workspace"], &["--target", WINDOWS_TARGET], &[]),
    ]
}

/// Executa os passos em ordem; para no primeiro código diferente de zero e o devolve.
pub fn run_steps(steps: &[Step], runner: &mut impl FnMut(&Step) -> i32) -> i32 {
    for step in steps {
        let code = runner(step);
        if code != 0 {
            return code;
        }
    }
    0
}

/// Executor real: registra o comando em stderr e o roda como processo filho.
/// Usa o binário de `$CARGO` quando o programa é `cargo`.
pub fn run_process(step: &Step) -> i32 {
    eprintln!("==> {}", step.label());
    let program = if step.program == "cargo" {
        std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned())
    } else {
        step.program.clone()
    };
    match Command::new(program).args(&step.args).status() {
        Ok(status) => status.code().unwrap_or(EXIT_NO_CODE),
        Err(err) => {
            eprintln!("erro ao executar `{}`: {err}", step.label());
            EXIT_NO_CODE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cargo(args: &[&str]) -> Step {
        Step::new("cargo", args)
    }

    #[test]
    fn steps_are_the_quality_gate_in_order() {
        let win = "x86_64-pc-windows-gnu";
        let expected = vec![
            cargo(&["fmt", "--check"]),
            cargo(&[
                "clippy",
                "--workspace",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ]),
            cargo(&[
                "clippy",
                "--workspace",
                "--all-targets",
                "--target",
                win,
                "--",
                "-D",
                "warnings",
            ]),
            cargo(&["test", "--workspace"]),
            cargo(&["test", "--workspace", "--target", win]),
        ];
        assert_eq!(steps(), expected);
    }

    #[test]
    fn run_steps_stops_at_first_failure_and_propagates_its_code() {
        let mut calls = 0;
        let code = run_steps(&steps(), &mut |_| {
            calls += 1;
            if calls == 3 { 101 } else { 0 }
        });
        assert_eq!(code, 101);
        assert_eq!(calls, 3);
    }

    #[test]
    fn run_steps_returns_zero_when_all_pass() {
        let mut calls = 0;
        let code = run_steps(&steps(), &mut |_| {
            calls += 1;
            0
        });
        assert_eq!(code, 0);
        assert_eq!(calls, 5);
    }
}
