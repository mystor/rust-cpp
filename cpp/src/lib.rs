//! This crate `cpp` provides macros that allow embedding arbitrary C++ code.
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
//! cpp = "0.4"
//!
//! [build-dependencies]
//! cpp_build = "0.4"
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
    (@find_rust_macro [$($a:tt)*] rust!($($rust_body:tt)*) $($rest:tt)*) => {
        __cpp_internal!{ @expand_rust_macro [$($a)*] $($rust_body)* }
        __cpp_internal!{ @find_rust_macro [$($a)*] $($rest)* }
    };
    (@find_rust_macro [$($a:tt)*] ( $($in:tt)* ) $($rest:tt)* ) =>
        { __cpp_internal!{ @find_rust_macro [$($a)*] $($in)* $($rest)* }  };
    (@find_rust_macro [$($a:tt)*] [ $($in:tt)* ] $($rest:tt)* ) =>
        { __cpp_internal!{ @find_rust_macro [$($a)*] $($in)* $($rest)* }  };
    (@find_rust_macro [$($a:tt)*] { $($in:tt)* } $($rest:tt)* ) =>
        { __cpp_internal!{ @find_rust_macro [$($a)*] $($in)* $($rest)* }  };
    (@find_rust_macro [$($a:tt)*] $t:tt $($rest:tt)*) =>
        { __cpp_internal!{ @find_rust_macro [$($a)*] $($rest)* } };
    (@find_rust_macro [$($a:tt)*]) => {};

    (@expand_rust_macro [$($a:tt)*] $i:ident [$($an:ident : $at:ty as $ac:tt),*] {$($body:tt)*}) => {
        #[doc(hidden)]
        $($a)* extern "C" fn $i($($an : *const $at),*) {
            $(let $an : $at = unsafe { $an.read() };)*
            (|| { $($body)* })();
            $(::std::mem::forget($an);)*

        }
    };
    (@expand_rust_macro [$($a:tt)*] $i:ident [$($an:ident : $at:ty as $ac:tt),*] -> $rt:ty as $rc:tt {$($body:tt)*}) => {
        #[doc(hidden)]
        $($a)* extern "C" fn $i($($an : *const $at, )* rt : *mut $rt) -> *mut $rt {
            $(let $an : $at = unsafe { $an.read() };)*
            {
                #[allow(unused_mut)]
                let mut lambda = || {$($body)*};
                unsafe { ::std::ptr::write(rt, lambda()) };
            }
            $(::std::mem::forget($an);)*
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
/// The second variant is used to embed C++ code within rust code. A list of
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
/// You can also put the unsafe keyword as the first keyword of the cpp! macro, which
/// has the same effect as putting the whole macro in an unsafe block:
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
/// The cpp! macro can contain, in the C++ code, a rust! sub-macro, which allows
/// to include rust code in C++ code. This is useful to
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
/// The syntax for the rust! macro is:
/// ```ignore
/// rust!($uniq_ident:ident [$($arg_name:ident : $arg_rust_type:ty as $arg_c_type:tt),*]
///      $(-> $ret_rust_type:ty as $rust_c_type:tt)* {$($body:tt)*})
/// ```
/// uniq_ident is an unique identifier which will be used to name the extern function
#[macro_export]
macro_rules! cpp {
    // raw text inclusion
    ({$($body:tt)*}) => { __cpp_internal!{ @find_rust_macro [#[no_mangle] pub] $($body)*} };

    // inline closure
    ([$($captures:tt)*] $($rest:tt)*) => {
        {
            __cpp_internal!{ @find_rust_macro [] $($rest)*}
            #[allow(unused)]
            #[derive(__cpp_internal_closure)]
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
/// alignment as the the C++ class "MyClass". It will also implement the `Drop` trait
/// calling the destructor, the `Clone` trait calling the copy constructor, if the
/// class is copyable (or `Copy` if it is trivially copyable), and `Default` if the class
/// is default constructible
///
/// The presence of the unsafe keyword in the macro is required as this macro is
/// calling potentially unsafe. The C++ constructors and destructor might be called
/// when the class is created/cloned/destructed. You must ensure that the C++ class
/// can be safely moved in memory.
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
///   be called twice. Note that it will never return None.
/// * The trait `Ord` can also be specified when the semantics of the `operator<` corresponds
///   to a total order
///
#[macro_export]
macro_rules! cpp_class {
    ($(#[$($attrs:tt)*])* unsafe struct $name:ident as $type:expr) => {
        __cpp_class_internal!{@parse [ $(#[$($attrs)*])* ] [] [unsafe struct $name as $type] }
    };
    ($(#[$($attrs:tt)*])* pub unsafe struct $name:ident as $type:expr) => {
        __cpp_class_internal!{@parse [ $(#[$($attrs)*])* ] [pub] [unsafe struct $name as $type] }
    };
    ($(#[$($attrs:tt)*])* pub($($pub:tt)*) unsafe struct $name:ident as $type:expr) => {
        __cpp_class_internal!{@parse [ $(#[$($attrs)*])* ] [pub($($pub)*)] [unsafe struct $name as $type] }
    };
}

/// Implementation details for cpp_class!
#[doc(hidden)]
#[macro_export]
macro_rules! __cpp_class_internal {
    (@parse [$($attrs:tt)*] [$($vis:tt)*] [unsafe struct $name:ident as $type:expr]) => {
        __cpp_class_internal!{@parse_attributes [ $($attrs)* ]  [
            #[derive(__cpp_internal_class)]
            #[repr(C)]
            $($vis)* struct $name {
                _opaque : [<$name as $crate::CppTrait>::BaseType ; <$name as $crate::CppTrait>::ARRAY_SIZE
                    + (stringify!($($attrs)* $($vis)* unsafe struct $name as $type), 0).1]
            }
        ]}
    };

    (@parse_attributes [] [$($result:tt)*]) => ( $($result)* );
    (@parse_attributes [#[derive($($der:ident),*)] $($tail:tt)* ] [$($result:tt)*] )
        => (__cpp_class_internal!{@parse_derive [$($der),*] @parse_attributes [$($tail)*] [ $($result)* ] } );
    (@parse_attributes [ #[$m:meta] $($tail:tt)* ]  [$($result:tt)*])
        => (__cpp_class_internal!{@parse_attributes [$($tail)*]  [ #[$m] $($result)* ] } );

    (@parse_derive [] @parse_attributes $($result:tt)*) => (__cpp_class_internal!{@parse_attributes $($result)*} );
    (@parse_derive [PartialEq $(,$tail:ident)*] $($result:tt)*)
        => ( __cpp_class_internal!{@parse_derive [$($tail),*] $($result)*} );
    (@parse_derive [PartialOrd $(,$tail:ident)*] $($result:tt)*)
        => ( __cpp_class_internal!{@parse_derive [$($tail),*] $($result)*} );
    (@parse_derive [Ord $(,$tail:ident)*] $($result:tt)*)
        => ( __cpp_class_internal!{@parse_derive [$($tail),*] $($result)*} );
    (@parse_derive [Default $(,$tail:ident)*] $($result:tt)*)
        => ( __cpp_class_internal!{@parse_derive [$($tail),*] $($result)*} );
    (@parse_derive [Clone $(,$tail:ident)*] $($result:tt)*)
        => ( __cpp_class_internal!{@parse_derive [$($tail),*] $($result)*} );
    (@parse_derive [Copy $(,$tail:ident)*] $($result:tt)*)
        => ( __cpp_class_internal!{@parse_derive [$($tail),*] $($result)*} );
    (@parse_derive [$i:ident $(,$tail:ident)*] @parse_attributes [$($attr:tt)*] [$($result:tt)*] )
        => ( __cpp_class_internal!{@parse_derive [$($tail),*] @parse_attributes [$($attr)*] [ #[derive($i)] $($result)* ] } );
}
