use clap::Parser;
use colored::Colorize;
use std::env;
use std::path::PathBuf;

fn get_default_project_path() -> PathBuf {
    let path = env::current_dir().unwrap();
    path
}

fn get_default_mutation_path() -> PathBuf {
    let mut path = env::current_dir().unwrap();
    path.push("tmp");
    path
}

#[derive(Parser, Debug)]
#[command(bin_name = "cargo")]
#[command(name = "cargo")]
pub enum Cli {
    Darwin(Darwin),
}

pub(crate) fn help() -> String {
    format!(
        r#"
{} : Tests pass, the mutation hasn't been caught, suspicion of missing test
{}      : Tests failed, the mutation has been caught
{} : Mutation introduces infinite loop, inconclusive
{}  : Mutation introduces non buildable modification
    "#,
        "[Missing]".yellow(),
        "[OK]".green(),
        "[Timeout]".white(),
        "[Killed]".white()
    )
}

#[derive(clap::Args, Debug)]
/// Darwin mutates your code, if your code still passes check tests, then your code isn't
/// enough tested
pub struct Darwin {
    /// Path of the project to mutate
    #[arg(name = "PROJECT PATH", default_value = get_default_project_path().into_os_string())]
    pub(crate) root_path: PathBuf,
    /// Root path to mutated projects
    #[arg(long, default_value = get_default_mutation_path().into_os_string())]
    pub(crate) mutation_path: PathBuf,
    /// Don't run the mutation only list them
    #[arg(long, action, default_value = "false")]
    pub(crate) dry_run: bool,
}
