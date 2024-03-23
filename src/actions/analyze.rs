use crate::actions::get_project_walker;
use crate::mutation::Mutation;
use eyre::{eyre, WrapErr};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use syn::{Attribute, ItemFn};

pub static FUNCTION_ITEM: &'static str = "function_item";
static ATTRIBUTE_ITEM: &'static str = "attribute_item";
pub static BLOCK_ITEM: &'static str = "block";
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

pub fn check_function_is_test(
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

fn handle_binary_expression(
    child: tree_sitter::Node,
    file: &String,
    mutations: &mut Vec<Mutation>,
    function_name: &String,
) -> eyre::Result<()> {
    let binary_expr_data = &file[child.start_byte()..child.end_byte()];

    let mut binary_expr_cursor = child.walk();
    for component in child.children(&mut binary_expr_cursor) {
        if component.kind() == BINARY_EXPR_ITEM {
            handle_binary_expression(component, file, mutations, function_name)?;
        }

        if [MINUS_ITEM, PLUS_ITEM].contains(&component.kind()) {
            let operator_item = component;

            let binary_expr: syn::ExprBinary = syn::parse_str(binary_expr_data)?;
            let mutations_details = match binary_expr.op {
                syn::BinOp::Sub(..) => {
                    log::trace!(
                        "Binary - operation found at line {}",
                        operator_item.start_position().row + 1
                    );

                    vec![
                        ("+", "replace - by +"),
                        ("*", "replace - by *"),
                        ("&&", "replace - by &&"),
                    ]
                }
                syn::BinOp::Add(..) => {
                    log::trace!(
                        "--> Binary + operation found at line {}",
                        operator_item.start_position().row + 1
                    );
                    vec![("-", "replace + by -"), ("*", "replace + by *")]
                }
                _ => vec![],
            };
            for (mutation, reason) in mutations_details {
                mutations.push(
                    Mutation::new(mutation, operator_item)
                        .with_reason(reason)
                        .with_function_name(&function_name),
                )
            }
        }
    }

    Ok(())
}

fn handle_block(
    node_block: tree_sitter::Node,
    file: &String,
    mutations: &mut Vec<Mutation>,
    function_name: String,
) -> eyre::Result<()> {
    let mut cursor = node_block.walk();
    for child in node_block.children(&mut cursor) {
        if child.kind() == BINARY_EXPR_ITEM {
            handle_binary_expression(child, file, mutations, &function_name)?;
        }
    }
    Ok(())
}

/// Analyze a path
///
/// Detect Rust files
///
/// Generate in memory Mutations
pub(crate) fn analyze(root_path: &PathBuf) -> eyre::Result<Vec<Mutation>> {
    log::info!("Analyze project {}", dunce::simplified(root_path).display());
    let mut mutants = vec![];
    let walker = get_project_walker(&root_path)?;

    for entry in walker {
        if rust_source(&entry) {
            let path = entry.path();
            let mutated_files = get_mutations_for_file(path, &root_path)
                .wrap_err("Unable to get mutations for file")?;
            mutants.extend(mutated_files);
        }
    }

    Ok(mutants)
}

fn get_mutations_for_file(path: &Path, root_path: &PathBuf) -> eyre::Result<Vec<Mutation>> {
    let relative_path = path.strip_prefix(root_path)?;
    log::debug!("Handle file {}", relative_path.to_string_lossy());
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
                log::debug!("-> Handle function {}", item_fn.sig.ident);

                let mut cursor = tree.walk();
                for node in child_node.children(&mut cursor) {
                    if node.kind() == BLOCK_ITEM {
                        handle_block(
                            node,
                            &content,
                            &mut file_mutants,
                            item_fn.sig.ident.to_string(),
                        )?;
                    }
                }
            }
        }
    }

    for mutation in file_mutants.iter_mut() {
        mutation.set_file_path(&path.to_path_buf());
        mutation.mutate_file(&content);
    }

    Ok(file_mutants)
}
