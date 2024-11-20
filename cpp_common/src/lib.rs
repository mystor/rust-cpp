//! Implementation detail for the `cpp` crate.
//!
//! The purpose of this crate is only to allow sharing code between the
//! `cpp_build` and the `cpp_macros` crates.

#[macro_use]
extern crate syn;
extern crate proc_macro2;

#[macro_use]
extern crate lazy_static;

use std::collections::hash_map::DefaultHasher;
use std::env;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use proc_macro2::{Span, TokenStream, TokenTree};
use syn::ext::IdentExt;
use syn::parse::{Parse, ParseStream, Result};
use syn::{Attribute, Ident, Type};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub const LIB_NAME: &str = "librust_cpp_generated.a";
pub const MSVC_LIB_NAME: &str = "rust_cpp_generated.lib";

pub mod flags {
    pub const IS_COPY_CONSTRUCTIBLE: u32 = 0;
    pub const IS_DEFAULT_CONSTRUCTIBLE: u32 = 1;
    pub const IS_TRIVIALLY_DESTRUCTIBLE: u32 = 2;
    pub const IS_TRIVIALLY_COPYABLE: u32 = 3;
    pub const IS_TRIVIALLY_DEFAULT_CONSTRUCTIBLE: u32 = 4;
}

pub mod kw {
    #![allow(non_camel_case_types)]
    custom_keyword!(rust);
}

/// This constant is expected to be a unique string within the compiled binary
/// which precedes a definition of the metadata. It begins with
/// rustcpp~metadata, which is printable to make it easier to locate when
/// looking at a binary dump of the metadata.
///
/// NOTE: In the future we may want to use a object file parser and a custom
/// section rather than depending on this string being unique.
#[rustfmt::skip]
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

impl Parse for Capture {
    /// Parse a single captured variable inside within a `cpp!` macro.
    /// Example: `mut foo as "int"`
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Capture {
            mutable: input.parse::<Option<Token![mut]>>()?.is_some(),
            name: input.call(Ident::parse_any)?,
            cpp: {
                input.parse::<Token![as]>()?;
                input.parse::<syn::LitStr>()?.value()
            },
        })
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct ClosureSig {
    pub captures: Vec<Capture>,
    pub ret: Option<Type>,
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
        Ident::new(&format!("__cpp_closure_{}", self.name_hash()), Span::call_site())
    }
}

#[derive(Clone, Debug)]
pub struct Closure {
    pub sig: ClosureSig,
    pub body: TokenTree,
    pub body_str: String, // with `rust!` macro replaced
    pub callback_offset: u32,
}

impl Parse for Closure {
    /// Parse the inside of a `cpp!` macro when this macro is a closure.
    /// Example: `unsafe [foo as "int"] -> u32 as "int" { /*... */ }
    fn parse(input: ParseStream) -> Result<Self> {
        input.parse::<Option<Token![unsafe]>>()?;

        // Capture
        let capture_content;
        bracketed!(capture_content in input);
        let captures =
            syn::punctuated::Punctuated::<Capture, Token![,]>::parse_terminated(&capture_content)?
                .into_iter()
                .collect();

        // Optional return type
        let (ret, cpp) = if input.peek(Token![->]) {
            input.parse::<Token![->]>()?;
            let t: syn::Type = input.parse()?;
            input.parse::<Token![as]>()?;
            let s = input.parse::<syn::LitStr>()?.value();
            (Some(t), s)
        } else {
            (None, "void".to_owned())
        };

        let body = input.parse::<TokenTree>()?;
        // Need to filter the spaces because there is a difference between
        // proc_macro2 and proc_macro and the hashes would not match
        let std_body = body.to_string().chars().filter(|x| !x.is_whitespace()).collect();

        Ok(Closure {
            sig: ClosureSig { captures, ret, cpp, std_body },
            body,
            body_str: String::new(),
            callback_offset: 0,
        })
    }
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
            let mut result = false;
            if x.path().is_ident("derive") {
                x.parse_nested_meta(|m| {
                    if m.path.is_ident(i) {
                        result = true;
                    }
                    Ok(())
                })
                .unwrap();
            }
            result
        })
    }
}

impl Parse for Class {
    /// Parse the inside of a `cpp_class!` macro.
    /// Example: `#[derive(Default)] pub unsafe struct Foobar as "FooBar"`
    fn parse(input: ParseStream) -> Result<Self> {
        Ok(Class {
            attrs: input.call(Attribute::parse_outer)?,
            name: {
                input.parse::<syn::Visibility>()?;
                input.parse::<Token![unsafe]>()?;
                input.parse::<Token![struct]>()?;
                input.parse()?
            },
            cpp: {
                input.parse::<Token![as]>()?;
                input.parse::<syn::LitStr>()?.value()
            },
            line: String::new(),
        })
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum Macro {
    Closure(Closure),
    Lit(TokenStream),
}

impl Parse for Macro {
    /// Parse the inside of a `cpp!` macro (a literal or a closure)
    fn parse(input: ParseStream) -> Result<Self> {
        if input.peek(syn::token::Brace) {
            let content;
            braced!(content in input);
            return Ok(Macro::Lit(content.parse()?));
        }
        Ok(Macro::Closure(input.parse::<Closure>()?))
    }
}

#[derive(Debug)]
pub struct RustInvocation {
    pub begin: Span,
    pub end: Span,
    pub id: Ident,
    pub return_type: Option<String>,
    pub arguments: Vec<(Ident, String)>, // Vec of name and type
}

impl Parse for RustInvocation {
    /// Parse a `rust!` macro something looking like `rust!(ident [foo : bar as "bar"] { /*...*/ })`
    fn parse(input: ParseStream) -> Result<Self> {
        let rust_token = input.parse::<kw::rust>()?;
        input.parse::<Token![!]>()?;
        let macro_content;
        let p = parenthesized!(macro_content in input);
        let r = RustInvocation {
            begin: rust_token.span,
            end: p.span.close(),
            id: macro_content.parse()?,
            arguments: {
                let capture_content;
                bracketed!(capture_content in macro_content);
                capture_content
                    .parse_terminated(
                        |input: ParseStream| -> Result<(Ident, String)> {
                            let i = input.call(Ident::parse_any)?;
                            input.parse::<Token![:]>()?;
                            input.parse::<Type>()?;
                            input.parse::<Token![as]>()?;
                            let s = input.parse::<syn::LitStr>()?.value();
                            Ok((i, s))
                        },
                        Token![,],
                    )?
                    .into_iter()
                    .collect()
            },
            return_type: if macro_content.peek(Token![->]) {
                macro_content.parse::<Token![->]>()?;
                macro_content.parse::<Type>()?;
                macro_content.parse::<Token![as]>()?;
                Some(macro_content.parse::<syn::LitStr>()?.value())
            } else {
                None
            },
        };
        macro_content.parse::<TokenTree>()?;
        Ok(r)
    }
}
