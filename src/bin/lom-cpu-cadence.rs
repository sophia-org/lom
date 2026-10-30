//! Diagnostic Vello CPU cadence entry; it serves only the protected CPU arms.

use std::process::ExitCode;

fn main() -> ExitCode {
    match lom::cli::run_cpu_cadence(std::env::args_os().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("lom-cpu-cadence: {error}");
            ExitCode::from(2)
        }
    }
}
