use std::fs;
use std::fs::File;
use std::io::Write;
use std::ops::ControlFlow;
use std::path::PathBuf;

use crate::mutation::Mutation;

pub(crate) mod sink;

fn generate_report(mutation: &Mutation, mutation_root: &PathBuf) -> eyre::Result<()> {
    let content = mutation.display(false)?;
    let data = content.as_bytes();
    let mutation_log_path =
        mutation_root.join(format!("mutation_{}.log", mutation.get_mutation_id()));
    let mut mutation_log_file = File::create(mutation_log_path)?;
    mutation_log_file.write_all(data)?;

    Ok(())
}

fn generate_summary(
    mutations: &Vec<Mutation>,
    mutation_root: &PathBuf,
    project_path: &PathBuf,
) -> eyre::Result<()> {
    let summary_path = mutation_root.join("summary");
    let mut summary_file = File::create(summary_path)?;

    let data = mutations
        .iter()
        .try_fold(vec![], |mut acc: Vec<u8>, mutation| {
            match mutation.simple(project_path) {
                Ok(data) => {
                    acc.extend_from_slice(format!("{data}\n").as_bytes());
                    ControlFlow::Continue(acc)
                }
                Err(err) => ControlFlow::Break(err),
            }
        });

    match data {
        ControlFlow::Continue(data) => {
            summary_file.write_all(&data)?;
        }
        ControlFlow::Break(err) => Err(err)?,
    }
    Ok(())
}

pub fn generate_reports(
    mutations: &Vec<Mutation>,
    mutation_root: &PathBuf,
    project_path: &PathBuf,
) -> eyre::Result<()> {
    log::info!("Generate reports");
    let report_path = mutation_root.join("reports");
    fs::create_dir_all(&report_path)?;

    for mutation in mutations {
        generate_report(mutation, &report_path)?
    }
    generate_summary(mutations, mutation_root, project_path)?;
    Ok(())
}
