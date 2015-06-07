#![feature(plugin)]

#![plugin(cpp)]

use std::ffi::CString;

cpp_include!(<cmath>);

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
fn foreign_type() {
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
    let mut v: Vec<u8> = Vec::new();
    let mut vs = &v[..];
    let s = &b"hey_there"[..];

    unsafe {
        cpp!((mut vs, s) {
            // Create a new slice object from whole cloth,
            // and copy it into the old vs object!
            Slice<uint8_t> new_vs = {
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
            Slice<uint8_t> result = {
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
