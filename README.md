# rust-cpp [stable branch]

[![Build Status](https://travis-ci.org/mystor/rust-cpp.svg?branch=stable)](https://travis-ci.org/mystor/rust-cpp)

This is the `stable` branch of rust-cpp. It is a re-write of rust-cpp as a syntex plugin, which allows it to run on stable rust.

rust-cpp is a syntex plugin which enables you to write C++ code inline in your rust code.

## Usage

> NOTE: This documentation is incomplete. Please come back later when I have
> documented the code well enough that you don't have to just read the source :S

> NOTE: As the stable branch of rust-cpp is not on crates.io, you will have to
> download it and use the path manually. This will likely be changed in the
> future.

rust-cpp runs as a build plugin, so first it will need to be added to your
project as a `build-dependency`:

```toml
[build-dependencies]
cpp = { version = "*" }
```

You'll also need to be sure to add a build script to your project, if you haven't already:

```toml
[package]
# ...
build = "build.rs"
```

The entry point for your module will need to be re-named, such that the real
entry point can be automatically generated. For example, instead of `main.rs`,
you would have `main.rs.in`. The old `main.rs` file should then be replaced with
the following:

```rust
include!(concat!(env!("OUT_DIR"), "/main.rs"))
```

This tells the rust compiler when it tries to compile your module to read the
generated output file from `rust-cpp` and treat it as though it was written as
the project's `main.rs` file.

Now, your `build.rs` should look like this:

```rust
extern crate cpp;

use std::env;
use std::path::Path;

fn main() {
    cpp::build(
        Path::new("src/main.rs.in"),
        &Path::new(&env::var("OUT_DIR").unwrap()).join("main.rs"),
        "cpp_test",
        |_| ());
}
```
