use clap::Parser;
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

#[derive(clap::Args, Debug)]
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
