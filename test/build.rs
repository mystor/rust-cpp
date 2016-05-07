extern crate cpp;

use std::env;
use std::path::Path;

fn main() {
    cpp::build(
        Path::new("src/lib.rs.in"),
        &Path::new(&env::var("OUT_DIR").unwrap()).join("lib.rs"),
        "cpp_test",
        |_| ());
}
