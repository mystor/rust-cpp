//! This crate `cpp` only provides a single macro, the `cpp!` macro. This macro
//! by itself is not useful, but when combined with the `cpp_build` crate it
//! allows embedding arbitrary C++ code.
//!
//! There are two variants of the `cpp!` macro. The first variant is used for
//! raw text inclusion. Text is included into the generated `C++` file in the
//! order which they were defined, inlining module declarations.
//!
//! ```ignore
//! cpp! {{
//!     #include <stdint.h>
//!     #include <stdio.h>
//! }}
//! ```
//!
//! The second variant is used to embed C++ code within rust code. A list of
//! variable names which should be captured are taken as the first argument,
//! with their corresponding C++ type. The body is compiled as a C++ function.
//!
//! This variant of the macro may only be invoked in expression context, and
//! requires an `unsafe` block, as it is performing FFI.
//!
//! ```ignore
//! let y: i32 = 10;
//! let mut z: i32 = 20;
//! let x: i32 = cpp!([y as "int32_t", mut z as "int32_t"] -> i32 as "int32_t" {
//!     z++;
//!     return y + z;
//! });
//! ```
//!
//! ## rust! pseudo-macro
//!
//! The first variant of the cpp! macro can contain, in the C++ code, a rust! sub-
//! macro, which allows to include rust code in C++ code. This is useful to
//! implement callback or override virtual functions. Example:
//!
//! ```ignore
//! trait MyTrait {
//!    fn compute_value(&self, x : i32) -> i32;
//! }
//!
//! cpp!{{
//!    struct TraitPtr { void *a,*b; };
//!    class MyClassImpl : public MyClass {
//!      public:
//!        TraitPtr m_trait;
//!        int computeValue(int x) const override {
//!            return rust!(MCI_computeValue [m_trait : &MyTrait as "TraitPtr", x : i32 as "int"]
//!                -> i32 as "int" {
//!                m_trait.compute_value(x)
//!            });
//!        }
//!    }
//! }}
//! ```
//!
//! The syntax for the rust! macro is:
//! ```ignore
//! rust!($uniq_ident:ident [$($arg_name:ident : $arg_rust_type:ty as $arg_c_type:tt),*]
//!      $(-> $ret_rust_type:ty as $rust_c_type:tt)* {$($body:tt)*})
//! ```
//! uniq_ident is an unique identifier which will be used to name the extern function
//!
//! # Usage
//!
//! This crate must be used in tandem with the `cpp_build` crate. A basic Cargo
//! project which uses these projects would have a structure like the following:
//!
//! ```text
//! crate
//! |-- Cargo.toml
//! |-- src
//!     |-- lib.rs
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
//! cpp = "0.3"
//!
//! [build-dependencies]
//! cpp_build = "0.3"
//! ```
//!
//! #### build.rs
//!
//! ```ignore
//! extern crate cpp_build;
//!
//! fn main() {
//!     cpp_build::build("src/lib.rs");
//! }
//! ```
//!
//! #### lib.rs
//!
//! ```ignore
//! #[macro_use]
//! extern crate cpp;
//!
//! cpp!{{
//!     #include <stdio.h>
//! }}
//!
//! fn main() {
//!     unsafe {
//!         cpp!([] {
//!             printf("Hello, World!\n");
//!         });
//!     }
//! }
//! ```

#[macro_use]
#[allow(unused_imports)]
extern crate cpp_macros;
#[doc(hidden)]
pub use cpp_macros::*;

/// Internal macro which is used to locate the rust! invocations in the
/// C++ code embbeded in cpp! invocation, to translate them into extern
/// functions
#[doc(hidden)]
#[macro_export]
macro_rules! __cpp_internal {
    (@find_rust_macro rust!($($rust_body:tt)*) $($rest:tt)*) => {
        __cpp_internal!{ @expand_rust_macro $($rust_body)* }
        __cpp_internal!{ @find_rust_macro $($rest)* }
    };
    (@find_rust_macro ( $($in:tt)* ) $($rest:tt)* ) =>
        { __cpp_internal!{ @find_rust_macro $($in)* $($rest)* }  };
    (@find_rust_macro [ $($in:tt)* ] $($rest:tt)* ) =>
        { __cpp_internal!{ @find_rust_macro $($in)* $($rest)* }  };
    (@find_rust_macro { $($in:tt)* } $($rest:tt)* ) =>
        { __cpp_internal!{ @find_rust_macro $($in)* $($rest)* }  };
    (@find_rust_macro $t:tt $($rest:tt)*) =>
        { __cpp_internal!{ @find_rust_macro $($rest)* } };
    (@find_rust_macro) => {};

    (@expand_rust_macro $i:ident [$($an:ident : $at:ty as $ac:tt),*] {$($body:tt)*}) => {
        #[no_mangle]
        #[doc(hidden)]
        pub extern "C" fn $i($($an : *const $at),*) {
            $(let $an : $at = unsafe { $an.read() };)*
            { $($body)* }
            $(::std::mem::forget($an);)*

        }
    };
    (@expand_rust_macro $i:ident [$($an:ident : $at:ty as $ac:tt),*] -> $rt:ty as $rc:tt {$($body:tt)*}) => {
        #[no_mangle]
        #[doc(hidden)]
        pub extern "C" fn $i($($an : *const $at, )* rt : *mut $rt) -> *mut $rt {
            $(let $an : $at = unsafe { $an.read() };)*
            {
                #[allow(unused_mut)]
                let mut lambda = || {$($body)*};
                unsafe { std::ptr::write(rt, lambda()) };
            }
            $(::std::mem::forget($an);)*
            rt
        }
    };

    (@expand_rust_macro $($invalid:tt)*) => {
        compile_error!(concat!( "Cannot parse rust! macro: ", stringify!([ $($invalid)* ]) ))
    };
}

/// This macro is used to embed arbitrary C++ code. See the module level
/// documentation for more details.
#[macro_export]
macro_rules! cpp {
    // raw text inclusion
    ({$($body:tt)*}) => { __cpp_internal!{ @find_rust_macro $($body)*} };

    // inline closure
    ([$($captures:tt)*] $($rest:tt)*) => {
        {
            #[allow(unused)]
            #[derive(__cpp_internal_closure)]
            enum CppClosureInput {
                Input = (stringify!([$($captures)*] $($rest)*), 0).1
            }
            __cpp_closure_impl![$($captures)*]
        }
    };
}

#[doc(hidden)]
pub trait CppTrait {
    type BaseType;
    const ARRAY_SIZE: usize;
    const CPP_TYPE: &'static str;
}

/// This macro allow to wrap a relocatable C++ struct or class that might
/// have destructor or copy constructor, and instantiate the Drop and Clone
/// trait appropriately.
///
/// Warning: This only work if the C++ class that are relocatable, i.e., that
/// can be moved in memory using memmove.
/// This disallows most classes from the standard library.
/// This restriction exists because rust is allowed to move your types around.
/// Most C++ types that do not contain self-references or
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
/// This will create a rust struct MyClass, which has the same size and
/// alignment as the the C++ class "MyClass". It will also implement the Drop trait
/// calling the destructor, the Clone trait calling the copy constructor, if the
/// class is copyable (or Copy if it is trivially copyable), and Default if the class
/// is default constructible
///
#[macro_export]
macro_rules! cpp_class {
    (unsafe struct $name:ident as $type:expr) => {
        #[derive(__cpp_internal_class)]
        #[repr(C)]
        struct $name {
            _opaque : [<$name as $crate::CppTrait>::BaseType ; <$name as $crate::CppTrait>::ARRAY_SIZE
                + (stringify!(unsafe struct $name as $type), 0).1]
        }
    };
    (pub unsafe struct $name:ident as $type:expr) => {
        #[derive(__cpp_internal_class)]
        #[repr(C)]
        pub struct $name {
            _opaque : [<$name as $crate::CppTrait>::BaseType ; <$name as $crate::CppTrait>::ARRAY_SIZE
                + (stringify!(pub unsafe struct $name as $type), 0).1]
        }
    };
}

