# rust-cpp - Embed C++ code directly in Rust

[![Build Status](https://travis-ci.org/mystor/rust-cpp.svg?branch=master)](https://travis-ci.org/mystor/rust-cpp)
[![Build status](https://ci.appveyor.com/api/projects/status/uu76vmcrwnjqra0u/branch/master?svg=true)](https://ci.appveyor.com/project/mystor/rust-cpp/branch/master)
[![Documentation](https://docs.rs/cpp/badge.svg)](https://docs.rs/cpp/)

`rust-cpp` is a build tool & macro which enables you to write C++ code inline in
your rust code.

This crate's interface is still very unstable, however, thanks to custom derive,
it now has one of the first potentially stable interfaces. The current macro
interface should either remain fairly stable, meaning that programs built with
it will likely build with minimal, or no, changes when updating to new versions.

## Usage

For usage information and in-depth documentation, see
the [`cpp` crate module level documentation](https://docs.rs/cpp).

## Warning about Macros

The build phase cannot identify and parse the information found in `cpp!` blocks
which are generated with rust's macro system. These blocks will attempt to
generate rust code, but will not generate the corresponding C++ code. The
procedural macro tries to avoid allowing the build to succeed if the `cpp!`
block is generated, but this is not guaranteed. Do not create `cpp!` blocks with
macros to avoid this.

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

#### [Macros 1.1 and syn (a.k.a. `v0.2` or `master`)](https://github.com/mystor/rust-cpp/)

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
