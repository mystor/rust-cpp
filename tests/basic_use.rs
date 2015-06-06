#![feature(plugin)]

#![plugin(cpp)]

use std::ffi::CString;

cpp_include!(<iostream>);
cpp_include!(<cstdint>);


#[test]
fn basic_math() {
    let a: i32 = 10;
    let b: i32 = 20;

    let cpp_result = unsafe {
        cpp!((a, b) -> i32 {
            int32_t* av = (int32_t*) a;
            int32_t* bv = (int32_t*) b;

            int32_t c = *av * 10;
            int32_t d = *bv * 20;

            std::cout << "Hello from C++!\n";

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
    let local_cstring = cs.as_ptr();

    let cpp_result = unsafe {
        cpp!((local_cstring) -> i32 {
            (*(char**)local_cstring)[3] = 'a';

            return 5;
        })
    };

    assert_eq!(cpp_result, 5);
    assert_eq!(cs.as_bytes(), b"Helao, World!");
}
