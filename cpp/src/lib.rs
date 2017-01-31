//! This crate `cpp` only provides a single macro, the `cpp!` macro. This macro by itself
//! is not useful, but when combined with the `cpp_build` and `cpp_macro` crate provides
//! the entry point for embedding arbitrary C++ code.
//!
//! This code provides the majority of the rust code generation logic.

/// This variant is used for raw text inclusion. It is used like the following:
///
/// ```
/// cpp! {{
///     #include <stdint.h>
///     #include <stdio.h>
/// }}
/// ```
///
/// This variant is used for closures. It is used like the following:
///
/// ```
/// let y: i32 = 10;
/// let mut z: i32 = 20;
/// let x: i32 = cpp! {[y as "int32_t", mut z as "int32_t"] -> i32 as "int32_t" {
///     z++;
///     return y + z;
/// }};
/// ```
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
