#![feature(plugin_registrar, rustc_private)]

#[macro_use]
extern crate syntax;

#[macro_use]
extern crate rustc;

#[macro_use]
extern crate rustc_front;

#[macro_use]
extern crate lazy_static;

extern crate uuid;
extern crate gcc;


use rustc::plugin::Registry;
use syntax::parse::token::intern;
use syntax::feature_gate::AttributeType;

use syntax::ext::base::{SyntaxExtension};

mod data;
mod mac;
mod lint;
mod types;

#[plugin_registrar]
pub fn plugin_registrar(reg: &mut Registry) {
    // Record the target triple so that it can be used later in the syntax extension
    // I do this here because I couldn't find a way to get the target triple in the
    // syntax extension callbacks.
    *data::CPP_TARGET.lock().unwrap() = reg.sess.target.target.llvm_target.clone();

    reg.register_syntax_extension(intern("cpp_include"),
                                  SyntaxExtension::NormalTT(Box::new(mac::expand_cpp_include),
                                                            None, false));
    reg.register_syntax_extension(intern("cpp_header"),
                                  SyntaxExtension::NormalTT(Box::new(mac::expand_cpp_header),
                                                            None, false));
    reg.register_syntax_extension(intern("cpp_flags"),
                                  SyntaxExtension::NormalTT(Box::new(mac::expand_cpp_flags),
                                                            None, false));
    reg.register_syntax_extension(intern("cpp"),
                                  SyntaxExtension::NormalTT(Box::new(mac::expand_cpp),
                                                            None, false));

    reg.register_late_lint_pass(Box::new(lint::CppLintPass));
    reg.register_attribute(format!("cpp_type"), AttributeType::Whitelisted);
}
