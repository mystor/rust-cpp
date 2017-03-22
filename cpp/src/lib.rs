//! This crate `cpp` only provides a single macro, the `cpp!` macro. This macro
//! by itself is not useful, but when combined with the `cpp_build` and
//! `cpp_macro` crates it allows embedding arbitrary C++ code.
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
//! This crate must be used in tandem with the `cpp_build` and `cpp_macro`
//! crates. A basic Cargo project which uses these projects would have a
//! structure like the following:
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
//! cpp = "0.2"
//! cpp_macros = "0.2"
//!
//! [build-dependencies]
//! cpp_build = "0.2"
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
//! #[macro_use]
//! extern crate cpp_macros;
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

/// This macro is used to embed arbitrary C++ code. See the module level
/// documentation for more details.
#[macro_export]
macro_rules! cpp {
    ({$($body:tt)*}) => { /* Raw text inclusion */ };

    ([$($captures:tt)*] $($rest:tt)*) => {
        {
            #[allow(non_camel_case_types, dead_code)]
            #[derive(__cpp_internal_closure)]
            struct __cpp_closure(cpp! {
                @TYPE [$($captures)*] $($rest)*
            });
            cpp!{@CAPTURES __cpp_closure [] => $($captures)*}
        }
    };

    {@CAPTURES $name:ident
     [$($e:expr),*] =>
    } => {
        $name::run($($e),*)
    };

    {@CAPTURES $name:ident
     [$($e:expr),*] =>
     mut $i:ident as $cty:expr , $($rest:tt)*
    } => {
        cpp!{@CAPTURES $name [$($e ,)* &mut $i] => $($rest)*}
    };
    {@CAPTURES $name:ident
     [$($e:expr),*] =>
     mut $i:ident as $cty:expr
    } => {
        cpp!{@CAPTURES $name [$($e ,)* &mut $i] =>}
    };

    {@CAPTURES $name:ident
     [$($e:expr),*] =>
     $i:ident as $cty:expr , $($rest:tt)*
    } => {
        cpp!{@CAPTURES $name [$($e ,)* &$i] => $($rest)*}
    };
    {@CAPTURES $name:ident
     [$($e:expr),*] =>
     $i:ident as $cty:expr
    } => {
        cpp!{@CAPTURES $name [$($e ,)* &$i] =>}
    };

    (@TYPE $($rest:tt)*) => { () };
}
