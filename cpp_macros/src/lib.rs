//! This crate is the `cpp` procedural macro implementation. It is useless
//! without the companion crates `cpp`, and `cpp_build`.
//!
//! For more information, see the [`cpp` crate module level
//! documentation](https://docs.rs/cpp).

extern crate cpp_synom as synom;

extern crate cpp_syn as syn;

#[macro_use]
extern crate quote;

extern crate cpp_common;

extern crate proc_macro;

#[macro_use]
extern crate lazy_static;

extern crate aho_corasick;

extern crate byteorder;

use std::env;
use std::path::PathBuf;
use std::collections::HashMap;
use proc_macro::TokenStream;
use cpp_common::{parsing, LIB_NAME, MSVC_LIB_NAME, VERSION, flags};
use syn::Ident;

use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::fs::File;
use aho_corasick::{AcAutomaton, Automaton};
use byteorder::{LittleEndian, ReadBytesExt};

struct MetaData {
    size: usize,
    align: usize,
    flags: u64
}
impl MetaData {
    fn has_flag(&self, f : u32) -> bool {
        self.flags & (1 << f) != 0
    }
}

lazy_static! {
    static ref OUT_DIR: PathBuf =
        PathBuf::from(env::var("OUT_DIR").expect(r#"
-- rust-cpp fatal error --

The OUT_DIR environment variable was not set.
NOTE: rustc must be run by Cargo."#));

    static ref METADATA: HashMap<u64, Vec<MetaData>> = {
        let file = open_lib_file().expect(r#"
-- rust-cpp fatal error --

Failed to open the target library file.
NOTE: Did you make sure to add the rust-cpp build script?"#);

        read_metadata(file)
            .expect(r#"
-- rust-cpp fatal error --

I/O error while reading metadata from target library file."#)
    };
}

/// NOTE: This panics when it can produce a better error message
fn read_metadata(file: File) -> io::Result<HashMap<u64, Vec<MetaData>>> {
    let mut file = BufReader::new(file);
    let end = {
        const AUTO_KEYWORD: &'static [&'static [u8]] = &[&cpp_common::STRUCT_METADATA_MAGIC];
        let aut = AcAutomaton::new(AUTO_KEYWORD);
        let found = aut.stream_find(&mut file).next().expect(
            r#"
-- rust-cpp fatal error --

Struct metadata not present in target library file.
NOTE: Double-check that the version of cpp_build and cpp_macros match"#,
        )?;
        found.end
    };
    file.seek(SeekFrom::Start(end as u64))?;

    // Read & convert the version buffer into a string & compare with our
    // version.
    let mut version_buf = [0; 16];
    file.read(&mut version_buf)?;
    let version = version_buf
        .iter()
        .take_while(|b| **b != b'\0')
        .map(|b| *b as char)
        .collect::<String>();

    assert_eq!(
        version,
        VERSION,
        r#"
-- rust-cpp fatal error --

Version mismatch between cpp_macros and cpp_build for same crate."#
    );

    let length = file.read_u64::<LittleEndian>()?;
    let mut metadata = HashMap::new();
    for _ in 0..length {
        let hash = file.read_u64::<LittleEndian>()?;
        let size = file.read_u64::<LittleEndian>()? as usize;
        let align = file.read_u64::<LittleEndian>()? as usize;
        let flags = file.read_u64::<LittleEndian>()? as u64;

        metadata
            .entry(hash)
            .or_insert(Vec::new())
            .push(MetaData{size, align, flags});
    }
    Ok(metadata)
}

/// Try to open a file handle to the lib file. This is used to scan it for
/// metadata. We check both MSVC_LIB_NAME and LIB_NAME, in case we are on
/// or are targeting windows.
fn open_lib_file() -> io::Result<File> {
    if let Ok(file) = File::open(OUT_DIR.join(MSVC_LIB_NAME)) {
        Ok(file)
    } else {
        File::open(OUT_DIR.join(LIB_NAME))
    }
}

/// Strip tokens from the prefix and suffix of the source string, extracting the
/// original argument to the cpp! macro.
fn macro_text(mut source: &str) -> &str {
    #[cfg_attr(rustfmt, rustfmt_skip)]
    const PREFIX: &'static [&'static str] = &[
        "#", "[", "allow", "(", "unused", ")", "]",
        "enum", "CppClosureInput", "{",
        "Input", "=", "(", "stringify", "!", "("
    ];

    #[cfg_attr(rustfmt, rustfmt_skip)]
    const SUFFIX: &'static [&'static str] = &[
        ")", ",", "0", ")", ".", "1", ",", "}"
    ];

    source = source.trim();

    for token in PREFIX {
        assert!(
            source.starts_with(token),
            "expected prefix token {}, got {}",
            token,
            source
        );
        source = &source[token.len()..].trim();
    }

    for token in SUFFIX.iter().rev() {
        assert!(
            source.ends_with(token),
            "expected suffix token {}, got {}",
            token,
            source
        );
        source = &source[..source.len() - token.len()].trim();
    }

    source
}

#[proc_macro_derive(__cpp_internal_closure)]
pub fn expand_internal(input: TokenStream) -> TokenStream {
    assert_eq!(
        env!("CARGO_PKG_VERSION"),
        VERSION,
        "Internal Error: mismatched cpp_common and cpp_macros versions"
    );

    // Parse the macro input
    let source = input.to_string();
    let tokens = macro_text(&source);

    let closure = parsing::cpp_closure(synom::ParseState::new(tokens))
        .expect("cpp! macro")
        .sig;

    // Get the size data compiled by the build macro
    let size_data = METADATA.get(&closure.name_hash()).expect(
        r#"
-- rust-cpp fatal error --

This cpp! macro is not found in the library's rust-cpp metadata.
NOTE: Only cpp! macros found directly in the program source will be parsed -
NOTE: They cannot be generated by macro expansion."#,
    );

    let mut extern_params = Vec::new();
    let mut tt_args = Vec::new();
    let mut call_args = Vec::new();
    for (i, capture) in closure.captures.iter().enumerate() {
        let written_name = &capture.name;
        let mac_name: Ident = format!("$var_{}", written_name).into();
        let mac_cty: Ident = format!("$cty_{}", written_name).into();

        // Generate the assertion to check that the size and align of the types
        // match before calling.
        let MetaData { size, align, .. } = size_data[i + 1];
        let sizeof_msg = format!(
            "size_of for argument `{}` does not match between c++ and \
             rust",
            &capture.name
        );
        let alignof_msg = format!(
            "align_of for argument `{}` does not match between c++ and \
             rust",
            &capture.name
        );
        let assertion = quote!{
            // Perform a compile time check that the sizes match. This should be
            // a no-op.
            ::std::mem::forget(
                ::std::mem::transmute::<_, [u8; #size]>(
                    ::std::ptr::read(&#mac_name)));

            // NOTE: Both of these calls should be dead code in opt builds.
            assert!(::std::mem::size_of_val(&#mac_name) == #size,
                    #sizeof_msg);
            assert!(::std::mem::align_of_val(&#mac_name) == #align,
                    #alignof_msg);
        };

        let mb_mut = if capture.mutable {
            quote!(mut)
        } else {
            quote!()
        };
        let ptr = if capture.mutable {
            quote!(*mut)
        } else {
            quote!(*const)
        };

        let arg_name : Ident = format!("arg_{}", written_name).into();

        extern_params.push(quote!(#arg_name : #ptr u8));

        tt_args.push(quote!(#mb_mut #mac_name : ident as #mac_cty : tt));

        call_args.push(quote!({
            #assertion
            &#mb_mut #mac_name as #ptr _ as #ptr u8
        }));
    }

    let extern_name = closure.extern_name();
    let ret_ty = &closure.ret;
    let (ret_size, ret_align) = (size_data[0].size, size_data[0].align);
    let is_void = closure.cpp == "void";

    let decl = if is_void {
        quote! {
            fn #extern_name(#(#extern_params),*);
        }
    } else {
        quote! {
            fn #extern_name(#(#extern_params,)* _result: *mut #ret_ty);
        }
    };

    let call = if is_void {
        assert!(ret_size == 0, "`void` should have a size of 0!");
        quote! {
            #extern_name(#(#call_args),*);
            ::std::mem::transmute::<(), #ret_ty>(())
        }
    } else {
        quote! {
            let mut result: #ret_ty = ::std::mem::uninitialized();
            #extern_name(#(#call_args,)* &mut result);
            result
        }
    };

    let result = quote! {
        extern "C" {
            #decl
        }

        macro_rules! __cpp_closure_impl {
            (#(#tt_args),*) => {
                {
                    // Perform a compile time check that the sizes match.
                    ::std::mem::forget(
                        ::std::mem::transmute::<_, [u8; #ret_size]>(
                            ::std::mem::uninitialized::<#ret_ty>()));

                    // Perform a runtime check that the sizes match.
                    assert!(::std::mem::size_of::<#ret_ty>() == #ret_size);
                    assert!(::std::mem::align_of::<#ret_ty>() == #ret_align);

                    #call
                }
            }
        }
    };

    result.to_string().parse().unwrap()
}

#[proc_macro_derive(__cpp_internal_class)]
pub fn expand_wrap_class(input: TokenStream) -> TokenStream {
    let source = input.to_string();

    #[cfg_attr(rustfmt, rustfmt_skip)]
    const SUFFIX: &'static [&'static str] = &[
        ")", ",", "0", ")", ".", "1", "]", ",", "}"
    ];

    let s = source.find("stringify!(").expect("expected 'strignify!' token in class content") + 11;
    let mut tokens : &str = &source[s..].trim();

    for token in SUFFIX.iter().rev() {
        assert!(
            tokens.ends_with(token),
            "expected suffix token {}, got {}",
            token,
            tokens
        );
        tokens = &tokens[..tokens.len() - token.len()].trim();
    }

    let class = parsing::cpp_class(synom::ParseState::new(tokens))
        .expect("cpp_class! macro");

    let hash = class.name_hash();

    let size_data = METADATA.get(&hash).expect(
        r#"
-- rust-cpp fatal error --

This cpp_class! macro is not found in the library's rust-cpp metadata.
NOTE: Only cpp_class! macros found directly in the program source will be parsed -
NOTE: They cannot be generated by macro expansion."#,
    );

    let (size, align) = (size_data[0].size, size_data[0].align);

    let base_type = match align {
        1 => { quote!(u8) }
        2 => { quote!(u16) }
        4 => { quote!(u32) }
        8 => { quote!(u64) }
        _ => { panic!("unsupported alignment") }
    };

    let destructor_name : Ident = format!("__cpp_destructor_{}", hash).into();
    let copyctr_name : Ident = format!("__cpp_copy_{}", hash).into();
    let defaultctr_name : Ident = format!("__cpp_default_{}", hash).into();
    let class_name = class.name;

    let mut result = quote! {
        impl ::cpp::CppTrait for #class_name {
            type BaseType = #base_type;
            const ARRAY_SIZE: usize =  #size / #align;
            const CPP_TYPE: &'static str = stringify!(#class_name);
        }
    };
    if !size_data[0].has_flag(flags::IS_TRIVIALLY_DESTRUCTIBLE) {
        result = quote!{ #result
            impl Drop for #class_name {
                fn drop(&mut self) {
                    unsafe {
                        extern "C" { fn #destructor_name(_: *mut #class_name); }
                        #destructor_name(&mut *self);
                    }
                }
            }
        };
    };
    if size_data[0].has_flag(flags::IS_COPY_CONSTRUCTIBLE) {
        if !size_data[0].has_flag(flags::IS_TRIVIALLY_COPYABLE) {
            result = quote!{ #result
                impl Clone for #class_name {
                    fn clone(&self) -> Self {
                        unsafe {
                            extern "C" { fn #copyctr_name(src: *const #class_name, dst: *mut #class_name); }
                            let mut ret : Self = std::mem::uninitialized();
                            #copyctr_name(& *self, &mut ret);
                            ret
                        }
                    }
                }
            };
        } else {
            result = quote!{ #result
                impl Copy for #class_name { }
                impl Clone for #class_name {
                    fn clone(&self) -> Self { *self }
                }
            };
        };
    }
    if size_data[0].has_flag(flags::IS_DEFAULT_CONSTRUCTIBLE) {
        result = quote!{ #result
            impl Default for #class_name {
                fn default() -> Self {
                    unsafe {
                        extern "C" { fn #defaultctr_name(dst: *mut #class_name); }
                        let mut ret : Self = std::mem::uninitialized();
                        #defaultctr_name(&mut ret);
                        ret
                    }
                }
            }
        };
    }

    result.to_string().parse().unwrap()
}

