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

use rustc::plugin::Registry;
use syntax::parse::token::intern;

use syntax::codemap::Span;
use syntax::owned_slice::OwnedSlice;
use syntax::util::small_vector::SmallVector;
use syntax::abi;
use syntax::ast::*;
use syntax::ext::base::{SyntaxExtension, MacResult, ExtCtxt, DummyResult, MacEager};
use syntax::ext::build::AstBuilder;
use syntax::parse::token::{self, InternedString};
use syntax::ptr::*;

use rustc::lint::*;
use rustc::session::search_paths::SearchPaths;

use uuid::Uuid;

lazy_static! {
    static ref CPP_HEADERS: Mutex<String> = Mutex::new(String::new());
    static ref CPP_FNDECLS: Mutex<String> = Mutex::new(String::new());
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
        Some(TtDelimited(_, ref del)) => {
            let mut iter = del.tts.iter();
            loop {
                match iter.next() {
                    None => break,
                    Some(&TtToken(_, token::Ident(ref id, _))) => {
                        captured_idents.push(id.clone());

                        match iter.next() {
                            Some(&TtToken(_, token::Comma)) => (),
                            None => break,
                            Some(tt) => {
                                ec.span_err(tt.get_span(),
                                            "Unexpected token in captured ident list");
                                return DummyResult::expr(tt.get_span());
                            }
                        }
                    }
                    Some(tt) => {
                        ec.span_err(tt.get_span(),
                                    "Unexpected token in captured ident list");
                        return DummyResult::expr(tt.get_span());
                    }
                }
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

    let arg_ty = ec.ty_ptr(mac_span,
                           ec.ty_ident(mac_span,
                                       Ident::new(intern("u8"))),
                           MutImmutable);

    let params: Vec<_> = captured_idents.iter().map(|id| {
        ec.arg(mac_span, id.clone(), arg_ty.clone())
    }).collect();

    let args: Vec<_> = captured_idents.iter().map(|id| {
        ec.expr_cast(mac_span,
                     ec.expr_cast(mac_span,
                                  ec.expr_addr_of(mac_span, ec.expr_ident(mac_span, id.clone())),
                                  ec.ty_ptr(mac_span,
                                            ec.ty_infer(mac_span),
                                            MutImmutable)),
                     arg_ty.clone())
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
                                Generics {
                                    lifetimes: Vec::new(),
                                    ty_params: OwnedSlice::empty(),
                                    where_clause: WhereClause {
                                        id: DUMMY_NODE_ID,
                                        predicates: Vec::new(),
                                    }
                                }),
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

    let c_params = captured_idents.iter().map(|id| {
        format!("void* {}", id.name.as_str())
    }).fold(String::new(), |b, e| {
        if b.is_empty() { e } else { format!("{}, {}", b, e) }
    });

    let c_decl = format!("int32_t {}({}) {}", fn_ident.name.as_str(), c_params, body_str);

    // Add the generated function declaration to the CPP_FNDECLS global variable.
    let mut fndecls = CPP_FNDECLS.lock().unwrap();
    *fndecls = format!("{}\n{}", *fndecls, c_decl);

    println!("{}", syntax::print::pprust::expr_to_string(&exp));

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

    fn check_crate(&mut self, cx: &Context, _: &Crate) {
        // Generate the c++ we want to compile
        let headers = CPP_HEADERS.lock().unwrap();
        let fndecls = CPP_FNDECLS.lock().unwrap();
        let cppcode = format!("/* Headers */{}\n\n/* Function Declarations */\nextern \"C\" {{ {}\n}}",
                              *headers, *fndecls);

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

        println!("starting libs");
        for lib in &cx.sess().opts.libs {
            println!("lib: {}", lib.0);
        }

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
}
