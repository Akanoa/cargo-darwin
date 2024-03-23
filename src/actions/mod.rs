use std::path::PathBuf;

pub(crate) mod analyze;
pub(crate) mod clean;
pub(crate) mod generate;
pub(crate) mod reporting;
pub(crate) mod verify;

pub(crate) fn get_project_walker(project_path: &PathBuf) -> eyre::Result<Vec<globwalk::DirEntry>> {
    let project_path = std::fs::canonicalize(project_path)?;
    let entries =
        globwalk::GlobWalkerBuilder::from_patterns(&project_path, &["*", "*/**", "!target"])
            .build()?
            .into_iter()
            .filter_map(Result::ok)
            .collect::<Vec<globwalk::DirEntry>>();
    Ok(entries)
}
