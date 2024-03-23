use colored::Colorize;
use std::fmt::{Display, Formatter};

#[derive(Debug, PartialEq)]
pub(crate) enum MutationStatus {
    Success,
    Fail,
    Timeout,
    CompilationFailed,
}

impl Display for MutationStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            MutationStatus::Success => write!(f, "Missing test, code base vulnerable to mutation"),
            MutationStatus::Fail => write!(f, "Mutation caught, code base robust to mutation"),
            MutationStatus::Timeout => write!(f, "Mutation causes an infinite loop, inconclusive"),
            MutationStatus::CompilationFailed => write!(f, "Mutation killed, unsustainable"),
        }
    }
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

    pub(crate) fn pretty(&self) -> String {
        match self.status {
            MutationStatus::Success => {
                // Tests pass, the mutation hasn't been caught, suspicion of missing test
                format!("{}", "[Missing]".yellow())
            }
            MutationStatus::Fail => {
                // Tests failed, the mutation has been caught
                format!("{}", "[OK]     ".green())
            }
            MutationStatus::Timeout => {
                // Mutation introduces infinite loop, inconclusive
                format!("{}", "[Timeout]".white())
            }
            MutationStatus::CompilationFailed => {
                // Mutation introduces non compilable project
                format!("{}", "[Killed] ".white())
            }
        }
    }

    pub(crate) fn simple(&self) -> String {
        match self.status {
            MutationStatus::Success => {
                // Tests pass, the mutation hasn't been caught, suspicion of missing test
                format!("{}", "[Missing]")
            }
            MutationStatus::Fail => {
                // Tests failed, the mutation has been caught
                format!("{}", "[OK]")
            }
            MutationStatus::Timeout => {
                // Mutation introduces infinite loop, inconclusive
                format!("{}", "[Timeout]")
            }
            MutationStatus::CompilationFailed => {
                // Mutation introduces non compilable project
                format!("{}", "[Killed]")
            }
        }
    }
}
