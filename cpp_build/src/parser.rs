use cpp_common::{Class, Closure, Macro, RustInvocation};
use regex::Regex;
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::mem::swap;
use std::path::{Path, PathBuf};
use syn::visit::Visit;

#[derive(Debug)]
pub enum Error {
    ParseCannotOpenFile {
        src_path: String,
    },
    ParseSyntaxError {
        src_path: String,
        error: syn::parse::Error,
    },
    LexError {
        src_path: String,
        line: u32,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Error::ParseCannotOpenFile { ref src_path } => {
                write!(f, "Parsing crate: cannot open file `{}`.", src_path)
            }
            Error::ParseSyntaxError {
                ref src_path,
                ref error,
            } => write!(f, "Parsing file : `{}`:\n{}", src_path, error),
            Error::LexError {
                ref src_path,
                ref line,
            } => write!(f, "{}:{}: Lexing error", src_path, line + 1),
        }
    }
}

#[derive(Debug)]
struct LineError(u32, String);

impl LineError {
    fn add_line(self, a: u32) -> LineError {
        LineError(self.0 + a, self.1)
    }
}

impl fmt::Display for LineError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}:{}", self.0 + 1, self.1)
    }
}

impl From<LexError> for LineError {
    fn from(e: LexError) -> Self {
        LineError(e.line, "Lexing error".into())
    }
}

enum ExpandSubMacroType<'a> {
    Lit,
    Closure(&'a mut u32), // the offset
}

// Given a string containing some C++ code with a rust! macro,
// this functions expand the rust! macro to a call to an extern
// function
fn expand_sub_rust_macro(input: String, mut t: ExpandSubMacroType) -> Result<String, LineError> {
    let mut result = input;
    let mut extra_decl = String::new();
    let mut search_index = 0;

    loop {
        let (begin, end, line) = {
            let mut begin = 0;
            let mut cursor = new_cursor(&result);
            cursor.advance(search_index);
            while !cursor.is_empty() {
                cursor = skip_whitespace(cursor);
                let r = skip_literal(cursor)?;
                cursor = r.0;
                if r.1 {
                    continue;
                }
                if cursor.is_empty() {
                    break;
                }
                if let Ok((cur, ident)) = symbol(cursor) {
                    begin = cursor.off as usize;
                    cursor = cur;
                    if ident != "rust" {
                        continue;
                    }
                } else {
                    cursor = cursor.advance(1);
                    continue;
                }
                cursor = skip_whitespace(cursor);
                if !cursor.starts_with("!") {
                    continue;
                }
                break;
            }
            if cursor.is_empty() {
                return Ok(extra_decl + &result);
            }
            let end = find_delimited((find_delimited(cursor, "(")?.0).advance(1), ")")?.0;
            (begin, end.off as usize + 1, cursor.line)
        };
        let input: ::proc_macro2::TokenStream = result[begin..end]
            .parse()
            .map_err(|_| LineError(line, "TokenStream parse error".into()))?;
        let rust_invocation =
            ::syn::parse2::<RustInvocation>(input).map_err(|e| LineError(line, e.to_string()))?;
        let fn_name = match t {
            ExpandSubMacroType::Lit => {
                extra_decl.push_str(&format!("extern \"C\" void {}();\n", rust_invocation.id));
                rust_invocation.id.clone().to_string()
            }
            ExpandSubMacroType::Closure(ref mut offset) => {
                use cpp_common::FILE_HASH;
                **offset += 1;
                format!(
                    "rust_cpp_callbacks{file_hash}[{offset}]",
                    file_hash = *FILE_HASH,
                    offset = **offset - 1
                )
            }
        };

        let mut decl_types = rust_invocation
            .arguments
            .iter()
            .map(|&(_, ref val)| format!("rustcpp::argument_helper<{}>::type", val))
            .collect::<Vec<_>>();
        let mut call_args = rust_invocation
            .arguments
            .iter()
            .map(|&(ref val, _)| val.to_string())
            .collect::<Vec<_>>();

        let fn_call = match rust_invocation.return_type {
            None => format!(
                "reinterpret_cast<void (*)({types})>({f})({args})",
                f = fn_name,
                types = decl_types.join(", "),
                args = call_args.join(", ")
            ),
            Some(rty) => {
                decl_types.push(format!("rustcpp::return_helper<{rty}>", rty = rty));
                call_args.push("0".to_string());
                format!(
                    "std::move(*reinterpret_cast<{rty}*(*)({types})>({f})({args}))",
                    rty = rty,
                    f = fn_name,
                    types = decl_types.join(", "),
                    args = call_args.join(", ")
                )
            }
        };

        let fn_call = {
            // remove the rust! macro from the C++ snippet
            let orig = result.drain(begin..end);
            // add \ņ to the invocation in order to keep the same amount of line numbers
            // so errors point to the right line.
            orig.filter(|x| *x == '\n').fold(fn_call, |mut res, _| {
                res.push('\n');
                res
            })
        };
        // add the invocation of call where the rust! macro used to be.
        result.insert_str(begin, &fn_call);
        search_index = begin + fn_call.len();
    }
}

#[test]
fn test_expand_sub_rust_macro() {
    let x = expand_sub_rust_macro(
        "{ rust!(xxx [] { 1 }); }".to_owned(),
        ExpandSubMacroType::Lit,
    );
    assert_eq!(
        x.unwrap(),
        "extern \"C\" void xxx();\n{ reinterpret_cast<void (*)()>(xxx)(); }"
    );

    let x = expand_sub_rust_macro(
        "{ hello( rust!(xxx [] { 1 }), rust!(yyy [] { 2 }); ) }".to_owned(),
        ExpandSubMacroType::Lit,
    );
    assert_eq!(x.unwrap(), "extern \"C\" void xxx();\nextern \"C\" void yyy();\n{ hello( reinterpret_cast<void (*)()>(xxx)(), reinterpret_cast<void (*)()>(yyy)(); ) }");

    let s = "{ /* rust! */  /* rust!(xxx [] { 1 }) */ }".to_owned();
    assert_eq!(
        expand_sub_rust_macro(s.clone(), ExpandSubMacroType::Lit).unwrap(),
        s
    );
}

#[path = "strnom.rs"]
mod strnom;
use crate::strnom::*;

fn skip_literal(mut input: Cursor) -> PResult<bool> {
    //input = whitespace(input)?.0;
    if input.starts_with("\"") {
        input = cooked_string(input.advance(1))?.0;
        debug_assert!(input.starts_with("\""));
        return Ok((input.advance(1), true));
    }
    if input.starts_with("b\"") {
        input = cooked_byte_string(input.advance(2))?.0;
        debug_assert!(input.starts_with("\""));
        return Ok((input.advance(1), true));
    }
    if input.starts_with("\'") {
        input = input.advance(1);
        let cur = cooked_char(input)?.0;
        if !cur.starts_with("\'") {
            return Ok((symbol(input)?.0, true));
        }
        return Ok((cur.advance(1), true));
    }
    if input.starts_with("b\'") {
        input = cooked_byte(input.advance(2))?.0;
        if !input.starts_with("\'") {
            return Err(LexError { line: input.line });
        }
        return Ok((input.advance(1), true));
    }
    lazy_static! {
        static ref RAW: Regex = Regex::new(r##"^b?r#*""##).unwrap();
    }
    if RAW.is_match(input.rest) {
        let q = input.rest.find('r').unwrap();
        input = input.advance(q + 1);
        return raw_string(input).map(|x| (x.0, true));
    }
    Ok((input, false))
}

fn new_cursor(s: &str) -> Cursor {
    Cursor {
        rest: s,
        off: 0,
        line: 0,
        column: 0,
    }
}

#[test]
fn test_skip_literal() -> Result<(), LexError> {
    assert!((skip_literal(new_cursor(r#""fofofo"ok xx"#))?.0).starts_with("ok"));
    assert!((skip_literal(new_cursor(r#""kk\"kdk"ok xx"#))?.0).starts_with("ok"));
    assert!((skip_literal(new_cursor("r###\"foo \" bar \\\" \"###ok xx"))?.0).starts_with("ok"));
    assert!(
        (skip_literal(new_cursor("br###\"foo 'jjk' \" bar \\\" \"###ok xx"))?.0).starts_with("ok")
    );
    assert!((skip_literal(new_cursor("'4'ok xx"))?.0).starts_with("ok"));
    assert!((skip_literal(new_cursor("'\''ok xx"))?.0).starts_with("ok"));
    assert!((skip_literal(new_cursor("b'\''ok xx"))?.0).starts_with("ok"));
    assert!((skip_literal(new_cursor("'abc ok xx"))?.0).starts_with(" ok"));
    assert!((skip_literal(new_cursor("'a ok xx"))?.0).starts_with(" ok"));

    assert!((skip_whitespace(new_cursor("ok xx"))).starts_with("ok"));
    assert!((skip_whitespace(new_cursor("   ok xx"))).starts_with("ok"));
    assert!((skip_whitespace(new_cursor(
        " \n /*  /*dd \n // */ */ // foo \n    ok xx/* */"
    )))
    .starts_with("ok"));

    Ok(())
}

// advance the cursor until it finds the needle.
fn find_delimited<'a>(mut input: Cursor<'a>, needle: &str) -> PResult<'a, ()> {
    let mut stack: Vec<&'static str> = vec![];
    while !input.is_empty() {
        input = skip_whitespace(input);
        input = skip_literal(input)?.0;
        if input.is_empty() {
            break;
        }
        if stack.is_empty() && input.starts_with(needle) {
            return Ok((input, ()));
        } else if stack.last().map_or(false, |x| input.starts_with(x)) {
            stack.pop();
        } else if input.starts_with("(") {
            stack.push(")");
        } else if input.starts_with("[") {
            stack.push("]");
        } else if input.starts_with("{") {
            stack.push("}");
        } else if input.starts_with(")") || input.starts_with("]") || input.starts_with("}") {
            return Err(LexError { line: input.line });
        }
        input = input.advance(1);
    }
    Err(LexError { line: input.line })
}

#[test]
fn test_find_delimited() -> Result<(), LexError> {
    assert!((find_delimited(new_cursor(" x f ok"), "f")?.0).starts_with("f ok"));
    assert!((find_delimited(new_cursor(" {f} f ok"), "f")?.0).starts_with("f ok"));
    assert!(
        (find_delimited(new_cursor(" (f\")\" { ( ) } /* ) */ f ) f ok"), "f")?.0)
            .starts_with("f ok")
    );
    Ok(())
}

#[test]
fn test_cursor_advance() -> Result<(), LexError> {
    assert_eq!(new_cursor("\n\n\n").advance(2).line, 2);
    assert_eq!(new_cursor("\n \n\n").advance(2).line, 1);
    assert_eq!(new_cursor("\n\n\n").advance(2).column, 0);
    assert_eq!(new_cursor("\n \n\n").advance(2).column, 1);

    assert_eq!(
        (find_delimited(new_cursor("\n/*\n  \n */ ( \n ) /* */ f"), "f")?.0).line,
        4
    );
    assert_eq!(
        (find_delimited(new_cursor("\n/*\n  \n */ ( \n ) /* */ f"), "f")?.0).column,
        9
    );
    Ok(())
}

fn line_directive(path: &Path, cur: Cursor) -> String {
    let mut line = format!(
        "#line {} \"{}\"\n",
        cur.line + 1,
        path.to_string_lossy().replace('\\', "\\\\")
    );
    for _ in 0..cur.column {
        line.push(' ');
    }
    line
}

#[derive(Default)]
pub struct Parser {
    pub closures: Vec<Closure>,
    pub classes: Vec<Class>,
    pub snippets: String,
    pub callbacks_count: u32,
    current_path: PathBuf, // The current file being parsed
    mod_dir: PathBuf,
    mod_error: Option<Error>, // An error occuring while visiting the modules
}

impl Parser {
    pub fn parse_crate(&mut self, crate_root: PathBuf) -> Result<(), Error> {
        let parent = crate_root
            .parent()
            .map(|x| x.to_owned())
            .unwrap_or_default();
        self.parse_mod(crate_root, parent)
    }

    fn parse_mod(&mut self, mod_path: PathBuf, submod_dir: PathBuf) -> Result<(), Error> {
        let mut s = String::new();
        let mut f = File::open(&mod_path).map_err(|_| Error::ParseCannotOpenFile {
            src_path: mod_path.to_str().unwrap().to_owned(),
        })?;
        f.read_to_string(&mut s)
            .map_err(|_| Error::ParseCannotOpenFile {
                src_path: mod_path.to_str().unwrap().to_owned(),
            })?;

        let fi = syn::parse_file(&s).map_err(|x| Error::ParseSyntaxError {
            src_path: mod_path.to_str().unwrap().to_owned(),
            error: x,
        })?;

        let mut current_path = mod_path;
        let mut mod_dir = submod_dir;

        swap(&mut self.current_path, &mut current_path);
        swap(&mut self.mod_dir, &mut mod_dir);

        self.find_cpp_macros(&s)?;
        self.visit_file(&fi);
        if let Some(err) = self.mod_error.take() {
            return Err(err);
        }

        swap(&mut self.current_path, &mut current_path);
        swap(&mut self.mod_dir, &mut mod_dir);

        Ok(())
    }

    /*
    fn parse_macro(&mut self, tts: TokenStream) {
        let mut last_ident: Option<syn::Ident> = None;
        let mut is_macro = false;
        for t in tts.into_iter() {
            match t {
                TokenTree::Punct(ref p) if p.as_char() == '!'  => is_macro = true,
                TokenTree::Ident(i) => {
                    is_macro = false;
                    last_ident = Some(i);
                }
                TokenTree::Group(d) => {
                    if is_macro && last_ident.as_ref().map_or(false, |i| i == "cpp") {
                        self.handle_cpp(&d.stream())
                    } else if is_macro && last_ident.as_ref().map_or(false, |i| i == "cpp_class") {
                        self.handle_cpp_class(&d.stream())
                    } else {
                        self.parse_macro(d.stream())
                    }
                    is_macro = false;
                    last_ident = None;
                }
                _ => {
                    is_macro = false;
                    last_ident = None;
                }
            }
        }
    }
    */

    fn find_cpp_macros(&mut self, source: &str) -> Result<(), Error> {
        let mut cursor = new_cursor(source);
        while !cursor.is_empty() {
            cursor = skip_whitespace(cursor);
            let r = skip_literal(cursor).map_err(|e| self.lex_error(e))?;
            cursor = r.0;
            if r.1 {
                continue;
            }
            if let Ok((cur, ident)) = symbol(cursor) {
                cursor = cur;
                if ident != "cpp" && ident != "cpp_class" {
                    continue;
                }
                cursor = skip_whitespace(cursor);
                if !cursor.starts_with("!") {
                    continue;
                }
                cursor = skip_whitespace(cursor.advance(1));
                let delim = if cursor.starts_with("(") {
                    ")"
                } else if cursor.starts_with("[") {
                    "]"
                } else if cursor.starts_with("{") {
                    "}"
                } else {
                    continue;
                };
                cursor = cursor.advance(1);
                let mut macro_cur = cursor;
                cursor = find_delimited(cursor, delim)
                    .map_err(|e| self.lex_error(e))?
                    .0;
                let size = (cursor.off - macro_cur.off) as usize;
                macro_cur.rest = &macro_cur.rest[..size];
                if ident == "cpp" {
                    self.handle_cpp(macro_cur).unwrap_or_else(|e| {
                        panic!(
                            "Error while parsing cpp! macro:\n{:?}:{}",
                            self.current_path, e
                        )
                    });
                } else {
                    debug_assert_eq!(ident, "cpp_class");
                    self.handle_cpp_class(macro_cur).unwrap_or_else(|e| {
                        panic!(
                            "Error while parsing cpp_class! macro:\n{:?}:{}",
                            self.current_path, e
                        )
                    });
                }
                continue;
            }
            if cursor.is_empty() {
                break;
            }
            cursor = cursor.advance(1); // Not perfect, but should work
        }
        Ok(())
    }

    fn lex_error(&self, e: LexError) -> Error {
        Error::LexError {
            src_path: self.current_path.clone().to_str().unwrap().to_owned(),
            line: e.line,
        }
    }

    fn handle_cpp(&mut self, x: Cursor) -> Result<(), LineError> {
        // Since syn don't give the exact string, we extract manually
        let begin = (find_delimited(x, "{")?.0).advance(1);
        let end = find_delimited(begin, "}")?.0;
        let extracted = &begin.rest[..(end.off - begin.off) as usize];

        let input: ::proc_macro2::TokenStream = x
            .rest
            .parse()
            .map_err(|_| LineError(x.line, "TokenStream parse error".into()))?;
        match ::syn::parse2::<Macro>(input).map_err(|e| LineError(x.line, e.to_string()))? {
            Macro::Closure(mut c) => {
                c.callback_offset = self.callbacks_count;
                c.body_str = line_directive(&self.current_path, begin)
                    + &expand_sub_rust_macro(
                        extracted.to_string(),
                        ExpandSubMacroType::Closure(&mut self.callbacks_count),
                    )
                    .map_err(|e| e.add_line(begin.line))?;
                self.closures.push(c);
            }
            Macro::Lit(_l) => {
                self.snippets.push('\n');
                let snip = expand_sub_rust_macro(
                    line_directive(&self.current_path, begin) + extracted,
                    ExpandSubMacroType::Lit,
                )
                .map_err(|e| e.add_line(begin.line))?;
                self.snippets.push_str(&snip);
            }
        }
        Ok(())
    }

    fn handle_cpp_class(&mut self, x: Cursor) -> Result<(), LineError> {
        let input: ::proc_macro2::TokenStream = x
            .rest
            .parse()
            .map_err(|_| LineError(x.line, "TokenStream parse error".into()))?;
        let mut class =
            ::syn::parse2::<Class>(input).map_err(|e| LineError(x.line, e.to_string()))?;
        class.line = line_directive(&self.current_path, x);
        self.classes.push(class);
        Ok(())
    }
}

impl<'ast> Visit<'ast> for Parser {
    /* This is currently commented out because proc_macro2 don't allow us to get the text verbatim
       (https://github.com/alexcrichton/proc-macro2/issues/110#issuecomment-411959999)
    fn visit_macro(&mut self, mac: &syn::Macro) {
        if mac.path.segments.len() != 1 {
            return;
        }
        if mac.path.segments[0].ident == "cpp" {
            self.handle_cpp(&mac.tts);
        } else if mac.path.segments[0].ident == "cpp_class" {
            self.handle_cpp_class(&mac.tts);
        } else {
            self.parse_macro(mac.tts.clone());
        }
    }*/

    fn visit_item_mod(&mut self, item: &'ast syn::ItemMod) {
        if self.mod_error.is_some() {
            return;
        }

        if item.content.is_some() {
            let mut parent = self.mod_dir.join(item.ident.to_string());
            swap(&mut self.mod_dir, &mut parent);
            syn::visit::visit_item_mod(self, item);
            swap(&mut self.mod_dir, &mut parent);
            return;
        }

        // Determine the path of the inner module's file
        for attr in &item.attrs {
            match attr.parse_meta() {
                // parse #[path = "foo.rs"]: read module from the specified path
                Ok(syn::Meta::NameValue(syn::MetaNameValue {
                    ref path,
                    lit: syn::Lit::Str(ref s),
                    ..
                })) if path.is_ident("path") => {
                    let mod_path = self.mod_dir.join(&s.value());
                    let parent = self
                        .mod_dir
                        .parent()
                        .map(|x| x.to_owned())
                        .unwrap_or_default();
                    return self
                        .parse_mod(mod_path, parent)
                        .unwrap_or_else(|err| self.mod_error = Some(err));
                }
                // parse #[cfg(feature = "feature")]: don't follow modules not enabled by current features
                Ok(syn::Meta::List(syn::MetaList {
                    ref path,
                    ref nested,
                    ..
                })) if path.is_ident("cfg") => {
                    for n in nested {
                        match n {
                            syn::NestedMeta::Meta(syn::Meta::NameValue(syn::MetaNameValue {
                                path,
                                lit: syn::Lit::Str(feature),
                                ..
                            })) if path.is_ident("feature") => {
                                let feature_env_var = "CARGO_FEATURE_".to_owned()
                                    + &feature.value().to_uppercase().replace("-", "_");
                                if std::env::var_os(feature_env_var).is_none() {
                                    return;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }

        let mod_name = item.ident.to_string();
        let subdir = self.mod_dir.join(&mod_name);
        let subdir_mod = subdir.join("mod.rs");
        if subdir_mod.is_file() {
            return self
                .parse_mod(subdir_mod, subdir)
                .unwrap_or_else(|err| self.mod_error = Some(err));
        }

        let adjacent = self.mod_dir.join(&format!("{}.rs", mod_name));
        if adjacent.is_file() {
            return self
                .parse_mod(adjacent, subdir)
                .unwrap_or_else(|err| self.mod_error = Some(err));
        }

        panic!(
            "No file with module definition for `mod {}` in file {:?}",
            mod_name, self.current_path
        );
    }
}
