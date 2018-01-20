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

/// This macro is used to embed arbitrary C++ code. See the module level
/// documentation for more details.
#[macro_export]
macro_rules! cpp {
    ({$($body:tt)*}) => { /* Raw text inclusion */ };

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
/// ```ignore
/// cpp_class!(pub struct MyClass, "MyClass");
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
/// alignement as the the C++ class "MyClass". It will also call the destructor
/// of MyClass on drop, and its copy constructor on clone.
///
/// Warning: This only work if the C++ class can be moved in memory (using
/// memcpy). This disallow most classes from the standard library.
///
#[macro_export]
macro_rules! cpp_class {
    (struct $name:ident, $type:expr) => {
        #[derive(__cpp_internal_class)]
        #[repr(C)]
        struct $name {
            _opaque : [<$name as $crate::CppTrait>::BaseType ; <$name as $crate::CppTrait>::ARRAY_SIZE + (stringify!(struct $name, $type), 0).1]
        }
    };
    (pub struct $name:ident, $type:expr) => {
        #[derive(__cpp_internal_class)]
        #[repr(C)]
        pub struct $name {
            _opaque : [<$name as $crate::CppTrait>::BaseType ; <$name as $crate::CppTrait>::ARRAY_SIZE + (stringify!(pub struct $name, $type), 0).1]
        }
    };
}

