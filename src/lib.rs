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
use syntex_syntax::ext::base::{
    MacResult,
    ExtCtxt,
    DummyResult,
    MacEager,
    TTMacroExpander,
    IdentMacroExpander
};
use syntex_syntax::util::small_vector::SmallVector;
use syntex_syntax::codemap::{Span, FileLines};
use syntex_syntax::parse::token;
use syntex_syntax::abi::Abi;
use syntex_syntax::ext::build::AstBuilder;
use syntex_syntax::ptr::P;

use uuid::Uuid;

const RS_NAMESPACE: &'static str = r#"
#include <cstdint>

namespace rs {
  template<typename T>
  struct Slice {
    T* data;
    uintptr_t len;
  };

  struct Trait {
    void* data;
    void* vtable;
  };

  typedef int8_t i8;
  static_assert(sizeof(i8) == 1, "int is the right size");
  typedef int16_t i16;
  static_assert(sizeof(i16) == 2, "int is the right size");
  typedef int32_t i32;
  static_assert(sizeof(i32) == 4, "int is the right size");
  typedef int64_t i64;
  static_assert(sizeof(i64) == 8, "int is the right size");
  typedef intptr_t isize;

  typedef uint8_t u8;
  static_assert(sizeof(u8) == 1, "int is the right size");
  typedef uint16_t u16;
  static_assert(sizeof(u16) == 2, "int is the right size");
  typedef uint32_t u32;
  static_assert(sizeof(u32) == 4, "int is the right size");
  typedef uint64_t u64;
  static_assert(sizeof(u64) == 8, "int is the right size");
  typedef uintptr_t usize;

  typedef float f32;
  static_assert(sizeof(f32) == 4, "float is the right size");
  typedef double f64;
  static_assert(sizeof(f64) == 8, "float is the right size");

  typedef u8 bool_;
  static_assert(sizeof(bool_) == 1, "booleans are the right size");

  typedef uint32_t char_;
  static_assert(sizeof(char_) == 4, "char is the right size");

  typedef Slice<u8> str;
}
"#;

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
    reg.add_ident_macro("cpp_fn", CppFn(state.clone()));

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
            RS_NAMESPACE,
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

fn inner_text<'cx>(ec: &'cx mut ExtCtxt, tts: &[ast::TokenTree]) -> String {
    if tts.len() == 0 {
        return String::new();
    }

    let span = Span {
        lo: tts.first().unwrap().get_span().lo,
        hi: tts.last().unwrap().get_span().hi,
        expn_id: tts.first().unwrap().get_span().expn_id,
    };

    ec.parse_sess.codemap().span_to_snippet(span).unwrap_or(String::new())
}

fn get_tt_braced_text<'cx>(ec: &'cx mut ExtCtxt, tt: ast::TokenTree) -> Option<String> {
    match tt {
        ast::TokenTree::Delimited(span, ref del) =>
            if del.open_token() != token::OpenDelim(token::Brace) {
                None
            } else {
                ec.parse_sess.codemap().span_to_snippet(span).ok()
            },
        _ => None,
    }
}

fn str_to_ident(s: &str) -> ast::Ident {
    ast::Ident::with_empty_ctxt(token::intern(s))
}

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

struct CppParam {
    rs: P<ast::Ty>,
    cpp: String,
    name: String,
}

struct CppFunc {
    ident: ast::Ident,
    rs: ast::ForeignMod,
    cpp: String,
}

fn make_shared_function<'cx>(ec: &'cx mut ExtCtxt,
                             name_hint: &str,
                             span: Span,
                             body: String,
                             params: &[CppParam],
                             cpp_ret: String,
                             rs_ret: P<ast::Ty>) -> CppFunc {
    // Generate a unique name for the extern C function. This name shoukd
    // not conflict with anything
    let locinfo = match ec.parse_sess.codemap().span_to_lines(span) {
        Ok(FileLines{ref file, ref lines}) if !lines.is_empty() =>
            format!("_{}__l{}__",
                    escape_ident(&file.name),
                    lines[0].line_index + 1),
        _ => String::new(),
    };
    let fn_name = format!("_{}_{}{}", name_hint, locinfo,
                          Uuid::new_v4().simple().to_string());
    let fn_ident = ast::Ident::with_empty_ctxt(token::intern(&fn_name));

    // Create the ast::ForeignMod for the rust side
    let rs_params: Vec<_> = params.iter()
        .map(|p| ec.arg(span, str_to_ident(&p.name), p.rs.clone()))
        .collect();

    let foreign_mod = ast::ForeignMod {
        abi: Abi::C,
        items: vec![ast::ForeignItem {
            ident: fn_ident.clone(),
            attrs: Vec::new(),
            node: ast::ForeignItemKind::Fn(ec.fn_decl(rs_params, rs_ret),
                                           ast::Generics::default()),
            id: ast::DUMMY_NODE_ID,
            span: span,
            vis: ast::Visibility::Inherited,
        }],
    };

    // Create the source for the C++ side
    let cpp_params = params.iter()
        .fold(String::new(), |mut acc, p| {
            if !acc.is_empty() {
                acc.push_str(", ");
            }
            acc.push_str(&p.cpp);
            acc.push_str(" ");
            acc.push_str(&p.name);
            acc
        });

    let line_pragma = match ec.parse_sess.codemap().span_to_lines(span) {
        Ok(FileLines{ref file, ref lines}) if !lines.is_empty() =>
            format!("#line {} {:?}", lines[0].line_index + 1, file.name),
        _ => String::new(),
    };

    let cpp_decl = format!("\n{}\n{} {}({}) {}\n",
                           line_pragma, cpp_ret, fn_name,
                           cpp_params, body);

    CppFunc {
        ident: fn_ident,
        rs: foreign_mod,
        cpp: cpp_decl,
    }
}

// Macro expander implementations

struct CppInclude(Rc<RefCell<State>>);
impl TTMacroExpander for CppInclude {
    fn expand<'cx>(&self,
                   ec: &'cx mut ExtCtxt,
                   _: Span,
                   tts: &[ast::TokenTree])
                   -> Box<MacResult+'cx>
    {
        let inner = inner_text(ec, tts);

        let mut st = self.0.borrow_mut();
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
                   _: Span,
                   tts: &[ast::TokenTree])
                   -> Box<MacResult+'cx>
    {
        let inner = inner_text(ec, tts);

        let mut st = self.0.borrow_mut();
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

        let mut params = Vec::new();
        let mut args = Vec::new();

        // Parse the identifier list
        match parser.parse_token_tree().ok() {
            Some(ast::TokenTree::Delimited(span, ref del)) => {
                let mut parser = ec.new_parser_from_tts(&del.tts[..]);
                loop {
                    if parser.check(&token::Eof) {
                        break;
                    }

                    let mutable = parser.parse_mutability()
                        .unwrap_or(ast::Mutability::Immutable);
                    let constness = if mutable == ast::Mutability::Mutable { "" } else { "const" };
                    let ident = parser.parse_ident().unwrap();
                    let cppty = match &*parser.parse_str().unwrap().0 {
                        "void" => format!("{} void* ", constness),
                        x => format!("{} {}& ", constness, x),
                    };
                    let rsty = ec.ty_ptr(mac_span,
                                         ec.ty_ident(mac_span,
                                                     ast::Ident::with_empty_ctxt(
                                                         token::intern("u8"))),
                                         mutable);

                    params.push(CppParam {
                        rs: rsty.clone(),
                        cpp: cppty,
                        name: ident.name.as_str().to_string(),
                    });

                    // Build the rust call argument
                    let addr_of = if mutable == ast::Mutability::Immutable {
                        ec.expr_addr_of(mac_span,
                                        ec.expr_ident(mac_span, ident.clone()))
                    } else {
                        ec.expr_mut_addr_of(mac_span,
                                            ec.expr_ident(mac_span,
                                                          ident.clone()))
                    };
                    args.push(ec.expr_cast(mac_span,
                                           ec.expr_cast(mac_span,
                                                        addr_of,
                                                        ec.ty_ptr(mac_span,
                                                                  ec.ty_infer(mac_span),
                                                                  mutable)),
                                           rsty));

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
        let body_str = match get_tt_braced_text(ec, body_tt) {
            Some(x) => x,
            None => {
                ec.span_err(mac_span, "cpp! body must be surrounded by `{}`");
                return DummyResult::expr(mac_span);
            }
        };
        parser.expect(&token::Eof).unwrap();

        // Build the shared function
        let func = make_shared_function(ec,
                                        "cpp_generated",
                                        mac_span,
                                        body_str,
                                        &params,
                                        ret_cxxty,
                                        ret_ty);

        // Add the function decl to the string output
        st.fndecls.push_str(&func.cpp);

        // Create a block, with the foreign module and the function call as the
        // return value
        let exp = ec.expr_block(// Block
            ec.block(mac_span,
                     vec![ec.stmt_item(mac_span,
                                       ec.item(mac_span,
                                               func.ident.clone(),
                                               Vec::new(),
                                               ast::ItemKind::ForeignMod(func.rs)))],
                     Some(ec.expr_call_ident(mac_span, func.ident.clone(), args))));

        // Emit the rust code into the AST
        MacEager::expr(exp)
    }
}

struct CppFn(Rc<RefCell<State>>);
impl IdentMacroExpander for CppFn {
    fn expand<'cx>(&self,
                   ec: &'cx mut ExtCtxt,
                   mac_span: Span,
                   ident: ast::Ident,
                   tts: Vec<ast::TokenTree>)
                   -> Box<MacResult+'cx>
    {
        let mut st = self.0.borrow_mut();
        let mut parser = ec.new_parser_from_tts(&tts);

        let mut params = Vec::new();

        match parser.parse_token_tree().ok() {
            Some(ast::TokenTree::Delimited(_, ref del)) => {
                let mut parser = ec.new_parser_from_tts(&del.tts);
                loop {
                    if parser.check(&token::Eof) {
                        break;
                    }

                    let p_ident = parser.parse_ident().unwrap();
                    parser.expect(&token::Colon).unwrap();
                    let p_ty = parser.parse_ty().unwrap();

                    let p_cpp_ty = format!("{}", parser.parse_str().unwrap().0);

                    params.push(CppParam {
                        rs: p_ty,
                        cpp: p_cpp_ty,
                        name: p_ident.name.as_str().to_string(),
                    });

                    if !parser.eat(&token::Comma) {
                        break;
                    }
                }
            }
            _ => {
                ec.span_err(mac_span, "Unexpected!");
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
        let body_str = match get_tt_braced_text(ec, body_tt) {
            Some(x) => x,
            None => {
                ec.span_err(mac_span, "cpp! body must be surrounded by `{}`");
                return DummyResult::expr(mac_span);
            }
        };
        parser.expect(&token::Eof).unwrap();

        // Build the shared function
        let func = make_shared_function(ec,
                                        &ident.name.as_str(),
                                        mac_span,
                                        body_str,
                                        &params,
                                        ret_cxxty,
                                        ret_ty.clone());

        // Add the function decl to the string output
        st.fndecls.push_str(&func.cpp);

        // Item declaring the extern "C" function
        let extern_item = ec.item(mac_span,
                                  func.ident.clone(),
                                  Vec::new(),
                                  ast::ItemKind::ForeignMod(func.rs));

        // XXX: Refactor this to be shared with make_shared_function
        let fn_params = params.iter()
            .map(|p| ec.arg(mac_span, str_to_ident(&p.name), p.rs.clone()))
            .collect();

        let args = params.iter()
            .map(|p| ec.expr_ident(mac_span, str_to_ident(&p.name)))
            .collect();

        let fn_item = ec.item(
            mac_span,
            ident,
            Vec::new(),
            ast::ItemKind::Fn(
                ec.fn_decl(fn_params, ret_ty),
                ast::Unsafety::Unsafe,
                ast::Constness::NotConst,
                Abi::Rust,
                ast::Generics::default(),
                ec.block(
                    mac_span,
                    vec![ec.stmt_item(mac_span, extern_item)],
                    Some(ec.expr_call_ident(mac_span, func.ident.clone(), args)))));


        MacEager::items(SmallVector::one(fn_item))
    }
}
