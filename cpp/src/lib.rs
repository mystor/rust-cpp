#![allow(clippy::needless_doctest_main)]
//! This crate `cpp` provides macros that allow embedding arbitrary C++ code.
//!
//! # Usage
//!
//! This crate must be used in tandem with the [`cpp_build`](https://docs.rs/cpp_build) crate. A basic Cargo
//! project which uses these projects would have a structure like the following:
//!
//! ```text
//! crate
//! |-- Cargo.toml
//! |-- src
//!     |-- main.rs
//! |-- build.rs
//! ```
//!
//! Where the files look like the following:
//!
//! #### Cargo.toml
//!
//! ```toml
//! [package]
//! build = "build.rs"
//!
//! [dependencies]
//! cpp = "0.5"
//!
//! [build-dependencies]
//! cpp_build = "0.5"
//! ```
//!
//! #### build.rs
//!
//! ```no_run
//! extern crate cpp_build;
//! fn main() {
//!     cpp_build::build("src/main.rs");
//! }
//! ```
//!
//! #### main.rs
//!
//! ```ignore
//! # // tested in test/src/examples.rs
//! use cpp::cpp;
//!
//! cpp!{{
//!     #include <iostream>
//! }}
//!
//! fn main() {
//!     let name = std::ffi::CString::new("World").unwrap();
//!     let name_ptr = name.as_ptr();
//!     let r = unsafe {
//!         cpp!([name_ptr as "const char *"] -> u32 as "int32_t" {
//!             std::cout << "Hello, " << name_ptr << std::endl;
//!             return 42;
//!         })
//!     };
//!     assert_eq!(r, 42)
//! }
//! ```
//!
//! # Build script
//!
//! Use the `cpp_build` crates from your `build.rs` script.
//! The same version of `cpp_build` and `cpp` crates should be used.
//! You can simply use the `cpp_build::build` function, or the `cpp_build::Config`
//! struct if you want more option.
//!
//! Behind the scene, it uses the `cc` crate.
//!
//! ## Using external libraries
//!
//! Most likely you will want to link against external libraries. You need to tell cpp_build
//! about the include path and other flags via `cpp_build::Config` and you need to let cargo
//! know about the link. More info in the [cargo docs](https://doc.rust-lang.org/cargo/reference/build-scripts.html).
//!
//! Your `build.rs` could look like this:
//!
//! ```no_run
//! fn main() {
//!     let include_path = "/usr/include/myexternallib";
//!     let lib_path = "/usr/lib/myexternallib";
//!     cpp_build::Config::new().include(include_path).build("src/lib.rs");
//!     println!("cargo:rustc-link-search={}", lib_path);
//!     println!("cargo:rustc-link-lib=myexternallib");
//! }
//! ```
//!
//! (But you probably want to allow to configure the path via environment variables or
//! find them using some external tool such as the `pkg-config` crate, instead of hardcoding
//! them in the source)
//!
//! # Limitations
//!
//! As with all procedure macro crates we also need to parse Rust source files to
//! extract C++ code. That leads to the fact that some of the language features
//! might not be supported in full. One example is the attributes. Only a limited
//! number of attributes is supported, namely: `#[path = "..."]` for `mod`
//! declarations to specify an alternative path to the module file and
//! `#[cfg(feature = "...")]` for `mod` declarations to conditionally include the
//! module into the parsing process. Please note that the latter is only supported
//! in its simplest form: straight-forward `feature = "..."` without any
//! additional conditions, `cfg!` macros are also not supported at the moment.
//!
//! Since the C++ code is included within a rust file, the C++ code must obey both
//! the Rust and the C++ lexing rules. For example, Rust supports nested block comments
//! (`/* ... /* ... */ ... */`) while C++ does not, so nested comments not be used in the
//! `cpp!` macro. Also the Rust lexer will not understand the C++ raw literal, nor all
//! the C++ escape sequences within literal, so only string literals that are both valid
//! in Rust and in C++ should be used. The same applies for group separators in numbers.
//! Be careful to properly use `#if` / `#else` / `#endif`, and not have unbalanced delimiters.

#![no_std]

#[macro_use]
#[allow(unused_imports)]
extern crate cpp_macros;
#[doc(hidden)]
pub use cpp_macros::*;

/// Internal macro which is used to locate the `rust!` invocations in the
/// C++ code embedded in `cpp!` invocation, to translate them into `extern`
/// functions
#[doc(hidden)]
#[macro_export]
macro_rules! __cpp_internal {
    (@find_rust_macro [$($a:tt)*] rust!($($rust_body:tt)*) $($rest:tt)*) => {
        $crate::__cpp_internal!{ @expand_rust_macro [$($a)*] $($rust_body)* }
        $crate::__cpp_internal!{ @find_rust_macro [$($a)*] $($rest)* }
    };
    (@find_rust_macro [$($a:tt)*] ( $($in:tt)* ) $($rest:tt)* ) =>
        { $crate::__cpp_internal!{ @find_rust_macro [$($a)*] $($in)* $($rest)* }  };
    (@find_rust_macro [$($a:tt)*] [ $($in:tt)* ] $($rest:tt)* ) =>
        { $crate::__cpp_internal!{ @find_rust_macro [$($a)*] $($in)* $($rest)* }  };
    (@find_rust_macro [$($a:tt)*] { $($in:tt)* } $($rest:tt)* ) =>
        { $crate::__cpp_internal!{ @find_rust_macro [$($a)*] $($in)* $($rest)* }  };
    (@find_rust_macro [$($a:tt)*] $t:tt $($rest:tt)*) =>
        { $crate::__cpp_internal!{ @find_rust_macro [$($a)*] $($rest)* } };
    (@find_rust_macro [$($a:tt)*]) => {};

    (@expand_rust_macro [$($a:tt)*] $i:ident [$($an:ident : $at:ty as $ac:tt),*] {$($body:tt)*}) => {
        #[allow(non_snake_case)]
        #[allow(unused_unsafe)]
        #[cfg_attr(feature = "cargo-clippy", allow(clippy::forget_copy))]
        #[cfg_attr(feature = "cargo-clippy", allow(clippy::forget_ref))]
        #[doc(hidden)]
        $($a)* unsafe extern "C" fn $i($($an : *const $at),*) {
            $(let $an : $at = unsafe { $an.read() };)*
            (|| { $($body)* })();
            $(::core::mem::forget($an);)*

        }
    };
    (@expand_rust_macro [$($a:tt)*] $i:ident [$($an:ident : $at:ty as $ac:tt),*] -> $rt:ty as $rc:tt {$($body:tt)*}) => {
        #[allow(non_snake_case)]
        #[allow(unused_unsafe)]
        #[cfg_attr(feature = "cargo-clippy", allow(clippy::forget_copy))]
        #[cfg_attr(feature = "cargo-clippy", allow(clippy::forget_ref))]
        #[doc(hidden)]
        $($a)* unsafe extern "C" fn $i($($an : *const $at, )* rt : *mut $rt) -> *mut $rt {

            $(let $an : $at = unsafe { $an.read() };)*
            {
                #[allow(unused_mut)]
                let mut lambda = || {$($body)*};
                unsafe { ::core::ptr::write(rt, lambda()) };
            }
            $(::core::mem::forget($an);)*
            rt
        }
    };

    (@expand_rust_macro $($invalid:tt)*) => {
        compile_error!(concat!( "Cannot parse rust! macro: ", stringify!([ $($invalid)* ]) ))
    };
}

/// This macro is used to embed arbitrary C++ code.
///
/// There are two variants of the `cpp!` macro. The first variant is used for
/// raw text inclusion. Text is included into the generated `C++` file in the
/// order which they were defined, inlining module declarations.
///
/// ```ignore
/// cpp! {{
///     #include <stdint.h>
///     #include <stdio.h>
/// }}
/// ```
///
/// The second variant is used to embed C++ code within Rust code. A list of
/// variable names which should be captured are taken as the first argument,
/// with their corresponding C++ type. The body is compiled as a C++ function.
///
/// This variant of the macro may only be invoked in expression context, and
/// requires an `unsafe` block, as it is performing FFI.
///
/// ```ignore
/// let y: i32 = 10;
/// let mut z: i32 = 20;
/// let x: i32 = unsafe { cpp!([y as "int32_t", mut z as "int32_t"] -> i32 as "int32_t" {
///     z++;
///     return y + z;
/// })};
/// ```
///
/// You can also put the unsafe keyword as the first keyword of the `cpp!` macro, which
/// has the same effect as putting the whole macro in an `unsafe` block:
///
/// ```ignore
/// let x: i32 = cpp!(unsafe [y as "int32_t", mut z as "int32_t"] -> i32 as "int32_t" {
///     z++;
///     return y + z;
/// });
/// ```
///
/// ## rust! pseudo-macro
///
/// The `cpp!` macro can contain, in the C++ code, a `rust!` sub-macro, which allows
/// the inclusion of Rust code in C++ code. This is useful to
/// implement callback or override virtual functions. Example:
///
/// ```ignore
/// trait MyTrait {
///    fn compute_value(&self, x : i32) -> i32;
/// }
///
/// cpp!{{
///    struct TraitPtr { void *a,*b; };
///    class MyClassImpl : public MyClass {
///      public:
///        TraitPtr m_trait;
///        int computeValue(int x) const override {
///            return rust!(MCI_computeValue [m_trait : &MyTrait as "TraitPtr", x : i32 as "int"]
///                -> i32 as "int" {
///                m_trait.compute_value(x)
///            });
///        }
///    }
/// }}
/// ```
///
/// The syntax for the `rust!` macro is:
/// ```ignore
/// rust!($uniq_ident:ident [$($arg_name:ident : $arg_rust_type:ty as $arg_c_type:tt),*]
///      $(-> $ret_rust_type:ty as $rust_c_type:tt)* {$($body:tt)*})
/// ```
/// `uniq_ident` is a unique identifier which will be used to name the `extern` function
#[macro_export]
macro_rules! cpp {
    // raw text inclusion
    ({$($body:tt)*}) => { $crate::__cpp_internal!{ @find_rust_macro [#[no_mangle] pub] $($body)*} };

    // inline closure
    ([$($captures:tt)*] $($rest:tt)*) => {
        {
            $crate::__cpp_internal!{ @find_rust_macro [] $($rest)*}
            #[allow(unused)]
            #[derive($crate::__cpp_internal_closure)]
            enum CppClosureInput {
                Input = (stringify!([$($captures)*] $($rest)*), 0).1
            }
            __cpp_closure_impl![$($captures)*]
        }
    };

    // wrap unsafe
    (unsafe $($tail:tt)*) => { unsafe { cpp!($($tail)*) } };
}

#[doc(hidden)]
pub trait CppTrait {
    type BaseType;
    const ARRAY_SIZE: usize;
    const CPP_TYPE: &'static str;
}

/// This macro allows wrapping a relocatable C++ struct or class that might have
/// a destructor or copy constructor, implementing the `Drop` and `Clone` trait
/// appropriately.
///
/// ```ignore
/// cpp_class!(pub unsafe struct MyClass as "MyClass");
/// impl MyClass {
///     fn new() -> Self {
///         unsafe { cpp!([] -> MyClass as "MyClass" { return MyClass(); }) }
///     }
///     fn member_function(&self, param : i32) -> i32 {
///         unsafe { cpp!([self as "const MyClass*", param as "int"] -> i32 as "int" {
///             return self->member_function(param);
///         }) }
///     }
/// }
/// ```
///
/// This will create a Rust struct `MyClass`, which has the same size and
/// alignment as the C++ class `MyClass`. It will also implement the `Drop` trait
/// calling the destructor, the `Clone` trait calling the copy constructor, if the
/// class is copyable (or `Copy` if it is trivially copyable), and `Default` if the class
/// is default constructible
///
/// ## Derived Traits
///
/// The `Default`, `Clone` and `Copy` traits are implicitly implemented if the C++
/// type has the corresponding constructors.
///
/// You can add the `#[derive(...)]` attribute in the macro in order to get automatic
/// implementation of the following traits:
///
/// * The trait `PartialEq` will call the C++ `operator==`.
/// * You can add the trait `Eq` if the semantics of the C++ operator are those of `Eq`
/// * The trait `PartialOrd` need the C++ `operator<` for that type. `lt`, `le`, `gt` and
///   `ge` will use the corresponding C++ operator if it is defined, otherwise it will
///   fallback to the less than operator. For PartialOrd::partial_cmp, the `operator<` will
///   be called twice. Note that it will never return `None`.
/// * The trait `Ord` can also be specified when the semantics of the `operator<` corresponds
///   to a total order
///
/// ## Safety Warning
///
/// Use of this macro is highly unsafe. Only certain C++ classes can be bound
/// to, C++ classes may perform arbitrary unsafe operations, and invariants are
/// easy to break.
///
/// A notable restriction is that this macro only works if the C++ class is
/// relocatable.
///
/// ## Relocatable classes
///
/// In order to be able to we wrapped the C++ class must be relocatable. That means
/// that it can be moved in memory using `memcpy`. This restriction exists because
/// safe Rust is allowed to move your types around.
///
/// Most C++ types which do not contain self-references will be compatible,
/// although this property cannot be statically checked by `rust-cpp`.
/// All types that satisfy `std::is_trivially_copyable` are compatible.
/// Maybe future version of the C++ standard would allow a comile-time check:
/// [P1144](http://www.open-std.org/jtc1/sc22/wg21/docs/papers/2019/p1144r4.html)
///
/// Unfortunately, as the STL often uses internal self-references for
/// optimization purposes, such as the small-string optimization, this disallows
/// most std:: classes.
/// But `std::unique_ptr<T>` and `std::shared_ptr<T>` works.
///
#[macro_export]
macro_rules! cpp_class {
    ($(#[$($attrs:tt)*])* unsafe struct $name:ident as $type:expr) => {
        $crate::__cpp_class_internal!{@parse [ $(#[$($attrs)*])* ] [] [unsafe struct $name as $type] }
    };
    ($(#[$($attrs:tt)*])* pub unsafe struct $name:ident as $type:expr) => {
        $crate::__cpp_class_internal!{@parse [ $(#[$($attrs)*])* ] [pub] [unsafe struct $name as $type] }
    };
    ($(#[$($attrs:tt)*])* pub($($pub:tt)*) unsafe struct $name:ident as $type:expr) => {
        $crate::__cpp_class_internal!{@parse [ $(#[$($attrs)*])* ] [pub($($pub)*)] [unsafe struct $name as $type] }
    };
}

/// Implementation details for cpp_class!
#[doc(hidden)]
#[macro_export]
macro_rules! __cpp_class_internal {
    (@parse [$($attrs:tt)*] [$($vis:tt)*] [unsafe struct $name:ident as $type:expr]) => {
        $crate::__cpp_class_internal!{@parse_attributes [ $($attrs)* ] [] [
            #[derive($crate::__cpp_internal_class)]
            #[repr(C)]
            $($vis)* struct $name {
                _opaque : [<$name as $crate::CppTrait>::BaseType ; <$name as $crate::CppTrait>::ARRAY_SIZE
                    + (stringify!($($attrs)* $($vis)* unsafe struct $name as $type), 0).1]
            }
        ]}
    };

    (@parse_attributes [] [$($attributes:tt)*] [$($result:tt)*]) => ( $($attributes)* $($result)* );
    (@parse_attributes [#[derive($($der:ident),*)] $($tail:tt)* ] [$($attributes:tt)*] [$($result:tt)*] )
        => ($crate::__cpp_class_internal!{@parse_derive [$($der),*] @parse_attributes [$($tail)*] [ $($attributes)* ] [ $($result)* ] } );
    (@parse_attributes [ #[$m:meta] $($tail:tt)* ] [$($attributes:tt)*] [$($result:tt)*])
        => ($crate::__cpp_class_internal!{@parse_attributes [$($tail)*] [$($attributes)* #[$m] ] [ $($result)* ] } );

    (@parse_derive [] @parse_attributes $($result:tt)*) => ($crate::__cpp_class_internal!{@parse_attributes $($result)*} );
    (@parse_derive [PartialEq $(,$tail:ident)*] $($result:tt)*)
        => ( $crate::__cpp_class_internal!{@parse_derive [$($tail),*] $($result)*} );
    (@parse_derive [PartialOrd $(,$tail:ident)*] $($result:tt)*)
        => ( $crate::__cpp_class_internal!{@parse_derive [$($tail),*] $($result)*} );
    (@parse_derive [Ord $(,$tail:ident)*] $($result:tt)*)
        => ( $crate::__cpp_class_internal!{@parse_derive [$($tail),*] $($result)*} );
    (@parse_derive [Default $(,$tail:ident)*] $($result:tt)*)
        => ( $crate::__cpp_class_internal!{@parse_derive [$($tail),*] $($result)*} );
    (@parse_derive [Clone $(,$tail:ident)*] $($result:tt)*)
        => ( $crate::__cpp_class_internal!{@parse_derive [$($tail),*] $($result)*} );
    (@parse_derive [Copy $(,$tail:ident)*] $($result:tt)*)
        => ( $crate::__cpp_class_internal!{@parse_derive [$($tail),*] $($result)*} );
    (@parse_derive [$i:ident $(,$tail:ident)*] @parse_attributes [$($attr:tt)*] [$($attributes:tt)*] [$($result:tt)*] )
        => ( $crate::__cpp_class_internal!{@parse_derive [$($tail),*] @parse_attributes [$($attr)*] [$($attributes)* #[derive($i)] ] [ $($result)* ] } );
}
