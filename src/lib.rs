#[macro_use]
extern crate syntex_syntax;
extern crate syntex;
extern crate uuid;
extern crate gcc;

use std::cell::RefCell;
use std::rc::Rc;
use std::fs::File;
use std::io::prelude::*;
use std::env;
use std::path::Path;
use std::iter::FromIterator;

use syntex::Registry;
use syntex_syntax::ast;
use syntex_syntax::ext::base::{MacResult, ExtCtxt, DummyResult, MacEager, TTMacroExpander};
use syntex_syntax::util::small_vector::SmallVector;
use syntex_syntax::codemap::{Span, FileLines};
use syntex_syntax::parse::token;
use syntex_syntax::abi::Abi;
use syntex_syntax::ext::build::AstBuilder;

use uuid::Uuid;

/// Expand the cpp! macros in file src, placing the resulting rust file in dst.
///
/// C++ code will be generated and built based on the information parsed from
/// these macros. Additional configuration on the C++ build can be performed in
/// the configure function.
pub fn build<F>(src: &Path, dst: &Path, name: &str, configure: F)
    where F: for <'a> FnOnce(&'a mut gcc::Config)
{
    let mut registry = Registry::new();
    let generator = register(&mut registry);
    registry.expand(name, src, dst).unwrap();
    generator.build(name, configure);
}

/// Register rust-cpp's macros onto a given syntex Registry.
///
/// These macros perform rust code generation, and data collection. The data
/// which is collected by the syntax macros will be stored within the returned
/// cpp::CodeGen struct, which can be used to then generate and build the
/// generated C++ code.
pub fn register(reg: &mut Registry) -> CodeGen {
    let state: Rc<RefCell<State>> = Default::default();

    reg.add_macro("cpp_include", CppInclude(state.clone()));
    reg.add_macro("cpp_header", CppHeader(state.clone()));
    reg.add_macro("cpp", Cpp(state.clone()));

    CodeGen{ state: state }
}

#[must_use]
#[derive(Debug)]
pub struct CodeGen {
    state: Rc<RefCell<State>>,
}

impl CodeGen {
    /// Build and link the C++ code. The configure function is passed a
    /// gcc::Config object, which can be configured before the module is built,
    /// such that additional options can be easily passed.
    pub fn build<F>(self, name: &str, configure: F)
        where F: for <'a> FnOnce(&'a mut gcc::Config)
    {
        let file = Path::new(&env::var("OUT_DIR").unwrap())
            .join(&format!("{}.cpp", name));
        self.codegen(&file);

        let mut config = gcc::Config::new();
        config.cpp(true).file(file);
        configure(&mut config);
        config.compile(&format!("lib{}.a", name));
    }

    /// Generate the C++ code, without building it. The code will be output
    /// to the file located at `file`
    pub fn codegen(self, file: &Path) {
        let state = self.state.borrow();
        let code = String::from_iter([
            "// This is machine generated code, created by rust-cpp\n",
            &state.includes,
            &state.headers,
            "extern \"C\" {\n",
            &state.fndecls,
            "}",
        ].iter().cloned());

        // Write out the file
        let mut f = File::create(file).unwrap();
        f.write_all(code.as_bytes()).unwrap();
    }
}

#[derive(Debug, Default)]
struct State {
    includes: String,
    headers: String,
    fndecls: String,
}

// Macro expander implementations

struct CppInclude(Rc<RefCell<State>>);
impl TTMacroExpander for CppInclude {
    fn expand<'cx>(&self,
                   ec: &'cx mut ExtCtxt,
                   mac_span: Span,
                   tts: &[ast::TokenTree])
                   -> Box<MacResult+'cx>
    {
        let mut st = self.0.borrow_mut();

        if tts.len() == 0 {
            ec.span_err(mac_span, "unexpected empty cpp_include!");
            return DummyResult::any(mac_span);
        }

        // Get the span of the tokens passed to the macro
        let span = Span {
            lo: tts.first().unwrap().get_span().lo,
            hi: tts.last().unwrap().get_span().hi,
            expn_id: mac_span.expn_id,
        };

        // Add the text from that span as an include to the headers
        let inner = ec.parse_sess.codemap().span_to_snippet(span).unwrap();
        st.includes.push_str("\n#include ");
        st.includes.push_str(&inner);
        st.includes.push_str("\n");

        MacEager::items(SmallVector::zero())
    }
}

struct CppHeader(Rc<RefCell<State>>);
impl TTMacroExpander for CppHeader {
    fn expand<'cx>(&self,
                   ec: &'cx mut ExtCtxt,
                   mac_span: Span,
                   tts: &[ast::TokenTree])
                   -> Box<MacResult+'cx>
    {
        let mut st = self.0.borrow_mut();

        if tts.len() == 0 {
            ec.span_err(mac_span, "unexpected empty cpp_header!");
            return DummyResult::any(mac_span);
        }

        let span = Span {
            lo: tts.first().unwrap().get_span().lo,
            hi: tts.last().unwrap().get_span().hi,
            expn_id: mac_span.expn_id,
        };

        let inner = ec.parse_sess.codemap().span_to_snippet(span).unwrap();
        st.headers.push_str("\n");
        st.headers.push_str(&inner);
        st.headers.push_str("\n");

        MacEager::items(SmallVector::zero())
    }
}

struct Cpp(Rc<RefCell<State>>);
impl TTMacroExpander for Cpp {
    fn expand<'cx>(&self,
                   ec: &'cx mut ExtCtxt,
                   mac_span: Span,
                   tts: &[ast::TokenTree])
                   -> Box<MacResult+'cx>
    {
        let mut st = self.0.borrow_mut();
        let mut parser = ec.new_parser_from_tts(tts);
        let mut captured_idents = Vec::new();

        // Parse the identifier list
        match parser.parse_token_tree().ok() {
            Some(ast::TokenTree::Delimited(span, ref del)) => {
                let mut parser = ec.new_parser_from_tts(&del.tts[..]);
                loop {
                    if parser.check(&token::Eof) {
                        break;
                    }

                    let mutable = parser.parse_mutability().unwrap_or(ast::Mutability::Immutable);
                    let ident = parser.parse_ident().unwrap();
                    let cxxty = (*parser.parse_str().unwrap().0).to_owned();
                    captured_idents.push((ident, mutable, cxxty));

                    if !parser.eat(&token::Comma) {
                        break;
                    }
                }
                if !parser.check(&token::Eof) {
                    ec.span_err(span, "Unexpected token in captured identifier list");
                    return DummyResult::expr(span);
                }
            }
            Some(ref tt) => {
                ec.span_err(tt.get_span(),
                            "First argument to cpp! must be a list of captured identifiers");
                return DummyResult::expr(tt.get_span());
            }
            None => {
                ec.span_err(mac_span, "Unexpected empty cpp! macro invocation");
                return DummyResult::expr(mac_span);
            }
        }

        // Check if we are looking at an ->
        let (ret_ty, ret_cxxty) = if parser.eat(&token::RArrow) {
            if let Ok(ty) = parser.parse_ty() {
                let cxxty = match parser.parse_str() {
                    Ok((string, _)) => string,
                    Err(_) => {
                        ec.span_err(mac_span, "ERROR");
                        return DummyResult::expr(mac_span);
                    }
                };
                (ty, (*cxxty).to_owned())
            } else {
                ec.span_err(mac_span, "Unexpected error while parsing type");
                return DummyResult::expr(mac_span);
            }
        } else {
            (ec.ty(mac_span, ast::TyKind::Tup(Vec::new())), "void".to_owned())
        };

        // Read in the body
        let body_tt = parser.parse_token_tree().unwrap();
        parser.expect(&token::Eof).unwrap();

        // Extract the string body of the c++ code
        let body_str = match body_tt {
            ast::TokenTree::Delimited(span, ref del) => {
                if del.open_token() != token::OpenDelim(token::Brace) {
                    ec.span_err(span, "cpp! body must be surrounded by `{}`");
                    return DummyResult::expr(span);
                }

                ec.parse_sess.codemap().span_to_snippet(span).unwrap()
            }
            _ => {
                ec.span_err(mac_span, "cpp! body must be a block surrounded by `{}`");
                return DummyResult::expr(body_tt.get_span());
            }
        };

        // Generate the rust parameters and arguments
        let params: Vec<_> = captured_idents.iter()
            .map(|&(ref id, mutable, _)| {
                let arg_ty = ec.ty_ptr(mac_span,
                                       ec.ty_ident(mac_span,
                                                   ast::Ident::with_empty_ctxt(
                                                       token::intern("u8"))),
                                       mutable);
                ec.arg(mac_span, id.clone(), arg_ty)
            })
            .collect();

        let args: Vec<_> = captured_idents.iter()
            .map(|&(ref id, mutable, _)| {
                let arg_ty = ec.ty_ptr(mac_span,
                                       ec.ty_ident(mac_span,
                                                   ast::Ident::with_empty_ctxt(
                                                       token::intern("u8"))),
                                       mutable);

                let addr_of = if mutable == ast::Mutability::Immutable {
                    ec.expr_addr_of(mac_span,
                                    ec.expr_ident(mac_span, id.clone()))
                } else {
                    ec.expr_mut_addr_of(mac_span,
                                        ec.expr_ident(mac_span,
                                                      id.clone()))
                };

                ec.expr_cast(mac_span,
                             ec.expr_cast(mac_span,
                                          addr_of,
                                          ec.ty_ptr(mac_span,
                                                    ec.ty_infer(mac_span),
                                                    mutable)),
                             arg_ty)
            })
            .collect();


        fn escape_ident(s: &str) -> String {
            const VALID_CHARS: &'static str =
                "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ_123456789";
            let mut out = String::new();
            for c in s.chars() {
                if VALID_CHARS.contains(c) {
                    out.push(c);
                } else {
                    out.push('_')
                }
            }
            out
        }

        let locinfo = match ec.parse_sess.codemap().span_to_lines(mac_span) {
            Ok(FileLines{ref file, ref lines}) if !lines.is_empty() =>
                format!("_{}__l{}__",
                        escape_ident(&file.name),
                        lines[0].line_index + 1),
            _ => String::new(),
        };

        let fn_name = format!("_generated_{}{}",
                              locinfo,
                              Uuid::new_v4().simple().to_string());
        let fn_ident = ast::Ident::with_empty_ctxt(token::intern(&fn_name));

        // extern "C" declaration of function
        let foreign_mod = ast::ForeignMod {
            abi: Abi::C,
            items: vec![ast::ForeignItem {
                ident: fn_ident.clone(),
                attrs: Vec::new(),
                node: ast::ForeignItemKind::Fn(ec.fn_decl(params, ret_ty),
                                               ast::Generics::default()),
                id: ast::DUMMY_NODE_ID,
                span: mac_span,
                vis: ast::Visibility::Inherited,
            }],
        };

        let exp = ec.expr_block(// Block
            ec.block(mac_span,
                     // Extern "C" declarations for the c implemented functions
                     vec![ec.stmt_item(mac_span,
                                       ec.item(mac_span,
                                               fn_ident.clone(),
                                               Vec::new(),
                                               ast::ItemKind::ForeignMod(foreign_mod)))],
                     Some(ec.expr_call_ident(mac_span, fn_ident.clone(), args))));

        // Generate the C++ code for the function declaration
        let cpp_params = captured_idents.into_iter()
            .fold(String::new(), |mut acc, (id, mutable, ty)| {
                if !acc.is_empty() {
                    acc.push_str(", ");
                }
                if mutable == ast::Mutability::Immutable {
                    acc.push_str("const ");
                }
                if ty == "void" {
                    acc.push_str("void*");
                } else {
                    acc.push_str(&ty);
                    acc.push_str("& ");
                }
                acc.push_str(&id.name.as_str());
                acc
            });

        let line_pragma = match ec.parse_sess.codemap().span_to_lines(mac_span) {
            Ok(FileLines{ref file, ref lines}) if !lines.is_empty() =>
                format!("#line {} {:?}", lines[0].line_index + 1, file.name),
            _ => String::new(),
        };

        let cpp_decl = format!("\n{}\n{} {}({}) {}\n",
                               line_pragma, ret_cxxty, fn_name,
                               cpp_params, body_str);
        st.fndecls.push_str(&cpp_decl);

        // Emit the rust code into the AST
        MacEager::expr(exp)
    }
}

