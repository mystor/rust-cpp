extern crate syntex_syntax as syntax;

use syntax::ast;
use syntax::parse;
use syntax::parse::parser::Parser;
use syntax::parse::token;
use syntax::print::pprust;
use syntax::tokenstream::TokenTree;

#[derive(Debug)]
pub enum MError {
    Other(String),
    None
}

pub type MResult<T> = Result<T, MError>;

impl<'a> From<syntax::diagnostics::plugin::DiagnosticBuilder<'a>> for MError {
    fn from(a: syntax::diagnostics::plugin::DiagnosticBuilder<'a>) -> MError {
        MError::Other(format!("{:?}", a))
    }
}

#[derive(Debug, Hash)]
pub struct Capture {
    pub mutable: bool,
    pub name: String,
    pub cpp_ty: String,
}

#[derive(Debug, Hash)]
pub struct CppClosure {
    pub captures: Vec<Capture>,
    pub cpp_ty: String,
    pub rs_ty: String,
    pub body: String
}

fn parse_captures(sess: &parse::ParseSess, parser: &mut Parser) -> MResult<Vec<Capture>> {
    // Get the token tree for the captures, and build a new parser out of it
    let tt = try!(parser.parse_token_tree());
    let tt = if let TokenTree::Delimited(_, d) = tt {
        d.tts.clone()
    } else {
        return Err(MError::None);
    };
    let mut parser = parse::tts_to_parser(sess, tt, vec![]);
    let mut captures = vec![];

    loop {
        if parser.eat(&token::Eof) {
            break
        }
        let mutability = try!(parser.parse_mutability());
        let name = try!(parser.parse_ident());
        try!(parser.expect_keyword(token::keywords::As));
        let cpp_ty = try!(parser.parse_str());
        captures.push(Capture {
            mutable: mutability == ast::Mutability::Mutable,
            name: name.to_string(),
            cpp_ty: cpp_ty.0.to_string(),
        });
        if !parser.eat(&token::Comma) {
            break
        }
    }

    Ok(captures)
}

fn parse_ret_ty(_: &parse::ParseSess, parser: &mut Parser) -> MResult<(String, String)> {
    if parser.eat(&token::RArrow) {
        let ty = try!(parser.parse_ty());
        try!(parser.expect_keyword(token::keywords::As));
        let cpp_ty = try!(parser.parse_str());

        Ok((pprust::ty_to_string(&ty), cpp_ty.0.to_string()))
    } else {
        Ok(("()".to_string(), "void".to_string()))
    }
}

fn parse_body(_: &parse::ParseSess, parser: &mut Parser) -> MResult<String> {
    if let Some((s, _, _)) = parser.parse_optional_str() {
        Ok(s.to_string())
    } else {
        let tt = try!(parser.parse_token_tree());
        Ok(pprust::tt_to_string(&tt))
    }
}

pub fn parse_cpp_closure(sess: &parse::ParseSess, parser: &mut Parser) -> CppClosure {
    let caps = parse_captures(sess, parser).unwrap();
    let (rs_ty, cpp_ty) = parse_ret_ty(sess, parser).unwrap();
    let body = parse_body(sess, parser).unwrap();

    CppClosure {
        captures: caps,
        rs_ty: rs_ty,
        cpp_ty: cpp_ty,
        body: body,
    }
}

pub use syntax::parse::{ParseSess, new_parser_from_source_str, tts_to_parser};
