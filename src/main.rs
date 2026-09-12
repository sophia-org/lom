//! Command-line boundary. Default startup does not open a session.

use std::process::ExitCode;

fn main() -> ExitCode {
    match lom::cli::run(std::env::args_os().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("lom: {error}");
            ExitCode::from(2)
        }
    }
}
