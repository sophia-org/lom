//! Diagnostic Vello CPU reference entry; it serves only the protected CPU arms.

use std::process::ExitCode;

fn main() -> ExitCode {
    match lom::cli::run_cpu_reference(std::env::args_os().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("lom-cpu-reference: {error}");
            ExitCode::from(2)
        }
    }
}
