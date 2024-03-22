#[derive(Debug, PartialEq)]
pub(crate) enum MutationStatus {
    Success,
    Fail,
    Timeout,
    CompilationFailed,
}

#[derive(Debug, PartialEq)]
pub(crate) struct MutationReport {
    pub(crate) stdout: String,
    pub(crate) stderr: String,
    pub(crate) status: MutationStatus,
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
