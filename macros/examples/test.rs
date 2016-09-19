#![feature(rustc_macro, custom_derive)]

#[macro_use]
extern crate cpp_macros;

macro_rules! cppclosure {
    (($($id:ident as $cty:tt),*) -> $rty:ty as $crty:tt $body:tt) => {
        {
            #[derive(rust_cpp_internal)]
            struct Dummy(__!(
                ($($id as $cty),*) -> $rty as $crty $body
            ));
            Dummy::call($(& $id as *const _ as *const u8),*)
        }
    };
}

#[allow(unused)]
fn main() {
    let a = 10u32;
    let b = 20u32;
    let c = 30u32;
    unsafe {
        let _ = cppclosure!((a as "a", b as "b", c as "c") -> i32 as "i32" {
            frob
        });

        let _ = cppclosure!((a as "a", b as "b", c as "c") -> i32 as "i32" r#"
quxx
"#);
    }
}
