extern crate cpp;

use std::path::Path;

fn main() {
    cpp::build(
        Path::new("src/lib.rs"),
        "cpp_test",
        |_| ());
}
