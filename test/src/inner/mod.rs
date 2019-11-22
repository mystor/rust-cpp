#![cfg_attr(not(test), allow(dead_code, unused_imports))]

use cpp::cpp;

pub mod innerinner;

#[path = "explicit_path.rs"]
pub mod innerpath;

pub fn inner() -> i32 {
    unsafe {
        let x: i32 = 10;
        cpp! {[x as "int"] -> i32 as "int" {
            return x;
        }}
    }
}
