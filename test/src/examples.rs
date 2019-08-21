cpp! {{
    #include <iostream>
}}

#[test]
fn main() {
    let name = std::ffi::CString::new("World").unwrap();
    let name_ptr = name.as_ptr();
    let r = unsafe {
        cpp!([name_ptr as "const char *"] -> u32 as "int32_t" {
            std::cout << "Hello, " << name_ptr << std::endl;
            return 42;
        })
    };
    assert_eq!(r, 42)
}
