#[macro_use]
extern crate cpp_syn as syn;

#[macro_use]
extern crate cpp_synom as synom;

#[macro_use]
extern crate quote;

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use syn::{Ident, Ty, Spanned};

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
    pub fn extern_name(&self) -> Ident {
        // XXX: Use a better hasher than the default?
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        let result = hasher.finish();
        format!("__cpp_closure_{}", result).into()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Closure {
    pub sig: ClosureSig,
    pub body: Spanned<String>,
}

pub enum Macro {
    Closure(Closure),
    Lit(Spanned<String>)
}

pub mod parsing {
    use syn::parse::{ident, string, ty, tt, int};
    use syn::{Ty, Ident, Spanned, DUMMY_SPAN};
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

    named!(pub sizes_data -> Vec<(Ident, Vec<(usize, usize)>)>, many0!(do_parse!(
        name: ident >>
        nums: many0!(tuple!(int, int)) >>
        punct!(";") >>
        ((
            name,
            nums.into_iter()
                .map(|(size, align)| (size.value as usize, align.value as usize))
                .collect()
        )))));
}

pub fn parse_sizes_data(d: &str) -> Vec<(Ident, Vec<(usize, usize)>)> {
    // XXX: Handle this error better
    parsing::sizes_data(synom::ParseState::new(d)).expect(r#"
-- rust-cpp fatal error --

Failed to parse size data output in macro parser.
"#)
}
