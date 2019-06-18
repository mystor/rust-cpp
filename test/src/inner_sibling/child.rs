pub fn inner_sibling_child() -> i32 {
    unsafe {
        let x_c: i32 = 20;
        cpp! {[x_c as "int"] -> i32 as "int" {
            return x_c;
        }}
    }
}
