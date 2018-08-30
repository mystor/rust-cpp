#[macro_use]
extern crate syn;
extern crate proc_macro2;
#[macro_use]
extern crate quote;

#[macro_use]
extern crate lazy_static;

use std::collections::hash_map::DefaultHasher;
use std::env;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use proc_macro2::{Span, TokenTree};
use syn::{Attribute, Ident, Type};

pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");

pub const LIB_NAME: &'static str = "librust_cpp_generated.a";
pub const MSVC_LIB_NAME: &'static str = "rust_cpp_generated.lib";

pub mod flags {
    pub const IS_COPY_CONSTRUCTIBLE: u32 = 0;
    pub const IS_DEFAULT_CONSTRUCTIBLE: u32 = 1;
    pub const IS_TRIVIALLY_DESTRUCTIBLE: u32 = 2;
    pub const IS_TRIVIALLY_COPYABLE: u32 = 3;
    pub const IS_TRIVIALLY_DEFAULT_CONSTRUCTIBLE: u32 = 4;
}

/// This constant is expected to be a unique string within the compiled binary
/// which preceeds a definition of the metadata. It begins with
/// rustcpp~metadata, which is printable to make it easier to locate when
/// looking at a binary dump of the metadata.
///
/// NOTE: In the future we may want to use a object file parser and a custom
/// section rather than depending on this string being unique.
#[cfg_attr(rustfmt, rustfmt_skip)]
pub const STRUCT_METADATA_MAGIC: [u8; 128] = [
    b'r', b'u', b's', b't', b'c', b'p', b'p', b'~',
    b'm', b'e', b't', b'a', b'd', b'a', b't', b'a',
    92,  74,  112, 213, 165, 185, 214, 120, 179, 17,  185, 25,  182, 253, 82,  118,
    148, 29,  139, 208, 59,  153, 78,  137, 230, 54,  26,  177, 232, 121, 132, 166,
    44,  106, 218, 57,  158, 33,  69,  32,  54,  204, 123, 226, 99,  117, 60,  173,
    112, 61,  56,  174, 117, 141, 126, 249, 79,  159, 6,   119, 2,   129, 147, 66,
    135, 136, 212, 252, 231, 105, 239, 91,  96,  232, 113, 94,  164, 255, 152, 144,
    64,  207, 192, 90,  225, 171, 59,  154, 60,  2,   0,   191, 114, 182, 38,  134,
    134, 183, 212, 227, 31,  217, 12,  5,   65,  221, 150, 59,  230, 96,  73,  62,
];

lazy_static! {
    pub static ref OUT_DIR: PathBuf = PathBuf::from(env::var("OUT_DIR").expect(
        r#"
-- rust-cpp fatal error --

The OUT_DIR environment variable was not set.
NOTE: rustc must be run by Cargo."#
    ));
    pub static ref FILE_HASH: u64 = {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        OUT_DIR.hash(&mut hasher);
        hasher.finish()
    };
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct Capture {
    pub mutable: bool,
    pub name: Ident,
    pub cpp: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct ClosureSig {
    pub captures: Vec<Capture>,
    pub ret: Type,
    pub cpp: String,
    pub std_body: String,
}

impl ClosureSig {
    pub fn name_hash(&self) -> u64 {
        // XXX: Use a better hasher than the default?
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }

    pub fn extern_name(&self) -> Ident {
        Ident::new(
            &format!("__cpp_closure_{}", self.name_hash()),
            Span::call_site(),
        )
    }
}

#[derive(Clone, Debug)]
pub struct Closure {
    pub sig: ClosureSig,
    pub body: String,
    pub callback_offset: u32,
}

#[derive(Clone, Debug)]
pub struct Class {
    pub name: Ident,
    pub cpp: String,
    pub attrs: Vec<Attribute>,
    pub line: String, // the #line directive
}

impl Class {
    pub fn name_hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.name.hash(&mut hasher);
        self.cpp.hash(&mut hasher);
        hasher.finish()
    }

    pub fn derives(&self, i: &str) -> bool {
        self.attrs.iter().any(|x| {
            use syn::{Meta, NestedMeta};
            x.interpret_meta().map_or(false, |m| {
                if let Meta::List(ref list) = m {
                    list.ident == "derive" && list.nested.iter().any(|y| {
                        if let NestedMeta::Meta(Meta::Word(ref d)) = y {
                            d == i
                        } else {
                            false
                        }
                    })
                } else {
                    false
                }
            })
        })
    }
}

#[derive(Debug)]
pub enum Macro {
    Closure(Closure),
    Lit(TokenTree),
}

#[derive(Debug)]
pub struct RustInvocation {
    pub begin: Span,
    pub end: Span,
    pub id: Ident,
    pub return_type: Option<String>,
    pub arguments: Vec<(Ident, String)>, // Vec of name and type
}

pub mod parsing {
    use super::{Capture, Class, Closure, ClosureSig, Macro, RustInvocation};
    use proc_macro2::TokenTree;
    use syn::{punctuated::Punctuated, Attribute, Ident, LitStr, Type, Visibility};

    named!(ident_or_self -> Ident, alt!(syn!(Ident) |
        keyword!(self) => { |x| x.into() } ));

    named!(name_as_string -> Capture,
           do_parse!(
               is_mut: option!(keyword!(mut)) >>
                   id: ident_or_self >>
                   keyword!(as) >>
                   cty: syn!(LitStr) >>
                   (Capture {
                       mutable: is_mut.is_some(),
                       name: id,
                       cpp: cty.value()
                   })
           ));

    named!(captures -> Vec<Capture>, map!(brackets!(call!(
        Punctuated::<Capture, Token![,]>::parse_separated_with, name_as_string)), |x| x.1.into_iter().collect()));

    named!(ret_ty -> (Type, String),
           alt!(
               do_parse!(punct!(->) >>
                         rty: syn!(Type) >>
                         keyword!(as) >>
                         cty: syn!(LitStr) >>
                         ((rty, cty.value()))) |
               value!((parse_quote!{()}, "void".to_owned()))
           ));

    named!(code_block -> TokenTree, syn!(TokenTree));

    named!(pub cpp_closure -> Closure,
           do_parse!(option!(keyword!(unsafe)) >>
                     captures: captures >>
                     ret: ret_ty >>
                     code: code_block >>
                     (Closure {
                         sig: ClosureSig {
                             captures: captures,
                             ret: ret.0,
                             cpp: ret.1,
                             // Need to filter the spaces because there is a difference between
                             // proc_macro2 and proc_macro and the hashes would not match
                             std_body: code.to_string().chars().filter(|x| *x != ' ').collect(),
                         },
                         body: code.to_string(),
                         callback_offset: 0
                     })));

    named!(pub build_macro -> Macro , alt!(
        cpp_closure => { |c| Macro::Closure(c) } |
        code_block => { |b| Macro::Lit(b) }
    ));

    named!(pub cpp_class -> Class,
           do_parse!(
            attrs: many0!(Attribute::parse_outer) >>
            option!(syn!(Visibility)) >>
            keyword!(unsafe) >>
            keyword!(struct) >>
            name: syn!(Ident) >>
            keyword!(as) >>
            cpp_type: syn!(LitStr) >>
            (Class {
                name: name,
                cpp: cpp_type.value(),
                attrs: attrs,
                line: String::default(),

            })));

    named!(rust_macro_argument -> (Ident, String),
        do_parse!(
            name: syn!(Ident) >>
            punct!(:) >>
            syn!(Type) >>
            keyword!(as) >>
            cty: syn!(LitStr) >>
            ((name, cty.value()))));

    named!(pub find_rust_macro -> RustInvocation,
        do_parse!(
            begin: custom_keyword!(rust) >>
            punct!(!) >>
            content: parens!(do_parse!(
                id: syn!(Ident) >>
                args: map!(brackets!(call!(Punctuated::<(Ident, String), Token![,]>::parse_separated_with, rust_macro_argument)),
                          |x| x.1.into_iter().collect()) >>
                rty : option!(
                    do_parse!(punct!(->) >>
                    syn!(Type) >>
                    keyword!(as) >>
                    cty: syn!(LitStr) >>
                    (cty.value()))) >>
                syn!(TokenTree) >>
                (id, args, rty))) >>
            (RustInvocation{
                begin: begin.span(),
                end: (content.0).0,
                id: (content.1).0,
                return_type: (content.1).2,
                arguments: (content.1).1
            })));

    named!(pub find_all_rust_macro -> Vec<RustInvocation>,
        map!(many0!(alt!(find_rust_macro => {|x| vec![x] }
                | map!(brackets!(call!(find_all_rust_macro)), |x| x.1)
                | map!(parens!(call!(find_all_rust_macro)), |x| x.1)
                | map!(braces!(call!(find_all_rust_macro)), |x| x.1)
                | syn!(TokenTree) => { |_| vec![] })),
            |x| x.into_iter().flat_map(|x|x).collect()));

}
