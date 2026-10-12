//! Argumentos da CLI (T020, R8, contracts/cli.md): só o parse. Nada de varredura nem de relatório
//! aqui (T021/T022). Usa `lexopt`, com todas as mensagens em português (FR-019).
//!
//! Os argumentos recebidos por [`parse`] NÃO incluem o `argv[0]` (o nome do programa): o `main`
//! passa `std::env::args_os().skip(1)`. O caminho fica como `OsString`/`PathBuf`, sem exigir
//! UTF-8, para nomes inválidos e surrogates soltos passarem intactos.

use std::ffi::{OsStr, OsString};
use std::fmt;
use std::num::NonZeroUsize;
use std::path::PathBuf;

use lexopt::{Arg, Parser};

/// Tamanho padrão dos rankings (20), verificado como não nulo em tempo de compilação.
const DEFAULT_TOP: NonZeroUsize = match NonZeroUsize::new(20) {
    Some(top) => top,
    None => panic!("o padrão de --top é maior que zero"),
};

/// Texto de ajuda: uso, opções e códigos de saída (contracts/cli.md).
pub const HELP: &str = "\
Disk Analyst: analisa o uso de disco.

Uso:
  disk-analyst scan <CAMINHO> [--top N] [--json] [--progress] [--engine auto|dir|mft] [--stats]
  disk-analyst --help | --version

Argumentos:
  <CAMINHO>        pasta, raiz de volume ou arquivo; se for um link, a raiz é resolvida

Opções:
  --top N          tamanho dos rankings; inteiro maior ou igual a 1 (padrão: 20)
  --json           escreve em stdout só o documento JSON do relatório
  --progress       mostra o progresso em stderr; ignorado com --json
  --engine MOTOR   auto ou dir usam a varredura de diretórios; mft ainda não está disponível
  --stats          ao final, escreve em stderr o pico de memória, os itens e os bytes por item
  -h, --help       mostra esta ajuda
  -V, --version    mostra a versão

Códigos de saída:
  0     varredura concluída (mesmo com itens inacessíveis)
  2     erro de uso: opção inválida, caminho inexistente ou raiz ilegível, --engine mft
  130   cancelada por Ctrl+C (relatório parcial emitido no primeiro sinal)
  1     erro inesperado de E/S ao escrever a saída
";

/// O que o usuário pediu.
#[derive(Debug, PartialEq)]
pub enum Command {
    Scan(ScanArgs),
    Help,
    Version,
}

/// Argumentos de `scan`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanArgs {
    pub path: PathBuf,
    pub top: NonZeroUsize,
    pub json: bool,
    pub progress: bool,
    pub engine: Engine,
    pub stats: bool,
}

/// Motor de varredura. `mft` não existe aqui: é erro de uso até a Fase 2 (ADR-0003).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Engine {
    Auto,
    Dir,
}

/// Erro de uso. O `Display` é só a mensagem, em português; dica de `--help` é do `main`.
#[derive(Debug, PartialEq, Eq)]
pub struct UsageError(String);

impl fmt::Display for UsageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UsageError {}

fn usage(message: impl Into<String>) -> UsageError {
    UsageError(message.into())
}

/// Traduz um erro do `lexopt` (em inglês) para uma mensagem de uso em português.
fn from_lexopt(error: lexopt::Error) -> UsageError {
    match error {
        lexopt::Error::MissingValue {
            option: Some(option),
        } => usage(format!("falta o valor de {option}")),
        lexopt::Error::MissingValue { option: None } => usage("falta um valor"),
        lexopt::Error::UnexpectedOption(option) => usage(format!("opção desconhecida: {option}")),
        lexopt::Error::UnexpectedArgument(argument) => {
            usage(format!("argumento a mais: {}", argument.to_string_lossy()))
        }
        lexopt::Error::UnexpectedValue { option, .. } => {
            usage(format!("a opção {option} não aceita valor"))
        }
        _ => usage("argumento inválido"),
    }
}

/// `--help`/`-h` e `--version`/`-V` valem em qualquer posição (até um `--`, que encerra as
/// opções). Vence o primeiro que aparecer.
fn early_command(args: &[OsString]) -> Option<Command> {
    for arg in args {
        match arg.to_str() {
            Some("--") => return None,
            Some("--help" | "-h") => return Some(Command::Help),
            Some("--version" | "-V") => return Some(Command::Version),
            _ => {}
        }
    }
    None
}

/// Interpreta os argumentos (sem o `argv[0]`). Sem argumentos é erro de uso, com dica de `--help`.
pub fn parse<I>(args: I) -> Result<Command, UsageError>
where
    I: IntoIterator<Item = OsString>,
{
    let args: Vec<OsString> = args.into_iter().collect();
    if let Some(command) = early_command(&args) {
        return Ok(command);
    }

    let mut parser = Parser::from_args(args);
    match parser.next().map_err(from_lexopt)? {
        None => Err(usage("faltou o subcomando; use --help para ver o uso")),
        Some(Arg::Value(value)) if value == "scan" => parse_scan(&mut parser),
        Some(Arg::Value(value)) => Err(usage(format!(
            "subcomando desconhecido: {}",
            value.to_string_lossy()
        ))),
        Some(other) => Err(from_lexopt(other.unexpected())),
    }
}

fn parse_scan(parser: &mut Parser) -> Result<Command, UsageError> {
    let mut path: Option<PathBuf> = None;
    let mut top = DEFAULT_TOP;
    let (mut json, mut progress, mut stats) = (false, false, false);
    let mut engine = Engine::Auto;

    while let Some(arg) = parser.next().map_err(from_lexopt)? {
        match arg {
            Arg::Long("top") => top = parse_top(&parser.value().map_err(from_lexopt)?)?,
            Arg::Long("json") => json = true,
            Arg::Long("progress") => progress = true,
            Arg::Long("stats") => stats = true,
            Arg::Long("engine") => engine = parse_engine(&parser.value().map_err(from_lexopt)?)?,
            Arg::Value(value) => {
                if path.is_some() {
                    return Err(from_lexopt(lexopt::Error::UnexpectedArgument(value)));
                }
                path = Some(PathBuf::from(value));
            }
            other => return Err(from_lexopt(other.unexpected())),
        }
    }

    let path = path.ok_or_else(|| usage("faltou o <CAMINHO>"))?;
    Ok(Command::Scan(ScanArgs {
        path,
        top,
        json,
        progress,
        engine,
        stats,
    }))
}

fn parse_top(value: &OsStr) -> Result<NonZeroUsize, UsageError> {
    value
        .to_str()
        .and_then(|text| text.parse::<usize>().ok())
        .and_then(NonZeroUsize::new)
        .ok_or_else(|| usage("--top espera um inteiro maior ou igual a 1"))
}

fn parse_engine(value: &OsStr) -> Result<Engine, UsageError> {
    match value.to_str() {
        Some("auto") => Ok(Engine::Auto),
        Some("dir") => Ok(Engine::Dir),
        Some("mft") => Err(usage("Leitura da MFT ainda não disponível")),
        _ => Err(usage("--engine aceita auto, dir ou mft")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::num::NonZeroUsize;
    use std::path::PathBuf;

    fn parse_strs(args: &[&str]) -> Result<Command, UsageError> {
        parse(args.iter().map(OsString::from))
    }

    fn error_of(args: &[&str]) -> String {
        match parse_strs(args) {
            Err(e) => e.to_string(),
            Ok(command) => panic!("esperava erro de uso, veio {command:?} para {args:?}"),
        }
    }

    fn defaults(path: &str) -> ScanArgs {
        ScanArgs {
            path: PathBuf::from(path),
            top: NonZeroUsize::new(20).unwrap(),
            json: false,
            progress: false,
            engine: Engine::Auto,
            stats: false,
        }
    }

    #[test]
    fn scan_with_only_a_path_uses_the_defaults() {
        assert_eq!(
            parse_strs(&["scan", r"C:\x"]).unwrap(),
            Command::Scan(defaults(r"C:\x"))
        );
    }

    #[test]
    fn scan_with_every_option_turns_everything_on() {
        let expected = ScanArgs {
            path: PathBuf::from("x"),
            top: NonZeroUsize::new(5).unwrap(),
            json: true,
            progress: true,
            engine: Engine::Dir,
            stats: true,
        };
        assert_eq!(
            parse_strs(&[
                "scan",
                "x",
                "--top",
                "5",
                "--json",
                "--progress",
                "--engine",
                "dir",
                "--stats"
            ])
            .unwrap(),
            Command::Scan(expected)
        );
    }

    #[test]
    fn options_may_come_before_the_path() {
        let Command::Scan(args) = parse_strs(&["scan", "--json", "--top", "3", "x"]).unwrap()
        else {
            panic!("esperava Scan");
        };
        assert_eq!(args.path, PathBuf::from("x"));
        assert!(args.json);
        assert_eq!(args.top.get(), 3);
    }

    #[test]
    fn top_accepts_the_equals_form() {
        let Command::Scan(args) = parse_strs(&["scan", "x", "--top=7"]).unwrap() else {
            panic!("esperava Scan");
        };
        assert_eq!(args.top.get(), 7);
    }

    #[test]
    fn invalid_top_values_are_usage_errors() {
        let message = "--top espera um inteiro maior ou igual a 1";
        for bad in ["0", "x", "-5", "1.5", ""] {
            assert_eq!(
                error_of(&["scan", "x", "--top", bad]),
                message,
                "--top {bad:?}"
            );
        }
        assert_eq!(error_of(&["scan", "x", "--top=0"]), message);
    }

    #[test]
    fn top_without_a_value_is_a_missing_value_error() {
        assert_eq!(error_of(&["scan", "x", "--top"]), "falta o valor de --top");
    }

    #[test]
    fn engine_auto_and_dir_are_accepted() {
        for (name, engine) in [("auto", Engine::Auto), ("dir", Engine::Dir)] {
            let Command::Scan(args) = parse_strs(&["scan", "x", "--engine", name]).unwrap() else {
                panic!("esperava Scan");
            };
            assert_eq!(args.engine, engine);
        }
    }

    #[test]
    fn engine_mft_is_a_usage_error_with_the_exact_message() {
        assert_eq!(
            error_of(&["scan", "x", "--engine", "mft"]),
            "Leitura da MFT ainda não disponível"
        );
    }

    #[test]
    fn unknown_engine_is_a_usage_error() {
        assert_eq!(
            error_of(&["scan", "x", "--engine", "foo"]),
            "--engine aceita auto, dir ou mft"
        );
    }

    #[test]
    fn engine_without_a_value_is_a_missing_value_error() {
        assert_eq!(
            error_of(&["scan", "x", "--engine"]),
            "falta o valor de --engine"
        );
    }

    #[test]
    fn scan_without_a_path_is_a_usage_error() {
        assert_eq!(error_of(&["scan"]), "faltou o <CAMINHO>");
        assert_eq!(error_of(&["scan", "--json"]), "faltou o <CAMINHO>");
    }

    #[test]
    fn a_second_path_is_an_extra_argument() {
        assert_eq!(error_of(&["scan", "a", "b"]), "argumento a mais: b");
    }

    #[test]
    fn unknown_option_is_reported_by_name() {
        assert_eq!(
            error_of(&["--frobnicate"]),
            "opção desconhecida: --frobnicate"
        );
        assert_eq!(
            error_of(&["scan", "x", "--frobnicate"]),
            "opção desconhecida: --frobnicate"
        );
        assert_eq!(error_of(&["scan", "x", "-z"]), "opção desconhecida: -z");
    }

    #[test]
    fn a_flag_does_not_take_a_value() {
        assert_eq!(
            error_of(&["scan", "x", "--json=1"]),
            "a opção --json não aceita valor"
        );
    }

    #[test]
    fn unknown_subcommand_is_a_usage_error() {
        assert_eq!(error_of(&["foo"]), "subcomando desconhecido: foo");
    }

    #[test]
    fn no_arguments_is_a_usage_error_with_a_hint_to_help() {
        assert_eq!(
            error_of(&[]),
            "faltou o subcomando; use --help para ver o uso"
        );
    }

    #[test]
    fn help_and_version_win_in_any_position() {
        assert_eq!(parse_strs(&["--help"]).unwrap(), Command::Help);
        assert_eq!(parse_strs(&["-h"]).unwrap(), Command::Help);
        assert_eq!(parse_strs(&["scan", "--help"]).unwrap(), Command::Help);
        assert_eq!(
            parse_strs(&["scan", "a", "b", "--help"]).unwrap(),
            Command::Help
        );
        assert_eq!(parse_strs(&["--version"]).unwrap(), Command::Version);
        assert_eq!(parse_strs(&["-V"]).unwrap(), Command::Version);
        assert_eq!(
            parse_strs(&["scan", "x", "--top", "0", "-V"]).unwrap(),
            Command::Version
        );
    }

    #[test]
    fn help_text_mentions_every_option_and_the_exit_codes() {
        for item in [
            "<CAMINHO>",
            "--top",
            "--json",
            "--progress",
            "--engine",
            "--stats",
            "--help",
            "--version",
            "auto",
            "dir",
            "mft",
        ] {
            assert!(HELP.contains(item), "o HELP não cita {item}");
        }
        for code in ["0", "2", "130", "1"] {
            assert!(
                HELP.contains(code),
                "o HELP não cita o código de saída {code}"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_path_is_preserved_byte_for_byte() {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};
        let raw = OsString::from_vec(b"a\xFFb".to_vec());
        let command = parse([OsString::from("scan"), raw.clone()]).unwrap();
        let Command::Scan(args) = command else {
            panic!("esperava Scan")
        };
        assert_eq!(args.path.as_os_str().as_bytes(), b"a\xFFb");
        assert_eq!(args.path.into_os_string(), raw);
    }

    #[cfg(windows)]
    #[test]
    fn path_with_a_lone_surrogate_is_preserved() {
        use std::os::windows::ffi::{OsStrExt, OsStringExt};
        let units = [0x61u16, 0xD800, 0x62];
        let raw = OsString::from_wide(&units);
        let command = parse([OsString::from("scan"), raw.clone()]).unwrap();
        let Command::Scan(args) = command else {
            panic!("esperava Scan")
        };
        assert_eq!(
            args.path.as_os_str().encode_wide().collect::<Vec<_>>(),
            units
        );
        assert_eq!(args.path.into_os_string(), raw);
    }
}
