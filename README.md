# rust-cpp - Embed C++ code directly in Rust

[![Build Status](https://travis-ci.org/mystor/rust-cpp.svg?branch=master)](https://travis-ci.org/mystor/rust-cpp)
[![Build status](https://ci.appveyor.com/api/projects/status/uu76vmcrwnjqra0u/branch/master?svg=true)](https://ci.appveyor.com/project/mystor/rust-cpp/branch/master)
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


## Diference with the [`cxx`](https://cxx.rs) crate

This crate allow to write C++ code "inline" while with the [`cxx`](https://cxx.rs) crate, you have
to write a bit of boiler plate to have calls to functions declared in a different .cpp file.
Having C++ code inline with the rust code might be helpful when trying to call to a C++ library
and that there are many roundtrip with small code snippet within a function.
It can be fastidious to write and maintain the boiler plate for many small functions in different
places, so this crate helps reducing boiler plate.

That said, these crate could be used in together. The cxx crate also offer some types such as `CxxString` and co. that can also be used wth this crate. The cxx bridge also does more type
checking which can avoid some errors.

## History

`rust-cpp` has had multiple different implementations. The code for these old
implementations is still present in the tree today.

#### [`rustc_plugin`](https://github.com/mystor/rust-cpp/tree/legacy_rustc_plugin)

`rust-cpp` started life as a unstable compiler plugin. This code no longer
builds on modern nightly rusts, but it had some features which are still
unavailable on more recent versions of `rust-cpp`, as it was able to take
advantage of the rust compiler's type inference by abusing a lint pass to
collect type information.

Development on the original version ceased for 2 major reasons:

1) The rustc internal libraries changed very often, meaning that constant
   maintenance work was required to keep it working on the latest nightly
   versions.

2) The plugin had no support for stable rust, which is undesirable because the
   majority of crates are not built with the latest nightly compiler, and
   requiring unstable features is a deal breaker for them.

These limitations led to the development of the next phase of `rust-cpp`'s
lifetime.

#### [stable (a.k.a `v0.1`)](https://github.com/mystor/rust-cpp/tree/legacy_v0.1)

The next phase in `rust-cpp`'s lifetime was when it was rewritten as a
`syntex` plugin. `syntex` is an extraction of the rust compiler's
internal `syntax` library, and has support for performing procedural macro
expansions by rewriting rust source files.

Performing a full rewrite of the source tree was very unfortunate, as it would
mean that all compiler errors in crates which use the plugin would be reported
in a generated file instead of at the original source location. Instead, this
version of `rust-cpp` used a stable `macro_rules!` macro to perform the rust
code generation, and a build step based on `syntex` to perform the c++ code
generation and compilation.

Unfortunately this architecture meant that one of the neatest features,
closures, was not available in this version of `rust-cpp`. Implementing
that feature required some form of procedural code generation on the rust
side, which was not possible in rust at that time without performing full text
rewrites of the source code.

#### Macros 1.1 and syn (a.k.a. `v0.2`)

This is the current implementation of `rust-cpp`. In `rustc 1.15`, the first
form of procedural macros, custom derive, was stabilized. Alongside this,
@dtolnay implemented [`syn`](https://github.com/dtolnay/syn), which was a small,
fast to compile, crate for parsing rust code. `rust-cpp` uses a fork of `syn`
for its rust code parsing.

`rust-cpp` now internally uses a custom derive to implement the procedural
components of the rust code generation, which means that closures are available
again! It also builds much more quickly than the previous version as it no
longer depends on `syntex` which could take a long time to build.

The fork of `syn` ([`cpp_syn`](https://github.com/mystor/cpp_syn)) which
`rust-cpp` uses differs from `syn` in that it keeps track of source location
information for each AST node. This feature has not been landed into `syn` yet
as it is a breaking change, and none of `syn`'s other consumers would make use
of it yet.

#### `v0.5`

The syn version was upgraded to `syn 1.0`
Since `syn` did not allow to access the actual source location or text, the `cpp_build`
uses its own rust lexer, forked from the `proc_macro2` crate.
