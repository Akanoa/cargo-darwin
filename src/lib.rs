use clap::Parser;
use cli::{Cli, Darwin};
use eyre::{eyre, Context};
use mutation::Mutation;
use normpath::PathExt;
use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use syn::spanned::Spanned;
use syn::{Attribute, ItemFn, Token};
use walkdir::WalkDir;

mod cli;
mod mutation;

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

fn handle_bin_op(expression: &syn::ExprBinary) -> eyre::Result<Vec<syn::ExprBinary>> {
    let mut mutated_binary_expressions = vec![];
    let mut cloned_expr = expression.clone();
    let span = cloned_expr.span();
    match expression.op {
        syn::BinOp::Add(..) => {
            log::trace!("Binary + operation found");
            cloned_expr.op = syn::BinOp::Sub(Token![-](span));
            mutated_binary_expressions.push(cloned_expr.clone());
            cloned_expr.op = syn::BinOp::Div(Token![/](span));
            mutated_binary_expressions.push(cloned_expr);
        }
        syn::BinOp::Sub(..) => {
            log::trace!("Binary - operation found");
            cloned_expr.op = syn::BinOp::Mul(Token![*](span));
            mutated_binary_expressions.push(cloned_expr.clone());
            cloned_expr.op = syn::BinOp::And(Token![&&](span));
            mutated_binary_expressions.push(cloned_expr);
        }
        _ => {}
    }

    Ok(mutated_binary_expressions)
}

fn handle_mutable_function(function: &syn::ItemFn) -> eyre::Result<Vec<ItemFn>> {
    let statements = &function.block.stmts;
    let mut mutated_functions = vec![];

    let mut mutated_statement_by_index: HashMap<usize, Vec<syn::Stmt>> = HashMap::new();

    for (index, statement) in statements.iter().enumerate() {
        let mut mutated_statements: Vec<syn::Stmt> = vec![];
        match statement {
            syn::Stmt::Expr(syn::Expr::Binary(expression), semi) => {
                for mutated_binary_expr in handle_bin_op(expression)? {
                    let mutated_statement =
                        syn::Stmt::Expr(syn::Expr::Binary(mutated_binary_expr), semi.clone());
                    mutated_statements.push(mutated_statement);
                }
            }
            _ => {}
        }
        mutated_statement_by_index.insert(index, mutated_statements);
    }

    for (index, statements) in mutated_statement_by_index {
        for statement in statements {
            let mut function_clone = function.clone();
            let _ = std::mem::replace(&mut function_clone.block.stmts[index], statement);
            mutated_functions.push(function_clone);
        }
    }

    Ok(mutated_functions)
}

fn handle_function_item(function: &syn::ItemFn) -> eyre::Result<Vec<ItemFn>> {
    if !is_test_function(&function.attrs)? {
        log::debug!("Handle function {}", function.sig.ident);
        return handle_mutable_function(function);
    }

    Ok(vec![])
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

fn syn_parse(path: &Path) -> eyre::Result<Vec<syn::File>> {
    log::info!("Handle file {path:?}");
    let mut source_file = File::open(path)?;
    let mut content = String::new();
    source_file.read_to_string(&mut content)?;
    let ast = syn::parse_file(&content)?;

    let mut mutants: Vec<syn::File> = vec![];
    let mut mutated_item_by_index: HashMap<usize, Vec<syn::Item>> = HashMap::new();

    for (index, item) in ast.items.iter().enumerate() {
        match item {
            syn::Item::Fn(function_item) => {
                let mutated_functions: Vec<syn::Item> = handle_function_item(function_item)?
                    .iter()
                    .map(|item_fn| syn::Item::Fn(item_fn.clone()))
                    .collect();
                mutated_item_by_index.insert(index, mutated_functions);
            }
            _ => {}
        }
    }

    for (index, mutated_items) in mutated_item_by_index {
        for mutated_item in mutated_items {
            let mut ast_clone = ast.clone();
            let _ = std::mem::replace(&mut ast_clone.items[index], mutated_item);
            mutants.push(ast_clone);
        }
    }

    Ok(mutants)
}

fn copy_project_with_mutation(
    entries: &Vec<globwalk::DirEntry>,
    project_path: &PathBuf,
    mutation_root: &PathBuf,
    mutant_file: &syn::File,
    mutant_file_path: &PathBuf,
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

    let mutant_content = prettyplease::unparse(&mutant_file);

    let mutant_file_path = std::fs::canonicalize(mutation_root.join(mutant_file_path))
        .wrap_err(eyre!("Unable to canonicalize path {mutant_file_path:?}"))?;
    let mut file_to_mutate = File::create(&mutant_file_path)
        .wrap_err(eyre!("Unable to open file {mutant_file_path:?}"))?;
    file_to_mutate
        .write_all(mutant_content.as_bytes())
        .wrap_err(eyre!("Unable to write file {mutant_file_path:?}"))?;
    file_to_mutate.flush()?;

    Ok(())
}

fn generate_mutants(
    mutants: HashMap<PathBuf, Vec<syn::File>>,
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
    for (mutant_file, mutations) in mutants {
        let mutant_file_path = PathBuf::from(mutant_file);
        for mutation in mutations {
            copy_project_with_mutation(
                &walker,
                &project_path,
                &mutation_root.join(format!("{mutation_id}")),
                &mutation,
                &mutant_file_path,
            )?;
            mutation_id += 1;
        }
    }

    Ok(())
}

fn list_sources(root_path: &PathBuf) -> eyre::Result<HashMap<PathBuf, Vec<syn::File>>> {
    dbg!(root_path);
    let root_path = root_path.normalize().wrap_err("x")?.as_path().to_path_buf();
    let walker = WalkDir::new(&root_path);
    // Contient l'ensemble des variants de mutation par fichiers
    let mut mutated_files_per_project: HashMap<PathBuf, Vec<syn::File>> = HashMap::new();
    let mut mutants = vec![];

    for entry in walker {
        let entry = entry.wrap_err("Unable to found entry")?;
        if rust_source(&entry) {
            let path = entry.path();
            let mutated_files = tree_sitter_parse(path, &root_path)?;
            mutants.extend(mutated_files);
            // mutated_files_per_project
            //     .insert(path.strip_prefix(&root_path)?.to_path_buf(), mutated_files);
        }
    }

    for mutant in mutants {
        println!("{}", mutant.display().unwrap())
    }

    Ok(mutated_files_per_project)
}

pub fn run() -> eyre::Result<()> {
    let cli = Cli::parse();

    let Cli::Darwin(Darwin {
        mutation_path,
        root_path,
    }) = cli;

    let mutants = list_sources(&PathBuf::from(&root_path))?;
    generate_mutants(mutants, &root_path, &mutation_path)?;

    Ok(())
}
