extern crate cpp;

fn main() {
    cpp::build("src/lib.rs", "cpp_test", |cfg| {
        cfg.flag("-std=c++0x");
    });
}
