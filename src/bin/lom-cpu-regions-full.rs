//! Same fixed fixture sequence with full-frame updates on the six-slot owner.
use std::process::ExitCode;
fn main() -> ExitCode {
    match lom::cli::run_cpu_regions_full(std::env::args_os().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("lom-cpu-regions-full: {error}");
            ExitCode::from(2)
        }
    }
}
