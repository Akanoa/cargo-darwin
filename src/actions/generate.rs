use crate::actions::get_project_walker;
use crate::actions::verify::run_test_for_mutation;
use crate::mutation::Mutation;
use eyre::{eyre, WrapErr};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};

fn create_mutated_project(
    entries: &Vec<globwalk::DirEntry>,
    project_path: &PathBuf,
    mutation_root: &PathBuf,
    mutation: &Mutation,
) -> eyre::Result<()> {
    log::debug!("Create mutation {}", mutation.get_mutation_id());
    log::trace!(
        "Create mutation {} in function {} of file {} at line {}:{}",
        mutation.reason,
        mutation.function_name,
        dunce::simplified(mutation.get_file_path()?.strip_prefix(project_path)?).display(),
        mutation.chunk.start_point.row + 1,
        mutation.chunk.start_point.column
    );
    std::fs::create_dir_all(mutation_root)?;

    for entry in entries {
        let old_path = entry.path();
        let relative_path = entry.path().strip_prefix(project_path.as_path())?;
        let new_path = mutation_root.join(Path::new(&relative_path).to_path_buf());

        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&new_path)?;
        } else {
            std::fs::copy(old_path, new_path)?;
        }
    }

    let relative_path = std::fs::canonicalize(mutation.get_file_path()?)?;
    let mutant_file_path = relative_path.strip_prefix(project_path.as_path())?;
    let mutant_file_path = std::fs::canonicalize(mutation_root.join(mutant_file_path))
        .wrap_err(eyre!("Unable to canonicalize path {mutant_file_path:?}"))?;
    let mut file_to_mutate = File::create(&mutant_file_path)
        .wrap_err(eyre!("Unable to open file {mutant_file_path:?}"))?;
    file_to_mutate
        .write_all(mutation.get_mutated_file()?.as_bytes())
        .wrap_err(eyre!("Unable to write file {mutant_file_path:?}"))?;
    file_to_mutate.flush()?;

    Ok(())
}

pub fn generate_and_verify_mutants(
    mutants: &mut Vec<Mutation>,
    project_path: &PathBuf,
    mutation_root: &PathBuf,
) -> eyre::Result<()> {
    log::info!("Generate mutant projects");

    // Clean previous run
    if Path::exists(mutation_root) {
        log::debug!("Cleaning {}", mutation_root.display());
        std::fs::remove_dir_all(mutation_root)?;
    }

    let walker = get_project_walker(project_path)?;
    log::debug!("Creating {}", mutation_root.display());
    std::fs::create_dir_all(mutation_root)?;

    let mutation_root = std::fs::canonicalize(Path::new(&mutation_root))
        .wrap_err("Unable to get canonical mutation_root")?;

    let mut mutation_id = 0;

    for mutation in mutants {
        let mutation_path = mutation_root.join(format!("{mutation_id}"));
        mutation.set_mutation_project_path(&mutation_path);
        mutation.set_mutation_id(mutation_id);
        create_mutated_project(&walker, &project_path, &mutation_path, mutation)?;
        run_test_for_mutation(mutation, project_path)?;
        mutation_id += 1;
    }

    Ok(())
}
