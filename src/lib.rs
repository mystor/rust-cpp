#![feature(plugin_registrar, rustc_private)]

#[macro_use]
extern crate syntax;

#[macro_use]
extern crate rustc;

#[macro_use]
extern crate lazy_static;

extern crate uuid;
extern crate gcc;


use std::sync::Mutex;
use std::env;
use std::io::prelude::*;
use std::fs::File;
use std::ffi::OsString;
use std::collections::HashMap;

use rustc::plugin::Registry;
use syntax::parse::token::intern;

use syntax::codemap::Span;
use syntax::util::small_vector::SmallVector;
use syntax::abi;
use syntax::ast::*;
use syntax::ast_util::empty_generics;
use syntax::ext::base::{SyntaxExtension, MacResult, ExtCtxt, DummyResult, MacEager};
use syntax::ext::build::AstBuilder;
use syntax::parse::token::{self, InternedString};
use syntax::ptr::*;

use rustc::lint::*;
use rustc::session::search_paths::SearchPaths;
use rustc::middle::ty;
use rustc::middle::ty::expr_ty;
use rustc::middle::ty::sty::*;


use uuid::Uuid;

lazy_static! {
    static ref CPP_HEADERS: Mutex<String> = Mutex::new(String::new());
    static ref CPP_FNDECLS: Mutex<HashMap<String, CppFn>> = Mutex::new(HashMap::new());
}

struct CppParam {
    mutable: bool,
    name: String,
    ty: Option<String>,
}

impl CppParam {
    fn to_string(&self) -> String {
        let mut s = String::new();
        if !self.mutable {
            s.push_str("const ");
        }

        if let Some(ref ty) = self.ty {
            s.push_str(ty);
        } else {
            s.push_str("void");
        }
        s.push_str("* ");

        s.push_str(&self.name);

        s
    }
}

struct CppFn {
    name: String,
    arg_idents: Vec<CppParam>,
    ret_ty: Option<String>,
    body: String,
}

impl CppFn {
    fn to_string(&self) -> String {
        // Generate the parameter list
        let c_params = self.arg_idents.iter().fold(String::new(), |acc, new| {
            if acc.is_empty() {
                new.to_string()
            } else {
                format!("{}, {}", acc, new.to_string())
            }
        });

        let c_ty = if let Some(ref ty) = self.ret_ty {
            &ty[..]
        } else {
            panic!("Unexpected None ret_ty on CppFn")
        };

        format!("{} {}({}) {}", c_ty, self.name, c_params, self.body)
    }
}

#[plugin_registrar]
pub fn plugin_registrar(reg: &mut Registry) {
    reg.register_syntax_extension(intern("cpp_include"),
                                  SyntaxExtension::NormalTT(Box::new(expand_cpp_include),
                                                            None, false));
    reg.register_syntax_extension(intern("cpp"),
                                  SyntaxExtension::NormalTT(Box::new(expand_cpp),
                                                            None, false));

    reg.register_lint_pass(Box::new(CppLintPass));
    println!("Working Dir: {:?}", reg.sess.working_dir);
}

pub fn expand_cpp_include<'a>(ec: &'a mut ExtCtxt,
                              mac_span: Span,
                              tts: &[TokenTree]) -> Box<MacResult + 'a> {
    if tts.len() == 0 {
        ec.span_err(mac_span,
                    "unexpected empty cpp_include!");
        return DummyResult::any(mac_span);
    }

    let span = Span {
        lo: tts.first().unwrap().get_span().lo,
        hi: tts.last().unwrap().get_span().hi,
        expn_id: mac_span.expn_id,
    };

    let inner = ec.parse_sess.span_diagnostic.cm.span_to_snippet(span).unwrap();

    let mut headers = CPP_HEADERS.lock().unwrap();
    *headers = format!("{}\n#include {}\n", *headers, inner);

    MacEager::items(SmallVector::zero())
}

pub fn expand_cpp<'a>(ec: &'a mut ExtCtxt,
                      mac_span: Span,
                      tts: &[TokenTree]) -> Box<MacResult + 'a> {
    let mut parser = ec.new_parser_from_tts(tts);
    let mut captured_idents = Vec::new();

    // Parse the identifier list
    match parser.parse_token_tree().ok() {
        Some(TtDelimited(span, ref del)) => {
            let mut parser = ec.new_parser_from_tts(&del.tts[..]);
            loop {
                if parser.check(&token::Eof) { break }

                let mutable = parser.parse_mutability().unwrap_or(MutImmutable);
                let ident = parser.parse_ident().unwrap();
                captured_idents.push((ident, mutable));

                if !parser.eat(&token::Comma).unwrap() {
                    break
                }
            }
            if !parser.check(&token::Eof) {
                ec.span_err(span,
                            "Unexpected token in captured identifier list");
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
    let ret_ty = if parser.eat(&token::RArrow).unwrap() {
        parser.parse_ty()
    } else {
        ec.ty(mac_span, TyTup(Vec::new()))
    };

    // Read in the body
    let body_tt = parser.parse_token_tree().unwrap();
    parser.expect(&token::Eof).unwrap();

    // Extract the string body of the c++ code
    let body_str = match body_tt {
        TtDelimited(span, ref del) => {
            if del.open_token() != token::OpenDelim(token::Brace) {
                ec.span_err(span, "cpp! body must be surrounded by `{}`");
                return DummyResult::expr(span);
            }

            ec.parse_sess.span_diagnostic.cm.span_to_snippet(span).unwrap()
        }
        _ => {
            ec.span_err(mac_span, "cpp! body must be a block surrounded by `{}`");
            return DummyResult::expr(body_tt.get_span());
        }
    };


    // Generate the rust parameters and arguments
    let params: Vec<_> = captured_idents.iter().map(|&(ref id, mutable)| {
        let arg_ty = ec.ty_ptr(mac_span,
                               ec.ty_ident(mac_span,
                                           Ident::new(intern("u8"))),
                               mutable);
        ec.arg(mac_span, id.clone(), arg_ty)
    }).collect();

    let args: Vec<_> = captured_idents.iter().map(|&(ref id, mutable)| {
        let arg_ty = ec.ty_ptr(mac_span,
                               ec.ty_ident(mac_span,
                                           Ident::new(intern("u8"))),
                               mutable);

        let addr_of = if mutable == MutImmutable {
            ec.expr_addr_of(mac_span, ec.expr_ident(mac_span, id.clone()))
        } else {
            ec.expr_mut_addr_of(mac_span, ec.expr_ident(mac_span, id.clone()))
        };

        ec.expr_cast(mac_span,
                     ec.expr_cast(mac_span,
                                  addr_of,
                                  ec.ty_ptr(mac_span,
                                            ec.ty_infer(mac_span),
                                            mutable)),
                     arg_ty)
    }).collect();

    let fn_ident = Ident::new(intern(
        &format!("rust_cpp_{}", Uuid::new_v4().to_simple_string())));

    // extern "C" declaration of function
    let foreign_mod = ForeignMod {
        abi: abi::C,
        items: vec![P(ForeignItem {
            ident: fn_ident.clone(),
            attrs: Vec::new(),
            node: ForeignItemFn(ec.fn_decl(params, ret_ty),
                                empty_generics()),
            id: DUMMY_NODE_ID,
            span: mac_span,
            vis: Inherited,
        })]
    };

    let exp = ec.expr_block(
        // Block
        ec.block(
            mac_span,
            // Extern "C" declarations for the c implemented functions
            vec![ec.stmt_item(
                mac_span,
                ec.item(mac_span,
                        fn_ident.clone(),
                        vec![
                            ec.attribute(
                                mac_span,
                                ec.meta_list(
                                    mac_span,
                                    InternedString::new("link"),
                                    vec![
                                        ec.meta_name_value(
                                            mac_span,
                                            InternedString::new("name"),
                                            LitStr(InternedString::new("c++"),
                                                   CookedStr))
                                            ])),
                            ec.attribute(
                                mac_span,
                                ec.meta_list(
                                    mac_span,
                                    InternedString::new("link"),
                                    vec![
                                        ec.meta_name_value(
                                            mac_span,
                                            InternedString::new("name"),
                                            LitStr(InternedString::new("rust_cpp_tmp"),
                                                   CookedStr)),
                                        ec.meta_name_value(
                                            mac_span,
                                            InternedString::new("kind"),
                                            LitStr(InternedString::new("static"),
                                                   CookedStr))
                                            ]))],
                        ItemForeignMod(foreign_mod)))],
            Some(ec.expr_call_ident(
                mac_span,
                fn_ident.clone(),
                args))));

    let cpp_decl = CppFn {
        name: format!("{}", fn_ident.name.as_str()),
        arg_idents: captured_idents.iter().map(|&(ref id, mutable)| CppParam {
            mutable: mutable == MutMutable,
            name: format!("{}", id.name.as_str()),
            ty: None,
        }).collect(),
        ret_ty: None,
        body: body_str,
    };

    // Add the generated function declaration to the CPP_FNDECLS global variable.
    let mut fndecls = CPP_FNDECLS.lock().unwrap();
    fndecls.insert(fn_ident.name.as_str().to_string(), cpp_decl);

    // Emit the rust code into the AST
    MacEager::expr(exp)
}

fn super_hack_get_out_dir() -> OsString {
    let mut out_dir = None;
    let mut found_out_dir = false;
    for a in std::env::args() {
        if a == "--out-dir" {
            found_out_dir = true;
            continue;
        }
        if found_out_dir {
            out_dir = Some(OsString::from(format!("{}", a)));
            break;
        }
    }

    out_dir.unwrap_or_else(|| {
        std::env::current_dir().unwrap().into_os_string()
    })
}

/// This lint pass right now doesn't actually do any linting, instead it has the role
/// of building the library containing the code we just extracted with the macro,
/// before rustc tries to link against it.
///
/// At some point, I'd like to use this phase to gain type information about the captured
/// variables, and return values, and use that to ensure the correctness of the code, and
/// provide nicer types etc. to the c++ code which the user writes.
///
/// For example, #[repr(C)] structs could have an equivalent type definition generated in
/// the C++ side, which could allow for some nicer interaction with the captured values.
struct CppLintPass;
impl LintPass for CppLintPass {
    fn get_lints(&self) -> LintArray {
        lint_array!()
    }

    fn check_expr(&mut self, cx: &Context, exp: &Expr) {
        if let ExprCall(ref callee, ref args) = exp.node {
            if let ExprPath(None, ref path) = callee.node {
                if path.segments.len() == 1 {
                    let name = path.segments[0].identifier.name.as_str();

                    record_type_data(cx, name, exp, args);
                }
            }
        }
    }
}

fn type_to_cpp_type(ty: ty::Ty, level: usize, maxlevel: usize) -> String {
    if level > maxlevel { return format!("void"); }

    match ty.sty {
        ty_bool => format!("int8_t"),

        ty_int(TyIs) => format!("intptr_t"),
        ty_int(TyI8) => format!("int8_t"),
        ty_int(TyI16) => format!("int16_t"),
        ty_int(TyI32) => format!("int32_t"),
        ty_int(TyI64) => format!("int64_t"),

        ty_uint(TyUs) => format!("uintptr_t"),
        ty_uint(TyU8) => format!("uint8_t"),
        ty_uint(TyU16) => format!("uint16_t"),
        ty_uint(TyU32) => format!("uint32_t"),
        ty_uint(TyU64) => format!("uint64_t"),

        ty_float(TyF32) => format!("float"),
        ty_float(TyF64) => format!("double"),

        ty_ptr(ref ty) => format!("{}*", type_to_cpp_type(ty.ty, level + 1, maxlevel)),
        ty_rptr(_, ref ty) => format!("{}*", type_to_cpp_type(ty.ty, level + 1, maxlevel)),

        ty_tup(ref it) => {
            if it.len() == 0 {
                // Unit type
                format!("void")
            } else {
                if level == 0 {
                    panic!("Illegal cpp! return type")
                } else {
                    format!("void")
                }
            }
        }

        _ => {
            if level == 0 {
                panic!("Illegal cpp! return type")
            } else {
                format!("void")
            }
        }
    }
}

fn record_type_data(cx: &Context,
                    name: &str,
                    call: &Expr,
                    args: &[P<Expr>]) {
    let mut decls = CPP_FNDECLS.lock().unwrap();
    if let Some(cppfn) = decls.get_mut(name) {
        let ret_ty = expr_ty(cx.tcx, call);
        cppfn.ret_ty = Some(type_to_cpp_type(ret_ty, 0, 10));

        for (i, arg) in args.iter().enumerate() {
            // Strip the two casts off
            if let ExprCast(ref e, _) = arg.node {
                if let ExprCast(ref e, _) = e.node {
                    if let ExprAddrOf(_, ref e) = e.node {
                        let arg_ty = expr_ty(cx.tcx, e);
                        cppfn.arg_idents[i].ty = Some(type_to_cpp_type(arg_ty, 1, 10));
                        continue
                    }
                }
            }

            panic!("Expected a double-casted reference as an argument.")
        }
    } else { return }

    // We've processed all of them!
    // Finalize!
    if decls.values().all(|x| x.ret_ty.is_some()) {
        finalize(cx, &mut decls);
    }
}

fn finalize(cx: &Context, decls: &mut HashMap<String, CppFn>) {
    // Generate the c++ we want to compile
    let headers = CPP_HEADERS.lock().unwrap();

    let fndecls = decls.values().fold(String::new(), |acc, new| {
        format!("{}\n\n{}", acc, new.to_string())
    });

    let cppcode = format!(r#"
/* Headers */
{}
/* Function Declarations */
extern "C" {{
  {}
}}
"#, *headers, fndecls);

    // Get the output directory, which is _way_ harder than I was expecting,
    // (also super hacky).
    let out_dir = super_hack_get_out_dir();

    // Create the C++ file which we will compile
    {
        let path = std::path::Path::new(&out_dir).join("rust_cpp_tmp.cpp");
        let mut f = File::create(path).unwrap();
        f.write_all(cppcode.as_bytes()).unwrap();
    }

    // I didn't want to write my own compiler driver-driver, so I'm using gcc.
    // Unfortuantely, it expects to be run within a cargo build script, so I'm going
    // to set a bunch of environment variables to trick it into not crashing
    env::set_var("TARGET", &cx.sess().target.target.llvm_target);
    env::set_var("HOST", &cx.sess().host.llvm_target);
    env::set_var("OPT_LEVEL", format!("{}", cx.sess().opts.cg.opt_level.unwrap_or(0)));
    env::set_var("CARGO_MANIFEST_DIR", &out_dir);
    env::set_var("OUT_DIR", &out_dir);
    env::set_var("PROFILE", "");

    // Please look away - I'm about to do something truely awful
    unsafe {
        let sp = &cx.sess().opts.search_paths;
        // OH GOD
        let sp_mut: &mut SearchPaths = &mut *(sp as *const _ as *mut _);

        sp_mut.add_path(&out_dir.to_str().unwrap());
    }

    println!("########### Running GCC ###########");
    gcc::Config::new()
        .cpp(true)
        .file("rust_cpp_tmp.cpp")
        .compile("librust_cpp_tmp.a");
    println!("########### Done Rust-C++ ############");
}
