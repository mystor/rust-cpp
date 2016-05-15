extern crate cpp;

fn main() {
    cpp::build("src/lib.rs", "cpp_test", |_| ());
}
