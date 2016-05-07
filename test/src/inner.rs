pub fn inner() -> i32 {
    unsafe {
        cpp!(() -> i32 "int32_t" {
            return 10;
        })
    }
}
