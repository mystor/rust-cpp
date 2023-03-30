#!/bin/bash

cargo publish --manifest-path cpp_common/Cargo.toml
cargo publish --manifest-path cpp_macros/Cargo.toml
cargo publish --manifest-path cpp_build/Cargo.toml
cargo publish --manifest-path cpp/Cargo.toml

