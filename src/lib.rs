use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use clap::Parser;
use eyre::{eyre, Context};

use syn::spanned::Spanned;
use syn::{Attribute, ItemFn, Token};
use walkdir::WalkDir;

#[derive(Parser, Debug)]
struct Args {
    /// Path of the project to mutate
    #[arg(name = "PROJECT PATH")]
    root_path: OsString,
    /// Root path to mutated projects
    #[arg(long, default_value = "./tmp")]
    mutation_path: OsString,
}

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

fn syn_parse(path: &Path) -> eyre::Result<Vec<syn::File>> {
    //log::info!("Handle file {path:?}");
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
    project_path: &OsString,
    mutation_root: &OsString,
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
    let walker = WalkDir::new(&root_path);
    // Contient l'ensemble des variants de mutation par fichiers
    let mut mutated_files_per_project: HashMap<PathBuf, Vec<syn::File>> = HashMap::new();

    for entry in walker {
        let entry = entry?;
        if rust_source(&entry) {
            let path = entry.path();
            let mutated_files = syn_parse(path)?;
            mutated_files_per_project
                .insert(path.strip_prefix(&root_path)?.to_path_buf(), mutated_files);
        }
    }

    Ok(mutated_files_per_project)
}

pub fn run() -> eyre::Result<()> {
    let args = Args::parse();

    let mutants = list_sources(&PathBuf::from(&args.root_path))?;
    generate_mutants(mutants, &args.root_path, &args.mutation_path)?;

    Ok(())
}
