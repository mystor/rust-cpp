#![cfg_attr(feature = "closures", feature(proc_macro, custom_derive))]
#![cfg_attr(not(test), allow(dead_code))]
#![allow(improper_ctypes)]

#[macro_use]
extern crate cpp;

#[cfg(feature="closures")]
#[macro_use]
extern crate cpp_macros;

#[cfg(test)]
use std::ffi::CString;

cpp! {
    // Bring in rust-types!
    #include "rust_types.h"
}

cpp! {
    fn basic_math_impl(a: i32 as "rs::i32", b: i32 as "rs::i32") -> i32 as "rs::i32" {
        int32_t c = a * 10;
        int32_t d = b * 20;

        return c + d;
    }
}

#[test]
fn basic_math() {
    let a: i32 = 10;
    let b: i32 = 20;

    let cpp_result = unsafe {
        basic_math_impl(a, b)
    };

    assert_eq!(cpp_result, 500);
    assert_eq!(a, 10);
    assert_eq!(b, 20);
}

cpp! {
    fn strings_impl(local_cstring: *mut u8 as "char *") {
        local_cstring[3] = 'a';
    }
}

#[test]
fn strings() {
    let cs = CString::new(&b"Hello, World!"[..]).unwrap();

    unsafe {
        strings_impl(cs.as_ptr() as *mut _);
    }

    assert_eq!(cs.as_bytes(), b"Helao, World!");
}

cpp! {
    fn foreign_type_impl(a: *const u8 as "void *") -> usize as "uintptr_t" {
        return reinterpret_cast<uintptr_t>(a);
    }
}

#[test]
fn foreign_type() {
    #[allow(dead_code)]
    struct WeirdRustType {
        a: Vec<u8>,
        b: String,
    }

    let a = WeirdRustType {
        a: Vec::new(),
        b: String::new(),
    };

    unsafe {
        let addr_a = &a as *const _ as usize;
        let c_addr_a = foreign_type_impl(&a as *const _ as *mut _);

        assert_eq!(addr_a, c_addr_a);
    }
}

#[cfg(test)]
mod inner;

#[test]
fn inner_module() {
    let x = inner::inner();
    assert_eq!(x, 10);
}

cpp! {
    #include <cmath>
    fn c_std_lib_impl(num1: f32 as "float",
                      num2: f32 as "float")
                      -> f32 as "float" {
        return sqrt(num1) + cbrt(num2);
    }
}

#[test]
fn c_std_lib() {
    let num1: f32 = 10.4;
    let num2: f32 = 12.5;

    unsafe {
        let res = c_std_lib_impl(num1, num2);

        let res_rs = num1.sqrt() + num2.cbrt();

        assert!((res - res_rs).abs() < 0.001);
    }
}

enum CppVec {}

cpp! {
    #include <vector>

    fn make_vector_impl() -> *const CppVec as "std::vector<uint32_t>*" {
        auto vec = new std::vector<uint32_t>;
        vec->push_back(10);
        return vec;
    }

    fn use_vector_impl(cpp_vector: *const CppVec as "std::vector<uint32_t>*")
                       -> bool as "bool"
    {
        uint32_t first_element = (*cpp_vector)[0];
        delete cpp_vector;
        return first_element == 10;
    }
}

#[test]
fn c_vector() {
    unsafe {
        let cpp_vector = make_vector_impl();
        let result = use_vector_impl(cpp_vector);

        assert!(result);
    }
}

cpp! {
    #[derive(PartialEq, Eq, Debug)]
    enum Foo {
        Apple,
        Peach,
        Cucumber,
    }

    fn basic_enum_impl_1(foo: Foo as "Foo", bar: Foo as "Foo", quxx: Foo as "Foo")
                         -> bool as "bool"
    {
        return foo == Apple && bar == Peach && quxx == Cucumber;
    }

    fn basic_enum_impl_2() -> Foo as "Foo" {
        return Cucumber;
    }
}

#[test]
fn basic_enum() {
    let foo = Foo::Apple;
    let bar = Foo::Peach;
    let quxx = Foo::Cucumber;

    unsafe {
        assert!(basic_enum_impl_1(foo, bar, quxx));

        let returned_enum = basic_enum_impl_2();
        assert_eq!(returned_enum, Foo::Cucumber);
    }
}

cpp! {
    raw {
        #define SOME_CONSTANT 10
    }

    fn return_some_constant() -> u32 as "uint32_t" {
        return SOME_CONSTANT;
    }
}

#[test]
fn header() {
    unsafe {
        let c = return_some_constant();
        assert_eq!(c, 10);
    }
}

cpp! {
    #[derive(Copy, Clone)]
    struct S {
        a: i32 as "int32_t",
    }
}

#[test]
fn derive_copy() {
    let x = S { a: 10 };
    let mut y = x;
    assert_eq!(x.a, 10);
    assert_eq!(y.a, 10);
    y.a = 20;
    assert_eq!(x.a, 10);
    assert_eq!(y.a, 20);
}

cpp! {
    raw "#define SOME_VALUE 10"

    fn string_body_impl() -> i32 as "int32_t" r#"
        return SOME_VALUE;
    "#
}

#[test]
fn string_body() {
    unsafe {
        assert_eq!(string_body_impl(), 10);
    }
}

cpp! {
    #[derive(Eq, PartialEq, Debug)]
    #[allow(dead_code)]
    enum class EnumClass {
        A,
        B,
    }

    #[derive(Eq, PartialEq, Debug)]
    #[allow(dead_code)]
    enum prefix EnumPrefix {
        A,
        B,
    }

    fn test_enum_class() -> EnumClass as "EnumClass" {
        return EnumClass::B;
    }

    fn test_enum_prefix() -> EnumPrefix as "EnumPrefix" {
        return EnumPrefix_B;
    }
}

#[test]
fn enum_class_prefix() {
    unsafe {
        assert_eq!(test_enum_class(), EnumClass::B);
        assert_eq!(test_enum_prefix(), EnumPrefix::B);
    }
}

mod test_pub {
    cpp! {
        pub struct PubStruct {
            pub a: i32 as "int32_t",
        }

        #[allow(dead_code)]
        pub enum PubEnum {A, B,}
        #[allow(dead_code)]
        pub enum class PubEnumClass {A, B,}
        #[allow(dead_code)]
        pub enum prefix PubEnumPrefix {A, B,}

        pub fn test_pub_things(a: PubStruct as "PubStruct",
                               b: PubEnum as "PubEnum",
                               c: PubEnumClass as "PubEnumClass",
                               d: PubEnumPrefix as "PubEnumPrefix") -> bool as "bool" {
            return a.a == 10 && b == A && c == PubEnumClass::A && d == PubEnumPrefix_A;
        }
    }
}

#[test]
fn pub_struct() {
    let a = test_pub::PubStruct{ a: 10 };
    let b = test_pub::PubEnum::A;
    let c = test_pub::PubEnumClass::A;
    let d = test_pub::PubEnumPrefix::A;
    unsafe {
        assert!(test_pub::test_pub_things(a, b, c, d));
    }
}

#[cfg(feature="closures")]
#[test]
fn immutable_closure() {
    let x: i32 = 10;
    let y = unsafe {
        cpp!((x as "int32_t") -> i32 as "int32_t" {
            return x + 20;
        })
    };
    assert_eq!(y, 30);
}
