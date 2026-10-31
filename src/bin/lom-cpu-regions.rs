//! Fixed fixture sequence through ordinary CPU region updates.
use std::process::ExitCode;
fn main() -> ExitCode {
    match lom::cli::run_cpu_regions(std::env::args_os().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("lom-cpu-regions: {error}");
            ExitCode::from(2)
        }
    }
}
