//! Fixed diagnostic cadence using the ordinary CPU checksum policy.

use std::process::ExitCode;

fn main() -> ExitCode {
    match lom::cli::run_cpu_cadence_no_checksum(std::env::args_os().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("lom-cpu-cadence-no-checksum: {error}");
            ExitCode::from(2)
        }
    }
}
