//! Fork of the equivalent file from the proc-macro2 file.
//! Modified to support line number counting in Cursor.
//! Also contains some function from stable.rs of proc_macro2.

#![allow(dead_code)] // Why is this needed ?

use std::str::{Bytes, CharIndices, Chars};

use unicode_xid::UnicodeXID;

#[derive(Debug)]
pub struct LexError {
    pub line: u32,
}

#[derive(Copy, Clone, Eq, PartialEq)]
pub struct Cursor<'a> {
    pub rest: &'a str,
    pub off: u32,
    pub line: u32,
    pub column: u32,
}

impl<'a> Cursor<'a> {
    #[allow(clippy::suspicious_map)]
    pub fn advance(&self, amt: usize) -> Cursor<'a> {
        let mut column_start: Option<usize> = None;
        Cursor {
            rest: &self.rest[amt..],
            off: self.off + (amt as u32),
            line: self.line
                + self.rest[..amt]
                    .char_indices()
                    .filter(|&(_, ref x)| *x == '\n')
                    .map(|(i, _)| { column_start = Some(i); } )
                    .count() as u32,
            column: match column_start {
                None => self.column + (amt as u32),
                Some(i) => (amt - i) as u32 - 1,
            },
        }
    }

    pub fn find(&self, p: char) -> Option<usize> {
        self.rest.find(p)
    }

    pub fn starts_with(&self, s: &str) -> bool {
        self.rest.starts_with(s)
    }

    pub fn is_empty(&self) -> bool {
        self.rest.is_empty()
    }

    pub fn len(&self) -> usize {
        self.rest.len()
    }

    pub fn as_bytes(&self) -> &'a [u8] {
        self.rest.as_bytes()
    }

    pub fn bytes(&self) -> Bytes<'a> {
        self.rest.bytes()
    }

    pub fn chars(&self) -> Chars<'a> {
        self.rest.chars()
    }

    pub fn char_indices(&self) -> CharIndices<'a> {
        self.rest.char_indices()
    }
}

pub type PResult<'a, O> = Result<(Cursor<'a>, O), LexError>;

pub fn whitespace(input: Cursor) -> PResult<()> {
    if input.is_empty() {
        return Err(LexError { line: input.line });
    }

    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let s = input.advance(i);
        if bytes[i] == b'/' {
            if s.starts_with("//")
            //                 && (!s.starts_with("///") || s.starts_with("////"))
            //                 && !s.starts_with("//!")
            {
                if let Some(len) = s.find('\n') {
                    i += len + 1;
                    continue;
                }
                break;
            } else if s.starts_with("/**/") {
                i += 4;
                continue;
            } else if s.starts_with("/*")
            //                 && (!s.starts_with("/**") || s.starts_with("/***"))
            //                 && !s.starts_with("/*!")
            {
                let (_, com) = block_comment(s)?;
                i += com.len();
                continue;
            }
        }
        match bytes[i] {
            b' ' | 0x09..=0x0d => {
                i += 1;
                continue;
            }
            b if b <= 0x7f => {}
            _ => {
                let ch = s.chars().next().unwrap();
                if is_whitespace(ch) {
                    i += ch.len_utf8();
                    continue;
                }
            }
        }
        return if i > 0 {
            Ok((s, ()))
        } else {
            Err(LexError { line: s.line })
        };
    }
    Ok((input.advance(input.len()), ()))
}

pub fn block_comment(input: Cursor) -> PResult<&str> {
    if !input.starts_with("/*") {
        return Err(LexError { line: input.line });
    }

    let mut depth = 0;
    let bytes = input.as_bytes();
    let mut i = 0;
    let upper = bytes.len() - 1;
    while i < upper {
        if bytes[i] == b'/' && bytes[i + 1] == b'*' {
            depth += 1;
            i += 1; // eat '*'
        } else if bytes[i] == b'*' && bytes[i + 1] == b'/' {
            depth -= 1;
            if depth == 0 {
                return Ok((input.advance(i + 2), &input.rest[..i + 2]));
            }
            i += 1; // eat '/'
        }
        i += 1;
    }
    Err(LexError { line: input.line })
}

pub fn skip_whitespace(input: Cursor) -> Cursor {
    match whitespace(input) {
        Ok((rest, _)) => rest,
        Err(_) => input,
    }
}

fn is_whitespace(ch: char) -> bool {
    // Rust treats left-to-right mark and right-to-left mark as whitespace
    ch.is_whitespace() || ch == '\u{200e}' || ch == '\u{200f}'
}

// --- functions from stable.rs

#[inline]
fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic()
        || c == '_'
        || (c > '\x7f' && UnicodeXID::is_xid_start(c))
}

#[inline]
fn is_ident_continue(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || c == '_'
        || (c > '\x7f' && UnicodeXID::is_xid_continue(c))
}

pub fn symbol(input: Cursor) -> PResult<&str> {
    let mut chars = input.char_indices();

    let raw = input.starts_with("r#");
    if raw {
        chars.next();
        chars.next();
    }

    match chars.next() {
        Some((_, ch)) if is_ident_start(ch) => {}
        _ => return Err(LexError { line: input.line }),
    }

    let mut end = input.len();
    for (i, ch) in chars {
        if !is_ident_continue(ch) {
            end = i;
            break;
        }
    }

    let a = &input.rest[..end];
    if a == "r#_" {
        Err(LexError { line: input.line })
    } else {
        let ident = if raw { &a[2..] } else { a };
        Ok((input.advance(end), ident))
    }
}

pub fn cooked_string(input: Cursor) -> PResult<()> {
    let mut chars = input.char_indices().peekable();
    while let Some((byte_offset, ch)) = chars.next() {
        match ch {
            '"' => {
                return Ok((input.advance(byte_offset), ()));
            }
            '\r' => {
                if let Some((_, '\n')) = chars.next() {
                    // ...
                } else {
                    break;
                }
            }
            '\\' => match chars.next() {
                Some((_, 'x')) => {
                    if !backslash_x_char(&mut chars) {
                        break;
                    }
                }
                Some((_, 'n')) | Some((_, 'r')) | Some((_, 't')) | Some((_, '\\'))
                | Some((_, '\'')) | Some((_, '"')) | Some((_, '0')) => {}
                Some((_, 'u')) => {
                    if !backslash_u(&mut chars) {
                        break;
                    }
                }
                Some((_, '\n')) | Some((_, '\r')) => {
                    while let Some(&(_, ch)) = chars.peek() {
                        if ch.is_whitespace() {
                            chars.next();
                        } else {
                            break;
                        }
                    }
                }
                _ => break,
            },
            _ch => {}
        }
    }
    Err(LexError { line: input.line })
}

pub fn cooked_byte_string(mut input: Cursor) -> PResult<()> {
    let mut bytes = input.bytes().enumerate();
    'outer: while let Some((offset, b)) = bytes.next() {
        match b {
            b'"' => {
                return Ok((input.advance(offset), ()));
            }
            b'\r' => {
                if let Some((_, b'\n')) = bytes.next() {
                    // ...
                } else {
                    break;
                }
            }
            b'\\' => match bytes.next() {
                Some((_, b'x')) => {
                    if !backslash_x_byte(&mut bytes) {
                        break;
                    }
                }
                Some((_, b'n')) | Some((_, b'r')) | Some((_, b't')) | Some((_, b'\\'))
                | Some((_, b'0')) | Some((_, b'\'')) | Some((_, b'"')) => {}
                Some((newline, b'\n')) | Some((newline, b'\r')) => {
                    let rest = input.advance(newline + 1);
                    for (offset, ch) in rest.char_indices() {
                        if !ch.is_whitespace() {
                            input = rest.advance(offset);
                            bytes = input.bytes().enumerate();
                            continue 'outer;
                        }
                    }
                    break;
                }
                _ => break,
            },
            b if b < 0x80 => {}
            _ => break,
        }
    }
    Err(LexError { line: input.line })
}

pub fn raw_string(input: Cursor) -> PResult<()> {
    let mut chars = input.char_indices();
    let mut n = 0;
    while let Some((byte_offset, ch)) = chars.next() {
        match ch {
            '"' => {
                n = byte_offset;
                break;
            }
            '#' => {}
            _ => return Err(LexError { line: input.line }),
        }
    }
    for (byte_offset, ch) in chars {
        match ch {
            '"' if input.advance(byte_offset + 1).starts_with(&input.rest[..n]) => {
                let rest = input.advance(byte_offset + 1 + n);
                return Ok((rest, ()));
            }
            '\r' => {}
            _ => {}
        }
    }
    Err(LexError { line: input.line })
}

pub fn cooked_byte(input: Cursor) -> PResult<()> {
    let mut bytes = input.bytes().enumerate();
    let ok = match bytes.next().map(|(_, b)| b) {
        Some(b'\\') => match bytes.next().map(|(_, b)| b) {
            Some(b'x') => backslash_x_byte(&mut bytes),
            Some(b'n') | Some(b'r') | Some(b't') | Some(b'\\') | Some(b'0') | Some(b'\'')
            | Some(b'"') => true,
            _ => false,
        },
        b => b.is_some(),
    };
    if ok {
        match bytes.next() {
            Some((offset, _)) => {
                if input.chars().as_str().is_char_boundary(offset) {
                    Ok((input.advance(offset), ()))
                } else {
                    Err(LexError { line: input.line })
                }
            }
            None => Ok((input.advance(input.len()), ())),
        }
    } else {
        Err(LexError { line: input.line })
    }
}

pub fn cooked_char(input: Cursor) -> PResult<()> {
    let mut chars = input.char_indices();
    let ok = match chars.next().map(|(_, ch)| ch) {
        Some('\\') => match chars.next().map(|(_, ch)| ch) {
            Some('x') => backslash_x_char(&mut chars),
            Some('u') => backslash_u(&mut chars),
            Some('n') | Some('r') | Some('t') | Some('\\') | Some('0') | Some('\'') | Some('"') => {
                true
            }
            _ => false,
        },
        ch => ch.is_some(),
    };
    if ok {
        match chars.next() {
            Some((idx, _)) => Ok((input.advance(idx), ())),
            None => Ok((input.advance(input.len()), ())),
        }
    } else {
        Err(LexError { line: input.line })
    }
}

macro_rules! next_ch {
    ($chars:ident @ $pat:pat $(| $rest:pat)*) => {
        match $chars.next() {
            Some((_, ch)) => match ch {
                $pat $(| $rest)*  => ch,
                _ => return false,
            },
            None => return false
        }
    };
}

fn backslash_x_char<I>(chars: &mut I) -> bool
where
    I: Iterator<Item = (usize, char)>,
{
    next_ch!(chars @ '0'..='7');
    next_ch!(chars @ '0'..='9' | 'a'..='f' | 'A'..='F');
    true
}

fn backslash_x_byte<I>(chars: &mut I) -> bool
where
    I: Iterator<Item = (usize, u8)>,
{
    next_ch!(chars @ b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F');
    next_ch!(chars @ b'0'..=b'9' | b'a'..=b'f' | b'A'..=b'F');
    true
}

fn backslash_u<I>(chars: &mut I) -> bool
where
    I: Iterator<Item = (usize, char)>,
{
    next_ch!(chars @ '{');
    next_ch!(chars @ '0'..='9' | 'a'..='f' | 'A'..='F');
    loop {
        let c = next_ch!(chars @ '0'..='9' | 'a'..='f' | 'A'..='F' | '_' | '}');
        if c == '}' {
            return true;
        }
    }
}
