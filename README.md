# rust-cpp - Embed C++ code directly in Rust

[![Build Status](https://travis-ci.org/mystor/rust-cpp.svg?branch=master)](https://travis-ci.org/mystor/rust-cpp)
[![Build status](https://ci.appveyor.com/api/projects/status/uu76vmcrwnjqra0u/branch/master?svg=true)](https://ci.appveyor.com/project/mystor/rust-cpp/branch/master)
[![Documentation](https://docs.rs/cpp/badge.svg)](https://docs.rs/cpp/)

> rust-cpp is a build tool & macro which enables you to write C++ code inline in
> your rust code.

## Usage

For usage information and in-depth documentation, see
the [`cpp` crate module level documentation](https://docs.rs/cpp).

## Warning about Macros

The build phase cannot identify and parse the information found in `cpp!` blocks
which are generated with rust's macro system. These blocks will attempt to
generate rust code, but will not generate the corresponding C++ code. The
procedural macro tries to avoid allowing the build to succeed if the `cpp!`
block is generated, but this is not guaranteed. Do not create `cpp! {}` blocks
with macros to avoid this.
