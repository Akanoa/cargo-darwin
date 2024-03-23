use crate::mutation::Mutation;

pub(crate) fn clean_mutation_project(mutation: &Mutation) -> eyre::Result<()> {
    log::debug!("Remove project mutation");
    let mutation_project_path = mutation.get_mutation_project_path()?;
    std::fs::remove_dir_all(mutation_project_path)?;

    Ok(())
}
