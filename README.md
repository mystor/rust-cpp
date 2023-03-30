# rust-cpp - Embed C++ code directly in Rust

[![Documentation](https://docs.rs/cpp/badge.svg)](https://docs.rs/cpp/)

## Overview

`rust-cpp` is a build tool & macro which enables you to write C++ code inline in
your rust code.

```rust
let name = std::ffi::CString::new("World").unwrap();
let name_ptr = name.as_ptr();
let r = unsafe {
    cpp!([name_ptr as "const char *"] -> u32 as "int32_t" {
        std::cout << "Hello, " << name_ptr << std::endl;
        return 42;
    })
};
assert_eq!(r, 42)
```

The crate also help to expose some C++ class to Rust by automatically
implementing trait such as Drop, Clone (if the C++ type can be copied), and others

```rust
cpp_class!{
    #[derive(PartialEq)]
    unsafe struct MyClass as "std::unique_ptr<MyClass>"
}
```

## Usage

For usage information and in-depth documentation, see
the [`cpp` crate module level documentation](https://docs.rs/cpp).


## Differences with the [`cxx`](https://cxx.rs) crate

This crate allows to write C++ code "inline" within your Rust functions, while with the [`cxx`](https://cxx.rs) crate, you have
to write a bit of boiler plate to have calls to functions declared in a different `.cpp` file.

Having C++ code inline might be helpful when trying to call to a C++ library and that one may wish to make plenty of call to small snippets.
It can otherwise be fastidious to write and maintain the boiler plate for many small functions in different places. 

These crate can also be used in together. The `cxx` crate offer some useful types such as `CxxString` that can also be used with this crate.

The `cxx` bridge does more type checking which can avoid some classes of errors. While this crate can only check for equal size and alignment.
