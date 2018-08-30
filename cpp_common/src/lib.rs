#[macro_use]
extern crate cpp_syn as syn;

#[macro_use]
extern crate cpp_synom as synom;

#[macro_use]
extern crate quote;

#[macro_use]
extern crate lazy_static;

use std::collections::hash_map::DefaultHasher;
use std::env;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use syn::{Ident, MetaItem, Spanned, Ty};

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
    pub callback_offset: u32,
}

#[derive(Clone, Debug)]
pub struct Class {
    pub name: Ident,
    pub cpp: String,
    pub public: bool,
    pub attrs: Vec<MetaItem>,
    pub line: String, // the #line directive
}

impl Class {
    pub fn name_hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.name.hash(&mut hasher);
        self.cpp.hash(&mut hasher);
        self.public.hash(&mut hasher);
        hasher.finish()
    }

    pub fn derives(&self, i: &str) -> bool {
        self.attrs.iter().any(|x| {
            if let MetaItem::List(ref n, ref list) = x {
                n.as_ref() == "derive" && list.iter().any(|y| {
                    if let syn::NestedMetaItem::MetaItem(MetaItem::Word(ref d)) = y {
                        d.as_ref() == i
                    } else {
                        false
                    }
                })
            } else {
                false
            }
        })
    }
}

pub enum Macro {
    Closure(Closure),
    Lit(Spanned<String>),
}

pub struct RustInvocation {
    pub begin: usize,
    pub end: usize,
    pub id: Ident,
    pub return_type: Option<String>,
    pub arguments: Vec<(String, String)>, // Vec of name and type
}

pub mod parsing {
    use super::{Capture, Class, Closure, ClosureSig, Macro, RustInvocation};
    use syn::parse::{ident, lit, string, tt, ty};
    use syn::{Ident, MetaItem, NestedMetaItem, Spanned, Ty, DUMMY_SPAN};
    use synom::space::{block_comment, whitespace};

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

    named!(ident_or_self -> Ident, alt!( ident | keyword!("self") => { |_| "self".into() } ));

    named!(name_as_string -> Capture,
           do_parse!(
               is_mut: option!(keyword!("mut")) >>
                   id: ident_or_self >>
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
           do_parse!(option!(keyword!("unsafe")) >>
                     captures: captures >>
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
                         callback_offset: 0
                     })));

    named!(pub build_macro -> Macro , mac_body!(alt!(
        cpp_closure => { |c| Macro::Closure(c) } |
        code_block => { |b| Macro::Lit(b) }
    )));

    named!(pub expand_macro -> Closure, mac_body!(
        map!(tuple!(
            punct!("@"), keyword!("TYPE"), cpp_closure
        ), (|(_, _, x)| x))));

    //FIXME: make cpp_syn::attr::parsing::outer_attr  public
    // This is just a trimmed down version of it
    named!(pub outer_attr -> MetaItem, alt!(
        do_parse!(
            punct!("#") >>
            punct!("[") >>
            attr: meta_item >>
            punct!("]") >>
            (attr)
        ) | do_parse!(
            punct!("///") >>
            not!(tag!("/")) >>
            content: spanned!(take_until!("\n")) >>
            (MetaItem::NameValue("doc".into(), content.node.into()))
        ) | do_parse!(
            option!(whitespace) >>
            peek!(tuple!(tag!("/**"), not!(tag!("*")))) >>
            com: block_comment >>
            (MetaItem::NameValue("doc".into(), com[3..com.len()-2].into()))
        )
    ));

    named!(meta_item -> MetaItem, alt!(
        do_parse!(
            id: ident >>
            punct!("(") >>
            inner: terminated_list!(punct!(","), nested_meta_item) >>
            punct!(")") >>
            (MetaItem::List(id, inner))
        )
        |
        do_parse!(
            name: ident >>
            punct!("=") >>
            value: lit >>
            (MetaItem::NameValue(name, value))
        )
        |
        map!(ident, MetaItem::Word)
    ));
    named!(nested_meta_item -> NestedMetaItem, alt!(
        meta_item => { NestedMetaItem::MetaItem }
        |
        lit => { NestedMetaItem::Literal }
    ));

    named!(pub cpp_class -> Class,
           do_parse!(
            attrs: many0!(outer_attr) >>
            is_pub: option!(tuple!(
                keyword!("pub"),
                option!(delimited!(punct!("("), many0!(tt), punct!(")"))))) >>
            keyword!("unsafe") >>
            keyword!("struct") >>
            name: ident >>
            keyword!("as") >>
            cpp_type: string >>
            (Class {
                name: name,
                cpp: cpp_type.value,
                public: is_pub.is_some(),
                attrs: attrs,
                line: String::default(),
            })));

    named!(pub class_macro -> Class , mac_body!(cpp_class));

    named!(rust_macro_argument -> (String, String),
        do_parse!(
            name: ident >>
            punct!(":") >>
            ty >>
            keyword!("as") >>
            cty: string >>
            ((name.as_ref().to_owned(), cty.value))));

    named!(pub find_rust_macro -> RustInvocation,
        do_parse!(
            alt!(take_until!("rust!") | take_until!("rust !")) >>
            begin: spanned!(keyword!("rust")) >>
            punct!("!") >>
            punct!("(") >>
            id: ident >>
            punct!("[") >>
            args: separated_list!(punct!(","), rust_macro_argument) >>
            punct!("]") >>
            rty : option!(do_parse!(punct!("->") >>
                    ty >>
                    keyword!("as") >>
                    cty: string >>
                    (cty.value))) >>
            tt >>
            end: spanned!(punct!(")")) >>
            (RustInvocation{
                begin: begin.span.lo,
                end: end.span.hi,
                id: id,
                return_type: rty,
                arguments: args
            })));

    named!(pub find_all_rust_macro -> Vec<RustInvocation>,
        do_parse!(
            r : many0!(find_rust_macro) >>
            many0!(alt!( tt => {|_| ""} | punct!("]") | punct!(")") | punct!("}")))
            >> (r)));

}
