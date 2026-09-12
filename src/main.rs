//! Command-line entrypoint for Lom's project scaffold.

use std::process::ExitCode;

const HELP: &str = "Lom — native Sophia shell (project scaffold)

Usage: lom [--help | --version]

  -h, --help       Show this help
  -V, --version    Show the package version

Native shell startup is not implemented yet.
This scaffold does not connect to a display, initialize a GPU, or contact Sophia.
";

fn main() -> ExitCode {
    let mut arguments = std::env::args_os().skip(1);
    match (arguments.next(), arguments.next()) {
        (Some(flag), None) if flag == "--help" || flag == "-h" => {
            print!("{HELP}");
            ExitCode::SUCCESS
        }
        (Some(flag), None) if flag == "--version" || flag == "-V" => {
            println!("lom {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        (None, None) => {
            eprintln!("lom: native shell startup is not implemented yet; use --help");
            ExitCode::from(2)
        }
        _ => {
            eprintln!("lom: unsupported arguments; use --help");
            ExitCode::from(2)
        }
    }
}
