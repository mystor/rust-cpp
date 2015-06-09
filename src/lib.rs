#![feature(plugin_registrar, rustc_private)]

#[macro_use]
extern crate syntax;

#[macro_use]
extern crate rustc;

#[macro_use]
extern crate lazy_static;

extern crate uuid;
extern crate gcc;


use rustc::plugin::Registry;
use syntax::parse::token::intern;

use syntax::ext::base::{SyntaxExtension};

mod data;
mod mac;
mod lint;
mod types;

#[plugin_registrar]
pub fn plugin_registrar(reg: &mut Registry) {
    reg.register_syntax_extension(intern("cpp_include"),
                                  SyntaxExtension::NormalTT(Box::new(mac::expand_cpp_include),
                                                            None, false));
    reg.register_syntax_extension(intern("cpp"),
                                  SyntaxExtension::NormalTT(Box::new(mac::expand_cpp),
                                                            None, false));

    reg.register_lint_pass(Box::new(lint::CppLintPass));
}
