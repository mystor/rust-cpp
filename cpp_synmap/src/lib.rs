extern crate cpp_syn as syn;

use std::fs::File;
use std::io::prelude::*;
use std::io;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use syn::{Crate, Item, Attribute, ItemKind, Ident, MetaItem, Lit, LitKind, Span};
use syn::fold::{self, Folder};
use std::error;
use std::fmt;
use std::panic;

const FILE_PADDING_BYTES: usize = 100;

#[derive(Debug)]
pub enum Error {
    StringError(String),
    IOError(io::Error),
}
impl error::Error for Error {
    fn description(&self) -> &str {
        match *self {
            Error::StringError(ref s) => s,
            Error::IOError(ref err) => err.description(),
        }
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(error::Error::description(self), f)
    }
}
impl From<String> for Error {
    fn from(s: String) -> Error {
        Error::StringError(s)
    }
}
impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
        Error::IOError(err)
    }
}

#[derive(Debug)]
pub struct SourceMap {
    files: Vec<(Span, PathBuf)>,
    offset: usize,
}

impl SourceMap {
    pub fn new() -> SourceMap {
        SourceMap {
            files: Vec::new(),
            offset: 0,
        }
    }

    pub fn parse_file<P: AsRef<Path>>(&mut self, path: P) -> Result<Crate, Error> {
        // Load the original source file, and read it to a string
        let mut source = String::new();
        File::open(path.as_ref())?.read_to_string(&mut source)?;

        // Parse the source file as a crate
        let krate = syn::parse_crate(&source)?;

        // We've parsed it, register the file in the sourcemap
        let offset = self.offset;
        self.offset += source.len() + FILE_PADDING_BYTES;
        self.files.push((Span {
            lo: offset,
            hi: offset + source.len(),
        }, PathBuf::from(path.as_ref())));

        // Perform the fold
        let mut walker = Walker {
            offset: offset,
            working: path.as_ref().parent().unwrap().to_path_buf(),
            sourcemap: self,
        };

        panic::catch_unwind(panic::AssertUnwindSafe(|| walker.fold_crate(krate)))
            .map_err(|err| match err.downcast::<Error>() {
                Ok(err) => *err,
                Err(err) => panic::resume_unwind(err),
            })
    }

    fn get_path(&self, mut span: Span) -> (&Path, Span) {
        for &(fspan, ref p) in &self.files {
            if span.lo >= fspan.lo && span.lo <= fspan.hi {
                // Remove the offset
                span.lo -= fspan.lo;
                span.hi -= fspan.lo;
                // Set the path
                return (p, span);
            }
        }
        panic!("Given span is out of range");
    }

    pub fn get_src(&self, span: Span) -> String {
        let (path, span) = self.get_path(span);

        let mut file = File::open(&path).expect("Unable to open source file");
        file.seek(io::SeekFrom::Start(span.lo as u64)).expect("Unable to seek source file");

        let mut data = vec![0; span.hi - span.lo];
        file.read_exact(&mut data).expect("Unable to read span bytes");

        String::from_utf8(data).expect("Span is not utf-8")
    }

    pub fn get_line_no(&self, span: Span) -> (&Path, usize, usize) {
        let (path, span) = self.get_path(span);

        let mut file = BufReader::new(File::open(&path).expect("Unable to open source file"));

        let mut col = span.lo;
        let mut line = 1;
        let mut _dummy = Vec::new();
        loop {
            let amount = file.read_until(b'\n', &mut _dummy).unwrap();
            if col < amount {
                return (path, line, col);
            }
            col -= amount;
            line += 1;
        }
    }
}

struct Walker<'a> {
    offset: usize,
    working: PathBuf,
    sourcemap: &'a mut SourceMap
}

impl<'a> Walker<'a> {
    fn read_submodule(&mut self, path: &Path) -> Result<(Vec<Attribute>, Vec<Item>), Error> {
        let faux_crate = self.sourcemap.parse_file(path)?;
        if faux_crate.shebang.is_some() {
            return Err(format!("Submodules should not contain shebangs").into());
        }

        let Crate { attrs, items, .. } = faux_crate;
        Ok((attrs, items))
    }

    fn get_attrs_items(&mut self,
                       attrs: &[Attribute],
                       ident: &Ident)
                       -> Result<(Vec<Attribute>, Vec<Item>), Error> {
        // Determine the path of the inner module's file
        for attr in attrs {
            match attr.value {
                MetaItem::NameValue(ref id, Lit{ node: LitKind::Str(ref s, _), .. }) => {
                    if id.as_ref() == "path" {
                        let explicit = self.working.join(&s[..]);
                        return self.read_submodule(&explicit);
                    }
                }
                _ => {}
            }
        }

        let subdir = self.working.join(&format!("{}/mod.rs", ident));
        if subdir.is_file() {
            return self.read_submodule(&subdir);
        }

        let adjacent = self.working.join(&format!("{}.rs", ident));
        if adjacent.is_file() {
            return self.read_submodule(&adjacent);
        }

        Err(format!("No matching file with module definition for `mod {}`",
                    ident)
            .into())
    }
}

impl<'a> Folder for Walker<'a> {
    fn fold_item(&mut self, mut item: Item) -> Item {
        match item.node {
            ItemKind::Mod(None) => {
                // XXX: Handle errors better
                let (attrs, items) = match self.get_attrs_items(&item.attrs, &item.ident) {
                    Ok((attrs, items)) => (attrs, items),
                    Err(e) => panic!(e),
                };
                item.attrs.extend_from_slice(&attrs);
                item.node = ItemKind::Mod(Some(items));
                item
            }
            _ => fold::noop_fold_item(self, item),
        }
    }

    fn fold_span(&mut self, span: Span) -> Span {
        Span {
            lo: span.lo + self.offset,
            hi: span.hi + self.offset,
        }
    }
}
