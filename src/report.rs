#[derive(Debug)]
pub(crate) enum MutationStatus {
    Success,
    Fail,
    Timeout,
    CompilationFailed,
}

#[derive(Debug)]
pub(crate) struct MutationReport {
    stdout: String,
    stderr: String,
    status: MutationStatus,
}

impl MutationReport {
    pub(crate) fn new(stdout: String, stderr: String, status: MutationStatus) -> Self {
        MutationReport {
            stdout,
            stderr,
            status,
        }
    }
}
