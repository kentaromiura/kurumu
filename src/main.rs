#![expect(clippy::print_stdout)]
use std::{fs, path::Path};

use oxc_allocator::{Allocator, FromIn};
use oxc_ast;
use oxc_ast::ast::*;
//use oxc_ast::utf8_to_utf16::Utf8ToUtf16;
use oxc_codegen::{Codegen, CodegenOptions};
use oxc_parser::ParserReturn;
use oxc_parser::{ParseOptions, Parser};
use oxc_resolver::{ResolveOptions, Resolver};
use oxc_span::SourceType;
//use oxc_transformer::Transformer;
use oxc_semantic::SemanticBuilder;
use oxc_traverse::{Traverse, TraverseCtx, traverse_mut};

use pico_args::Arguments;
use std::collections::HashMap;
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
        let name = to_process_next.remove(0);
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

        // println!("\n\nAST:");
        // println!("{}", &ret.program.to_pretty_json());

        if to_process_next.len() == 0 {
            break;
        }
    }

    let modules = format!("{{{}}}", modules_src.join(","));
    let source_text = format!(
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

    //let source_text = fs::read_to_string(path).map_err(|_| format!("Missing '{name}'"))?;
    let source_type = SourceType::from_path("out.js").unwrap();

    let mut ret = Parser::new(&allocator, &source_text, source_type)
        .with_options(ParseOptions {
            parse_regular_expression: true,
            ..ParseOptions::default()
        })
        .parse();
    let mut my_t = HtmlCssTransform {
        css_cache: HashMap::new(),
        collected_css: Vec::new(),
        collected_variables: Vec::new(),
        counter: 0,
    };

    let src_transformed = css_handler(&mut ret, &mut my_t, &allocator);
    println!("{}", src_transformed);
    Ok(())
}

struct HtmlCssTransform<'a> {
    css_cache: HashMap<String, u32>, // css content -> class id
    pub collected_css: Vec<String>,  // (class_id, css)
    counter: u32,
    pub collected_variables: Vec<(u32, Vec<Expression<'a>>, Vec<String>)>, // id -> (expressions, units)
}

impl<'a> HtmlCssTransform<'a> {
    fn extract_css<'b>(&mut self, allocator: &'a Allocator, call: &'b mut CallExpression<'a>) -> (String, Vec<Expression<'a>>, Vec<String>) {
        let mut strings = Vec::new();
        let mut substitutions = 0;

        if call.arguments.is_empty() {
            return (String::new(), vec![], vec![]);
        }

        if let Some(Argument::ArrayExpression(arr)) = call.arguments.get(0) {
            for el in &arr.elements {
                match el {
                    ArrayExpressionElement::StringLiteral(s) => {
                        strings.push(s.value.as_str().to_string());
                    }
                    ArrayExpressionElement::TemplateLiteral(t) => {
                        if let Some(quasi) = t.quasis.first() {
                            let value = quasi.value.raw.as_str().to_string();
                            strings.push(value);
                        }
                    }
                    _ => {}
                }
            }
        }

        let mut extracted_expressions = Vec::new();
        let unique_id = self.counter;
        self.counter += 1;

        if let Some(Argument::ArrayExpression(arr)) = call.arguments.get_mut(1) {
            substitutions = arr.elements.len();
            let elements = std::mem::replace(&mut arr.elements, oxc_allocator::Vec::new_in(allocator));
            for el in elements {
                match el {
                    ArrayExpressionElement::Identifier(id) => {
                        extracted_expressions.push(Expression::Identifier(id));
                    }
                    ArrayExpressionElement::TemplateLiteral(t) => {
                        extracted_expressions.push(Expression::TemplateLiteral(t));
                    }
                    ArrayExpressionElement::StringLiteral(s) => {
                        extracted_expressions.push(Expression::StringLiteral(s));
                    }
                    _ => {}
                }
            }
        }

        let mut css = String::new();
        let mut units = Vec::new();

        if !strings.is_empty() {
            css.push_str(&strings[0]);

            for i in 0..substitutions {
                css.push_str(&format!("var(--kentacss{}-{})", unique_id, i));
                let mut unit_str = String::new();
                if i + 1 < strings.len() {
                    let next_str = &strings[i + 1];
                    if let Some(semicolon_idx) = next_str.find(';') {
                        css.push_str(&next_str[semicolon_idx..]);
                        unit_str = next_str[..semicolon_idx].to_string();
                    } else {
                        unit_str = next_str.clone();
                    }
                }
                units.push(unit_str);
            }
        }

        (css, extracted_expressions, units)
    }

    fn get_or_create_id(&mut self, css: String) -> u32 {
        if let Some(id) = self.css_cache.get(&css) {
            return *id;
        } else {
            let id = self.counter;
            self.counter += 1;
            let transformed = emotionless::next(id, css);
            self.css_cache.insert(transformed.clone(), id);
            self.collected_css.push(transformed.clone());
            id
        }
    }

    fn is_html_css_call(&mut self, call: &mut CallExpression) -> bool {
        if call.callee.is_member_expression() {
            if let Some(me) = call.callee.get_member_expr() {
                if me.is_specific_member_access("Html", "css") {
                    return true;
                }
                return false;
            } else {
                return false;
            }
        } else {
            return false;
        }
    }
}
struct TraverseState {}
impl<'a> Traverse<'a, TraverseState> for HtmlCssTransform<'a> {
    fn exit_expression(
        &mut self,
        expr: &mut Expression<'a>,
        ctx: &mut TraverseCtx<'a, TraverseState>,
    ) {
        let is_target = if let Expression::CallExpression(call) = expr {
            self.is_html_css_call(call)
        } else {
            false
        };

        if is_target {
            //let _ = dbg!(&expr);
            if let Expression::CallExpression(call) = expr {
                //let _ = dbg!(expr.parent());
                //println!("Gotcha on exit");
                let span = call.span;
                let (css, extracted, units) = self.extract_css(ctx.ast.allocator, call);
                let id = self.get_or_create_id(css);
                if !extracted.is_empty() {
                    self.collected_variables.push((id, extracted, units));
                }
                *expr = ctx.ast.expression_string_literal(
                    span,
                    ctx.ast.atom(&id.to_string()),
                    None,
                );
            }
        }
    }

    fn enter_call_expression(
        &mut self,
        _node: &mut CallExpression<'a>,
        _ctx: &mut TraverseCtx<'a, TraverseState>,
    ) {
        // if node.callee.is_member_expression() {
        //     if let Some(me) = node.callee.get_member_expr() {
        //         if me.is_specific_member_access("Html", "css") {
        //             let _ = dbg!(&me);
        //             let _ = dbg!(_ctx.parent());
        //             println!("Gotcha")
        //         }
        //     }
        // }
        //
        //if(node.callee.type === 'MemberExpression' && path.node.callee.property.name === 'css' && path.node.callee.object.name == 'Html')

        // // Read parent
        // if let Ancestor::BinaryExpressionRight(bin_expr_ref) = ctx.parent() {
        //     // This is legal
        //     if let Expression::Identifier(id) = bin_expr_ref.left() {
        //         println!("left side is ID: {}", &id.name);
        //     }

        //     // This would be a compile failure, because the right side is where we came from
        //     // dbg!(bin_expr_ref.right());
        // }

        // // Read grandparent
        // if let Ancestor::ExpressionStatementExpression(stmt_ref) = ctx.ancestor(1) {
        //     // This is legal
        //     println!("expression stmt's span: {:?}", stmt_ref.span());

        //     // This would be a compile failure, because the expression is where we came from
        //     // dbg!(stmt_ref.expression());
        // }
    }
}

fn css_handler<'a>(
    pr: &mut ParserReturn<'a>,
    my_t: &mut HtmlCssTransform<'a>,
    allocator: &'a Allocator,
) -> String {
    // let ret = Transformer::new(&allocator, path, &transform_options)
    //        .build_with_scoping(scoping, &mut program);

    //let allocator = Allocator::default();
    //Transformer::new(allocator, source_path, options)
    // let ret = Transformer::new(&allocator, path, &transform_options)
    //     .build_with_scoping(scoping, &mut program);
    let scoping = SemanticBuilder::new()
        .build(&pr.program)
        .semantic
        .into_scoping();
    let state = TraverseState {};
    //let _scoping =
    let __ret = traverse_mut(my_t, allocator, &mut pr.program, scoping, state);

    codegen(&pr, true)
    //"".to_string()
}
//https://github.com/oxc-project/oxc/blob/main/crates/oxc_codegen/examples/codegen.rs
fn codegen(ret: &ParserReturn<'_>, minify: bool) -> String {
    //ret.program.with_mut(||)
    Codegen::new()
        .with_options(CodegenOptions {
            minify,
            ..CodegenOptions::default()
        })
        .build(&ret.program)
        .code
}
