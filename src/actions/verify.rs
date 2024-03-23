use std::io::Read;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use eyre::eyre;
use wait_timeout::ChildExt;

use crate::mutation::Mutation;
use crate::report::{MutationReport, MutationStatus};

/// Run cargo build on mutated project
///
/// Run cargo test
///
/// Capture output
///
/// Generate the report
pub(crate) fn run_test_for_mutation(
    mutation: &mut Mutation,
    project_path: &PathBuf,
) -> eyre::Result<()> {
    let path = mutation.get_mutation_project_path()?;

    log::trace!(
        "Build mutation {} in function {} of file {} at line {}:{}",
        mutation.reason,
        mutation.function_name,
        dunce::simplified(mutation.get_file_path()?.strip_prefix(project_path)?).display(),
        mutation.chunk.start_point.row + 1,
        mutation.chunk.start_point.column
    );

    log::trace!(
        "Test mutation {} in function {} of file {} at line {}:{}",
        mutation.reason,
        mutation.function_name,
        dunce::simplified(mutation.get_file_path()?.strip_prefix(project_path)?).display(),
        mutation.chunk.start_point.row + 1,
        mutation.chunk.start_point.column
    );

    let command = std::process::Command::new("cargo")
        .arg("build")
        .current_dir(path)
        .env("RUSTFLAGS", "-Awarnings")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?
        .wait_with_output()?;

    let report = if command.status.code() == Some(101) {
        let stdout = String::from_utf8_lossy(&command.stdout).to_string();
        let stderr = String::from_utf8_lossy(&command.stderr).to_string();

        MutationReport::new(stdout, stderr, MutationStatus::CompilationFailed)
    } else {
        let mut command = std::process::Command::new("cargo")
            .arg("test")
            .current_dir(path)
            .env("RUSTFLAGS", "-Awarnings")
            .env("RUST_BACKTRACE", "0")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let cargo_test_result = command.wait_timeout(Duration::from_secs(60))?;
        match cargo_test_result {
            Some(status) => {
                let mut stdout = String::new();
                command
                    .stdout
                    .ok_or(eyre!("No stdout"))?
                    .read_to_string(&mut stdout)?;
                let mut stderr = String::new();
                command
                    .stderr
                    .ok_or(eyre!("No stderr"))?
                    .read_to_string(&mut stderr)?;

                let status = match status.code() {
                    Some(101) => MutationStatus::Fail,
                    Some(0) => MutationStatus::Success,
                    _ => unreachable!(),
                };
                MutationReport::new(stdout, stderr, status)
            }
            None => {
                command.kill()?;
                MutationReport::new(
                    "".to_string(),
                    "Timeout!".to_string(),
                    MutationStatus::Timeout,
                )
            }
        }
    };
    mutation.set_report(report);
    mutation.pretty(project_path)?;
    Ok(())
}
