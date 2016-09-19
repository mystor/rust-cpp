use std::cell::RefCell;
use std::rc::Rc;
use std::fs::File;
use std::io::prelude::*;
use std::env;
use std::path::Path;
use std::iter::FromIterator;
use std::hash::{Hash, Hasher, SipHasher};

use syntex_syntax::ast;
use syntex_syntax::ext::base::{
    MacroLoader,
    MacResult,
    ExtCtxt,
    DummyResult,
    TTMacroExpander,
    NamedSyntaxExtension,
    SyntaxExtension,
};
use syntex_syntax::codemap::{Span, FileLines, SpanSnippetError};
use syntex_syntax::parse::{self, token};
use syntex_syntax::parse::token::{Token, keywords};
use syntex_syntax::ext::expand;
use syntex_syntax::feature_gate;
use syntex_syntax::parse::{PResult, parser, common};
use syntex_syntax::tokenstream::TokenTree;

use gcc;

use cpp_common::{parse_cpp_closure, CppClosure};

const RUST_TYPES_HEADER: &'static str = r#"
#ifndef _RUST_TYPES_H_
#define _RUST_TYPES_H_

#include <cstdint>

namespace rs {
    typedef int8_t i8;
    static_assert(sizeof(i8) == 1, "int is the right size");
    typedef int16_t i16;
    static_assert(sizeof(i16) == 2, "int is the right size");
    typedef int32_t i32;
    static_assert(sizeof(i32) == 4, "int is the right size");
    typedef int64_t i64;
    static_assert(sizeof(i64) == 8, "int is the right size");
    typedef intptr_t isize;

    typedef uint8_t u8;
    static_assert(sizeof(u8) == 1, "int is the right size");
    typedef uint16_t u16;
    static_assert(sizeof(u16) == 2, "int is the right size");
    typedef uint32_t u32;
    static_assert(sizeof(u32) == 4, "int is the right size");
    typedef uint64_t u64;
    static_assert(sizeof(u64) == 8, "int is the right size");
    typedef uintptr_t usize;

    typedef float f32;
    static_assert(sizeof(f32) == 4, "float is the right size");
    typedef double f64;
    static_assert(sizeof(f64) == 8, "float is the right size");

    typedef u8 bool_;
    static_assert(sizeof(bool_) == 1, "booleans are the right size");

    typedef uint32_t char_;
    static_assert(sizeof(char_) == 4, "char is the right size");
}
#endif
"#;

pub fn mk_macro<F>(name: &str, extension: F) -> NamedSyntaxExtension
    where F: TTMacroExpander + 'static
{
    let name = token::intern(name);
    let syntax_extension = SyntaxExtension::NormalTT(
        Box::new(extension),
        None,
        false
    );
    (name, syntax_extension)
}

pub fn build<P: AsRef<Path>, F>(src: P, name: &str, configure: F)
    where F: for<'a> FnOnce(&'a mut gcc::Config)
{
    // This must be a Rc, such that it may be referred to by the macro handler
    let state: Rc<RefCell<State>> = Default::default();

    // Run the syntax extension through syntex_syntax, to parse out the
    // information stored in the macro invocations
    {
        // Create the syntax extensions
        let syntax_exts = vec![
            mk_macro("cpp", Cpp(state.clone())),
        ];

        // Run the expanders on the crate
        let sess = parse::ParseSess::new();

        let krate = parse::parse_crate_from_file(
            src.as_ref(),
            Vec::new(),
            &sess).unwrap();

        let features = feature_gate::get_features(
            &sess.span_diagnostic,
            &krate.attrs);

        struct DummyMacroLoader;
        impl MacroLoader for DummyMacroLoader{
            fn load_crate(&mut self, _extern_crate: &ast::Item, _allows_macros: bool) -> Vec<ast::MacroDef> {
                Vec::new()
            }
        }

        let mut ecfg = expand::ExpansionConfig::default(name.to_string());
        ecfg.features = Some(&features);

        let mut dml = DummyMacroLoader;
        let mut ecx = ExtCtxt::new(&sess, Vec::new(), ecfg, &mut dml);

        expand::expand_crate(&mut ecx, syntax_exts, krate);
    }

    let out_dir = env::var("OUT_DIR")
        .expect("Environment Variable OUT_DIR must be set");
    let file = Path::new(&out_dir).join(&format!("{}.cpp", name));
    let rust_types_file = Path::new(&out_dir).join("rust_types.h");

    // Generate the output code
    {
        let state = state.borrow();
        let code = String::from_iter([
            "// This is machine generated code, created by rust-cpp\n",
            &state.includes[..],
            &state.headers[..],
            "extern \"C\" {\n",
            &state.fndecls[..],
            "}",
        ].iter().cloned());

        // Write out the file
        let mut f = File::create(&file).unwrap();
        f.write_all(code.as_bytes()).unwrap();
    }

    // Write out the rust types file
    {
        let mut f = File::create(&rust_types_file).unwrap();
        f.write_all(RUST_TYPES_HEADER.as_bytes()).unwrap();
    }

    // Invoke gcc to build the library.
    {
        let mut config = gcc::Config::new();
        config.cpp(true).file(file);
        configure(&mut config);
        config.compile(&format!("lib{}.a", name));
    }
}

#[derive(Debug, Default)]
struct State {
    includes: String,
    headers: String,
    fndecls: String,
}

fn span_snippet<'s>(ec: &mut ExtCtxt<'s>, span: Span) -> PResult<'s, String> {
    match ec.parse_sess.codemap().span_to_snippet(span) {
        Err(SpanSnippetError::IllFormedSpan(..)) =>
            fatal(ec, span, "Unexpected ill-formed span"),
        Err(SpanSnippetError::DistinctSources(..)) =>
            fatal(ec, span, "Unexpected span across distinct sources"),
        Err(SpanSnippetError::MalformedForCodemap(..)) =>
            fatal(ec, span, "Unexpected malformed span for codemap"),
        Err(SpanSnippetError::SourceNotAvailable{ref filename}) =>
            fatal(ec, span, &format!("Unexpected unavaliable source: {}", filename)),
        Ok(v) => Ok(v),
    }
}

fn fatal<'s, T>(ec: &mut ExtCtxt<'s>, span: Span, msg: &str) -> PResult<'s, T> {
    Err(ec.struct_span_fatal(span, msg))
}

fn line_pragma(ec: &mut ExtCtxt, span: Span) -> String {
    match ec.parse_sess.codemap().span_to_lines(span) {
        Ok(FileLines{ref file, ref lines}) if !lines.is_empty() =>
            format!("#line {} {:?}\n", lines[0].line_index + 1, file.name),
        _ => String::new(),
    }
}

fn read_code_block<'s>(ec: &mut ExtCtxt<'s>,
                       parser: &mut parser::Parser<'s>)
                       -> PResult<'s, (Span, String)> {
    match try!(parser.parse_token_tree()) {
        TokenTree::Token(span, token::Literal(token::Str_(s), _)) |
        TokenTree::Token(span, token::Literal(token::StrRaw(s, _), _)) => {
            let s = ast::Ident::with_empty_ctxt(s).name.as_str();
            Ok((span, s.to_string()))
        }
        TokenTree::Delimited(_, ref del) if del.delim == token::Brace => {
            let span = Span {
                lo: del.open_span.hi,
                hi: del.close_span.lo,
                expn_id: del.open_span.expn_id,
            };

            Ok((span, try!(span_snippet(ec, span))))
        }
        tt => return fatal(ec, tt.get_span(), "Unexpected token while parsing import")
    }
}

fn expand_include<'s>(ec: &mut ExtCtxt<'s>,
                      parser: &mut parser::Parser<'s>,
                      st: &mut State,
                      _: Span)
                      -> PResult<'s, ()> {
    let (span, text) = match try!(parser.parse_token_tree()) {
        // < foo >
        TokenTree::Token(lo_span, Token::Lt) => {
            let hi;
            loop {
                match try!(parser.parse_token_tree()) {
                    TokenTree::Token(hi_span, Token::Gt) => {
                        hi = hi_span.hi;
                        break;
                    }
                    TokenTree::Token(span, Token::Eof) =>
                        return fatal(ec, span, "Unexpected EOF while parsing import"),
                    _ => continue,
                }
            }

            let span = Span {
                lo: lo_span.lo,
                hi: hi,
                expn_id: lo_span.expn_id,
            };

            (span, try!(span_snippet(ec, span)))
        }

        // "foo"
        TokenTree::Token(span, Token::Literal(token::Lit::Str_(_), _)) =>
            (span, try!(span_snippet(ec, span))),

        tt => return fatal(ec, tt.get_span(), "Unexpected token while parsing import")
    };

    // Add the #include statement to the output
    st.includes.push_str(&line_pragma(ec, span));
    st.includes.push_str(&format!("#include {}\n", text));

    Ok(())
}

fn expand_raw<'s>(ec: &mut ExtCtxt<'s>,
                  parser: &mut parser::Parser<'s>,
                  st: &mut State,
                  _: Span)
                  -> PResult<'s, ()> {
    let (span, text) = try!(read_code_block(ec, parser));

    // Add the #include statement to the output
    st.headers.push_str(&line_pragma(ec, span));
    st.headers.push_str(&format!("{}\n", text));

    Ok(())
}

fn expand_fn<'s>(ec: &mut ExtCtxt<'s>,
                 parser: &mut parser::Parser<'s>,
                 st: &mut State,
                 _: Span)
                 -> PResult<'s, ()> {
    let mut name_args = String::new();

    // Parse the function name
    let id = try!(parser.parse_ident());
    name_args.push_str(&id.name.as_str());

    // Parse the argument list
    let args: Vec<_> =
        try!(parser.parse_unspanned_seq(
            &token::OpenDelim(token::Paren),
            &token::CloseDelim(token::Paren),
            common::SeqSep::trailing_allowed(token::Comma),
            |p| {
                let name = try!(p.parse_ident());
                try!(p.expect(&Token::Colon));
                try!(p.parse_ty_sum());
                try!(p.expect_keyword(keywords::As));
                let (cppty, _) = try!(p.parse_str());

                Ok(format!("{} {}", cppty, name))
            }));

    name_args.push('(');

    let mut just_args = String::new();
    for arg in args {
        if !just_args.is_empty() {
            just_args.push_str(", ");
        }
        just_args.push_str(&arg);
    }
    name_args.push_str(&just_args);
    name_args.push(')');

    // The actual function declaration
    let mut func = String::new();

    // Parse the return type, defaulting to 'void' if no type is provided
    if parser.eat(&token::RArrow) {
        // XXX: Allow ! as a return type?
        try!(parser.parse_ty());
        try!(parser.expect_keyword(keywords::As));
        let (cppty, _) = try!(parser.parse_str());
        func.push_str(&cppty);
    } else {
        func.push_str("void");
    }
    func.push(' ');
    func.push_str(&name_args);
    func.push_str(" {");

    // Read the body
    let (span, code) = try!(read_code_block(ec, parser));
    func.push_str(&code);
    func.push_str("}");

    // Write out the function declaration
    st.fndecls.push_str(&line_pragma(ec, span));
    st.fndecls.push_str(&format!("{}\n", func));

    Ok(())
}

fn expand_enum<'s>(ec: &mut ExtCtxt<'s>,
                   parser: &mut parser::Parser<'s>,
                   st: &mut State,
                   kw_span: Span)
                   -> PResult<'s, ()> {
    let mut s = format!("enum ");
    let mut add_prefix = false;

    // Parse the function name. If we see "class" or "prefix", then record the
    // relevant information and parse another ident
    let mut id = try!(parser.parse_ident());
    if id.name.as_str() == "class" {
        s.push_str("class ");
        id = try!(parser.parse_ident());
    } else if id.name.as_str() == "prefix" {
        add_prefix = true;
        id = try!(parser.parse_ident());
    }

    s.push_str(&id.name.as_str());
    s.push_str(" {\n");

    // Parse the argument list
    let mut opts = String::new();
    try!(parser.parse_unspanned_seq(
        &token::OpenDelim(token::Brace),
        &token::CloseDelim(token::Brace),
        common::SeqSep::trailing_allowed(token::Comma),
        |p| {
            let name = try!(p.parse_ident());

            let name_str = if add_prefix {
                format!("{}_{}", id, name)
            } else {
                format!("{}", name)
            };

            if opts.is_empty() {
                opts.push_str("    ");
            } else {
                opts.push_str(",\n    ");
            }
            opts.push_str(&name_str);
            Ok(())
        }));
    s.push_str(&opts);
    s.push_str("\n};\n");

    st.headers.push_str(&line_pragma(ec, kw_span));
    st.headers.push_str(&s);

    Ok(())
}

fn expand_struct<'s>(ec: &mut ExtCtxt<'s>,
                     parser: &mut parser::Parser<'s>,
                     st: &mut State,
                     kw_span: Span)
                     -> PResult<'s, ()> {
    let mut s = format!("struct ");

    // Parse the function name
    let id = try!(parser.parse_ident());
    s.push_str(&id.name.as_str());
    s.push_str(" {\n");

    // Parse the argument list
    let args: Vec<_> =
        try!(parser.parse_unspanned_seq(
            &token::OpenDelim(token::Brace),
            &token::CloseDelim(token::Brace),
            common::SeqSep::trailing_allowed(token::Comma),
            |p| {
                p.eat_keyword(keywords::Pub);
                let name = try!(p.parse_ident());
                try!(p.expect(&Token::Colon));
                try!(p.parse_ty_sum());
                try!(p.expect_keyword(keywords::As));
                let (cppty, _) = try!(p.parse_str());

                Ok(format!("    {} {};\n", cppty, name))
            }));

    for arg in args {
        s.push_str(&arg);
    }

    s.push_str("};\n");

    st.headers.push_str(&line_pragma(ec, kw_span));
    st.headers.push_str(&s);

    Ok(())
}

fn expand_closure(ec: &mut ExtCtxt, closure: CppClosure, st: &mut State, span: Span) {
    let cpp_param = closure.captures.iter().map(|cap| {
        format!("{} {}& {}",
                if cap.mutable { "" } else { "const" },
                cap.cpp_ty,
                cap.name)
    }).collect::<Vec<_>>().join(", ");

    let hash = {
        let mut hasher = SipHasher::new();
        closure.hash(&mut hasher);
        hasher.finish()
    };

    let result = format!(r#"
{cpp_ty} _rust_cpp_closure_{hash}({cpp_param}) {{ {body} }}
"#,
                         cpp_ty = closure.cpp_ty,
                         hash = hash,
                         cpp_param = cpp_param,
                         body = closure.body);
    for line in result.lines() {
        println!("cargo:warning={}", line);
    }

    st.fndecls.push_str(&line_pragma(ec, span));
    st.fndecls.push_str(&format!("{}\n", result));
}

struct Cpp(Rc<RefCell<State>>);
impl TTMacroExpander for Cpp {
    fn expand<'cx>(&self,
                   ec: &'cx mut ExtCtxt,
                   mac_span: Span,
                   tts: &[TokenTree])
                   -> Box<MacResult+'cx>
    {
        let mut st = self.0.borrow_mut();
        let mut parser = ec.new_parser_from_tts(tts);

        // Check for a closure. If there is a closure, no other items will be
        // present in the macro, so we can just stop here.
        if parser.check(&token::OpenDelim(token::Paren)) {
            let closure = parse_cpp_closure(ec.parse_sess, &mut parser);
            expand_closure(ec, closure, &mut st, mac_span);
            if let Err(mut e) = parser.expect(&token::Eof) {
                e.emit();
            }
            return DummyResult::any(mac_span);
        }

        loop {
            if parser.check(&token::Eof) {
                break
            }

            let res = match parser.parse_token_tree() {
                // Looking at a meta item, parse it, discarding it, and move on
                Ok(TokenTree::Token(_, Token::Pound)) => {
                    match parser.parse_token_tree() {
                        Ok(TokenTree::Token(span, Token::Ident(ref i))) =>
                            if i.name.as_str() == "include" {
                                expand_include(ec, &mut parser, &mut *st, span)
                            } else {
                                fatal(ec, span, "Unrecognized token after #")
                            },

                        // The meta item will take the form #[...], so we can just
                        // parse the [] as a single token tree
                        Ok(TokenTree::Delimited(..)) => Ok(()),
                        Ok(tt) => fatal(ec, tt.get_span(), "Unrecognized token after #"),
                        Err(e) => Err(e),
                    }
                }

                // Looking at an identifier, check which one
                Ok(TokenTree::Token(span, Token::Ident(ref i))) => {
                    if i.name.as_str() == "raw" {
                        expand_raw(ec, &mut parser, &mut *st, span)
                    } else if i.name.as_str() == "fn" {
                        expand_fn(ec, &mut parser, &mut *st, span)
                    } else if i.name.as_str() == "enum" {
                        expand_enum(ec, &mut parser, &mut *st, span)
                    } else if i.name.as_str() == "struct" {
                        expand_struct(ec, &mut parser, &mut *st, span)
                    } else if i.name.as_str() == "pub" {
                        // If we see a `pub`, it isn't relevant to us, so ignore it
                        Ok(())
                    } else {
                        fatal(ec, span, "Unrecognized token")
                    }
                }

                // Error cases
                Err(e) => Err(e),
                Ok(ref tt) => fatal(ec, tt.get_span(), "Unrecognized token"),
            };

            if let Err(mut e) = res {
                e.span_note(mac_span, "While parsing cpp! macro");
                e.emit();
                return DummyResult::any(mac_span)
            }
        }

        DummyResult::any(mac_span)
    }
}
