pub fn nomod_inner() -> i32 {
    unsafe {
        let nomod_inner: i32 = 10;
        cpp! {[nomod_inner as "int"] -> i32 as "int" {
            return nomod_inner;
        }}
    }
}
