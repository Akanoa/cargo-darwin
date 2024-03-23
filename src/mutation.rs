use eyre::eyre;
use std::fs::File;
use std::io::Read;
use std::ops::Range;
use std::path::PathBuf;

use crate::actions::reporting::sink::UnifiedColorDiff;
use crate::report::MutationReport;

#[derive(Debug, PartialEq)]
pub struct Mutation {
    mutation: String,
    pub(crate) chunk: MutationChunk,
    pub(crate) reason: String,
    mutated_file: Option<String>,
    file_path: Option<PathBuf>,
    mutation_project_path: Option<PathBuf>,
    report: Option<MutationReport>,
    pub(crate) function_name: String,
    id: usize,
}

impl Mutation {
    pub fn display(&self, pretty_diff: bool) -> eyre::Result<String> {
        let file_path = self
            .file_path
            .as_ref()
            .ok_or(eyre!("Mutated file not specified"))?;

        let file_path_string = dunce::simplified(file_path)
            .to_str()
            .ok_or(eyre!("Unable to make a string from file_path"))?;
        let mutated_file = format!("Mutation of file {}", file_path_string);

        let mut mutation_status = "".to_string();
        if let Some(report) = &self.report {
            let MutationReport { status, .. } = report;
            mutation_status = format!("Mutation status : {}", status)
        }

        let reason = &self.reason;
        let reason_string = format!("Mutation reason: {reason}");

        let mutated_content = self
            .mutated_file
            .as_ref()
            .ok_or(eyre!("Mutation result missing"))?;
        let mut file = File::open(file_path).unwrap();
        let mut original_content = String::new();
        file.read_to_string(&mut original_content)?;

        let input = imara_diff::intern::InternedInput::new(
            original_content.as_str(),
            mutated_content.as_str(),
        );

        let diff = if pretty_diff {
            imara_diff::diff(
                imara_diff::Algorithm::Myers,
                &input,
                UnifiedColorDiff::new(&input),
            )
        } else {
            imara_diff::diff(
                imara_diff::Algorithm::Myers,
                &input,
                imara_diff::UnifiedDiffBuilder::new(&input),
            )
        };

        let mutation_diff = format!("Mutation diff:\n{diff}");

        let mut report_str = "".to_string();
        if let Some(report) = &self.report {
            let MutationReport {
                stdout,
                stderr,
                status: _status,
            } = report;
            report_str = format!("stderr:\n{stderr}\nstdout:\n{stdout}--\n");
        }

        Ok(format!(
            "{mutated_file}\n{reason_string}\n{mutation_status}\n{mutation_diff}{report_str}"
        ))
    }

    fn get_details(&self, project_path: &PathBuf) -> eyre::Result<String> {
        let details = format!(
            "Mutation #{} {} in function \"{}\" of file {} at line {}:{}",
            &self.id,
            &self.reason,
            &self.function_name,
            dunce::simplified(self.get_file_path()?.strip_prefix(project_path)?).display(),
            self.chunk.start_point.row + 1,
            self.chunk.start_point.column
        );
        Ok(details)
    }

    pub(crate) fn pretty(&self, project_path: &PathBuf) -> eyre::Result<()> {
        let details = self.get_details(project_path)?;

        let status = self
            .report
            .as_ref()
            .ok_or(eyre!("No report defined"))?
            .pretty();

        println!("{status} : {details}");

        Ok(())
    }

    pub(crate) fn simple(&self, project_path: &PathBuf) -> eyre::Result<String> {
        let details = self.get_details(project_path)?;

        let status = self
            .report
            .as_ref()
            .ok_or(eyre!("No report defined"))?
            .simple();

        let result = format!("{status} : {details}");

        Ok(result)
    }
}

#[derive(Debug, PartialEq, Default)]
pub(crate) struct MutationChunk {
    start: usize,
    end: usize,
    pub(crate) start_point: Point,
    end_point: Point,
}

impl MutationChunk {
    #[allow(unused)]
    fn new_chunk(range: Range<usize>) -> Self {
        MutationChunk {
            start: range.start,
            end: range.end,
            ..Default::default()
        }
    }
}

impl<'a> From<tree_sitter::Node<'a>> for MutationChunk {
    fn from(value: tree_sitter::Node) -> Self {
        MutationChunk {
            start: value.start_byte(),
            end: value.end_byte(),
            start_point: value.start_position().into(),
            end_point: value.end_position().into(),
        }
    }
}

impl<'a> From<&tree_sitter::Node<'a>> for MutationChunk {
    fn from(value: &tree_sitter::Node) -> Self {
        MutationChunk {
            start: value.start_byte(),
            end: value.end_byte(),
            start_point: value.start_position().into(),
            end_point: value.end_position().into(),
        }
    }
}

#[derive(Debug, PartialEq, Default)]
pub(crate) struct Point {
    pub(crate) row: usize,
    pub(crate) column: usize,
}

impl From<tree_sitter::Point> for Point {
    fn from(value: tree_sitter::Point) -> Self {
        Point {
            row: value.row,
            column: value.column,
        }
    }
}

impl Mutation {
    pub(crate) fn new<N: Into<MutationChunk>>(mutation_chunk: &str, node: N) -> Self {
        Mutation {
            mutation: String::from(mutation_chunk),
            chunk: node.into(),
            reason: "".to_string(),
            mutated_file: None,
            file_path: None,
            mutation_project_path: None,
            report: None,
            function_name: "".to_string(),
            id: 0,
        }
    }

    pub(crate) fn with_reason(self, reason: &str) -> Self {
        Mutation {
            reason: reason.to_string(),
            ..self
        }
    }

    pub(crate) fn with_function_name(self, function_name: &str) -> Self {
        Mutation {
            function_name: function_name.to_string(),
            ..self
        }
    }

    pub(crate) fn mutate_file(&mut self, file: &String) {
        let mut file_clone = file.clone();
        let mutated_range = self.chunk.start..self.chunk.end;
        // The mutation chunk as the same size as the mutated area
        // we can swap the chunk in place
        if self.mutation.len() == mutated_range.len() {
            file_clone.replace_range(mutated_range.clone(), &self.mutation);
        }
        // The mutation chunk takes more or less place than the mutated area
        // we have to recreate a new string
        else {
            let (start_part, end_part) = file_clone.split_at(self.chunk.end);
            let mut start_part = String::from(start_part);
            start_part.truncate(start_part.len() - mutated_range.len());
            start_part.push_str(&self.mutation);
            start_part.push_str(end_part);
            file_clone = start_part;
        }

        self.mutated_file = Some(file_clone)
    }

    pub(crate) fn get_mutated_file(&self) -> eyre::Result<&String> {
        self.mutated_file
            .as_ref()
            .ok_or(eyre!("No mutate file generated yet"))
    }

    pub(crate) fn get_file_path(&self) -> eyre::Result<&PathBuf> {
        self.file_path
            .as_ref()
            .ok_or(eyre!("No mutation file path defined yet"))
    }

    pub(crate) fn get_mutation_project_path(&self) -> eyre::Result<&PathBuf> {
        self.mutation_project_path
            .as_ref()
            .ok_or(eyre!("No mutation project path defined yet"))
    }

    pub(crate) fn set_file_path(&mut self, path: &PathBuf) {
        self.file_path = Some(path.clone())
    }

    pub(crate) fn set_mutation_project_path(&mut self, path: &PathBuf) {
        self.mutation_project_path = Some(path.clone())
    }

    pub(crate) fn set_report(&mut self, report: MutationReport) {
        self.report = Some(report)
    }

    pub(crate) fn set_mutation_id(&mut self, id: usize) {
        self.id = id
    }

    pub(crate) fn get_mutation_id(&self) -> usize {
        self.id
    }
}

#[cfg(test)]
mod tests {
    use crate::mutation::{Mutation, MutationChunk};

    #[test]
    fn test_mutation_in_place() {
        let file = r#"Hello, world"#.to_string();
        let mut mutation = Mutation::new("|", MutationChunk::new_chunk(5..6));
        mutation.mutate_file(&file);
        assert_eq!(
            mutation.get_mutated_file().unwrap(),
            &r#"Hello| world"#.to_string()
        );
    }

    #[test]
    fn test_mutation_insert() {
        let file = r#"Hello, world"#.to_string();
        let mut mutation = Mutation::new("|||", MutationChunk::new_chunk(5..6));
        mutation.mutate_file(&file);
        assert_eq!(
            mutation.get_mutated_file().unwrap(),
            &r#"Hello||| world"#.to_string()
        );
    }

    #[test]
    fn test_let_assign() {
        let file = r#"let x = 666;"#.to_string();
        let mut mutation = Mutation::new("42", MutationChunk::new_chunk(8..11));
        mutation.mutate_file(&file);
        assert_eq!(
            mutation.get_mutated_file().unwrap(),
            &r#"let x = 42;"#.to_string()
        );
    }
}
