//! Fixed fixture cadence through the matched CPU region owner.
use std::process::ExitCode;
fn main() -> ExitCode {
    match lom::cli::run_cpu_region_cadence(std::env::args_os().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("lom-cpu-region-cadence: {error}");
            ExitCode::from(2)
        }
    }
}
