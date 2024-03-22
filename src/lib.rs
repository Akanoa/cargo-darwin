use crate::report::{MutationReport, MutationStatus};
use clap::Parser;
use cli::{Cli, Darwin};
use eyre::{eyre, Context};
use mutation::Mutation;
use normpath::PathExt;
use std::fs::File;
use std::io::{Read, Stdout, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use syn::{Attribute, ItemFn};
use wait_timeout::ChildExt;
use walkdir::WalkDir;

mod cli;
mod mutation;
mod report;

static FUNCTION_ITEM: &'static str = "function_item";
static ATTRIBUTE_ITEM: &'static str = "attribute_item";
static BLOCK_ITEM: &'static str = "block";
static BINARY_EXPR_ITEM: &'static str = "binary_expression";
static MINUS_ITEM: &'static str = "-";
static PLUS_ITEM: &'static str = "+";

fn rust_source(entry: &walkdir::DirEntry) -> bool {
    entry
        .path()
        .extension()
        .map(|extension| extension == "rs")
        .unwrap_or(false)
}

fn is_test_function(attrs: &Vec<Attribute>) -> eyre::Result<bool> {
    for attr in attrs {
        if let syn::Meta::Path(path) = &attr.meta {
            let merge_path = path
                .segments
                .iter()
                .map(|x| x.ident.to_string())
                .collect::<Vec<_>>()
                .join("::");
            let known_pattern = ["test", "tokio::test"];
            if known_pattern.contains(&merge_path.as_str()) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn check_function_is_test(
    parent: &tree_sitter::Node,
    function_item: &tree_sitter::Node,
    index: usize,
    file: &String,
) -> eyre::Result<bool> {
    if index == 0 {
        return Ok(false);
    }

    // is there attribute on function
    if let Some(attribute_node) = parent.child(index - 1) {
        if attribute_node.kind() == ATTRIBUTE_ITEM {
            let attribute_data = &file[attribute_node.start_byte()..function_item.end_byte()];
            let item_fn: ItemFn = syn::parse_str(attribute_data)?;
            return is_test_function(&item_fn.attrs);
        }
    }

    Ok(false)
}

fn handle_block(
    node_block: tree_sitter::Node,
    file: &String,
    mutations: &mut Vec<Mutation>,
) -> eyre::Result<()> {
    let mut cursor = node_block.walk();
    for child in node_block.children(&mut cursor) {
        if child.kind() == BINARY_EXPR_ITEM {
            let binary_expr_data = &file[child.start_byte()..child.end_byte()];

            let mut binary_expr_cursor = child.walk();
            for component in child.children(&mut binary_expr_cursor) {
                if [MINUS_ITEM, PLUS_ITEM].contains(&component.kind()) {
                    let operator_item = component;

                    let binary_expr: syn::ExprBinary = syn::parse_str(binary_expr_data)?;
                    match binary_expr.op {
                        syn::BinOp::Sub(..) => {
                            log::trace!("Binary - operation found");
                            mutations.push(
                                Mutation::new("+", &operator_item).with_reason("replace - by +"),
                            );
                            mutations.push(
                                Mutation::new("*", &operator_item).with_reason("replace - by *"),
                            );
                            mutations.push(
                                Mutation::new("&&", &operator_item).with_reason("replace - by &&"),
                            );
                        }
                        syn::BinOp::Add(..) => mutations
                            .push(Mutation::new("-", &operator_item).with_reason("replace + by -")),
                        _ => {}
                    }
                }
            }
        }
    }
    Ok(())
}

fn tree_sitter_parse(path: &Path, root_path: &PathBuf) -> eyre::Result<Vec<Mutation>> {
    let relative_path = path.strip_prefix(root_path)?;
    let path = normpath::BasePathBuf::new(path.to_path_buf())?;
    log::info!("Handle file {relative_path:?}");
    let mut source_file = File::open(&path)?;
    let mut content = String::new();
    source_file.read_to_string(&mut content)?;

    let mut parser = tree_sitter::Parser::new();
    parser.set_language(tree_sitter_rust::language())?;

    let tree = parser
        .parse(&content, None)
        .ok_or(eyre!("Unable to parse file {path:?}"))?;

    let mut root_cursor = tree.walk();
    let mut file_mutants = vec![];
    for (child_index, child_node) in tree.root_node().children(&mut root_cursor).enumerate() {
        if child_node.kind() == FUNCTION_ITEM {
            if !check_function_is_test(&tree.root_node(), &child_node, child_index, &content)? {
                let function_data = &content[child_node.start_byte()..child_node.end_byte()];
                let item_fn: ItemFn = syn::parse_str(function_data)?;
                log::info!(
                    "Handle function {} line : {}",
                    item_fn.sig.ident,
                    child_node.start_position().row + 1
                );

                let mut cursor = tree.walk();
                for node in child_node.children(&mut cursor) {
                    if node.kind() == BLOCK_ITEM {
                        handle_block(node, &content, &mut file_mutants)?;
                    }
                }
            }
        }
    }

    for mutation in file_mutants.iter_mut() {
        mutation.set_file_path(&path.as_path().to_path_buf());
        mutation.mutate_file(&content);
    }

    Ok(file_mutants)
}

fn copy_project_with_mutation(
    entries: &Vec<globwalk::DirEntry>,
    project_path: &PathBuf,
    mutation_root: &PathBuf,
    mutation: &Mutation,
) -> eyre::Result<()> {
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

fn generate_mutants(
    mutants: &mut Vec<Mutation>,
    project_path: &PathBuf,
    mutation_root: &PathBuf,
) -> eyre::Result<()> {
    let project_path = std::fs::canonicalize(project_path)?;
    let walker =
        globwalk::GlobWalkerBuilder::from_patterns(&project_path, &["*", "*/**", "!target"])
            .build()?
            .into_iter()
            .filter_map(Result::ok)
            .collect::<Vec<globwalk::DirEntry>>();

    std::fs::create_dir_all(mutation_root)?;

    let mutation_root = std::fs::canonicalize(Path::new(&mutation_root))
        .wrap_err("Unable to get canonical mutation_root")?;

    let mut mutation_id = 0;

    for mutation in mutants {
        let mutation_path = mutation_root.join(format!("{mutation_id}"));
        mutation.set_mutation_project_path(&mutation_path);
        copy_project_with_mutation(&walker, &project_path, &mutation_path, mutation)?;
        mutation_id += 1;
    }

    Ok(())
}

/// Analyze a path
/// Detect Rust files
/// Generate in memory Mutation
fn analyze(root_path: &PathBuf) -> eyre::Result<Vec<Mutation>> {
    let root_path = root_path.normalize().wrap_err("x")?.as_path().to_path_buf();
    let walker = WalkDir::new(&root_path);
    let mut mutants = vec![];

    for entry in walker {
        let entry = entry.wrap_err("Unable to found entry")?;
        if rust_source(&entry) {
            let path = entry.path();
            let mutated_files = tree_sitter_parse(path, &root_path)?;
            mutants.extend(mutated_files);
        }
    }

    Ok(mutants)
}

fn run_test_for_mutation(mutation: &Mutation) -> eyre::Result<MutationReport> {
    let path = mutation.get_mutation_project_path()?;

    let command = std::process::Command::new("cargo")
        .arg("build")
        .current_dir(path)
        .env("RUSTFLAGS", "-Awarnings")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?
        .wait_with_output()?;

    return if command.status.code() == Some(101) {
        let stdout = String::from_utf8_lossy(&command.stdout).to_string();
        let stderr = String::from_utf8_lossy(&command.stderr).to_string();

        println!("stdout:\n{stderr}\nstdout:\n{stdout}");

        Ok(MutationReport::new(
            stdout,
            stderr,
            MutationStatus::CompilationFailed,
        ))
    } else {
        let mut command = std::process::Command::new("cargo")
            .arg("test")
            .current_dir(path)
            .env("RUSTFLAGS", "-Awarnings")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let cargo_test_result = command.wait_timeout(Duration::from_secs(60))?;
        match cargo_test_result {
            Some(status) => {
                let mut stdout = String::new();
                command
                    .stdout
                    .ok_or(eyre!("No stdout"))?
                    .read_to_string(&mut stdout)?;
                let mut stderr = String::new();
                command
                    .stderr
                    .ok_or(eyre!("No stderr"))?
                    .read_to_string(&mut stderr)?;

                let status = match status.code() {
                    Some(101) => MutationStatus::Fail,
                    Some(0) => MutationStatus::Success,
                    _ => unreachable!(),
                };
                Ok(MutationReport::new(stdout, stderr, status))
            }
            None => {
                command.kill()?;
                Ok(MutationReport::new(
                    "".to_string(),
                    "Timeout!".to_string(),
                    MutationStatus::Timeout,
                ))
            }
        }
    };
}

fn run_tests(mutations: &Vec<Mutation>) -> eyre::Result<()> {
    for mutation in mutations {
        let report = run_test_for_mutation(mutation)?;
        dbg!(report);
    }
    Ok(())
}

/// Display mutation but don't run tests
fn display_mutations(mutations: &Vec<Mutation>) -> eyre::Result<()> {
    for mutation in mutations {
        println!("{}", mutation.display()?)
    }
    Ok(())
}

pub fn run() -> eyre::Result<()> {
    let cli = Cli::parse();

    let Cli::Darwin(Darwin {
        mutation_path,
        root_path,
        dry_run,
    }) = cli;

    let mut mutants = analyze(&PathBuf::from(&root_path))?;

    if !dry_run {
        generate_mutants(&mut mutants, &root_path, &mutation_path)?;
        run_tests(&mutants)?;
    } else {
        display_mutations(&mutants)?;
    }

    Ok(())
}
