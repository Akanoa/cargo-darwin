use eyre::eyre;
use imara_diff::UnifiedDiffBuilder;
use std::fmt::{Display, Formatter};
use std::fs::File;
use std::io::Read;
use std::ops::Range;
use std::path::PathBuf;

#[derive(Debug, PartialEq)]
pub struct Mutation {
    mutation: String,
    chunk: MutationChunk,
    reason: Option<String>,
    mutated_file: Option<String>,
    file_path: Option<PathBuf>,
}

impl Mutation {
    pub fn display(&self) -> eyre::Result<String> {
        let file_path = self
            .file_path
            .as_ref()
            .ok_or(eyre!("Mutated file not specified"))?;

        let file_path_string = file_path
            .to_str()
            .ok_or(eyre!("Unable to make a string from file_path"))?;
        let mutated_file = format!("Mutation of file {file_path_string}");

        let reason = self
            .reason
            .as_ref()
            .ok_or(eyre!("Mutation reason missing"))?;
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
        let diff = imara_diff::diff(
            imara_diff::Algorithm::Myers,
            &input,
            UnifiedDiffBuilder::new(&input),
        );

        let mutation_diff = format!("Mutation diff:\n{diff}");

        Ok(format!("{mutated_file}\n{reason_string}\n{mutation_diff}"))
    }
}

impl Display for &Mutation {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut content = "Mutation of file ".to_string();

        write!(f, "{content}")
    }
}

#[derive(Debug, PartialEq, Default)]
struct MutationChunk {
    start: usize,
    end: usize,
    start_point: Point,
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
struct Point {
    row: usize,
    column: usize,
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
            reason: None,
            mutated_file: None,
            file_path: None,
        }
    }

    pub(crate) fn with_reason<'a>(self, reason: &'a str) -> Self {
        Mutation {
            reason: Some(reason.to_string()),
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

    pub(crate) fn get_mutated_file(&self) -> &Option<String> {
        &self.mutated_file
    }

    pub(crate) fn set_file_path(&mut self, path: &PathBuf) {
        self.file_path = Some(path.clone())
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
            mutation.get_mutated_file(),
            &Some(r#"Hello| world"#.to_string())
        );
    }

    #[test]
    fn test_mutation_insert() {
        let file = r#"Hello, world"#.to_string();
        let mut mutation = Mutation::new("|||", MutationChunk::new_chunk(5..6));
        mutation.mutate_file(&file);
        assert_eq!(
            mutation.get_mutated_file(),
            &Some(r#"Hello||| world"#.to_string())
        );
    }

    #[test]
    fn test_let_assign() {
        let file = r#"let x = 666;"#.to_string();
        let mut mutation = Mutation::new("42", MutationChunk::new_chunk(8..11));
        mutation.mutate_file(&file);
        assert_eq!(
            mutation.get_mutated_file(),
            &Some(r#"let x = 42;"#.to_string())
        );
    }
}