#![expect(clippy::print_stdout)]
use std::{fs, path::Path};

use oxc_allocator::{Allocator, FromIn};
use oxc_ast;
use oxc_ast::ast::*;
//use oxc_ast::utf8_to_utf16::Utf8ToUtf16;
use oxc_codegen::{CodeGenerator, CodegenOptions};

use oxc_parser::ParserReturn;
use oxc_parser::{ParseOptions, Parser};
use oxc_span::SourceType;
use pico_args::Arguments;
use std::collections::HashMap;

use oxc_resolver::{ResolveOptions, Resolver};
// https://stackoverflow.com/a/50278316
fn format_radix(mut x: u32, radix: u32) -> String {
    let mut result = vec![];

    loop {
        let m = x % radix;
        x = x / radix;

        // will panic if you use a bad radix (< 2 or > 36).
        result.push(std::char::from_digit(m, radix).unwrap());
        if x == 0 {
            break;
        }
    }
    result.into_iter().rev().collect()
}

fn main() -> Result<(), String> {
    let mut args = Arguments::from_env();
    let mut modules = HashMap::new();
    let mut id: u32 = 0;
    let allocator = Allocator::default();

    let mut modules_src: Vec<String> = Vec::new();

    let options = ResolveOptions {
        alias_fields: vec![vec!["browser".into()]],
        alias: vec![],
        extensions: vec![".js".into()],
        extension_alias: vec![(".js".into(), vec![".ts".into(), ".js".into()])],
        // ESM
        //condition_names: vec!["node".into(), "import".into()],
        // CJS
        condition_names: vec!["node".into(), "require".into()],
        ..ResolveOptions::default()
    };
    let resolver = Resolver::new(options);

    let mut current_path = std::env::current_dir().unwrap();

    let mut to_process_next: Vec<String> = Vec::new();
    let command = args.subcommand().expect("entry file missing.");

    let entry_file = command.as_deref().unwrap_or("default");
    if entry_file == "default" {
        println!("Missing entry point .js");
        std::process::exit(-1);
    }
    to_process_next.push(entry_file.to_string());
    loop {
        // while to_process_next.len() > 0
        let name = to_process_next.pop().unwrap();
        let (full_path, ncp) = match resolver.resolve(&current_path, &name) {
            Err(error) => {
                println!("Error: {error}");
                ("".to_string(), Path::new("").to_path_buf())
            }
            Ok(resolution) => (
                resolution.full_path().to_str().unwrap().to_string(),
                resolution.path().to_path_buf(),
            ),
        };
        current_path = ncp.parent().unwrap().to_path_buf();
        let path = full_path.clone().to_string();
        if !modules.contains_key(&path) {
            modules.insert(path.clone(), id);
            id = id + 1;
        }

        let path = Path::new(&name);
        let source_text = fs::read_to_string(path).map_err(|_| format!("Missing '{name}'"))?;
        let source_type = SourceType::from_path(path).unwrap();

        let mut ret = Parser::new(&allocator, &source_text, source_type)
            .with_options(ParseOptions {
                parse_regular_expression: true,
                ..ParseOptions::default()
            })
            .parse();
        let program = &mut ret.program;
        let _new_body = program
            .body
            .iter_mut()
            .map(|x| {
                if Statement::is_declaration(x) {
                    match x.as_declaration_mut() {
                        Some(Declaration::VariableDeclaration(var)) => {
                            for declarator in &mut var.declarations {
                                if let Some(init) = &mut declarator.init {
                                    if init.is_require_call() {
                                        if let Expression::CallExpression(call_expr) = init {
                                            let args = &mut call_expr.arguments;
                                            let mut required = args.pop();
                                            if let Some(Argument::StringLiteral(a)) = &mut required
                                            {
                                                let resolved = match resolver
                                                    .resolve(&current_path, a.value.as_str())
                                                {
                                                    Err(error) => {
                                                        println!("Error: {error}");
                                                        "".to_string()
                                                    }
                                                    Ok(resolution) => {
                                                        let full_path = resolution.full_path();
                                                        full_path
                                                            .clone()
                                                            .to_str()
                                                            .unwrap()
                                                            .to_string()
                                                    }
                                                };

                                                if !modules.contains_key(&resolved) {
                                                    modules.insert(resolved.clone(), id);
                                                    to_process_next.push(resolved.clone());
                                                    id = id + 1;
                                                }

                                                let id = modules.get(&resolved).unwrap().clone();
                                                let new_path = format_radix(id, 32);

                                                a.value =
                                                    Atom::from_in(new_path.clone(), &allocator);
                                                a.raw = Some(Atom::from_in(new_path, &allocator));
                                                args.push(required.unwrap());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        _ => (),
                    }
                }
                if let Statement::ExpressionStatement(es) = x {
                    let es_mut = es as &mut ExpressionStatement;

                    let e = &mut es_mut.expression;
                    if e.is_require_call() {
                        if let Expression::CallExpression(call_expr) = e {
                            let args = &mut call_expr.arguments;
                            let mut required = args.pop();
                            if let Some(Argument::StringLiteral(a)) = &mut required {
                                let resolved =
                                    match resolver.resolve(&current_path, a.value.as_str()) {
                                        Err(error) => {
                                            println!("Error: {error}");
                                            "".to_string()
                                        }
                                        Ok(resolution) => {
                                            let full_path = resolution.full_path();
                                            full_path.clone().to_str().unwrap().to_string()
                                        }
                                    };

                                if !modules.contains_key(&resolved) {
                                    modules.insert(resolved.clone(), id);
                                    to_process_next.push(resolved.clone());
                                    id = id + 1;
                                }

                                let id = modules.get(&resolved).unwrap().clone();
                                let new_path = format_radix(id, 32);

                                a.value = Atom::from_in(new_path.clone(), &allocator);
                                a.raw = Some(Atom::from_in(new_path, &allocator));
                                args.push(required.unwrap());
                            }
                        }
                    }
                }
                x
            })
            .collect::<Vec<&mut Statement>>();

        modules_src.push(format!(
            "{}:function (require, module, exports, global) {{{}}}",
            format_radix(modules_src.len() as u32, 32),
            codegen(&ret, true)
        ));

        //println!("\n\nAST:");
        //println!("{}", &ret.program.to_pretty_json());

        if to_process_next.len() == 0 {
            break;
        }
    }

    let modules = format!("{{{}}}", modules_src.join(","));
    println!(
        "(function (modules, global) {{
        var cache = {{}}, require = function (id) {{
                var module = cache[id];
                if (!module) {{
                    module = cache[id] = {{}};
                    var exports = module.exports = {{}};
                    modules[id].call(exports, require, module, exports, global);
                }}
                return module.exports;
            }};
        require('0');
    }}({}, this));",
        modules
    );

    Ok(())
}

//https://github.com/oxc-project/oxc/blob/main/crates/oxc_codegen/examples/codegen.rs
fn codegen(ret: &ParserReturn<'_>, minify: bool) -> String {
    //ret.program.with_mut(||)
    CodeGenerator::new()
        .with_options(CodegenOptions {
            minify,
            ..CodegenOptions::default()
        })
        .build(&ret.program)
        .code
}
