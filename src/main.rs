#![expect(clippy::print_stdout)]
use std::hash::Hash;
//use std::any::Any;
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
use oxc_semantic::{Scoping, SemanticBuilder};
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
        counter: 0,
    };

    let src_transformed = css_handler(&mut ret, &mut my_t, &allocator);
    println!("{}", src_transformed);
    Ok(())
}

struct HtmlCssTransform {
    css_cache: HashMap<String, u32>, // css content -> class id
    pub collected_css: Vec<String>,  // (class_id, css)
    counter: u32,
}

impl HtmlCssTransform {
    fn extract_css(&mut self, call: &mut CallExpression) -> String {
        let _ = dbg!(&call);
        /*
        var pink = Html.css([
              "\n&:hover {\n    color: ",
              ";\n}"
            ], [pinkColor]);
            ---
            arguments: Vec(
                    [
                        ArrayExpression(
                            ArrayExpression {
                                span: Span {
                                    start: 611,
                                    end: 643,
                                },
                                elements: Vec(
                                    [
                                        TemplateLiteral(
                                            TemplateLiteral {
                                                span: Span {
                                                    start: 612,
                                                    end: 636,
                                                },
                                                quasis: Vec(
                                                    [
                                                        TemplateElement {
                                                            span: Span {
                                                                start: 613,
                                                                end: 635,
                                                            },
                                                            value: TemplateElementValue {
                                                                raw: "\n&:hover {\n    color: ",
                                                                cooked: Some(
                                                                    "\n&:hover {\n    color: ",
                                                                ),
                                                            },
                                                            tail: true,
                                                        },
                                                    ],
                                                ),
                                                expressions: Vec(
                                                    [],
                                                ),
                                            },
                                        ),
                                        TemplateLiteral(
                                            TemplateLiteral {
                                                span: Span {
                                                    start: 637,
                                                    end: 642,
                                                },
                                                quasis: Vec(
                                                    [
                                                        TemplateElement {
                                                            span: Span {
                                                                start: 638,
                                                                end: 641,
                                                            },
                                                            value: TemplateElementValue {
                                                                raw: ";\n}",
                                                                cooked: Some(
                                                                    ";\n}",
                                                                ),
                                                            },
                                                            tail: true,
                                                        },
                                                    ],
                                                ),
                                                expressions: Vec(
                                                    [],
                                                ),
                                            },
                                        ),
                                    ],
                                ),
                                trailing_comma: None,
                            },
                        ),
                        ArrayExpression(
                            ArrayExpression {
                                span: Span {
                                    start: 644,
                                    end: 655,
                                },
                                elements: Vec(
                                    [
                                        Identifier(
                                            IdentifierReference {
                                                span: Span {
                                                    start: 645,
                                                    end: 654,
                                                },
                                                name: "pinkColor",
                                                reference_id: Cell {
                                                    value: Some(
                                                        ReferenceId(
                                                            19,
                                                        ),
                                                    ),
                                                },
                                            },
                                        ),
                                    ],
                                ),
                                trailing_comma: None,
                            },
                        ),
                    ],
         */
        String::from("")
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
impl<'a> Traverse<'a, TraverseState> for HtmlCssTransform {
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
                let css = self.extract_css(call);
                let id = self.get_or_create_id(css);
                *expr = ctx.ast.expression_string_literal(
                    call.span,
                    ctx.ast.atom(&id.to_string()),
                    None,
                );
            }
        }
    }

    fn enter_call_expression(
        &mut self,
        node: &mut CallExpression<'a>,
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
    my_t: &mut HtmlCssTransform,
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
