//! `synmap` provides utilities for parsing multi-file crates into `syn` AST
//! nodes, and resolving the spans attached to those nodes into raw source text,
//! and line/column information.
//!
//! The primary entry point for the crate is the `SourceMap` type, which stores
//! mappings from byte offsets to file information, along with cached file
//! information.

extern crate cpp_syn as syn;

extern crate memchr;

use std::fmt;
use std::error;
use std::fs::{self, File};
use std::io::prelude::*;
use std::io::{self, Error, ErrorKind};
use std::path::{Path, PathBuf};
use syn::{Crate, Item, Attribute, ItemKind, Ident, MetaItem, Lit, LitKind, Span};
use syn::fold::{self, Folder};

/// This constant controls the amount of padding which is created between
/// consecutive files' span ranges. It is non-zero to ensure that the low byte
/// offset of one file is not equal to the high byte offset of the previous
/// file.
const FILE_PADDING_BYTES: usize = 1;

/// Information regarding the on-disk location of a span of code.
/// This type is produced by `SourceMap::locinfo`.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Hash)]
pub struct LocInfo<'a> {
    pub path: &'a Path,
    pub line: usize,
    pub col: usize,
}

impl<'a> fmt::Display for LocInfo<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}:{}:{}", self.path.display(), self.line, self.col)
    }
}

#[derive(Debug)]
struct FileInfo {
    span: Span,
    path: PathBuf,
    src: String,
    lines: Vec<usize>
}

/// NOTE: This produces line and column. Line is 1-indexed, column is 0-indexed
fn offset_line_col(lines: &Vec<usize>, off: usize) -> (usize, usize) {
    match lines.binary_search(&off) {
        Ok(found) => (found + 1, 0),
        Err(idx) => (idx, off - lines[idx - 1]),
    }
}

fn lines_offsets(s: &[u8]) -> Vec<usize> {
    let mut lines = vec![0];
    let mut prev = 0;
    while let Some(len) = memchr::memchr(b'\n', &s[prev..]) {
        prev += len + 1;
        lines.push(prev);
    }
    lines
}

/// The `SourceMap` is the primary entry point for `synmap`. It maintains a
/// mapping between `Span` objects and the original source files they were
/// parsed from.
#[derive(Debug)]
pub struct SourceMap {
    files: Vec<FileInfo>,
    offset: usize,
}

impl SourceMap {
    /// Create a new `SourceMap` object with no files inside of it.
    pub fn new() -> SourceMap {
        SourceMap {
            files: Vec::new(),
            offset: 0,
        }
    }

    /// Read and parse the passed-in file path as a crate root, recursively
    /// parsing each of the submodules. Produces a syn `Crate` object with
    /// all submodules inlined.
    ///
    /// `Span` objects inside the resulting crate object are `SourceMap`
    /// relative, and should be interpreted by passing to the other methods on
    /// this type, such as `locinfo`, `source_text`, or `filename`.
    pub fn add_crate_root<P: AsRef<Path>>(&mut self, path: P) -> io::Result<Crate> {
        self.parse_canonical_file(fs::canonicalize(path)?)
    }

    /// This is an internal method which requires a canonical pathbuf as
    /// returned from a method like `fs::canonicalize`.
    fn parse_canonical_file(&mut self, path: PathBuf) -> io::Result<Crate> {
        // Parse the crate with syn
        let mut source = String::new();
        File::open(&path)?.read_to_string(&mut source)?;
        let krate = syn::parse_crate(&source)
            .map_err(|e| Error::new(ErrorKind::InvalidData, e))?;

        // Register the read-in file in the SourceMap
        let offset = self.offset;
        self.offset += source.len() + FILE_PADDING_BYTES;
        self.files.push(FileInfo {
            span: Span {
                lo: offset,
                hi: offset + source.len(),
            },
            path: path,
            lines: lines_offsets(source.as_bytes()),
            src: source,
        });

        // Walk the parsed Crate object, recursively filling in the bodies of
        // `mod` statements, and rewriting spans to be SourceMap-relative
        // instead of file-relative.
        let mut walker = Walker {
            idx: self.files.len() - 1,
            error: None,
            sm: self,
        };

        let krate = walker.fold_crate(krate);
        if let Some(err) = walker.error {
            return Err(err);
        }

        Ok(krate)
    }


    fn local_fileinfo(&self, mut span: Span) -> io::Result<(&FileInfo, Span)> {
        if span.lo > span.hi {
            return Err(Error::new(ErrorKind::InvalidInput,
                                  "Invalid span object with negative length"));
        }
        for fi in &self.files {
            if span.lo >= fi.span.lo && span.lo <= fi.span.hi &&
                span.hi >= fi.span.lo && span.hi <= fi.span.hi {
                // Remove the offset
                span.lo -= fi.span.lo;
                span.hi -= fi.span.lo;
                // Set the path
                return Ok((fi, span));
            }
        }
        Err(Error::new(ErrorKind::InvalidInput,
                       "Span is not part of any input file"))
    }

    /// Get the filename which contains the given span.
    ///
    /// Fails if the span is invalid or spans multiple source files.
    pub fn filename(&self, span: Span) -> io::Result<&Path> {
        Ok(&self.local_fileinfo(span)?.0.path)
    }

    /// Get the source text for the passed-in span.
    ///
    /// Fails if the span is invalid or spans multiple source files.
    pub fn source_text(&self, span: Span) -> io::Result<&str> {
        let (fi, span) = self.local_fileinfo(span)?;
        Ok(&fi.src[span.lo..span.hi])
    }

    /// Get a LocInfo object for the passed-in span, containing line, column,
    /// and file name information for the beginning and end of the span. The
    /// `path` field in the returned LocInfo struct will be a reference to a
    /// canonical path.
    ///
    /// Fails if the span is invalid or spans multiple source files.
    pub fn locinfo(&self, span: Span) -> io::Result<LocInfo> {
        let (fi, span) = self.local_fileinfo(span)?;

        let (line, col) = offset_line_col(&fi.lines, span.lo);
        Ok(LocInfo {
            path: &fi.path,
            line: line,
            col: col,
        })
    }
}

struct Walker<'a> {
    idx: usize,
    error: Option<Error>,
    sm: &'a mut SourceMap,
}

impl<'a> Walker<'a> {
    fn read_submodule(&mut self, path: PathBuf) -> io::Result<Crate> {
        let faux_crate = self.sm.parse_canonical_file(path)?;
        if faux_crate.shebang.is_some() {
            return Err(Error::new(ErrorKind::InvalidData,
                                  "Only the root file of a crate may contain shebangs"));
        }

        Ok(faux_crate)
    }

    fn get_attrs_items(&mut self,
                       attrs: &[Attribute],
                       ident: &Ident)
                       -> io::Result<Crate> {
        let parent = self.sm.files[self.idx].path.parent()
            .ok_or(Error::new(ErrorKind::InvalidInput,
                              "cannot parse file without parent directory"))?
            .to_path_buf();

        // Determine the path of the inner module's file
        for attr in attrs {
            match attr.value {
                MetaItem::NameValue(ref id, Lit{ node: LitKind::Str(ref s, _), .. }) => {
                    if id.as_ref() == "path" {
                        let explicit = parent.join(&s[..]);
                        return self.read_submodule(explicit);
                    }
                }
                _ => {}
            }
        }

        let subdir = parent.join(&format!("{}/mod.rs", ident));
        if subdir.is_file() {
            return self.read_submodule(subdir);
        }

        let adjacent = parent.join(&format!("{}.rs", ident));
        if adjacent.is_file() {
            return self.read_submodule(adjacent);
        }

        Err(Error::new(ErrorKind::NotFound,
                       format!("No file with module definition for `mod {}`", ident)))
    }
}

impl<'a> Folder for Walker<'a> {
    fn fold_item(&mut self, mut item: Item) -> Item {
        if self.error.is_some() {
            return item; // Early return to avoid extra work when erroring
        }

        match item.node {
            ItemKind::Mod(None) => {
                let (attrs, items) = match self.get_attrs_items(&item.attrs, &item.ident) {
                    Ok(Crate{ attrs, items, .. }) => (attrs, items),
                    Err(e) => {
                        // Get the file, line, and column information for the
                        // mod statement we're looking at.
                        let span = self.fold_span(item.span);
                        let loc = match self.sm.locinfo(span) {
                            Ok(li) => li.to_string(),
                            Err(_) => "unknown location".to_owned(),
                        };

                        let e = Error::new(ErrorKind::Other, ModParseErr {
                            err: e,
                            msg: format!("Error while parsing `mod {}` \
                                          statement at {}",
                                         item.ident, loc)
                        });
                        self.error = Some(e);
                        return item;
                    },
                };
                item.attrs.extend_from_slice(&attrs);
                item.node = ItemKind::Mod(Some(items));
                item
            }
            _ => fold::noop_fold_item(self, item),
        }
    }

    fn fold_span(&mut self, span: Span) -> Span {
        let offset = self.sm.files[self.idx].span.lo;
        Span {
            lo: span.lo + offset,
            hi: span.hi + offset,
        }
    }
}

/// This is an internal error which is used to build errors when parsing an
/// inner module fails.
#[derive(Debug)]
struct ModParseErr {
    err: Error,
    msg: String,
}
impl error::Error for ModParseErr {
    fn description(&self) -> &str {
        &self.msg
    }

    fn cause(&self) -> Option<&error::Error> {
        Some(&self.err)
    }
}
impl fmt::Display for ModParseErr {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.msg, f)?;
        fmt::Display::fmt(&self.err, f)
    }
}
