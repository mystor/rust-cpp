#![feature(plugin)]

#![plugin(cpp)]

use std::ffi::CString;
use std::ptr;

cpp_include!(<cmath>);
cpp_include!(<vector>);

#[test]
fn basic_math() {
    let a: i32 = 10;
    let b: i32 = 20;

    let cpp_result = unsafe {
        cpp!((a, b) -> i32 {
            int32_t c = a * 10;
            int32_t d = b * 20;

            return c + d;
        })
    };

    assert_eq!(cpp_result, 500);
    assert_eq!(a, 10);
    assert_eq!(b, 20);
}

#[test]
fn strings() {
    let csvec: Vec<_> = b"Hello, World!".iter().cloned().collect();
    let cs = CString::new(csvec).unwrap();
    let mut local_cstring = cs.as_ptr();

    unsafe {
        cpp!((mut local_cstring) {
            local_cstring[3] = 'a';
        });
    }

    assert_eq!(cs.as_bytes(), b"Helao, World!");
}

#[test]
#[allow(improper_ctypes)]
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
        let addr_a = &a as *const WeirdRustType as usize;
        let c_addr_a = cpp!((a) -> usize {
            return (uintptr_t) &a;
        });

        assert_eq!(addr_a, c_addr_a);
    }
}

#[test]
fn slice_arg() {
    let v: Vec<u8> = Vec::new();
    let mut vs = &v[..];
    let s = &b"hey_there"[..];

    unsafe {
        cpp!((mut vs, s) {
            // Create a new slice object from whole cloth,
            // and copy it into the old vs object!
            rs::Slice<uint8_t> new_vs = {
                .data = s.data,
                .len = 4,
            };
            vs = new_vs;
        });
    }

    // vs now holds a reference to the contents of s,
    // but only the first 4 of them.
    assert_eq!(vs, b"hey_");
}

#[test]
fn slice_return() {
    let s = &b"hey_there"[..];

    let out = unsafe {
        cpp!((s) -> *const [u8] {
            // Create a new slice object from whole cloth,
            // and copy it into the old vs object!
            rs::Slice<uint8_t> result = {
                .data = s.data,
                .len = 4,
            };

            return result;
        })
    };

    // vs now holds a reference to the contents of s,
    // but only the first 4 of them.
    assert_eq!(unsafe { &*out }, b"hey_");
}

#[test]
fn c_std_lib() {
    let num1 = 10.4f32;
    let num2 = 12.5f32;
    unsafe {
        let res = cpp!((num1, num2) -> f32 {
            return sqrt(num1) + cbrt(num2);
        });

        let rs_res = num1.sqrt() + num2.cbrt();

        // C and Rust have different float stuff
        assert!((res - rs_res).abs() < 0.001);
    }
}

#[test]
fn c_vector() {
    unsafe {
        #[cpp_type = "std::vector<uint32_t>"]
        enum CppVec {}

        let cpp_vector = cpp!(() -> *const CppVec {
            std::vector<uint32_t> *vec = new std::vector<uint32_t>;
            vec->push_back(10);

            return vec;
        });

        // Destroy the cpp_vector!
        let result = cpp!((cpp_vector) -> bool {
            uint32_t first_element = (*cpp_vector)[0];
            delete cpp_vector;
            cpp_vector = nullptr;
            return first_element == 10;
        });

        assert!(result);
        assert_eq!(cpp_vector, ptr::null());
    }
}

#[test]
fn basic_enum() {
    #[allow(dead_code)]
    #[derive(PartialEq, Eq, Debug)]
    #[repr(C)]
    enum Foo {
        Apple,
        Pear,
        Peach,
        Cucumber,
    };

    let foo = Foo::Apple;
    let bar = Foo::Peach;
    let quxx = Foo::Cucumber;

    unsafe {
        let success = cpp!((foo, bar, quxx) -> bool {
            using namespace rs::basic_enum;

            return foo == Foo::Apple && bar == Foo::Peach && quxx == Foo::Cucumber;
        });

        assert!(success);

        let returned_enum = cpp!(() -> Foo {
            using namespace rs::basic_enum;

            return Foo::Cucumber;
        });

        assert_eq!(returned_enum, Foo::Cucumber);
    }
}

#[test]
fn repr_c() {
    #[derive(PartialEq, Eq, Debug)]
    #[repr(C)]
    struct SomeStruct {
        a: i32,
        b: i32,
    }

    let mut my_struct = SomeStruct {
        a: 5,
        b: 10
    };

    unsafe {
        let retval = cpp!((mut my_struct) -> i32 {
            int32_t result = my_struct.a + my_struct.b;

            my_struct.a *= 6;
            my_struct.b *= 6;

            return result;
        });

        assert_eq!(retval, 15);
        assert_eq!(my_struct, SomeStruct { a: 30, b: 60 });
    }
}

#[test]
fn repr_c_cycle() {
    #[repr(C)]
    struct A {
        b: *mut B,
    }

    #[repr(C)]
    struct B {
        a: *mut A,
    }

    let a = A { b: 0 as *mut B };

    unsafe {
        let retval = cpp!((a) -> *mut B {
            return a.b;
        });

        assert_eq!(retval, a.b);
    }
}

cpp_header! {
    #define SOME_CONSTANT 10
}

#[test]
fn header() {
    unsafe {
        let c = cpp!(() -> i32 { return SOME_CONSTANT; });
        assert_eq!(c, 10);
    }
}
