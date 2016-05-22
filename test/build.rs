extern crate cpp;

fn main() {
    cpp::build("src/lib.rs", "cpp_test", |cfg| {
        // This flag is required in order to ensure that the test compiles due
        // to its use of enum class
        cfg.flag("-std=c++0x");
    });
}
