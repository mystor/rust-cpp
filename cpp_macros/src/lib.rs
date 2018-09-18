//! This crate is the `cpp` procedural macro implementation. It is useless
//! without the companion crates `cpp`, and `cpp_build`.
//!
//! For more information, see the [`cpp` crate module level
//! documentation](https://docs.rs/cpp).
#![recursion_limit = "128"]

#[macro_use]
extern crate syn;

#[macro_use]
extern crate quote;

extern crate cpp_common;

extern crate proc_macro;
extern crate proc_macro2;
use proc_macro2::Span;

#[macro_use]
extern crate lazy_static;

extern crate aho_corasick;

extern crate byteorder;

use cpp_common::{flags, kw, RustInvocation, FILE_HASH, LIB_NAME, MSVC_LIB_NAME, OUT_DIR, VERSION};
use std::collections::HashMap;
use std::iter::FromIterator;
use syn::parse::Parser;
use syn::Ident;

use aho_corasick::{AcAutomaton, Automaton};
use byteorder::{LittleEndian, ReadBytesExt};
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};

struct MetaData {
    size: usize,
    align: usize,
    flags: u64,
}
impl MetaData {
    fn has_flag(&self, f: u32) -> bool {
        self.flags & (1 << f) != 0
    }
}

lazy_static! {
    static ref METADATA: HashMap<u64, Vec<MetaData>> = {
        let file = open_lib_file().expect(
            r#"
-- rust-cpp fatal error --

Failed to open the target library file.
NOTE: Did you make sure to add the rust-cpp build script?"#,
        );

        read_metadata(file).expect(
            r#"
-- rust-cpp fatal error --

I/O error while reading metadata from target library file."#,
        )
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
        version, VERSION,
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
            .push(MetaData { size, align, flags });
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

fn find_all_rust_macro(
    input: syn::parse::ParseStream,
) -> Result<Vec<RustInvocation>, syn::parse::Error> {
    let mut r = Vec::<RustInvocation>::new();
    while !input.is_empty() {
        if input.peek(kw::rust) {
            if let Ok(ri) = input.parse::<RustInvocation>() {
                r.push(ri);
            }
        } else if input.peek(syn::token::Brace) {
            let c;
            braced!(c in input);
            r.extend(find_all_rust_macro(&c)?);
        } else if input.peek(syn::token::Paren) {
            let c;
            parenthesized!(c in input);
            r.extend(find_all_rust_macro(&c)?);
        } else if input.peek(syn::token::Bracket) {
            let c;
            bracketed!(c in input);
            r.extend(find_all_rust_macro(&c)?);
        } else {
            input.parse::<proc_macro2::TokenTree>()?;
        }
    }
    return Ok(r);
}

/// Find the occurence of the strignify! macro within the macro derive
fn extract_original_macro(input: &syn::DeriveInput) -> Option<proc_macro2::TokenStream> {
    #[derive(Default)]
    struct Finder(Option<proc_macro2::TokenStream>);
    impl<'ast> syn::visit::Visit<'ast> for Finder {
        fn visit_macro(&mut self, mac: &'ast syn::Macro) {
            if mac.path.segments.len() == 1 && mac.path.segments[0].ident == "stringify" {
                self.0 = Some(mac.tts.clone());
            }
        }
    }
    let mut f = Finder::default();
    syn::visit::visit_derive_input(&mut f, &input);
    f.0
}

#[proc_macro_derive(__cpp_internal_closure)]
pub fn expand_internal(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    assert_eq!(
        env!("CARGO_PKG_VERSION"),
        VERSION,
        "Internal Error: mismatched cpp_common and cpp_macros versions"
    );

    // Parse the macro input
    let input = extract_original_macro(&parse_macro_input!(input as syn::DeriveInput)).unwrap();

    let closure = match syn::parse2::<cpp_common::Closure>(input) {
        Ok(x) => x,
        Err(err) => return err.to_compile_error().into(),
    };

    // Get the size data compiled by the build macro
    let size_data = match METADATA.get(&closure.sig.name_hash()) {
        Some(x) => x,
        None => {
            return quote!(compile_error!{
r#"This cpp! macro is not found in the library's rust-cpp metadata.
NOTE: Only cpp! macros found directly in the program source will be parsed -
NOTE: They cannot be generated by macro expansion."#})
                .into()
        }
    };

    let mut extern_params = Vec::new();
    let mut tt_args = Vec::new();
    let mut call_args = Vec::new();
    for (i, capture) in closure.sig.captures.iter().enumerate() {
        let written_name = &capture.name;
        let span = written_name.span();
        let mac_name = Ident::new(&format!("var_{}", written_name), span);
        let mac_cty = Ident::new(&format!("cty_{}", written_name), span);

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
        let assertion = quote_spanned!{span=>
            // Perform a compile time check that the sizes match. This should be
            // a no-op.
            ::std::mem::forget(
                ::std::mem::transmute::<_, [u8; #size]>(
                    ::std::ptr::read(&$#mac_name)));

            // NOTE: Both of these calls should be dead code in opt builds.
            assert!(::std::mem::size_of_val(&$#mac_name) == #size,
                    #sizeof_msg);
            assert!(::std::mem::align_of_val(&$#mac_name) == #align,
                    #alignof_msg);
        };

        let mb_mut = if capture.mutable {
            quote_spanned!(span=> mut)
        } else {
            quote!()
        };
        let ptr = if capture.mutable {
            quote_spanned!(span=> *mut)
        } else {
            quote_spanned!(span=> *const)
        };

        let arg_name = Ident::new(&format!("arg_{}", written_name), span);

        extern_params.push(quote_spanned!(span=> #arg_name : #ptr u8));

        tt_args.push(quote_spanned!(span=> #mb_mut $#mac_name : ident as $#mac_cty : tt));

        call_args.push(quote_spanned!(span=> {
            #assertion
            &#mb_mut $#mac_name as #ptr _ as #ptr u8
        }));
    }

    let extern_name = closure.sig.extern_name();
    let ret_ty = &closure.sig.ret;
    let MetaData {
        size: ret_size,
        align: ret_align,
        flags,
    } = size_data[0];
    let is_void = closure.sig.cpp == "void";

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
            ::std::mem::transmute::<(), (#ret_ty)>(())
        }
    } else {
        quote!{
            let mut result: #ret_ty = ::std::mem::uninitialized();
            #extern_name(#(#call_args,)* &mut result);
            result
        }
    };

    let input = proc_macro2::TokenStream::from_iter([closure.body].iter().map(|x| x.clone()));
    let rust_invocations = find_all_rust_macro.parse2(input).expect("rust! macro");
    let init_callbacks = if !rust_invocations.is_empty() {
        let rust_cpp_callbacks = Ident::new(
            &format!("rust_cpp_callbacks{}", *FILE_HASH),
            Span::call_site(),
        );
        let offset = (flags >> 32) as isize;
        let callbacks: Vec<Ident> = rust_invocations.iter().map(|x| x.id.clone()).collect();
        quote! {
            use ::std::sync::{Once, ONCE_INIT};
            static INIT_INVOCATIONS: Once = ONCE_INIT;
            INIT_INVOCATIONS.call_once(|| {
                // #rust_cpp_callbacks is in fact an array. Since we cannot represent it in rust,
                // we just are gonna take the pointer to it can offset from that.
                extern "C" {
                    #[no_mangle]
                    static mut #rust_cpp_callbacks: *const ::std::os::raw::c_void;
                }
                let callbacks_array : *mut *const ::std::os::raw::c_void = &mut #rust_cpp_callbacks;
                let mut offset = #offset;
                #(
                    offset += 1;
                    *callbacks_array.offset(offset - 1) = #callbacks as *const ::std::os::raw::c_void;
                )*
            });
        }
    } else {
        quote!()
    };

    let result = quote! {
        extern "C" {
            #decl
        }

        macro_rules! __cpp_closure_impl {
            (#(#tt_args),*) => {
                {
                    #init_callbacks

                    // Perform a compile time check that the sizes match.
                    ::std::mem::forget(
                        ::std::mem::transmute::<_, [u8; #ret_size]>(
                            ::std::mem::uninitialized::<(#ret_ty)>()));

                    // Perform a runtime check that the sizes match.
                    assert!(::std::mem::size_of::<(#ret_ty)>() == #ret_size);
                    assert!(::std::mem::align_of::<(#ret_ty)>() == #ret_align);

                    #call
                }
            }
        }
    };

    result.into()
}

#[proc_macro_derive(__cpp_internal_class)]
pub fn expand_wrap_class(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    // Parse the macro input
    let input = extract_original_macro(&parse_macro_input!(input as syn::DeriveInput)).unwrap();

    let class = match ::syn::parse2::<cpp_common::Class>(input) {
        Ok(x) => x,
        Err(err) => return err.to_compile_error().into(),
    };

    let hash = class.name_hash();

    // Get the size data compiled by the build macro
    let size_data = match METADATA.get(&hash) {
        Some(x) => x,
        None => {
            return quote!(compile_error!{
r#"This cpp_class! macro is not found in the library's rust-cpp metadata.
NOTE: Only cpp_class! macros found directly in the program source will be parsed -
NOTE: They cannot be generated by macro expansion."#})
                .into()
        }
    };

    let (size, align) = (size_data[0].size, size_data[0].align);

    let base_type = match align {
        1 => quote!(u8),
        2 => quote!(u16),
        4 => quote!(u32),
        8 => quote!(u64),
        _ => panic!("unsupported alignment"),
    };

    let destructor_name = Ident::new(&format!("__cpp_destructor_{}", hash), Span::call_site());
    let copyctr_name = Ident::new(&format!("__cpp_copy_{}", hash), Span::call_site());
    let defaultctr_name = Ident::new(&format!("__cpp_default_{}", hash), Span::call_site());
    let class_name = class.name.clone();

    let mut result = quote! {
        #[doc(hidden)]
        impl ::cpp::CppTrait for #class_name {
            type BaseType = #base_type;
            const ARRAY_SIZE: usize =  #size / #align;
            const CPP_TYPE: &'static str = stringify!(#class_name);
        }
    };
    if !size_data[0].has_flag(flags::IS_TRIVIALLY_DESTRUCTIBLE) {
        result = quote!{ #result
            impl ::std::ops::Drop for #class_name {
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
        if !size_data[0].has_flag(flags::IS_TRIVIALLY_COPYABLE) && !class.derives("Copy") {
            result = quote!{ #result
                impl ::std::clone::Clone for #class_name {
                    fn clone(&self) -> Self {
                        unsafe {
                            extern "C" { fn #copyctr_name(src: *const #class_name, dst: *mut #class_name); }
                            let mut ret : Self = ::std::mem::uninitialized();
                            #copyctr_name(& *self, &mut ret);
                            ret
                        }
                    }
                }
            };
        } else {
            result = quote!{ #result
                impl ::std::marker::Copy for #class_name { }
                impl ::std::clone::Clone for #class_name {
                    fn clone(&self) -> Self { *self }
                }
            };
        };
    } else if class.derives("Clone") {
        panic!("C++ class is not copyable");
    }

    if size_data[0].has_flag(flags::IS_DEFAULT_CONSTRUCTIBLE) {
        result = quote!{ #result
            impl ::std::default::Default for #class_name {
                fn default() -> Self {
                    unsafe {
                        extern "C" { fn #defaultctr_name(dst: *mut #class_name); }
                        let mut ret : Self = ::std::mem::uninitialized();
                        #defaultctr_name(&mut ret);
                        ret
                    }
                }
            }
        };
    } else if class.derives("Default") {
        panic!("C++ class is not default constructible");
    }

    if class.derives("PartialEq") {
        let equal_name = Ident::new(&format!("__cpp_equal_{}", hash), Span::call_site());
        result = quote!{ #result
            impl ::std::cmp::PartialEq for #class_name {
                fn eq(&self, other: &#class_name) -> bool {
                    unsafe {
                        extern "C" { fn #equal_name(a: *const #class_name, b: *const #class_name) -> bool; }
                        #equal_name(& *self, other)
                    }
                }
            }
        };
    }
    if class.derives("PartialOrd") {
        let compare_name = Ident::new(&format!("__cpp_compare_{}", hash), Span::call_site());
        let f = |func, cmp| {
            quote!{
                fn #func(&self, other: &#class_name) -> bool {
                    unsafe {
                        extern "C" { fn #compare_name(a: *const #class_name, b: *const #class_name, cmp : i32) -> i32; }
                        #compare_name(& *self, other, #cmp) != 0
                    }
                }
            }
        };
        let lt = f(quote!{lt}, -2);
        let gt = f(quote!{gt}, 2);
        let le = f(quote!{le}, -1);
        let ge = f(quote!{ge}, 1);
        result = quote!{ #result
            impl ::std::cmp::PartialOrd for #class_name {
                #lt #gt #le #ge

                fn partial_cmp(&self, other: &#class_name) -> ::std::option::Option<::std::cmp::Ordering> {
                    use ::std::cmp::Ordering;
                    unsafe {
                        extern "C" { fn #compare_name(a: *const #class_name, b: *const #class_name, cmp : i32) -> i32; }
                        ::std::option::Option::Some(match #compare_name(& *self, other, 0) {
                            -1 => Ordering::Less,
                            0 => Ordering::Equal,
                            1 => Ordering::Greater,
                            _ => panic!()
                        })
                    }
                }
            }
        };
    }
    if class.derives("Ord") {
        let compare_name = Ident::new(&format!("__cpp_compare_{}", hash), Span::call_site());
        result = quote!{ #result
            impl ::std::cmp::Ord for #class_name {
                fn cmp(&self, other: &#class_name) -> ::std::cmp::Ordering {
                    unsafe {
                        use ::std::cmp::Ordering;
                        extern "C" { fn #compare_name(a: *const #class_name, b: *const #class_name, cmp : i32) -> i32; }
                        match #compare_name(& *self, other, 0) {
                            -1 => Ordering::Less,
                            0 => Ordering::Equal,
                            1 => Ordering::Greater,
                            _ => panic!()
                        }
                    }
                }
            }
        };
    }

    if class.derives("Hash") {
        panic!("Deriving from Hash is not implemented")
    };
    if class.derives("Debug") {
        panic!("Deriving from Debug is not implemented")
    };

    result.into()
}
