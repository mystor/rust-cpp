#[macro_use]
extern crate cpp_syn as syn;

#[macro_use]
extern crate cpp_synom as synom;

#[macro_use]
extern crate quote;

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use syn::{Ident, Ty, Spanned};

pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");

pub const LIB_NAME: &'static str = "librust_cpp_generated.a";
pub const MSVC_LIB_NAME: &'static str = "rust_cpp_generated.lib";

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
    91,  74,  112, 213, 165, 185, 214, 120, 179, 17,  185, 25,  182, 253, 82,  118,
    148, 29,  139, 208, 59,  153, 78,  137, 230, 54,  26,  177, 232, 121, 132, 166,
    44,  106, 218, 57,  158, 33,  69,  32,  54,  204, 123, 226, 99,  117, 60,  173,
    112, 61,  56,  174, 117, 141, 126, 249, 79,  159, 6,   119, 2,   129, 147, 66,
    135, 136, 212, 252, 231, 105, 239, 91,  96,  232, 113, 94,  164, 255, 152, 144,
    64,  207, 192, 90,  225, 171, 59,  154, 60,  2,   0,   191, 114, 182, 38,  134,
    134, 183, 212, 227, 31,  217, 12,  5,   65,  221, 150, 59,  230, 96,  73,  62,
];

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct Capture {
    pub mutable: bool,
    pub name: Ident,
    pub cpp: String,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct ClosureSig {
    pub captures: Vec<Capture>,
    pub ret: Ty,
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
        format!("__cpp_closure_{}", self.name_hash()).into()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Closure {
    pub sig: ClosureSig,
    pub body: Spanned<String>,
}

pub enum Macro {
    Closure(Closure),
    Lit(Spanned<String>),
}

pub mod parsing {
    use syn::parse::{ident, string, ty, tt};
    use syn::{Ty, Spanned, DUMMY_SPAN};
    use super::{Capture, ClosureSig, Closure, Macro};

    macro_rules! mac_body {
        ($i: expr, $submac:ident!( $($args:tt)* )) => {
            delimited!(
                $i,
                alt!(punct!("[") | punct!("(") | punct!("{")),
                $submac!($($args)*),
                alt!(punct!("]") | punct!(")") | punct!("}"))
            )
        };
        ($i:expr, $e:expr) => {
            mac_body!($i, call!($e))
        };
    }

    named!(name_as_string -> Capture,
           do_parse!(
               is_mut: option!(keyword!("mut")) >>
                   id: ident >>
                   keyword!("as") >>
                   cty: string >>
                   (Capture {
                       mutable: is_mut.is_some(),
                       name: id,
                       cpp: cty.value
                   })
           ));

    named!(captures -> Vec<Capture>,
           delimited!(
               punct!("["),
               terminated_list!(
                   punct!(","),
                   name_as_string
               ),
               punct!("]")));

    named!(ret_ty -> (Ty, String),
           alt!(
               do_parse!(punct!("->") >>
                         rty: ty >>
                         keyword!("as") >>
                         cty: string >>
                         ((rty, cty.value))) |
               value!((Ty::Tup(Vec::new()), "void".to_owned()))
           ));

    named!(code_block -> Spanned<String>,
           alt!(
               do_parse!(s: string >> (Spanned {
                   node: s.value,
                   span: DUMMY_SPAN,
               })) |
               // XXX: This is really inefficient and means that things like ++y
               // will parse incorrectly (as we care about the layout of the
               // original source) so consider this a temporary monkey patch.
               // Once we get spans we can work past it.
               delimited!(punct!("{"), spanned!(many0!(tt)), punct!("}")) => {
                   |Spanned{ node, span }| Spanned {
                       node: quote!(#(#node)*).to_string(),
                       span: span
                   }
               }
           ));

    named!(pub cpp_closure -> Closure,
           do_parse!(captures: captures >>
                     ret: ret_ty >>
                     code: code_block >>
                     (Closure {
                         sig: ClosureSig {
                             captures: captures,
                             ret: ret.0,
                             cpp: ret.1,
                             std_body: code.node.clone(),
                         },
                         body: code,
                     })));

    named!(pub build_macro -> Macro , mac_body!(alt!(
        cpp_closure => { |c| Macro::Closure(c) } |
        code_block => { |b| Macro::Lit(b) }
    )));

    named!(pub expand_macro -> Closure, mac_body!(
        map!(tuple!(
            punct!("@"), keyword!("TYPE"), cpp_closure
        ), (|(_, _, x)| x))));
}
