# rust-cpp [stable branch]

[![Build Status](https://travis-ci.org/mystor/rust-cpp.svg?branch=stable)](https://travis-ci.org/mystor/rust-cpp)

This is the `stable` branch of rust-cpp. It is a re-write, and re-design of
rust-cpp built on syntex_syntax, allowing it to run on stable rust.

rust-cpp is a build tool & macro which enables you to write C++ code inline in
your rust code.

## Setup

> NOTE: As the stable branch of rust-cpp is not on crates.io, you will have to
> download it and use the path manually. This will likely be changed in the
> future.

Add `cpp` as a dependency to your project. It will need to be added both as a
build dependency, and as a normal dependency, with different flags. You'll also
need a `build.rs` set up for your project.

```toml
[package]
# ...
build = "build.rs"

[build-dependencies]
# ...
cpp = { version = "0.1.0", features = ["build"] }

[dependencies]
# ...
cpp = { version = "0.1.0", features = ["macro"] }
```

You'll also then need to call the `cpp` build plugin from your `build.rs`. It
should look something like this:

```rust
extern crate cpp;

fn main() {
    cpp::build("src/lib.rs", "crate_name", |cfg| {
        // cfg is a gcc::Config object. You can use it to add additional
        // configuration options to the invocation of the C++ compiler.
    });
}
```

## Usage

In your crate, include the cpp crate macros:

```rust
#[macro_use]
extern crate cpp;
```

Then, use the `cpp!` macro to define code and other logic which you want shared
between rust and C++. The `cpp!` macro supports the following forms:

```rust
cpp! {
    // Include a C++ library into the C++ shim. Only the `#include` directive 
    // is supported in this context.
    #include <cstdlib>
    #include "foo.h"
    
    // Write some logic directly into the shim. Either a curly-braced block or
    // string literal are supported
    raw {
        #define X 10
        struct Foo {
            uint32_t x;
        };
    }
    
    raw r#"
        #define Y 20
    "#
    
    // Define a function which can be called from rust, but is implemented in
    // C++. Its name is used as the C function name, and cannot collide with
    // other C functions. The body may be defined as a curly-braced block or 
    // string literal.
    // These functions are unsafe, and can only be called from unsafe blocks.
    fn my_function(x: i32 as "int32_t", y: u64 as "uint32_t") -> f32 as "float" {
        return (float)(x + y);
    }
    fn my_raw_function(x: i32 as "int32_t") -> u32 as "uint32_t" r#"
        return x;
    "#
    
    // Define a struct which is shared between C++ and rust. In C++-land its
    // name will be in the global namespace. In rust it will be located 
    // wherever the cpp! block is located
    struct MyStruct {
        x: i32 as "int32_t",
        y: *const i8 as "const char*"
    }
    
    // Define an enum which is shared between C++ and rust. In C++-land it 
    // will be defined in the global namespace as an `enum class`. In rust, 
    // it will be located wherever the cpp! block is located.
    enum MyEnum {
        A,
        B,
        C,
        D
    }
}
```
