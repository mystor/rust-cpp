use cpp::cpp;

pub mod child;
pub mod child2;

pub fn inner_sibling() -> i32 {
    unsafe {
        let x_s: i32 = 10;
        cpp! {[x_s as "int"] -> i32 as "int" {
            return x_s;
        }}
    }
}
