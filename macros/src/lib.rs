#![feature(rustc_macro, rustc_macro_lib)]
extern crate rustc_macro;
extern crate cpp_common;

use std::hash::{Hash, Hasher, SipHasher};
use rustc_macro::TokenStream;
use cpp_common::{parse_cpp_closure, ParseSess, new_parser_from_source_str};

#[rustc_macro_derive(rust_cpp_internal)]
pub fn expand(input: TokenStream) -> TokenStream {
    let source = input.to_string();
    let trimmed = source.trim();

    // Do some sketchy string manipulation to skip having to parse and traverse
    // the Dummy struct.
    // XXX: This won't translate well to tokentrees, so will need to be
    // rewritten after the conversion.
    const START: &'static str = "struct Dummy(__!(";
    const END: &'static str = "));";
    assert!(trimmed.starts_with(START));
    assert!(trimmed.ends_with(END));
    let macro_body = trimmed.trim_left_matches(START).trim_right_matches(END);

    // Get a parser over the remaining string
    let sess = ParseSess::new();
    let mut parser = new_parser_from_source_str(
        &sess, vec![], "name".to_string(), macro_body.to_string()
    );
    let closure = parse_cpp_closure(&sess, &mut parser);
    let hash = {
        let mut hasher = SipHasher::new();
        closure.hash(&mut hasher);
        hasher.finish()
    };

    let rs_param = closure.captures.iter().map(|cap| {
        format!("{} : *{} u8", cap.name, if cap.mutable { "mut" } else { "const" })
    }).collect::<Vec<_>>().join(", ");

    let cpp_arg = closure.captures.iter().map(|cap| {
        cap.name.to_string()
    }).collect::<Vec<_>>().join(", ");

    let result = format!(r#"
struct Dummy;
impl Dummy {{
    unsafe fn call({rs_param}) -> {rs_ty} {{
        extern "C" {{
            fn _rust_cpp_closure_{hash}({rs_param}) -> {rs_ty};
        }}
        _rust_cpp_closure_{hash}({cpp_arg})
    }}
}}
"#,
        rs_param = rs_param,
        cpp_arg = cpp_arg,
        rs_ty = closure.rs_ty,
        hash = hash,
    );
    println!("GENERATED CODE = {}", &result);

    result.parse().unwrap()
}
